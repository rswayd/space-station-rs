//! Manned deployable turrets (server): tg `/obj/machinery/deployable_turret`
//! (`code/game/objects/structures/deployable_turret.dm`), the folded heavy machine gun
//! (`/obj/item/deployable_turret_folded`, `/datum/component/deployable`) and the fork's CM90
//! laser machine gun (`modular_bandastation/weapon/code/ranged/ballistic/mounted_machine_gun.dm`
//! overrides `/obj/machinery/deployable_turret/hmg`).
//!
//! - A mob dragged onto a turret is buckled ([`crate::furniture::can_buckle`]): it drops what it
//!   holds, the turret is anchored and faces south (`mechmove01`). Unbuckled (resist), the turret
//!   comes loose again unless `always_anchored`.
//! - The gunner's empty-hand clicks, near or far, aim the turret (`gun_control`): the turret and
//!   the gunner turn to the target, and off cooldown a volley of `number_of_shots` projectiles
//!   leaves `rate_of_fire` deciseconds apart (`volley`, `fire_helper`), each at the last clicked
//!   target (its tile when it is gone). Another volley waits `cooldown_duration`; a click while
//!   the barrel cools after the burst plays the overheat sound once. The infinity gun and the
//!   full-auto CM90 fire one shot per click (their `checkfire`). A shot needs the gunner still
//!   buckled and not incapacitated.
//! - A wrench on an undeployable turret (the HMG) takes 3 s from a human and leaves the folded
//!   gun in their hands; the folded gun used in hand deploys in front of the user after 5 s
//!   (the tile must be free, `is_blocked_turf`), facing the user's direction.
// shortcut: no mouse tracking between clicks (tg `update_positioning` follows the cursor every
// tick), no gunner pixel offset or layer swap per direction, no view range change, no
// abstract `gun_control` items in the hands; integrity is not carried over to the folded gun
// (both are 250 in tg and the fork).

use bevy::prelude::*;
use ss13_mapload::ProjectileData;
use ss13_shared::chat::Span;
use ss13_shared::{Facing, TilePos};

use crate::construction::Job;
use crate::medical::surgery::Buckled;
use crate::obj_damage::{is_under, set_anchored};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, ToolKind, do_after, tool_of};
use crate::zone::BodyZone;

pub const TURRET_ROOT: &str = "/obj/machinery/deployable_turret";
pub const ULTIMATE: &str = "/obj/machinery/deployable_turret/ultimate";
pub const HMG: &str = "/obj/machinery/deployable_turret/hmg";
pub const HMG_FULL_AUTO: &str = "/obj/machinery/deployable_turret/hmg/full_auto";
pub const FOLDED_HMG: &str = "/obj/item/deployable_turret_folded";
pub const FOLDED_HMG_FULL_AUTO: &str = "/obj/item/deployable_turret_folded/full_auto";
/// `/obj/projectile/bullet/manned_turret`.
pub const TURRET_BULLET: &str = "/obj/projectile/bullet/manned_turret";
/// `/obj/projectile/beam/laser/flare` of the fork's CM90.
pub const FLARE: &str = "/obj/projectile/beam/laser/flare";

const MECHMOVE: &str = "sound/vehicles/mecha/mechmove01.ogg";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";
const SMG_SHOT: &str = "sound/items/weapons/gun/smg/shot.ogg";
const SEAR: &str = "sound/items/weapons/sear.ogg";
const HMG_SHOT: &str = "sound/items/weapons/gun/hmg/hmg.ogg";
const SMG_RACK: &str = "sound/items/weapons/gun/smg/smgrack.ogg";
/// `undeploy_time`, deciseconds.
const UNDEPLOY_DS: f32 = 30.0;
/// The folded gun's `deploy_time`, deciseconds.
const DEPLOY_DS: f32 = 50.0;

/// A deployable turret type's vars. Times in deciseconds.
#[derive(Clone, Debug, PartialEq)]
pub struct TurretDef {
    pub projectile: ProjectileData,
    pub number_of_shots: u32,
    pub rate_of_fire: f32,
    pub cooldown: f32,
    /// One shot per click, no cooldown (`ultimate`, the full-auto `checkfire`).
    pub per_click: bool,
    pub fire_sound: &'static str,
    pub overheat_sound: &'static str,
    /// `spawned_on_undeploy` when `can_be_undeployed`.
    pub undeploy_to: Option<&'static str>,
    pub always_anchored: bool,
}

/// `/obj/projectile/bullet/manned_turret` with the `/obj/projectile/bullet` defaults.
pub fn turret_bullet() -> ProjectileData {
    ProjectileData {
        path: TURRET_BULLET.to_owned(),
        damage: 20.0,
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
        wound_bonus: 0.0,
        wound_falloff_tile: -5.0,
        // `SHARP_POINTY`.
        sharpness: 2,
        ..default()
    }
}

/// `/obj/projectile/beam/laser/flare` with the `/obj/projectile/beam` defaults.
pub fn flare() -> ProjectileData {
    ProjectileData {
        path: FLARE.to_owned(),
        damage: 20.0,
        damage_type: "burn".into(),
        armor_flag: "laser".into(),
        speed: 1.25,
        range: 50.0,
        pass_flags: crate::projectile::PASSTABLE
            | crate::projectile::PASSGLASS
            | crate::projectile::PASSGRILLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ricochets_max: 50,
        ricochet_chance: 80.0,
        ricochet_decay_chance: 0.7,
        ricochet_decay_damage: 0.7,
        ricochet_incidence_leeway: 40.0,
        wound_bonus: -15.0,
        exposed_wound_bonus: 15.0,
        wound_falloff_tile: -5.0,
        reflectable: true,
        ..default()
    }
}

/// The [`TurretDef`] of `path`, `None` for anything but a deployable turret.
pub fn def(path: &str) -> Option<TurretDef> {
    if !is_under(path, TURRET_ROOT) {
        return None;
    }
    let mut d = TurretDef {
        projectile: turret_bullet(),
        number_of_shots: 40,
        rate_of_fire: 1.0,
        cooldown: 90.0,
        per_click: false,
        fire_sound: SMG_SHOT,
        overheat_sound: SEAR,
        undeploy_to: None,
        always_anchored: false,
    };
    if is_under(path, ULTIMATE) {
        d.per_click = true;
    } else if is_under(path, HMG) {
        // The fork's CM90 over tg's HMG: a laser, 4 shots every 1 s.
        d.projectile = flare();
        d.number_of_shots = 4;
        d.rate_of_fire = 2.0;
        d.cooldown = 10.0;
        d.fire_sound = HMG_SHOT;
        d.overheat_sound = SMG_RACK;
        d.undeploy_to = Some(FOLDED_HMG);
        if is_under(path, HMG_FULL_AUTO) {
            d.number_of_shots = 1;
            d.rate_of_fire = 1.0;
            d.per_click = true;
            // The fork's `spawned_on_undeploy` names the turret itself (a typo): the folded
            // full-auto gun is meant.
            d.undeploy_to = Some(FOLDED_HMG_FULL_AUTO);
        }
    }
    Some(d)
}

/// The turret a folded gun deploys into (`/datum/component/deployable`).
pub fn deploys_into(path: &str) -> Option<&'static str> {
    if is_under(path, FOLDED_HMG_FULL_AUTO) {
        Some(HMG_FULL_AUTO)
    } else if is_under(path, FOLDED_HMG) {
        Some(HMG)
    } else {
        None
    }
}

/// A turret's firing state (tg `cooldown`, `warned`, `target`, `target_turf` and the volley's
/// timers).
#[derive(Component, Clone, Debug, Default)]
pub struct TurretState {
    /// `cooldown`: `Time::elapsed_secs_f64` the next volley may start.
    pub ready_at: f64,
    pub warned: bool,
    pub target: Option<Entity>,
    pub target_pos: Option<TilePos>,
    pub pixel: [i16; 2],
    /// The volley's shot times and its gunner.
    pub shots: Vec<f64>,
    pub gunner: Option<Entity>,
}

/// The timed steps of this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurretJob {
    /// A wrench packs the target turret up.
    Undeploy,
    /// The folded gun `item` unfolds onto `at`, facing `dir`.
    Deploy {
        item: Entity,
        at: TilePos,
        dir: ss13_shared::Dir,
    },
}

pub struct DeployableTurretsPlugin;

impl Plugin for DeployableTurretsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 9, |w, c| {
            gunner_click(w, c.mob, c.target, c.pos, c.modifiers)
        });
        app.add_click_handler(ClickPhase::AttackSelf, 3269, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 59, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_systems(
            Update,
            (ranged_clicks, volleys)
                .chain()
                .after(crate::click::ResolveClicks)
                .before(crate::projectile::ProjectileSystems),
        );
    }
}

use crate::tools::now;

fn def_of(world: &World, e: Entity) -> Option<TurretDef> {
    def(&crate::construction::path_of(world, e)?)
}

/// The turret `mob` mans, if any.
pub fn manned(world: &World, mob: Entity) -> Option<Entity> {
    let turret = world.get::<Buckled>(mob)?.0;
    def_of(world, turret).map(|_| turret)
}

/// `user_buckle_mob` after the buckle: hands emptied, anchored, facing south.
pub(crate) fn post_buckle(world: &mut World, actor: Actor, mob: Entity, turret: Entity) {
    let Some(&pos) = world.get::<TilePos>(turret) else {
        return;
    };
    let (user, target, what) = (
        crate::medical::name_of(world, actor.mob),
        crate::medical::name_of(world, mob),
        crate::medical::name_of(world, turret),
    );
    let (own, others) = if mob == actor.mob {
        (
            format!("Вы встаёте за {what}."),
            format!("{user} встаёт за {what}."),
        )
    } else {
        (
            format!("Вы ставите {target} за {what}."),
            format!("{user} ставит {target} за {what}."),
        )
    };
    crate::medical::visible(world, actor.mob, &own, &others, Span::Notice);
    let held: Vec<Entity> = world
        .get::<crate::inventory::Inventory>(mob)
        .map(|inv| inv.hands.iter().flatten().copied().collect())
        .unwrap_or_default();
    for item in held {
        if !crate::inventory::no_drop(world, item) {
            crate::inventory::put_down(world, item, pos);
        }
    }
    set_facing(world, turret, ss13_shared::Dir::South);
    set_anchored(world, turret, true);
    playsound(world, PlaySound::new(pos, MECHMOVE, 50.0, true));
}

/// `unbuckle_mob`: loose again unless `always_anchored`, the volley stops.
pub(crate) fn post_unbuckle(world: &mut World, turret: Entity) {
    let Some(d) = def_of(world, turret) else {
        return;
    };
    if let Some(&pos) = world.get::<TilePos>(turret) {
        playsound(world, PlaySound::new(pos, MECHMOVE, 50.0, true));
    }
    if !d.always_anchored {
        set_anchored(world, turret, false);
    }
    if let Some(mut s) = world.get_mut::<TurretState>(turret) {
        s.shots.clear();
        s.gunner = None;
    }
}

fn set_facing(world: &mut World, e: Entity, dir: ss13_shared::Dir) {
    world.entity_mut(e).insert(Facing(dir));
}

fn gunner_click(
    world: &mut World,
    mob: Entity,
    target: Entity,
    target_pos: TilePos,
    modifiers: ss13_shared::protocol::ClickModifiers,
) -> bool {
    let Some(turret) = manned(world, mob) else {
        return false;
    };
    let max = crate::projectile::MAX_AIM_PIXEL;
    let pixel = modifiers
        .pixel
        .map_or([0, 0], |p| p.map(|v| i16::from(v).clamp(-max, max)));
    checkfire(world, turret, mob, target, target_pos, pixel)
}

fn ranged_clicks(
    world: &mut World,
    mut clicks: Local<bevy::ecs::message::MessageCursor<crate::click::RangedClick>>,
) {
    let clicks: Vec<crate::click::RangedClick> = clicks
        .read(world.resource::<Messages<crate::click::RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let empty = world
            .get::<crate::inventory::Inventory>(click.mob)
            .is_some_and(|inv| inv.active_item().is_none());
        if empty {
            gunner_click(
                world,
                click.mob,
                click.target,
                click.target_pos,
                click.modifiers,
            );
        }
    }
}

/// tg `direction_track` + `checkfire`. False (the click goes on) for the turret's own tile.
pub fn checkfire(
    world: &mut World,
    turret: Entity,
    gunner: Entity,
    target: Entity,
    target_pos: TilePos,
    pixel: [i16; 2],
) -> bool {
    let (Some(d), Some(&pos)) = (def_of(world, turret), world.get::<TilePos>(turret)) else {
        return false;
    };
    if target == gunner || target_pos == pos {
        return false;
    }
    if crate::martial::incapacitated(world, gunner) {
        return true;
    }
    if let Some(dir) = crate::pull::dir_between(pos, target_pos) {
        set_facing(world, turret, dir);
        set_facing(world, gunner, dir);
    }
    let t = now(world);
    if world.get::<TurretState>(turret).is_none() {
        world.entity_mut(turret).insert(TurretState::default());
    }
    let mut s = world
        .get_mut::<TurretState>(turret)
        .expect("inserted above");
    s.target = Some(target);
    s.target_pos = Some(target_pos);
    s.pixel = pixel;
    s.gunner = Some(gunner);
    if d.per_click {
        s.shots.push(t);
        return true;
    }
    if t < s.ready_at {
        // The window after the burst is done and before the barrel is ready.
        let burst_end = s.ready_at - f64::from(d.cooldown) / 10.0
            + f64::from(d.rate_of_fire * d.number_of_shots as f32) / 10.0;
        if !s.warned && t > burst_end {
            s.warned = true;
            playsound(world, PlaySound::new(pos, d.overheat_sound, 100.0, true));
        }
        return true;
    }
    s.ready_at = t + f64::from(d.cooldown) / 10.0;
    s.warned = false;
    s.shots = (1..=d.number_of_shots)
        .map(|i| t + f64::from(d.rate_of_fire * i as f32) / 10.0)
        .collect();
    true
}

/// The volleys' timers (`fire_helper`).
fn volleys(world: &mut World) {
    let t = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &TurretState)>()
        .iter(world)
        .filter(|(_, s)| s.shots.iter().any(|&at| at <= t))
        .map(|(e, _)| e)
        .collect();
    for turret in due {
        let n = {
            let mut s = world.get_mut::<TurretState>(turret).expect("queried");
            let before = s.shots.len();
            s.shots.retain(|&at| at > t);
            before - s.shots.len()
        };
        for _ in 0..n {
            fire_helper(world, turret);
        }
    }
}

/// tg `fire_helper`: one shot at the target (its tile when it is gone).
pub fn fire_helper(world: &mut World, turret: Entity) {
    let (Some(d), Some(&from), Some(s)) = (
        def_of(world, turret),
        world.get::<TilePos>(turret),
        world.get::<TurretState>(turret).cloned(),
    ) else {
        return;
    };
    let Some(gunner) = s.gunner else {
        return;
    };
    if world.get::<Buckled>(gunner).map(|b| b.0) != Some(turret)
        || crate::martial::incapacitated(world, gunner)
    {
        return;
    }
    let target = s.target.filter(|&e| world.get_entity(e).is_ok());
    let target_pos = target
        .and_then(|e| world.get::<TilePos>(e).copied())
        .or(s.target_pos);
    let Some(target_pos) = target_pos else {
        return;
    };
    playsound(world, PlaySound::new(from, d.fire_sound, 75.0, true));
    crate::projectile::fire(
        world,
        crate::projectile::Shot {
            data: &d.projectile,
            firer: gunner,
            from,
            target,
            target_pos,
            zone: BodyZone::Chest,
            hit_prone: true,
            pixel: s.pixel,
            deviation: 0.0,
            fired_from: Some(turret),
        },
    );
}

fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<crate::human::HumanLook>(mob).is_some()
}

/// tg `wrench_act`: a human packs an undeployable turret up.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(d) = def_of(world, target) else {
        return false;
    };
    if d.undeploy_to.is_none()
        || !is_human(world, mob)
        || !matches!(tool_of(world, held), Some((ToolKind::Wrench, _)))
    {
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(target) {
        playsound(world, PlaySound::new(pos, RATCHET, 50.0, true));
    }
    crate::balloon::balloon_alert(world, mob, mob, "разборка...");
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    do_after(
        world,
        actor,
        target,
        UNDEPLOY_DS,
        0,
        Job::DeployableTurret(TurretJob::Undeploy),
    );
    true
}

/// The folded gun's `attack_self` (`/datum/component/deployable/deploy`).
fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(_) = crate::construction::path_of(world, item).and_then(|p| deploys_into(&p)) else {
        return false;
    };
    let (Some(&pos), dir) = (
        world.get::<TilePos>(mob),
        world
            .get::<Facing>(mob)
            .map_or(ss13_shared::Dir::South, |f| f.0),
    ) else {
        return true;
    };
    let (dx, dy) = dir.offset();
    let at = TilePos {
        x: pos.x + dx,
        y: pos.y + dy,
        ..pos
    };
    if crate::construction::tile_blocked(world, at) {
        crate::balloon::balloon_alert(world, mob, mob, "недостаточно места для установки здесь");
        return true;
    }
    crate::balloon::balloon_alert(world, mob, mob, "установка...");
    playsound(world, PlaySound::new(pos, RATCHET, 50.0, true));
    let actor = Actor {
        client,
        mob,
        item: Some(item),
    };
    do_after(
        world,
        actor,
        mob,
        DEPLOY_DS,
        0,
        Job::DeployableTurret(TurretJob::Deploy { item, at, dir }),
    );
    true
}

/// [`TurretJob`] at the end of its `do_after`.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: TurretJob) {
    match job {
        TurretJob::Undeploy => {
            let turret = action.target;
            let Some(to) = def_of(world, turret).and_then(|d| d.undeploy_to) else {
                return;
            };
            let Some(&pos) = world.get::<TilePos>(turret) else {
                return;
            };
            if let Some(gunner) = crate::medical::surgery::buckled_to(world, turret) {
                let actor = Actor {
                    client: action.client,
                    mob: gunner,
                    item: None,
                };
                crate::medical::surgery::unbuckle(world, actor, gunner);
            }
            world.despawn(turret);
            let Some(folded) = crate::construction::spawn_obj(world, to, pos) else {
                return;
            };
            if let Some(inventory) = world.get::<crate::inventory::Inventory>(mob).cloned() {
                crate::storage::put_in_hands(world, mob, &inventory, folded);
            }
        }
        TurretJob::Deploy { item, at, dir } => {
            // Revalidated after the wait: the gun still held, the tile still free.
            let held = world
                .get::<crate::inventory::Inventory>(mob)
                .is_some_and(|inv| inv.holds(item));
            let Some(into) =
                crate::construction::path_of(world, item).and_then(|p| deploys_into(&p))
            else {
                return;
            };
            if !held || crate::construction::tile_blocked(world, at) {
                return;
            }
            world.despawn(item);
            if let Some(turret) = crate::construction::spawn_obj(world, into, at) {
                set_facing(world, turret, dir);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defs_follow_tg_and_the_fork() {
        let base = def(TURRET_ROOT).unwrap();
        assert_eq!(
            (base.number_of_shots, base.rate_of_fire, base.cooldown),
            (40, 1.0, 90.0)
        );
        assert_eq!(base.projectile.damage, 20.0);
        assert!(base.undeploy_to.is_none() && !base.per_click);
        assert!(def(ULTIMATE).unwrap().per_click);
        let hmg = def(HMG).unwrap();
        assert_eq!((hmg.number_of_shots, hmg.cooldown), (4, 10.0));
        assert_eq!(hmg.projectile.path, FLARE);
        assert_eq!(hmg.undeploy_to, Some(FOLDED_HMG));
        let auto = def(HMG_FULL_AUTO).unwrap();
        assert!(auto.per_click);
        assert_eq!(auto.undeploy_to, Some(FOLDED_HMG_FULL_AUTO));
        assert!(def("/obj/machinery/porta_turret").is_none());
        assert_eq!(deploys_into(FOLDED_HMG), Some(HMG));
        assert_eq!(deploys_into(FOLDED_HMG_FULL_AUTO), Some(HMG_FULL_AUTO));
        assert_eq!(deploys_into("/obj/item/wrench"), None);
    }

    fn world_with_turret(path: &str) -> (World, Entity, Entity) {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let turret = world
            .spawn((
                ss13_mapload::DmObj {
                    path: path.to_owned(),
                    ..default()
                },
                TilePos::new(5, 5, 1),
            ))
            .id();
        let gunner = world
            .spawn((
                TilePos::new(5, 5, 1),
                Buckled(turret),
                crate::health::Stat::Conscious,
            ))
            .id();
        (world, turret, gunner)
    }

    #[test]
    fn a_volley_then_a_cooldown() {
        let (mut world, turret, gunner) = world_with_turret(HMG);
        let target = world.spawn(TilePos::new(5, 9, 1)).id();
        assert!(checkfire(
            &mut world,
            turret,
            gunner,
            target,
            TilePos::new(5, 9, 1),
            [0, 0]
        ));
        let s = world.get::<TurretState>(turret).unwrap().clone();
        assert_eq!(s.shots.len(), 4);
        assert!((s.shots[3] - 0.8).abs() < 1e-9);
        assert!((s.ready_at - 1.0).abs() < 1e-9);
        assert_eq!(
            world.get::<Facing>(turret).map(|f| f.0),
            Some(ss13_shared::Dir::North)
        );
        // A second click on cooldown starts nothing new but retargets.
        let other = world.spawn(TilePos::new(9, 5, 1)).id();
        checkfire(
            &mut world,
            turret,
            gunner,
            other,
            TilePos::new(9, 5, 1),
            [0, 0],
        );
        let s = world.get::<TurretState>(turret).unwrap();
        assert_eq!(s.shots.len(), 4);
        assert_eq!(s.target, Some(other));
        // Its own tile is not a target: the click goes on.
        assert!(!checkfire(
            &mut world,
            turret,
            gunner,
            turret,
            TilePos::new(5, 5, 1),
            [0, 0]
        ));
    }

    #[test]
    fn full_auto_fires_once_per_click() {
        let (mut world, turret, gunner) = world_with_turret(HMG_FULL_AUTO);
        let target = world.spawn(TilePos::new(1, 5, 1)).id();
        for _ in 0..3 {
            checkfire(
                &mut world,
                turret,
                gunner,
                target,
                TilePos::new(1, 5, 1),
                [0, 0],
            );
        }
        assert_eq!(world.get::<TurretState>(turret).unwrap().shots.len(), 3);
    }
}
