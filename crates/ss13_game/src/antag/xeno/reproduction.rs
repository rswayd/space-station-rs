//! Facehuggers, embryos, weeds, eggs and nests (`facehugger.dm`, `alien_embryo.dm`, `aliens.dm`,
//! `alien_nest.dm`).
//!
//! - A [`FaceHugger`] lies on the floor awake and leaps at a huggable mob stepping on it; awake it
//!   leaps at whoever picks it up, an alien may carry it and a click puts it on a face. On the
//!   face it latches (brute, paralysis, a knockdown) and after 10-15 s implants an [`AlienEmbryo`]
//!   and dies. A helmet covering the mouth kills it.
//! - An [`AlienEmbryo`] grows a stage a minute (spaceacillin doubles it, a nest cuts 20%), with
//!   symptoms from stage 3; at stage 6 it polls ghosts ([`attempt_grow`]) and gibs the host with
//!   a larva. No candidates: back to stage 5 for a minute. Aliens see the hosts' stages
//!   (`AddInfectionImages`).
//! - A weed node (`/obj/structure/alien/weeds/node`) grows weeds within 3 tiles every 5-10 s;
//!   weeds of a destroyed node find a new one or die in 2-8 s. Heat burns them.
//! - An [`Egg`] grows a hugger in 90-150 s and bursts when a huggable mob comes next to it (an
//!   already implanted one must be unconscious), when clicked by an alien ("retrieve"), or
//!   killing the hugger when broken.
//! - A nest (`/obj/structure/bed/nest`): only aliens buckle the mob on it, by a click on the nest
//!   with the mob standing there; a captive struggles free in 100 s (`user_unbuckle_mob`).
// shortcut: no worn-state sprites or the `knockoff` after death; no leaping from a throw
// (`throw_impact`) or from a found-in-storage item; the embryo cannot be cut out (no organ item
// for the surgery), emotes of its symptoms are not played, a host in surgery is gibbed anyway;
// no burst overlay or 0.6 s animation; nest sustenance beyond the growth speed; one weed sprite;
// no spawned corpse of the gelpod; eggs and weeds only burn from hot air (no attack sounds).

use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmObj};
use ss13_net::server::Seers;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, LightSource, Optics, Overlay, TilePos};

use super::abilities::{self, XenoJob};
use super::{Caste, XENO_SIGHT, Xeno, make_xeno};
use crate::combat::CombatRng;
use crate::construction::{Job, path_of, spawn_obj, tile_changed};
use crate::ghost_poll::{self, Candidate, PollSpec};
use crate::health::{DamageType, Health, Stat};
use crate::inventory::{CarriedBy, Inventory, NoDrop, put_down, unequip};
use crate::medical::surgery::Buckled;
use crate::medical::{name_of, visible};
use crate::server::PlayerMob;
use crate::status_effects::{Incap, Timed};
use crate::tools::{Actor, do_after, now, tell, tell_mob};

pub const WEED: &str = "/obj/structure/alien/weeds";
pub const WEED_NODE: &str = "/obj/structure/alien/weeds/node";
pub const EGG: &str = "/obj/structure/alien/egg";
pub const EGG_GROWN: &str = "/obj/structure/alien/egg/grown";
pub const EGG_BURST: &str = "/obj/structure/alien/egg/burst";
pub const EGG_FAKE: &str = "/obj/structure/alien/egg/fake";
pub const RESIN_WALL: &str = "/obj/structure/alien/resin/wall";
pub const RESIN_MEMBRANE: &str = "/obj/structure/alien/resin/membrane";
pub const NEST: &str = "/obj/structure/bed/nest";
pub const FACEHUGGER: &str = "/obj/item/clothing/mask/facehugger";
const LAMARR: &str = "/obj/item/clothing/mask/facehugger/lamarr";
const HUGGER_DEAD: &str = "/obj/item/clothing/mask/facehugger/dead";
const HUGGER_IMPREGNATED: &str = "/obj/item/clothing/mask/facehugger/impregnated";
const HUGGER_TOY: &str = "/obj/item/clothing/mask/facehugger/toy";
const ALIEN_DMI: &str = "icons/mob/nonhuman-player/alien.dmi";
/// `HEADCOVERSMOUTH` of `flags_cover`.
const HEADCOVERSMOUTH: u32 = 1 << 4;
/// `strength`.
const HUGGER_STRENGTH: f32 = 5.0;
/// `MIN_/MAX_IMPREGNATION_TIME`, `MIN_/MAX_ACTIVE_TIME`, seconds.
const IMPREGNATE_SECS: (u32, u32) = (10, 15);
const ACTIVE_SECS: (u32, u32) = (20, 40);
/// `growth_time`.
const GROWTH_SECS: f64 = 60.0;
/// `MIN_/MAX_GROWTH_TIME`, seconds.
const EGG_GROWTH_SECS: (u32, u32) = (90, 150);
/// `Burst`'s timer, seconds.
const EGG_BURST_SECS: f64 = 1.5;
/// `NODERANGE`; node `minimum_/maximum_growtime`; the wait to die alone, seconds.
const NODE_RANGE: u32 = 3;
const NODE_GROWTH_SECS: (u32, u32) = (5, 10);
const WEED_DEATH_SECS: (u32, u32) = (2, 8);
/// The nest's self-unbuckle `do_after`, deciseconds.
const NEST_ESCAPE_DS: f32 = 1000.0;
/// `POLL_IGNORE_ALIEN_LARVA`.
pub const POLL_IGNORE_ALIEN_LARVA: &str = "alien_larva";
const ALIEN_POLL_SECS: f64 = 20.0;
const HUGGER_TICK: f64 = 0.25;
const WORLD_TICK: f64 = 1.0;
/// `should_atmos_process` of huggers and weeds, of eggs.
const WARM_AIR: f64 = 300.0;
const HOT_AIR: f64 = 500.0;
/// `LIGHT_COLOR_BLUE`.
const NODE_LIGHT: [u8; 3] = [100, 150, 250];

/// `facehugger_state` and how it was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HuggerState {
    Awake,
    Asleep,
    Dead,
}

/// A facehugger.
#[derive(Component, Clone, Copy, Debug)]
pub struct FaceHugger {
    pub state: HuggerState,
    pub sterile: bool,
    /// Not a toy (`real`).
    pub real: bool,
    /// `go_active`'s time.
    awake_at: f64,
    /// `impregnate_target`'s `target` and time: latched to this mob until then.
    victim: Option<(Entity, f64)>,
}

/// An `/obj/item/organ/body_egg/alien_embryo` in a host (`TRAIT_XENO_HOST`).
#[derive(Component, Clone, Copy, Debug)]
pub struct AlienEmbryo {
    /// `stage`, 1 to 6.
    pub stage: u8,
    /// When the next stage comes.
    next_at: f64,
    /// A poll is running (`bursting`).
    bursting: bool,
}

/// An `/obj/structure/alien/egg`'s `status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EggStatus {
    Growing,
    Grown,
    Bursting,
    Burst,
    Fake,
}

/// An alien egg.
#[derive(Component, Clone, Copy, Debug)]
pub struct Egg {
    pub status: EggStatus,
    grow_at: f64,
    burst_at: f64,
    kill: bool,
}

/// A weed tile (`parent_node`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Weed {
    pub node: Option<Entity>,
    dying_at: Option<f64>,
}

/// A weed node's growth clock.
#[derive(Component, Clone, Copy, Debug)]
pub struct WeedNode {
    grow_at: f64,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_observer(add_hugger)
        .add_observer(add_structure)
        .add_systems(
            Update,
            (
                hugger_tick,
                embryo_tick,
                egg_tick,
                weed_tick,
                abilities::corrosion_tick,
            )
                .after(crate::click::ResolveClicks)
                .before(crate::health::HealthSystems),
        )
        .add_click_handler(ClickPhase::EmptyHand, 3491, |w, c| empty_hand(w, c))
        .add_click_handler(ClickPhase::HeldOnTarget, 4501, |w, c| held_on_target(w, c));
}

fn roll(world: &mut World, range: (u32, u32)) -> f64 {
    f64::from(world.resource_mut::<CombatRng>().range(range.0, range.1))
}

// ---------------------------------------------------------------------------------------------
// Facehuggers

fn add_hugger(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let entity = add.entity;
    let Ok(item) = items.get(entity) else {
        return;
    };
    if !is_under(&item.path, FACEHUGGER) {
        return;
    }
    let dead = is_under(&item.path, HUGGER_DEAD) || is_under(&item.path, HUGGER_IMPREGNATED);
    let toy = is_under(&item.path, HUGGER_TOY);
    commands.entity(entity).insert(FaceHugger {
        state: if dead {
            HuggerState::Dead
        } else {
            HuggerState::Awake
        },
        sterile: toy || is_under(&item.path, LAMARR),
        real: !toy,
        awake_at: 0.0,
        victim: None,
    });
}

/// `CanHug`: a living human without a hive node (an alien), a mask slot, an uncovered mouth.
pub fn can_hug(world: &World, mob: Entity) -> bool {
    if world.get::<Stat>(mob).is_none_or(|s| *s == Stat::Dead) {
        return false;
    }
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return false;
    };
    crate::species::can_wear(world, mob, crate::inventory::slot_bit(InventorySlot::Mask))
        && !head_covers_mouth(world, inventory)
}

fn head_covers_mouth(world: &World, inventory: &Inventory) -> bool {
    inventory
        .slot(InventorySlot::Head)
        .and_then(|h| world.get::<ss13_mapload::DmClothing>(h))
        .is_some_and(|c| c.flags_cover & HEADCOVERSMOUTH != 0)
}

/// `valid_to_attach` of the hugger on `mob`: a human with a head, not already a host.
fn valid_to_attach(world: &World, hugger: &FaceHugger, mob: Entity) -> bool {
    hugger.state == HuggerState::Awake
        && hugger.victim.is_none()
        && world.get::<Inventory>(mob).is_some()
        && world.get::<AlienEmbryo>(mob).is_none()
        && world.get::<Xeno>(mob).is_none()
        && world
            .get::<Health>(mob)
            .is_some_and(|h| h.has_part(crate::zone::BodyZone::Head))
}

/// `leap_to`: the hugger goes on `target`'s face (or dies against a helmet). False when it did
/// not attach.
pub fn leap_to(world: &mut World, hugger: Entity, target: Entity) -> bool {
    let Some(h) = world.get::<FaceHugger>(hugger).copied() else {
        return false;
    };
    if !valid_to_attach(world, &h, target) {
        return false;
    }
    let Some(inventory) = world.get::<Inventory>(target).cloned() else {
        return false;
    };
    if inventory
        .slot(InventorySlot::Mask)
        .is_some_and(|m| world.get::<FaceHugger>(m).is_some())
    {
        return false;
    }
    let (user, victim) = (name_of(world, hugger), name_of(world, target));
    let pos = crate::explosions::turf_of(world, target);
    visible(
        world,
        target,
        &format!("{user} прыгает вам на лицо!"),
        &format!("{user} прыгает на лицо {victim}!"),
        Span::Danger,
    );
    if head_covers_mouth(world, &inventory) {
        let helmet = inventory.slot(InventorySlot::Head).unwrap();
        let what = name_of(world, helmet);
        visible(
            world,
            target,
            &format!("{user} разбивается о ваш {what}!"),
            &format!("{user} разбивается о {what} {victim}!"),
            Span::Danger,
        );
        die(world, hugger);
        return false;
    }
    if let Some(worn) = inventory.slot(InventorySlot::Mask)
        && let Some(pos) = pos
    {
        unequip(world, target, pos, InventorySlot::Mask);
        put_down(world, worn, pos);
        let what = name_of(world, worn);
        visible(
            world,
            target,
            &format!("{user} срывает {what} с вашего лица!"),
            &format!("{user} срывает {what} с лица {victim}!"),
            Span::Danger,
        );
    }
    // `equip_to_slot_if_possible(.., bypass_equip_delay_self = 1, 1)`.
    let tile = world.get::<TilePos>(hugger).copied();
    world
        .entity_mut(hugger)
        .remove::<TilePos>()
        .insert(CarriedBy(target));
    if let Some(mut inv) = world.get_mut::<Inventory>(target) {
        *inv.slot_mut(InventorySlot::Mask) = Some(hugger);
    }
    if let Some(tile) = tile {
        tile_changed(world, tile);
    }
    crate::inventory::refresh_look(world, target);
    attach_to_victim(world, hugger, target);
    true
}

/// `attach_to_victim`: the jump's effects and the start of the implanting.
fn attach_to_victim(world: &mut World, hugger: Entity, victim: Entity) {
    let Some(h) = world.get::<FaceHugger>(hugger).copied() else {
        return;
    };
    if h.sterile {
        return;
    }
    let zone = [crate::zone::BodyZone::Head, crate::zone::BodyZone::Chest]
        [world.resource_mut::<CombatRng>().range(0, 1) as usize];
    crate::bots::hurt_zone(world, victim, zone, DamageType::Brute, HUGGER_STRENGTH);
    if h.real {
        crate::status_effects::apply(world, victim, Incap::Paralyze, 1.0);
        crate::status_effects::adjust_timed(world, victim, Timed::Confusion, 20.0, None);
        crate::status_effects::knockdown(world, victim, 10.0, 0.0);
    }
    let t = now(world);
    let impregnate = t + roll(world, IMPREGNATE_SECS);
    let awake = t + roll(world, ACTIVE_SECS);
    world.entity_mut(hugger).insert(NoDrop);
    if let Some(mut h) = world.get_mut::<FaceHugger>(hugger) {
        h.victim = Some((victim, impregnate));
        go_idle(&mut h, awake);
    }
    let (user, name) = (name_of(world, hugger), name_of(world, victim));
    visible(
        world,
        victim,
        &format!("{user} цепляется вам на лицо, обхватывая голову!"),
        &format!("{user} цепляется на лицо {name}!"),
        Span::Danger,
    );
}

/// `go_idle`, the time it wakes up.
fn go_idle(h: &mut FaceHugger, awake_at: f64) {
    if h.state != HuggerState::Dead {
        h.state = HuggerState::Asleep;
        h.awake_at = awake_at;
    }
}

/// `die`: the last spasm, the dead sprite, it comes off a face.
pub fn die(world: &mut World, hugger: Entity) {
    let Some(mut h) = world.get_mut::<FaceHugger>(hugger) else {
        return;
    };
    if h.state == HuggerState::Dead || !h.real {
        return;
    }
    h.state = HuggerState::Dead;
    h.victim = None;
    let name = name_of(world, hugger);
    world.entity_mut(hugger).remove::<NoDrop>();
    crate::mobs::set_icon_state(world, hugger, ALIEN_DMI, "facehugger_dead");
    let text = format!("{name} сворачивается в клубок!");
    visible(world, hugger, &text, &text, Span::Danger);
}

/// `impregnate_target`: if it still holds the face of the living victim.
fn impregnate(world: &mut World, hugger: Entity, victim: Entity) {
    if let Some(mut h) = world.get_mut::<FaceHugger>(hugger) {
        h.victim = None;
    }
    let held = world
        .get::<Inventory>(victim)
        .and_then(|i| i.slot(InventorySlot::Mask))
        == Some(hugger);
    if !held || world.get::<Stat>(victim).is_none_or(|s| *s == Stat::Dead) {
        return;
    }
    let h = world.get::<FaceHugger>(hugger).copied();
    let (user, name) = (name_of(world, hugger), name_of(world, victim));
    if h.is_some_and(|h| h.sterile) {
        visible(
            world,
            victim,
            &format!("{user} тычется в ваше лицо!"),
            &format!("{user} тычется в лицо {name}!"),
            Span::Danger,
        );
        return;
    }
    visible(
        world,
        victim,
        &format!("{user} безвольно обмякает на вашем лице!"),
        &format!("{user} безвольно обмякает на лице {name}!"),
        Span::Danger,
    );
    die(world, hugger);
    crate::mobs::set_icon_state(world, hugger, ALIEN_DMI, "facehugger_impregnated");
    if !robotic_chest(world, victim) && world.get::<AlienEmbryo>(victim).is_none() {
        implant(world, victim);
        info!("{victim} was impregnated by a facehugger");
    }
}

fn robotic_chest(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::medical::wounds::Limbs>(mob)
        .is_some_and(|l| l.limb(crate::zone::BodyZone::Chest).robotic)
}

/// `new /obj/item/organ/body_egg/alien_embryo(target)`.
pub fn implant(world: &mut World, host: Entity) {
    let t = now(world);
    world.entity_mut(host).insert(AlienEmbryo {
        stage: 0,
        next_at: 0.0,
        bursting: false,
    });
    advance(world, host, t);
}

/// `hugger_tick`: timers and proximity.
fn hugger_tick(world: &mut World, mut clock: Local<f64>) {
    let t = now(world);
    if t < *clock {
        return;
    }
    *clock = t + HUGGER_TICK;
    let huggers: Vec<(Entity, FaceHugger, Option<TilePos>)> = world
        .query::<(Entity, &FaceHugger, Option<&TilePos>)>()
        .iter(world)
        .map(|(e, h, p)| (e, *h, p.copied()))
        .collect();
    for (hugger, h, pos) in huggers {
        if let Some((victim, at)) = h.victim
            && t >= at
        {
            impregnate(world, hugger, victim);
            continue;
        }
        if h.state == HuggerState::Asleep && t >= h.awake_at {
            let mut woke = false;
            if let Some(mut h) = world.get_mut::<FaceHugger>(hugger) {
                if h.state == HuggerState::Asleep {
                    h.state = HuggerState::Awake;
                    h.awake_at = 0.0;
                    woke = true;
                }
            }
            if woke {
                crate::mobs::set_icon_state(world, hugger, ALIEN_DMI, "facehugger");
            }
        }
        let Some(pos) = pos else {
            continue;
        };
        if h.state == HuggerState::Awake {
            if air_above(world, pos, WARM_AIR) {
                die(world, hugger);
                continue;
            }
            // The tile's connect_loc `on_entered`, as a standing check.
            let on_tile: Vec<Entity> = world
                .resource::<ss13_world::SpatialIndex>()
                .on_tile(pos)
                .to_vec();
            for mob in on_tile {
                if can_hug(world, mob) && leap_to(world, hugger, mob) {
                    break;
                }
            }
        }
    }
}

/// tg `exposed_temperature > limit` on the tile.
fn air_above(world: &World, pos: TilePos, limit: f64) -> bool {
    world
        .get_resource::<Atmos>()
        .and_then(|a| a.air(pos))
        .is_some_and(|air| air.temperature > limit)
}

/// `attack_hand` of a human on an awake hugger: it jumps first.
fn empty_hand(world: &mut World, ctx: &crate::click_handlers::ClickCtx) -> bool {
    let Some(h) = world.get::<FaceHugger>(ctx.target).copied() else {
        return false;
    };
    h.state == HuggerState::Awake && !h.sterile && leap_to(world, ctx.target, ctx.mob)
}

/// `attack` of a hugger held by a human: it is dropped on the target's tile and leaps.
fn held_on_target(world: &mut World, ctx: &crate::click_handlers::ClickCtx) -> bool {
    let Some(held) = ctx.held else {
        return false;
    };
    if world.get::<FaceHugger>(held).is_none()
        || world.get::<Health>(ctx.target).is_none()
        || world.get::<Inventory>(ctx.target).is_none()
    {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(ctx.target) else {
        return false;
    };
    if let Some(slot) = world
        .get::<Inventory>(ctx.mob)
        .and_then(|i| i.slot_of(held))
    {
        unequip(world, ctx.mob, ctx.mob_pos, slot);
    }
    put_down(world, held, pos);
    leap_to(world, held, ctx.target);
    true
}

/// Drops a carried hugger on `pos`.
pub(super) fn drop_hugger(world: &mut World, hugger: Entity, pos: TilePos) {
    world.entity_mut(hugger).insert(pos);
    tile_changed(world, pos);
}

/// An alien's held hugger put on a face (`facehugger/attack`).
pub(super) fn hug_with_carried(world: &mut World, mob: Entity, target: Entity) -> bool {
    let Some(hugger) = world
        .get::<Xeno>(mob)
        .and_then(|x| x.carried.first().copied())
    else {
        return false;
    };
    let Some(h) = world.get::<FaceHugger>(hugger).copied() else {
        return false;
    };
    if !valid_to_attach(world, &h, target) {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    if let Some(mut x) = world.get_mut::<Xeno>(mob) {
        x.carried.retain(|&c| c != hugger);
    }
    drop_hugger(world, hugger, pos);
    leap_to(world, hugger, target);
    true
}

// ---------------------------------------------------------------------------------------------
// The embryo

/// `advance_embryo_stage`.
fn advance(world: &mut World, host: Entity, t: f64) {
    let slowdown = slowdown(world, host);
    let Some(mut e) = world.get_mut::<AlienEmbryo>(host) else {
        return;
    };
    if e.stage >= 6 {
        return;
    }
    e.stage += 1;
    e.next_at = if e.stage < 6 {
        t + GROWTH_SECS * slowdown
    } else {
        f64::MAX
    };
}

/// `advance_embryo_stage`'s `slowdown`.
fn slowdown(world: &World, host: Entity) -> f64 {
    let mut s = 1.0;
    if crate::medical::diseases::virus_resistant(world, host) {
        s *= 2.0;
    }
    if world
        .get::<Buckled>(host)
        .is_some_and(|b| path_of(world, b.0).is_some_and(|p| is_under(&p, NEST)))
    {
        s *= 0.8;
    }
    s
}

/// Stage growth, symptoms (`on_life`), the burst (`egg_process`), and the images aliens see.
fn embryo_tick(world: &mut World, mut clock: Local<f64>) {
    let t = now(world);
    if t < *clock {
        return;
    }
    let dt = crate::breath::SSMOBS_WAIT_SECS;
    *clock = t + dt;
    let hosts: Vec<(Entity, AlienEmbryo)> = world
        .query::<(Entity, &AlienEmbryo)>()
        .iter(world)
        .map(|(h, e)| (h, *e))
        .collect();
    for (host, embryo) in hosts {
        let alive = world.get::<Stat>(host).is_some_and(|s| *s != Stat::Dead);
        if t >= embryo.next_at {
            advance(world, host, t);
        }
        if alive {
            symptoms(world, host, embryo.stage, dt as f32);
        }
        refresh_image(world, host, embryo.stage);
        if embryo.stage == 6 && !embryo.bursting && world.resource_mut::<CombatRng>().chance(50.0) {
            attempt_grow(world, host);
        }
    }
}

fn symptoms(world: &mut World, host: Entity, stage: u8, dt: f32) {
    let say = |world: &mut World, percent: f32, text: &str| -> bool {
        let chance =
            crate::medical::wounds::spt_prob(&mut world.resource_mut::<CombatRng>(), percent, dt);
        if chance {
            tell_mob(world, host, Span::Danger, text);
        }
        chance
    };
    match stage {
        3 | 4 => {
            say(world, 1.0, "Ваше горло болит.");
            say(world, 1.0, "Слизь стекает по задней стенке вашего горла.");
        }
        5 => {
            if say(world, 2.0, "Ваши мышцы ломит.")
                && world.resource_mut::<CombatRng>().chance(20.0)
            {
                crate::bots::hurt_zone(
                    world,
                    host,
                    crate::zone::BodyZone::Chest,
                    DamageType::Brute,
                    1.0,
                );
            }
            if say(world, 2.0, "У вас болит живот.")
                && world.resource_mut::<CombatRng>().chance(20.0)
            {
                crate::mobs::defense::hurt(world, host, DamageType::Tox, 1.0, None);
            }
        }
        6 => {
            tell_mob(
                world,
                host,
                Span::Danger,
                "Вы чувствуете, как что-то прорывается наружу из вашей груди...",
            );
            crate::mobs::defense::hurt(world, host, DamageType::Tox, 5.0 * dt, None);
        }
        _ => {}
    }
}

/// `AddInfectionImages`: an `infected[stage]` image on the host, for aliens.
fn refresh_image(world: &mut World, host: Entity, stage: u8) {
    let key = "xeno_infection";
    crate::alt_appearance::remove_alt_appearance(world, host, key);
    let Some(mut look) = world.get::<Appearance>(host).cloned() else {
        return;
    };
    let Some(icon_state) = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(ALIEN_DMI, &format!("infected{stage}")))
    else {
        return;
    };
    look.overlays.push(Overlay {
        icon_state,
        dir: None,
        color: Some([255; 4]),
        pixel_offset: [0, 0],
        plane: None,
        only_dirs: 0,
    });
    let seers = Seers {
        mobs: Vec::new(),
        groups: XENO_SIGHT,
    };
    crate::alt_appearance::add_alt_appearance(world, host, key, seers, look);
}

/// `attempt_grow`: ghosts are asked for the larva.
fn attempt_grow(world: &mut World, host: Entity) {
    if let Some(mut e) = world.get_mut::<AlienEmbryo>(host) {
        e.bursting = true;
    }
    let name = name_of(world, host);
    let group: Vec<Entity> = ghost_poll::ghosts(world)
        .into_iter()
        .filter(|&g| wants_alien(world, g))
        .collect();
    let spec = PollSpec {
        question: format!("Чужой вырывается из тела {name}!"),
        role_name: "личинка чужого".to_owned(),
        secs: ALIEN_POLL_SECS,
        ignore: Some(POLL_IGNORE_ALIEN_LARVA),
        target: Some(host),
        ..default()
    };
    ghost_poll::poll_candidates(world, spec, group, move |world, signed_up| {
        on_poll_concluded(world, host, signed_up);
    });
}

fn wants_alien(world: &World, ghost: Entity) -> bool {
    world
        .get::<PlayerMob>(ghost)
        .and_then(|p| world.get::<crate::antag::dynamic::AntagPreferences>(p.client))
        .is_some_and(|prefs| prefs.wants(super::ROLE_ALIEN))
}

/// `on_poll_concluded`: a larva, or the embryo back a stage for a minute.
fn on_poll_concluded(world: &mut World, host: Entity, mut signed_up: Vec<Candidate>) {
    if world.get::<AlienEmbryo>(host).is_none() {
        return;
    }
    let Some(chosen) = ghost_poll::pick_candidate(world, &mut signed_up) else {
        let t = now(world);
        if let Some(mut e) = world.get_mut::<AlienEmbryo>(host) {
            e.bursting = false;
            e.stage = 5;
            e.next_at = t + GROWTH_SECS;
        }
        return;
    };
    burst(world, host, chosen);
}

/// The second half of `on_poll_concluded`: the larva comes out and the host is gibbed.
fn burst(world: &mut World, host: Entity, chosen: Candidate) {
    let Some(pos) = crate::explosions::turf_of(world, host) else {
        return;
    };
    let Some(larva) = crate::mobs::spawn::spawn_mob(world, super::LARVA, pos) else {
        if let Some(mut e) = world.get_mut::<AlienEmbryo>(host) {
            e.bursting = false;
        }
        return;
    };
    if !crate::medical::brainmob::possess(world, chosen.client, chosen.mob, larva) {
        world.despawn(larva);
        if let Some(mut e) = world.get_mut::<AlienEmbryo>(host) {
            e.bursting = false;
        }
        return;
    }
    world.try_despawn(chosen.mob).ok();
    make_xeno(world, larva, true);
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/mobs/non-humanoids/hiss/hiss5.ogg", 100.0, false),
    );
    let (name, host_name) = (name_of(world, larva), name_of(world, host));
    visible(
        world,
        larva,
        &format!("Вы вырываетесь из тела {host_name}!"),
        &format!("{name} вырывается из тела {host_name} в брызгах крови!"),
        Span::BoldWarning,
    );
    world.entity_mut(host).remove::<AlienEmbryo>();
    crate::alt_appearance::remove_alt_appearance(world, host, "xeno_infection");
    crate::health::gib(world, host);
    info!("{larva} burst out of {host}");
}

// ---------------------------------------------------------------------------------------------
// Weeds and eggs

fn add_structure(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let entity = add.entity;
    let Ok(obj) = objs.get(entity) else {
        return;
    };
    let path = obj.path.clone();
    if !is_under(&path, "/obj/structure/alien") {
        return;
    }
    commands.queue(move |world: &mut World| init_structure(world, entity, &path));
}

fn init_structure(world: &mut World, entity: Entity, path: &str) {
    let t = now(world);
    if is_under(path, EGG) {
        let status = if is_under(path, EGG_BURST) {
            EggStatus::Burst
        } else if is_under(path, EGG_GROWN) {
            EggStatus::Grown
        } else if is_under(path, EGG_FAKE) {
            EggStatus::Fake
        } else {
            EggStatus::Growing
        };
        let grow_at = t + roll(world, EGG_GROWTH_SECS);
        world.entity_mut(entity).insert(Egg {
            status,
            grow_at,
            burst_at: 0.0,
            kill: false,
        });
        return;
    }
    if !is_under(path, WEED) {
        return;
    }
    if is_under(path, WEED_NODE) {
        // The non-node weeds under it go (`LateInitialize`).
        if let Some(&pos) = world.get::<TilePos>(entity) {
            let under: Vec<Entity> = world
                .resource::<ss13_world::SpatialIndex>()
                .on_tile(pos)
                .to_vec();
            for e in under {
                if e != entity && world.get::<Weed>(e).is_some() {
                    remove_tile_thing(world, e, pos);
                }
            }
        }
        let grow_at = t + roll(world, NODE_GROWTH_SECS);
        world.entity_mut(entity).insert((
            WeedNode { grow_at },
            Weed {
                node: Some(entity),
                dying_at: None,
            },
            Optics {
                opaque: false,
                tile: None,
                light: Some(LightSource {
                    range: 4.0,
                    power: 0.5,
                    color: NODE_LIGHT,
                    height: 1.0,
                    angle: 360,
                    dir: ss13_shared::Dir::South,
                    offset: [0, 0],
                }),
            },
        ));
        return;
    }
    if world.get::<Weed>(entity).is_none() {
        world.entity_mut(entity).insert(Weed {
            node: None,
            dying_at: None,
        });
    }
}

/// A weed on `pos`, if any.
pub fn weed_at(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<Weed>(e).is_some())
}

fn remove_tile_thing(world: &mut World, entity: Entity, pos: TilePos) {
    world.despawn(entity);
    tile_changed(world, pos);
}

/// `blacklisted_turfs`.
fn weed_blacklisted(world: &World, pos: TilePos) -> bool {
    const BAD: [&str; 5] = [
        "/turf/open/space",
        "/turf/open/chasm",
        "/turf/open/lava",
        "/turf/open/water",
        "/turf/open/openspace",
    ];
    crate::construction::turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .is_none_or(|p| BAD.iter().any(|b| is_under(&p, b)))
}

/// Tiles air can flow to from `pos`: `get_atmos_adjacent_turfs`, or plain open neighbours on
/// levels without simulated air.
fn open_neighbours(world: &World, pos: TilePos) -> Vec<TilePos> {
    if let Some(atmos) = world.get_resource::<Atmos>()
        && atmos.level(pos.z).is_some()
    {
        return atmos.adjacent_turfs(pos, false);
    }
    let index = world.resource::<ss13_world::SpatialIndex>();
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .map(|(dx, dy)| TilePos::new(pos.x + dx, pos.y + dy, pos.z))
        .filter(|&p| {
            crate::construction::turf_at(world, p).is_some() && index.dense_on_tile(p).is_empty()
        })
        .collect()
}

/// `try_expand`: weeds grow to the free tiles next to this one.
fn expand(world: &mut World, weed: Entity, node: Entity, pos: TilePos) {
    if weed_blacklisted(world, pos) {
        remove_tile_thing(world, weed, pos);
        return;
    }
    for neighbour in open_neighbours(world, pos) {
        if weed_blacklisted(world, neighbour) || weed_at(world, neighbour).is_some() {
            continue;
        }
        if let Some(new) = spawn_obj(world, WEED, neighbour) {
            world.entity_mut(new).insert(Weed {
                node: Some(node),
                dying_at: None,
            });
        }
    }
}

/// Node growth, orphaned weeds, hot air on weeds; once a second.
fn weed_tick(world: &mut World, mut clock: Local<f64>) {
    let t = now(world);
    if t < *clock {
        return;
    }
    *clock = t + WORLD_TICK;
    let due: Vec<(Entity, WeedNode, TilePos)> = world
        .query::<(Entity, &WeedNode, &TilePos)>()
        .iter(world)
        .filter(|(_, n, _)| t >= n.grow_at)
        .map(|(e, n, p)| (e, *n, *p))
        .collect();
    for (node, _, pos) in due {
        let next = t + roll(world, NODE_GROWTH_SECS);
        if let Some(mut n) = world.get_mut::<WeedNode>(node) {
            n.grow_at = next;
        }
        let around: Vec<(Entity, TilePos)> = {
            let index = world.resource::<ss13_world::SpatialIndex>();
            index
                .in_radius(pos, NODE_RANGE)
                .filter(|&e| world.get::<Weed>(e).is_some())
                .filter_map(|e| world.get::<TilePos>(e).map(|p| (e, *p)))
                .collect()
        };
        for (weed, wpos) in around {
            // `try_expand` needs a parent; the node's own range feeds the weeds around it.
            if world.get::<Weed>(weed).is_some_and(|w| w.node.is_some()) {
                expand(world, weed, node, wpos);
            }
        }
    }
    let weeds: Vec<(Entity, Weed, TilePos)> = world
        .query::<(Entity, &Weed, &TilePos)>()
        .iter(world)
        .map(|(e, w, p)| (e, *w, *p))
        .collect();
    for (weed, w, pos) in weeds {
        if air_above(world, pos, WARM_AIR) {
            crate::obj_damage::take_damage(world, weed, 5.0, DamageType::Burn);
            if world.get_entity(weed).is_err() {
                continue;
            }
        }
        if let Some(at) = w.dying_at {
            if t >= at {
                remove_tile_thing(world, weed, pos);
            }
            continue;
        }
        let Some(node) = w.node else {
            continue;
        };
        if world.get_entity(node).is_ok() {
            continue;
        }
        // `after_parent_destroyed`.
        let new_parent = {
            let index = world.resource::<ss13_world::SpatialIndex>();
            index
                .in_radius(pos, NODE_RANGE)
                .find(|&e| e != node && world.get::<WeedNode>(e).is_some())
        };
        let at = t + roll(world, WEED_DEATH_SECS);
        if let Some(mut w) = world.get_mut::<Weed>(weed) {
            match new_parent {
                Some(parent) => w.node = Some(parent),
                None => {
                    w.node = None;
                    w.dying_at = Some(at);
                }
            }
        }
    }
}

fn set_egg_state(world: &mut World, egg: Entity, status: EggStatus, state: &str) {
    if let Some(mut e) = world.get_mut::<Egg>(egg) {
        e.status = status;
    }
    crate::mobs::set_icon_state(world, egg, ALIEN_DMI, state);
}

/// `Burst(kill)`.
fn burst_egg(world: &mut World, egg: Entity, kill: bool) {
    let Some(e) = world.get::<Egg>(egg).copied() else {
        return;
    };
    if !matches!(e.status, EggStatus::Grown | EggStatus::Growing) {
        return;
    }
    let t = now(world);
    if let Some(mut e) = world.get_mut::<Egg>(egg) {
        e.status = EggStatus::Bursting;
        e.burst_at = t + EGG_BURST_SECS;
        e.kill = kill;
    }
}

/// `finish_bursting`: the child comes out on the egg's tile; dead, or at the first huggable mob.
fn finish_bursting(world: &mut World, egg: Entity, pos: TilePos, kill: bool) {
    set_egg_state(world, egg, EggStatus::Burst, "egg_hatched");
    let Some(hugger) = spawn_obj(world, FACEHUGGER, pos) else {
        return;
    };
    world.flush();
    if kill {
        die(world, hugger);
        return;
    }
    let around: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .in_radius(pos, 1)
        .collect();
    for mob in around {
        if can_hug(world, mob) && leap_to(world, hugger, mob) {
            break;
        }
    }
}

/// Egg growth, bursting, breaking (`atom_break`), hot air (`> 500 K`), proximity.
fn egg_tick(world: &mut World, mut clock: Local<f64>) {
    let t = now(world);
    if t < *clock {
        return;
    }
    *clock = t + 0.5;
    let eggs: Vec<(Entity, Egg, TilePos)> = world
        .query::<(Entity, &Egg, &TilePos)>()
        .iter(world)
        .map(|(e, g, p)| (e, *g, *p))
        .collect();
    for (egg, g, pos) in eggs {
        match g.status {
            EggStatus::Growing if t >= g.grow_at => {
                set_egg_state(world, egg, EggStatus::Grown, "egg");
            }
            EggStatus::Bursting if t >= g.burst_at => {
                finish_bursting(world, egg, pos, g.kill);
                continue;
            }
            _ => {}
        }
        if air_above(world, pos, HOT_AIR) {
            crate::obj_damage::take_damage(world, egg, 5.0, DamageType::Burn);
        }
        if world.get_entity(egg).is_err() {
            continue;
        }
        if matches!(g.status, EggStatus::Growing | EggStatus::Grown)
            && let (Some(obj), Some(integrity)) = (
                world.get::<DmObj>(egg),
                world.get::<crate::obj_damage::Integrity>(egg),
            )
            && obj.integrity_failure > 0.0
            && integrity.current <= obj.integrity_failure * obj.max_integrity
        {
            burst_egg(world, egg, true);
            continue;
        }
        if g.status == EggStatus::Grown {
            let around: Vec<Entity> = world
                .resource::<ss13_world::SpatialIndex>()
                .in_radius(pos, 1)
                .collect();
            let prey = around.into_iter().any(|m| {
                can_hug(world, m)
                    && (world.get::<AlienEmbryo>(m).is_none() || unconscious(world, m))
            });
            if prey {
                burst_egg(world, egg, false);
            }
        }
    }
}

fn unconscious(world: &World, mob: Entity) -> bool {
    world
        .get::<Stat>(mob)
        .is_some_and(|s| matches!(s, Stat::HardCrit | Stat::Dead))
        || crate::status_effects::knocked_out(world, mob)
}

// ---------------------------------------------------------------------------------------------
// Alien hands on structures

/// `attack_hand`/`attack_alien` on eggs and nests, picking up huggers. True when handled.
pub(super) fn xeno_click(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    caste: Caste,
) -> bool {
    if let Some(e) = world.get::<Egg>(target).copied() {
        egg_click(world, client, target, e);
        return true;
    }
    if path_of(world, target).is_some_and(|p| is_under(&p, NEST)) {
        return nest_click(world, client, mob, target);
    }
    if world.get::<FaceHugger>(target).is_some() && caste != Caste::Larva {
        return pick_up_hugger(world, client, mob, target);
    }
    false
}

fn egg_click(world: &mut World, client: Entity, target: Entity, e: Egg) {
    match e.status {
        EggStatus::Bursting => tell(world, client, Span::Notice, "Дитя вылупляется."),
        EggStatus::Burst => {
            tell(
                world,
                client,
                Span::Notice,
                "Вы убираете вылупившееся яйцо.",
            );
            if let Some(&pos) = world.get::<TilePos>(target) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(
                        pos,
                        "sound/effects/blob/attackblob.ogg",
                        100.0,
                        true,
                    ),
                );
                remove_tile_thing(world, target, pos);
            }
        }
        EggStatus::Growing => tell(world, client, Span::Notice, "Дитя ещё не развилось."),
        EggStatus::Grown => {
            tell(world, client, Span::Notice, "Вы забираете дитя.");
            burst_egg(world, target, false);
        }
        EggStatus::Fake => {}
    }
}

/// An alien's non-combat click on a nest: unbuckle its occupant, else buckle who stands on it
/// (`user_buckle_mob`, tg's drag). A combat-mode click claws it (`attack_alien`).
fn nest_click(world: &mut World, client: Entity, mob: Entity, nest: Entity) -> bool {
    if super::combat(world, mob) {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    if let Some(captive) = crate::medical::surgery::buckled_to(world, nest) {
        crate::medical::surgery::unbuckle(world, actor, captive);
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(nest) else {
        return true;
    };
    let victim = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| {
            world.get::<Health>(e).is_some()
                && world.get::<Xeno>(e).is_none()
                && world.get::<Buckled>(e).is_none()
        });
    if let Some(victim) = victim {
        crate::medical::surgery::buckle(world, actor, victim, nest);
        let (user, name) = (name_of(world, mob), name_of(world, victim));
        visible(
            world,
            victim,
            &format!("{user} обливает вас зловонной смолой, запирая в гнезде!"),
            &format!("{user} выделяет густую смолу, закрепляя {name} в гнезде!"),
            Span::Danger,
        );
    }
    true
}

fn pick_up_hugger(world: &mut World, client: Entity, mob: Entity, hugger: Entity) -> bool {
    let Some(&pos) = world.get::<TilePos>(hugger) else {
        return true;
    };
    let carried = world.get::<Xeno>(mob).map_or(0, |x| x.carried.len());
    if carried >= 2 {
        tell(world, client, Span::Warning, "Ваши руки заняты.");
        return true;
    }
    let name = name_of(world, hugger);
    world.entity_mut(hugger).remove::<TilePos>();
    tile_changed(world, pos);
    if let Some(mut x) = world.get_mut::<Xeno>(mob) {
        x.carried.push(hugger);
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы подбираете {name}."),
    );
    true
}

/// `nest/user_unbuckle_mob` for a captive freeing itself: 100 s of struggling; hooked from
/// `crate::medical::surgery::unbuckle`. True when this replaced the plain unbuckling.
pub fn nest_unbuckle(world: &mut World, actor: Actor, mob: Entity, furniture: Entity) -> bool {
    if actor.mob != mob
        || world.get::<Xeno>(mob).is_some()
        || !path_of(world, furniture).is_some_and(|p| is_under(&p, NEST))
    {
        return false;
    }
    let name = name_of(world, mob);
    visible(
        world,
        mob,
        "Вы пытаетесь вырваться из смолы... (Не двигайтесь около полутора минут.)",
        &format!("{name} пытается вырваться из смолы!"),
        Span::Warning,
    );
    do_after(
        world,
        actor,
        furniture,
        NEST_ESCAPE_DS,
        0,
        Job::Xeno(XenoJob::NestEscape),
    );
    true
}

/// The end of the struggle.
pub(super) fn complete_escape(world: &mut World, mob: Entity, nest: Entity) {
    if world.get::<Buckled>(mob).map(|b| b.0) != Some(nest) {
        return;
    }
    world.entity_mut(mob).remove::<Buckled>();
    let name = name_of(world, mob);
    visible(
        world,
        mob,
        "Вы вырываетесь из смолы!",
        &format!("{name} вырывается из смолы!"),
        Span::Warning,
    );
}

/// `mouse_drop_receive` guard: only aliens buckle onto a nest (`nest/user_buckle_mob`).
pub fn nest_refuses(world: &World, actor: Entity, furniture_path: &str) -> bool {
    is_under(furniture_path, NEST) && world.get::<Xeno>(actor).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nest_is_only_for_aliens() {
        let mut world = World::new();
        let human = world.spawn_empty().id();
        let alien = world.spawn(Xeno::new(Caste::Drone, 1)).id();
        assert!(nest_refuses(&world, human, NEST));
        assert!(!nest_refuses(&world, alien, NEST));
        assert!(!nest_refuses(&world, human, "/obj/structure/bed"));
    }

    #[test]
    fn a_hugger_goes_idle_unless_dead() {
        let mut h = FaceHugger {
            state: HuggerState::Awake,
            sterile: false,
            real: true,
            awake_at: 0.0,
            victim: None,
        };
        go_idle(&mut h, 30.0);
        assert_eq!(h.state, HuggerState::Asleep);
        assert_eq!(h.awake_at, 30.0);
        h.state = HuggerState::Dead;
        go_idle(&mut h, 60.0);
        assert_eq!(h.state, HuggerState::Dead);
    }
}
