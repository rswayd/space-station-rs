//! Cryo cells (server): tg `/obj/machinery/cryo_cell`
//! (`code/modules/atmospherics/machinery/components/unary_devices/cryo.dm`) and its `Cryo`
//! window (`ss13_tgui::interfaces::cryo`). The door is [`super::pods`]'.
//!
//! - The cell is a unary pipe machine facing its `dir` (tg `gas_machine_connector`): a 1250 L
//!   port on the default layer joins the pipe network once it is built.
//! - Alt-click (not from its own tile) opens or closes the door; closing it on a mob turns the
//!   cell on, opening turns it off. A carbon dragged onto it is shut in at once when
//!   incapacitated, else after a 2.5 s shove. Ctrl-click switches a working closed cell on or
//!   off. A cup (`/obj/item/reagent_containers/cup`) clicked on it goes in when it has none.
//! - An empty hand on a working cell opens the window (not from its tile, not for the occupant):
//!   power, door, auto-eject and beaker eject.
//! - Every SSair fire a running cell needs a pipe on its port and more than 5 moles in it, else
//!   it stops (and ejects with auto-eject); over 2000 K the gas burns it; the occupant's body
//!   temperature and the gas share heat (`CALCULATE_CONDUCTION_ENERGY` with the cell's heat
//!   capacity and conduction from the matter bins, less the clothing's cold protection), the
//!   core temperature follows the skin. The occupant breathes the cell's gas and takes its
//!   temperature (`crate::breath`).
//! - Every machine tick a running cell puts out fires on the occupant, stops when it is dead or
//!   fully healed (without wounds left) and then ejects it with auto-eject, and otherwise moves
//!   `0.5 / (efficiency × 25)` u a second of the beaker into the occupant's blood at 25 ×
//!   efficiency (`trans_to(..., methods = VAPOR)`). Cryoxadone heals a body under `T0C`
//!   (`ss13_reagents::metabolism`). Reports go to the medical channel.
//! - A step of the occupant starts a 30 s breakout that opens the cell if it is still inside
//!   and conscious.
// shortcut: no sounds, sound loop, flick animations or occupant drawn in the tube (the
// occupant is hidden like in a closet); no vapor exposure effects (`expose_mob`), no screwdriver,
// crowbar or wrench rules (the machine frame ones apply; no prying a closed unpowered cell, no
// pressure release), no examine lines, no idle/active power switch; a moved or rotated cell
// keeps its first port.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_atmos::pipenet::{COLOR_OMNI, Custom, DeviceId, DeviceKind, DeviceSpec, Pipenet, flags};
use ss13_atmos::{Atmos, AtmosSystems, GasMixture};
use ss13_mapload::{DmAtom, DmItem, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::ui::UiStatus;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::cryo::{CryoBeaker, CryoData, CryoOccupant, CryoUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
};

use super::pods::{Pod, close_pod, occupant, open_pod, operational, panel_open};
use crate::breath::{Respiration, Worn};
use crate::chem::{Chem, MobReagents, Reagents};
use crate::chem_dispenser::{InsertedBeaker, beaker_of, replace_beaker};
use crate::closets::InCloset;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::power::{MACHINE_TICK_SECS, MachineTicks};
use crate::stock_parts::PartKind;
use crate::tools::tell;

pub const CRYO_ROOT: &str = "/obj/machinery/cryo_cell";
const CUP_ROOT: &str = "/obj/item/reagent_containers/cup";
/// `CRYO_MULTIPLY_FACTOR`, `CRYO_TX_QTY`.
const MULTIPLY_FACTOR: f32 = 25.0;
const TX_QTY: f32 = 0.5;
/// `CRYO_MIN_GAS_MOLES`.
pub const MIN_GAS_MOLES: f64 = 5.0;
/// `CRYO_BREAKOUT_TIME`.
const BREAKOUT_SECS: f64 = 30.0;
/// `mouse_drop_receive`'s shove `do_after`.
const SHOVE_SECS: f64 = 2.5;
/// `MAX_TEMPERATURE`.
const MAX_TEMPERATURE: f64 = 4000.0;
/// Over this the gas burns the cell.
const BURN_TEMPERATURE: f64 = 2000.0;
/// `initial(heat_capacity)`, `initial(conduction_coefficient)`.
const HEAT_CAPACITY: f64 = 20000.0;
const CONDUCTION_COEFFICIENT: f64 = 0.3;
/// `CELL_VOLUME * 0.5`: the port's litres.
const PORT_VOLUME: f64 = 1250.0;
/// `PIPING_LAYER_DEFAULT`.
const LAYER: u8 = 3;
/// `TCMB`.
const TCMB: f64 = 2.7;
const COVER_LAYER_STATES: [&str; 2] = ["cover-on", "cover-off"];

/// A cryo cell's live state; its door is its [`Pod`], its beaker its [`InsertedBeaker`].
#[derive(Component, Clone, Debug, PartialEq)]
#[require(InsertedBeaker)]
pub struct CryoCell {
    pub on: bool,
    pub autoeject: bool,
    pub treating_wounds: bool,
    /// The pipe port (tg `internal_connector`) once the pipe network has it.
    pub device: Option<DeviceId>,
    /// A breakout in progress: the occupant and when it ends (game seconds).
    pub breakout: Option<(Entity, f64)>,
    /// A shove in progress: (user, target, ends at, user's tile, target's tile).
    pub shove: Option<(Entity, Entity, f64, TilePos, TilePos)>,
    view: CryoData,
}

impl Default for CryoCell {
    fn default() -> Self {
        Self {
            on: false,
            autoeject: true,
            treating_wounds: false,
            device: None,
            breakout: None,
            shove: None,
            view: CryoData::default(),
        }
    }
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // tg `cryo_cell/item_interaction`: a cup goes in.
    app.add_click_handler(ClickPhase::HeldOnTarget, 2610, |w, c| {
        c.held.is_some_and(|h| {
            crate::medical::cryo::item_interaction(w, c.client, c.mob, h, c.target)
        })
    });
    app.register_ui::<CryoWindow>()
        .add_observer(disconnect_on_despawn)
        .add_systems(
            Update,
            (
                cap_windows.in_set(UiSystems::StatusHook),
                acts.in_set(UiSystems::Handle),
                refresh_views
                    .after(UiSystems::Handle)
                    .before(UiSystems::Send),
            ),
        )
        .add_systems(
            Update,
            (
                connect_cells,
                ctrl_clicks,
                timers,
                process_cells.after(crate::power::PowerSystems),
                process_atmos
                    .run_if(ss13_atmos::air_tick_due)
                    .after(AtmosSystems::Machinery),
            )
                .run_if(any_with_component::<CryoCell>),
        );
}

/// `RefreshParts`: (`efficiency`, `heat_capacity`, `conduction_coefficient`) from the matter
/// bins' tiers (at least one, as a cell without bins would divide by zero).
pub fn part_numbers(bin_tiers: u32) -> (f32, f64, f64) {
    let tier = bin_tiers.max(1);
    (
        tier as f32,
        HEAT_CAPACITY / f64::from(tier),
        CONDUCTION_COEFFICIENT * f64::from(tier),
    )
}

fn parts(world: &World, e: Entity) -> (f32, f64, f64) {
    let r = crate::stock_parts::ratings(world, e);
    part_numbers(r.sum(PartKind::MatterBin))
}

/// The heat moved from the gas into the body this fire (`process_atmos`): positive warms the
/// body. `None` when they are within a kelvin.
pub fn conducted_heat(
    gas_temperature: f64,
    gas_heat_capacity: f64,
    body_temperature: f64,
    cold_protection: f64,
    heat_capacity: f64,
    conduction: f64,
) -> Option<f64> {
    let delta = gas_temperature - body_temperature;
    if delta.abs() <= 1.0 || gas_heat_capacity <= 0.0 {
        return None;
    }
    // `CALCULATE_CONDUCTION_ENERGY(delta, heat_capacity, air_heat_capacity)`.
    let energy = delta * (heat_capacity * gas_heat_capacity / (heat_capacity + gas_heat_capacity));
    Some(((1.0 - cold_protection) * 0.1 + conduction) * energy)
}

/// `(CRYO_TX_QTY / (efficiency * CRYO_MULTIPLY_FACTOR)) * seconds`: units drawn from the beaker,
/// and the multiplier they reach the occupant with.
pub fn beaker_dose(efficiency: f32, seconds: f32) -> (f32, f32) {
    let multiplier = efficiency * MULTIPLY_FACTOR;
    (TX_QTY / multiplier * seconds, multiplier)
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// The cryo cell `mob` is in.
pub fn holder(world: &World, mob: Entity) -> Option<Entity> {
    let cell = world.get::<InCloset>(mob)?.closet;
    world.get::<CryoCell>(cell).map(|_| cell)
}

fn port_air(world: &World, cell: Entity) -> Option<&ss13_atmos::pipenet::PipeAir> {
    let id = world.get::<CryoCell>(cell)?.device?;
    world.get_resource::<Pipenet>()?.air(id, 0)
}

/// `return_temperature`: the gas's temperature when it holds more than 5 moles.
pub fn return_temperature(world: &World, cell: Entity) -> Option<f64> {
    port_air(world, cell)
        .filter(|a| a.total_moles() > MIN_GAS_MOLES)
        .map(|a| a.temperature())
}

/// `handle_internal_lifeform(breath_request)`: `breath_request / volume` of the gas's moles.
pub fn handle_internal_lifeform(
    world: &mut World,
    cell: Entity,
    volume: f64,
) -> Option<GasMixture> {
    let id = world.get::<CryoCell>(cell)?.device?;
    let mut net = world.get_resource_mut::<Pipenet>()?;
    let air = net.air_mut(id, 0)?;
    let moles = air.total_moles() * (volume / air.volume.max(1.0));
    let breath = air.remove(moles);
    net.update_parents(id);
    breath
}

/// `assume_air`: the exhaled breath into the gas.
pub fn assume_air(world: &mut World, cell: Entity, giver: &GasMixture) {
    let Some(id) = world.get::<CryoCell>(cell).and_then(|c| c.device) else {
        return;
    };
    if let Some(mut net) = world.get_resource_mut::<Pipenet>() {
        if let Some(air) = net.air_mut(id, 0) {
            air.merge(giver);
        }
        net.update_parents(id);
    }
}

/// tg `gas_machine_connector/New`: the port joins the pipe network once it is built.
// Cost: per tick a query over the cells not connected yet (a handful per map).
fn connect_cells(world: &mut World) {
    if !world
        .get_resource::<Pipenet>()
        .is_some_and(Pipenet::is_built)
    {
        return;
    }
    let pending: Vec<(Entity, TilePos, u8)> = world
        .query::<(Entity, &CryoCell, &TilePos, Option<&DmAtom>)>()
        .iter(world)
        .filter(|(_, c, ..)| c.device.is_none())
        .map(|(e, _, &pos, atom)| (e, pos, atom.map_or(2, |a| a.dir)))
        .collect();
    for (e, pos, dir) in pending {
        let dir = if matches!(dir, 1 | 2 | 4 | 8) { dir } else { 2 };
        let id = world.resource_mut::<Pipenet>().add(DeviceSpec {
            entity: e,
            pos,
            init_dirs: dir,
            node_dirs: vec![dir],
            layer: LAYER,
            color: COLOR_OMNI,
            flags: flags::ONE_PER_TURF,
            kind: DeviceKind::Component,
            port_volumes: vec![PORT_VOLUME],
            custom: Custom::None,
        });
        if let Some(mut cell) = world.get_mut::<CryoCell>(e) {
            cell.device = Some(id);
        }
    }
}

/// tg `destroy_connected_machine` + `on_deconstruction`: the port leaves the network, its gas
/// goes to the tile, the beaker drops.
fn disconnect_on_despawn(
    remove: On<Remove<CryoCell>>,
    cells: Query<(&CryoCell, &InsertedBeaker, Option<&TilePos>)>,
    mut commands: Commands,
) {
    let Ok((cell, beaker, pos)) = cells.get(remove.entity) else {
        return;
    };
    let (device, beaker, pos) = (cell.device, beaker.0, pos.copied());
    commands.queue(move |world: &mut World| {
        let gas = device.and_then(|id| world.get_resource_mut::<Pipenet>()?.remove(id));
        let Some(pos) = pos else {
            return;
        };
        if let (Some(gas), Some(mut atmos)) = (gas, world.get_resource_mut::<Atmos>()) {
            atmos.merge_air(pos, &gas);
        }
        if let Some(beaker) = beaker.filter(|&b| world.get_entity(b).is_ok()) {
            crate::storage::take_out(world, beaker, pos);
        }
    });
}

/// `set_on(active)`.
fn set_on(world: &mut World, e: Entity, on: bool) {
    let Some(mut cell) = world.get_mut::<CryoCell>(e) else {
        return;
    };
    if cell.on == on {
        return;
    }
    cell.on = on;
    refresh_look(world, e);
}

/// `aas_config_announce(/datum/aas_config_entry/medical_cryo_announcements)` on the medical
/// channel.
fn announce(world: &mut World, e: Entity, line: &str, ejecting: bool) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let text = if ejecting {
        format!("{line} Auto ejecting patient now.")
    } else {
        line.to_owned()
    };
    let name = super::name_of(world, e);
    world.write_message(crate::radio::MachineRadio {
        freq: ss13_chat::radio::FREQ_MEDICAL,
        z: pos.z,
        name,
        text,
    });
}

/// `open_machine`: off unless the panel is open (tg's check of the closed state).
pub fn open(world: &mut World, e: Entity) {
    if world.get::<Pod>(e).is_some_and(|p| !p.open) && !panel_open(world, e) {
        set_on(world, e, false);
    }
    if let Some(mut cell) = world.get_mut::<CryoCell>(e) {
        cell.breakout = None;
    }
    open_pod(world, e);
    refresh_view(world, e);
}

/// `close_machine(user)`: an occupant turns it on.
pub fn close(world: &mut World, e: Entity, target: Option<Entity>) {
    if let Some(mut cell) = world.get_mut::<CryoCell>(e) {
        cell.treating_wounds = false;
    }
    if world.get::<Pod>(e).is_none_or(|p| !p.open) || panel_open(world, e) {
        return;
    }
    close_pod(world, e, target);
    if occupant(world, e).is_some() {
        set_on(world, e, true);
    }
    refresh_view(world, e);
}

/// `update_icon_state` + `update_overlays`: `pod-open`, `pod-on` (on and working) or `pod-off`;
/// the panel and, closed, the cover.
pub fn refresh_look(world: &mut World, e: Entity) {
    let Some(cell) = world.get::<CryoCell>(e) else {
        return;
    };
    let running = cell.on && operational(world, e);
    let open = world.get::<Pod>(e).is_some_and(|p| p.open);
    let state = if open {
        "pod-open"
    } else if running {
        "pod-on"
    } else {
        "pod-off"
    };
    crate::obj_damage::set_icon_state(world, e, state);
    let mut states: Vec<&str> = Vec::new();
    if panel_open(world, e) {
        states.push("pod-panel");
    }
    if !open {
        states.push(COVER_LAYER_STATES[usize::from(!running)]);
    }
    let overlays: Vec<Overlay> = {
        let icon = world.get::<DmAtom>(e).map(|a| a.icon.clone());
        let icons = world.get_resource::<IconIndex>();
        states
            .iter()
            .filter_map(|s| icons.zip(icon.as_ref()).and_then(|(i, f)| i.get(f, s)))
            .map(Overlay::new)
            .collect()
    };
    if let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

fn same_tile(world: &World, a: Entity, b: Entity) -> bool {
    matches!((world.get::<TilePos>(a), world.get::<TilePos>(b)), (Some(x), Some(y)) if x == y)
}

/// `attack_hand` -> `ui_interact`: the window of a working cell, not from its own tile
/// (`can_interact`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    if !operational(world, e) || same_tile(world, mob, e) {
        return;
    }
    refresh_view(world, e);
    let title = super::name_of(world, e);
    world.write_message(OpenUi::new::<CryoWindow>(client, mob, e, title));
}

/// `click_alt`: the door, not from the cell's own tile ("so players don't close the cryo on
/// themselves without a doctor's help").
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    if same_tile(world, mob, e) {
        return;
    }
    if world.get::<Pod>(e).is_some_and(|p| p.open) {
        close(world, e, None);
    } else {
        open(world, e);
    }
    let text = if world.get::<Pod>(e).is_some_and(|p| p.open) {
        "дверь открыта"
    } else {
        "дверь закрыта"
    };
    tell(world, client, Span::Notice, text);
}

/// `mouse_drop_receive`: an incapacitated carbon goes in at once, another after a shove.
pub fn mouse_drop_receive(world: &mut World, user: Entity, e: Entity, target: Entity) {
    if world.get::<super::wounds::Limbs>(target).is_none() {
        return;
    }
    if crate::status_effects::incapacitation(world, target).incapacitated
        || world
            .get::<Stat>(target)
            .is_some_and(|s| *s != Stat::Conscious)
    {
        close(world, e, Some(target));
        return;
    }
    // tg's `isliving` branch returns before its shove for every carbon; the shove is ported as
    // intended.
    let (Some(&user_pos), Some(&target_pos)) =
        (world.get::<TilePos>(user), world.get::<TilePos>(target))
    else {
        return;
    };
    let ends = now(world) + SHOVE_SECS;
    if let Some(mut cell) = world.get_mut::<CryoCell>(e) {
        cell.shove = Some((user, target, ends, user_pos, target_pos));
    }
    let (who, whom, what) = (
        super::name_of(world, user),
        super::name_of(world, target),
        super::name_of(world, e),
    );
    super::visible(
        world,
        user,
        &format!("Вы начинаете заталкивать {whom} в {what}."),
        &format!("{who} начинает заталкивать {whom} в {what}."),
        Span::Notice,
    );
}

/// `container_resist_act` (a step of the occupant): a 30 s breakout.
pub fn container_resist(world: &mut World, mob: Entity, e: Entity) {
    if world
        .get::<CryoCell>(e)
        .is_none_or(|c| c.breakout.is_some_and(|(m, _)| m == mob))
    {
        return;
    }
    let ends = now(world) + BREAKOUT_SECS;
    if let Some(mut cell) = world.get_mut::<CryoCell>(e) {
        cell.breakout = Some((mob, ends));
    }
    let (who, what) = (super::name_of(world, mob), super::name_of(world, e));
    super::visible(
        world,
        mob,
        &format!(
            "Вы толкаетесь внутри {what}, нажимая ногой на защёлку... (это займёт около 30 \
             секунд.)"
        ),
        &format!("{who} бьёт ногами по стеклу {what}!"),
        Span::Notice,
    );
}

/// The breakouts and shoves that are due, revalidated at the moment of effect.
fn timers(world: &mut World) {
    let now = now(world);
    let cells: Vec<(Entity, CryoCell)> = world
        .query::<(Entity, &CryoCell)>()
        .iter(world)
        .filter(|(_, c)| c.breakout.is_some() || c.shove.is_some())
        .map(|(e, c)| (e, c.clone()))
        .collect();
    for (e, cell) in cells {
        if let Some((mob, ends)) = cell.breakout
            && now >= ends
        {
            if let Some(mut c) = world.get_mut::<CryoCell>(e) {
                c.breakout = None;
            }
            let inside = world.get::<InCloset>(mob).is_some_and(|c| c.closet == e);
            let awake = world.get::<Stat>(mob) == Some(&Stat::Conscious)
                && !crate::status_effects::incapacitation(world, mob).knocked_out;
            if inside && awake {
                let (who, what) = (super::name_of(world, mob), super::name_of(world, e));
                super::visible(
                    world,
                    mob,
                    &format!("Вы выбираетесь из {what}!"),
                    &format!("{who} выбирается из {what}!"),
                    Span::Warning,
                );
                open(world, e);
            }
        }
        if let Some((user, target, ends, user_pos, target_pos)) = cell.shove
            && now >= ends
        {
            if let Some(mut c) = world.get_mut::<CryoCell>(e) {
                c.shove = None;
            }
            let stayed = world.get::<TilePos>(user) == Some(&user_pos)
                && world.get::<TilePos>(target) == Some(&target_pos)
                && world.get::<Stat>(user) == Some(&Stat::Conscious)
                && !crate::status_effects::incapacitation(world, user).incapacitated
                && world.get::<InCloset>(target).is_none();
            if stayed {
                close(world, e, Some(target));
            }
        }
    }
}

/// `click_ctrl`: a working closed cell on or off.
fn ctrl_clicks(world: &mut World, mut clicks: Local<MessageCursor<crate::click::CtrlClick>>) {
    let pending: Vec<(Entity, Entity)> = clicks
        .read(world.resource::<Messages<crate::click::CtrlClick>>())
        .map(|c| (c.client, c.target))
        .collect();
    for (client, e) in pending {
        let Some(on) = world.get::<CryoCell>(e).map(|c| c.on) else {
            continue;
        };
        if !operational(world, e) || world.get::<Pod>(e).is_none_or(|p| p.open) {
            continue;
        }
        set_on(world, e, !on);
        refresh_view(world, e);
        let text = if on {
            "выключено"
        } else {
            "включено"
        };
        tell(world, client, Span::Notice, text);
    }
}

/// A held cup (`/obj/item/reagent_containers/cup`) into a cell without one. True when the click
/// was the cell's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<CryoCell>(target).is_none()
        || world
            .get::<crate::combat::CombatMode>(mob)
            .is_some_and(|c| c.0)
    {
        return false;
    }
    let cup = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, CUP_ROOT))
        && world.get::<Reagents>(held).is_some();
    if !cup {
        return false;
    }
    if beaker_of(world, target).is_some() {
        tell(world, client, Span::Warning, "ёмкость уже внутри!");
        return true;
    }
    replace_beaker(world, mob, target, Some(held));
    tell(world, client, Span::Notice, "ёмкость вставлена");
    refresh_view(world, target);
    true
}

/// `process(seconds_per_tick)` every machine tick (2 s).
// Cost: one pass over the cells (a few per map) per machine tick.
fn process_cells(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let cells: Vec<Entity> = world
        .query::<(Entity, &CryoCell)>()
        .iter(world)
        .filter(|(_, c)| c.on)
        .map(|(e, _)| e)
        .collect();
    for _ in from..ticks {
        for &e in &cells {
            cell_tick(world, e, MACHINE_TICK_SECS as f32);
        }
    }
}

fn cell_tick(world: &mut World, e: Entity, secs: f32) {
    let Some(cell) = world.get::<CryoCell>(e).cloned() else {
        return;
    };
    if !cell.on {
        return;
    }
    // `on_set_is_operational`: losing power turns it off.
    if !operational(world, e) {
        set_on(world, e, false);
        return;
    }
    let Some(mob) = occupant(world, e) else {
        set_on(world, e, false);
        return;
    };
    if world
        .get::<crate::fire::FireStacks>(mob)
        .is_some_and(|f| f.on_fire)
    {
        crate::fire::extinguish_mob(world, mob);
    }
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        set_on(world, e, false);
        announce(
            world,
            e,
            "Cryogenics report: Patient is deceased.",
            cell.autoeject,
        );
        if cell.autoeject {
            open(world, e);
        }
        return;
    }
    let healed = world
        .get::<Health>(mob)
        .is_some_and(|h| h.health() >= h.max);
    if healed {
        let wounded = world
            .get::<super::wounds::Limbs>(mob)
            .is_some_and(super::wounds::Limbs::any_wounds);
        let mut treating = cell.treating_wounds;
        if wounded && !treating {
            treating = true;
            announce(
                world,
                e,
                "Cryogenics report: Patient vitals fully recovered, continuing automated wound \
                 treatment.",
                false,
            );
        } else if !wounded {
            treating = false;
        }
        if let Some(mut c) = world.get_mut::<CryoCell>(e) {
            c.treating_wounds = treating;
        }
        if !treating {
            set_on(world, e, false);
            announce(
                world,
                e,
                "Cryogenics report: Patient fully restored.",
                cell.autoeject,
            );
            if cell.autoeject {
                open(world, e);
            }
            return;
        }
    }
    let moles = port_air(world, e).map_or(0.0, |a| a.total_moles());
    let Some(beaker) = beaker_of(world, e) else {
        return;
    };
    if moles <= MIN_GAS_MOLES {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(mut source) = world.get::<Reagents>(beaker).map(|r| r.0.clone()) else {
        return;
    };
    let (efficiency, ..) = parts(world, e);
    let (amount, multiplier) = beaker_dose(efficiency, secs);
    let Some(mut reagents) = world.get_mut::<MobReagents>(mob) else {
        return;
    };
    source.trans_to(
        &chem,
        &mut reagents.blood,
        amount,
        multiplier,
        None,
        false,
        false,
    );
    if let Some(mut r) = world.get_mut::<Reagents>(beaker) {
        r.0 = source;
    }
}

/// `process_atmos` every SSair fire.
// Cost: one pass over the running cells (a few per map) per air tick, O(1) each.
fn process_atmos(world: &mut World) {
    let cells: Vec<(Entity, CryoCell)> = world
        .query::<(Entity, &CryoCell)>()
        .iter(world)
        .filter(|(_, c)| c.on)
        .map(|(e, c)| (e, c.clone()))
        .collect();
    for (e, cell) in cells {
        atmos_tick(world, e, &cell);
    }
}

fn atmos_tick(world: &mut World, e: Entity, cell: &CryoCell) {
    let workable = cell.device.and_then(|id| {
        let net = world.get_resource::<Pipenet>()?;
        let node = net.device(id)?.nodes.first().copied().flatten();
        let air = net.air(id, 0)?;
        (node.is_some() && air.total_moles() >= MIN_GAS_MOLES)
            .then(|| (id, air.temperature(), air.heat_capacity()))
    });
    let Some((id, gas_temperature, gas_heat_capacity)) = workable else {
        set_on(world, e, false);
        announce(
            world,
            e,
            "Cryogenics report: Insufficient cryogenic gas, shutting down.",
            cell.autoeject,
        );
        if cell.autoeject {
            open(world, e);
        }
        return;
    };
    if gas_temperature > BURN_TEMPERATURE {
        let damage = (gas_temperature / 200.0).clamp(10.0, 20.0) as f32;
        let _ = crate::obj_damage::take_damage(world, e, damage, DamageType::Burn);
    }
    if let Some(mob) = occupant(world, e)
        && let Some(body) = world.get::<Respiration>(mob).map(|r| r.bodytemperature)
    {
        // `ishuman`: a body with an inventory to wear clothes in.
        let human = world.get::<Inventory>(mob).is_some();
        let cold_protection = world.get::<Inventory>(mob).map_or(0.0, |inventory| {
            Worn::read(world, inventory).cold_protection(gas_temperature)
        });
        let (_, heat_capacity, conduction) = parts(world, e);
        if let Some(heat) = conducted_heat(
            gas_temperature,
            gas_heat_capacity,
            body,
            cold_protection,
            heat_capacity,
            conduction,
        ) {
            if let Some(mut r) = world.get_mut::<Respiration>(mob) {
                // `adjust_bodytemperature(heat / heat_capacity, TCMB)`.
                if r.bodytemperature >= TCMB {
                    r.bodytemperature = (r.bodytemperature + heat / heat_capacity).max(TCMB);
                }
                // "lets have the core temp match the body temp in humans".
                if human {
                    r.coretemperature = r.bodytemperature;
                }
            }
            if let Some(air) = world.resource_mut::<Pipenet>().air_mut(id, 0) {
                air.gas.temperature =
                    (air.gas.temperature - heat / gas_heat_capacity).clamp(TCMB, MAX_TEMPERATURE);
            }
        }
    }
    world.resource_mut::<Pipenet>().update_parents(id);
}

/// `ui_data`.
fn compute_view(world: &mut World, e: Entity) -> CryoData {
    let Some(cell) = world.get::<CryoCell>(e).cloned() else {
        return CryoData::default();
    };
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let occupant = occupant(world, e).map(|mob| occupant_data(world, mob));
    let beaker = beaker_of(world, e).and_then(|b| {
        let holder = &world.get::<Reagents>(b)?.0;
        let contents = holder
            .reagents
            .iter()
            .map(|r| {
                let name = chem
                    .as_ref()
                    .map_or_else(String::new, |c| c.def(r.id).name.clone());
                (name, (r.volume * 100.0).round() / 100.0)
            })
            .collect();
        Some(CryoBeaker {
            max_volume: holder.maximum_volume,
            current_volume: holder.total_volume,
            contents,
        })
    });
    CryoData {
        is_operating: cell.on,
        is_open: world.get::<Pod>(e).is_some_and(|p| p.open),
        auto_eject: cell.autoeject,
        occupant,
        cell_temperature: port_air(world, e).map_or(TCMB, |a| a.temperature()) as f32,
        beaker,
    }
}

/// `ui_data["occupant"]`.
fn occupant_data(world: &World, mob: Entity) -> CryoOccupant {
    let stat = world.get::<Stat>(mob).copied().unwrap_or_default();
    let knocked_out = crate::status_effects::incapacitation(world, mob).knocked_out;
    let text = match stat {
        Stat::Dead => "Dead",
        Stat::HardCrit => "Unconscious",
        _ if knocked_out => "Unconscious",
        _ => "Stable",
    };
    let health = world.get::<Health>(mob).cloned().unwrap_or_default();
    let body = world
        .get::<Respiration>(mob)
        .map_or(crate::breath::BODYTEMP_NORMAL, |r| r.bodytemperature);
    CryoOccupant {
        name: super::name_of(world, mob),
        stat: text.to_owned(),
        body_temperature: body.round() as f32,
        health: health.health(),
        max_health: health.max,
        brute_loss: health.brute,
        oxy_loss: health.oxy,
        tox_loss: health.tox,
        fire_loss: health.burn,
    }
}

fn refresh_view(world: &mut World, e: Entity) {
    if world.get::<CryoCell>(e).is_none() {
        return;
    }
    let view = compute_view(world, e);
    if let Some(mut c) = world.get_mut::<CryoCell>(e)
        && c.view != view
    {
        c.view = view;
    }
}

/// Cells with an open window get fresh data.
// Cost: per tick, one pass over the open windows.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<&UiWindow, With<UiKind<CryoWindow>>>();
    let mut cells: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
    cells.sort();
    cells.dedup();
    for e in cells {
        refresh_view(world, e);
    }
}

/// `ui_status`: `notcontained_state` and `can_interact` (not from the cell's tile), closed for a
/// broken or unpowered cell.
fn cap_windows(world: &mut World) {
    let windows: Vec<(Entity, Entity, Entity)> = world
        .query_filtered::<(Entity, &UiWindow), With<UiKind<CryoWindow>>>()
        .iter(world)
        .map(|(w, window)| (w, window.actor, window.source))
        .collect();
    for (window, user, e) in windows {
        let inside = world.get::<InCloset>(user).is_some_and(|c| c.closet == e);
        if (!operational(world, e) || inside || same_tile(world, user, e))
            && let Some(mut w) = world.get_mut::<UiWindow>(window)
        {
            w.cap_status(UiStatus::Close);
        }
    }
}

/// The server side of the `Cryo` interface.
pub struct CryoWindow;

impl UiInterface for CryoWindow {
    const NAME: &'static str = CryoUi::NAME;
    type Data = CryoData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryoAct {
    Power,
    Door,
    AutoEject,
    Eject,
}

impl ServerUi for CryoWindow {
    type Source = &'static CryoCell;
    type UserParam = ();
    type Act = CryoAct;

    fn static_data(_: &CryoCell) {}

    fn data(cell: &CryoCell, _: Entity, _: &()) -> CryoData {
        cell.view.clone()
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<CryoAct, UiReject> {
        Ok(match action {
            "power" => CryoAct::Power,
            "door" => CryoAct::Door,
            "autoeject" => CryoAct::AutoEject,
            "eject" => CryoAct::Eject,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<CryoWindow>>>) {
    let pending: Vec<(Entity, Entity, CryoAct)> = acts
        .read(world.resource::<Messages<UiActed<CryoWindow>>>())
        .map(|a| (a.source, a.actor, a.act))
        .collect();
    for (e, actor, act) in pending {
        apply_act(world, e, actor, act);
    }
}

/// `ui_act`, checked at the moment of effect.
pub fn apply_act(world: &mut World, e: Entity, actor: Entity, act: CryoAct) {
    let Some(cell) = world.get::<CryoCell>(e).cloned() else {
        return;
    };
    let open_now = world.get::<Pod>(e).is_some_and(|p| p.open);
    match act {
        CryoAct::Power => {
            if cell.on {
                set_on(world, e, false);
            } else if !open_now {
                set_on(world, e, true);
            }
        }
        CryoAct::Door => {
            if open_now && !same_tile(world, actor, e) {
                close(world, e, None);
            } else {
                open(world, e);
            }
        }
        CryoAct::AutoEject => {
            if let Some(mut c) = world.get_mut::<CryoCell>(e) {
                c.autoeject = !c.autoeject;
            }
        }
        CryoAct::Eject => {
            if beaker_of(world, e).is_some() {
                // `Adjacent(user) && !issilicon(user)`: into the hands, else onto the tile.
                let silicon = crate::silicon::interact::has_silicon_access(world, actor);
                if silicon {
                    if let (Some(beaker), Some(&pos)) =
                        (beaker_of(world, e), world.get::<TilePos>(e))
                    {
                        crate::storage::take_out(world, beaker, pos);
                    }
                    if let Some(mut b) = world.get_mut::<InsertedBeaker>(e) {
                        b.0 = None;
                    }
                } else {
                    replace_beaker(world, actor, e, None);
                }
            }
        }
    }
    refresh_view(world, e);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matter_bins_set_efficiency_and_cooling() {
        // The board's two tier 1 bins, then two tier 4.
        assert_eq!(part_numbers(2), (2.0, 10000.0, 0.6));
        assert_eq!(part_numbers(8), (8.0, 2500.0, 2.4));
        assert_eq!(part_numbers(0), (1.0, 20000.0, 0.3));
    }

    #[test]
    fn cold_gas_draws_heat_from_the_body() {
        // 73 K gas of heat capacity 2000 against a 310 K body, no protection, two tier 1 bins.
        let heat = conducted_heat(73.0, 2000.0, 310.0, 0.0, 10000.0, 0.6).unwrap();
        let energy = (73.0 - 310.0) * (10000.0 * 2000.0 / 12000.0);
        assert!((heat - 0.7 * energy).abs() < 1e-6);
        assert!(heat < 0.0, "the body cools");
        // Full cold protection keeps only the conduction share.
        let covered = conducted_heat(73.0, 2000.0, 310.0, 1.0, 10000.0, 0.6).unwrap();
        assert!((covered - 0.6 * energy).abs() < 1e-6);
        // Within a kelvin nothing moves.
        assert_eq!(
            conducted_heat(300.5, 2000.0, 300.0, 0.0, 10000.0, 0.6),
            None
        );
    }

    #[test]
    fn the_beaker_doses_half_a_unit_a_second() {
        // Efficiency 2: 0.5 / 50 × 2 s drawn, ×50 reaching the blood: 1 u per machine tick.
        let (drawn, multiplier) = beaker_dose(2.0, 2.0);
        assert!((drawn - 0.02).abs() < 1e-6);
        assert!((drawn * multiplier - 1.0).abs() < 1e-5);
    }
}
