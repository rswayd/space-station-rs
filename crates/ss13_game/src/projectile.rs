//! Projectiles (server): tg `/obj/projectile` (`code/modules/projectiles/projectile.dm`) and
//! what it does to what it hits (`/mob/living/bullet_act`, `apply_projectile_effects`,
//! `living_defense.dm`).
//!
//! A fired projectile is an entity with its type's sprite, turned to its angle (tg `angle`: 0 is
//! north, clockwise) and moved every tick by `speed` tiles per decisecond
//! (`SSprojectiles.pixels_per_decisecond` is one tile), in eighth-tile steps; its sprite carries
//! the sub-tile offset, so clients see it fly. Entering a tile (tg `scan_moved_turf` ->
//! `select_target`) it hits, in order: what it was fired at, a living mob standing there (lying
//! ones only when the firer was in combat mode, within 10 tiles; dead ones never), then the
//! first dense thing it cannot pass (`PASSTABLE`/`PASSGLASS`/`PASSGRILLE` against tables,
//! glass and grilles), or a dense turf. After `range` tiles it is gone (`on_range`). It flies
//! on past the clicked point.
//!
//! A hit on a mob (`bullet_act`): the zone is the firer's selected zone, or a random one with
//! `accurate_range - accuracy_falloff × distance` percent (`ran_zone`); worn armor of the
//! projectile's `armor_flag` (`crate::armor`) reduces the damage (at most 90%) and the
//! stuns, knockdowns and stamina. Mechs take it through their armor (`crate::mecha`), objects
//! with integrity through theirs (`crate::obj_damage`).
//!
//! Cost: projectiles are few (a shot lives under 4 s); each tick each one moves up to ~2 tiles
//! in 1/8-tile steps, looking at the tile index only when it enters a new tile.
//!
//! Aim (tg `aim_projectile` with click modifiers): the angle points at the clicked pixel
//! ([`Shot::pixel`], tg `ICON_X`/`ICON_Y`), turned by the spread ([`Shot::deviation`]). Hard
//! surfaces (walls, windows) can bounce bullets (`handle_ricochet`), and hits on mobs roll
//! wounds with the projectile's `wound_bonus`/`sharpness` (`crate::medical::wounds`).
// shortcut: no reflection by held items (worn ablative armor reflects: `crate::clothing::ablative`), ricochet auto-aim, embedding, dismemberment, impact visuals, hitscan
// beams, homing or projectile sounds; add them with the guns that need them.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmObj, Opaque, ProjectileData, Turf};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, TilePos};
use ss13_world::{Density, SpatialIndex};

use crate::combat::CombatRng;
use crate::health::{COMBAT_MESSAGE_RANGE, DamageType, Health, HealthSystems, Stat};
use crate::medical::wounds::CANT_WOUND;
use crate::status_effects::Incap;
use crate::zone::BodyZone;

/// `PASSTABLE`, `PASSGLASS`, `PASSGRILLE`.
pub const PASSTABLE: u32 = 1 << 0;
pub const PASSGLASS: u32 = 1 << 1;
pub const PASSGRILLE: u32 = 1 << 2;
/// `MAX_RANGE_HIT_PRONE_TARGETS`, tiles.
pub const MAX_RANGE_HIT_PRONE_TARGETS: f32 = 10.0;
/// Movement step, tiles: small enough not to jump a tile corner on the way.
const STEP: f32 = 0.125;
/// Pixels per tile (`ICON_SIZE_X`).
const PX: f32 = ss13_shared::TILE_SIZE as f32;
/// How far from the clicked tile's center a client may aim, pixels: a sprite reaches at most
/// this far over its neighbours.
pub const MAX_AIM_PIXEL: i16 = 32;
/// `reflect_range_decrease`, tiles.
const REFLECT_RANGE_DECREASE: f32 = 5.0;

/// A projectile in flight.
#[derive(Component, Clone, Debug)]
pub struct Projectile {
    pub data: ProjectileData,
    pub firer: Entity,
    /// What it was fired at (tg `original`): hit on its tile whatever its state.
    pub original: Option<Entity>,
    /// The firer's selected zone (`def_zone`).
    pub zone: BodyZone,
    /// `hit_prone_targets`: the firer was in combat mode.
    pub hit_prone: bool,
    pub start: TilePos,
    /// Position in tiles (tile centers are whole numbers) and the unit step.
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    /// Tiles moved (`maximum_range - range`).
    pub moved: f32,
    /// Degrees clockwise from north (`angle`).
    pub angle: f32,
    /// The gun it came from (`fired_from`).
    pub fired_from: Option<Entity>,
    /// `ricochets`; after one the firer can be hit too (`ignore_source_check`).
    pub ricochets: u32,
}

/// tg `p_x`, `p_y`: the clicked point the projectile was aimed at, pixels from the target
/// tile's center ([`Shot::pixel`]).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AimPixel(pub [i16; 2]);

/// What a shot needs: tg `ready_proj` + `aim_projectile`.
pub struct Shot<'a> {
    pub data: &'a ProjectileData,
    pub firer: Entity,
    pub from: TilePos,
    pub target: Option<Entity>,
    pub target_pos: TilePos,
    pub zone: BodyZone,
    pub hit_prone: bool,
    /// The clicked point, pixels from `target_pos`'s center (clamped to
    /// [`MAX_AIM_PIXEL`]).
    pub pixel: [i16; 2],
    /// Spread, degrees (`deviation`).
    pub deviation: f32,
    pub fired_from: Option<Entity>,
}

/// Projectiles move in this set, after the guns fired this tick.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProjectileSystems;

pub struct ProjectileServerPlugin;

impl Plugin for ProjectileServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            move_projectiles
                .in_set(ProjectileSystems)
                .after(crate::click::ResolveClicks)
                .before(HealthSystems),
        );
    }
}

/// tg `get_angle(source, target)`: degrees clockwise from north.
pub fn angle_between(from: TilePos, to: TilePos) -> f32 {
    let (dx, dy) = ((to.x - from.x) as f32, (to.y - from.y) as f32);
    dx.atan2(dy).to_degrees().rem_euclid(360.0)
}

/// tg `calculate_projectile_angle_and_pixel_offsets`: the angle from `from`'s center to
/// `pixel` (clamped to [`MAX_AIM_PIXEL`]) off `to`'s center; 0 at the own center.
pub fn aim_angle(from: TilePos, to: TilePos, pixel: [i16; 2]) -> f32 {
    let [px, py] = pixel.map(|p| f32::from(p.clamp(-MAX_AIM_PIXEL, MAX_AIM_PIXEL)));
    let dx = (to.x - from.x) as f32 * PX + px;
    let dy = (to.y - from.y) as f32 * PX + py;
    if dx == 0.0 && dy == 0.0 {
        return 0.0;
    }
    dx.atan2(dy).to_degrees().rem_euclid(360.0)
}

/// tg `fire_casing` -> `throw_proj` -> `fire`: spawns the projectile and, point blank (the
/// target on an adjacent tile, tg `direct_target`), hits at once. `None` without a projectile.
pub fn fire(world: &mut World, shot: Shot) -> Option<Entity> {
    fire_with(world, shot, ())
}

/// [`fire`] with `extra` on the projectile before it flies or hits (a magic bolt's
/// [`crate::spells::MagicBolt`]).
pub fn fire_with(world: &mut World, shot: Shot, extra: impl Bundle) -> Option<Entity> {
    if shot.data.path.is_empty() {
        return None;
    }
    // `throw_proj`: a target on the firer's own tile (a gun in one's mouth) is hit at once.
    let direct = shot.target.filter(|&t| {
        (t != shot.firer || shot.target_pos == shot.from)
            && world.get::<Health>(t).is_some()
            && world
                .resource::<SpatialIndex>()
                .adjacent(shot.from, shot.target_pos, |e| e == t)
    });
    let angle =
        (aim_angle(shot.from, shot.target_pos, shot.pixel) + shot.deviation).rem_euclid(360.0);
    let (sin, cos) = angle.to_radians().sin_cos();
    let mut projectile = Projectile {
        data: shot.data.clone(),
        firer: shot.firer,
        original: shot.target,
        zone: shot.zone,
        hit_prone: shot.hit_prone,
        start: shot.from,
        x: shot.from.x as f32,
        y: shot.from.y as f32,
        dx: sin,
        dy: cos,
        moved: 0.0,
        angle,
        fired_from: shot.fired_from,
        ricochets: 0,
    };
    let entity = crate::construction::spawn_obj(world, &shot.data.path, shot.from)?;
    world
        .entity_mut(entity)
        .insert((extra, AimPixel(shot.pixel)));
    if let Some(mut look) = world.get_mut::<Appearance>(entity) {
        look.rotation = angle.round() as u16 % 360;
        look.mouse_opacity = ss13_shared::MouseOpacity::Transparent;
    }
    // `kinetic/prehit_pierce`: a modkit's ignored mob is passed through.
    let direct = direct.filter(|&t| !crate::mining::modkits::phases(world, &projectile, t));
    if let Some(target) = direct {
        let name = crate::obj_damage::strip(&crate::security::name_of(world, entity)).to_owned();
        crate::mining::rocks::kinetic_prehit(world, &mut projectile, target);
        hit(world, entity, &projectile, &name, target);
        crate::mining::rocks::kinetic_strike(world, &projectile, target);
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
        return None;
    }
    world.entity_mut(entity).insert(projectile);
    Some(entity)
}

fn move_projectiles(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let mut query = world.query::<(Entity, &Projectile)>();
    let flying: Vec<Entity> = query.iter(world).map(|(e, _)| e).collect();
    for e in flying {
        step(world, e, dt);
    }
}

/// One tick of tg `process` -> `process_movement` for projectile `e`.
fn step(world: &mut World, e: Entity, dt: f32) {
    let Some(mut p) = world.get::<Projectile>(e).cloned() else {
        return;
    };
    let Some(&mut_tile) = world.get::<TilePos>(e) else {
        world.despawn(e);
        return;
    };
    let mut tile = mut_tile;
    let mut left = p.data.speed * 10.0 * dt;
    while left > 0.0 {
        let d = left.min(STEP);
        left -= d;
        let (old_x, old_y) = (p.x, p.y);
        p.x += p.dx * d;
        p.y += p.dy * d;
        let next = TilePos::new(p.x.round() as i32, p.y.round() as i32, tile.z);
        if next == tile {
            continue;
        }
        let prev = tile;
        tile = next;
        p.moved += 1.0;
        // tg `reduce_range`: wound falloff per tile.
        if p.data.wound_falloff_tile != 0.0 && p.data.wound_bonus != CANT_WOUND {
            p.data.wound_bonus += p.data.wound_falloff_tile;
            p.data.exposed_wound_bonus =
                (p.data.exposed_wound_bonus + p.data.wound_falloff_tile).max(0.0);
        }
        if p.data.damage >= 0.0 {
            p.data.damage += p.data.damage_falloff_tile;
        }
        if p.data.stamina >= 0.0 {
            p.data.stamina += p.data.stamina_falloff_tile;
        }
        if !world.resource::<SpatialIndex>().in_bounds(tile) {
            world.despawn(e);
            return;
        }
        if let Some(target) = select_target(world, &p, e, tile) {
            // `kinetic/prehit_pierce`: a modkit's ignored mob is passed through
            // (`PROJECTILE_PIERCE_PHASE`).
            if crate::mining::modkits::phases(world, &p, target) {
                continue;
            }
            // The sleeping carp's deflection (`COMSIG_ATOM_PRE_BULLET_ACT`, `crate::martial`):
            // it flies on from here, the carp's own now.
            if world.get::<Health>(target).is_some()
                && crate::martial::deflect_projectile(world, target, e, &mut p)
            {
                continue;
            }
            // Reflective armor (`crate::clothing::ablative`): it flies on, the wearer's now.
            if world.get::<Health>(target).is_some()
                && crate::clothing::ablative::reflect_projectile(world, target, e, &mut p)
            {
                continue;
            }
            // A nightmare in the dark dodges it (`crate::antag::nightmare`): it flies on.
            if world.get::<Health>(target).is_some()
                && crate::antag::nightmare::dodge_projectile(world, target, e)
            {
                continue;
            }
            if try_ricochet(world, &mut p, target, tile, prev) {
                // It stays where it was, turned (tg: the bump happens from the turf before).
                p.x = old_x;
                p.y = old_y;
                tile = prev;
                p.moved -= 1.0;
                continue;
            }
            let name = crate::obj_damage::strip(&crate::security::name_of(world, e)).to_owned();
            crate::mining::rocks::kinetic_prehit(world, &mut p, target);
            hit(world, e, &p, &name, target);
            crate::mining::rocks::kinetic_strike(world, &p, target);
            crate::mining::rocks::bullet_act(world, p.firer, target);
            if let Ok(entity) = world.get_entity_mut(e) {
                entity.despawn();
            }
            return;
        }
        if p.moved >= p.data.range {
            crate::mining::rocks::kinetic_on_range(world, &mut p, tile);
            world.despawn(e);
            return;
        }
    }
    let offset = [
        ((p.x - tile.x as f32) * 32.0).round() as i16,
        ((p.y - tile.y as f32) * 32.0).round() as i16,
    ];
    if tile != mut_tile {
        world.entity_mut(e).insert(tile);
    }
    if let Some(mut look) = world.get_mut::<Appearance>(e) {
        look.pixel_offset = offset;
    }
    world.entity_mut(e).insert(p);
}

/// tg `flags_ricochet` (`RICOCHET_HARD` 1, `RICOCHET_SHINY` 2) and
/// `receive_ricochet_chance_mod` of what a projectile hit.
// shortcut: the walls' and windows' values are written here, not converted per type; they are
// the only atoms with `flags_ricochet` in tg.
fn ricochet_surface(world: &World, e: Entity) -> Option<(u32, f32)> {
    let path = crate::construction::path_of(world, e)?;
    let path = path.as_str();
    if is_under(path, "/turf/closed/wall/mineral/titanium") {
        Some((RICOCHET_HARD | RICOCHET_SHINY, 1.0))
    } else if is_under(path, "/turf/closed/wall") {
        Some((RICOCHET_HARD, 1.0))
    } else if is_under(path, "/obj/structure/window/reinforced/shuttle")
        || is_under(path, "/obj/structure/window/reinforced/titanium")
        || is_under(path, "/obj/structure/window/reinforced/plasma/plastitanium")
    {
        Some((RICOCHET_HARD, 1.2))
    } else if is_under(path, "/obj/structure/window/reinforced") {
        Some((RICOCHET_HARD, 1.1))
    } else if is_under(path, "/obj/structure/window") {
        Some((RICOCHET_HARD, 0.5))
    } else {
        None
    }
}
const RICOCHET_HARD: u32 = 1;
const RICOCHET_SHINY: u32 = 2;

/// tg `impact`'s ricochet: `check_ricochet_flag`, `check_ricochet`, `handle_ricochet`, then
/// the decays. True when `p` bounced off `target` (turned, back on `prev`).
fn try_ricochet(
    world: &mut World,
    p: &mut Projectile,
    target: Entity,
    tile: TilePos,
    prev: TilePos,
) -> bool {
    if p.ricochets >= p.data.ricochets_max {
        return false;
    }
    let Some((flags, chance_mod)) = ricochet_surface(world, target) else {
        return false;
    };
    let flag = p.data.armor_flag.as_str();
    let ok = (matches!(flag, "energy" | "laser") && flags & RICOCHET_SHINY != 0)
        || (matches!(flag, "bomb" | "bullet") && flags & RICOCHET_HARD != 0);
    if !ok
        || !world
            .resource_mut::<CombatRng>()
            .chance(p.data.ricochet_chance * chance_mod)
    {
        return false;
    }
    let Some(new_angle) = ricochet_angle(tile, prev, p.angle, &p.data) else {
        return false;
    };
    p.ricochets += 1;
    p.angle = new_angle;
    let (sin, cos) = new_angle.to_radians().sin_cos();
    p.dx = sin;
    p.dy = cos;
    p.data.range = (p.data.range - REFLECT_RANGE_DECREASE).max(0.0);
    p.moved = 0.0;
    p.data.ricochet_chance *= p.data.ricochet_decay_chance;
    p.data.damage *= p.data.ricochet_decay_damage;
    p.data.stamina *= p.data.ricochet_decay_damage;
    true
}

/// tg `/atom/handle_ricochet`: the surface on `at` faces the projectile's tile `from`; `None`
/// when the projectile comes from behind or (bullets) too steeply for its
/// `ricochet_incidence_leeway`.
pub fn ricochet_angle(
    at: TilePos,
    from: TilePos,
    angle: f32,
    data: &ProjectileData,
) -> Option<f32> {
    let face = crate::mecha::core::dir_between(at, from)?;
    let face_angle = dir_angle(face);
    let incidence = (face_angle - (angle + 180.0)).rem_euclid(360.0);
    if incidence > 90.0 && incidence < 270.0 {
        return None;
    }
    let leeway = data.ricochet_incidence_leeway;
    if matches!(data.armor_flag.as_str(), "bullet" | "bomb")
        && leeway != 0.0
        && ((incidence < 90.0 && incidence < 90.0 - leeway)
            || (incidence > 270.0 && incidence - 270.0 > leeway))
    {
        return None;
    }
    Some((face_angle + incidence).rem_euclid(360.0))
}

/// tg `dir2angle`.
fn dir_angle(dir: ss13_shared::Dir) -> f32 {
    use ss13_shared::Dir::*;
    match dir {
        North => 0.0,
        NorthEast => 45.0,
        East => 90.0,
        SouthEast => 135.0,
        South => 180.0,
        SouthWest => 225.0,
        West => 270.0,
        NorthWest => 315.0,
    }
}

/// tg `can_hit_target` for a living mob met on the way (not aimed at).
fn hits_mob(world: &World, p: &Projectile, mob: Entity) -> bool {
    if (mob == p.firer && p.ricochets == 0)
        || world.get::<crate::closets::InCloset>(mob).is_some()
        || world.get::<crate::mecha::Pilot>(mob).is_some()
    {
        return false;
    }
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return false;
    }
    let lying = world
        .get::<crate::health::BodyPosition>(mob)
        .is_some_and(|b| b.lying());
    !lying || (p.hit_prone && p.moved <= MAX_RANGE_HIT_PRONE_TARGETS)
}

/// tg `CanAllowThrough` of a dense object for a projectile with `pass_flags` (the emitter's
/// beam uses it too).
pub fn passes(world: &World, e: Entity, pass_flags: u32) -> bool {
    let Some(path) = world.get::<DmObj>(e).map(|o| o.path.as_str()) else {
        return false;
    };
    (pass_flags & PASSTABLE != 0 && is_under(path, "/obj/structure/table"))
        || (pass_flags & PASSGLASS != 0 && is_under(path, "/obj/structure/window"))
        || (pass_flags & PASSGRILLE != 0 && is_under(path, "/obj/structure/grille"))
        // `/obj/machinery/door/CanAllowThrough`: glass doors let `PASSGLASS` through.
        || (pass_flags & PASSGLASS != 0
            && is_under(path, "/obj/machinery/door")
            && world.get::<Opaque>(e).is_none())
}

/// tg `select_target`: what the projectile hits on entering `tile`.
fn select_target(world: &World, p: &Projectile, me: Entity, tile: TilePos) -> Option<Entity> {
    // A magic missile's `can_only_hit_target`: it phases through everything else.
    if world
        .get::<crate::spells::MagicBolt>(me)
        .is_some_and(|b| b.only_target)
    {
        return p
            .original
            .filter(|&o| world.get::<TilePos>(o) == Some(&tile));
    }
    let index = world.resource::<SpatialIndex>();
    if let Some(original) = p.original
        && original != p.firer
        && world.get::<TilePos>(original) == Some(&tile)
        && (world.get::<Health>(original).is_some()
            || world.get::<Density>(original).is_some()
            || crate::misc_items::target::is_target(world, original))
    {
        return Some(original);
    }
    let on_tile = index.on_tile(tile);
    if let Some(&mob) = on_tile.iter().find(|&&e| {
        e != me
            && world.get::<Health>(e).is_some()
            && world.get::<crate::spells::Jaunting>(e).is_none()
            && hits_mob(world, p, e)
    }) {
        return Some(mob);
    }
    let mut turf = None;
    for &e in index.dense_on_tile(tile) {
        if e == me || e == p.firer || world.get::<Density>(e).is_none() {
            continue;
        }
        if world.get::<Health>(e).is_some() {
            continue;
        }
        if world.get::<Turf>(e).is_some() {
            turf = Some(e);
            continue;
        }
        if !passes(world, e, p.data.pass_flags) {
            return Some(e);
        }
    }
    turf
}

/// A hit of projectile `e`: a magic bolt fizzles on antimagic (`prehit_pierce`), then
/// [`impact`] and the bolt's `on_hit`.
fn hit(world: &mut World, e: Entity, p: &Projectile, name: &str, target: Entity) {
    let bolt = world.get::<crate::spells::MagicBolt>(e).copied();
    if let Some(bolt) = &bolt
        && crate::spells::effects::bolt_blocked(world, bolt, e, target)
    {
        return;
    }
    if crate::mobs::megafauna::deflects_projectile(world, target) {
        return;
    }
    impact(world, p, name, target);
    // A dart's `on_hit`: its reagents or DNA injector (`crate::guns_special`).
    crate::guns_special::dart_hit(world, e, p, target);
    // The kinetic crusher's destabilizer marks or mines (`crate::mining::crusher`).
    crate::mining::crusher::destabilizer_hit(world, e, p.firer, target);
    crate::misc_items::target::bullet_act(world, e, p, target);
    crate::mobs::megafauna::on_projectile_hit(world, p, target);
    if let Some(bolt) = bolt {
        crate::spells::effects::bolt_hit(world, &bolt, p, target);
    }
}

/// tg `on_hit` -> `bullet_act` of `target`.
fn impact(world: &mut World, p: &Projectile, name: &str, target: Entity) {
    ion_pulse(world, p, target);
    let kind = DamageType::from_dm(&p.data.damage_type).unwrap_or(DamageType::Brute);
    if world.get::<Health>(target).is_some() {
        hit_living(world, p, name, target);
        return;
    }
    // `hydroponics/bullet_act`: a somatoray shot on a planted tray.
    if crate::botany::bullet_act(world, &p.data.path, target) {
        return;
    }
    let from = world
        .get::<TilePos>(target)
        .and_then(|&t| crate::mecha::core::dir_between(t, p.start));
    if crate::mecha::core::bullet_act(world, target, p.data.damage, kind, &p.data.armor_flag, from)
    {
        return;
    }
    // `fueltank/bullet_act`: a brute or burn hit blows it up.
    if p.data.damage > 0.0
        && matches!(kind, DamageType::Brute | DamageType::Burn)
        && crate::explosions::sources::is_fueltank(world, target)
    {
        info!("{:?} detonated a fuel tank via projectile", p.firer);
        crate::explosions::sources::boom(world, target);
        return;
    }
    if world.get::<Turf>(target).is_none() && matches!(kind, DamageType::Brute | DamageType::Burn) {
        // shortcut: objects check their melee armor, not the projectile's flag.
        crate::obj_damage::take_damage(world, target, p.data.damage, kind);
    }
}

/// `/obj/projectile/ion/on_hit`: `empulse(target, emp_radius, emp_radius)` (`ion/weak`: 0).
fn ion_pulse(world: &mut World, p: &Projectile, target: Entity) {
    const ION: &str = "/obj/projectile/ion";
    if !ss13_mapload::fidelity::is_under(&p.data.path, ION) {
        return;
    }
    let radius = i32::from(!ss13_mapload::fidelity::is_under(
        &p.data.path,
        "/obj/projectile/ion/weak",
    ));
    if let Some(&pos) = world.get::<TilePos>(target) {
        world.write_message(crate::emp::Empulse {
            epicenter: pos,
            heavy: radius,
            light: radius,
        });
    }
}

/// tg `/mob/living/bullet_act` + `apply_projectile_effects`.
fn hit_living(world: &mut World, p: &Projectile, name: &str, target: Entity) {
    let Some(&at) = world.get::<TilePos>(target) else {
        return;
    };
    // Human `bullet_act` -> `check_block`: a worn reactive armor may stop it (not one's own
    // shot at oneself, `projectile_hit`).
    let own_shot = p.firer == target && p.original == Some(target);
    let blocked =
        !own_shot && crate::clothing::reactive::hit_reaction(world, target, Some(p.firer), name);
    // `COMSIG_ATOM_BULLET_ACT` of a laser tag team member (`crate::clothing::lasertag`).
    if let Some(stamina) = crate::clothing::lasertag::hit_stamina(world, &p.data.path, target) {
        crate::status_effects::adjust_stamina(world, target, stamina);
    }
    if blocked {
        return;
    }
    let distance = p.start.chebyshev(at).unwrap_or(0) as f32;
    let accuracy = (p.data.accurate_range - p.data.accuracy_falloff * distance).clamp(5.0, 100.0);
    let zone =
        crate::medical::random_valid_zone(&mut world.resource_mut::<CombatRng>(), p.zone, accuracy);
    let worn = crate::armor::getarmor(world, target, zone, &p.data.armor_flag);
    let (armor, line) = crate::armor::run_armor_check(
        worn,
        p.data.armour_penetration,
        p.data.weak_against_armour,
        None,
        None,
    );
    let mut personal = Vec::new();
    if let Some((span, text)) = line {
        personal.push((target, span, text));
    }
    if armor < 100.0 {
        let victim = crate::security::name_of(world, target);
        let zone_text = zone.ru().1;
        crate::security::show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{} попадает {victim} в {zone_text}!", capitalize(name)),
            ),
            &[(
                target,
                Span::BoldWarning,
                format!("{} попадает по вам в {zone_text}!", capitalize(name)),
            )],
        );
    }
    for (mob, span, text) in personal {
        if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
            crate::tools::tell(world, client, span, text);
        }
    }
    // Mining mobs' `ranged_armour` (`COMSIG_PROJECTILE_PREHIT`).
    let raw =
        crate::mobs::lavaland::ranged_armour(world, target, p.data.damage, &p.data.damage_type);
    let damage = crate::armor::blocked_damage(raw, armor);
    match DamageType::from_dm(&p.data.damage_type).unwrap_or(DamageType::Brute) {
        DamageType::Stamina => crate::status_effects::adjust_stamina(world, target, damage),
        kind if damage > 0.0 => {
            // tg `apply_damage` -> `receive_damage`: the bodypart's wound damage multiplier,
            // then the wound roll with the projectile's bonuses.
            let damage = damage
                * world
                    .get::<crate::medical::wounds::Limbs>(target)
                    .map_or(1.0, |l| l.limb(zone).damage_multiplier());
            let mut before = 0.0;
            if let Ok(mut entity) = world.get_entity_mut(target)
                && let Ok((mut health, mut stat)) =
                    entity.get_components_mut::<(&mut Health, &mut Stat)>()
            {
                before = health.part(zone).damage();
                let mut new = *stat;
                crate::health::apply_damage_zone(&mut health, &mut new, zone, kind, damage);
                stat.set_if_neq(new);
            }
            crate::medical::wounds::after_hit(
                world,
                target,
                zone,
                crate::medical::wounds::Hit {
                    kind,
                    damage,
                    sharpness: p.data.sharpness,
                    wound_bonus: p.data.wound_bonus,
                    exposed_wound_bonus: p.data.exposed_wound_bonus,
                    part_damage_before: before,
                },
            );
        }
        _ => {}
    }
    // `create_projectile_hit_effects`: a brute hit splatters blood.
    if DamageType::from_dm(&p.data.damage_type) == Some(DamageType::Brute) {
        let dir = crate::effects::temp_visual::angle_dir(p.angle);
        crate::effects::temp_visual::blood_splatter(world, target, dir);
    }
    // `/obj/projectile/energy/electrode/on_hit`: the electrodes catch on.
    if p.data.tase_stamina > 0.0 && armor < 100.0 {
        crate::taser::tase(world, p, target);
    }
    // tg `apply_effects`: nothing at full block, the rest scaled by what gets through.
    if armor >= 100.0 {
        return;
    }
    let through = (100.0 - armor) / 100.0;
    for (kind, ds) in [
        (Incap::Stun, p.data.stun),
        (Incap::Paralyze, p.data.paralyze),
        (Incap::Immobilize, p.data.immobilize),
    ] {
        if ds > 0.0 {
            crate::status_effects::apply(world, target, kind, ds / 10.0 * through);
        }
    }
    if p.data.knockdown > 0.0 {
        crate::status_effects::knockdown(world, target, p.data.knockdown / 10.0 * through, 0.0);
    }
    if p.data.stamina > 0.0 {
        crate::status_effects::adjust_stamina(world, target, p.data.stamina * through);
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_are_clockwise_from_north() {
        let o = TilePos::new(10, 10, 1);
        assert_eq!(angle_between(o, TilePos::new(10, 15, 1)), 0.0);
        assert_eq!(angle_between(o, TilePos::new(15, 10, 1)), 90.0);
        assert_eq!(angle_between(o, TilePos::new(10, 5, 1)), 180.0);
        assert_eq!(angle_between(o, TilePos::new(5, 10, 1)), 270.0);
        assert!((angle_between(o, TilePos::new(12, 12, 1)) - 45.0).abs() < 1e-4);
    }

    #[test]
    fn aim_points_at_the_clicked_pixel_and_clamps_it() {
        let o = TilePos::new(10, 10, 1);
        let to = TilePos::new(14, 12, 1);
        let corner = aim_angle(o, to, [16, 16]);
        assert!((corner - 144.0f32.atan2(80.0).to_degrees()).abs() < 1e-3);
        assert_eq!(aim_angle(o, to, [0, 0]), angle_between(o, to));
        assert_eq!(aim_angle(o, to, [900, -900]), aim_angle(o, to, [32, -32]));
        assert_eq!(aim_angle(o, o, [0, 0]), 0.0);
    }

    #[test]
    fn ricochets_reflect_and_respect_the_leeway() {
        let mut data = ProjectileData {
            armor_flag: "bullet".into(),
            ricochet_incidence_leeway: 0.0,
            ..default()
        };
        let wall = TilePos::new(5, 5, 1);
        // Head on from the west: straight back.
        assert_eq!(
            ricochet_angle(wall, TilePos::new(4, 5, 1), 90.0, &data),
            Some(270.0)
        );
        // Grazing a north face going east-south-east: out east-north-east.
        assert_eq!(
            ricochet_angle(wall, TilePos::new(5, 6, 1), 120.0, &data),
            Some(60.0)
        );
        // From behind the face: no bounce.
        assert_eq!(
            ricochet_angle(wall, TilePos::new(5, 6, 1), 0.0, &data),
            None
        );
        // A 40 degree leeway lets only shots within 40 degrees of the surface bounce.
        data.ricochet_incidence_leeway = 40.0;
        assert_eq!(
            ricochet_angle(wall, TilePos::new(4, 5, 1), 90.0, &data),
            None
        );
        assert_eq!(
            ricochet_angle(wall, TilePos::new(5, 6, 1), 120.0, &data),
            Some(60.0)
        );
    }
}
