//! The cryptographic sequencer (server), after tg `/obj/item/card/emag` and its doorjack
//! subtype (`code/game/objects/items/emags.dm`) and the `emag_act` overrides of the families the
//! port has.
//!
//! - A left click with an emag in the active hand ([`interact_with_atom`], from
//!   `crate::inventory` before the targets' own item handling) on a family that takes emags
//!   checks `can_emag` ([`can_emag`]): the plain emag refuses airlocks, windoors and firelocks
//!   (tram airlocks excepted); the doorjack takes only those, using one of its 3 charges, each
//!   back 3 minutes after its use. Then the target's `emag_act` ([`emag_act`]):
//!   - an airlock (closed, powered, not moving) sparks for 0.6 s, then opens if it can and stays
//!     bolted with its lights off, both powers lost, for good unoperable (`crate::doors`);
//!   - a windoor (closed, not moving) opens 0.6 s later and stays open;
//!   - a firelock opens and stops following alarms (`crate::wallmount::firelock`);
//!   - a secure closet or crate: its lock breaks open (`broken`, no locking any more), an
//!     account lock (`crate::closets::AccountLock`) is bypassed;
//!   - a lockbox: its lock breaks open (`crate::lockbox`);
//!   - an APC (panel shut): its interface unlocks and cannot be locked again;
//!   - an air alarm: the flood and vent siphon modes are offered;
//!   - a supply console: contraband is offered (`crate::cargo::console`);
//!   - a communications console, a vending machine, a chem dispenser, a cyborg
//!     (`crate::silicon::chassis`), a modular computer (`crate::modpc::pda`): their own
//!     `emag_act`.
//! - Bots take emags through their own clicks (`crate::bots`), after [`can_emag`].
// shortcut: no bluespace emag reach (`prox_check`), no emags stuck together, no Donk Co. fake
// emag, no `emag_act` of pAIs, the holodeck or other
// unported families; no hacked-APC flicker; no blackbox or combat log of the attempt.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName, DoorKind};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};

use crate::atmos_machinery::AirAlarm;
use crate::closets::Closet;
use crate::doors::{Door, DoorState, with_door};
use crate::power::Apc;
use crate::sound::{PlaySound, SHORT_RANGE_SOUND_EXTRARANGE, playsound};
use crate::tools::Actor;

/// `/obj/item/card/emag` and its subtypes.
pub const EMAG: &str = "/obj/item/card/emag";
/// `/obj/item/card/emag/doorjack`.
pub const DOORJACK: &str = "/obj/item/card/emag/doorjack";
/// `typesof(/obj/machinery/door/airlock/tram)`: left out of the emag's blacklist.
const TRAM_AIRLOCK: &str = "/obj/machinery/door/airlock/tram";
/// Doorjack `max_charges`, `charge_time` (1800 deciseconds).
pub const DOORJACK_CHARGES: usize = 3;
pub const DOORJACK_CHARGE_SECS: f64 = 180.0;
/// The `addtimer(finish_emag_act, 0.6 SECONDS)` of airlocks and windoors.
pub const FINISH_SECS: f64 = 0.6;
const SPARKS: &str = "sparks";
const TWOBEEP: &str = "sound/machines/beep/twobeep.ogg";
/// `SILENCED_SOUND_EXTRARANGE`.
const SILENCED_SOUND_EXTRARANGE: f32 = -11.0;

/// A doorjack's used charges: each comes back at its time (tg `charge_timers`). Absent: all
/// charges there.
#[derive(Component, Clone, Debug, Default)]
pub struct DoorjackCharges {
    pub recharge_at: Vec<f64>,
}

impl DoorjackCharges {
    pub fn charges(&self) -> usize {
        DOORJACK_CHARGES.saturating_sub(self.recharge_at.len())
    }
}

/// An airlock or windoor between `emag_act` and `finish_emag_act`.
#[derive(Component, Clone, Copy, Debug)]
pub struct EmagFinish {
    pub at: f64,
}

pub struct EmagPlugin;

impl Plugin for EmagPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `/obj/item/card/emag/interact_with_atom` on the families with an `emag_act`.
        app.add_click_handler(ClickPhase::HeldOnTarget, 300, |w, c| {
            !c.modifiers.right
                && c.held.is_some_and(|h| {
                    crate::emag::interact_with_atom(
                        w,
                        crate::tools::Actor {
                            client: c.client,
                            mob: c.mob,
                            item: Some(h),
                        },
                        c.target,
                    )
                })
        });
        app.add_systems(
            Update,
            (
                finish_emags.run_if(any_with_component::<EmagFinish>),
                recharge_doorjacks.run_if(any_with_component::<DoorjackCharges>),
            ),
        );
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn item_path(world: &World, item: Entity) -> Option<&str> {
    world.get::<DmItem>(item).map(|i| i.path.as_str())
}

/// The item is an emag (any `/obj/item/card/emag`).
pub fn is_emag(world: &World, item: Entity) -> bool {
    item_path(world, item).is_some_and(|p| is_under(p, EMAG))
}

fn is_doorjack(world: &World, item: Entity) -> bool {
    item_path(world, item).is_some_and(|p| is_under(p, DOORJACK))
}

/// The doors only a doorjack opens (the emag's `type_blacklist`, the doorjack's
/// `type_whitelist`).
fn jack_door(world: &World, target: Entity) -> bool {
    let kind = world.get::<Door>(target).map(|d| d.kind);
    matches!(
        kind,
        Some(DoorKind::Airlock | DoorKind::Windoor | DoorKind::Firedoor)
    ) && !crate::construction::path_of(world, target).is_some_and(|p| is_under(&p, TRAM_AIRLOCK))
}

/// The families with an `emag_act` here.
fn takes_emag(world: &World, target: Entity) -> bool {
    let Ok(e) = world.get_entity(target) else {
        return false;
    };
    e.get::<Door>().is_some_and(|d| {
        matches!(
            d.kind,
            DoorKind::Airlock | DoorKind::Windoor | DoorKind::Firedoor
        )
    }) || e.contains::<Closet>()
        || e.contains::<crate::lockbox::Lockbox>()
        || e.contains::<Apc>()
        || e.contains::<AirAlarm>()
        || e.contains::<crate::cargo::console::CargoConsole>()
        || e.contains::<crate::comms_server::CommsConsole>()
        || e.contains::<crate::vending::Vendor>()
        || e.contains::<crate::chem_dispenser::ChemDispenser>()
        || e.contains::<crate::silicon::chassis::Chassis>()
        || e.contains::<crate::modpc::pda::ModularComputer>()
}

fn name(world: &World, e: Entity) -> String {
    if let Some(name) = world.get::<Name>(e) {
        return name.as_str().to_owned();
    }
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

fn sparks(world: &mut World, target: Entity, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(target) {
        let sound = PlaySound::new(pos, SPARKS, vol, true).extrarange(SHORT_RANGE_SOUND_EXTRARANGE);
        playsound(world, sound);
    }
}

/// tg `visible_message` of `target` to everyone around it.
fn visible(world: &mut World, target: Entity, span: Span, text: String) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    for message in crate::pull::visible_message(world, pos, ChatChannel::Local, (span, text), &[]) {
        world.write_message(message);
    }
}

/// tg `/obj/item/card/emag/interact_with_atom` on a family that takes emags. True when the
/// click was the emag's (used or refused); false lets the click go on.
pub fn interact_with_atom(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(card) = actor.item.filter(|&i| is_emag(world, i)) else {
        return false;
    };
    if !takes_emag(world, target) {
        return false;
    }
    if can_emag(world, actor.mob, card, target) {
        emag_act(world, actor, card, target);
    }
    true
}

/// tg `can_emag`: the doorjack needs a charge and a door of its whitelist; the plain emag
/// refuses those doors. Tells `mob` why not.
pub fn can_emag(world: &mut World, mob: Entity, card: Entity, target: Entity) -> bool {
    let door = jack_door(world, target);
    if is_doorjack(world, card) {
        let charges = world
            .get::<DoorjackCharges>(card)
            .map_or(DOORJACK_CHARGES, DoorjackCharges::charges);
        let card_name = name(world, card);
        if charges == 0 {
            tell_mob(
                world,
                mob,
                Span::Warning,
                format!("{card_name} is recharging!"),
            );
            return false;
        }
        if !door {
            tell_mob(
                world,
                mob,
                Span::Warning,
                format!(
                    "{card_name} is unable to interface with this. It only seems to fit into \
                     airlock electronics."
                ),
            );
        }
        return door;
    }
    if door {
        let text = format!(
            "The {} cannot be affected by the {}! A more specialized hacking device is required.",
            name(world, target),
            name(world, card)
        );
        tell_mob(world, mob, Span::Warning, text);
        return false;
    }
    true
}

/// tg `target.emag_act(user, card)` of the ported families. True when it took.
pub fn emag_act(world: &mut World, actor: Actor, card: Entity, target: Entity) -> bool {
    let (client, mob) = (actor.client, actor.mob);
    if let Some(kind) = world.get::<Door>(target).map(|d| d.kind) {
        return match kind {
            DoorKind::Airlock => airlock(world, mob, card, target),
            DoorKind::Windoor => windoor(world, target),
            DoorKind::Firedoor => firelock(world, mob, card, target),
            _ => false,
        };
    }
    if world.get::<Closet>(target).is_some() {
        return closet(world, mob, target);
    }
    if world.get::<crate::lockbox::Lockbox>(target).is_some() {
        return crate::lockbox::emag_act(world, client, target);
    }
    if world.get::<Apc>(target).is_some() {
        return apc(world, mob, target);
    }
    if world.get::<AirAlarm>(target).is_some() {
        return air_alarm(world, mob, target);
    }
    if world
        .get::<crate::cargo::console::CargoConsole>(target)
        .is_some()
    {
        return crate::cargo::console::emag_act(world, mob, card, target);
    }
    if world
        .get::<crate::comms_server::CommsConsole>(target)
        .is_some()
    {
        return crate::comms_server::emag_act(world, client, target);
    }
    if world.get::<crate::vending::Vendor>(target).is_some() {
        return crate::vending::emag_act(world, client, target);
    }
    if world
        .get::<crate::chem_dispenser::ChemDispenser>(target)
        .is_some()
    {
        crate::chem_dispenser::emag_act(world, client, target);
        return true;
    }
    if world
        .get::<crate::silicon::chassis::Chassis>(target)
        .is_some()
    {
        return crate::silicon::chassis::emag_act(world, actor, target);
    }
    if world
        .get::<crate::modpc::pda::ModularComputer>(target)
        .is_some()
    {
        return crate::modpc::pda::emag_act(world, client, target);
    }
    false
}

/// The doorjack's `use_charge`; nothing for other emags.
fn use_charge(world: &mut World, mob: Entity, card: Entity) {
    if !is_doorjack(world, card) {
        return;
    }
    let at = now(world) + DOORJACK_CHARGE_SECS;
    if world.get::<DoorjackCharges>(card).is_none() {
        world.entity_mut(card).insert(DoorjackCharges::default());
    }
    let charges = {
        let mut used = world
            .get_mut::<DoorjackCharges>(card)
            .expect("inserted above");
        used.recharge_at.push(at);
        used.charges()
    };
    let text = format!(
        "You use {}. It now has {charges} charge{} remaining.",
        name(world, card),
        if charges == 1 { "" } else { "s" }
    );
    tell_mob(world, mob, Span::Notice, text);
}

/// `/obj/machinery/door/airlock/emag_act`: closed, powered, not moving, not emagged yet.
fn airlock(world: &mut World, mob: Entity, card: Entity, target: Entity) -> bool {
    let started = with_door(world, target, |door, appearance, ctx| {
        let ok = !door.is_operating()
            && door.state == DoorState::Closed
            && door.has_power(ctx.env.powered)
            && !door.emagged;
        if ok {
            // `set_machine_stat(MAINT)`, `operating = TRUE`: the sparks, no lights.
            door.emagging = true;
            door.apply_look(appearance);
        }
        (ok, false)
    });
    if started != Some(true) {
        return false;
    }
    use_charge(world, mob, card);
    let at = now(world) + FINISH_SECS;
    world.entity_mut(target).insert(EmagFinish { at });
    true
}

/// `/obj/machinery/door/window/emag_act`: closed, not moving, not emagged yet.
fn windoor(world: &mut World, target: Entity) -> bool {
    let started = with_door(world, target, |door, _, _| {
        let ok = !door.is_operating() && door.state == DoorState::Closed && !door.emagged;
        if ok {
            door.emagged = true;
            door.emagging = true;
        }
        (ok, false)
    });
    if started != Some(true) {
        return false;
    }
    // `flick("[base_state]spark", src)`.
    if let Some(base) = world
        .get::<ss13_mapload::DmDoor>(target)
        .map(|d| d.base_icon_state.clone())
    {
        crate::animation::flick(world, target, &format!("{base}spark"));
    }
    sparks(world, target, 75.0);
    let at = now(world) + FINISH_SECS;
    world.entity_mut(target).insert(EmagFinish { at });
    true
}

/// `/obj/machinery/door/firedoor/emag_act`: opens, alarms no longer move it.
fn firelock(world: &mut World, mob: Entity, card: Entity, target: Entity) -> bool {
    if world.get::<Door>(target).is_none_or(|d| d.emagged) {
        return false;
    }
    use_charge(world, mob, card);
    with_door(world, target, |door, appearance, ctx| {
        door.emagged = true;
        let moved = door.state == DoorState::Closed
            && !door.is_operating()
            && ctx.open(door, appearance, false);
        ((), moved)
    });
    crate::wallmount::firelock::refresh_firelock(world, target);
    true
}

/// `finish_emag_act` of airlocks and windoors whose 0.6 s are over.
fn finish_emags(world: &mut World) {
    let t = now(world);
    let mut query = world.query::<(Entity, &EmagFinish)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, f)| t >= f.at)
        .map(|(e, _)| e)
        .collect();
    for e in due {
        world.entity_mut(e).remove::<EmagFinish>();
        match world.get::<Door>(e).map(|d| d.kind) {
            Some(DoorKind::Airlock) => finish_airlock(world, e),
            Some(DoorKind::Windoor) => {
                // `open(BYPASS_DOOR_CHECKS)`.
                with_door(world, e, |door, appearance, ctx| {
                    door.emagging = false;
                    door.apply_look(appearance);
                    ((), ctx.open_bypass(door, appearance))
                });
            }
            _ => {}
        }
    }
}

/// `/obj/machinery/door/airlock/finish_emag_act`: open if it can (before the flag, so with the
/// usual checks), then emagged, bolted, its lights off and both powers lost.
fn finish_airlock(world: &mut World, e: Entity) {
    with_door(world, e, |door, appearance, ctx| {
        door.emagging = false;
        let moved = ctx.open(door, appearance, false);
        door.emagged = true;
        door.feedback = false;
        door.locked = true;
        door.apply_look(appearance);
        ((), moved)
    });
    crate::wires::lose_power(world, e, true);
    crate::wires::lose_power(world, e, false);
}

/// The doorjack's `recharge`: each due charge comes back with a quiet beep.
fn recharge_doorjacks(world: &mut World) {
    let t = now(world);
    let mut query = world.query::<(Entity, &mut DoorjackCharges)>();
    let mut beeps = Vec::new();
    let mut done = Vec::new();
    for (e, mut used) in query.iter_mut(world) {
        let before = used.recharge_at.len();
        used.recharge_at.retain(|&at| at > t);
        if used.recharge_at.len() < before {
            beeps.push(e);
        }
        if used.recharge_at.is_empty() {
            done.push(e);
        }
    }
    for e in beeps {
        if let Some(pos) = crate::modpc::pda::turf_of(world, e) {
            let sound = PlaySound::new(pos, TWOBEEP, 10.0, true)
                .extrarange(SILENCED_SOUND_EXTRARANGE)
                .falloff_distance(0.0);
            playsound(world, sound);
        }
    }
    for e in done {
        world.entity_mut(e).remove::<DoorjackCharges>();
    }
}

/// `/obj/structure/closet/emag_act`: a secure closet's lock breaks open.
/// An account lock (`locked_to_account/on_emag`) is bypassed first.
fn closet(world: &mut World, mob: Entity, target: Entity) -> bool {
    let account_lock = world
        .entity_mut(target)
        .take::<crate::closets::AccountLock>()
        .is_some();
    if account_lock {
        tell_mob(world, mob, Span::Notice, "account lock bypassed");
    }
    {
        let Some(mut c) = world.get_mut::<Closet>(target) else {
            return account_lock;
        };
        if !c.secure || c.broken {
            return account_lock;
        }
        c.broken = true;
        c.locked = false;
    }
    crate::closets::refresh_look(world, target);
    visible(
        world,
        target,
        Span::Warning,
        "Искры вылетают из панели замка!".to_owned(),
    );
    tell_mob(world, mob, Span::Notice, "замок взломан!");
    sparks(world, target, 50.0);
    true
}

/// `/obj/machinery/power/apc/emag_act`: with the panel shut, the interface unlocks for good.
// shortcut: the cover is never open (not ported), no `malfhack`, `BROKEN` or `MAINT` APCs.
fn apc(world: &mut World, mob: Entity, target: Entity) -> bool {
    if world.get::<Apc>(target).is_none_or(|a| a.emagged) {
        return false;
    }
    if world
        .get::<crate::construction::PanelOpen>(target)
        .is_some_and(|p| p.0)
    {
        tell_mob(world, mob, Span::Warning, "сначала закройте панель!");
        return false;
    }
    crate::animation::flick(world, target, "apc-spark");
    sparks(world, target, 75.0);
    if let Some(mut a) = world.get_mut::<Apc>(target) {
        a.emagged = true;
        a.locked = false;
    }
    tell_mob(world, mob, Span::Notice, "интерфейс повреждён");
    true
}

/// `/obj/machinery/airalarm/emag_act`: the authentication sensors scrambled (the emag modes).
fn air_alarm(world: &mut World, mob: Entity, target: Entity) -> bool {
    {
        let Some(mut a) = world.get_mut::<AirAlarm>(target) else {
            return false;
        };
        if a.emagged {
            return false;
        }
        a.emagged = true;
    }
    let text = format!("Sparks fly out of {}!", name(world, target));
    visible(world, target, Span::Warning, text);
    tell_mob(world, mob, Span::Notice, "authentication sensors scrambled");
    sparks(world, target, 50.0);
    true
}

/// The examine lines of an emagged door and a doorjack's charges.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    if world
        .get::<Door>(target)
        .is_some_and(|d| d.emagged && matches!(d.kind, DoorKind::Airlock | DoorKind::Windoor))
    {
        lines.push((
            Span::Warning,
            "Its access panel is smoking slightly.".to_owned(),
        ));
    }
    if !is_doorjack(world, target) {
        return;
    }
    let used = world.get::<DoorjackCharges>(target);
    let charges = used.map_or(DOORJACK_CHARGES, DoorjackCharges::charges);
    lines.push((Span::Notice, format!("It has {charges} charges remaining.")));
    let Some(used) = used else {
        return;
    };
    lines.push((
        Span::Notice,
        "A small display on the back reads:".to_owned(),
    ));
    let t = now(world);
    for (i, at) in used.recharge_at.iter().enumerate() {
        let left = (at - t).max(0.0);
        lines.push((
            Span::Notice,
            format!("CHARGE #{}: {} s", i + 1, left.ceil() as u64),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doorjack_charges_count_down_from_three() {
        let mut used = DoorjackCharges::default();
        assert_eq!(used.charges(), 3);
        used.recharge_at.extend([1.0, 2.0, 3.0]);
        assert_eq!(used.charges(), 0);
        used.recharge_at.push(4.0);
        assert_eq!(used.charges(), 0, "never below zero");
    }
}
