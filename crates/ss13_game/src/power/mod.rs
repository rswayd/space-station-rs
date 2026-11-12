//! Power (server): tg areas, APCs, powernets, SMES and machine loads (`code/modules/power/`).
//!
//! - Map load data is [`DmPower`] (`ss13_mapload::power`); [`init_power`] builds the live state
//!   once at startup: [`AreaPower`] on area entities, [`Apc`], [`Smes`], [`Generator`],
//!   [`PowerMachine`] and [`LightFixture`] on machines, the cable graph in [`Grid`].
//! - Every [`MACHINE_TICK_SECS`] (tg `SSmachines.wait`, 2 s) [`power_tick`] runs tg's
//!   `SSmachines.fire()` order: powernet resets (SMES take back unused output), APC early process
//!   (static loads), the four APC charge steps, APC late process (channels by cell charge), then
//!   SMES, generators and emergency lights.
//! - When an area's channels change, [`apply_area_power`] toggles [`Unpowered`] on its machines
//!   and switches its lights (normal, emergency, off). Doors read [`Unpowered`].
//!
//! Cost per machine tick (0.5 Hz): O(nets + APCs + SMES + generators + lights running on or
//! recharging an emergency cell); MetaStation is ~150 APCs and a few dozen nets, so well under
//! a millisecond. An area change touches only that area's machines and lights. Nothing scans
//! the map after startup.

pub mod accumulator;
pub mod apc_ui;
pub mod cable_coil;
pub mod chargers;
pub mod emitter;
pub mod field_generator;
pub mod grid;
pub mod lighting;
pub mod model;
pub mod pacman;
pub mod shields;
pub mod singularity;
pub mod smes;
pub mod solar;
pub mod supermatter;
pub mod teg;
pub mod tesla;

use std::collections::HashMap;
use std::sync::Arc;

use bevy::platform::time::Instant;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::light::{DmLight, MINIMUM_USEFUL_LIGHT_RANGE, source};
use ss13_mapload::power::{
    APC_HELPER_AWAY_GENERAL_ACCESS, APC_HELPER_CELL_5K, APC_HELPER_CELL_10K,
    APC_HELPER_FULL_CHARGE, APC_HELPER_NO_CHARGE, APC_HELPER_ROOT, APC_HELPER_SYNDICATE_ACCESS,
    APC_HELPER_UNLOCKED, APC_ROOT, CABLE_ROOT, LIGHT_ROOT, LIGHTS_OFF_HELPER, MACHINE_ROOT,
    RTG_ROOT, SMES_ROOT, TERMINAL_ROOT,
};
use ss13_mapload::{Area, Areas, DmAtom, DmPower};
use ss13_shared::appearance::Overlay;
use ss13_shared::{Appearance, Dir, IconStateId, LightSource, Optics, TilePos};
use ss13_world::{MapInit, Mapped};

pub use grid::{Grid, NetId, NodeKind, Powernet};
pub use model::{
    Apc, AreaPower, Cell, Channel, ChargeStep, EMERGENCY_CELL_CHARGE, LIGHT_EMERGENCY_POWER_USE,
    LightMode, Smes,
};

/// tg `SSmachines.wait`: machines and power process every 2 seconds.
pub const MACHINE_TICK_SECS: f64 = 2.0;
/// Machine ticks one frame may catch up after a stall; the rest is dropped, like tg's MC
/// skipping a late subsystem fire.
const MAX_CATCH_UP: u32 = 3;

/// tg `COLOR_SOFT_RED`, `LIGHT_COLOR_BLUE`, `LIGHT_COLOR_GREEN`: the APC's light by charging
/// state.
const APC_LIGHT_COLORS: [[u8; 4]; 3] = [
    [0xFA, 0x82, 0x82, 255],
    [0x64, 0x96, 0xFA, 255],
    [0x64, 0xC8, 0x64, 255],
];
/// `/obj/machinery/power/apc` `light_on_range`.
const APC_LIGHT_RANGE: f32 = 1.5;
/// `/obj/machinery/light` emergency light: `bulb_low_power_brightness_mul`,
/// `bulb_low_power_pow_mul`, `bulb_low_power_pow_min`, `bulb_low_power_colour` (no subtype
/// overrides them).
const EMERGENCY_BRIGHTNESS_MUL: f32 = 0.25;
const EMERGENCY_POW_MUL: f32 = 0.75;
const EMERGENCY_POW_MIN: f32 = 0.5;
const EMERGENCY_COLOR: [u8; 3] = [0xFF, 0x32, 0x32];

/// On a machine whose area has no power on its channel (tg `machine_stat & NOPOWER`). Only
/// machines that use power get it (tg `powered()` is always true for `NO_POWER_USE`).
///
/// The hook for other features: doors refuse to move without it; vending machines should check
/// it before vending and drop their light (tg `/obj/machinery/vending/power_change`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Unpowered;

/// A machine drawing from its area's APC.
#[derive(Component, Clone, Copy, Debug)]
pub struct PowerMachine {
    pub channel: Channel,
    pub area: Entity,
    /// Static load while it runs, watts (tg `idle_power_usage` / `active_power_usage` by
    /// `use_power`).
    pub watts: f64,
}

/// Puts a constant power into its powernet (tg RTG `process()`: `add_avail`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Generator {
    pub watts: f64,
}

/// A light fixture with a working bulb (tg `/obj/machinery/light`).
#[derive(Component, Clone, Debug)]
pub struct LightFixture {
    pub area: Entity,
    pub mode: LightMode,
    /// The light it emits when on; `None`: no working bulb (never lights).
    pub on_light: Option<LightSource>,
    pub brightness: f32,
    /// Emergency cell charge; `None`: no cell (`no_low_power`, `start_with_cell = FALSE`).
    pub emergency: Option<f64>,
    /// Static lighting load while on, watts (`brightness * bulb_power * power_consumption_rate`).
    pub watts: f64,
    /// `base_state` and `"[base_state]_emergency"` icon states.
    pub states: [Option<IconStateId>; 2],
    /// The light while on in an area with a fire alarm (`fire_brightness`, `fire_power`,
    /// `fire_colour`), and whether the area has one now (tg `local_area.fire`).
    pub fire_light: Option<LightSource>,
    pub fire: bool,
    /// The light while on with its APC's night shift on (`nightshift_brightness`,
    /// `nightshift_light_power`, `nightshift_light_color`), and whether it is on now (tg
    /// `nightshift_enabled`, [`set_nightshift`]).
    pub night_light: Option<LightSource>,
    pub nightshift: bool,
    /// tg `status`: `on_light` is the template only with a working bulb.
    pub status: lighting::LightStatus,
    /// The light with a working bulb of `brightness`, whatever bulb is in now.
    pub template: Option<LightSource>,
    /// `"[base_state]-empty"`, `-broken`, `-burned`.
    pub status_states: [Option<IconStateId>; 3],
    /// `overlay_icon` states: `base_state`, `_emergency`, `_nightshift`, `_flickering`.
    pub overlay_states: [Option<IconStateId>; 4],
    /// Takes `/obj/item/light/bulb` (small and floor lights), else tubes (`light_type`).
    pub bulb: bool,
    pub switchcount: u32,
    /// tg `flickering` ([`lighting::flicker`]).
    pub flickering: bool,
    pub integrity: f32,
    /// For the load of a new bulb: `bulb_power`, `power_consumption_rate`.
    pub bulb_power: f32,
    pub consumption_rate: f32,
}

impl LightFixture {
    /// tg `use_emergency_power()` light: a quarter of the range, red, dimming with the cell.
    // shortcut: power is rounded to 0.05 so a draining cell changes the replicated light ~5
    // times instead of every tick; drop the rounding if the dimming needs to be smooth.
    fn emergency_light(&self) -> Option<LightSource> {
        let on = self.on_light?;
        let fraction = (self.emergency.unwrap_or(0.0) / EMERGENCY_CELL_CHARGE) as f32;
        let power = (EMERGENCY_POW_MUL * fraction).max(EMERGENCY_POW_MIN);
        Some(LightSource {
            range: (self.brightness * EMERGENCY_BRIGHTNESS_MUL).max(MINIMUM_USEFUL_LIGHT_RANGE),
            power: (power * 20.0).round() / 20.0,
            color: EMERGENCY_COLOR,
            ..on
        })
    }

    fn light(&self) -> Option<LightSource> {
        match self.mode {
            LightMode::On if self.status != lighting::LightStatus::Ok => None,
            // tg `update()` while `flickering`: a quarter of the range, the night colour.
            LightMode::On if self.flickering => self.on_light.map(|on| LightSource {
                range: self.brightness * EMERGENCY_BRIGHTNESS_MUL,
                power: EMERGENCY_POW_MUL,
                color: self.night_light.map_or([0xFF, 0xDD, 0xCC], |n| n.color),
                ..on
            }),
            LightMode::On if self.fire => self.fire_light,
            LightMode::On if self.nightshift => self.night_light,
            LightMode::On => self.on_light,
            LightMode::Emergency => self.emergency_light(),
            LightMode::Off => None,
        }
    }

    /// tg `update_overlays()`: only a fixture that is `on` with a whole bulb glows. Emergency
    /// mode is `on = FALSE` (`power_change`), its red is the icon state alone.
    fn overlays(&self) -> Vec<Overlay> {
        if self.mode != LightMode::On || self.status != lighting::LightStatus::Ok {
            return Vec::new();
        }
        let [base, emergency, nightshift, flickering] = self.overlay_states;
        let tint = if self.flickering {
            flickering
        } else if self.fire {
            emergency
        } else if self.nightshift {
            nightshift
        } else {
            base
        };
        let glow = base.map(|state| Overlay {
            plane: Some(ss13_shared::EMISSIVE_PLANE),
            ..Overlay::new(state)
        });
        glow.into_iter().chain(tint.map(Overlay::new)).collect()
    }
}

/// On fixtures processed every machine tick: running on, or recharging, the emergency cell.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct LightProcessing;

/// An area's machines and lights, for fan-out when its channels change.
#[derive(Component, Clone, Debug, Default)]
pub struct AreaMembers {
    pub machines: Vec<(Entity, Channel)>,
    pub lights: Vec<Entity>,
}

/// APC overlay sprites (tg `update_overlays`) and the last drawn state.
#[derive(Component, Clone, Debug)]
pub struct ApcLook {
    key: Option<u16>,
    sprites: Arc<ApcSprites>,
    /// The APC's light vars, its `dir` and pixel offset, for `set_light` by charging state.
    light: DmLight,
    path: String,
    dir: u8,
    pixel: [i16; 2],
}

#[derive(Debug, Default)]
struct ApcSprites {
    /// `apcox-[locked]`.
    lock: [Option<IconStateId>; 2],
    /// `apco3-[charging]`.
    charging: [Option<IconStateId>; 3],
    /// `apco0-[equipment]`, `apco1-[lighting]`, `apco2-[environ]`, indexed by [`Channel`] order
    /// of tg's file: equipment, lighting, environ.
    channels: [[Option<IconStateId>; 4]; 3],
}

/// An area's channels changed: its machines and lights follow.
#[derive(Message, Clone, Copy, Debug)]
pub struct AreaPowerChanged(pub Entity);

#[derive(Resource, Default)]
struct PowerClock {
    accumulated: f64,
}

/// tg `SSmachines.times_fired`: machine ticks so far.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MachineTicks(pub u64);

/// DM `rand`/`prob` for the engine machines (emitter pauses, supermatter arcs).
#[derive(Resource, Debug, Default)]
pub struct EngineRng(pub crate::combat::CombatRng);

/// The generation machines of the machine tick: emitters (early, before the APCs), then solar
/// panels, solar controls and energy accumulators (with the other machines).
#[derive(bevy::ecs::system::SystemParam)]
pub struct Generation<'w, 's> {
    ticks: ResMut<'w, MachineTicks>,
    rng: ResMut<'w, EngineRng>,
    emitters: Query<'w, 's, (Entity, &'static TilePos, &'static mut emitter::Emitter)>,
    beams: ResMut<'w, emitter::PendingBeams>,
    accumulators: Query<'w, 's, (Entity, &'static mut accumulator::Accumulator)>,
    solars: solar::Solars<'w, 's>,
    tegs: Query<'w, 's, (Entity, &'static mut teg::Teg)>,
    pacmans: Query<
        'w,
        's,
        (
            Entity,
            &'static mut pacman::Pacman,
            Option<&'static crate::obj_damage::Anchored>,
        ),
    >,
    shield_walls: Query<'w, 's, (Entity, &'static mut shields::ShieldWallGen)>,
}

impl Generation<'_, '_> {
    /// tg `processing_early`: emitters draw their power and fire, shield wall generators draw.
    fn process_early(&mut self, grid: &mut Grid, now: f64, dt: f64) {
        self.ticks.0 += 1;
        for (e, &pos, mut emitter) in &mut self.emitters {
            let net = grid.node_net(e).and_then(|n| grid.net_mut(n));
            if emitter.process_early(net, now, dt, &mut self.rng.0) {
                self.beams.0.push((e, pos, emitter::dir2angle(emitter.dir)));
            }
        }
        // `shieldwallgen/process_early`: an active generator's draw (its walls in `shields`).
        for (e, mut gen_) in &mut self.shield_walls {
            let net = grid.node_net(e).and_then(|n| grid.net_mut(n));
            gen_.draw_power(net, dt);
        }
    }

    /// tg `processing`: solars, accumulators and thermoelectric generators feed their
    /// powernets.
    fn process(&mut self, grid: &mut Grid, dt: f64) {
        self.solars.process(grid);
        for (e, mut teg) in &mut self.tegs {
            teg::machine_tick(grid, e, &mut teg);
        }
        // `port_gen/process`: `if(powernet) add_avail(...)`, the fuel burnt either way.
        for (e, mut pac, anchored) in &mut self.pacmans {
            let joules = pac.process(anchored.is_some_and(|a| a.0), dt, &mut self.rng.0);
            if joules > 0.0
                && let Some(net) = grid.node_net(e).and_then(|n| grid.net_mut(n))
            {
                net.newavail += joules;
            }
        }
        for (e, mut acc) in &mut self.accumulators {
            let joules = acc.process(dt);
            if let Some(net) = grid.node_net(e).and_then(|n| grid.net_mut(n)) {
                net.newavail += joules;
            }
        }
    }
}

/// What [`init_power`] found.
#[derive(Resource, Debug, Default, Clone)]
pub struct PowerReport {
    pub cables: usize,
    pub nets: usize,
    pub apcs: usize,
    /// APCs whose terminal has no cable under it.
    pub apcs_unwired: usize,
    /// Areas that need power (`requires_power`, not `always_unpowered`) with entities on the
    /// map, and those of them with an APC.
    pub areas_needing_power: usize,
    pub areas_with_apc: usize,
    /// Areas that got a second APC (tg logs it as a mapping error; the last one wins).
    pub duplicate_apcs: usize,
    pub smes: usize,
    /// SMES without a terminal (tg breaks them).
    pub smes_broken: usize,
    pub generators: usize,
    pub machines: usize,
    pub lights: usize,
    /// Sum of the static machine loads, watts.
    pub static_watts: f64,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PowerSystems;

pub struct PowerServerPlugin;

impl Plugin for PowerServerPlugin {
    fn build(&self, app: &mut App) {
        // Maps load in `PreStartup`.
        app.init_resource::<Grid>()
            .init_resource::<PowerClock>()
            .init_resource::<PowerReport>()
            .init_resource::<MachineTicks>()
            .init_resource::<EngineRng>()
            .add_message::<AreaPowerChanged>()
            .add_plugins((
                solar::SolarServerPlugin,
                emitter::EmitterServerPlugin,
                field_generator::FieldGeneratorPlugin,
                pacman::PacmanPlugin,
                smes::SmesPlugin,
                shields::ShieldsPlugin,
                supermatter::SupermatterServerPlugin,
                chargers::ChargersPlugin,
                apc_ui::ApcUiPlugin,
                teg::TegPlugin,
                lighting::LightingPlugin,
                singularity::SingularityPlugin,
                tesla::TeslaPlugin,
            ))
            .add_systems(
                MapInit,
                (init_power, accumulator::init_accumulators).chain(),
            )
            .add_systems(
                Update,
                (power_tick, apply_area_power, update_apc_looks)
                    .chain()
                    .in_set(PowerSystems),
            );
    }
}

fn layer(dm: &DmPower) -> u8 {
    dm.cable_layer.clamp(0.0, 255.0) as u8
}

/// Live power state from [`DmPower`] of the map pass; a later load's cables join the grid.
// Machines built later get the same through `register_machine` (`crate::machine_frame`).
pub(crate) fn init_power(world: &mut World) {
    let start = Instant::now();
    let mut report = PowerReport::default();

    let mut area_query = world.query_filtered::<(Entity, &DmPower), (With<Area>, With<Mapped>)>();
    let mut area_of_path: HashMap<String, Entity> = HashMap::new();
    let mut new_areas = Vec::new();
    for (e, dm) in area_query.iter(world) {
        area_of_path.insert(dm.path.clone(), e);
        // tg `/area/Initialize`: areas that need no power have every channel on.
        let power = if dm.requires_power {
            [dm.power_equip, dm.power_light, dm.power_environ]
        } else {
            [true; 3]
        };
        let area = AreaPower {
            requires_power: dm.requires_power,
            always_unpowered: dm.always_unpowered,
            power,
            lightswitch: dm.lightswitch,
            ..default()
        };
        new_areas.push((e, (area, AreaMembers::default())));
    }
    world.insert_batch(new_areas);

    let mut query = world.query_filtered::<(
        Entity,
        &DmPower,
        &TilePos,
        Option<&DmAtom>,
        Option<&DmLight>,
        Option<&Optics>,
    ), With<Mapped>>();
    let mut cables = Vec::new();
    let mut apcs = Vec::new();
    let mut apc_helpers = Vec::new();
    let mut lights_off = Vec::new();
    let mut smes = Vec::new();
    let mut terminals = Vec::new();
    let mut generators = Vec::new();
    let mut machines = Vec::new();
    let mut lights = Vec::new();
    for (e, dm, &pos, atom, light, optics) in query.iter(world) {
        let path = dm.path.as_str();
        let dir = atom.map_or(2, |a| a.dir);
        if is_under(path, CABLE_ROOT) {
            cables.push((e, pos, layer(dm)));
        } else if is_under(path, APC_ROOT) {
            apcs.push((e, pos, dm.clone(), atom.cloned(), light.cloned()));
        } else if is_under(path, APC_HELPER_ROOT) {
            apc_helpers.push((
                e,
                pos,
                path.to_owned(),
                dm.cell_maxcharge,
                dm.cell_chargerate,
            ));
        } else if is_under(path, LIGHTS_OFF_HELPER) {
            lights_off.push((e, pos));
        } else if is_under(path, SMES_ROOT) {
            smes.push((e, pos, dm.clone()));
        } else if is_under(path, TERMINAL_ROOT) {
            terminals.push((e, pos, dir, layer(dm)));
        } else if is_under(path, RTG_ROOT) {
            generators.push((e, pos, dm.power_gen, layer(dm)));
        } else if is_under(path, LIGHT_ROOT) {
            let light = light.cloned().unwrap_or_default();
            // The light a working bulb gives: the map's, or `update()`'s for one broken or empty
            // on the map.
            let on_light = optics.and_then(|o| o.light).or_else(|| {
                let whole = DmLight {
                    status: 0,
                    ..light.clone()
                };
                let pixel = atom.map_or([0, 0], |a| [a.pixel_x, a.pixel_y]);
                let mut on = source(path, &whole, dir, pixel)?;
                ss13_mapload::light::tint_fixture(path, &mut on, atom.and_then(|a| a.color));
                Some(on)
            });
            let icon = atom.map(|a| a.icon.clone()).unwrap_or_default();
            lights.push((e, pos, dm.clone(), light, on_light, icon));
        } else if is_under(path, MACHINE_ROOT) && dm.use_power > 0.0 {
            let watts = if dm.use_power >= 2.0 {
                dm.active_power_usage
            } else {
                dm.idle_power_usage
            };
            machines.push((e, pos, Channel::from_dm(dm.power_channel), watts));
        }
    }

    let area_at = |world: &World, pos: TilePos| world.resource::<Areas>().area_at(pos);
    for (helper, pos) in lights_off {
        if let Some(mut area) = area_at(world, pos).and_then(|a| world.get_mut::<AreaPower>(a)) {
            area.lightswitch = false;
        }
        world.despawn(helper);
    }

    let mut grid = world.remove_resource::<Grid>().unwrap_or_default();
    for &(e, pos, ref dm) in &smes {
        grid.add_node(e, pos, layer(dm), NodeKind::Smes);
    }
    for &(e, pos, _, layer) in &terminals {
        grid.add_node(e, pos, layer, NodeKind::Terminal);
    }

    // Taken out for the loops below, which mutate the world; put back at the end.
    let icon_index = world.remove_resource::<IconIndex>();
    let index = icon_index.as_ref();
    let mut sprites_of_icon: HashMap<String, Arc<ApcSprites>> = HashMap::new();
    let mut new_apcs = Vec::new();
    for (e, pos, dm, atom, light) in apcs {
        let area = if dm.areastring.is_empty() {
            area_at(world, pos)
        } else {
            area_of_path.get(&dm.areastring).copied().or_else(|| {
                warn!("APC at {pos:?}: bad areastring {}", dm.areastring);
                area_at(world, pos)
            })
        };
        let Some(area) = area else {
            continue;
        };
        let mut cell = (dm.cell_maxcharge > 0.0).then(|| Cell {
            charge: dm.start_charge * dm.cell_maxcharge / 100.0,
            maxcharge: dm.cell_maxcharge,
            chargerate: dm.cell_chargerate,
        });
        // tg `/obj/effect/mapping_helpers/apc/LateInitialize` order: cells, then charge.
        for (_, _, helper, maxcharge, chargerate) in apc_helpers.iter().filter(|h| h.1 == pos) {
            if is_under(helper, APC_HELPER_CELL_5K) || is_under(helper, APC_HELPER_CELL_10K) {
                cell = Some(Cell {
                    charge: *maxcharge,
                    maxcharge: *maxcharge,
                    chargerate: *chargerate,
                });
            }
        }
        for (_, _, helper, ..) in apc_helpers.iter().filter(|h| h.1 == pos) {
            if let Some(cell) = &mut cell {
                if is_under(helper, APC_HELPER_NO_CHARGE) {
                    cell.charge = 0.0;
                } else if is_under(helper, APC_HELPER_FULL_CHARGE) {
                    cell.charge = cell.maxcharge;
                }
            }
        }
        let mut apc = Apc::new(area, cell);
        apc.channels = [dm.equipment, dm.lighting, dm.environ].map(|c| c.clamp(0.0, 3.0) as u8);
        apc.operating = dm.operating;
        apc.chargemode = dm.chargemode;
        // `unlock()`, `give_syndicate_access()`, `give_away_general_access()`.
        for (_, _, helper, ..) in apc_helpers.iter().filter(|h| h.1 == pos) {
            if is_under(helper, APC_HELPER_UNLOCKED) {
                apc.locked = false;
            } else if is_under(helper, APC_HELPER_SYNDICATE_ACCESS) {
                apc.req_access = vec!["syndicate".to_owned()];
            } else if is_under(helper, APC_HELPER_AWAY_GENERAL_ACCESS) {
                apc.req_access = vec!["away_general".to_owned()];
            }
        }
        grid.add_node(e, pos, layer(&dm), NodeKind::Machine);
        let atom = atom.unwrap_or_default();
        let sprites = sprites_of_icon
            .entry(atom.icon.clone())
            .or_insert_with(|| Arc::new(apc_sprites(index, &atom.icon)))
            .clone();
        let look = ApcLook {
            key: None,
            sprites,
            light: light.unwrap_or_default(),
            path: dm.path.clone(),
            dir: atom.dir,
            pixel: [atom.pixel_x, atom.pixel_y],
        };
        new_apcs.push((e, area, apc, look));
    }
    for (helper, ..) in apc_helpers {
        world.despawn(helper);
    }
    report.apcs = new_apcs.len();
    for &(e, area, ..) in &new_apcs {
        let mut area = world.get_mut::<AreaPower>(area).expect("APC area");
        if area.apc.is_some() {
            report.duplicate_apcs += 1;
        }
        area.apc = Some(e);
    }

    let mut new_smes = Vec::new();
    for (e, pos, dm) in smes {
        // tg `/obj/machinery/power/smes/Initialize`: the first terminal next to it facing it.
        let terminal = Dir::CARDINALS.into_iter().find_map(|d| {
            let at = pos.step(d);
            terminals
                .iter()
                .find(|t| t.1 == at && t.2 == d.reverse() as u8)
                .map(|t| t.0)
        });
        report.smes_broken += usize::from(terminal.is_none());
        new_smes.push((
            e,
            Smes {
                charge: dm.charge.min(dm.capacity),
                capacity: dm.capacity,
                input_level: dm.input_level,
                output_level: dm.output_level,
                input_level_max: model::SMES_LEVEL_MAX,
                output_level_max: model::SMES_LEVEL_MAX,
                input_attempt: dm.input_attempt,
                output_attempt: dm.output_attempt,
                inputting: false,
                outputting: false,
                input_available: 0.0,
                output_used: 0.0,
                terminal,
            },
        ));
    }
    report.smes = new_smes.len();
    world.insert_batch(new_smes);

    report.generators = generators.len();
    for &(e, pos, watts, layer) in &generators {
        grid.add_node(e, pos, layer, NodeKind::Machine);
        world.entity_mut(e).insert(Generator { watts });
    }

    for (e, pos, channel, watts) in machines {
        let Some(area) = area_at(world, pos) else {
            continue;
        };
        report.machines += 1;
        report.static_watts += watts;
        world.entity_mut(e).insert(PowerMachine {
            channel,
            area,
            watts,
        });
        let mut area_mut = world.entity_mut(area);
        area_mut.get_mut::<AreaPower>().expect("area").static_power[channel as usize] += watts;
        area_mut
            .get_mut::<AreaMembers>()
            .expect("area")
            .machines
            .push((e, channel));
    }

    for (e, pos, dm, light, on_light, icon) in lights {
        let Some(area) = area_at(world, pos) else {
            continue;
        };
        report.lights += 1;
        let lightswitch = world.get::<AreaPower>(area).is_some_and(|a| a.lightswitch);
        // The emergency cell starts empty in areas with the switch off (`emergency_light`
        // cell `Initialize`).
        let emergency = (!dm.no_low_power && dm.start_with_cell).then_some(if lightswitch {
            EMERGENCY_CELL_CHARGE
        } else {
            0.0
        });
        let status = lighting::LightStatus::from_dm(light.status);
        let mut fixture =
            lighting::new_fixture(index, &dm, &light, on_light, &icon, area, emergency, status);
        if lighting::roundstart_break(world, &fixture) {
            fixture.status = lighting::LightStatus::Broken;
            fixture.on_light = None;
        }
        // Map load lit every working bulb. The fixture starts off and `apply_area_power` turns
        // on those with power; it skips one that stays off, which must not keep the map's light.
        if let Some(mut optics) = world.get_mut::<Optics>(e) {
            optics.light = None;
        }
        refresh_light(&fixture, None, world.get_mut::<Appearance>(e));
        world.entity_mut(e).insert(fixture);
        world
            .get_mut::<AreaMembers>(area)
            .expect("area")
            .lights
            .push(e);
    }

    // shortcut: power cables only (`/obj/structure/cable/multilayer/multiz` hubs do not link
    // z-levels yet); add with multi-z maps.
    grid.build(cables);
    report.cables = grid.cable_count();
    report.nets = grid.nets().count();
    for &(e, ..) in &new_apcs {
        report.apcs_unwired += usize::from(grid.node_net(e).is_none());
    }
    world.insert_resource(grid);

    // tg `addtimer(update, 0.5 SECONDS)` in the APC `Initialize`: the area follows its APC.
    for (e, area, apc, look) in new_apcs {
        apc.update(&mut world.get_mut::<AreaPower>(area).expect("area"));
        world.entity_mut(e).insert((apc, look));
    }
    let mut areas = world.query_filtered::<(Entity, &AreaPower, &AreaMembers), With<Mapped>>();
    let mut changed = Vec::new();
    for (e, area, members) in areas.iter(world) {
        let needs = area.requires_power && !area.always_unpowered;
        let on_map = !members.machines.is_empty() || !members.lights.is_empty();
        if needs && (on_map || area.apc.is_some()) {
            report.areas_needing_power += 1;
            report.areas_with_apc += usize::from(area.apc.is_some());
        }
        changed.push(AreaPowerChanged(e));
    }
    world.write_message_batch(changed);
    info!(
        "power: {} cables in {} nets, {} APCs ({} unwired, {} duplicate) for {}/{} areas that \
         need power, {} SMES ({} without terminal), {} generators, {} machines ({:.1} kW static), \
         {} lights, in {:.2?}",
        report.cables,
        report.nets,
        report.apcs,
        report.apcs_unwired,
        report.duplicate_apcs,
        report.areas_with_apc,
        report.areas_needing_power,
        report.smes,
        report.smes_broken,
        report.generators,
        report.machines,
        report.static_watts / 1000.0,
        report.lights,
        start.elapsed()
    );
    if let Some(icon_index) = icon_index {
        world.insert_resource(icon_index);
    }
    world.insert_resource(report);
}

fn apc_sprites(index: Option<&IconIndex>, icon: &str) -> ApcSprites {
    let get = |state: String| index.and_then(|i| i.get(icon, &state));
    ApcSprites {
        lock: std::array::from_fn(|i| get(format!("apcox-{i}"))),
        charging: std::array::from_fn(|i| get(format!("apco3-{i}"))),
        channels: std::array::from_fn(|c| std::array::from_fn(|i| get(format!("apco{c}-{i}")))),
    }
}

/// tg `SSmachines.fire()` power order, every [`MACHINE_TICK_SECS`].
#[allow(clippy::too_many_arguments)]
fn power_tick(
    time: Res<Time>,
    mut clock: ResMut<PowerClock>,
    mut grid: ResMut<Grid>,
    mut apcs: Query<(Entity, &mut Apc)>,
    mut areas: Query<&mut AreaPower>,
    mut smes: Query<(Entity, &mut Smes)>,
    generators: Query<(Entity, &Generator)>,
    mut lights: Query<
        (
            Entity,
            &mut LightFixture,
            Option<&mut Optics>,
            Option<&mut Appearance>,
        ),
        With<LightProcessing>,
    >,
    mut changed: MessageWriter<AreaPowerChanged>,
    mut commands: Commands,
    mut generation: Generation,
) {
    clock.accumulated += time.delta_secs_f64();
    let mut ticks = 0;
    while clock.accumulated >= MACHINE_TICK_SECS {
        clock.accumulated -= MACHINE_TICK_SECS;
        ticks += 1;
        if ticks > MAX_CATCH_UP {
            continue;
        }
        let dt = MACHINE_TICK_SECS;

        // Powernet resets; SMES restore what their net did not use.
        let mut restoring = Vec::new();
        for (id, net) in grid.nets_mut() {
            if net.begin_reset() {
                restoring.push(id);
            }
        }
        for (e, mut s) in &mut smes {
            if let Some(net) = grid
                .node_net(e)
                .filter(|n| restoring.contains(n))
                .and_then(|n| grid.net_mut(n))
            {
                s.restore(net);
            }
        }
        for (_, net) in grid.nets_mut() {
            net.finish_reset();
        }
        generation.process_early(&mut grid, time.elapsed_secs_f64(), dt);

        // APCs, in tg's five passes.
        for (e, mut apc) in &mut apcs {
            let net = grid.node_net(e);
            if let Ok(mut area) = areas.get_mut(apc.area) {
                apc.early_process(&mut area, net.and_then(|n| grid.net_mut(n)), dt);
            }
        }
        for step in ChargeStep::ORDER {
            for (e, mut apc) in &mut apcs {
                let net = grid.node_net(e);
                if let Ok(mut area) = areas.get_mut(apc.area) {
                    apc.charge_channel(step, &mut area, net.and_then(|n| grid.net_mut(n)), dt);
                    if step == ChargeStep::Full {
                        let net = net.and_then(|n| grid.net(n));
                        if apc.late_process(&mut area, net, dt) {
                            changed.write(AreaPowerChanged(apc.area));
                        }
                        if let Some(on) = apc.nightshift_request.take() {
                            commands.queue(move |world: &mut World| {
                                apply_low_power_nightshift(world, e, on);
                            });
                        }
                    }
                }
            }
        }

        // Machines.
        for (e, mut s) in &mut smes {
            let out = grid.node_net(e);
            s.process_output(out.and_then(|n| grid.net_mut(n)), dt);
            let input = s.terminal.and_then(|t| grid.node_net(t));
            s.process_input(input.and_then(|n| grid.net_mut(n)), dt);
        }
        for (e, generator) in &generators {
            if let Some(net) = grid.node_net(e).and_then(|n| grid.net_mut(n)) {
                net.newavail += generator.watts * dt;
            }
        }
        generation.process(&mut grid, dt);
        for (e, mut light, optics, appearance) in &mut lights {
            let Ok(mut area) = areas.get_mut(light.area) else {
                continue;
            };
            match light.mode {
                // tg `process()` while powered: recharge the emergency cell at 2 W from the
                // area (`charge_cell` on the equipment channel), until full.
                LightMode::On => {
                    let Some(charge) = light.emergency else {
                        commands.entity(e).remove::<LightProcessing>();
                        continue;
                    };
                    let want = (LIGHT_EMERGENCY_POWER_USE * dt).min(EMERGENCY_CELL_CHARGE - charge);
                    let apc = area.apc.and_then(|a| apcs.get_mut(a).ok());
                    let got = use_energy(want, Channel::Equip, &mut area, apc, &mut grid);
                    let charge = (charge + got).min(EMERGENCY_CELL_CHARGE);
                    light.emergency = Some(charge);
                    if charge >= EMERGENCY_CELL_CHARGE {
                        commands.entity(e).remove::<LightProcessing>();
                    }
                }
                // tg `use_emergency_power()`, or back to `update()` when the cell is flat.
                LightMode::Emergency => {
                    let need = LIGHT_EMERGENCY_POWER_USE * dt;
                    let charge = light.emergency.unwrap_or(0.0);
                    if charge >= need {
                        light.emergency = Some(charge - need);
                        refresh_light(&light, optics, appearance);
                    } else {
                        set_mode(&mut light, LightMode::Off, &mut area, optics, appearance);
                        commands.entity(e).remove::<LightProcessing>();
                    }
                }
                LightMode::Off => {
                    commands.entity(e).remove::<LightProcessing>();
                }
            }
        }
    }
}

/// tg `/obj/machinery/proc/use_energy()` (`force = TRUE`): grid surplus of the area's APC,
/// then its cell. Returns what was used.
pub fn use_energy(
    amount: f64,
    channel: Channel,
    area: &mut AreaPower,
    apc: Option<(Entity, Mut<Apc>)>,
    grid: &mut Grid,
) -> f64 {
    use_energy_from(amount, channel, area, apc, grid, false)
}

/// tg `use_energy(amount, channel, ignore_apc)`: with `ignore_apc` only the grid's surplus
/// (`charge_cell(grid_only = TRUE)`).
pub fn use_energy_from(
    amount: f64,
    channel: Channel,
    area: &mut AreaPower,
    apc: Option<(Entity, Mut<Apc>)>,
    grid: &mut Grid,
    ignore_apc: bool,
) -> f64 {
    if amount <= 0.0 {
        return 0.0;
    }
    if !area.requires_power {
        return amount;
    }
    let Some((apc_entity, mut apc)) = apc.filter(|(_, apc)| apc.operating) else {
        return 0.0;
    };
    let net = grid.node_net(apc_entity).and_then(|n| grid.net_mut(n));
    let grid_used = net.map_or(0.0, |net| {
        let used = net.surplus().min(amount);
        net.load += used;
        used
    });
    let cell_used = if amount > grid_used && !ignore_apc {
        apc.cell
            .as_mut()
            .map_or(0.0, |c| c.take(amount - grid_used, true))
    } else {
        0.0
    };
    let used = grid_used + cell_used;
    area.used[channel as usize] += used;
    used
}

/// tg `update_use_power`: the static load follows idle/active.
pub fn update_use_power(world: &mut World, machine: Entity, active: bool) {
    let Some(dm) = world.get::<DmPower>(machine) else {
        return;
    };
    let watts = if active {
        dm.active_power_usage
    } else {
        dm.idle_power_usage
    };
    if let Some(mut pm) = world.get_mut::<PowerMachine>(machine) {
        let old = pm.watts;
        pm.watts = watts;
        let (area, channel) = (pm.area, pm.channel);
        if let Some(mut a) = world.get_mut::<AreaPower>(area) {
            a.static_power[channel as usize] += watts - old;
        }
    }
}

/// The machine's area and channel: its [`PowerMachine`], else (machines that use no power) the
/// area of its tile and its `power_channel`.
pub fn machine_power(world: &World, machine: Entity) -> Option<PowerMachine> {
    if let Some(pm) = world.get::<PowerMachine>(machine) {
        return Some(*pm);
    }
    let pos = world.get::<TilePos>(machine)?;
    let dm = world.get::<DmPower>(machine)?;
    Some(PowerMachine {
        channel: Channel::from_dm(dm.power_channel),
        area: world.resource::<Areas>().area_at(*pos)?,
        watts: 0.0,
    })
}

/// tg `SSmachines.register_machine` + `power_change` of a machine built after map load
/// (`crate::machine_frame`): what [`init_power`] gives a mapped machine of its type.
pub fn register_machine(world: &mut World, machine: Entity) {
    let (Some(dm), Some(&pos)) = (
        world.get::<DmPower>(machine).cloned(),
        world.get::<TilePos>(machine),
    ) else {
        return;
    };
    if !is_under(&dm.path, MACHINE_ROOT) || dm.use_power <= 0.0 {
        return;
    }
    let Some(area) = world.resource::<Areas>().area_at(pos) else {
        return;
    };
    let watts = if dm.use_power >= 2.0 {
        dm.active_power_usage
    } else {
        dm.idle_power_usage
    };
    let channel = Channel::from_dm(dm.power_channel);
    let Some(powered) = world.get_mut::<AreaPower>(area).map(|mut a| {
        a.static_power[channel as usize] += watts;
        a.powered(channel)
    }) else {
        return;
    };
    if let Some(mut members) = world.get_mut::<AreaMembers>(area) {
        members.machines.push((machine, channel));
    }
    let mut entity = world.entity_mut(machine);
    entity.insert(PowerMachine {
        channel,
        area,
        watts,
    });
    if !powered {
        entity.insert(Unpowered);
    }
}

/// A machine taken apart: its static load and area membership go.
pub fn unregister_machine(world: &mut World, machine: Entity) {
    let Some(pm) = world.get::<PowerMachine>(machine).copied() else {
        return;
    };
    if let Some(mut area) = world.get_mut::<AreaPower>(pm.area) {
        let load = &mut area.static_power[pm.channel as usize];
        *load = (*load - pm.watts).max(0.0);
    }
    if let Some(mut members) = world.get_mut::<AreaMembers>(pm.area) {
        members.machines.retain(|&(e, _)| e != machine);
    }
    world.entity_mut(machine).remove::<PowerMachine>();
}

/// tg `/obj/machinery/proc/use_energy(joules, ignore_apc)` of `machine` from its area (grid
/// surplus, then the APC cell unless `ignore_apc`). Returns what was used; a machine in no
/// area uses nothing.
pub fn machine_use_energy(
    world: &mut World,
    machine: Entity,
    joules: f64,
    ignore_apc: bool,
) -> f64 {
    // tg `use_energy` reads the machine's area whatever its `use_power`; machines that use
    // none have no `PowerMachine`.
    let Some(pm) = machine_power(world, machine) else {
        return 0.0;
    };
    let Some(apc) = world.get::<AreaPower>(pm.area).map(|a| a.apc) else {
        return 0.0;
    };
    world.resource_scope(|world, mut grid: Mut<Grid>| match apc {
        Some(apc) if apc != pm.area => {
            let [mut area, mut apc_entity] = world.entity_mut([pm.area, apc]);
            let (Some(mut area), Some(apc_state)) =
                (area.get_mut::<AreaPower>(), apc_entity.get_mut::<Apc>())
            else {
                return 0.0;
            };
            use_energy_from(
                joules,
                pm.channel,
                &mut area,
                Some((apc, apc_state)),
                &mut grid,
                ignore_apc,
            )
        }
        _ => world.get_mut::<AreaPower>(pm.area).map_or(0.0, |mut area| {
            use_energy_from(joules, pm.channel, &mut area, None, &mut grid, ignore_apc)
        }),
    })
}

/// tg `/obj/machinery/proc/charge_cell(amount, cell, grid_only)`: `use_energy(min(amount,
/// cell.used_charge()))` from the machine's area, all of it into the cell. Returns what the cell
/// got.
pub fn charge_cell(
    world: &mut World,
    machine: Entity,
    amount: f64,
    cell: &mut Cell,
    grid_only: bool,
) -> f64 {
    let demand = machine_use_energy(world, machine, amount.min(cell.used_charge()), grid_only);
    cell.give(demand)
}

/// tg `power_change()` of every machine and light in the areas whose channels changed.
fn apply_area_power(
    mut changed: MessageReader<AreaPowerChanged>,
    mut areas: Query<(&mut AreaPower, &AreaMembers)>,
    machines: Query<Has<Unpowered>, With<PowerMachine>>,
    mut lights: Query<(
        &mut LightFixture,
        Option<&mut Optics>,
        Option<&mut Appearance>,
    )>,
    mut rng: ResMut<EngineRng>,
    mut commands: Commands,
) {
    for &AreaPowerChanged(area_entity) in changed.read() {
        let Ok((mut area, members)) = areas.get_mut(area_entity) else {
            continue;
        };
        for &(machine, channel) in &members.machines {
            let Ok(unpowered) = machines.get(machine) else {
                continue;
            };
            match (area.powered(channel), unpowered) {
                (true, true) => {
                    commands.entity(machine).remove::<Unpowered>();
                }
                (false, false) => {
                    commands.entity(machine).insert(Unpowered);
                }
                _ => {}
            }
        }
        for &light_entity in &members.lights {
            let Ok((mut light, optics, appearance)) = lights.get_mut(light_entity) else {
                continue;
            };
            let mode = if light.on_light.is_some() {
                model::light_mode(&area, light.emergency, MACHINE_TICK_SECS)
            } else {
                LightMode::Off
            };
            if mode == light.mode {
                continue;
            }
            // tg `update(trigger)`: every switch on counts and may burn the bulb out; the
            // first one is `post_machine_initialize`'s `update(trigger = FALSE)`.
            let mut mode = mode;
            if mode == LightMode::On {
                light.switchcount += 1;
                let count = light.switchcount as f32;
                if light.switchcount > 1 && rng.0.chance((count * count * 0.01).min(60.0)) {
                    light.status = lighting::LightStatus::Burned;
                    light.on_light = None;
                    mode = LightMode::Off;
                    if mode == light.mode {
                        refresh_light(&light, optics, appearance);
                        continue;
                    }
                }
            }
            set_mode(&mut light, mode, &mut area, optics, appearance);
            let recharging =
                mode == LightMode::On && light.emergency.is_some_and(|c| c < EMERGENCY_CELL_CHARGE);
            if mode == LightMode::Emergency || recharging {
                commands.entity(light_entity).insert(LightProcessing);
            }
        }
    }
}

/// Switches a fixture's mode: its static lighting load, light and icon state.
fn set_mode(
    light: &mut LightFixture,
    mode: LightMode,
    area: &mut AreaPower,
    optics: Option<Mut<Optics>>,
    appearance: Option<Mut<Appearance>>,
) {
    let watts = light.watts;
    let load = &mut area.static_power[Channel::Light as usize];
    match (light.mode == LightMode::On, mode == LightMode::On) {
        (false, true) => *load += watts,
        (true, false) => *load -= watts,
        _ => {}
    }
    light.mode = mode;
    refresh_light(light, optics, appearance);
}

/// Writes the fixture's light, icon state and overlays, touching the components only on a change so
/// replication sends nothing for an unchanged light.
fn refresh_light(
    light: &LightFixture,
    optics: Option<Mut<Optics>>,
    appearance: Option<Mut<Appearance>>,
) {
    let source = light.light();
    if let Some(mut optics) = optics
        && optics.light != source
    {
        optics.light = source;
    }
    // tg `update_icon_state`: `_emergency` in low power mode or while the area is on fire.
    let emergency = match light.mode {
        LightMode::Emergency => true,
        LightMode::On => light.fire,
        LightMode::Off => false,
    };
    let state = match light.status {
        lighting::LightStatus::Ok => light.states[usize::from(emergency)],
        lighting::LightStatus::Empty => light.status_states[0],
        lighting::LightStatus::Broken => light.status_states[1],
        lighting::LightStatus::Burned => light.status_states[2],
    };
    let Some(mut appearance) = appearance else {
        return;
    };
    if let Some(state) = state
        && appearance.icon_state != state
    {
        appearance.icon_state = state;
    }
    let overlays = light.overlays();
    if appearance.overlays != overlays {
        appearance.overlays = overlays;
    }
}

/// tg `/area/proc/set_fire_effect(new_fire)`: the area's `fire` and its lights' `handle_fire`
/// (red while on). False when it did not change.
pub fn set_area_fire(world: &mut World, area: Entity, fire: bool) -> bool {
    let Some(mut power) = world.get_mut::<AreaPower>(area) else {
        return false;
    };
    if power.fire == fire {
        return false;
    }
    power.fire = fire;
    let lights = world
        .get::<AreaMembers>(area)
        .map(|m| m.lights.clone())
        .unwrap_or_default();
    let mut query = world.query::<(
        &mut LightFixture,
        Option<&mut Optics>,
        Option<&mut Appearance>,
    )>();
    for light in lights {
        if let Ok((mut fixture, optics, appearance)) = query.get_mut(world, light) {
            fixture.fire = fire;
            refresh_light(&fixture, optics, appearance);
        }
    }
    true
}

/// tg `/obj/machinery/power/apc/proc/set_nightshift(on)`: the APC's `nightshift_lights` and its
/// area's fixtures' `nightshift_enabled` (dimmer and warmer while on).
// Refused off while a low cell forces it (`low_power_nightshift_lights`).
// shortcut: the lighting load stays the full bulb's (tg draws `nightshift_brightness *
// nightshift_light_power`); a fixture's own `color` does not override the night colour; no
// `nightshift_allowed` (no ported fixture turns it off).
pub fn set_nightshift(world: &mut World, apc: Entity, on: bool) {
    let Some(mut state) = world.get_mut::<model::Apc>(apc) else {
        return;
    };
    if (state.low_power_nightshift_lights && !on) || state.nightshift_lights == on {
        return;
    }
    state.nightshift_lights = on;
    let area = state.area;
    let lights = world
        .get::<AreaMembers>(area)
        .map(|m| m.lights.clone())
        .unwrap_or_default();
    let mut query = world.query::<(
        &mut LightFixture,
        Option<&mut Optics>,
        Option<&mut Appearance>,
    )>();
    for light in lights {
        if let Ok((mut fixture, optics, appearance)) = query.get_mut(world, light)
            && fixture.nightshift != on
        {
            fixture.nightshift = on;
            refresh_light(&fixture, optics, appearance);
        }
    }
}

/// `late_process`'s low-power night shift: on when the cell runs low; when it recovers, off
/// unless the Night Shift event is running outside an emergency (tg checks `SSevents.running`
/// and `SEC_LEVEL_RED`).
fn apply_low_power_nightshift(world: &mut World, apc: Entity, on: bool) {
    let keep = !on
        && world
            .get_resource::<crate::events::controller::Events>()
            .is_some_and(|ev| ev.running().any(|n| n == crate::events::nightshift::NAME))
        && !crate::events::nightshift::emergency(world);
    if !keep {
        set_nightshift(world, apc, on);
    }
}

/// tg `/obj/machinery/power/apc/proc/toggle_nightshift_lights(user)`: refused while a low cell
/// forces night shift, and within 10 s of the last switch.
pub fn toggle_nightshift_lights(world: &mut World, user: Entity, apc: Entity) {
    let now = world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64());
    let Some(state) = world.get::<model::Apc>(apc) else {
        return;
    };
    let refusal = if state.low_power_nightshift_lights {
        Some("слишком мало питания!")
    } else if state.last_nightshift_switch > now - NIGHTSHIFT_TOGGLE_COOLDOWN {
        Some("цикл освещения перезаряжается!")
    } else {
        None
    };
    let on = !state.nightshift_lights;
    if let Some(text) = refusal {
        crate::balloon::balloon_alert(world, user, apc, text);
        return;
    }
    if let Some(mut state) = world.get_mut::<model::Apc>(apc) {
        state.last_nightshift_switch = now;
    }
    set_nightshift(world, apc, on);
}

/// tg `toggle_nightshift_lights`: "~10 seconds between each toggle to prevent spamming".
const NIGHTSHIFT_TOGGLE_COOLDOWN: f64 = 10.0;

/// tg `/obj/machinery/light/proc/break_light_tube()`: the bulb breaks (`LIGHT_BROKEN`,
/// `"[base_state]-broken"`) with glass and sparks, the fixture goes dark until a new bulb goes
/// in ([`lighting`]). False when it had no bulb to break.
pub fn break_light_tube(world: &mut World, light: Entity) -> bool {
    lighting::break_tube(world, light, false)
}

/// tg `/obj/machinery/light/proc/burn_out()` of a working bulb (an EMP): `LIGHT_BURNED`,
/// `"[base_state]-burned"`.
pub fn burn_out_light(world: &mut World, light: Entity) -> bool {
    lighting::burn_out(world, light)
}

/// tg APC `update_overlays()` and its charging-coloured light, when the overlay key changes.
fn update_apc_looks(
    mut apcs: Query<
        (
            Entity,
            &Apc,
            &mut ApcLook,
            Option<&mut Appearance>,
            Option<&mut Optics>,
        ),
        Changed<Apc>,
    >,
    mut commands: Commands,
) {
    for (e, apc, mut look, appearance, optics) in &mut apcs {
        let key = apc.look_key();
        if look.key == Some(key) {
            continue;
        }
        look.key = Some(key);
        let s = &look.sprites;
        if let Some(mut appearance) = appearance {
            let mut states = vec![
                s.lock[usize::from(apc.locked)],
                s.charging[usize::from(apc.charging.min(2))],
            ];
            if apc.operating {
                states.extend(
                    [Channel::Equip, Channel::Light, Channel::Environ]
                        .map(|c| s.channels[c as usize][usize::from(apc.channels[c as usize])]),
                );
            }
            appearance.overlays = states
                .into_iter()
                .flatten()
                // tg `update_overlays`: each status light plus its `emissive_appearance`.
                .flat_map(|icon_state| {
                    [
                        Overlay::new(icon_state),
                        Overlay {
                            plane: Some(ss13_shared::EMISSIVE_PLANE),
                            ..Overlay::new(icon_state)
                        },
                    ]
                })
                .collect();
        }
        let light = DmLight {
            light_range: APC_LIGHT_RANGE,
            light_color: Some(APC_LIGHT_COLORS[usize::from(apc.charging.min(2))]),
            light_on: true,
            ..look.light.clone()
        };
        let light = source(&look.path, &light, look.dir, look.pixel);
        match optics {
            Some(mut optics) => {
                if optics.light != light {
                    optics.light = light;
                }
            }
            None => {
                commands.entity(e).insert(Optics { light, ..default() });
            }
        }
    }
}

#[cfg(test)]
mod nightshift_tests {
    use super::*;

    fn source(range: f32, power: f32) -> LightSource {
        LightSource {
            range,
            power,
            color: [255; 3],
            height: 1.0,
            angle: 360,
            dir: Dir::South,
            offset: [0, 0],
        }
    }

    #[test]
    fn night_shift_dims_the_areas_lit_fixtures() {
        let mut w = World::new();
        let area = w.spawn_empty().id();
        let fixture = LightFixture {
            area,
            mode: LightMode::On,
            on_light: Some(source(8.0, 1.0)),
            brightness: 8.0,
            emergency: None,
            watts: 0.0,
            states: [None, None],
            fire_light: Some(source(9.0, 0.5)),
            fire: false,
            night_light: Some(source(8.0, 0.45)),
            nightshift: false,
            status: lighting::LightStatus::Ok,
            template: Some(source(8.0, 1.0)),
            status_states: [None; 3],
            overlay_states: [None; 4],
            bulb: false,
            switchcount: 0,
            flickering: false,
            integrity: 100.0,
            bulb_power: 1.0,
            consumption_rate: 20.0,
        };
        let light = w.spawn((fixture, Optics::default())).id();
        w.entity_mut(area).insert(AreaMembers {
            machines: Vec::new(),
            lights: vec![light],
        });
        let apc = w.spawn(Apc::new(area, None)).id();
        set_nightshift(&mut w, apc, true);
        assert!(w.get::<Apc>(apc).unwrap().nightshift_lights);
        let lit = |w: &World| w.get::<Optics>(light).unwrap().light.map(|l| l.power);
        assert_eq!(lit(&w), Some(0.45));
        // A fire alarm's light comes first.
        w.get_mut::<LightFixture>(light).unwrap().fire = true;
        assert_eq!(
            w.get::<LightFixture>(light)
                .unwrap()
                .light()
                .map(|l| l.power),
            Some(0.5)
        );
        w.get_mut::<LightFixture>(light).unwrap().fire = false;
        set_nightshift(&mut w, apc, false);
        assert!(!w.get::<Apc>(apc).unwrap().nightshift_lights);
        assert_eq!(lit(&w), Some(1.0));
    }

    #[test]
    fn toggle_is_refused_by_cooldown_and_low_power() {
        let mut w = World::new();
        let area = w.spawn(AreaMembers::default()).id();
        let apc = w.spawn(Apc::new(area, None)).id();
        let user = w.spawn_empty().id();
        toggle_nightshift_lights(&mut w, user, apc);
        assert!(w.get::<Apc>(apc).unwrap().nightshift_lights);
        // Within 10 s of the last switch (no clock: time 0).
        toggle_nightshift_lights(&mut w, user, apc);
        assert!(w.get::<Apc>(apc).unwrap().nightshift_lights);
        // A low cell keeps it on whatever asks.
        let mut state = w.get_mut::<Apc>(apc).unwrap();
        state.last_nightshift_switch = f64::NEG_INFINITY;
        state.low_power_nightshift_lights = true;
        toggle_nightshift_lights(&mut w, user, apc);
        set_nightshift(&mut w, apc, false);
        assert!(w.get::<Apc>(apc).unwrap().nightshift_lights);
    }

    #[test]
    fn only_a_lit_whole_bulb_has_overlays() {
        let state = |i| Some(IconStateId(i));
        let fixture = LightFixture {
            area: Entity::PLACEHOLDER,
            mode: LightMode::On,
            on_light: Some(source(8.0, 1.0)),
            brightness: 8.0,
            emergency: None,
            watts: 0.0,
            states: [None, None],
            fire_light: None,
            fire: false,
            night_light: None,
            nightshift: false,
            status: lighting::LightStatus::Ok,
            template: None,
            status_states: [None; 3],
            overlay_states: [state(10), state(11), state(12), state(13)],
            bulb: false,
            switchcount: 0,
            flickering: false,
            integrity: 100.0,
            bulb_power: 1.0,
            consumption_rate: 20.0,
        };
        // The mask (`emissive_appearance`) and the sprite.
        let drawn = |f: &LightFixture| -> Vec<(u32, bool)> {
            f.overlays()
                .iter()
                .map(|o| (o.icon_state.0, o.plane == Some(ss13_shared::EMISSIVE_PLANE)))
                .collect()
        };
        assert_eq!(drawn(&fixture), [(10, true), (10, false)]);
        for (tint, f) in [
            (
                11,
                LightFixture {
                    fire: true,
                    nightshift: true,
                    ..fixture.clone()
                },
            ),
            (
                12,
                LightFixture {
                    nightshift: true,
                    ..fixture.clone()
                },
            ),
            (
                13,
                LightFixture {
                    flickering: true,
                    fire: true,
                    ..fixture.clone()
                },
            ),
        ] {
            assert_eq!(drawn(&f), [(10, true), (tint, false)]);
        }
        for mode in [LightMode::Off, LightMode::Emergency] {
            assert!(
                drawn(&LightFixture {
                    mode,
                    ..fixture.clone()
                })
                .is_empty()
            );
        }
        for status in [
            lighting::LightStatus::Empty,
            lighting::LightStatus::Broken,
            lighting::LightStatus::Burned,
        ] {
            assert!(
                drawn(&LightFixture {
                    status,
                    ..fixture.clone()
                })
                .is_empty()
            );
        }
    }
}
