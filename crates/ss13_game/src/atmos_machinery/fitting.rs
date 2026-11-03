//! Wrenching atmos machines off and building them back from fittings: tg
//! `/obj/machinery/atmospherics/wrench_act`, `unsafe_pressure_release` and `on_deconstruction`
//! (`atmosmachinery.dm`), `/obj/item/pipe` and `/obj/item/pipe_meter`
//! (`game/machinery/pipe/construction.dm`), `/obj/machinery/meter/wrench_act`.
//!
//! - A wrench on a pipe or a device that `can_unwrench` (switched off; a port without a
//!   portable): 2 s, none when it holds no gas. Over 2 atm above the tile the user is warned and,
//!   when it comes off, thrown away from it (`pressure / 250` tiles at `pressure / 1250`). Its
//!   gas goes onto the tile and a fitting of it is left: type, direction, allowed directions,
//!   layer, color, flip. Destroyed, it leaves the fitting too (tg `deconstruct(FALSE)`).
//! - A fitting: used in hand it turns clockwise, a right click on it moves it to the next layer;
//!   a wrench on one lying on the floor builds its machine there, switched off, unless the tile
//!   has a one-per-tile machine of its kind or a machine on its layer with a direction in common.
//!   A smart pipe in the way is narrowed when it can be (`try_smart_reconfiguration`).
//! - A meter comes off with a wrench in 4 s as a meter item, which a wrench fastens back onto a
//!   pipe of its layer; a meter drops off as that item when its pipe goes (`drop_meter`).
// shortcut: no welding fittings into sliced pipes (crafting), no "Invert Pipe" verb (flippable
// fittings flip only through the machine they came from), no turf meters from a screwdriver, no
// fingerprints or investigate log; a destroyed machine's fitting is whole (no item damage yet).

use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_atmos::gas::ONE_ATMOSPHERE;
use ss13_atmos::pipenet::{Custom, DeviceKind, Pipenet, default_node_dirs, flags};
use ss13_chat::Span;
use ss13_mapload::{DmAtmosDevice, DmName, IconIndex};
use ss13_shared::{Appearance, Dir, Facing, TilePos};

use super::devices::{
    ConditionalGate, GasFilter, GasMixer, GasPump, HeatExchanger, OutletInjector, PassiveGate,
    PortsConnector, VentPump, VentScrubber, VolumePump,
};
use super::meter::Meter;
use super::{AreaAtmos, AtmosDevice};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::tools::{Actor, ToolKind, tell};

pub use ss13_mapload::atmos_machine::{PIPE_FITTING, PIPE_METER_ITEM};
const PIPE_ITEM_DMI: &str = "icons/obj/pipes_n_cables/pipe_item.dmi";
const METER: &str = "/obj/machinery/meter";
/// `wrench_act`: `2 SECONDS`; the meter's `40`.
const UNWRENCH_DS: f32 = 20.0;
const METER_UNWRENCH_DS: f32 = 40.0;
const PIPING_LAYER_MIN: u8 = 1;
const PIPING_LAYER_MAX: u8 = 5;
const PIPING_LAYER_DEFAULT: u8 = 3;
/// `PIPING_LAYER_P_X`, `PIPING_LAYER_P_Y`.
const PIPING_LAYER_P: i16 = 5;

/// The fitting's own type (the machine's `construction_type`): how its direction maps onto the
/// built machine's (`fixed_dir`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FittingKind {
    Directional,
    /// `pipe/binary`: south and west become north and east.
    Binary,
    /// `binary/bendable` (heat exchanging pipes): as `Binary`, and laid bent too.
    Bendable,
    Trinary,
    /// `trinary/flippable`: a flipped one lies diagonally.
    Flippable,
    Quaternary,
}

impl FittingKind {
    pub fn of(construction_type: &str) -> Self {
        if is_under(construction_type, "/obj/item/pipe/trinary/flippable") {
            Self::Flippable
        } else if is_under(construction_type, "/obj/item/pipe/trinary") {
            Self::Trinary
        } else if is_under(construction_type, "/obj/item/pipe/binary/bendable") {
            Self::Bendable
        } else if is_under(construction_type, "/obj/item/pipe/binary") {
            Self::Binary
        } else if is_under(construction_type, "/obj/item/pipe/quaternary") {
            Self::Quaternary
        } else {
            Self::Directional
        }
    }

    /// `fixed_dir`: the built machine's direction for a fitting lying in `dir`.
    pub fn fixed_dir(self, dir: u8) -> u8 {
        match self {
            Self::Binary | Self::Bendable => match dir {
                2 => 1,
                8 => 4,
                d => d,
            },
            Self::Flippable if is_diagonal(dir) => turn(dir, 45),
            _ => dir,
        }
    }
}

/// `/obj/item/pipe`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct PipeFitting {
    /// `pipe_type`: the machine it builds.
    pub pipe_type: String,
    pub kind: FittingKind,
    /// The fitting's `dir`.
    pub dir: u8,
    /// `p_init_dir`: the directions a smart pipe built from it may connect in.
    pub init_dir: u8,
    pub layer: u8,
    /// `pipe_color`.
    pub color: [u8; 3],
    /// `trinary/flippable`'s `flipped`.
    pub flipped: bool,
}

/// `/obj/item/pipe_meter`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeterFitting {
    pub layer: u8,
}

/// `ISDIAGONALDIR`, `ISNOTSTUB`: more than one bit.
fn is_diagonal(bits: u8) -> bool {
    bits & bits.wrapping_sub(1) != 0
}

/// BYOND `turn(dir, angle)`: counterclockwise for a positive angle, in steps of 45°.
pub fn turn(dir: u8, angle: i32) -> u8 {
    const RING: [u8; 8] = [1, 9, 8, 10, 2, 6, 4, 5];
    let Some(i) = RING.iter().position(|&d| d == dir) else {
        return dir;
    };
    RING[(i + (angle / 45).rem_euclid(8) as usize) % 8]
}

/// A smart pipe already on the tile, as `try_smart_reconfiguration` sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmartPipe {
    /// Its `dir` (`update_pipe_icon`: from its connections).
    pub dir: u8,
    pub connections: u8,
}

/// What `try_smart_reconfiguration` settles on: the fitting's new `p_init_dir` and the pipe in
/// the way's new `initialize_directions`, each when it changes. `None`: no way out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reconfiguration {
    pub ours: Option<u8>,
    pub theirs: Option<u8>,
}

/// `try_smart_reconfiguration(machine, our_init_dirs)`: a smart pipe never gives up a live
/// connection, never drops under two directions, never gains a direction it did not allow.
pub fn smart_reconfiguration(
    we_are_smart: bool,
    our_init_dirs: u8,
    p_init_dir: u8,
    other_init_dirs: u8,
    other_smart: Option<SmartPipe>,
) -> Option<Reconfiguration> {
    const NS: u8 = 1 | 2;
    const EW: u8 = 4 | 8;
    match (we_are_smart, other_smart) {
        (true, Some(other)) if is_diagonal(other.dir) => {
            // Bent: bounce off it the other way.
            let opposing = our_init_dirs & !other.connections;
            is_diagonal(opposing).then_some(Reconfiguration {
                ours: Some(opposing),
                theirs: Some(other.connections),
            })
        }
        (true, Some(other)) => {
            // Straight: cross it.
            let (ours, theirs) = if other.dir & EW != 0 {
                (NS, EW)
            } else if other.dir & NS != 0 {
                (EW, NS)
            } else {
                return None;
            };
            (ours & !p_init_dir == 0 && !other_init_dirs & theirs == 0).then_some(Reconfiguration {
                ours: Some(ours),
                theirs: Some(theirs),
            })
        }
        (true, None) => {
            let opposing = our_init_dirs & !other_init_dirs;
            is_diagonal(opposing).then_some(Reconfiguration {
                ours: Some(opposing),
                theirs: None,
            })
        }
        (false, Some(other)) => {
            if our_init_dirs & other.connections != 0 {
                return None;
            }
            let opposing = other_init_dirs & !our_init_dirs;
            is_diagonal(opposing).then_some(Reconfiguration {
                ours: None,
                theirs: Some(opposing),
            })
        }
        (false, None) => None,
    }
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// `can_unwrench(user)` with its overrides: `Err(None)` for a machine that never comes off,
/// `Err(Some(why))` for one that cannot now.
fn can_unwrench(world: &World, target: Entity) -> Result<(), Option<String>> {
    let dm = world.get::<DmAtmosDevice>(target).ok_or(None)?;
    if !dm.can_unwrench {
        return Err(None);
    }
    let name = name_of(world, target);
    // `airlock_pump/Initialize(mapload)`: a mapped pump loses `can_unwrench`.
    // shortcut: every airlock pump is mapped until fittings come from the pipe dispenser.
    if is_under(&dm.path, super::airlock_pump::AIRLOCK_PUMP) {
        return Err(Some(format!(
            "Нельзя открутить {name}: он надёжно закреплён на месте!"
        )));
    }
    if let Some(port) = world.get::<PortsConnector>(target)
        && let Some(device) = port.connected
    {
        let device = name_of(world, device);
        return Err(Some(format!(
            "Нельзя открутить {name}: сначала отсоедините {device}!"
        )));
    }
    let on = world.get::<VentPump>(target).is_some_and(|v| v.on)
        || world.get::<VentScrubber>(target).is_some_and(|v| v.on)
        || world.get::<GasPump>(target).is_some_and(|v| v.on)
        || world.get::<VolumePump>(target).is_some_and(|v| v.on)
        || world.get::<PassiveGate>(target).is_some_and(|v| v.on)
        || world.get::<ConditionalGate>(target).is_some_and(|v| v.on)
        || world.get::<GasFilter>(target).is_some_and(|v| v.on)
        || world.get::<GasMixer>(target).is_some_and(|v| v.on)
        || world.get::<OutletInjector>(target).is_some_and(|v| v.on);
    // `is_operational`: powered (and not broken, which atmos devices are not yet).
    if on && world.get::<Unpowered>(target).is_none() {
        return Err(Some(format!(
            "Нельзя открутить {name}: сначала выключите его!"
        )));
    }
    Ok(())
}

/// `wrench_act`'s reading: how far the gas inside is over the tile's (what blows at the user)
/// and whether it holds no gas at all.
fn unwrench_pressure(world: &World, target: Entity) -> Option<(f64, bool)> {
    let device = world.get::<AtmosDevice>(target)?;
    let pos = *world.get::<TilePos>(target)?;
    let net = world.resource::<Pipenet>();
    let d = net.device(device.id)?;
    let atmos = world.resource::<Atmos>();
    let env = atmos.pressure(pos).unwrap_or(0.0);
    let env_moles = atmos.air(pos).map_or(0.0, |a| a.total_moles());
    if d.is_pipe() {
        // `pipe/return_air()`: its pipeline's air.
        let air = net.pipeline_air(device.id, 0);
        let pressure = air.map_or(0.0, |a| a.pressure()) - env;
        return Some((pressure, air.is_none_or(|a| a.total_moles() <= 0.0)));
    }
    // A component's `return_air()` is its tile's: only ports with nothing on the other side
    // (no node, or a port with no portable) blow.
    let mut internal: f64 = 0.0;
    for (i, air) in d.airs.iter().enumerate() {
        let open = match d.nodes.get(i).copied().flatten() {
            None => true,
            Some(node) => net
                .device(node)
                .is_some_and(|n| matches!(n.custom, Custom::Connector { attached: None })),
        };
        if open {
            internal = internal.max(air.pressure() - env);
        }
    }
    let empty = env_moles <= 0.0 || d.airs.iter().all(|a| a.total_moles() <= 0.0);
    Some((internal, empty))
}

/// A wrench on an atmos machine or a meter: starts taking it off. True when the click was
/// theirs.
pub fn wrench_off(world: &mut World, actor: Actor, target: Entity) -> bool {
    if world.get::<Meter>(target).is_some() {
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("Вы начинаете откручивать {}...", name_of(world, target)),
        );
        start(
            world,
            actor,
            target,
            METER_UNWRENCH_DS,
            FittingJob::MeterOff,
        );
        return true;
    }
    if world.get::<AtmosDevice>(target).is_none() {
        return false;
    }
    match can_unwrench(world, target) {
        Ok(()) => {}
        Err(None) => return false,
        Err(Some(why)) => {
            tell(world, actor.client, Span::Warning, why);
            return true;
        }
    }
    let Some((pressure, empty)) = unwrench_pressure(world, target) else {
        return false;
    };
    let name = name_of(world, target);
    if !empty {
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("Вы начинаете откручивать {name}..."),
        );
    }
    if pressure > 2.0 * ONE_ATMOSPHERE {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!(
                "Как только вы начинаете откручивать {name}, вам в лицо бьёт струя воздуха... \
                 может, стоит передумать?"
            ),
        );
    }
    let delay = if empty { 0.0 } else { UNWRENCH_DS };
    start(world, actor, target, delay, FittingJob::Unwrench);
    true
}

fn start(world: &mut World, actor: Actor, target: Entity, delay_ds: f32, job: FittingJob) {
    crate::tools::use_tool(
        world,
        actor,
        target,
        delay_ds,
        0,
        crate::construction::Job::Atmos(super::windows::AtmosJob::Fitting(job)),
    );
}

/// The steps that wait on a wrench.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FittingJob {
    Unwrench,
    MeterOff,
}

/// The end of a [`FittingJob`].
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: FittingJob) {
    let target = action.target;
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let name = name_of(world, target);
    match job {
        FittingJob::MeterOff => {
            if world.get::<Meter>(target).is_none() {
                return;
            }
            tell(
                world,
                action.client,
                Span::Notice,
                format!("Вы откручиваете {name}."),
            );
            drop_meter(world, target, pos);
        }
        FittingJob::Unwrench => {
            // Checked again: it may have been switched on or given a portable meanwhile.
            if let Err(why) = can_unwrench(world, target) {
                if let Some(why) = why {
                    tell(world, action.client, Span::Warning, why);
                }
                return;
            }
            let pressure = unwrench_pressure(world, target).map_or(0.0, |(p, _)| p);
            tell(
                world,
                action.client,
                Span::Notice,
                format!("Вы откручиваете {name}."),
            );
            if pressure > 2.0 * ONE_ATMOSPHERE {
                tell(
                    world,
                    action.client,
                    Span::Danger,
                    "Давление отбрасывает вас!",
                );
                unsafe_pressure_release(world, mob, pos, pressure);
            }
            crate::obj_damage::deconstruct(world, target, true);
        }
    }
}

/// `unsafe_pressure_release`: the user flies away from the machine (a random cardinal when on
/// its tile), `pressure / 250` tiles at `pressure / 1250`.
fn unsafe_pressure_release(world: &mut World, mob: Entity, from: TilePos, pressure: f64) {
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let dir = match ((at.x - from.x).signum(), (at.y - from.y).signum()) {
        (0, 0) => {
            let pick = world
                .get_resource_mut::<crate::combat::CombatRng>()
                .map_or(0, |mut rng| rng.range(0, 3));
            [(0, 1), (0, -1), (1, 0), (-1, 0)][pick as usize]
        }
        d => d,
    };
    let range = (pressure / 250.0) as u32;
    // `get_edge_target_turf`: far enough that the range ends the flight first.
    let reach = range as i32 + 1;
    let target = TilePos::new(at.x + dir.0 * reach, at.y + dir.1 * reach, at.z);
    let now_ds = world.resource::<Time>().elapsed_secs_f64() * 10.0;
    if let Some(path) =
        crate::throw::Trajectory::new(at, target, range, (pressure / 1250.0) as f32, now_ds)
    {
        world.entity_mut(mob).insert(crate::throw::Thrown {
            path,
            thrower: None,
            target: None,
            last_dir: None,
        });
    }
}

/// `atmospherics/on_deconstruction` (from [`crate::obj_damage::deconstruct`]): a portable on
/// its port lets go, its gas goes onto the tile (`relocate_airs`, a pipe's share), it leaves
/// the pipe graph and everything that pointed at it, and a machine that `can_unwrench` leaves
/// its fitting. False when `target` is no atmos device.
pub fn deconstruct(world: &mut World, target: Entity) -> bool {
    let (Some(&pos), Some(device)) = (
        world.get::<TilePos>(target),
        world.get::<AtmosDevice>(target).copied(),
    ) else {
        return false;
    };
    if let Some(portable) = world
        .get::<PortsConnector>(target)
        .and_then(|p| p.connected)
    {
        super::portable::disconnect(world, portable);
    }
    let fitting = fitting_of(world, target);
    let released = world.resource_scope(|_, mut net: Mut<Pipenet>| net.remove(device.id));
    if let Some(gas) = released {
        world.resource_mut::<Atmos>().merge_air(pos, &gas);
    }
    forget(world, target, device, pos);
    if let Some(fitting) = fitting {
        spawn_fitting(world, pos, fitting);
    }
    true
}

/// Everything that pointed at a machine about to go: its area's vent and scrubber lists, its
/// heat exchange partner, the meters reading it (`drop_meter`).
fn forget(world: &mut World, target: Entity, device: AtmosDevice, pos: TilePos) {
    if let Some(mut areas) = world.get_resource_mut::<AreaAtmos>() {
        for lists in areas.0.values_mut() {
            lists.vents.retain(|&e| e != target);
            lists.scrubbers.retain(|&e| e != target);
        }
    }
    if let Some(partner) = world.get::<HeatExchanger>(target).and_then(|h| h.partner)
        && let Some(mut h) = world.get_mut::<HeatExchanger>(partner)
    {
        h.partner = None;
    }
    let meters: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            world
                .get::<Meter>(e)
                .is_some_and(|m| m.target == Some(device.id))
        })
        .collect();
    for meter in meters {
        drop_meter(world, meter, pos);
    }
}

/// `meter/on_deconstruction`: the meter item where it was.
fn drop_meter(world: &mut World, meter: Entity, pos: TilePos) {
    world.despawn(meter);
    if let Some(item) = crate::construction::spawn_obj(world, PIPE_METER_ITEM, pos) {
        world.entity_mut(item).insert(MeterFitting {
            layer: PIPING_LAYER_DEFAULT,
        });
    }
}

/// `make_from_existing`: the fitting a machine leaves.
fn fitting_of(world: &World, target: Entity) -> Option<PipeFitting> {
    let dm = world.get::<DmAtmosDevice>(target)?;
    if !dm.can_unwrench {
        return None;
    }
    let device = world.get::<AtmosDevice>(target)?;
    let d = world.resource::<Pipenet>().device(device.id)?;
    let kind = FittingKind::of(&dm.construction_type);
    let flipped = world
        .get::<GasFilter>(target)
        .map(|f| f.flipped)
        .or_else(|| world.get::<GasMixer>(target).map(|m| m.flipped))
        .unwrap_or(dm.flipped);
    let mut fitting = PipeFitting {
        pipe_type: dm.path.clone(),
        kind,
        dir: device.dir,
        init_dir: d.init_dirs,
        layer: device.layer,
        color: d.color,
        flipped: false,
    };
    // `trinary/flippable/make_from_existing`: `do_a_flip`.
    if kind == FittingKind::Flippable && flipped {
        flip(&mut fitting);
    }
    Some(fitting)
}

/// `do_a_flip`: a flippable fitting tilts 45° (back when flipped), others reverse.
fn flip(fitting: &mut PipeFitting) {
    if fitting.kind == FittingKind::Flippable {
        fitting.dir = turn(fitting.dir, if fitting.flipped { 45 } else { -45 });
        fitting.flipped = !fitting.flipped;
    } else {
        fitting.dir = super::reverse_dir(fitting.dir);
    }
}

/// `new construction_type(loc, ...)` + `update`: the item, named after its machine, drawn as
/// the machine's `pipe_state`.
pub fn spawn_fitting(world: &mut World, pos: TilePos, mut fitting: PipeFitting) -> Option<Entity> {
    let proto = world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(&fitting.pipe_type))
        .cloned()?;
    // `set_piping_layer`: machines on every layer sit on the default one.
    if proto
        .get::<DmAtmosDevice>()
        .is_some_and(|d| d.pipe_flags as u16 & flags::ALL_LAYER != 0)
    {
        fitting.layer = PIPING_LAYER_DEFAULT;
    }
    let machine_name = proto.get::<DmName>().cloned().unwrap_or_default();
    let state = proto
        .get::<DmAtmosDevice>()
        .map(|d| d.pipe_state.clone())
        .unwrap_or_default();
    let item = crate::construction::spawn_obj(world, PIPE_FITTING, pos)?;
    if let Some(mut name) = world.get_mut::<DmName>(item) {
        name.name = format!("{} fitting", crate::vending::plain_name(&machine_name.name));
        name.desc = machine_name.desc;
    }
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.0.get(&(PIPE_ITEM_DMI.to_owned(), state)).copied());
    if let Some(mut a) = world.get_mut::<Appearance>(item) {
        if let Some(icon) = icon {
            a.icon_state = icon;
        }
        a.color = Some([fitting.color[0], fitting.color[1], fitting.color[2], 255]);
    }
    world.entity_mut(item).insert(fitting);
    show_fitting(world, item);
    Some(item)
}

/// `setDir` and `set_piping_layer` of a fitting: its facing and layer shift.
fn show_fitting(world: &mut World, item: Entity) {
    let Some(fitting) = world.get::<PipeFitting>(item).cloned() else {
        return;
    };
    let shift = (i16::from(fitting.layer) - i16::from(PIPING_LAYER_DEFAULT)) * PIPING_LAYER_P;
    if let Some(mut a) = world.get_mut::<Appearance>(item) {
        a.pixel_offset = [shift, shift];
    }
    if let Some(dir) = Dir::from_bits(fitting.dir) {
        world.entity_mut(item).insert(Facing(dir));
    }
}

/// `pipe/attack_self`: a quarter turn clockwise.
pub fn attack_self(world: &mut World, item: Entity) -> bool {
    let Some(mut fitting) = world.get_mut::<PipeFitting>(item) else {
        return false;
    };
    fitting.dir = turn(fitting.dir, -90);
    show_fitting(world, item);
    true
}

/// `pipe/attack_hand_secondary`: the next layer (`set_piping_layer`; all-layer machines stay
/// on the default one).
pub fn next_layer(world: &mut World, client: Entity, item: Entity) -> bool {
    let all_layers = world
        .get::<PipeFitting>(item)
        .and_then(|f| {
            world
                .get_resource::<ss13_proto::Prototypes>()?
                .get(&f.pipe_type)?
                .get::<DmAtmosDevice>()
                .map(|d| d.pipe_flags as u16 & flags::ALL_LAYER != 0)
        })
        .unwrap_or(false);
    let Some(mut fitting) = world.get_mut::<PipeFitting>(item) else {
        return false;
    };
    fitting.layer = if all_layers {
        PIPING_LAYER_DEFAULT
    } else if fitting.layer >= PIPING_LAYER_MAX {
        PIPING_LAYER_MIN
    } else {
        fitting.layer + 1
    };
    let layer = fitting.layer;
    show_fitting(world, item);
    tell(world, client, Span::Notice, format!("Слой трубы: {layer}."));
    true
}

/// `pipe/examine`, `pipe_meter`'s has none.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    if let Some(fitting) = world.get::<PipeFitting>(target) {
        lines.push((Span::Notice, format!("Слой трубы: {}.", fitting.layer)));
        lines.push((
            Span::Notice,
            "Слой можно сменить, нажав на фитинг правой кнопкой.".to_owned(),
        ));
    }
}

/// The machine a fitting builds: the type's vars with the fitting's layer, color, flip and
/// directions, switched off (`build_pipe`).
fn built_device(world: &World, fitting: &PipeFitting) -> Option<DmAtmosDevice> {
    let mut dm = world
        .get_resource::<ss13_proto::Prototypes>()?
        .get(&fitting.pipe_type)?
        .get::<DmAtmosDevice>()?
        .clone();
    dm.on = false;
    dm.piping_layer = f64::from(fitting.layer);
    dm.pipe_color = format!(
        "#{:02X}{:02X}{:02X}",
        fitting.color[0], fitting.color[1], fitting.color[2]
    );
    dm.flipped = fitting.flipped;
    dm.initialize_directions = f64::from(fitting.init_dir);
    Some(dm)
}

/// A wrench on a fitting or a meter item lying on the floor: builds it. True when the click
/// was theirs.
pub fn wrench_on(world: &mut World, client: Entity, item: Entity) -> bool {
    if let Some(meter) = world.get::<MeterFitting>(item).copied() {
        attach_meter(world, client, item, meter);
        return true;
    }
    let Some(fitting) = world.get::<PipeFitting>(item).cloned() else {
        return false;
    };
    // `if(!isturf(loc))`: a held or stored fitting does nothing.
    if world.get::<crate::inventory::CarriedBy>(item).is_some() {
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(item) else {
        return true;
    };
    match build(world, pos, fitting) {
        Ok(_) => {
            world.despawn(item);
            crate::construction::tile_changed(world, pos);
            tell(world, client, Span::Notice, "Вы прикручиваете фитинг.");
        }
        Err(why) => tell(world, client, Span::Warning, why),
    }
    true
}

/// `pipe/wrench_act` from the tile checks on: the machine, connected, or why not.
pub fn build(
    world: &mut World,
    pos: TilePos,
    mut fitting: PipeFitting,
) -> Result<Entity, &'static str> {
    const HOGGED: &str = "Что-то занимает эту клетку!";
    const TAKEN: &str = "Здесь уже есть труба!";
    let dir = fitting.kind.fixed_dir(fitting.dir);
    let dm = built_device(world, &fitting).ok_or(TAKEN)?;
    let our_flags = dm.pipe_flags as u16;
    let smart = is_under(&fitting.pipe_type, super::SMART_PIPE);
    // Which machines on the tile it could clash with.
    let conflicting: Vec<_> = {
        let net = world.resource::<Pipenet>();
        let mut out = Vec::new();
        for &id in net.devices_on(pos) {
            let Some(d) = net.device(id) else { continue };
            if d.flags & our_flags & flags::ONE_PER_TURF != 0 {
                return Err(HOGGED);
            }
            if d.layer != fitting.layer && (d.flags | our_flags) & flags::ALL_LAYER == 0 {
                continue;
            }
            out.push(id);
        }
        out
    };
    let mut narrowed = None;
    for &id in &conflicting {
        let now = built_device(world, &fitting).ok_or(TAKEN)?;
        let ours = our_init_dirs(pos, &now, dir).ok_or(TAKEN)?;
        let net = world.resource::<Pipenet>();
        let Some(d) = net.device(id) else { continue };
        if d.init_dirs & ours == 0 {
            continue;
        }
        let other_smart = matches!(d.kind, DeviceKind::Pipe { smart: true, .. }).then(|| {
            let connections = d.connections(net);
            SmartPipe {
                dir: super::looks::smart_pipe_icon(connections, d.init_dirs).1,
                connections,
            }
        });
        let solved = (conflicting.len() == 1)
            .then(|| smart_reconfiguration(smart, ours, fitting.init_dir, d.init_dirs, other_smart))
            .flatten()
            .ok_or(TAKEN)?;
        if let Some(ours) = solved.ours {
            fitting.init_dir = ours;
        }
        narrowed = solved.theirs.map(|theirs| (id, theirs));
    }
    if let Some((id, theirs)) = narrowed {
        narrow_smart_pipe(world, id, theirs);
    }
    let dm = built_device(world, &fitting).ok_or(TAKEN)?;
    let machine = crate::construction::spawn_obj(world, &fitting.pipe_type, pos).ok_or(TAKEN)?;
    world.entity_mut(machine).insert(dm.clone());
    let Some((spec, dir)) = super::device_spec(machine, pos, &dm, dir) else {
        world.despawn(machine);
        return Err(TAKEN);
    };
    let layer = spec.layer;
    let (pipe, he) = match spec.kind {
        DeviceKind::Pipe { he, .. } => (true, he),
        _ => (false, ss13_atmos::pipenet::HeatExchange::None),
    };
    let id = world.resource_mut::<Pipenet>().add(spec);
    world
        .entity_mut(machine)
        .insert((AtmosDevice { id, layer, dir }, super::looks::LookDirty));
    // `on_construction`: `add_atom_colour(obj_color)` of what can be wrenched off; components
    // draw their pipe color from the graph, plain pipes from their atom color.
    if dm.can_unwrench
        && pipe
        && matches!(he, ss13_atmos::pipenet::HeatExchange::None)
        && let Some(mut a) = world.get_mut::<Appearance>(machine)
    {
        a.color = Some([fitting.color[0], fitting.color[1], fitting.color[2], 255]);
    }
    if let Some(facing) = Dir::from_bits(dir) {
        world.entity_mut(machine).insert(Facing(facing));
    }
    let mut areas = world.remove_resource::<AreaAtmos>().unwrap_or_default();
    super::install_device(world, machine, &dm, pos, he, &mut areas);
    world.insert_resource(areas);
    if world.get::<HeatExchanger>(machine).is_some() {
        super::devices::pair_heat_exchangers(world);
    }
    Ok(machine)
}

/// `SSair.get_init_dirs(pipe_type, fixed_dir(), p_init_dir)`.
fn our_init_dirs(pos: TilePos, dm: &DmAtmosDevice, dir: u8) -> Option<u8> {
    super::device_spec(Entity::PLACEHOLDER, pos, dm, dir).map(|(spec, _)| spec.init_dirs)
}

/// A smart pipe's `set_init_directions(theirs)` + `update_pipe_icon`; its live connections
/// stay among `theirs`, directions it newly allows connect at once.
pub(super) fn narrow_smart_pipe(world: &mut World, id: ss13_atmos::pipenet::DeviceId, theirs: u8) {
    let Some((entity, layer, color)) = world
        .resource::<Pipenet>()
        .device(id)
        .map(|d| (d.entity, d.layer, d.color))
    else {
        return;
    };
    let new = world.resource_mut::<Pipenet>().reconnect(
        id,
        theirs,
        default_node_dirs(4, theirs),
        layer,
        color,
    );
    let Some(new) = new else { return };
    if let Some(mut device) = world.get_mut::<AtmosDevice>(entity) {
        device.id = new;
    }
    let mut q = world.query::<&mut Meter>();
    for mut meter in q.iter_mut(world) {
        if meter.target == Some(id) {
            meter.target = Some(new);
        }
    }
}

/// `pipe_meter/wrench_act`: onto the pipe of its layer on its tile.
fn attach_meter(world: &mut World, client: Entity, item: Entity, meter: MeterFitting) {
    if world.get::<crate::inventory::CarriedBy>(item).is_some() {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(item) else {
        return;
    };
    let target = {
        let net = world.resource::<Pipenet>();
        net.devices_on(pos).iter().copied().find(|&id| {
            net.device(id)
                .is_some_and(|d| d.is_pipe() && d.layer == meter.layer)
        })
    };
    let Some(target) = target else {
        tell(
            world,
            client,
            Span::Warning,
            "Счётчик нужно прикрутить к трубе!",
        );
        return;
    };
    let Some(machine) = crate::construction::spawn_obj(world, METER, pos) else {
        return;
    };
    let shift = (i16::from(meter.layer) - i16::from(PIPING_LAYER_DEFAULT)) * PIPING_LAYER_P;
    if let Some(mut a) = world.get_mut::<Appearance>(machine) {
        a.pixel_offset = [shift, shift];
    }
    world.entity_mut(machine).insert(Meter {
        target: Some(target),
        look: None,
    });
    world.despawn(item);
    tell(
        world,
        client,
        Span::Notice,
        "Вы прикручиваете счётчик к трубе.",
    );
}

/// What a wrench does here: on a fitting on the floor, on an atmos machine or a meter. True
/// when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    actor: Actor,
    target: Entity,
    tool: Option<ToolKind>,
) -> bool {
    if tool != Some(ToolKind::Wrench) {
        return false;
    }
    wrench_on(world, actor.client, target) || wrench_off(world, actor, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_follows_byond() {
        assert_eq!(turn(1, -90), 4);
        assert_eq!(turn(4, -90), 2);
        assert_eq!(turn(8, -90), 1);
        assert_eq!(turn(1, 45), 9);
        assert_eq!(turn(1, -45), 5);
        assert_eq!(turn(5, 45), 1);
    }

    #[test]
    fn fitting_directions_map_onto_the_machine() {
        assert_eq!(FittingKind::Binary.fixed_dir(2), 1);
        assert_eq!(FittingKind::Binary.fixed_dir(8), 4);
        assert_eq!(FittingKind::Directional.fixed_dir(2), 2);
        // A flipped filter fitting lies diagonally and builds facing its original way.
        let mut f = PipeFitting {
            pipe_type: String::new(),
            kind: FittingKind::Flippable,
            dir: 1,
            init_dir: 0,
            layer: 3,
            color: [0; 3],
            flipped: false,
        };
        flip(&mut f);
        assert_eq!((f.dir, f.flipped), (5, true));
        assert_eq!(f.kind.fixed_dir(f.dir), 1);
        flip(&mut f);
        assert_eq!((f.dir, f.flipped), (1, false));
    }

    #[test]
    fn a_smart_pipe_crosses_a_straight_one() {
        // An east-west pipe allowing all four, a new all-four pipe: north-south over it.
        let straight = SmartPipe {
            dir: 4,
            connections: 4 | 8,
        };
        assert_eq!(
            smart_reconfiguration(true, 15, 15, 15, Some(straight)),
            Some(Reconfiguration {
                ours: Some(1 | 2),
                theirs: Some(4 | 8),
            })
        );
        // The new one may not go north-south: no way.
        assert_eq!(
            smart_reconfiguration(true, 4 | 8, 4 | 8, 15, Some(straight)),
            None
        );
    }

    #[test]
    fn a_smart_pipe_bounces_off_a_bent_one() {
        // Bent north-east: the new one takes south-west.
        let bent = SmartPipe {
            dir: 1 | 4,
            connections: 1 | 4,
        };
        assert_eq!(
            smart_reconfiguration(true, 15, 15, 15, Some(bent)),
            Some(Reconfiguration {
                ours: Some(2 | 8),
                theirs: Some(1 | 4),
            })
        );
        // A T joint leaves one direction: a stub is not allowed.
        let tee = SmartPipe {
            dir: 1 | 4 | 8,
            connections: 1 | 4 | 8,
        };
        assert_eq!(smart_reconfiguration(true, 15, 15, 15, Some(tee)), None);
    }

    #[test]
    fn a_device_narrows_a_smart_pipe_it_does_not_cut() {
        // A vent facing north on a smart pipe connected east-west: the pipe gives up north
        // and south... but keeps two directions only when they are its live ones.
        let straight = SmartPipe {
            dir: 4,
            connections: 4 | 8,
        };
        assert_eq!(
            smart_reconfiguration(false, 1, 0, 15, Some(straight)),
            Some(Reconfiguration {
                ours: None,
                theirs: Some(2 | 4 | 8),
            })
        );
        // Facing east, into a live connection: no.
        assert_eq!(smart_reconfiguration(false, 4, 0, 15, Some(straight)), None);
        // A smart pipe onto a device: it takes what the device leaves.
        assert_eq!(
            smart_reconfiguration(true, 15, 15, 1, None),
            Some(Reconfiguration {
                ours: Some(2 | 4 | 8),
                theirs: None,
            })
        );
    }
}
