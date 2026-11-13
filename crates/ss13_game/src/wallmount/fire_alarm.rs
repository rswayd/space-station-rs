//! Fire alarms (tg `firealarm.dm`; `areas.dm` `set_fire_effect`); the firelocks they drive are
//! [`super::firelock`].
//!
//! A left click pulls the alarm (`alarm()`: powered, the area not already `fire`): every
//! firelock serving the area activates and closes, the first active firelock of an area sets its
//! `fire` (lights red, every fire alarm there flashes red), the pulled alarm rings and draws its
//! active power. A right click resets (`reset()`): the area's firelocks crack open, the area's
//! `fire` clears with its last active firelock, the alarm stops ringing and goes back to idle
//! power. Hot or cold air next to a firelock activates its row the same way
//! ([`super::firelock`]), so the alarms alert without being pulled. The examine shows the
//! hazard light, the fault (location, automatic or manual), the security level and the thermal
//! detection light. An idle alarm shows the security level (`fire_<number>`, the level's light
//! color) and follows its changes (`check_security_level`).
// The `fire_detect` toggle and the stuck trigger/reset come from the wires
// (`crate::machine_wires`).
// shortcut: no alarm manager (consoles, AI), no silicon ctrl-click toggle, no off-station look
// (`fire_offstation`): every alarm counts as on the station. Add with those systems.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::firelock::{self, Firelock};
use super::{Look, light_of, refresh_look};
use crate::looping_sound::{self, SoundLoops, defs::FIREALARM};
use crate::power::{AreaPower, Unpowered};
use crate::tools::tell;

/// tg light colors: `COLOR_WHITE` (detection off), `LIGHT_COLOR_INTENSE_RED` (alarm); the
/// security level's own otherwise.
const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
const INTENSE_RED: [u8; 3] = [0xC8, 0x19, 0x19];

/// `/obj/machinery/firealarm`: its `my_area` and its siren (`soundloop`).
#[derive(Component, Clone, Copy, Debug)]
pub struct FireAlarm {
    pub area: Entity,
    /// Since when it rings (its siren, `soundloop`, runs meanwhile).
    pub ringing: Option<f64>,
    /// `can_trigger`, `can_reset`, `can_toggle_detection`: false while their wire is cut
    /// (`crate::machine_wires`).
    pub can_trigger: bool,
    pub can_reset: bool,
    pub can_toggle_detection: bool,
}

/// An area's fire alarm state (tg `/area` `firedoors`, `active_firelocks`, `firealarms`,
/// `fire_detect`, `fault_status`); its `fire` is [`AreaPower::fire`].
#[derive(Component, Clone, Debug)]
pub struct AreaFire {
    pub firedoors: Vec<Entity>,
    pub active_firelocks: Vec<Entity>,
    pub firealarms: Vec<Entity>,
    pub fire_detect: bool,
    /// `fault_status`, `fault_location`.
    pub fault: Fault,
    pub fault_location: String,
}

/// tg `AREA_FAULT_*`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fault {
    #[default]
    None,
    Automatic,
    Manual,
}

impl Default for AreaFire {
    fn default() -> Self {
        Self {
            firedoors: Vec::new(),
            active_firelocks: Vec::new(),
            firealarms: Vec::new(),
            fire_detect: true,
            fault: Fault::None,
            fault_location: String::new(),
        }
    }
}

pub(super) fn area_fire(world: &mut World, area: Entity) -> Mut<'_, AreaFire> {
    if world.get::<AreaFire>(area).is_none() {
        world.entity_mut(area).insert(AreaFire::default());
    }
    world.get_mut::<AreaFire>(area).expect("inserted above")
}

pub(crate) fn add_alarm(world: &mut World, alarm: Entity, area: Entity) {
    area_fire(world, area).firealarms.push(alarm);
    world.entity_mut(alarm).insert(FireAlarm {
        area,
        ringing: None,
        can_trigger: true,
        can_reset: true,
        can_toggle_detection: true,
    });
}

/// `my_area.fire || LAZYLEN(my_area.active_firelocks)`.
fn alerting(world: &World, area: Entity) -> bool {
    world.get::<AreaPower>(area).is_some_and(|a| a.fire)
        || world
            .get::<AreaFire>(area)
            .is_some_and(|f| !f.active_firelocks.is_empty())
}

/// tg `update_icon_state`, `update_overlays` and `update_appearance` (light) of a secured,
/// unbroken alarm on a station level.
pub(super) fn look(world: &World, e: Entity, alarm: &FireAlarm, powered: bool) -> Look {
    let mut look = Look {
        icon_state: "fire0".to_owned(),
        overlays: Vec::new(),
        light: None,
    };
    if !powered {
        return look;
    }
    let detect = world
        .get::<AreaFire>(alarm.area)
        .is_none_or(|f| f.fire_detect);
    let (overlay, emissive, range, power, color) = if alerting(world, alarm.area) {
        ("fire_alerting", "fire_alerting_e", 2.5, 1.5, INTENSE_RED)
    } else if detect {
        // `fire_[SSsecurity_level.get_current_level_as_number()]`.
        let level = crate::security_level::current(world);
        let overlay = format!("fire_{}", level.number());
        look.overlays = vec![(overlay, false), ("fire_level_e".to_owned(), true)];
        look.light = light_of(world, e, 1.6, 1.0, level.fire_alarm_light_color());
        return look;
    } else {
        ("fire_disabled", "fire_level_e", 1.6, 1.0, WHITE)
    };
    look.overlays = vec![(overlay.to_owned(), false), (emissive.to_owned(), true)];
    look.light = light_of(world, e, range, power, color);
    look
}

use crate::tools::now;

/// tg `firealarm/check_security_level`: every alarm's look follows the new level.
pub fn on_security_level_changed(world: &mut World) {
    let mut query = world.query_filtered::<Entity, With<FireAlarm>>();
    let alarms: Vec<Entity> = query.iter(world).collect();
    for alarm in alarms {
        refresh_look(world, alarm);
    }
}

/// tg `firealarm/alarm(user)`: closes every firelock of the area and rings. `client`: the user
/// who pulled it, told so.
pub fn alarm(world: &mut World, alarm: Entity, client: Option<Entity>) {
    let Some(&FireAlarm {
        area, can_trigger, ..
    }) = world.get::<FireAlarm>(alarm)
    else {
        return;
    };
    // `is_operational`, `can_trigger`, `my_area.fire`.
    if world.get::<Unpowered>(alarm).is_some()
        || !can_trigger
        || world.get::<AreaPower>(area).is_some_and(|a| a.fire)
    {
        return;
    }
    let firelocks = world
        .get::<AreaFire>(area)
        .map(|f| f.firedoors.clone())
        .unwrap_or_default();
    for firelock in firelocks {
        firelock::activate(world, firelock, firelock::AlarmType::Generic);
    }
    if let Some(client) = client {
        tell(world, client, Span::Notice, "triggered alarm!");
    }
    let name = name_of(world, alarm);
    let mut fire = area_fire(world, area);
    fire.fault = Fault::Manual;
    fire.fault_location = name;
    crate::power::update_use_power(world, alarm, true);
    let at = now(world);
    if let Some(mut a) = world.get_mut::<FireAlarm>(alarm) {
        a.ringing = Some(at);
    }
    refresh_look(world, alarm);
}

/// tg `firealarm/reset(user)`: cracks every firelock of the area open and stops ringing.
pub fn reset(world: &mut World, alarm: Entity, client: Option<Entity>) {
    let Some(&FireAlarm {
        area, can_reset, ..
    }) = world.get::<FireAlarm>(alarm)
    else {
        return;
    };
    if world.get::<Unpowered>(alarm).is_some() || !can_reset {
        return;
    }
    let firelocks = world
        .get::<AreaFire>(area)
        .map(|f| f.firedoors.clone())
        .unwrap_or_default();
    for firelock in firelocks {
        firelock::crack_open(world, firelock);
    }
    if let Some(client) = client {
        tell(world, client, Span::Notice, "reset alarm");
    }
    crate::power::update_use_power(world, alarm, false);
    if let Some(mut a) = world.get_mut::<FireAlarm>(alarm) {
        a.ringing = None;
    }
    refresh_look(world, alarm);
}

/// tg `firealarm/toggle_fire_detect(user, silent = TRUE)`: nothing while the detection wire is
/// cut.
pub fn toggle_fire_detect(world: &mut World, alarm: Entity) {
    let Some(&FireAlarm {
        area,
        can_toggle_detection,
        ..
    }) = world.get::<FireAlarm>(alarm)
    else {
        return;
    };
    if !can_toggle_detection {
        return;
    }
    let on = world.get::<AreaFire>(area).is_none_or(|f| f.fire_detect);
    set_fire_detect(world, alarm, !on);
}

/// tg `enable_fire_detect` / `disable_fire_detect`: the area's thermal detection, every alarm
/// of the area relit, and a `reset()` unless the area was pulled by hand.
pub fn set_fire_detect(world: &mut World, alarm: Entity, on: bool) {
    let Some(area) = world.get::<FireAlarm>(alarm).map(|a| a.area) else {
        return;
    };
    let mut fire = area_fire(world, area);
    if fire.fire_detect == on {
        return;
    }
    fire.fire_detect = on;
    let manual = fire.fault == Fault::Manual;
    let alarms = fire.firealarms.clone();
    for a in alarms {
        refresh_look(world, a);
    }
    if !manual {
        reset(world, alarm, None);
    }
}

/// tg `/area/proc/set_fire_effect` and the `COMSIG_AREA_FIRE_CHANGED` handlers: the lights, and
/// every fire alarm's `set_status()`.
pub(super) fn set_fire_effect(
    world: &mut World,
    area: Entity,
    fire: bool,
    fault: Fault,
    source: &str,
) {
    if !crate::power::set_area_fire(world, area, fire) {
        return;
    }
    let mut state = area_fire(world, area);
    state.fault = fault;
    state.fault_location = if fire {
        source.to_owned()
    } else {
        String::new()
    };
    let alarms = world
        .get::<AreaFire>(area)
        .map(|f| f.firealarms.clone())
        .unwrap_or_default();
    for alarm in alarms {
        set_status(world, alarm);
    }
}

/// tg `firealarm/set_status()`: the siren stops when the area calmed down.
fn set_status(world: &mut World, alarm: Entity) {
    let Some(area) = world.get::<FireAlarm>(alarm).map(|a| a.area) else {
        return;
    };
    if !alerting(world, area)
        && let Some(mut a) = world.get_mut::<FireAlarm>(alarm)
    {
        a.ringing = None;
    }
    refresh_look(world, alarm);
}

/// The sirens (`soundloop`, [`FIREALARM`]): a ringing alarm's, and a ringing firelock's while
/// powered (`on_power_loss`). Cost: a pass over the alarms and the ringing or sounding
/// firelocks per tick.
pub(super) fn ring_alarms(world: &mut World) {
    let mut query = world.query::<(Entity, &FireAlarm, Option<&SoundLoops>)>();
    let mut changed: Vec<(Entity, bool)> = query
        .iter(world)
        .filter(|(_, a, l)| a.ringing.is_some() != l.is_some_and(|l| l.is_active(FIREALARM.path)))
        .map(|(e, a, _)| (e, a.ringing.is_some()))
        .collect();
    let mut query = world.query_filtered::<(
        Entity,
        Has<firelock::Ringing>,
        Has<Unpowered>,
        Option<&SoundLoops>,
    ), (
        With<Firelock>,
        Or<(With<firelock::Ringing>, With<SoundLoops>)>,
    )>();
    changed.extend(
        query
            .iter(world)
            .map(|(e, ringing, unpowered, l)| {
                (
                    e,
                    ringing && !unpowered,
                    l.is_some_and(|l| l.is_active(FIREALARM.path)),
                )
            })
            .filter(|(_, want, has)| want != has)
            .map(|(e, want, _)| (e, want)),
    );
    for (e, on) in changed {
        looping_sound::set(world, e, &FIREALARM, on);
    }
}

fn name_of(world: &World, e: Entity) -> String {
    world.get::<ss13_mapload::DmName>(e).map_or_else(
        || "fire alarm".to_owned(),
        |n| crate::vending::plain_name(&n.name).to_owned(),
    )
}

/// tg `firealarm/examine` (on a station level).
pub(super) fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(alarm) = world.get::<FireAlarm>(target) else {
        return;
    };
    let fire = world.get::<AreaFire>(alarm.area);
    let level = format!(
        "The station security alert level is {}.",
        crate::security_level::current(world).name()
    );
    if alerting(world, alarm.area) {
        lines.push((
            Span::Plain,
            "The local area hazard light is flashing.".to_owned(),
        ));
        let (location, kind) =
            fire.map_or(("", Fault::None), |f| (f.fault_location.as_str(), f.fault));
        let kind = if kind == Fault::Automatic {
            "Automatic Detection"
        } else {
            "Manual Trigger"
        };
        lines.push((
            Span::Plain,
            format!("The fault location display is {location} ({kind})."),
        ));
        lines.push((Span::Plain, level));
        lines.push((
            Span::Plain,
            "Left-Click to activate all firelocks in this area.".to_owned(),
        ));
        lines.push((
            Span::Plain,
            "Right-Click to reset firelocks in this area.".to_owned(),
        ));
    } else {
        let detect = fire.is_none_or(|f| f.fire_detect);
        lines.push((Span::Plain, level));
        lines.push((
            Span::Plain,
            format!(
                "The local area thermal detection light is {}.",
                if detect { "lit" } else { "unlit" }
            ),
        ));
        lines.push((
            Span::Plain,
            "Left-Click to activate all firelocks in this area.".to_owned(),
        ));
    }
}
