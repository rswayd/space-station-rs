//! Assembly-triggered bombs (server): the tank transfer valve (tg `transfer_valve.dm`) and the
//! single tank bomb (`/obj/item/tank` `bomb_assemble`, `receive_signal`, `ignite`, `release`
//! in `tanks.dm`).
//!
//! - A transfer valve takes two gas tanks (primary, then secondary port) and one unsecured
//!   assembly (secured as it goes on). Opening the valve (its window, or the assembly's pulse,
//!   at most once per 0.5 s) moves the primary tank's gas into the secondary one, whose volume
//!   becomes both; the mixture then reacts in the tank every `SSobj` tick and the tank
//!   ruptures over `TANK_FRAGMENT_PRESSURE` (`crate::explosions::sources`). Closing it or
//!   taking a tank off splits the gas back by volume.
//! - An assembly holder goes onto a tank that is not in a container (not one of only
//!   igniters); a welder seals the tank ("bomb armed"), a wrench takes the holder back. The
//!   holder's signal beeps, and 1 s later the tank ignites: welded, its plasma and oxygen
//!   explode by temperature and fuel (`ignite`), else its gas is let out.
// shortcut: no wheelchair bombs, backpack straps (cable coil) or deep frying; no valve
// overlays (tank sprites); bombs are logged to the server log only (`log_bomber`,
// `message_admins`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_atmos::GasId;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AlwaysState, OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::server::{
    Assembly, AssemblyHolder, Attached, Device, Later, assembly_attack_self, beep, detach,
    drop_out, holder_attack_self, insert, later, name_of, parts_of, path_of, tell, toggle_secure,
    within_reach,
};
use super::{TransferValveData, TransferValveUi, acts};
use crate::internals::TankAir;
use crate::storage::StoredIn;
use crate::tools::{ToolKind, tool_of};

pub const VALVE: &str = "/obj/item/transfer_valve";
const VALVE_FAKE: &str = "/obj/item/transfer_valve/fake";
const VALVE_SOUND: &str = "sound/effects/valve_opening.ogg";
/// `addtimer(CALLBACK(src, PROC_REF(toggle_off)), 5)`, the tank bomb's 1 s fuse.
const VALVE_RESET: f64 = 0.5;
const IGNITE_DELAY: f64 = 1.0;
/// `T0C`.
const T0C: f64 = 273.15;

/// tg `/obj/item/transfer_valve` vars.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Valve {
    pub tank_one: Option<Entity>,
    pub tank_two: Option<Entity>,
    pub device: Option<Entity>,
    pub open: bool,
    /// `toggle`: a pulse may open it.
    pub toggle: bool,
}

impl Default for Valve {
    fn default() -> Self {
        Self {
            tank_one: None,
            tank_two: None,
            device: None,
            open: false,
            toggle: true,
        }
    }
}

/// What the valve's window shows, kept with the valve (names of the parts).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ValveView(pub TransferValveData);

/// A tank rigged as a bomb (tg `tank_assembly`, `bomb_status`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TankBomb {
    pub holder: Option<Entity>,
    pub welded: bool,
}

fn inside(world: &World, e: Option<Entity>, container: Entity) -> Option<Entity> {
    e.filter(|&e| world.get::<StoredIn>(e).is_some_and(|s| s.0 == container))
}

/// The valve state of `item`, given on first use (`None` for anything else).
pub fn valve(world: &mut World, item: Entity) -> Option<Valve> {
    if let Some(v) = world.get::<Valve>(item) {
        let mut v = v.clone();
        // tg `Exited` forgets what left.
        v.tank_one = inside(world, v.tank_one, item);
        v.tank_two = inside(world, v.tank_two, item);
        v.device = inside(world, v.device, item);
        return Some(v);
    }
    let path = path_of(world, item)?;
    // shortcut: the admin `fake` valve with its own tanks is not ported.
    if !is_under(path, VALVE) || is_under(path, VALVE_FAKE) {
        return None;
    }
    let v = Valve::default();
    world
        .entity_mut(item)
        .insert((v.clone(), ValveView::default(), AlwaysState));
    Some(v)
}

fn set_valve(world: &mut World, item: Entity, v: Valve) {
    let name = |e: Option<Entity>| e.map(|e| name_of(world, e));
    let view = TransferValveData {
        tank_one: name(v.tank_one),
        tank_two: name(v.tank_two),
        attached_device: name(v.device),
        valve: v.open,
    };
    world.entity_mut(item).insert((v, ValveView(view)));
}

/// Use in hand: the valve's window, or a rigged tank's holder (`tank/attack_self`).
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if valve(world, item).is_some() {
        let title = name_of(world, item);
        world.write_message(OpenUi::new::<TransferValveUi>(client, mob, item, title));
        return true;
    }
    if let Some(holder) = world.get::<TankBomb>(item).and_then(|b| b.holder)
        && world.get::<StoredIn>(holder).is_some_and(|s| s.0 == item)
    {
        holder_attack_self(world, client, mob, holder);
        return true;
    }
    false
}

/// Tanks and assemblies on a valve (and the valve on them, `interact_with_atom`); holders,
/// welders and wrenches on tanks. False when the click is neither's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if held == target || !within_reach(world, mob, target) {
        return false;
    }
    if valve(world, target).is_some() {
        if world.get::<TankAir>(held).is_some() {
            attach_tank(world, client, mob, target, held);
            return true;
        }
        if world.get::<Assembly>(held).is_some() {
            attach_device(world, client, mob, target, held);
            return true;
        }
        return false;
    }
    if valve(world, held).is_some() {
        if world.get::<TankAir>(target).is_some() {
            attach_tank(world, client, mob, held, target);
            return true;
        }
        if world
            .get::<Assembly>(target)
            .is_some_and(|a| a.attached == Attached::None)
        {
            attach_device(world, client, mob, held, target);
            return true;
        }
        return false;
    }
    if world.get::<TankAir>(target).is_none() {
        return false;
    }
    if world.get::<AssemblyHolder>(held).is_some() {
        bomb_assemble(world, client, target, held);
        return true;
    }
    match tool_of(world, held).map(|t| t.0) {
        Some(ToolKind::Welder) => weld(world, client, mob, held, target),
        Some(ToolKind::Wrench) => {
            let Some(holder) = world.get::<TankBomb>(target).and_then(|b| b.holder) else {
                return false;
            };
            bomb_disassemble(world, client, mob, target, holder);
            true
        }
        _ => false,
    }
}

/// `try_attach_tank`: the primary port, then the secondary.
fn attach_tank(world: &mut World, client: Entity, mob: Entity, item: Entity, tank: Entity) {
    let Some(mut v) = valve(world, item) else {
        return;
    };
    let name = name_of(world, tank);
    let port = if v.tank_one.is_none() {
        v.tank_one = Some(tank);
        "primary"
    } else if v.tank_two.is_none() {
        v.tank_two = Some(tank);
        "secondary"
    } else {
        tell(
            world,
            client,
            Span::Warning,
            "There are already two tanks attached, remove one first!",
        );
        return;
    };
    insert(world, tank, item);
    tell(
        world,
        client,
        Span::Notice,
        format!("You attach the {name} to the transfer valve's {port} port."),
    );
    // BandaStation: assembling the bomb is logged with both tanks' pressure and temperature.
    if let (Some(one), Some(two)) = (v.tank_one, v.tank_two) {
        let info = |e: Entity| {
            world
                .get::<TankAir>(e)
                .map(|t| format!("P:{}kPa, T:{}K", t.pressure(), t.air.temperature))
                .unwrap_or_default()
        };
        info!(
            "{mob} assembled a ttv bomb {item}: {} ({}) + {} ({})",
            name_of(world, one),
            info(one),
            name_of(world, two),
            info(two)
        );
    }
    set_valve(world, item, v);
}

/// `try_attach_assembly`: an unsecured assembly, secured on the fuse input.
fn attach_device(world: &mut World, client: Entity, mob: Entity, item: Entity, a: Entity) {
    let Some(mut v) = valve(world, item) else {
        return;
    };
    let name = name_of(world, a);
    if world.get::<Assembly>(a).is_some_and(|a| a.secured) {
        tell(
            world,
            client,
            Span::Notice,
            format!("The {name} is secured; unsecure it first."),
        );
        return;
    }
    if v.device.is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "There is already a device attached to the valve, remove it first!",
        );
        return;
    }
    insert(world, a, item);
    if let Some(mut asm) = world.get_mut::<Assembly>(a) {
        asm.attached = Attached::Valve(item);
    }
    toggle_secure(world, a);
    v.device = Some(a);
    tell(
        world,
        client,
        Span::Notice,
        format!("You attach the {name} to the valve controls and secure it."),
    );
    info!("{mob} attached a {name} to a ttv {item}");
    set_valve(world, item, v);
}

/// `/obj/item/transfer_valve/process_activation`.
pub fn process_activation(world: &mut World, item: Entity) {
    let Some(mut v) = valve(world, item) else {
        return;
    };
    if !v.toggle {
        return;
    }
    v.toggle = false;
    set_valve(world, item, v);
    toggle_valve(world, item);
    later(world, item, VALVE_RESET, Later::ValveReady);
}

/// `toggle_off`.
pub fn valve_ready(world: &mut World, item: Entity) {
    if let Some(mut v) = valve(world, item) {
        v.toggle = true;
        set_valve(world, item, v);
    }
}

/// `toggle_valve`: open merges the gases (into the secondary tank), closed splits them.
pub fn toggle_valve(world: &mut World, item: Entity) {
    let Some(mut v) = valve(world, item) else {
        return;
    };
    if let Some(pos) = crate::explosions::turf_of(world, item) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, VALVE_SOUND, 50.0, false),
        );
    }
    let (Some(one), Some(two)) = (v.tank_one, v.tank_two) else {
        return;
    };
    if v.open {
        split_gases(world, one, two);
        v.open = false;
    } else {
        let device = v.device.map(|d| name_of(world, d));
        info!(
            "bomb valve opened at {:?} with {device:?} attached",
            crate::explosions::turf_of(world, item)
        );
        merge_gases(world, one, two);
        v.open = true;
    }
    set_valve(world, item, v);
}

/// `merge_gases(tank_two)`: everything of `one` into `two`, the volumes added.
fn merge_gases(world: &mut World, one: Entity, two: Entity) {
    let Some(mut from) = world.get::<TankAir>(one).cloned() else {
        return;
    };
    let moved = from.air.remove_ratio(1.0);
    let volume = from.volume;
    if let Some(mut to) = world.get_mut::<TankAir>(two) {
        to.volume += volume;
        to.air.merge(&moved);
    }
    if let Some(mut t) = world.get_mut::<TankAir>(one) {
        t.air = from.air;
    }
}

/// `split_gases`: `one`'s share of the volume back to it.
fn split_gases(world: &mut World, one: Entity, two: Entity) {
    let (Some(v1), Some(v2)) = (
        world.get::<TankAir>(one).map(|t| t.volume),
        world.get::<TankAir>(two).map(|t| t.volume),
    ) else {
        return;
    };
    if v2 <= 0.0 {
        return;
    }
    let ratio = v1 / v2;
    let Some(back) = world.get_mut::<TankAir>(two).map(|mut t| {
        let back = t.air.remove_ratio(ratio);
        t.volume -= v1;
        back
    }) else {
        return;
    };
    if let Some(mut t) = world.get_mut::<TankAir>(one) {
        t.air.merge(&back);
    }
}

/// A tank off its port (`tankone`/`tanktwo` acts): the gas split, the valve closed, the tank
/// out onto the floor.
fn remove_tank(world: &mut World, item: Entity, first: bool) {
    let Some(mut v) = valve(world, item) else {
        return;
    };
    let Some(tank) = (if first { v.tank_one } else { v.tank_two }) else {
        return;
    };
    if v.open
        && let (Some(one), Some(two)) = (v.tank_one, v.tank_two)
    {
        split_gases(world, one, two);
    }
    v.open = false;
    if first {
        v.tank_one = None;
    } else {
        v.tank_two = None;
    }
    drop_out(world, tank, item);
    set_valve(world, item, v);
}

impl ServerUi for TransferValveUi {
    type Source = &'static ValveView;
    type UserParam = ();
    type Act = ValveAct;

    fn static_data(_: &ValveView) {}

    fn data(view: &ValveView, _: Entity, _: &()) -> TransferValveData {
        view.0.clone()
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<ValveAct, UiReject> {
        Ok(match action {
            acts::TANK_ONE => ValveAct::TankOne,
            acts::TANK_TWO => ValveAct::TankTwo,
            acts::TOGGLE => ValveAct::Toggle,
            acts::DEVICE => ValveAct::Device,
            acts::REMOVE_DEVICE => ValveAct::RemoveDevice,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValveAct {
    TankOne,
    TankTwo,
    Toggle,
    Device,
    RemoveDevice,
}

/// `/obj/item/transfer_valve/ui_act` (the window is open only while the valve is in the
/// user's hands, `crate::assemblies::server`).
pub fn valve_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<TransferValveUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, ValveAct)> = cursor
        .read(world.resource::<Messages<UiActed<TransferValveUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, item, act) in pending {
        let Some(v) = valve(world, item) else {
            continue;
        };
        match act {
            ValveAct::TankOne => remove_tank(world, item, true),
            ValveAct::TankTwo => remove_tank(world, item, false),
            ValveAct::Toggle => toggle_valve(world, item),
            ValveAct::Device => {
                if let Some(d) = v.device {
                    assembly_attack_self(world, client, actor, d);
                }
            }
            ValveAct::RemoveDevice => {
                if let Some(d) = v.device {
                    detach(world, d, item);
                    let mut v = v;
                    v.device = None;
                    set_valve(world, item, v);
                }
            }
        }
    }
}

// --- Single tank bombs --------------------------------------------------------------------

/// `/obj/item/tank/item_interaction` -> `bomb_assemble`.
fn bomb_assemble(world: &mut World, client: Entity, tank: Entity, holder: Entity) {
    if world
        .get::<TankBomb>(tank)
        .is_some_and(|b| b.holder.is_some())
    {
        tell(world, client, Span::Warning, "something already attached!");
        return;
    }
    let parts = parts_of(world, holder);
    let igniters = parts
        .iter()
        .filter(|&&p| {
            matches!(
                world.get::<Assembly>(p).map(|a| &a.device),
                Some(Device::Igniter { .. })
            )
        })
        .count();
    // Either part must be an igniter, but not every part.
    if igniters == 0 || igniters == parts.len() {
        return;
    }
    if world.get::<StoredIn>(tank).is_some() {
        tell(world, client, Span::Warning, "can't reach!");
        return;
    }
    insert(world, holder, tank);
    if let Some(mut h) = world.get_mut::<AssemblyHolder>(holder) {
        h.master = Some(tank);
    }
    let welded = world.get::<TankBomb>(tank).is_some_and(|b| b.welded);
    world.entity_mut(tank).insert(TankBomb {
        holder: Some(holder),
        welded,
    });
    tell(world, client, Span::Notice, "bomb assembled");
}

/// `bomb_disassemble`: the holder to the user's hands, the tank unwelded.
fn bomb_disassemble(world: &mut World, client: Entity, mob: Entity, tank: Entity, holder: Entity) {
    world.entity_mut(tank).insert(TankBomb::default());
    if let Some(mut h) = world.get_mut::<AssemblyHolder>(holder) {
        h.master = None;
    }
    super::server::to_hands(world, mob, holder);
    tell(world, client, Span::Notice, "bomb disarmed");
}

/// `/obj/item/tank/welder_act`: the tank is sealed for a bomb.
fn weld(world: &mut World, client: Entity, mob: Entity, welder: Entity, tank: Entity) -> bool {
    let bomb = world.get::<TankBomb>(tank).copied().unwrap_or_default();
    if bomb.welded {
        tell(world, client, Span::Warning, "already welded!");
        return true;
    }
    if !crate::tools::tool_usable(world, welder, 0, 0.0) {
        let text = crate::tools::unusable_message(world, welder, 0);
        tell(world, client, Span::Warning, text);
        return true;
    }
    world.entity_mut(tank).insert(TankBomb {
        welded: true,
        ..bomb
    });
    if let Some(pos) = crate::explosions::turf_of(world, tank) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/items/tools/welder.ogg", 40.0, true),
        );
    }
    let state = world
        .get::<TankAir>(tank)
        .map(|t| (t.air.temperature, t.pressure()));
    info!("{mob} welded a single tank bomb {tank}: {state:?}");
    tell(world, client, Span::Notice, "bomb armed");
    true
}

/// `/obj/item/tank/receive_signal`: a beep, then `ignite` in 1 s.
pub fn tank_receive_signal(world: &mut World, tank: Entity) {
    if world.get::<TankAir>(tank).is_none() {
        return;
    }
    beep(world, tank, crate::health::DEFAULT_MESSAGE_RANGE);
    later(world, tank, IGNITE_DELAY, Later::Ignite);
}

/// The explosion of a welded tank bomb (`ignite`) by its mixture's temperature (K) and fuel
/// (`plasma + oxygen / 6` moles): `(devastation, heavy, light, flash)`; `None`: it only burns.
pub fn ignite_blast(temperature: f64, fuel: f64) -> Option<(i32, i32, i32, i32)> {
    // DM `round(x, 1)`: to the nearest.
    let r = |x: f64| (x + 0.5).floor() as i32;
    if temperature > T0C + 400.0 {
        let s = fuel / 15.0;
        if s >= 2.0 {
            Some((r(s), r(s * 2.0), r(s * 3.0), r(s * 4.0)))
        } else if s >= 1.0 {
            Some((r(s), r(s * 2.0), r(s * 2.0), r(s * 3.0)))
        } else if s >= 0.5 {
            Some((0, 1, 2, 4))
        } else if s >= 0.2 {
            Some((0, 0, 1, 2))
        } else {
            None
        }
    } else if temperature > T0C + 250.0 {
        let s = fuel / 20.0;
        if s >= 1.0 {
            Some((0, r(s), r(s * 2.0), r(s * 3.0)))
        } else if s >= 0.5 {
            Some((0, 0, 1, 2))
        } else {
            None
        }
    } else if temperature > T0C + 100.0 {
        let s = fuel / 25.0;
        (s >= 1.0).then(|| (0, 0, r(s), r(s * 3.0)))
    } else {
        None
    }
}

/// `/obj/item/tank/ignite`: welded, it explodes (or burns its gas out over the tile); not
/// welded, it lets its gas out (`release`).
pub fn ignite(world: &mut World, tank: Entity) {
    let Some(air) = world.get::<TankAir>(tank).map(|t| t.air.clone()) else {
        return;
    };
    let Some(pos) = crate::explosions::turf_of(world, tank) else {
        return;
    };
    let bomb = world.get::<TankBomb>(tank).copied().unwrap_or_default();
    if !bomb.welded {
        release(world, tank, pos);
        return;
    }
    let igniter_heat = bomb
        .holder
        .map(|h| parts_of(world, h))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| match world.get::<Assembly>(p).map(|a| &a.device) {
            Some(Device::Igniter { condenser }) => Some(if *condenser { 50.0 } else { 1000.0 }),
            _ => None,
        })
        .fold(0.0_f64, f64::max);
    if igniter_heat <= 0.0 {
        return;
    }
    let fuel = air.get(GasId::PLASMA) + air.get(GasId::O2) / 6.0;
    let name = name_of(world, tank);
    match ignite_blast(air.temperature, fuel) {
        Some((dev, heavy, light, flash)) => {
            info!("a tank bomb exploded at {pos:?}: {dev}/{heavy}/{light}");
            crate::explosions::explosion(
                world,
                pos,
                crate::explosions::Explosion::new(dev, heavy, light)
                    .flash(flash)
                    .cause(&name),
            );
        }
        None => burn_out(world, pos, &air, igniter_heat),
    }
    if world.get_entity(tank).is_ok() {
        world.despawn(tank);
    }
}

/// `ground_zero.assume_air(bomb_mixture)` and `hotspot_expose(igniter_temperature, 125)`.
fn burn_out(world: &mut World, pos: TilePos, air: &ss13_atmos::GasMixture, heat: f64) {
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        atmos.merge_air(pos, air);
        atmos.hotspot_expose(pos, heat, 125.0, false);
    }
}

/// `/obj/item/tank/release`: all its gas onto the tile.
fn release(world: &mut World, tank: Entity, pos: TilePos) {
    let Some(gas) = world
        .get_mut::<TankAir>(tank)
        .map(|mut t| t.air.remove_ratio(1.0))
    else {
        return;
    };
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        atmos.merge_air(pos, &gas);
    }
}

/// `transfer_valve/examine` and `tank/examine`'s rigged line.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(v) = world.get::<Valve>(target) {
        let one = inside(world, v.tank_one, target);
        let two = inside(world, v.tank_two, target);
        let device = inside(world, v.device, target);
        if let Some(t) = one {
            let name = name_of(world, t);
            lines.push((
                Span::Notice,
                format!("A {name} is attached to the primary port."),
            ));
        }
        if let Some(t) = two {
            let name = name_of(world, t);
            lines.push((
                Span::Notice,
                format!("A {name} is attached to the secondary port."),
            ));
        }
        if one.is_none() || two.is_none() {
            lines.push((
                Span::Notice,
                "You could attach a gas tank onto the open port.".to_owned(),
            ));
        }
        if let Some(d) = device {
            let name = name_of(world, d);
            lines.push((
                Span::Notice,
                format!("A {name} is attached to the fuse input."),
            ));
        }
        return;
    }
    if world
        .get::<TankBomb>(target)
        .is_some_and(|b| b.holder.is_some())
    {
        lines.push((
            Span::Warning,
            "There is some kind of device rigged to the tank!".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tank_bombs_scale_like_tg() {
        // Hot and fuelled: strength 3 -> 3, 6, 9, flash 12.
        assert_eq!(ignite_blast(T0C + 500.0, 45.0), Some((3, 6, 9, 12)));
        // Strength 1.5: the light range is doubled, not tripled.
        assert_eq!(ignite_blast(T0C + 500.0, 22.5), Some((2, 3, 3, 5)));
        assert_eq!(ignite_blast(T0C + 500.0, 9.0), Some((0, 1, 2, 4)));
        assert_eq!(ignite_blast(T0C + 500.0, 1.0), None);
        assert_eq!(ignite_blast(T0C + 300.0, 40.0), Some((0, 2, 4, 6)));
        assert_eq!(ignite_blast(T0C + 150.0, 50.0), Some((0, 0, 2, 6)));
        assert_eq!(ignite_blast(T0C + 150.0, 10.0), None);
        assert_eq!(ignite_blast(T0C, 1000.0), None);
    }
}
