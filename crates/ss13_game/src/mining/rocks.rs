//! Mineral walls, ore and the miner's tools (server), after tg `/turf/closed/mineral`
//! (`code/game/turfs/closed/minerals.dm`), `/obj/item/stack/ore` and `/obj/item/gibtonite`
//! (`code/modules/mining/ores_coins.dm`), the mining satchel (`/obj/item/storage/bag/ore`), the
//! ore box (`code/modules/mining/satchel_ore_box.dm`), self-recharging guns
//! (`guns/energy/recharge.dm`) and the proto-kinetic accelerator's projectile
//! (`guns/energy/kinetic_accelerator.dm`).
//!
//! - Digging (`manual_mine`): a `TOOL_MINING` item (pickaxes, drills, the jackhammer) on a
//!   mineral wall, by click or by walking into it with the tool in the active hand (`Bumped`),
//!   is a `use_tool` of `tool_mine_speed` (4 s) × `toolspeed`; a weak wall (`weak_turf`) comes
//!   apart by hand in `hand_mine_speed` (15 s). `gets_drilled` drops the wall's
//!   `mineral_amt` of `mineral_type` ore and scrapes it away to its floor. Explosions drill
//!   rocks by severity and hardness (`ex_act`), kinetic blasts drill what they hit.
//! - Random walls (`/turf/closed/mineral/random`) roll their ore at round start:
//!   `prob(mineral_chance)` picks from [`RANDOM_ORES`] (1-5 ore, or a gibtonite wall).
//! - Gibtonite: digging it starts a chain reaction (`explosive_reaction`): a pulse every 0.5 s
//!   for `det_time` (8-10) pulses, then `explosion(1, 3, 5)`; digging it again while active blows
//!   it at once (`explosion(1, 2, 5)`). A mining scanner on it stops the reaction (`defuse`); a
//!   stable deposit is dug out as a gibtonite ore of the quality its remaining pulses give.
//! - Gibtonite ore (`/obj/item/gibtonite`): a mining tool or anything with 10 force, a
//!   projectile, an explosion or the igniter of a rigged assembly holder primes it; 10 s later
//!   it blows by its quality (`explosion(0, 1, 3)`, `(1, 2, 5)`, `(2, 4, 9)`). A mining scanner
//!   or a multitool stops it at the lowest quality. An assembly holder with an igniter rigs to
//!   it; a wrench takes the rig off.
//! - Ore: a lit welder on a stack smelts it into its `refined_type` (`welder_act`); a
//!   devastating blast destroys it, lighter ones leave it be.
//! - The satchel takes only ore (`can_hold`) and scoops the ore under its carrier after every
//!   step (`handle_move`): into an ore box the carrier pulls, else merged into its stacks or
//!   inserted.
//! - The ore box takes ore by hand or a satchel's ore; its `OreBox` window lists and empties it;
//!   a crowbar (5 s) pries it apart into 4 planks and its contents.
//! - Self-recharging guns (the PKA): empty unless carried by a mob (the ebow, cyborg and
//!   minebot guns hold their charge); a carried one refills its cell `recharge_time` (1.6 s,
//!   changed by cooldown modkits, `super::modkits`) after it runs low; a kinetic blast that
//!   drills rock reloads it in `recharge_time` times the firer's mining speed if that is
//!   sooner. Kinetic force does a quarter of its damage where the pressure is over 50 kPa.
// shortcut: no ore veins (`spread_vein`), depth-based odds (`exposure_based`), scan overlays,
// Journeyman scan pulses of the mining skill (its speed and experience are `crate::skills`'),
// hulk/alien digging, `drill_aoe`, ore breaking on throw or melting in fire, the gibtonite ore's
// two-handed grip, throw knockdown or admin notices, or the `extra` recharge time for several
// carried PKAs.
// tg resets the reload timer on a drilled rock even when that delays it (and undoes the rapid
// repeater's quick reload); here the sooner reload wins, as the comment there intends.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::mining::{GIBTONITE_ITEM, GIBTONITE_TURF, RANDOM_ORES};
use ss13_mapload::{DmAtom, DmItem, DmMineral, DmName, DmOre, DmTool, IconIndex, MapSeed};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::{Bumped, Moved, SpatialIndex, WorldSystems};

use super::{OreBoxData, OreBoxEntry, OreBoxUi, REMOVE_ALL};
use crate::construction::{
    Job, baseturfs_of, can_merge, change_turf, merge_into, scrape_away, spawn_obj, spawn_stack,
    turf_at,
};
use crate::explosions::{Explosion, Severity};
use crate::health::Stat;
use crate::inventory::{ALL_SLOTS, CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::sound::{PlaySound, playsound};
use crate::storage::{Stored, StoredIn, can_insert, put_inside, take_out};
use crate::tools::{Actor, DoAfter, Stack, ToolKind, do_after, tell, tool_of, use_tool};

pub const ORE_BOX: &str = "/obj/structure/ore_box";
const ORE_BAG: &str = "/obj/item/storage/bag/ore";
const WOOD: &str = "/obj/item/stack/sheet/mineral/wood";
const MINING_SCANNERS: [&str; 2] = [
    "/obj/item/mining_scanner",
    "/obj/item/t_scanner/adv_mining_scanner",
];
const RECHARGE_GUN: &str = "/obj/item/gun/energy/recharge";
const EBOW: &str = "/obj/item/gun/energy/recharge/ebow";
const KINETIC: &str = "/obj/projectile/kinetic";
/// `tool.force >= 10` primes gibtonite ore.
const PRIMING_FORCE: f32 = 10.0;
/// `/obj/item/gibtonite::det_time`, seconds.
const ORE_DET_SECS: f64 = 10.0;
/// `TOOL_MINING`.
const TOOL_MINING: &str = "mining";
/// `/turf/closed/mineral::tool_mine_speed`: the hardness reference.
const BASE_MINE_SPEED: f32 = 40.0;
/// `LAVALAND_EQUIPMENT_EFFECT_PRESSURE`, kPa.
const EQUIPMENT_PRESSURE: f64 = 50.0;
/// The gibtonite chain reaction's pulse.
const PULSE_SECS: f64 = 0.5;
const BREAK_STONE: &str = "sound/effects/break_stone.ogg";

/// A timed mining action ([`crate::construction::Job::Mining`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiningJob {
    /// A mining tool on a wall (`manual_mine`).
    Dig,
    /// A weak wall by hand (`attack_hand`).
    DigByHand,
    /// A crowbar on an ore box.
    PryOreBox,
}

/// tg `GIBTONITE_*` stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GibStage {
    Unstruck,
    Active,
    Stable,
    Detonated,
}

/// A gibtonite deposit's state (tg `stage`, `det_time`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Gibtonite {
    pub stage: GibStage,
    pub det_time: i32,
    /// The next pulse of an active deposit.
    pub next_pulse: f64,
}

/// tg `GIBTONITE_QUALITY_*` of a dug-out gibtonite ore.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GibtoniteQuality(pub u8);

/// A primed gibtonite ore (`primed`, `det_timer`): it blows at this game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GibPrimed(pub f64);

/// A self-recharging gun's reload timer (tg `recharge_timerid`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Recharging {
    pub due: Option<f64>,
}

pub(super) struct RocksPlugin;

impl Plugin for RocksPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<OreBoxUi>()
            .add_systems(ss13_world::MapInit, roll_random_ores)
            .add_systems(
                Update,
                (
                    (dig_on_bump, scoop_ore, recharge_guns)
                        .chain()
                        .after(WorldSystems::Movement),
                    gibtonite_pulses.run_if(any_with_component::<Gibtonite>),
                    primed_ores.run_if(any_with_component::<GibPrimed>),
                    ore_box_acts.in_set(UiSystems::Handle),
                ),
            );
    }
}

fn is_mining_tool(world: &World, item: Entity) -> bool {
    world
        .get::<DmTool>(item)
        .is_some_and(|t| t.behaviour == TOOL_MINING)
}

fn item_path(world: &World, item: Entity) -> Option<&str> {
    world.get::<DmItem>(item).map(|i| i.path.as_str())
}

fn obj_path(world: &World, e: Entity) -> Option<String> {
    crate::construction::path_of(world, e)
}

/// `/turf/closed/mineral/random/Initialize`: each random rock rolls its ore once.
fn roll_random_ores(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let mut rng = ss13_mapload::spawner::Rng::new(seed ^ 0x6d69_6e65);
    let mut query =
        world.query_filtered::<(Entity, &DmMineral, &TilePos), With<ss13_world::Mapped>>();
    let rocks: Vec<(Entity, DmMineral, TilePos)> = query
        .iter(world)
        .map(|(e, m, p)| (e, m.clone(), *p))
        .collect();
    let total: u32 = RANDOM_ORES.iter().map(|(_, w)| w).sum();
    let mut rolled = 0;
    for (rock, mineral, pos) in rocks {
        if mineral.gibtonite {
            let det_time = rng.range(8, 10) as i32;
            world.entity_mut(rock).insert(Gibtonite {
                stage: GibStage::Unstruck,
                det_time,
                next_pulse: 0.0,
            });
            continue;
        }
        if mineral.mineral_chance <= 0.0 || !rng.prob(mineral.mineral_chance) {
            continue;
        }
        let mut pick = (rng.next_u64() % u64::from(total)) as u32;
        let ore = RANDOM_ORES
            .iter()
            .find(|(_, w)| {
                if pick < *w {
                    true
                } else {
                    pick -= w;
                    false
                }
            })
            .map(|(p, _)| *p)
            .unwrap_or(RANDOM_ORES[0].0);
        rolled += 1;
        if ore == GIBTONITE_TURF {
            let base = baseturfs_of(world, rock);
            if let Some(turf) = change_turf(world, pos, GIBTONITE_TURF, base, false) {
                let det_time = rng.range(8, 10) as i32;
                world.entity_mut(turf).insert(Gibtonite {
                    stage: GibStage::Unstruck,
                    det_time,
                    next_pulse: 0.0,
                });
            }
            continue;
        }
        // `change_ore(random = TRUE)`.
        let amount = rng.range(1, 5) as u32;
        if let Some(mut m) = world.get_mut::<DmMineral>(rock) {
            m.mineral_type = ore.to_owned();
            m.mineral_amt = amount;
        }
    }
    if rolled > 0 {
        info!("mining: {rolled} random rocks got ore");
    }
}

/// tg `/turf/closed/mineral/item_interaction` and the other mining clicks of a held item: a
/// mining tool or a mining scanner on a wall, ore or a satchel into an ore box, a crowbar on an
/// ore box, a lit welder on ore. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if world.get::<DmMineral>(target).is_some() {
        if let Some(path) = item_path(world, held)
            && MINING_SCANNERS.iter().any(|s| is_under(path, s))
            && world.get::<Gibtonite>(target).is_some()
        {
            defuse(world, target, mob);
            return true;
        }
        if is_mining_tool(world, held) {
            manual_mine(world, actor, target);
            return true;
        }
        return false;
    }
    let target_path = obj_path(world, target).unwrap_or_default();
    if is_under(&target_path, ORE_BOX) {
        return ore_box_interaction(world, actor, held, target);
    }
    if is_under(&target_path, GIBTONITE_ITEM) {
        return gibtonite_ore_interaction(world, actor, held, target);
    }
    if world.get::<DmOre>(target).is_some()
        && tool_of(world, held).map(|t| t.0) == Some(ToolKind::Welder)
    {
        smelt_with_welder(world, client, held, target);
        return true;
    }
    false
}

/// `manual_mine`: a `use_tool` of the wall's `tool_mine_speed`.
fn manual_mine(world: &mut World, actor: Actor, wall: Entity) {
    let Some(speed) = world.get::<DmMineral>(wall).map(|m| m.tool_mine_speed) else {
        return;
    };
    if world.get::<DoAfter>(actor.mob).is_some() {
        return;
    }
    // shortcut: no `usesound` while digging (tools carry none here).
    // `use_tool`'s `TOOL_MINING` skill modifier.
    let speed = speed * mining_speed(world, actor.mob);
    use_tool(world, actor, wall, speed, 0, Job::Mining(MiningJob::Dig));
}

/// tg `/turf/closed/mineral/attack_hand`: only weak walls come apart by hand. True when
/// `target` is a weak wall.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(m) = world.get::<DmMineral>(target).cloned() else {
        return false;
    };
    if !m.weak_turf {
        return false;
    }
    if world.get::<DoAfter>(mob).is_some() {
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    let speed = m.hand_mine_speed * mining_speed(world, mob);
    if do_after(
        world,
        actor,
        target,
        speed,
        0,
        Job::Mining(MiningJob::DigByHand),
    ) {
        tell(
            world,
            client,
            Span::Notice,
            "You start pulling out pieces...",
        );
    }
    true
}

/// tg `/turf/closed/mineral/Bumped`: walking into a wall with a mining tool in the active hand
/// digs it.
fn dig_on_bump(world: &mut World, mut cursor: Local<MessageCursor<Bumped>>) {
    let bumps: Vec<Bumped> = cursor
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .collect();
    for bump in bumps {
        if world.get::<DmMineral>(bump.obstacle).is_none() {
            continue;
        }
        let Some(held) = world
            .get::<Inventory>(bump.entity)
            .and_then(Inventory::active_item)
        else {
            continue;
        };
        let Some(client) = world
            .get::<crate::server::PlayerMob>(bump.entity)
            .map(|p| p.client)
        else {
            continue;
        };
        if is_mining_tool(world, held) && world.get::<DoAfter>(bump.entity).is_none() {
            let actor = Actor {
                client,
                mob: bump.entity,
                item: Some(held),
            };
            manual_mine(world, actor, bump.obstacle);
        }
    }
}

/// The end of a [`MiningJob`].
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: MiningJob) {
    let target = action.target;
    match job {
        MiningJob::Dig | MiningJob::DigByHand => {
            // `if(!ismineralturf(src))`: someone else got it first.
            if world.get::<DmMineral>(target).is_some() {
                // `manual_mine` drills with `exp_multiplier = 1`, `attack_hand` with none.
                if job == MiningJob::Dig {
                    crate::skills::mining_experience(world, mob, target, 1.0);
                }
                gets_drilled(world, action.target_pos, Some(mob));
            }
        }
        MiningJob::PryOreBox => {
            let is_box = obj_path(world, target).is_some_and(|p| is_under(&p, ORE_BOX));
            if !is_box {
                return;
            }
            tell(
                world,
                action.client,
                Span::Notice,
                "You pry apart the ore box.",
            );
            deconstruct_ore_box(world, target);
        }
    }
}

/// tg `gets_drilled`: the wall's ore drops and it scrapes away to its floor. Gibtonite reacts
/// instead (see the module docs).
pub fn gets_drilled(world: &mut World, pos: TilePos, user: Option<Entity>) {
    let Some(wall) = turf_at(world, pos) else {
        return;
    };
    let Some(mineral) = world.get::<DmMineral>(wall).cloned() else {
        return;
    };
    if let Some(gib) = world.get::<Gibtonite>(wall).copied() {
        match gib.stage {
            GibStage::Unstruck if mineral.mineral_amt >= 1 => {
                playsound(
                    world,
                    PlaySound::new(pos, "sound/effects/hit_on_shattered_glass.ogg", 50.0, true),
                );
                explosive_reaction(world, wall, user);
                return;
            }
            GibStage::Active if mineral.mineral_amt >= 1 => {
                detonate(world, wall, pos, (1, 2, 5));
            }
            GibStage::Stable => {
                let quality = match gib.det_time {
                    ..=0 => 3,
                    1..=2 => 2,
                    _ => 1,
                };
                if let Some(ore) = spawn_obj(world, GIBTONITE_ITEM, pos) {
                    world.entity_mut(ore).insert(GibtoniteQuality(quality));
                    if quality > 1 {
                        set_icon_state(world, ore, &format!("gibtonite_{quality}"));
                    }
                }
            }
            _ => {}
        }
        scrape_away(world, pos, false);
        return;
    }
    scrape_away(world, pos, false);
    if !mineral.mineral_type.is_empty() && mineral.mineral_amt > 0 {
        spawn_stack(world, &mineral.mineral_type, pos, mineral.mineral_amt);
    }
    playsound(world, PlaySound::new(pos, BREAK_STONE, 50.0, true));
}

fn set_icon_state(world: &mut World, e: Entity, state: &str) {
    let Some(icon) = world.get::<DmAtom>(e).map(|a| a.icon.clone()) else {
        return;
    };
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&icon, state));
    if let (Some(id), Some(mut look)) = (id, world.get_mut::<Appearance>(e)) {
        look.icon_state = id;
    }
}

/// tg `explosive_reaction`: the chain reaction starts.
fn explosive_reaction(world: &mut World, wall: Entity, user: Option<Entity>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut gib) = world.get_mut::<Gibtonite>(wall) else {
        return;
    };
    if gib.stage != GibStage::Unstruck {
        return;
    }
    gib.stage = GibStage::Active;
    gib.next_pulse = now;
    world.entity_mut(wall).insert(DmName {
        name: "gibtonite deposit".into(),
        desc: "An active gibtonite reserve. Run!".into(),
    });
    if let Some(&pos) = world.get::<TilePos>(wall) {
        crate::security::show(
            world,
            pos,
            crate::health::COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Danger,
                "There's gibtonite inside! It's going to explode!".into(),
            ),
            &[],
        );
        info!("gibtonite reaction at {pos:?} triggered by {user:?}");
    }
}

/// tg `defuse`: a mining scanner stops an active deposit.
fn defuse(world: &mut World, wall: Entity, _defuser: Entity) {
    let Some(mut gib) = world.get_mut::<Gibtonite>(wall) else {
        return;
    };
    if gib.stage != GibStage::Active {
        return;
    }
    gib.stage = GibStage::Stable;
    gib.det_time = gib.det_time.max(0);
    let left = gib.det_time;
    world.entity_mut(wall).insert(DmName {
        name: "gibtonite deposit".into(),
        desc: "An inactive gibtonite reserve. The ore can be extracted.".into(),
    });
    if let Some(&pos) = world.get::<TilePos>(wall) {
        crate::security::show(
            world,
            pos,
            crate::health::COMBAT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Notice,
                format!(
                    "The chain reaction stopped! The gibtonite had {left} reactions left till the explosion!"
                ),
            ),
            &[],
        );
    }
}

fn detonate(world: &mut World, wall: Entity, pos: TilePos, (dev, heavy, light): (i32, i32, i32)) {
    if let Some(mut gib) = world.get_mut::<Gibtonite>(wall) {
        gib.stage = GibStage::Detonated;
    }
    if let Some(mut m) = world.get_mut::<DmMineral>(wall) {
        m.mineral_amt = 0;
    }
    let mut ex = Explosion::new(dev, heavy, light);
    ex.flame = Some(0.0);
    ex.flash = Some(0.0);
    ex.cause = "gibtonite".into();
    crate::explosions::explosion(world, pos, ex);
}

/// tg `countdown`: one pulse per 0.5 s, the blast when `det_time` runs out.
fn gibtonite_pulses(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &mut Gibtonite, &TilePos)>();
    let mut due = Vec::new();
    for (e, mut gib, pos) in query.iter_mut(world) {
        if gib.stage != GibStage::Active {
            continue;
        }
        while now >= gib.next_pulse && gib.det_time > 0 {
            gib.det_time -= 1;
            gib.next_pulse += PULSE_SECS;
        }
        if gib.det_time <= 0 && now >= gib.next_pulse {
            due.push((e, *pos));
        }
    }
    for (wall, pos) in due {
        let amount = world.get::<DmMineral>(wall).map_or(0, |m| m.mineral_amt);
        if amount >= 1 {
            detonate(world, wall, pos, (1, 3, 5));
        } else if let Some(mut gib) = world.get_mut::<Gibtonite>(wall) {
            gib.stage = GibStage::Detonated;
        }
    }
}

/// tg `/turf/closed/mineral/ex_act`: harder rocks resist light and heavy blasts. True when the
/// turf of `pos` is a mineral wall.
pub fn ex_act(world: &mut World, pos: TilePos, severity: Severity) -> bool {
    let Some(speed) = turf_at(world, pos)
        .and_then(|t| world.get::<DmMineral>(t))
        .map(|m| m.tool_mine_speed)
    else {
        return false;
    };
    let hardness = BASE_MINE_SPEED / speed.max(1.0);
    let drilled = match severity {
        Severity::Devastate => true,
        Severity::Heavy => world
            .resource_mut::<crate::combat::CombatRng>()
            .chance(90.0 * hardness),
        Severity::Light => world
            .resource_mut::<crate::combat::CombatRng>()
            .chance(75.0 * hardness),
    };
    if drilled {
        gets_drilled(world, pos, None);
    }
    true
}

fn is_gibtonite_ore(world: &World, e: Entity) -> bool {
    item_path(world, e).is_some_and(|p| is_under(p, GIBTONITE_ITEM))
}

/// The assembly holder rigged to a gibtonite ore (`rig`).
fn rig_of(world: &World, ore: Entity) -> Option<Entity> {
    world
        .get::<Stored>(ore)?
        .items()
        .iter()
        .copied()
        .find(|&e| {
            world
                .get::<crate::assemblies::server::AssemblyHolder>(e)
                .is_some()
        })
}

/// tg `/obj/item/gibtonite/item_interaction`, `wrench_act` and `multitool_act`.
fn gibtonite_ore_interaction(world: &mut World, actor: Actor, held: Entity, ore: Entity) -> bool {
    use crate::assemblies::server::{Assembly, AssemblyHolder, Device, parts_of};
    let holder = world.get::<AssemblyHolder>(held).is_some();
    if holder && rig_of(world, ore).is_none() {
        let igniter = parts_of(world, held).into_iter().any(|p| {
            matches!(
                world.get::<Assembly>(p).map(|a| &a.device),
                Some(Device::Igniter { .. })
            )
        });
        if !igniter {
            return false;
        }
        put_inside(world, held, ore);
        if let Some(mut h) = world.get_mut::<AssemblyHolder>(held) {
            h.master = Some(ore);
        }
        info!("{:?} attached rig {held:?} to gibtonite {ore:?}", actor.mob);
        tell(world, actor.client, Span::Notice, "attached rig");
        return true;
    }
    match tool_of(world, held).map(|t| t.0) {
        Some(ToolKind::Wrench) => {
            let Some(rig) = rig_of(world, ore) else {
                return false;
            };
            if let Some(mut h) = world.get_mut::<AssemblyHolder>(rig) {
                h.master = None;
            }
            info!(
                "{:?} detached rig {rig:?} from gibtonite {ore:?}",
                actor.mob
            );
            crate::assemblies::server::to_hands(world, actor.mob, rig);
            tell(world, actor.client, Span::Notice, "detached rig");
            return true;
        }
        Some(ToolKind::Multitool) => {
            defuse_ore(world, ore, Some(actor.mob));
            return true;
        }
        _ => {}
    }
    let force = world.get::<DmItem>(held).map_or(0.0, |i| i.force);
    if is_mining_tool(world, held) || force >= PRIMING_FORCE {
        prime_ore(world, ore, Some(actor.mob));
        return true;
    }
    if item_path(world, held).is_some_and(|p| MINING_SCANNERS.iter().any(|s| is_under(p, s))) {
        defuse_ore(world, ore, Some(actor.mob));
        return true;
    }
    false
}

/// A held tool on the gibtonite ore in the other hand. True when handled.
pub fn hand_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !is_gibtonite_ore(world, target) {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    gibtonite_ore_interaction(world, actor, held, target)
}

/// tg `GibtoniteReaction`: the ore primes and blows in 10 s.
pub fn prime_ore(world: &mut World, ore: Entity, user: Option<Entity>) {
    if !is_gibtonite_ore(world, ore) || world.get::<GibPrimed>(ore).is_some() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(ore).insert(GibPrimed(now + ORE_DET_SECS));
    set_icon_state(world, ore, "gibtonite_active");
    let Some(pos) = crate::explosions::turf_of(world, ore) else {
        return;
    };
    playsound(
        world,
        PlaySound::new(pos, "sound/effects/hit_on_shattered_glass.ogg", 50.0, true),
    );
    if let Some(user) = user {
        let name = crate::security::name_of(world, user);
        let what = crate::security::name_of(world, ore);
        let at = world.get::<TilePos>(user).copied().unwrap_or(pos);
        crate::security::show(
            world,
            at,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Warning,
                format!("{name} strikes the {what}, causing a chain reaction!"),
            ),
            &[(
                user,
                Span::Danger,
                format!("You strike the {what}, causing a chain reaction."),
            )],
        );
    }
    info!("gibtonite ore {ore:?} at {pos:?} primed by {user:?}");
}

/// tg `/obj/item/gibtonite/defuse`: the reaction stops, the quality drops to the lowest.
pub fn defuse_ore(world: &mut World, ore: Entity, defuser: Option<Entity>) {
    if world.get::<GibPrimed>(ore).is_none() {
        return;
    }
    world
        .entity_mut(ore)
        .remove::<GibPrimed>()
        .insert(GibtoniteQuality(1));
    set_icon_state(world, ore, "gibtonite");
    if let Some(defuser) = defuser
        && let Some(&at) = world.get::<TilePos>(defuser)
    {
        crate::security::show(
            world,
            at,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Notice,
                "The chain reaction stopped! ...The ore's quality looks diminished.".into(),
            ),
            &[(
                defuser,
                Span::Notice,
                "You stopped the chain reaction. ...The ore's quality looks diminished.".into(),
            )],
        );
    }
}

/// tg `/obj/item/gibtonite/detonate` when the timer runs out.
fn primed_ores(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &GibPrimed)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, p)| now >= p.0)
        .map(|(e, _)| e)
        .collect();
    for ore in due {
        let quality = world.get::<GibtoniteQuality>(ore).map_or(1, |q| q.0);
        let Some(pos) = crate::explosions::turf_of(world, ore) else {
            world.despawn(ore);
            continue;
        };
        let (dev, heavy, light) = match quality {
            3 => (2, 4, 9),
            2 => (1, 2, 5),
            _ => (0, 1, 3),
        };
        // Still primed: the blast's own `ex_act` on it does nothing more.
        let mut ex = Explosion::new(dev, heavy, light);
        ex.flame = Some(0.0);
        ex.flash = Some(0.0);
        ex.cause = "gibtonite ore".into();
        crate::explosions::explosion(world, pos, ex);
        if world.get_entity(ore).is_ok() {
            world.despawn(ore);
        }
    }
}

/// `COMSIG_IGNITER_ACTIVATE` on the holder's loc: a rigged gibtonite ore primes.
pub fn igniter_activated(world: &mut World, igniter: Entity) {
    use crate::assemblies::server::{Assembly, Attached};
    let Some(Attached::Holder(holder)) = world.get::<Assembly>(igniter).map(|a| a.attached) else {
        return;
    };
    let Some(ore) = world.get::<StoredIn>(holder).map(|s| s.0) else {
        return;
    };
    if is_gibtonite_ore(world, ore) {
        info!("gibtonite ore {ore:?} primed by its rig {holder:?}");
        prime_ore(world, ore, None);
    }
}

/// `/obj/item/gibtonite/bullet_act`: any projectile primes it.
pub fn bullet_act(world: &mut World, firer: Entity, target: Entity) {
    if is_gibtonite_ore(world, target) {
        let user = world.get_entity(firer).is_ok().then_some(firer);
        prime_ore(world, target, user);
    }
}

/// `/obj/item/gibtonite/ex_act` (it primes, nothing else) and `/obj/item/stack/ore/ex_act`
/// (destroyed by devastation, untouched by lighter blasts). True when `item` is one of them.
pub fn item_ex_act(world: &mut World, item: Entity, severity: Severity) -> bool {
    if is_gibtonite_ore(world, item) {
        prime_ore(world, item, None);
        return true;
    }
    if !is_ore(world, item) {
        return false;
    }
    if severity >= Severity::Devastate {
        world.despawn(item);
    }
    true
}

/// `/obj/item/gibtonite/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if !is_gibtonite_ore(world, target) {
        return;
    }
    lines.push(if rig_of(world, target).is_some() {
        (
            Span::Warning,
            "There is some kind of device rigged to it!".into(),
        )
    } else {
        (Span::Notice, "You could rig something to it.".into())
    });
}

/// `/datum/storage/bag/ore`'s `set_holdable(/obj/item/stack/ore)`: a mining satchel refuses
/// anything else.
pub fn satchel_refuses(world: &World, storage: Entity, item: Entity) -> bool {
    item_path(world, storage).is_some_and(|p| is_under(p, ORE_BAG)) && !is_ore(world, item)
}

/// `/obj/item/stack/ore/welder_act`: a lit welder smelts `max(1, ceil(1 / sheets))` ore into
/// `max(1, floor(sheets))` sheets of its `refined_type`, where `sheets` is the ore's material
/// per unit in sheets.
fn smelt_with_welder(world: &mut World, client: Entity, welder: Entity, ore: Entity) {
    let Some(refined) = world
        .get::<DmOre>(ore)
        .map(|o| o.refined_type.clone())
        .filter(|r| !r.is_empty())
    else {
        return;
    };
    if !crate::tools::tool_usable(world, welder, 0, 0.0) {
        let text = crate::tools::unusable_message(world, welder, 0);
        tell(world, client, Span::Warning, text);
        return;
    }
    let path = item_path(world, ore).unwrap_or_default().to_owned();
    let units: f64 = world
        .get_resource::<crate::science::techweb::ResearchDb>()
        .and_then(|db| db.stacks.get(&path))
        .map_or(0.0, |m| m.iter().map(|(_, n)| n).sum());
    // Sand is not a silo material: a sheet per unit, as tg's `/datum/material/sand`.
    let sheets = if units > 0.0 {
        units / ss13_mapload::research::SHEET_MATERIAL_AMOUNT
    } else {
        1.0
    };
    let used = ((1.0 / sheets).ceil() as u32).max(1);
    if crate::tools::stack_amount(world, ore) < used {
        return;
    }
    let Some(pos) = crate::explosions::turf_of(world, ore) else {
        return;
    };
    crate::tools::use_stack(world, ore, used);
    spawn_stack(world, &refined, pos, (sheets.floor() as u32).max(1));
}

/// The satchel a mob carries (`equipped` registers it on its carrier).
fn carried_satchel(world: &World, mob: Entity) -> Option<Entity> {
    let inventory = world.get::<Inventory>(mob)?;
    ALL_SLOTS
        .iter()
        .filter_map(|&s| inventory.slot(s))
        .find(|&i| item_path(world, i).is_some_and(|p| is_under(p, ORE_BAG)))
}

/// tg `/obj/item/storage/bag/ore/handle_move` after each step of its carrier.
// Cost: one inventory scan per step of a mob with an inventory, one tile lookup for those
// carrying a satchel.
fn scoop_ore(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        let Some(bag) = carried_satchel(world, m.entity) else {
            continue;
        };
        if world.get::<Stat>(m.entity) != Some(&Stat::Conscious) {
            continue;
        }
        let ores: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(m.to)
            .iter()
            .copied()
            .filter(|&e| world.get::<DmOre>(e).is_some())
            .collect();
        if ores.is_empty() {
            continue;
        }
        let pulled_box = world
            .get::<crate::pull::Pulling>(m.entity)
            .map(|p| p.0)
            .filter(|&b| obj_path(world, b).is_some_and(|p| is_under(&p, ORE_BOX)));
        let mut picked = false;
        for ore in ores {
            picked |= pickup_ore(world, bag, ore, pulled_box);
        }
        if picked {
            playsound(world, PlaySound::new(m.to, "rustle", 50.0, true));
        }
    }
}

/// tg `pickup_ore`: into the pulled ore box, else merged into the satchel's stacks, else in.
fn pickup_ore(world: &mut World, bag: Entity, ore: Entity, ore_box: Option<Entity>) -> bool {
    if let Some(ore_box) = ore_box {
        put_inside(world, ore, ore_box);
        return true;
    }
    let stored: Vec<Entity> = world
        .get::<Stored>(bag)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for into in stored {
        if world.get_entity(ore).is_err() {
            return true;
        }
        if can_merge(world, ore, into) {
            merge_into(world, ore, into);
        }
    }
    if world.get_entity(ore).is_err() {
        return true;
    }
    if can_insert(world, bag, ore).is_ok() {
        put_inside(world, ore, bag);
        return true;
    }
    false
}

/// tg `/obj/structure/ore_box/item_interaction` and `crowbar_act`.
fn ore_box_interaction(world: &mut World, actor: Actor, held: Entity, ore_box: Entity) -> bool {
    if world.get::<DmOre>(held).is_some() {
        put_inside(world, held, ore_box);
        return true;
    }
    if world.get::<crate::storage::Storage>(held).is_some() {
        let ores: Vec<Entity> = world
            .get::<Stored>(held)
            .map(|s| s.items().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|&e| world.get::<DmOre>(e).is_some())
            .collect();
        for ore in ores {
            put_inside(world, ore, ore_box);
        }
        tell(
            world,
            actor.client,
            Span::Notice,
            "You empty the ore into the ore box.",
        );
        return true;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(actor.mob)
        .is_some_and(|c| c.0);
    if !combat && tool_of(world, held).map(|t| t.0) == Some(ToolKind::Crowbar) {
        use_tool(
            world,
            actor,
            ore_box,
            50.0,
            0,
            Job::Mining(MiningJob::PryOreBox),
        );
        return true;
    }
    false
}

/// `atom_deconstruct`: 4 planks, then `dump_box_contents`.
fn deconstruct_ore_box(world: &mut World, ore_box: Entity) {
    let Some(&pos) = world.get::<TilePos>(ore_box) else {
        return;
    };
    spawn_stack(world, WOOD, pos, 4);
    dump_box(world, ore_box);
    world.despawn(ore_box);
    crate::construction::tile_changed(world, pos);
}

/// `dump_box_contents`.
pub fn dump_box(world: &mut World, ore_box: Entity) {
    let Some(&pos) = world.get::<TilePos>(ore_box) else {
        return;
    };
    let items: Vec<Entity> = world
        .get::<Stored>(ore_box)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for item in items {
        take_out(world, item, pos);
    }
}

/// An empty hand on an ore box opens its window (tg opens it on `examine`); on a weak wall,
/// digs it. True when handled.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if attack_hand(world, client, mob, target) {
        return true;
    }
    if !obj_path(world, target).is_some_and(|p| is_under(&p, ORE_BOX)) {
        return false;
    }
    let name = world
        .get::<DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<OreBoxUi>(client, mob, target, name));
    true
}

impl ServerUi for OreBoxUi {
    type Source = Option<&'static Stored>;
    type UserParam = (
        Query<'static, 'static, &'static DmName>,
        Query<'static, 'static, &'static Stack>,
    );
    type Act = ();

    fn static_data(_: Option<&Stored>) {}

    fn data(
        stored: Option<&Stored>,
        _: Entity,
        (names, stacks): &bevy::ecs::system::SystemParamItem<
            '_,
            '_,
            (
                Query<'static, 'static, &'static DmName>,
                Query<'static, 'static, &'static Stack>,
            ),
        >,
    ) -> OreBoxData {
        let mut materials: Vec<OreBoxEntry> = Vec::new();
        for &item in stored.map(Stored::items).unwrap_or_default() {
            let name = names.get(item).map(|n| n.name.clone()).unwrap_or_default();
            let amount = stacks.get(item).map_or(1, |s| s.amount);
            match materials.iter_mut().find(|m| m.name == name) {
                Some(m) => m.amount += amount,
                None => materials.push(OreBoxEntry { name, amount }),
            }
        }
        OreBoxData { materials }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<(), UiReject> {
        match action {
            REMOVE_ALL => Ok(()),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn ore_box_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<OreBoxUi>>>) {
    let boxes: Vec<Entity> = acts
        .read(world.resource::<Messages<UiActed<OreBoxUi>>>())
        .map(|a| a.source)
        .collect();
    for ore_box in boxes {
        if obj_path(world, ore_box).is_some_and(|p| is_under(&p, ORE_BOX)) {
            dump_box(world, ore_box);
        }
    }
}

/// tg `/obj/item/gun/energy/recharge::holds_charge`.
// shortcut: the recharge guns' values are written here, not converted per type
// (`super::modkits::base_recharge` has their `recharge_time`).
fn holds_charge(path: &str) -> bool {
    is_under(path, EBOW)
        || is_under(
            path,
            "/obj/item/gun/energy/recharge/kinetic_accelerator/cyborg",
        )
        || is_under(
            path,
            "/obj/item/gun/energy/recharge/kinetic_accelerator/minebot",
        )
}

/// tg `attempt_reload(set_recharge_time)` of `gun`, deciseconds: the reload timer restarts
/// (`restart`) or, if one runs sooner, keeps it.
pub fn attempt_reload(world: &mut World, gun: Entity, recharge_ds: f64, restart: bool) {
    let full = match (
        world.get::<crate::guns::Gun>(gun),
        world.get::<ss13_mapload::DmGun>(gun),
    ) {
        (Some(g), Some(dm)) => g.charge >= dm.cell_maxcharge,
        _ => return,
    };
    if full {
        return;
    }
    let at = world.resource::<Time>().elapsed_secs_f64() + recharge_ds / 10.0;
    let mut entity = world.entity_mut(gun);
    match entity.get_mut::<Recharging>() {
        Some(mut r) => {
            r.due = Some(match r.due {
                Some(due) if !restart => due.min(at),
                _ => at,
            });
        }
        None => {
            entity.insert(Recharging { due: Some(at) });
        }
    }
}

/// tg `attempt_reload` / `reload` / `empty_if_not_held` for every self-recharging gun.
// Cost: a handful of guns (PKAs, ebows), each checked once per tick.
#[allow(clippy::type_complexity)]
fn recharge_guns(
    mut guns: Query<(
        Entity,
        &DmItem,
        &mut crate::guns::Gun,
        &ss13_mapload::DmGun,
        Option<&mut Recharging>,
        Option<&CarriedBy>,
        Option<&super::modkits::RechargeTime>,
    )>,
    positions: Query<&TilePos>,
    time: Res<Time>,
    mut commands: Commands,
    mut sounds: MessageWriter<PlaySound>,
) {
    let now = time.elapsed_secs_f64();
    for (e, item, mut gun, dm, recharging, carried, recharge) in &mut guns {
        if !is_under(&item.path, RECHARGE_GUN) {
            continue;
        }
        let holds = holds_charge(&item.path);
        let recharge_ds =
            recharge.map_or_else(|| super::modkits::base_recharge(&item.path), |t| t.0);
        let Some(mut recharging) = recharging else {
            // `Initialize`: `empty()` unless it holds its charge.
            if !holds {
                gun.charge = 0.0;
            }
            commands.entity(e).insert(Recharging::default());
            continue;
        };
        if carried.is_none() && !holds {
            if gun.charge > 0.0 {
                gun.charge = 0.0;
            }
            recharging.due = None;
            continue;
        }
        if gun.charge >= dm.cell_maxcharge {
            recharging.due = None;
            continue;
        }
        match recharging.due {
            None => recharging.due = Some(now + recharge_ds / 10.0),
            Some(at) if now >= at => {
                gun.charge = dm.cell_maxcharge;
                recharging.due = None;
                let pos = carried
                    .and_then(|c| positions.get(c.0).ok())
                    .or_else(|| positions.get(e).ok());
                if let Some(&pos) = pos {
                    sounds.write(PlaySound::new(
                        pos,
                        "sound/items/weapons/kinetic_reload.ogg",
                        60.0,
                        true,
                    ));
                }
            }
            Some(_) => {}
        }
    }
}

/// `/obj/projectile/kinetic/prehit_pierce`: the modkits' `projectile_prehit`, then kinetic
/// force weakens to its `pressure_decrease` (a quarter, raised by modkits) where the target's
/// pressure is over 50 kPa (`lavaland_equipment_pressure_check`).
pub fn kinetic_prehit(world: &mut World, p: &mut crate::projectile::Projectile, target: Entity) {
    if !is_under(&p.data.path, KINETIC) {
        return;
    }
    super::modkits::prehit(world, p, target);
    let Some(pos) = crate::explosions::turf_of(world, target) else {
        return;
    };
    if pressure_over(world, pos) {
        p.data.damage *= super::modkits::pressure_decrease(world, p.fired_from);
    }
}

pub(super) fn pressure_over(world: &World, pos: TilePos) -> bool {
    world
        .get_resource::<ss13_atmos::Atmos>()
        .and_then(|a| a.pressure(pos))
        .unwrap_or(0.0)
        > EQUIPMENT_PRESSURE
}

/// `/obj/projectile/kinetic/strike_thing` on what it hit: the modkits strike, a mineral wall
/// is drilled and the gun may reload sooner.
pub fn kinetic_strike(world: &mut World, p: &crate::projectile::Projectile, target: Entity) {
    if !is_under(&p.data.path, KINETIC) {
        return;
    }
    let Some(pos) = crate::explosions::turf_of(world, target) else {
        return;
    };
    strike_thing(world, p, Some(target), pos);
}

/// `/obj/projectile/kinetic/on_range`: the blast where it stops.
pub fn kinetic_on_range(world: &mut World, p: &mut crate::projectile::Projectile, pos: TilePos) {
    if !is_under(&p.data.path, KINETIC) {
        return;
    }
    if pressure_over(world, pos) {
        p.data.damage *= super::modkits::pressure_decrease(world, p.fired_from);
    }
    strike_thing(world, p, None, pos);
}

fn strike_thing(
    world: &mut World,
    p: &crate::projectile::Projectile,
    target: Option<Entity>,
    pos: TilePos,
) {
    let mineral =
        |world: &World| turf_at(world, pos).filter(|&t| world.get::<DmMineral>(t).is_some());
    let was_mineral = mineral(&*world).is_some();
    super::modkits::strike(world, p, target, pos, was_mineral);
    let Some(wall) = mineral(&*world) else {
        return;
    };
    // `gets_drilled(firer, 1)`.
    crate::skills::mining_experience(world, p.firer, wall, 1.0);
    gets_drilled(world, pos, Some(p.firer));
    // `if(iscarbon(firer))`: `attempt_reload(recharge_time * skill_modifier)`.
    if let Some(gun) = p.fired_from
        && world.get::<crate::human::HumanLook>(p.firer).is_some()
    {
        let time =
            super::modkits::recharge_time(world, gun) * f64::from(mining_speed(world, p.firer));
        attempt_reload(world, gun, time, false);
    }
}

/// `get_skill_modifier(/datum/skill/mining, SKILL_SPEED_MODIFIER)`.
fn mining_speed(world: &World, mob: Entity) -> f32 {
    crate::skills::modifier(
        world,
        mob,
        crate::skills::Skill::Mining,
        crate::skills::Modifier::Speed,
    )
}

/// Whether `item` is an ore stack (for other modules: the ORM).
pub fn is_ore(world: &World, item: Entity) -> bool {
    world.get::<DmOre>(item).is_some()
}

/// Items in a storage that are ore (`atom_storage.remove_type(/obj/item/stack/ore)`).
pub fn stored_ores(world: &World, storage: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(storage)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&e| world.get::<DmOre>(e).is_some())
        .collect()
}
