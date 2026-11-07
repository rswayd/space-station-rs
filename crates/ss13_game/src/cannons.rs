//! Cannons and mounted guns (server): tg `/obj/structure/cannon`
//! (`code/game/objects/structures/cannons/cannon.dm`, `cannonballs.dm`) and
//! `/obj/structure/mounted_gun` (`cannons/mounted_guns/mounted_gun.dm`).
//!
//! - A cannon takes one cannonball off a stack, and 15 u of gunpowder or welding fuel from an
//!   open container (not a rag). A hot item lights the charge (`ignition_effect`), and 1.5 s
//!   later it fires the ball the way it faces (`fire(dir2angle(dir))`); fuel takes 20 off the
//!   damage (at least 2). The charge is used up either way. A wrench (2 s) fastens it unless
//!   it is the trash cannon, which wears out (5 shots, fuel twice as fast) and may blow up (10 %,
//!   fuel 20 %). The fishing mystery box cannon comes charged and loaded.
//! - A mounted gun is loaded with its accepted ammo out of combat mode (the casing is used
//!   up, `shots_per_load` up to `max_shots_per_fire`, with `load_delay`). An empty hand fires it
//!   after `fire_delay`: every loaded shot `shot_delay` apart the way it faces, `unanchored_variance`
//!   degrees off when loose, the last one with its own sound. The Ratvarian repeater takes no
//!   ammo: an empty hand on an unloaded one winds it (3 s, 12 shots), and every other shot waits
//!   twice as long.
// shortcut: no cannonball piercing (`projectile_piercing`), wall scraping, object damage,
// explosions of the explosive shell and the biggest one or the EMP shot; the organ gun fires
// plain junk (no weighted special junk); no ballista (its spear rides the projectile); no
// camera shake, firing sprite, debris on destruction or `simple_rotation` (they fire the way
// they were placed).

use bevy::prelude::*;
use ss13_mapload::{DmItem, ProjectileData};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};

use crate::chem::Reagents;
use crate::combat::CombatMode;
use crate::construction::Job;
use crate::obj_damage::is_under;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, ToolKind, do_after, tell, tool_of, use_tool};
use crate::zone::BodyZone;

pub const CANNON_ROOT: &str = "/obj/structure/cannon";
pub const TRASH_CANNON: &str = "/obj/structure/cannon/trash";
pub const MYSTERY_CANNON: &str = "/obj/structure/cannon/mystery_box";
pub const MOUNTED_ROOT: &str = "/obj/structure/mounted_gun";
const ORGAN: &str = "/obj/structure/mounted_gun/organ_gun";
const GATLING: &str = "/obj/structure/mounted_gun/canister_gatling";
const REPEATER: &str = "/obj/structure/mounted_gun/ratvarian_repeater";
const BALLISTA: &str = "/obj/structure/mounted_gun/ballista";
const CANNONBALLS: &str = "/obj/item/stack/cannonball";
const RAG: &str = "/obj/item/rag";
const GUNPOWDER: &str = "/datum/reagent/gunpowder";
const FUEL: &str = "/datum/reagent/fuel";
/// The trash cannon's leftovers (`Destroy`).
pub const TRASH_IRON: &str = "/obj/item/stack/sheet/iron/five";
pub const TRASH_RODS: &str = "/obj/item/stack/rods";

/// Projectiles spawned here (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
pub const CANNONBALL: &str = "/obj/projectile/bullet/cannonball";
pub const CANNONBALL_EXPLOSIVE: &str = "/obj/projectile/bullet/cannonball/explosive";
pub const CANNONBALL_EMP: &str = "/obj/projectile/bullet/cannonball/emp";
pub const CANNONBALL_BIGGEST: &str = "/obj/projectile/bullet/cannonball/biggest_one";
pub const CANNONBALL_TRASH: &str = "/obj/projectile/bullet/cannonball/trashball";
pub const STRILKA: &str = "/obj/projectile/bullet/strilka310";
pub const JUNK: &str = "/obj/projectile/bullet/junk";
pub const REPEATER_LASER: &str = "/obj/projectile/beam/laser/repeater";
const SHRAPNEL: &str = ss13_mapload::runtime::SHRAPNEL;

const CANNON_SOUND: &str = "sound/items/weapons/gun/general/cannon.ogg";
const MOUNTED_SOUND: &str = "sound/items/weapons/gun/general/mountedgun.ogg";
const MOUNTED_END: &str = "sound/items/weapons/gun/general/mountedgunend.ogg";
const THERMAL: &str = "sound/items/weapons/thermalpistol.ogg";
const WIND: &str = "sound/effects/magic/clockwork/fellowship_armory.ogg";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";

/// `charge_size`, units.
pub const CHARGE: f32 = 15.0;
/// `fire_delay` of a cannon, seconds.
const CANNON_FIRE_DELAY: f64 = 1.5;
/// `BAD_FUEL_DAMAGE_TAX`.
const BAD_FUEL_DAMAGE_TAX: f32 = 20.0;
/// `default_unfasten_wrench`, deciseconds.
const WRENCH_DS: f32 = 20.0;

fn bullet(path: &str, damage: f32) -> ProjectileData {
    ProjectileData {
        path: path.to_owned(),
        damage,
        damage_type: "brute".into(),
        armor_flag: "bullet".into(),
        speed: 1.25,
        range: 50.0,
        pass_flags: crate::projectile::PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ricochet_decay_chance: 0.7,
        ricochet_decay_damage: 0.7,
        ricochet_incidence_leeway: 40.0,
        wound_falloff_tile: -5.0,
        // `SHARP_POINTY`.
        sharpness: 2,
        ..default()
    }
}

/// The projectile a cannonball stack of `path` turns into (`projectile_type`).
pub fn cannonball(path: &str) -> ProjectileData {
    let (proj, damage) = if is_under(path, "/obj/item/stack/cannonball/shellball") {
        (CANNONBALL_EXPLOSIVE, 40.0)
    } else if is_under(path, "/obj/item/stack/cannonball/emp") {
        (CANNONBALL_EMP, 15.0)
    } else if is_under(path, "/obj/item/stack/cannonball/the_big_one") {
        (CANNONBALL_BIGGEST, 70.0)
    } else if is_under(path, "/obj/item/stack/cannonball/trashball") {
        (CANNONBALL_TRASH, 90.0)
    } else {
        (CANNONBALL, 110.0)
    };
    ProjectileData {
        sharpness: 0,
        wound_bonus: 0.0,
        // `paralyze = 5 SECONDS`, deciseconds.
        paralyze: 50.0,
        ..bullet(proj, damage)
    }
}

/// A mounted gun type's vars. Times in deciseconds.
#[derive(Clone, Debug, PartialEq)]
pub struct MountedDef {
    pub projectile: ProjectileData,
    pub max_shots: u32,
    pub shots_per_load: u32,
    /// `shots_in_gun` at start.
    pub initial_shots: u32,
    /// Item roots it is loaded with; empty: wound by hand (the repeater).
    pub ammo: &'static [&'static str],
    pub load_delay: f32,
    pub fire_delay: f32,
    pub shot_delay: f32,
    pub fire_sound: &'static str,
    pub last_fire_sound: &'static str,
    pub variance: f32,
}

/// The [`MountedDef`] of `path`; `None` for anything else and the ballista.
pub fn mounted(path: &str) -> Option<MountedDef> {
    if !is_under(path, MOUNTED_ROOT) || is_under(path, BALLISTA) {
        return None;
    }
    let mut d = MountedDef {
        projectile: ProjectileData {
            armour_penetration: 10.0,
            wound_bonus: -45.0,
            wound_falloff_tile: 0.0,
            ..bullet(STRILKA, 60.0)
        },
        max_shots: 1,
        shots_per_load: 1,
        initial_shots: 1,
        ammo: &["/obj/item/ammo_casing/strilka310"],
        load_delay: 0.0,
        fire_delay: 5.0,
        shot_delay: 3.0,
        fire_sound: MOUNTED_SOUND,
        last_fire_sound: MOUNTED_END,
        variance: 20.0,
    };
    if is_under(path, ORGAN) {
        d.projectile = bullet(JUNK, 40.0);
        (d.max_shots, d.shots_per_load, d.initial_shots) = (8, 2, 8);
        d.ammo = &["/obj/item/ammo_casing/junk"];
        (d.fire_delay, d.shot_delay) = (3.0, 2.0);
    } else if is_under(path, GATLING) {
        d.projectile = crate::grenades::shrapnel_data(false);
        d.projectile.path = SHRAPNEL.to_owned();
        (d.max_shots, d.shots_per_load, d.initial_shots) = (50, 50, 50);
        d.ammo = &["/obj/item/ammo_casing/canister_shot"];
        (d.fire_delay, d.shot_delay) = (3.0, 1.0);
    } else if is_under(path, REPEATER) {
        d.projectile = ProjectileData {
            path: REPEATER_LASER.to_owned(),
            damage: 15.0,
            damage_type: "burn".into(),
            armor_flag: "laser".into(),
            weak_against_armour: true,
            pass_flags: crate::projectile::PASSTABLE
                | crate::projectile::PASSGLASS
                | crate::projectile::PASSGRILLE,
            ricochets_max: 50,
            ricochet_chance: 80.0,
            wound_bonus: -20.0,
            exposed_wound_bonus: 40.0,
            sharpness: 0,
            reflectable: true,
            ..bullet(REPEATER_LASER, 15.0)
        };
        (d.max_shots, d.shots_per_load, d.initial_shots) = (12, 12, 12);
        d.ammo = &[];
        d.load_delay = 30.0;
        (d.fire_delay, d.shot_delay) = (1.0, 2.0);
        (d.fire_sound, d.last_fire_sound) = (THERMAL, THERMAL);
    }
    Some(d)
}

/// A cannon's load (`loaded_cannonball`, `reagents`, `charge_ignited`).
#[derive(Component, Clone, Debug, Default)]
pub struct Cannon {
    /// The loaded ball's stack path.
    pub ball: Option<String>,
    pub gunpowder: f32,
    pub fuel: f32,
    /// When the lit charge goes off, and who lit it.
    pub fuse: Option<(f64, Entity)>,
    /// The trash cannon's `fires_before_deconstruction`.
    pub wear: i32,
}

/// A mounted gun's `shots_in_gun`, `is_firing` and its shot timers.
#[derive(Component, Clone, Debug, Default)]
pub struct MountedGun {
    pub shots: u32,
    pub firing: bool,
    pub sequence: Vec<f64>,
    pub user: Option<Entity>,
}

/// The timed steps of this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CannonJob {
    /// Loading `item` into a mounted gun.
    Load { item: Entity },
    /// Winding the Ratvarian repeater.
    Wind,
}

pub struct CannonsPlugin;

impl Plugin for CannonsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::HeldOnTarget, 69, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::EmptyHand, 3529, |w, c| {
            attack_hand(w, c.client, c.mob, c.target)
        });
        app.add_systems(
            Update,
            timers
                .after(crate::click::ResolveClicks)
                .before(crate::projectile::ProjectileSystems),
        );
    }
}

use crate::tools::now;

fn path_of(world: &World, e: Entity) -> String {
    crate::construction::path_of(world, e).unwrap_or_default()
}

use crate::obj_damage::anchored;

fn balloon(world: &mut World, mob: Entity, source: Entity, text: &str) {
    crate::balloon::balloon_alert(world, mob, source, text);
}

/// The cannon's state, the mystery box one charged and loaded at first touch.
fn cannon_state(world: &mut World, e: Entity) -> Cannon {
    if let Some(c) = world.get::<Cannon>(e) {
        return c.clone();
    }
    let path = path_of(world, e);
    let mut c = Cannon {
        wear: 5,
        ..default()
    };
    if is_under(&path, MYSTERY_CANNON) {
        c.gunpowder = CHARGE;
        c.ball = Some(CANNONBALLS.to_owned());
    }
    world.entity_mut(e).insert(c.clone());
    c
}

fn mounted_state(world: &mut World, e: Entity, d: &MountedDef) -> MountedGun {
    if let Some(m) = world.get::<MountedGun>(e) {
        return m.clone();
    }
    let m = MountedGun {
        shots: d.initial_shots,
        ..default()
    };
    world.entity_mut(e).insert(m.clone());
    m
}

fn wrench(world: &mut World, client: Entity, mob: Entity, held: Entity, e: Entity) -> bool {
    if let Some(&pos) = world.get::<TilePos>(e) {
        playsound(world, PlaySound::new(pos, RATCHET, 50.0, true));
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let anchored = anchored(world, e);
    use_tool(world, actor, e, WRENCH_DS, 0, Job::Unfasten { anchored });
    true
}

fn is_hot(world: &World, item: Entity) -> bool {
    crate::medical::surgery::ToolInfo::read(world, Some(item)).temperature
        >= crate::medical::surgery::FIRE_MINIMUM_TEMPERATURE_TO_EXIST
}

fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let path = path_of(world, target);
    if is_under(&path, CANNON_ROOT) {
        return cannon_interaction(world, client, mob, held, target, &path);
    }
    let Some(d) = mounted(&path) else {
        return false;
    };
    if matches!(tool_of(world, held), Some((ToolKind::Wrench, _))) {
        return wrench(world, client, mob, held, target);
    }
    let held_path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if world.get::<CombatMode>(mob).is_some_and(|c| c.0)
        || !d.ammo.iter().any(|root| is_under(&held_path, root))
    {
        return false;
    }
    let m = mounted_state(world, target, &d);
    if m.firing {
        balloon(world, mob, target, "орудие стреляет!");
        return true;
    }
    if m.shots >= d.max_shots {
        balloon(world, mob, target, "уже заряжено!");
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if d.load_delay > 0.0 {
        let (who, what) = (
            crate::medical::name_of(world, mob),
            crate::medical::name_of(world, target),
        );
        crate::medical::visible(
            world,
            mob,
            &format!("Вы начинаете заряжать {what}."),
            &format!("{who} начинает заряжать {what}."),
            Span::Warning,
        );
    }
    do_after(
        world,
        actor,
        target,
        d.load_delay,
        0,
        Job::Cannon(CannonJob::Load { item: held }),
    );
    true
}

fn cannon_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    path: &str,
) -> bool {
    let c = cannon_state(world, target);
    if c.fuse.is_some() {
        balloon(world, mob, target, "сейчас выстрелит!");
        return true;
    }
    let held_path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if matches!(tool_of(world, held), Some((ToolKind::Wrench, _))) {
        if is_under(path, TRASH_CANNON) {
            return false;
        }
        return wrench(world, client, mob, held, target);
    }
    if is_under(&held_path, CANNONBALLS) {
        if c.ball.is_some() {
            balloon(world, mob, target, "уже заряжено!");
            return true;
        }
        if !crate::tools::use_stack(world, held, 1) {
            return true;
        }
        if let Some(mut c) = world.get_mut::<Cannon>(target) {
            c.ball = Some(held_path);
        }
        balloon(world, mob, target, "ядро заряжено");
        return true;
    }
    if is_hot(world, held) {
        if c.gunpowder < CHARGE && c.fuel < CHARGE {
            balloon(world, mob, target, &format!("нужно {CHARGE} ед. заряда!"));
            return true;
        }
        let (who, what) = (
            crate::medical::name_of(world, mob),
            crate::medical::name_of(world, target),
        );
        crate::medical::visible(
            world,
            mob,
            &format!("Вы поджигаете фитиль {what}."),
            &format!("{who} поджигает фитиль {what}."),
            Span::Warning,
        );
        let at = now(world) + CANNON_FIRE_DELAY;
        if let Some(mut c) = world.get_mut::<Cannon>(target) {
            c.fuse = Some((at, mob));
        }
        return true;
    }
    load_charge(world, client, mob, held, target, &held_path)
}

/// A reagent container's gunpowder or fuel into the cannon.
fn load_charge(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    held_path: &str,
) -> bool {
    let Some(chem) = world.get_resource::<crate::chem::Chem>().cloned() else {
        return false;
    };
    let Some(holder) = world.get::<Reagents>(held).map(|r| r.0.clone()) else {
        return false;
    };
    if !holder.is_open_container() || is_under(held_path, RAG) {
        return false;
    }
    if holder.total_volume <= 0.0 {
        balloon(world, mob, target, "пусто!");
        return true;
    }
    let c = world.get::<Cannon>(target).cloned().unwrap_or_default();
    if c.gunpowder + c.fuel >= CHARGE {
        balloon(world, mob, target, "полно!");
        return true;
    }
    let amount = |path: &str| chem.0.id(path).map_or(0.0, |id| holder.amount(id));
    let (reagent, powder) = if amount(GUNPOWDER) >= CHARGE {
        (GUNPOWDER, true)
    } else if amount(FUEL) >= CHARGE {
        (FUEL, false)
    } else {
        balloon(world, mob, target, "нужно 15 ед. заряда!");
        tell(
            world,
            client,
            Span::Warning,
            "Для зарядки нужно хотя бы 15 ед. пороха!",
        );
        return true;
    };
    let Some(id) = chem.0.id(reagent) else {
        return true;
    };
    if let Some(mut r) = world.get_mut::<Reagents>(held) {
        r.0.remove_reagent(id, CHARGE);
    }
    if let Some(mut c) = world.get_mut::<Cannon>(target) {
        if powder {
            c.gunpowder = CHARGE;
        } else {
            c.fuel = CHARGE;
        }
    }
    balloon(
        world,
        mob,
        target,
        if powder {
            "заряжено порохом"
        } else {
            "заряжено сварочным топливом"
        },
    );
    true
}

fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(d) = mounted(&path_of(world, target)) else {
        return false;
    };
    let m = mounted_state(world, target, &d);
    if m.firing || !m.sequence.is_empty() {
        balloon(world, mob, target, "орудие стреляет!");
        return true;
    }
    // The repeater winds instead of firing when not full.
    if d.ammo.is_empty() && m.shots < d.max_shots {
        let (who, what) = (
            crate::medical::name_of(world, mob),
            crate::medical::name_of(world, target),
        );
        crate::medical::visible(
            world,
            mob,
            &format!("Вы начинаете заводить {what}."),
            &format!("{who} начинает заводить {what}."),
            Span::Warning,
        );
        let actor = Actor {
            client,
            mob,
            item: None,
        };
        do_after(
            world,
            actor,
            target,
            d.load_delay,
            0,
            Job::Cannon(CannonJob::Wind),
        );
        return true;
    }
    if m.shots == 0 {
        // `fire_sequence` after `fire_delay` says so; nothing would be fired anyway.
        balloon(world, mob, target, "не заряжено!");
        return true;
    }
    let start = now(world) + f64::from(d.fire_delay) / 10.0;
    let repeater = is_under(&path_of(world, target), REPEATER);
    let mut sequence = Vec::new();
    let mut delay = 0.0f64;
    let mut left = m.shots;
    while left > 0 {
        left -= 1;
        sequence.push(start + delay / 10.0);
        // `time_until_next_shot`: the repeater pauses after every other shot.
        delay += f64::from(if repeater && left % 2 != 1 {
            d.shot_delay * 2.0
        } else {
            d.shot_delay
        });
    }
    if let Some(mut m) = world.get_mut::<MountedGun>(target) {
        m.sequence = sequence;
        m.firing = true;
        m.user = Some(mob);
    }
    true
}

/// [`CannonJob`] at the end of its `do_after`.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: CannonJob) {
    let target = action.target;
    let Some(d) = mounted(&path_of(world, target)) else {
        return;
    };
    let m = mounted_state(world, target, &d);
    if m.firing || m.shots >= d.max_shots {
        return;
    }
    match job {
        CannonJob::Load { item } => {
            if world.get_entity(item).is_err() {
                return;
            }
            world.despawn(item);
            balloon(world, mob, target, "орудие заряжено");
        }
        CannonJob::Wind => {
            if let Some(&pos) = world.get::<TilePos>(target) {
                playsound(world, PlaySound::new(pos, WIND, 50.0, false));
            }
        }
    }
    if let Some(mut m) = world.get_mut::<MountedGun>(target) {
        m.shots = (m.shots + d.shots_per_load).min(d.max_shots);
    }
}

fn facing(world: &World, e: Entity) -> Dir {
    world.get::<Facing>(e).map_or(Dir::South, |f| f.0)
}

/// Fires `data` from `e` the way it faces, `deviation` degrees off.
fn shoot(world: &mut World, e: Entity, firer: Entity, data: &ProjectileData, deviation: f32) {
    let Some(&from) = world.get::<TilePos>(e) else {
        return;
    };
    let (dx, dy) = facing(world, e).offset();
    let target_pos = TilePos {
        x: from.x + dx * 10,
        y: from.y + dy * 10,
        ..from
    };
    crate::projectile::fire(
        world,
        crate::projectile::Shot {
            data,
            firer,
            from,
            target: None,
            target_pos,
            zone: BodyZone::Chest,
            hit_prone: false,
            pixel: [0, 0],
            deviation,
            fired_from: Some(e),
        },
    );
}

/// tg `cannon/fire`: the ball (fuel takes [`BAD_FUEL_DAMAGE_TAX`] off), the charge used up.
pub fn fire_cannon(world: &mut World, e: Entity) {
    let Some(c) = world.get::<Cannon>(e).cloned() else {
        return;
    };
    if let Some(&pos) = world.get::<TilePos>(e) {
        playsound(world, PlaySound::new(pos, CANNON_SOUND, 50.0, true));
    }
    if let Some(ball) = &c.ball {
        let mut data = cannonball(ball);
        if c.fuel >= CHARGE {
            data.damage = (data.damage - BAD_FUEL_DAMAGE_TAX).max(2.0);
        }
        shoot(world, e, e, &data, 0.0);
    }
    let used_fuel = c.fuel >= CHARGE;
    if let Some(mut c) = world.get_mut::<Cannon>(e) {
        c.ball = None;
        c.gunpowder = 0.0;
        c.fuel = 0.0;
        c.fuse = None;
    }
    if is_under(&path_of(world, e), TRASH_CANNON) {
        trash_wear(world, e, used_fuel);
    }
}

/// `cannon/trash/fire`: wear, and maybe a blast.
fn trash_wear(world: &mut World, e: Entity, used_fuel: bool) {
    let explode_chance = if used_fuel { 20.0 } else { 10.0 };
    let wear = {
        let Some(mut c) = world.get_mut::<Cannon>(e) else {
            return;
        };
        c.wear -= if used_fuel { 2 } else { 1 };
        c.wear
    };
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let roll = world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(explode_chance);
    if roll {
        crate::medical::visible(
            world,
            e,
            "",
            &format!("{} взрывается!", crate::medical::name_of(world, e)),
            Span::Danger,
        );
        world.despawn(e);
        let mut ex = crate::explosions::Explosion::new(0, 1, 5);
        ex.flame = Some(5.0);
        ex.cause = "trash cannon".to_owned();
        crate::explosions::explosion(world, pos, ex);
    } else if wear <= 0 {
        crate::medical::visible(
            world,
            e,
            "",
            &format!(
                "{} разваливается от работы!",
                crate::medical::name_of(world, e)
            ),
            Span::Warning,
        );
        world.despawn(e);
    } else {
        return;
    }
    crate::construction::spawn_stack(world, TRASH_IRON, pos, 5);
    crate::construction::spawn_stack(world, TRASH_RODS, pos, 1);
}

fn timers(world: &mut World) {
    let t = now(world);
    let lit: Vec<Entity> = world
        .query::<(Entity, &Cannon)>()
        .iter(world)
        .filter(|(_, c)| c.fuse.is_some_and(|(at, _)| at <= t))
        .map(|(e, _)| e)
        .collect();
    for e in lit {
        fire_cannon(world, e);
    }
    let due: Vec<Entity> = world
        .query::<(Entity, &MountedGun)>()
        .iter(world)
        .filter(|(_, m)| m.sequence.first().is_some_and(|&at| at <= t))
        .map(|(e, _)| e)
        .collect();
    for e in due {
        while world
            .get::<MountedGun>(e)
            .is_some_and(|m| m.sequence.first().is_some_and(|&at| at <= t))
        {
            fire_loop(world, e);
        }
    }
}

/// tg `fire_loop` + `fire_gun` (+ `finish_firing` after the last).
fn fire_loop(world: &mut World, e: Entity) {
    let Some(d) = mounted(&path_of(world, e)) else {
        world.entity_mut(e).remove::<MountedGun>();
        return;
    };
    let (user, left) = {
        let mut m = world
            .get_mut::<MountedGun>(e)
            .expect("checked by the caller");
        m.sequence.remove(0);
        m.shots = m.shots.saturating_sub(1);
        if m.sequence.is_empty() {
            m.firing = false;
        }
        (m.user, m.shots)
    };
    if let Some(&pos) = world.get::<TilePos>(e) {
        let sound = if left > 0 {
            d.fire_sound
        } else {
            d.last_fire_sound
        };
        playsound(world, PlaySound::new(pos, sound, 50.0, false));
    }
    let deviation = if anchored(world, e) {
        0.0
    } else {
        let v = d.variance as u32;
        world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, 2 * v) as f32
            - v as f32
    };
    shoot(world, e, user.unwrap_or(e), &d.projectile, deviation);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mounted_defs_follow_tg() {
        let base = mounted("/obj/structure/mounted_gun").unwrap();
        assert_eq!((base.max_shots, base.initial_shots), (1, 1));
        assert_eq!(base.projectile.path, STRILKA);
        let organ = mounted(ORGAN).unwrap();
        assert_eq!((organ.max_shots, organ.shots_per_load), (8, 2));
        let gatling = mounted(GATLING).unwrap();
        assert_eq!(gatling.max_shots, 50);
        assert_eq!(gatling.projectile.path, SHRAPNEL);
        let rep = mounted(REPEATER).unwrap();
        assert!(rep.ammo.is_empty());
        assert_eq!(rep.load_delay, 30.0);
        assert!(mounted(BALLISTA).is_none());
        assert!(mounted(CANNON_ROOT).is_none());
    }

    #[test]
    fn cannonballs_by_stack() {
        assert_eq!(cannonball(CANNONBALLS).damage, 110.0);
        assert_eq!(
            cannonball("/obj/item/stack/cannonball/fourteen").path,
            CANNONBALL
        );
        assert_eq!(
            cannonball("/obj/item/stack/cannonball/shellball/seven").path,
            CANNONBALL_EXPLOSIVE
        );
        assert_eq!(
            cannonball("/obj/item/stack/cannonball/trashball").damage,
            90.0
        );
        assert_eq!(cannonball(CANNONBALLS).paralyze, 50.0);
    }

    #[test]
    fn a_mounted_gun_fires_its_loaded_shots_in_sequence() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let gun = world
            .spawn((
                ss13_mapload::DmObj {
                    path: ORGAN.to_owned(),
                    ..default()
                },
                TilePos::new(5, 5, 1),
            ))
            .id();
        let mob = world.spawn(TilePos::new(5, 4, 1)).id();
        let client = world.spawn_empty().id();
        assert!(attack_hand(&mut world, client, mob, gun));
        let m = world.get::<MountedGun>(gun).unwrap();
        assert!(m.firing);
        assert_eq!(m.sequence.len(), 8);
        // `fire_delay` 0.3 s, then 0.2 s apart.
        assert!((m.sequence[0] - 0.3).abs() < 1e-9);
        assert!((m.sequence[7] - 1.7).abs() < 1e-9);
        // Firing: another click does not start a second sequence.
        assert!(attack_hand(&mut world, client, mob, gun));
        assert_eq!(world.get::<MountedGun>(gun).unwrap().sequence.len(), 8);
    }
}
