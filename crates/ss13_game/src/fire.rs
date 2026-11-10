//! Fire and space wind on things (server). The air side (reactions, hotspots) is
//! `ss13_atmos`; this module lights fires and applies them:
//!
//! - open flames: a lit welder on the floor or in a hand exposes its turf every second (tg
//!   `open_flame()`: 700 K, 5 L), so plasma in the air catches fire;
//! - `fire_act` ([`FireAct`] from hotspots, and a living mob walking into one): mobs get 3 fire
//!   stacks and ignite (`/mob/living/fire_act`); flammable structures and machines take fire
//!   damage and start burning (`/obj/fire_act`, `/datum/component/burning`);
//! - burning mobs ([`FireStacks`], tg `/datum/status_effect/fire_handler/fire_stacks`): every
//!   2 s the stacks decay, the fire needs oxygen, heats the body (`harm_human`; the burns come
//!   from body temperature in `crate::breath`) and the turf (`hotspot_expose(700, 50, TRUE)`);
//!   the mob glows, has tg's fire overlay and the fire alert; resisting rolls on the floor
//!   (`/datum/status_effect/stop_drop_roll`);
//! - wet mobs ([`WetStacks`], tg `/datum/status_effect/fire_handler/wet_stacks`): water on the
//!   skin (`/datum/reagent/water/expose_mob`, `crate::janitor::expose`), firefighting foam;
//!   the stacks dry off by 0.5 a second (checked every 2 s). Wet and fire stacks are each
//!   other's `enemy_types`: the one newly applied cancels the other stack for stack, so a wet
//!   mob cannot be set alight until it dried;
//! - gas tanks react their contents (`/obj/item/tank/process`);
//! - space wind (tg `high_pressure_movements`): every [`PressureDifference`] pushes the
//!   unanchored, unpulled movables on its tile one step with tg's `move_prob`.
// shortcut: lighters are welders here; matches and cigarettes burn as flame sources in
// `crate::smoking` (igniters and sparkers are `crate::wallmount::machines`, firelocks closing on
// hot air `crate::wallmount::firelock`); no burning items or clothing (`burn_clothing`, ash):
// add with their items. Wet stacks: no `TRAIT_WET_FOR_LONGER`, `TRAIT_SLIPPERY_WHEN_WET`, droplet
// particles or freezing into the freon status (`WATER_VAPOR_DEPOSITION_POINT`); a negative
// `adjust_wet_stacks` on a dry mob does nothing (tg's `max(fire_stacks, stacks)` turns it into
// positive wet stacks that douse a burning mob, an obvious accident).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_atmos::reactions::{Exposures, Holder};
use ss13_atmos::{Atmos, AtmosSystems, FireAct, GasId, HotspotEffect, PressureDifference};
use ss13_mapload::{DmClothing, DmItem, DmObj, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_net::server::{NetServerSystems, Networked};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, Dir, LightSource, Optics, Overlay, TilePos};
use ss13_world::{Moved, Mover, SpatialIndex, WorldSystems};

use crate::breath::{AlertCategory, Alerts, Respiration};
use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, Health, HealthSystems, Stat};
use crate::internals::TankAir;
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::{INDESTRUCTIBLE, Integrity, clients_in_range, deconstruct, is_under};
use crate::pull::{PulledBy, step_once};
use crate::server::PlayerMob;
use crate::tools::Welder;

/// `MAX_FIRE_STACKS`, `MOB_BIG_FIRE_STACK_THRESHOLD`.
pub const MAX_FIRE_STACKS: f32 = 20.0;
pub const MOB_BIG_FIRE_STACK_THRESHOLD: f32 = 5.0;
/// `/mob/living::fire_stack_decay_rate`, stacks per second.
pub const FIRE_STACK_DECAY_RATE: f32 = -0.05;
/// `tick_interval` of the fire stacks status effect.
pub const FIRE_TICK_SECS: f64 = 2.0;
/// `wet_stacks/tick`: stacks per second.
pub const WET_STACK_DECAY_RATE: f32 = -0.5;
/// `WET_STACKS_DAMP`, `WET_STACKS_SOAKED`.
const WET_STACKS_DAMP: f32 = 3.0;
const WET_STACKS_SOAKED: f32 = 15.0;
/// `/mob/living/fire_act`: stacks added.
pub const FIRE_ACT_STACKS: f32 = 3.0;
/// `BODYTEMP_HEATING_MAX`, `BODYTEMP_FIRE_TEMP_SOFTCAP`.
pub const BODYTEMP_HEATING_MAX: f64 = 30.0;
pub const BODYTEMP_FIRE_TEMP_SOFTCAP: f64 = 1200.0;
/// `FIRE_SUIT_MAX_TEMP_PROTECT`, `FIRE_HELM_MAX_TEMP_PROTECT`, `FIRE_IMMUNITY_MAX_TEMP_PROTECT`.
pub const FIRE_SUIT_MAX_TEMP_PROTECT: f32 = 30000.0;
pub const FIRE_HELM_MAX_TEMP_PROTECT: f32 = 30000.0;
pub const FIRE_IMMUNITY_MAX_TEMP_PROTECT: f32 = 35000.0;
const THERMAL_PROTECTION_HEAD: f32 = 0.3;
const CHEST: u32 = 1 << 0;
const HEAD: u32 = 1 << 2;
/// `open_flame()` defaults.
pub const OPEN_FLAME_HEAT: f64 = 700.0;
pub const OPEN_FLAME_VOLUME: f64 = 5.0;
/// `SSobj` and `SSburning` (`/datum/controller/subsystem/processing::wait`).
pub const SSOBJ_WAIT_SECS: f64 = 1.0;
/// `/datum/status_effect/stop_drop_roll::tick_interval`.
pub const ROLL_TICK_SECS: f64 = 0.8;
/// `LIGHT_RANGE_FIRE`, `LIGHT_POWER_FIRE`, `LIGHT_COLOR_FIRE`.
pub const LIGHT_RANGE_FIRE: f32 = 3.0;
pub const LIGHT_POWER_FIRE: f32 = 2.0;
pub const LIGHT_COLOR_FIRE: [u8; 3] = [0xFA, 0xA0, 0x19];
/// `resistance_flags`.
pub const FIRE_PROOF: u32 = 1 << 1;
pub const FLAMMABLE: u32 = 1 << 2;
/// `/datum/component/burning`: damage per second.
pub const BURNING_DAMAGE_PER_SEC: f32 = 10.0;
const ONFIRE_DMI: &str = "icons/mob/effects/onfire.dmi";
/// BandaStation `GLOB.fire_overlay`.
const BURNING_DMI: &str = "modular_bandastation/aesthetics/fire/icons/fire.dmi";

/// `pressure_resistance` of `/obj/item`, `/obj/structure`, `/obj/machinery` and humans.
pub const ITEM_PRESSURE_RESISTANCE: f64 = 4.0;
pub const STRUCTURE_PRESSURE_RESISTANCE: f64 = 8.0;
pub const MACHINERY_PRESSURE_RESISTANCE: f64 = 15.0;
pub const HUMAN_PRESSURE_RESISTANCE: f64 = 25.0;
/// `MOVE_FORCE_DEFAULT`, `MOVE_RESIST_DEFAULT` (every movable here), `MOVE_FORCE_PUSH_RATIO`.
pub const MOVE_FORCE_DEFAULT: f64 = 1000.0;
pub const MOVE_RESIST_DEFAULT: f64 = 1000.0;
pub const MOVE_FORCE_PUSH_RATIO: f64 = 1.0;
/// `world.tick_lag` in seconds (20 fps).
const TICK_LAG_SECS: f64 = 0.05;

/// tg fire stacks on a living mob (`fire_stacks`, `on_fire`) and the status effect's clock.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct FireStacks {
    pub stacks: f32,
    pub on_fire: bool,
    next_tick: f64,
    /// The mob got its `Optics` from the fire (tg `moblight`).
    lit_by_fire: bool,
}

/// tg `/datum/status_effect/fire_handler/wet_stacks` on a living mob (`stack_limit`
/// [`MAX_FIRE_STACKS`]).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct WetStacks {
    pub stacks: f32,
    next_tick: f64,
}

/// `/datum/status_effect/stop_drop_roll`: the mob rolls, kept knocked down
/// (`crate::status_effects`) between rolls.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Rolling {
    next_tick: f64,
}

/// `/datum/component/burning` on a structure or machine.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Burning;

#[derive(Resource, Default)]
struct FireClocks {
    flames: f64,
    burning: f64,
    sound: f64,
}

/// `/datum/looping_sound/fire`: `volume`, `mid_length` (s).
pub const FIRE_LOOP_VOLUME: f32 = 30.0;
pub const FIRE_LOOP_SECS: f64 = 2.0;

pub struct FireServerPlugin;

impl Plugin for FireServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FireClocks>()
            .init_resource::<crate::atmos_exposure::AtomProcess>()
            .add_observer(network_hotspot)
            .add_systems(
                Update,
                crate::atmos_exposure::process_atoms
                    .run_if(ss13_atmos::air_tick_due)
                    .after(AtmosSystems::Fire)
                    .before(HealthSystems),
            )
            .add_systems(
                Update,
                (
                    open_flames,
                    fire_acts,
                    entered_fire,
                    fire_stacks_tick,
                    wet_stacks_tick,
                    rolling_tick,
                    burning_objects,
                    space_wind,
                    fire_sound_loops,
                    crate::turf_damage::burn_turfs,
                )
                    .chain()
                    .after(AtmosSystems::Fire)
                    .after(WorldSystems::Movement)
                    // Rolling knocks down: effects are applied before their countdown.
                    .before(crate::status_effects::StatusSystems)
                    .before(HealthSystems),
            )
            .add_systems(PostUpdate, fire_overlays.before(NetServerSystems::Send));
    }
}

/// Every `mid_length` each hot group plays a random fire clip at its middle, carrying
/// `drop_off_dist` further and fading from that far (tg `update_sound`).
// shortcut: one clock for every group (tg times each loop from its start); fine while the
// clips last as long as the loop.
fn fire_sound_loops(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs_f64();
    {
        let mut clocks = world.resource_mut::<FireClocks>();
        clocks.sound += delta;
        if clocks.sound < FIRE_LOOP_SECS {
            return;
        }
        clocks.sound -= FIRE_LOOP_SECS;
    }
    let Some(sounds) = world.get_resource::<Atmos>().map(Atmos::fire_sounds) else {
        return;
    };
    for (pos, drop_off) in sounds {
        let files = ss13_mapload::sound::files::FIRE_LOOP;
        let clip = files[world
            .resource_mut::<CombatRng>()
            .range(0, files.len() as u32 - 1) as usize];
        let drop_off = drop_off as f32;
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, clip, FIRE_LOOP_VOLUME, false)
                .extrarange(drop_off)
                .falloff_distance(drop_off),
        );
    }
}

fn network_hotspot(add: On<Add<HotspotEffect>>, mut commands: Commands) {
    commands.entity(add.entity).insert(Networked);
}

/// Where an item's flame touches the air: its own tile, or its holder's when held in a hand or
/// worn on the mask (`open_flame`'s `mob_slots`).
pub(crate) fn flame_turf(world: &World, item: Entity) -> Option<TilePos> {
    if let Some(&pos) = world.get::<TilePos>(item) {
        return Some(pos);
    }
    let holder = world.get::<CarriedBy>(item)?.0;
    let slot = world.get::<Inventory>(holder)?.slot_of(item)?;
    matches!(
        slot,
        InventorySlot::LeftHand | InventorySlot::RightHand | InventorySlot::Mask
    )
    .then(|| world.get::<TilePos>(holder).copied())?
}

/// `SSobj` -> `/obj/item/weldingtool/process` -> `open_flame()` of every lit welder, and
/// `/obj/item/tank/process` -> `air_contents.react()` and `handle_tolerances`
/// ([`crate::explosions::sources::tank_tolerances`]).
// Cost: once a second, one pass over welders (dozens) and gas tanks (hundreds); a tank of
// oxygen or air returns from `react` after one pass over its gases.
fn open_flames(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs_f64();
    {
        let mut clocks = world.resource_mut::<FireClocks>();
        clocks.flames += delta;
        if clocks.flames < SSOBJ_WAIT_SECS {
            return;
        }
        clocks.flames -= SSOBJ_WAIT_SECS;
    }
    let welders: Vec<Entity> = world
        .query::<(Entity, &Welder)>()
        .iter(world)
        .filter(|(_, w)| w.on)
        .map(|(e, _)| e)
        .collect();
    let turfs: Vec<TilePos> = welders
        .into_iter()
        .filter_map(|w| flame_turf(world, w))
        .collect();
    if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
        for pos in turfs {
            atmos.hotspot_expose(pos, OPEN_FLAME_HEAT, OPEN_FLAME_VOLUME, false);
        }
    }
    let mut tanks = world.query::<(Entity, &mut TankAir)>();
    let mut reacted_tanks = Vec::new();
    for (e, mut tank) in tanks.iter_mut(world) {
        let tank_air = tank.bypass_change_detection();
        let volume = tank_air.volume;
        let mut reacted = 0;
        if tank_air.air.react_recording(
            Holder::Other,
            volume,
            &mut Exposures::default(),
            &mut reacted,
        ) != 0
        {
            tank.set_changed();
        }
        if reacted != 0 {
            reacted_tanks.push((e, reacted));
        }
    }
    // `atmos_reaction_recorder`: what reacted, for the doppler arrays.
    for (e, reacted) in reacted_tanks {
        crate::explosions::sources::record_reactions(world, e, reacted);
    }
    crate::explosions::sources::tank_tolerances(world, SSOBJ_WAIT_SECS as f32);
}

/// Hotspot exposures: `fire_act(temperature, volume)` of everything on the tile.
fn fire_acts(world: &mut World, mut cursor: Local<MessageCursor<FireAct>>) {
    // Servers without `AtmosPlugin` (fixtures) have no fires.
    if !world.contains_resource::<Messages<FireAct>>() {
        return;
    }
    let acts: Vec<FireAct> = cursor
        .read(world.resource::<Messages<FireAct>>())
        .copied()
        .collect();
    for act in acts {
        let things: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(act.pos).to_vec();
        for thing in things {
            fire_act(world, thing, act.temperature);
        }
    }
}

/// `/atom/proc/fire_act` of one thing: living mobs catch fire, flammable objects burn.
pub fn fire_act(world: &mut World, thing: Entity, temperature: f64) {
    if world.get_entity(thing).is_err() {
        return;
    }
    if world.get::<Health>(thing).is_some() {
        adjust_fire_stacks(world, thing, FIRE_ACT_STACKS);
        ignite_mob(world, thing);
    } else if crate::explosions::sources::is_fueltank(world, thing) {
        // `fueltank/fire_act`: it blows up.
        crate::explosions::sources::boom(world, thing);
    } else if crate::fireplace::fire_act(world, thing) {
        // `bonfire/fire_act`: it lights.
    } else if crate::sheets::fire_act(world, thing, temperature) {
        // Plasma sheets, hot ice and coal flood their tile and are gone.
    } else if crate::service::flooder_fire_act(world, thing, temperature) {
        // `combustible_flooder/flame_react`: plasma statues flood and are gone.
    } else if crate::hazard_decals::fire_act(world, thing) {
        // `fuel_pool/fire_act`: it lights.
    } else if crate::smoking::fire_act(world, thing) {
        // Matches and cigarettes light.
    } else if crate::toys::fire_act(world, thing) {
        // Snap pops pop.
    } else if world.get::<DmObj>(thing).is_some() {
        obj_fire_act(world, thing, temperature);
    }
}

/// `on_entered` of a hotspot: a living mob walking into a (hot) fire.
fn entered_fire(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        if world.get::<Health>(m.entity).is_none() {
            continue;
        }
        if world.get::<Rolling>(m.entity).is_some() {
            stop_rolling(world, m.entity);
        }
        let Some(temperature) = world
            .get_resource::<Atmos>()
            .and_then(|a| a.hotspot(m.to))
            .filter(|h| !h.cold_fire)
            .map(|h| h.temperature)
        else {
            continue;
        };
        fire_act(world, m.entity, temperature);
    }
}

/// tg `adjust_fire_stacks` -> `apply_status_effect(fire_stacks, stacks)`: refreshes the effect
/// (clamped to 0..[`MAX_FIRE_STACKS`]) or creates it.
pub fn adjust_fire_stacks(world: &mut World, mob: Entity, stacks: f32) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let current = world.get::<FireStacks>(mob).map(|f| f.stacks);
    let stacks = if stacks < 0.0 {
        stacks.max(-current.unwrap_or(0.0))
    } else {
        stacks
    };
    match current {
        Some(current) => {
            let mut fire = world.get_mut::<FireStacks>(mob).expect("read above");
            fire.stacks = (current + stacks).clamp(0.0, MAX_FIRE_STACKS);
        }
        None => {
            world.entity_mut(mob).insert(FireStacks {
                stacks: stacks.clamp(0.0, MAX_FIRE_STACKS),
                on_fire: false,
                next_tick: now + FIRE_TICK_SECS,
                lit_by_fire: false,
            });
            // `fire_handler/on_creation`: the wet stacks (`enemy_types`) cancel stack for stack.
            if let Some(wet) = world.get::<WetStacks>(mob).map(|w| w.stacks) {
                let fire = stacks.clamp(0.0, MAX_FIRE_STACKS);
                set_wet(world, mob, wet - fire);
                if let Some(mut f) = world.get_mut::<FireStacks>(mob) {
                    f.stacks = (fire - wet).max(0.0);
                }
            }
        }
    }
    cache_stacks(world, mob);
}

/// The wet stacks become `stacks` (removed at 0).
fn set_wet(world: &mut World, mob: Entity, stacks: f32) {
    if stacks <= 0.0 {
        world.entity_mut(mob).remove::<WetStacks>();
    } else if let Some(mut wet) = world.get_mut::<WetStacks>(mob) {
        wet.stacks = stacks.min(MAX_FIRE_STACKS);
    }
}

/// tg `adjust_wet_stacks` -> `apply_status_effect(wet_stacks, stacks)`: refreshes the effect
/// (clamped to 0..[`MAX_FIRE_STACKS`]) or creates it, a new one cancelling the fire stacks
/// stack for stack (and putting the fire out when none are left).
pub fn adjust_wet_stacks(world: &mut World, mob: Entity, stacks: f32) {
    if world.get::<Health>(mob).is_none() {
        return;
    }
    if let Some(current) = world.get::<WetStacks>(mob).map(|w| w.stacks) {
        set_wet(world, mob, (current + stacks).clamp(0.0, MAX_FIRE_STACKS));
        return;
    }
    if stacks <= 0.0 {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let wet = stacks.min(MAX_FIRE_STACKS);
    let fire = world.get::<FireStacks>(mob).map_or(0.0, |f| f.stacks);
    if fire > 0.0 {
        let left = fire - wet;
        if left <= 0.0 {
            remove_fire(world, mob);
        } else {
            world.get_mut::<FireStacks>(mob).expect("read above").stacks = left;
            cache_stacks(world, mob);
        }
    }
    if wet - fire > 0.0 {
        world.entity_mut(mob).insert(WetStacks {
            stacks: wet - fire,
            next_tick: now + FIRE_TICK_SECS,
        });
    }
}

/// tg `set_wet_stacks(stacks, remove_fire_stacks = TRUE)`: the fire stacks go, the wet stacks
/// become `stacks` (forced: nothing cancels).
pub fn set_wet_stacks(world: &mut World, mob: Entity, stacks: f32) {
    if world.get::<Health>(mob).is_none() {
        return;
    }
    remove_fire(world, mob);
    if stacks <= 0.0 {
        world.entity_mut(mob).remove::<WetStacks>();
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let next_tick = world
        .get::<WetStacks>(mob)
        .map_or(now + FIRE_TICK_SECS, |w| w.next_tick);
    world.entity_mut(mob).insert(WetStacks {
        stacks: stacks.min(MAX_FIRE_STACKS),
        next_tick,
    });
}

/// The mob's wet stacks (0 when dry).
pub fn wet_stacks(world: &World, mob: Entity) -> f32 {
    world.get::<WetStacks>(mob).map_or(0.0, |w| w.stacks)
}

/// `wet_stacks/tick` of every wet mob, every [`FIRE_TICK_SECS`] of its own: the stacks dry off.
/// Cost: per tick, one pass over wet mobs only.
fn wet_stacks_tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<(Entity, f32)> = world
        .query::<(Entity, &WetStacks)>()
        .iter(world)
        .filter(|(_, w)| now >= w.next_tick)
        .map(|(e, w)| (e, w.stacks))
        .collect();
    for (mob, stacks) in due {
        world
            .get_mut::<WetStacks>(mob)
            .expect("queried above")
            .next_tick += FIRE_TICK_SECS;
        set_wet(
            world,
            mob,
            stacks + WET_STACK_DECAY_RATE * FIRE_TICK_SECS as f32,
        );
    }
}

/// `fire_stacks/get_examine_text` (not burning) and `wet_stacks/get_examine_text`.
pub fn examine_text(world: &World, mob: Entity) -> Option<&'static str> {
    if let Some(fire) = world.get::<FireStacks>(mob)
        && !fire.on_fire
        && fire.stacks > 0.0
    {
        return Some("Он покрыт чем-то легковоспламеняющимся.");
    }
    let wet = world.get::<WetStacks>(mob)?.stacks;
    Some(if wet <= WET_STACKS_DAMP {
        "Он выглядит немного влажным."
    } else if wet >= WET_STACKS_SOAKED {
        "Он выглядит насквозь промокшим."
    } else {
        "Он выглядит промокшим до нитки."
    })
}

/// tg `spreadFire` (a living mob bumping another): two burning mobs share their stacks evenly; a
/// burning one gives half of its stacks to the other and lights it. Silicons and megafauna
/// (`TRAIT_NOFIRE_SPREAD`) neither give nor take.
pub fn spread_fire(world: &mut World, mob: Entity, to: Entity) {
    let no_spread = |e: Entity| {
        world.get::<crate::silicon::Silicon>(e).is_some()
            || world.get::<crate::mobs::megafauna::Megafauna>(e).is_some()
    };
    if world.get::<Health>(to).is_none() || no_spread(mob) || no_spread(to) {
        return;
    }
    let stacks = |w: &World, e: Entity| w.get::<FireStacks>(e).map_or(0.0, |f| f.stacks);
    let (mine, theirs) = (stacks(world, mob), stacks(world, to));
    match (on_fire(world, mob), on_fire(world, to)) {
        (true, true) => {
            let split = (mine + theirs) / 2.0;
            adjust_fire_stacks(world, mob, split - mine);
            adjust_fire_stacks(world, to, split - theirs);
        }
        (true, false) => give_half(world, mob, to),
        (false, true) => give_half(world, to, mob),
        (false, false) => {}
    }
}

/// The burning `from` keeps half of its stacks and gives the other half to `to`, who ignites.
fn give_half(world: &mut World, from: Entity, to: Entity) {
    let stacks = world.get::<FireStacks>(from).map_or(0.0, |f| f.stacks);
    adjust_fire_stacks(world, from, -stacks / 2.0);
    let left = world.get::<FireStacks>(from).map_or(0.0, |f| f.stacks);
    adjust_fire_stacks(world, to, left);
    ignite_mob(world, to);
}

/// tg `ignite_mob` -> `ignite()`: with stacks and not burning yet.
pub fn ignite_mob(world: &mut World, mob: Entity) -> bool {
    let Some(fire) = world.get::<FireStacks>(mob) else {
        return false;
    };
    if fire.stacks <= 0.0 || fire.on_fire {
        return false;
    }
    world
        .get_mut::<FireStacks>(mob)
        .expect("read above")
        .on_fire = true;
    if let Some(&pos) = world.get::<TilePos>(mob) {
        let name = mob_name(world, mob);
        let self_client = world.get::<PlayerMob>(mob).map(|p| p.client);
        let others = clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[mob]);
        world.write_message(SendChat {
            clients: others,
            message: line(Span::Warning, format!("{name} загорается!")),
        });
        if let Some(client) = self_client {
            world.write_message(SendChat {
                clients: vec![client],
                message: line(Span::BoldWarning, "Вы загорелись!"),
            });
        }
    }
    cache_stacks(world, mob);
    true
}

/// tg `extinguish_mob()`: a burning mob goes out; its stacks stay.
pub fn extinguish_mob(world: &mut World, mob: Entity) {
    match world.get_mut::<FireStacks>(mob) {
        Some(mut fire) if fire.on_fire => fire.on_fire = false,
        _ => return,
    }
    cache_stacks(world, mob);
}

/// The fire status effect ends (`on_remove` -> `extinguish`).
fn remove_fire(world: &mut World, mob: Entity) {
    if let Some(fire) = world.entity_mut(mob).take::<FireStacks>() {
        if fire.lit_by_fire {
            world.entity_mut(mob).remove::<Optics>();
        }
        if let Some(mut alerts) = world.get_mut::<Alerts>(mob) {
            alerts.clear(AlertCategory::Fire);
        }
    }
}

/// tg `cache_stacks`: the fire alert and the mob light follow the stacks.
fn cache_stacks(world: &mut World, mob: Entity) {
    let Some(fire) = world.get::<FireStacks>(mob).cloned() else {
        return;
    };
    let burning = fire.on_fire && fire.stacks > 0.0;
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob) {
        if burning {
            alerts.throw(AlertCategory::Fire, "fire", 0);
        } else if alerts.get(AlertCategory::Fire).is_some() {
            alerts.clear(AlertCategory::Fire);
        }
    }
    if fire.on_fire {
        let percent = fire.stacks / MAX_FIRE_STACKS;
        let round = |v: f32| (v * 10.0).round() / 10.0;
        let light = LightSource {
            range: round(LIGHT_RANGE_FIRE * percent).max(1.5),
            power: round(LIGHT_POWER_FIRE * percent).max(0.5),
            color: LIGHT_COLOR_FIRE,
            height: 1.0,
            angle: 360,
            dir: Dir::South,
            offset: [0, 0],
        };
        let has_optics = world.get::<Optics>(mob).is_some();
        if fire.lit_by_fire || !has_optics {
            let optics = Optics {
                light: Some(light),
                ..default()
            };
            let mut entity = world.entity_mut(mob);
            if entity.get::<Optics>() != Some(&optics) {
                entity.insert(optics);
            }
            entity
                .get_mut::<FireStacks>()
                .expect("read above")
                .lit_by_fire = true;
        }
    } else if fire.lit_by_fire {
        world.entity_mut(mob).remove::<Optics>();
        world
            .get_mut::<FireStacks>(mob)
            .expect("read above")
            .lit_by_fire = false;
    }
}

/// `/datum/status_effect/fire_handler/fire_stacks/tick` of every mob with stacks, every
/// [`FIRE_TICK_SECS`] of its own.
fn fire_stacks_tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<Entity> = world
        .query::<(Entity, &FireStacks)>()
        .iter(world)
        .filter(|(_, f)| now >= f.next_tick)
        .map(|(e, _)| e)
        .collect();
    for mob in due {
        world
            .get_mut::<FireStacks>(mob)
            .expect("queried above")
            .next_tick += FIRE_TICK_SECS;
        fire_tick(world, mob, FIRE_TICK_SECS);
    }
}

fn fire_tick(world: &mut World, mob: Entity, spt: f64) {
    let Some(fire) = world.get::<FireStacks>(mob).cloned() else {
        return;
    };
    if fire.stacks <= 0.0 {
        remove_fire(world, mob);
        return;
    }
    if !fire.on_fire {
        return;
    }
    adjust_fire_stacks(world, mob, FIRE_STACK_DECAY_RATE * spt as f32);
    let stacks = world.get::<FireStacks>(mob).map_or(0.0, |f| f.stacks);
    if stacks <= 0.0 {
        remove_fire(world, mob);
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    // A level without simulated air is breathable station air (`crate::breath`).
    let oxygen = world.get_resource::<Atmos>().and_then(|a| {
        a.level(pos.z)
            .map(|_| a.moles(pos, GasId::O2).unwrap_or(0.0))
    });
    if oxygen.is_some_and(|o2| o2 < 1.0) {
        remove_fire(world, mob);
        return;
    }
    // `deal_damage`: `on_fire_stack` -> `harm_human`, then the turf.
    harm_human(world, mob, stacks, spt);
    if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
        atmos.hotspot_expose(pos, 700.0, 25.0 * spt, true);
    }
}

/// tg `get_thermal_protection`: fire suits and helmets.
pub fn thermal_protection(world: &World, mob: Entity) -> f32 {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return 0.0;
    };
    let clothing = |slot| {
        inventory
            .slot(slot)
            .and_then(|e| world.get::<DmClothing>(e))
    };
    let mut protection = 0.0;
    if let Some(suit) = clothing(InventorySlot::Suit)
        && suit.heat_protection & CHEST != 0
        && suit.max_heat_protection_temperature >= FIRE_SUIT_MAX_TEMP_PROTECT
    {
        protection += suit.max_heat_protection_temperature * 0.7;
    }
    if let Some(head) = clothing(InventorySlot::Head)
        && head.heat_protection & HEAD != 0
        && head.max_heat_protection_temperature >= FIRE_HELM_MAX_TEMP_PROTECT
    {
        protection += head.max_heat_protection_temperature * THERMAL_PROTECTION_HEAD;
    }
    protection.round()
}

/// `harm_human`: the fire heats the body (unless fireproof clothing covers it).
fn harm_human(world: &mut World, mob: Entity, stacks: f32, spt: f64) {
    let protection = thermal_protection(world, mob);
    let Some(mut resp) = world.get_mut::<Respiration>(mob) else {
        return;
    };
    if protection >= FIRE_IMMUNITY_MAX_TEMP_PROTECT {
        return;
    }
    if protection >= FIRE_SUIT_MAX_TEMP_PROTECT {
        resp.bodytemperature += 5.5 * spt;
        return;
    }
    let mut amount = (BODYTEMP_HEATING_MAX + f64::from(stacks) * 12.0) * 0.5 * spt;
    if resp.bodytemperature > BODYTEMP_FIRE_TEMP_SOFTCAP {
        amount = amount.powf(BODYTEMP_FIRE_TEMP_SOFTCAP / resp.bodytemperature);
    }
    resp.bodytemperature += amount;
}

/// Whether the mob is burning (tg `on_fire`).
pub fn on_fire(world: &World, mob: Entity) -> bool {
    world.get::<FireStacks>(mob).is_some_and(|f| f.on_fire)
}

/// `resist_fire` -> `apply_status_effect(/datum/status_effect/stop_drop_roll)`: the burning
/// mob drops (`Knockdown(1.6 s, ignore_canstun)`) and rolls. False when it is not burning.
pub fn resist_fire(world: &mut World, mob: Entity) -> bool {
    if !on_fire(world, mob) {
        return false;
    }
    if world.get::<Rolling>(mob).is_some() {
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    crate::status_effects::knockdown(world, mob, (ROLL_TICK_SECS * 2.0) as f32, 0.0);
    world.entity_mut(mob).insert(Rolling {
        next_tick: now + ROLL_TICK_SECS,
    });
    self_and_others(
        world,
        mob,
        (
            Span::Notice,
            "Вы останавливаетесь, падаете и катаетесь!".into(),
        ),
        format!(
            "{} катается по полу, пытаясь потушить себя!",
            mob_name(world, mob)
        ),
    );
    // "Start with one weaker roll."
    adjust_fire_stacks(world, mob, -0.25);
    true
}

/// `stop_rolling`: moving ends the roll (the knockdown runs out on its own).
fn stop_rolling(world: &mut World, mob: Entity) {
    world.entity_mut(mob).remove::<Rolling>();
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(SendChat {
            clients: vec![client],
            message: line(Span::Notice, "Вы прекращаете кататься."),
        });
    }
}

/// `/datum/status_effect/stop_drop_roll/tick`.
// shortcut: no progress bar.
fn rolling_tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<Entity> = world
        .query::<(Entity, &Rolling)>()
        .iter(world)
        .filter(|(_, r)| now >= r.next_tick)
        .map(|(e, _)| e)
        .collect();
    for mob in due {
        let traits = world
            .get::<crate::status_effects::Incapacitation>(mob)
            .copied()
            .unwrap_or_default();
        let knocked_out = match (world.get::<Health>(mob), world.get::<Stat>(mob)) {
            (Some(h), Some(&s)) => h.knocked_out(s),
            _ => true,
        };
        if traits.immobilized || traits.incapacitated || knocked_out {
            world.entity_mut(mob).remove::<Rolling>();
            continue;
        }
        world.get_mut::<Rolling>(mob).expect("queried").next_tick += ROLL_TICK_SECS;
        crate::status_effects::knockdown(world, mob, (ROLL_TICK_SECS * 1.2) as f32, 0.0);
        adjust_fire_stacks(world, mob, -1.0);
        if world.get::<FireStacks>(mob).is_none_or(|f| f.stacks <= 0.0) {
            remove_fire(world, mob);
            world.entity_mut(mob).remove::<Rolling>();
            self_and_others(
                world,
                mob,
                (Span::Notice, "Вы потушили себя.".into()),
                format!("{} успешно тушит себя!", mob_name(world, mob)),
            );
        }
    }
}

/// `/obj/fire_act`: fire damage (`0.02 * temperature`, at most 20) and, for flammable things,
/// the burning component.
// shortcut: only flammable objects take fire damage, without fire armor (`DmObj` carries the
// melee rating only; most flammable furniture has none); windows and other fire-resistant
// structures are spared instead of taking armored damage. Import the fire rating to extend.
fn obj_fire_act(world: &mut World, thing: Entity, temperature: f64) {
    let Some(obj) = world.get::<DmObj>(thing).cloned() else {
        return;
    };
    if obj.resistance_flags & FLAMMABLE == 0 || obj.resistance_flags & FIRE_PROOF != 0 {
        return;
    }
    let potential = (0.02 * temperature) as f32;
    if temperature > 0.0 && potential > obj.damage_deflection {
        fire_damage(world, thing, potential.clamp(0.0, 20.0));
    }
    set_burning(world, thing);
}

/// `AddComponent(/datum/component/burning)`: the thing burns with tg's fire overlay until it is
/// destroyed ([`burning_objects`]).
pub fn set_burning(world: &mut World, thing: Entity) {
    if world.get_entity(thing).is_err() || world.get::<Burning>(thing).is_some() {
        return;
    }
    world.entity_mut(thing).insert(Burning);
    let overlay = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(BURNING_DMI, "fire"));
    if let (Some(id), Some(mut appearance)) = (overlay, world.get_mut::<Appearance>(thing)) {
        appearance.overlays.push(Overlay::new(id));
    }
}

/// `/obj/extinguish`: the burning component and its overlay go.
pub fn extinguish_obj(world: &mut World, thing: Entity) {
    if world.get::<Burning>(thing).is_none() {
        return;
    }
    world.entity_mut(thing).remove::<Burning>();
    let overlay = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(BURNING_DMI, "fire"));
    if let (Some(id), Some(mut appearance)) = (overlay, world.get_mut::<Appearance>(thing)) {
        appearance.overlays.retain(|o| o.icon_state != id);
    }
}

/// `take_damage(amount, BURN, FIRE)` on a structure or machine: destroyed at 0
/// (`atom_destruction(FIRE)` -> `burn()` -> `deconstruct(FALSE)`).
pub fn fire_damage(world: &mut World, thing: Entity, amount: f32) {
    use crate::antag::blob::strains::Flag;
    if crate::antag::blob::structures::damage(
        world,
        thing,
        amount,
        crate::health::DamageType::Burn,
        Flag::Fire,
    )
    .is_some()
    {
        return;
    }
    let Some(obj) = world.get::<DmObj>(thing).cloned() else {
        // An item (set alight by lava, `crate::lava`): through its fire armor.
        if world
            .get::<DmItem>(thing)
            .is_some_and(|i| i.resistance_flags & INDESTRUCTIBLE == 0)
        {
            let armor = world
                .get::<ss13_mapload::DmArmor>(thing)
                .map_or(0.0, |a| a.fire);
            let amount = amount * (1.0 - armor / 100.0).clamp(0.0, 1.0);
            crate::explosions::ex_act::damage_item(world, thing, amount);
        }
        return;
    };
    if obj.resistance_flags & INDESTRUCTIBLE != 0 || obj.max_integrity <= 0.0 {
        return;
    }
    let mut integrity = world.get::<Integrity>(thing).copied().unwrap_or(Integrity {
        current: obj.max_integrity,
        broken: false,
    });
    if integrity.current <= 0.0 {
        return;
    }
    integrity.current = (integrity.current - amount).max(0.0);
    world.entity_mut(thing).insert(integrity);
    if integrity.current <= 0.0 {
        deconstruct(world, thing, false);
    }
}

/// `SSburning` -> `/datum/component/burning/process`.
fn burning_objects(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs_f64();
    {
        let mut clocks = world.resource_mut::<FireClocks>();
        clocks.burning += delta;
        if clocks.burning < SSOBJ_WAIT_SECS {
            return;
        }
        clocks.burning -= SSOBJ_WAIT_SECS;
    }
    let burning: Vec<Entity> = world
        .query_filtered::<Entity, With<Burning>>()
        .iter(world)
        .collect();
    for thing in burning {
        fire_damage(
            world,
            thing,
            BURNING_DAMAGE_PER_SEC * SSOBJ_WAIT_SECS as f32,
        );
    }
}

/// tg `get_fire_overlay` of a human: `human_small_fire` / `human_big_fire` above everything,
/// on top of whatever built the appearance (idempotent, like `crate::health`'s lying look).
fn fire_overlays(
    mut mobs: Query<(Option<Ref<FireStacks>>, &mut Appearance), With<Health>>,
    mut removed: RemovedComponents<FireStacks>,
    icons: Option<Res<IconIndex>>,
) {
    let Some(icons) = icons else {
        return;
    };
    let states = [
        icons.get(ONFIRE_DMI, "human_small_fire"),
        icons.get(ONFIRE_DMI, "human_big_fire"),
    ];
    let is_fire = |o: &Overlay| states.contains(&Some(o.icon_state));
    let removed: Vec<Entity> = removed.read().collect();
    // Cost: one pass over living mobs (one per player) per tick, writing only on a change.
    for (fire, mut appearance) in &mut mobs {
        if !appearance.is_changed() && fire.as_ref().is_none_or(|f| !f.is_changed()) {
            continue;
        }
        let wanted = fire
            .filter(|f| f.on_fire && f.stacks > 0.0)
            .and_then(|f| states[usize::from(f.stacks > MOB_BIG_FIRE_STACK_THRESHOLD)]);
        let current = appearance
            .overlays
            .iter()
            .find(|o| is_fire(o))
            .map(|o| o.icon_state);
        if current == wanted {
            continue;
        }
        appearance.overlays.retain(|o| !is_fire(o));
        if let Some(id) = wanted {
            appearance.overlays.push(Overlay::new(id));
        }
    }
    for mob in removed {
        if let Ok((None, mut appearance)) = mobs.get_mut(mob)
            && appearance.overlays.iter().any(is_fire)
        {
            appearance.overlays.retain(|o| !is_fire(o));
        }
    }
}

/// tg `pressure_resistance` of what space wind can move, `None` for what it never moves
/// (anchored things, turfs, effects, ghosts).
fn pressure_resistance(world: &World, thing: Entity) -> Option<f64> {
    let entity = world.get_entity(thing).ok()?;
    if let Some(item) = entity.get::<DmItem>() {
        return (!item.anchored).then_some(ITEM_PRESSURE_RESISTANCE);
    }
    if entity.contains::<Health>() {
        return Some(HUMAN_PRESSURE_RESISTANCE);
    }
    let obj = entity.get::<DmObj>()?;
    if entity
        .get::<crate::obj_damage::Anchored>()
        .is_none_or(|a| a.0)
    {
        return None;
    }
    Some(if is_under(&obj.path, "/obj/machinery") {
        MACHINERY_PRESSURE_RESISTANCE
    } else {
        STRUCTURE_PRESSURE_RESISTANCE
    })
}

/// `SSair` `process_high_pressure_delta` -> `high_pressure_movements` of every turf that moved
/// air this fire: each unanchored, unpulled movable on it rolls tg's `move_prob` once per fire
/// (`last_high_pressure_movement_air_cycle`) and steps along the pressure direction.
// Cost: per air fire, the turfs that moved enough air (a breach: tens to hundreds) times the
// things on them.
// shortcut: no buckling yet, so every mob but a magbooted one (`mob_negates_gravity`) can be
// blown away; tg knocks nobody down here (a strong wind only moves you more often).
fn space_wind(
    world: &mut World,
    mut cursor: Local<MessageCursor<PressureDifference>>,
    mut moved_in: Local<Vec<Entity>>,
) {
    if !world.contains_resource::<Messages<PressureDifference>>() {
        return;
    }
    let pushes: Vec<PressureDifference> = cursor
        .read(world.resource::<Messages<PressureDifference>>())
        .copied()
        .collect();
    if pushes.is_empty() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    moved_in.clear();
    for push in pushes {
        let things: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(push.pos).to_vec();
        for thing in things {
            if moved_in.contains(&thing) || world.get::<PulledBy>(thing).is_some() {
                continue;
            }
            if world.get::<Health>(thing).is_some()
                && crate::spacemove::negates_gravity(world, thing)
            {
                continue;
            }
            let Some(resistance) = pressure_resistance(world, thing) else {
                continue;
            };
            let mut prob_delta = 0.0;
            // `/mob/living/experience_pressure_difference`: a mob that just stepped holds on.
            if world.get::<PlayerMob>(thing).is_some()
                && world
                    .get::<Mover>(thing)
                    .is_some_and(|m| m.ready_at() >= now + TICK_LAG_SECS * 2.0)
            {
                prob_delta -= 30.0;
            }
            if experience_pressure_difference(world, push.difference, resistance, prob_delta)
                && step_once(world, thing, push.dir, None).is_ok()
            {
                moved_in.push(thing);
            }
        }
    }
}

/// The roll of `/atom/movable/experience_pressure_difference` for an unanchored movable.
pub fn experience_pressure_difference(
    world: &mut World,
    difference: f64,
    resistance: f64,
    prob_delta: f64,
) -> bool {
    const PROBABILITY_OFFSET: f64 = 25.0;
    const PROBABILITY_BASE_PERCENT: f64 = 75.0;
    let max_force = difference.sqrt() * (MOVE_FORCE_DEFAULT / 5.0);
    let mut move_prob = 100.0;
    if resistance > 0.0 {
        move_prob = (difference / resistance * PROBABILITY_BASE_PERCENT) - PROBABILITY_OFFSET;
    }
    move_prob += prob_delta;
    move_prob > PROBABILITY_OFFSET
        && max_force >= MOVE_RESIST_DEFAULT * MOVE_FORCE_PUSH_RATIO
        && world
            .get_resource_or_init::<CombatRng>()
            .chance(move_prob as f32)
}

fn mob_name(world: &World, mob: Entity) -> String {
    world
        .get::<Name>(mob)
        .map_or_else(|| "Кто-то".to_owned(), |n| n.to_string())
}

fn line(span: Span, text: impl Into<String>) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::Local,
        segments: vec![ChatSegment::new(span, text)],
    }
}

/// `visible_message(others, self)`.
fn self_and_others(world: &mut World, mob: Entity, own: (Span, String), others: String) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let viewers = clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[mob]);
    world.write_message(SendChat {
        clients: viewers,
        message: line(Span::Danger, others),
    });
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(SendChat {
            clients: vec![client],
            message: line(own.0, own.1),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_prob_follows_tg() {
        let mut world = World::new();
        world.insert_resource(CombatRng(7));
        // Below the force threshold (sqrt(diff) * 200 < 1000, diff < 25) nothing moves.
        assert!(!experience_pressure_difference(&mut world, 24.0, 4.0, 0.0));
        // A human (25): move_prob = diff / 25 * 75 - 25, must exceed 25: diff > 16.67, and the
        // force needs 25 kPa; at 25 kPa it is 50%.
        let mut moved = 0;
        for _ in 0..1000 {
            moved += usize::from(experience_pressure_difference(&mut world, 25.0, 25.0, 0.0));
        }
        assert!((400..600).contains(&moved), "{moved}");
        // An item (4) at 101 kPa: 1868% -> always.
        assert!((0..100).all(|_| experience_pressure_difference(&mut world, 101.0, 4.0, 0.0)));
        // A human who just stepped: -30, at 25 kPa move_prob 20 < 25: never.
        assert!(!experience_pressure_difference(
            &mut world, 25.0, 25.0, -30.0
        ));
    }

    #[test]
    fn bumping_spreads_fire_like_tg() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Messages<SendChat>>();
        let stacks = |w: &World, e: Entity| w.get::<FireStacks>(e).map_or(0.0, |f| f.stacks);
        // One burning with 8: it keeps 4 and the other takes 4 and lights.
        let burning = world.spawn(Health::default()).id();
        let dry = world.spawn(Health::default()).id();
        adjust_fire_stacks(&mut world, burning, 8.0);
        ignite_mob(&mut world, burning);
        spread_fire(&mut world, dry, burning);
        assert_eq!((stacks(&world, burning), stacks(&world, dry)), (4.0, 4.0));
        assert!(on_fire(&world, dry));
        // Both burning: 4 and 10 become 7 each.
        adjust_fire_stacks(&mut world, dry, 6.0);
        spread_fire(&mut world, burning, dry);
        assert_eq!((stacks(&world, burning), stacks(&world, dry)), (7.0, 7.0));
        // Neither burning: nothing; a silicon takes no fire.
        let calm = world.spawn(Health::default()).id();
        let other = world.spawn(Health::default()).id();
        spread_fire(&mut world, calm, other);
        assert!(world.get::<FireStacks>(other).is_none());
        let borg = world
            .spawn((Health::default(), crate::silicon::Silicon))
            .id();
        spread_fire(&mut world, burning, borg);
        assert!(world.get::<FireStacks>(borg).is_none());
        assert_eq!(stacks(&world, burning), 7.0);
    }

    #[test]
    fn wet_and_fire_stacks_cancel_and_wetness_dries() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn(Health::default()).id();
        adjust_fire_stacks(&mut world, mob, 5.0);
        assert!(ignite_mob(&mut world, mob));
        // 3 wet stacks on 5 burning ones: 2 fire stacks left, still burning, not wet.
        adjust_wet_stacks(&mut world, mob, 3.0);
        assert_eq!(world.get::<FireStacks>(mob).unwrap().stacks, 2.0);
        assert!(on_fire(&world, mob));
        assert_eq!(wet_stacks(&world, mob), 0.0);
        // 6 more: the fire is out and gone, 4 wet stacks remain.
        adjust_wet_stacks(&mut world, mob, 6.0);
        assert!(world.get::<FireStacks>(mob).is_none());
        assert_eq!(wet_stacks(&world, mob), 4.0);
        assert_eq!(
            examine_text(&world, mob),
            Some("Он выглядит промокшим до нитки.")
        );
        // New fire stacks on a wet mob are soaked up first.
        adjust_fire_stacks(&mut world, mob, 3.0);
        assert_eq!(wet_stacks(&world, mob), 1.0);
        assert_eq!(world.get::<FireStacks>(mob).unwrap().stacks, 0.0);
        assert!(!ignite_mob(&mut world, mob), "nothing to burn");
        // Drying: -0.5 a second, checked every 2 s.
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(2));
        wet_stacks_tick(&mut world);
        assert!(world.get::<WetStacks>(mob).is_none(), "1 - 1 = dry");
        adjust_wet_stacks(&mut world, mob, -5.0);
        assert!(
            world.get::<WetStacks>(mob).is_none(),
            "drying a dry mob does nothing"
        );
        set_wet_stacks(&mut world, mob, 30.0);
        assert_eq!(wet_stacks(&world, mob), MAX_FIRE_STACKS);
    }
}
