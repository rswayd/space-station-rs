//! The proto-kinetic crusher and its trophies (server), after tg
//! `code/modules/mining/equipment/kinetic_crusher/` (`/obj/item/kinetic_crusher`,
//! `/obj/projectile/destabilizer`, `/obj/item/crusher_trophy`, `trophies_fauna.dm`).
//!
//! - Used in hand it is wielded (the other hand empty): 20 force instead of 0; leaving both
//!   hands unwields it.
//! - A wielded right click fires the destabilizer (charged: 1.5 s to recharge, the legion
//!   skull 0.3 s less each, `kinetic_reload.ogg`): it marks a large mob it hits
//!   (`crate::afflictions::apply_crusher_mark`) and mines a mineral wall.
//! - A wielded hit on a marked mob detonates the mark: 50 brute, +30 from behind or with a
//!   boosted mark (`kinetic_accel.ogg`), plus the trophies' bonuses.
//! - Trophies attach by clicking one on the crusher or the crusher on one (one of each kind);
//!   a crowbar takes them all off. Effects: the goliath tentacle (2 more damage per 10 missing
//!   health of the user), the lobster claw (rebukes the target for a second), the legion skull
//!   (faster recharge).
// shortcut: wielding is the crusher's own flag (no `two_handed` component yet: no wielded
// icon, no unwield on swapping hands beyond the hand check each tick); an unwielded hit does
// no damage instead of being refused; no light, reskins, butchering, radial trophy removal,
// `crusher_damage` loot tracking, parry, ally-ignoring raptor feather; the other trophies
// (watcher, ice and magma wings, megafauna) attach without their effects.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Facing, TilePos};

use crate::combat::NextMove;
use crate::health::{Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::tools::tell;

pub const CRUSHER: &str = "/obj/item/kinetic_crusher";
pub const TROPHY: &str = "/obj/item/crusher_trophy";
pub const DESTABILIZER: &str = "/obj/projectile/destabilizer";
/// `force_wielded`, `charge_time`, `backstab_bonus`.
pub const FORCE_WIELDED: f32 = 20.0;
pub const CHARGE_SECS: f64 = 1.5;
pub const BACKSTAB_BONUS: f32 = 30.0;
pub const FIRE_SOUND: &str = "sound/items/weapons/plasma_cutter.ogg";
pub const RECHARGE_SOUND: &str = "sound/items/weapons/kinetic_reload.ogg";
pub const BACKSTAB_SOUND: &str = "sound/items/weapons/kinetic_accel.ogg";
const GOLIATH: &str = "/obj/item/crusher_trophy/goliath_tentacle";
const LOBSTER: &str = "/obj/item/crusher_trophy/lobster_claw";
const LEGION: &str = "/obj/item/crusher_trophy/legion_skull";

/// A crusher's state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Crusher {
    pub wielded: bool,
    /// Recharging until this game time (`charged` once past).
    pub recharge_at: Option<f64>,
    /// Attached trophies, held as parts of the crusher.
    pub trophies: Vec<Entity>,
}

/// On a destabilizer in flight: the crusher it came from.
#[derive(Component, Clone, Copy, Debug)]
pub struct Destabilizer {
    pub crusher: Entity,
    pub boosted: bool,
}

pub struct CrusherPlugin;

impl Plugin for CrusherPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Wield and unwield.
        app.add_click_handler(ClickPhase::AttackSelf, 3245, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        // Trophies on and off.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3475, |w, c| {
            item_interaction(w, c.client, c.mob, c.held, c.target)
        });
        app.add_systems(
            Update,
            fire_clicks
                .after(crate::click::ResolveClicks)
                .after(crate::medical::MedicalClicks)
                .before(crate::combat::MeleeSystems)
                .before(crate::inventory::inventory_intents),
        )
        .add_systems(Update, upkeep);
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn path_of(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

pub fn is_crusher(world: &World, e: Entity) -> bool {
    path_of(world, e).is_some_and(|p| is_under(p, CRUSHER))
}

fn state(world: &mut World, crusher: Entity) -> Crusher {
    if let Some(c) = world.get::<Crusher>(crusher) {
        return c.clone();
    }
    world.entity_mut(crusher).insert(Crusher::default());
    Crusher::default()
}

fn set_state(world: &mut World, crusher: Entity, c: Crusher) {
    world.entity_mut(crusher).insert(c);
}

fn play(world: &mut World, e: Entity, sound: &str, vol: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, e) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, vol, true));
    }
}

fn set_force(world: &mut World, crusher: Entity, wielded: bool) {
    if let Some(mut item) = world.get_mut::<DmItem>(crusher) {
        item.force = if wielded { FORCE_WIELDED } else { 0.0 };
    }
}

/// `two_handed` `attack_self`: wield with the other hand empty, or unwield.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, crusher: Entity) -> bool {
    if !is_crusher(world, crusher) {
        return false;
    }
    let mut c = state(world, crusher);
    if c.wielded {
        c.wielded = false;
        tell(
            world,
            client,
            Span::Notice,
            "Вы берёте крушитель в одну руку.",
        );
    } else {
        let free = world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.hands.iter().filter(|h| h.is_some()).count() == 1);
        if !free {
            tell(world, client, Span::Warning, "Нужна свободная вторая рука!");
            return true;
        }
        c.wielded = true;
        tell(
            world,
            client,
            Span::Notice,
            "Вы берёте крушитель в обе руки.",
        );
    }
    let wielded = c.wielded;
    set_state(world, crusher, c);
    set_force(world, crusher, wielded);
    true
}

/// The recharge (`recharge_projectile`) and the wield kept only in both hands.
fn upkeep(world: &mut World) {
    let now = now(world);
    let crushers: Vec<(Entity, Crusher)> = world
        .query::<(Entity, &Crusher)>()
        .iter(world)
        .map(|(e, c)| (e, c.clone()))
        .collect();
    for (e, mut c) in crushers {
        let mut changed = false;
        if c.recharge_at.is_some_and(|t| now >= t) {
            c.recharge_at = None;
            changed = true;
            play(world, e, RECHARGE_SOUND, 60.0);
        }
        if c.wielded {
            let holder = world.get::<CarriedBy>(e).map(|c| c.0);
            let held = holder
                .and_then(|h| world.get::<Inventory>(h))
                .is_some_and(|i| {
                    i.hands.contains(&Some(e)) && i.hands.iter().flatten().count() == 1
                });
            if !held {
                c.wielded = false;
                changed = true;
                set_force(world, e, false);
            }
        }
        if changed {
            set_state(world, e, c);
        }
    }
}

fn trophy_paths(world: &World, c: &Crusher) -> Vec<String> {
    c.trophies
        .iter()
        .filter_map(|&t| path_of(world, t).map(str::to_owned))
        .collect()
}

/// `charge_time` with the legion skulls' 0.3 s each.
pub fn charge_secs(paths: &[String]) -> f64 {
    let skulls = paths.iter().filter(|p| is_under(p, LEGION)).count();
    (CHARGE_SECS - 0.3 * skulls as f64).max(0.0)
}

/// `interact_with_atom_secondary` -> `fire_kinetic_blast` on a right click with a crusher.
fn fire_clicks(world: &mut World, mut clicks: Local<MessageCursor<crate::click::ClickResolved>>) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    let now = now(world);
    for click in clicks {
        let mob = click.mob;
        let Some(crusher) = world.get::<Inventory>(mob).and_then(Inventory::active_item) else {
            continue;
        };
        if !click.modifiers.right
            || !is_crusher(world, crusher)
            || world.get::<NextMove>(mob).is_some_and(|n| now < n.0)
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
        {
            continue;
        }
        world
            .resource_mut::<crate::medical::MedicalHandled>()
            .0
            .insert(mob);
        let c = state(world, crusher);
        if !c.wielded {
            tell(
                world,
                click.client,
                Span::Warning,
                "Сначала возьмите его в обе руки!",
            );
            continue;
        }
        if click.target == mob {
            tell(
                world,
                click.client,
                Span::Warning,
                "Нельзя целиться в себя!",
            );
            continue;
        }
        if let Some(mut next) = world.get_mut::<NextMove>(mob) {
            next.0 = now + crate::combat::CLICK_CD_MELEE;
        }
        if c.recharge_at.is_some() {
            continue;
        }
        fire(
            world,
            mob,
            crusher,
            click.mob_pos,
            click.target,
            click.target_pos,
        );
    }
}

/// `/obj/projectile/destabilizer`: no damage, `BOMB` armor, 6 tiles.
pub fn destabilizer_data() -> ss13_mapload::ProjectileData {
    ss13_mapload::ProjectileData {
        path: DESTABILIZER.to_owned(),
        damage: 0.0,
        damage_type: "brute".into(),
        armor_flag: "bomb".into(),
        speed: 1.25,
        range: 6.0,
        accurate_range: 100.0,
        ..default()
    }
}

fn fire(
    world: &mut World,
    mob: Entity,
    crusher: Entity,
    from: TilePos,
    target: Entity,
    target_pos: TilePos,
) {
    let data = destabilizer_data();
    let zone = world
        .get::<crate::health::ZoneSelected>(mob)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone();
    let shot = crate::projectile::Shot {
        data: &data,
        firer: mob,
        from,
        target: Some(target),
        target_pos,
        zone,
        hit_prone: true,
        pixel: [0, 0],
        deviation: 0.0,
        fired_from: Some(crusher),
    };
    crate::projectile::fire_with(
        world,
        shot,
        Destabilizer {
            crusher,
            boosted: false,
        },
    );
    play(world, mob, FIRE_SOUND, 100.0);
    let paths = {
        let c = state(world, crusher);
        trophy_paths(world, &c)
    };
    let mut c = state(world, crusher);
    c.recharge_at = Some(now(world) + charge_secs(&paths));
    set_state(world, crusher, c);
}

/// `destabilizer/on_hit` (`crate::projectile`): a living target is marked, a mineral wall mined.
pub fn destabilizer_hit(world: &mut World, projectile: Entity, firer: Entity, target: Entity) {
    let Some(d) = world.get::<Destabilizer>(projectile).copied() else {
        return;
    };
    if world.get::<Health>(target).is_some() {
        crate::afflictions::apply_crusher_mark(world, target, d.boosted);
        return;
    }
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::mining::rocks::gets_drilled(world, pos, Some(firer));
    }
}

/// `check_behind(user, target)`: the user stands in the half the target turns its back on.
pub fn behind(target_pos: TilePos, facing: Facing, user_pos: TilePos) -> bool {
    let (rx, ry) = facing.0.reverse().offset();
    let (dx, dy) = (
        (user_pos.x - target_pos.x).signum(),
        (user_pos.y - target_pos.y).signum(),
    );
    (rx != 0 && dx == rx) || (ry != 0 && dy == ry)
}

/// The trophies' `on_mark_detonation` bonus damage (the goliath tentacle) and effects (the
/// lobster claw's rebuke).
pub fn trophy_bonus(world: &mut World, paths: &[String], user: Entity, target: Entity) -> f32 {
    let mut bonus = 0.0;
    for path in paths {
        if is_under(path, GOLIATH) {
            let missing = world
                .get::<Health>(user)
                .map_or(0.0, |h| (h.max - h.health()).max(0.0));
            bonus += missing * 0.1 * 2.0;
        } else if is_under(path, LOBSTER) {
            crate::curses::apply_rebuked_for(world, target, 1.0);
        }
    }
    bonus
}

/// `kinetic_crusher/afterattack` of a wielded hit (`crate::combat`): the mark detonates.
pub fn after_attack(world: &mut World, user: Entity, target: Entity, weapon: Entity) {
    if !is_crusher(world, weapon)
        || world
            .get::<crate::afflictions::CrusherMark>(target)
            .is_none()
    {
        return;
    }
    let c = state(world, weapon);
    if !c.wielded {
        return;
    }
    let paths = trophy_paths(world, &c);
    let bonus = trophy_bonus(world, &paths, user, target);
    let is_behind = match (
        world.get::<TilePos>(target).copied(),
        world.get::<Facing>(target).copied(),
        world.get::<TilePos>(user).copied(),
    ) {
        (Some(t), Some(f), Some(u)) => behind(t, f, u),
        _ => false,
    };
    let boosted = world
        .get::<crate::afflictions::CrusherMark>(target)
        .is_some_and(|m| m.boosted);
    if is_behind || boosted {
        play(world, user, BACKSTAB_SOUND, 100.0);
    }
    crate::afflictions::detonate_crusher_mark(world, target, Some(BACKSTAB_BONUS), is_behind);
    if bonus > 0.0 {
        crate::medical::diseases::damage(world, target, crate::health::DamageType::Brute, bonus);
    }
}

/// Trophy and crusher clicks: `crusher_trophy/add_to` either way round, a crowbar takes all off.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Option<Entity>,
    target: Entity,
) -> bool {
    let Some(held) = held else {
        return false;
    };
    let is_trophy = |w: &World, e: Entity| path_of(w, e).is_some_and(|p| is_under(p, TROPHY));
    if is_trophy(world, held) && is_crusher(world, target) {
        add_trophy(world, client, target, held);
        return true;
    }
    if is_crusher(world, held) && is_trophy(world, target) {
        add_trophy(world, client, held, target);
        return true;
    }
    let crowbar = world
        .get::<ss13_mapload::DmTool>(held)
        .is_some_and(|t| t.behaviour == "crowbar");
    if crowbar && is_crusher(world, target) {
        remove_trophies(world, client, mob, target);
        return true;
    }
    false
}

/// `crusher_trophy/add_to`: one trophy of each `denied_type` (its own type).
pub fn add_trophy(world: &mut World, client: Entity, crusher: Entity, trophy: Entity) -> bool {
    let Some(path) = path_of(world, trophy).map(str::to_owned) else {
        return false;
    };
    let mut c = state(world, crusher);
    let clash = trophy_paths(world, &c)
        .iter()
        .any(|p| is_under(p, &path) || is_under(&path, p));
    if clash {
        tell(
            world,
            client,
            Span::Warning,
            "Не получается прикрепить трофей к крушителю. Может, снять пару трофеев?",
        );
        return false;
    }
    crate::stock_parts::attach(world, trophy, crusher);
    c.trophies.push(trophy);
    set_state(world, crusher, c);
    tell(
        world,
        client,
        Span::Notice,
        "Вы прикрепляете трофей к крушителю.",
    );
    true
}

/// `crowbar_act`: every trophy drops on the crusher's tile.
pub fn remove_trophies(world: &mut World, client: Entity, mob: Entity, crusher: Entity) {
    let mut c = state(world, crusher);
    if c.trophies.is_empty() {
        tell(world, client, Span::Warning, "Трофеев нет!");
        return;
    }
    let Some(pos) =
        crate::explosions::turf_of(world, crusher).or_else(|| world.get::<TilePos>(mob).copied())
    else {
        return;
    };
    for trophy in std::mem::take(&mut c.trophies) {
        if world.get_entity(trophy).is_ok() {
            crate::stock_parts::detach(world, trophy, pos);
        }
    }
    set_state(world, crusher, c);
    tell(world, client, Span::Notice, "Трофеи сняты.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_shared::Dir;

    #[test]
    fn legion_skulls_speed_the_recharge() {
        assert_eq!(charge_secs(&[]), 1.5);
        let skull = vec![LEGION.to_owned()];
        assert!((charge_secs(&skull) - 1.2).abs() < 1e-9);
    }

    #[test]
    fn behind_is_the_side_the_target_turns_its_back_on() {
        let t = TilePos::new(5, 5, 0);
        let (bx, by) = Dir::North.reverse().offset();
        let back = TilePos::new(5 + bx, 5 + by, 0);
        let front = TilePos::new(5 - bx, 5 - by, 0);
        assert!(behind(t, Facing(Dir::North), back));
        assert!(!behind(t, Facing(Dir::North), front));
    }

    #[test]
    fn goliath_tentacle_scales_with_missing_health() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let user = world
            .spawn(Health {
                brute: 50.0,
                ..Health::default()
            })
            .id();
        let target = world.spawn_empty().id();
        let bonus = trophy_bonus(&mut world, &[GOLIATH.to_owned()], user, target);
        assert!((bonus - 10.0).abs() < 1e-4, "{bonus}");
    }
}
