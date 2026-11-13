//! Weather warnings (server), after tg `/datum/component/weather_announcer`
//! (`code/datums/components/weatherannouncer.dm`) on the mining weather radio
//! (`/obj/item/radio/weather_monitor`) and the wall barometric monitor
//! (`/obj/machinery/mining_weather_monitor`).
//!
//! Every second ([`PROCESS_SECS`]) an enabled announcer reads the mining level's weather
//! ([`crate::weather`]): a barometer weather (`WEATHER_BAROMETER`) in its telegraph or main stage
//! is imminent or active, else the time until the level is eligible for its next weather
//! decides: more than 2 minutes is clear, more than 30 s incoming, less imminent. Imminent
//! weather that acts on mobs (`FUNCTIONAL_WEATHER`) is dangerous. On a change the announcer says
//! the warning (with a powered announcement system, its weather line, also on the supply
//! channel), shows the level's overlay and recolours its light. Examined, it tells the time to
//! the next storm.
//!
//! Without a radar tower (`/obj/machinery/power/weather_tower`, not ported) the forecast is
//! inaccurate as in tg: each storm cycle shifts it by `rand(-9, 9) * 5` seconds.
// shortcut: no radar towers (always the inaccurate forecast), no emissive overlays, no AAS
// config toggles or `act_up`; add with the towers and the AAS config window.
// tg resets `is_weather_dangerous` before every check, so its "incoming" warning is never said
// (the safe line is said instead) and every announcer speaks once at round start; here danger
// is judged for imminent weather only, as intended.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, IconStateId, Optics, Overlay};

use crate::combat::CombatRng;
use crate::tools::now;
use crate::weather::{ActiveWeather, Stage, WeatherSchedule};

/// `SSprocessing.wait`.
const PROCESS_SECS: f64 = 1.0;
/// `WEATHER_CLEAR_DELAY`, `WEATHER_INCOMING_DELAY`.
const CLEAR_DELAY: f64 = 120.0;
const INCOMING_DELAY: f64 = 30.0;
const RADIO: &str = "/obj/item/radio/weather_monitor";
const WALL_MONITOR: &str = "/obj/machinery/mining_weather_monitor";
const DMI: &str = "icons/obj/devices/miningradio.dmi";
/// `LIGHT_COLOR_GREEN`, `LIGHT_COLOR_DIM_YELLOW`, `LIGHT_COLOR_INTENSE_RED`.
const GREEN: [u8; 3] = [0x64, 0xC8, 0x64];
const DIM_YELLOW: [u8; 3] = [0xE1, 0xE1, 0x7D];
const INTENSE_RED: [u8; 3] = [0xC8, 0x19, 0x19];

/// `WEATHER_ALERT_*`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alert {
    #[default]
    Clear,
    Incoming,
    ImminentOrActive,
}

/// `/datum/component/weather_announcer`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct WeatherAnnouncer {
    pub level: Alert,
    /// `is_weather_dangerous`: meaningful for [`Alert::ImminentOrActive`] only.
    pub dangerous: bool,
    /// `state_normal`, `state_warning`, `state_danger`.
    states: [&'static str; 3],
    /// `accuracy_mod`, seconds.
    pub accuracy_mod: f64,
    /// The level overlay it shows.
    overlay: Option<IconStateId>,
    next_process: f64,
}

impl WeatherAnnouncer {
    fn new(states: [&'static str; 3]) -> Self {
        Self {
            level: Alert::Clear,
            dangerous: false,
            states,
            accuracy_mod: 0.0,
            overlay: None,
            next_process: 0.0,
        }
    }

    /// `on_update_overlays`.
    fn overlay_state(&self) -> &'static str {
        match self.level {
            Alert::Clear => self.states[0],
            Alert::Incoming => self.states[1],
            Alert::ImminentOrActive if self.dangerous => self.states[2],
            Alert::ImminentOrActive => self.states[1],
        }
    }

    /// `update_light_color`.
    fn light_color(&self) -> [u8; 3] {
        match self.level {
            Alert::Clear => GREEN,
            Alert::ImminentOrActive if self.dangerous => INTENSE_RED,
            _ => DIM_YELLOW,
        }
    }

    /// `get_warning_message`.
    fn message(&self) -> &'static str {
        match self.level {
            Alert::Clear => "Предупреждения о погоде отсутствуют.",
            Alert::Incoming => {
                "Внимание! К местному сектору приближается неблагоприятная погода. Немедленно \
                 прекратите любую деятельность на поверхности планеты."
            }
            Alert::ImminentOrActive if self.dangerous => {
                "Неблагоприятная погода охватила местный сектор. Немедленно найдите убежище."
            }
            Alert::ImminentOrActive => {
                "От приближающегося атмосферного фронта не ожидается неблагоприятной погоды."
            }
        }
    }

    /// The `/datum/aas_config_entry/weather` line (`is_weather_dangerous ? warning_level + 1 : 4`).
    fn aas_message(&self) -> &'static str {
        match self.level {
            Alert::Clear => "Все чисто, никаких предупреждений о погоде не поступало.",
            Alert::Incoming => "Приближается погодный фронт, начинайте искать укрытие.",
            Alert::ImminentOrActive if self.dangerous => {
                "Надвигается непогода, немедленно найдите укрытие."
            }
            Alert::ImminentOrActive => "Приближающийся погодный фронт не представляет опасности.",
        }
    }
}

/// What the mining level's weather looks like to a barometer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Forecast {
    /// No mining level (`time_till_storm` returns `null`).
    NoLevel,
    /// A barometer weather is telegraphing or raging there.
    Active { dangerous: bool },
    /// Seconds until the level may get its next weather (infinite: never).
    In(f64),
}

/// The raw `time_till_storm` over [`ActiveWeather`] and [`WeatherSchedule`].
pub fn forecast(world: &World) -> Forecast {
    let Some(z) = ss13_mapload::mining_z(world) else {
        return Forecast::NoLevel;
    };
    let mut active = None;
    if let Some(weathers) = world.get_resource::<ActiveWeather>() {
        for w in &weathers.0 {
            if w.barometer() && w.stage != Stage::WindDown && w.levels.contains(&z) {
                active = Some(active.unwrap_or(false) || w.functional());
            }
        }
    }
    if let Some(dangerous) = active {
        return Forecast::Active { dangerous };
    }
    let now = now(world);
    let next = world.get_resource::<WeatherSchedule>().and_then(|s| {
        s.levels
            .iter()
            .find(|(l, kinds, _)| *l == z && !kinds.is_empty())
            .map(|(_, _, at)| at.map_or(0.0, |t| (t - now).max(0.0)))
    });
    Forecast::In(next.unwrap_or(f64::INFINITY))
}

/// The inaccurate forecast: `time_until_next + accuracy_mod`, kept from backtracking past the
/// current level's threshold.
fn adjusted(time: f64, accuracy_mod: f64, level: Alert) -> f64 {
    let maximum = match level {
        Alert::Clear => f64::INFINITY,
        Alert::Incoming => CLEAR_DELAY,
        Alert::ImminentOrActive => INCOMING_DELAY,
    };
    (time + accuracy_mod).clamp(time.min(10.0), time.max(maximum))
}

/// `set_current_alert_level`: the level and danger for `forecast`, rolling a new
/// `accuracy_mod` (`roll`: `rand(-9, 9)`) when it has none.
fn alert_for(a: &mut WeatherAnnouncer, forecast: Forecast, roll: impl FnOnce() -> i32) -> Alert {
    a.dangerous = false;
    let time = match forecast {
        Forecast::NoLevel => return Alert::Clear,
        Forecast::Active { dangerous } => {
            a.accuracy_mod = 0.0;
            a.dangerous = dangerous;
            return Alert::ImminentOrActive;
        }
        Forecast::In(t) => {
            if a.accuracy_mod == 0.0 {
                a.accuracy_mod = f64::from(roll()) * 5.0;
            }
            adjusted(t, a.accuracy_mod, a.level)
        }
    };
    if time > CLEAR_DELAY {
        Alert::Clear
    } else if time > INCOMING_DELAY {
        Alert::Incoming
    } else {
        Alert::ImminentOrActive
    }
}

pub struct WeatherAnnouncerPlugin;

impl Plugin for WeatherAnnouncerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            process.run_if(any_with_component::<WeatherAnnouncer>),
        )
        .add_observer(radio_added)
        .add_observer(monitor_added);
    }
}

fn radio_added(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let e = add.entity;
    if items.get(e).is_ok_and(|i| is_under(&i.path, RADIO)) {
        commands.queue(move |world: &mut World| {
            attach(world, e, ["weatherwarning", "urgentwarning", "direwarning"]);
        });
    }
}

fn monitor_added(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let e = add.entity;
    if objs.get(e).is_ok_and(|o| is_under(&o.path, WALL_MONITOR)) {
        commands.queue(move |world: &mut World| {
            attach(world, e, ["wallgreen", "wallyellow", "wallred"]);
        });
    }
}

fn attach(world: &mut World, e: Entity, states: [&'static str; 3]) {
    if world.get_entity(e).is_err() {
        return;
    }
    world.entity_mut(e).insert(WeatherAnnouncer::new(states));
    refresh(world, e);
}

/// `enabled`: a machine's power (`COMSIG_MACHINERY_POWER_*`); a radio is always on.
fn enabled(world: &World, e: Entity) -> bool {
    world.get::<crate::power::Unpowered>(e).is_none()
}

/// `process`.
fn process(world: &mut World) {
    let now = now(world);
    let mut q = world.query::<(Entity, &WeatherAnnouncer)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, a)| a.next_process <= now)
        .map(|(e, _)| e)
        .collect();
    if due.is_empty() {
        return;
    }
    let forecast = forecast(world);
    for e in due {
        let was_enabled = world
            .get::<WeatherAnnouncer>(e)
            .is_some_and(|a| a.overlay.is_some());
        let on = enabled(world, e);
        if on != was_enabled {
            refresh(world, e);
        }
        let Some(mut a) = world.get::<WeatherAnnouncer>(e).cloned() else {
            continue;
        };
        a.next_process = now + PROCESS_SECS;
        if !on {
            world.entity_mut(e).insert(a);
            continue;
        }
        let (previous, previous_danger) = (a.level, a.dangerous);
        let level = alert_for(&mut a, forecast, || {
            world.resource_mut::<CombatRng>().range(0, 18) as i32 - 9
        });
        a.level = level;
        let changed = level != previous
            || (level == Alert::ImminentOrActive && a.dangerous != previous_danger);
        let (message, aas_message) = (a.message(), a.aas_message());
        world.entity_mut(e).insert(a);
        if !changed {
            continue;
        }
        let said = if crate::arrivals::aas_z(world).is_some() {
            crate::arrivals::aas_announce_on(world, ss13_chat::radio::FREQ_SUPPLY, aas_message);
            aas_message
        } else {
            message
        };
        crate::atmos_machinery::machine_say(world, e, said);
        refresh(world, e);
    }
}

/// `update_appearance` (the level overlay while enabled) and `update_light_color`.
fn refresh(world: &mut World, e: Entity) {
    let Some(a) = world.get::<WeatherAnnouncer>(e).cloned() else {
        return;
    };
    let wanted = if enabled(world, e) {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(DMI, a.overlay_state()))
    } else {
        None
    };
    if wanted != a.overlay {
        if let Some(mut appearance) = world.get_mut::<Appearance>(e) {
            if let Some(old) = a.overlay {
                appearance.overlays.retain(|o| o.icon_state != old);
            }
            if let Some(new) = wanted {
                appearance.overlays.push(Overlay::new(new));
            }
        }
        if let Some(mut stored) = world.get_mut::<WeatherAnnouncer>(e) {
            stored.overlay = wanted;
        }
    }
    let color = a.light_color();
    let recolour = world
        .get::<Optics>(e)
        .and_then(|o| o.light.as_ref())
        .is_some_and(|l| l.color != color);
    if recolour
        && let Some(mut optics) = world.get_mut::<Optics>(e)
        && let Some(light) = optics.light.as_mut()
    {
        light.color = color;
    }
}

/// `on_examine`: the time to the next storm and the accuracy warning.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(a) = world.get::<WeatherAnnouncer>(target) else {
        return;
    };
    let time = match forecast(world) {
        Forecast::NoLevel => return,
        Forecast::Active { .. } => 0.0,
        Forecast::In(t) => adjusted(t, a.accuracy_mod, a.level),
    };
    if time == 0.0 {
        lines.push((
            Span::Warning,
            "В настоящее время активен шторм, пожалуйста, не покидайте укрытие.".to_owned(),
        ));
    } else if time.is_finite() {
        lines.push((
            Span::Notice,
            format!(
                "Следующий шторм ожидается через {}.",
                crate::jobs::display_time_text(time)
            ),
        ));
    }
    lines.push((
        Span::Italics,
        "Из-за недостаточного покрытия радаров время прогноза может быть неточным.".to_owned(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn announcer() -> WeatherAnnouncer {
        WeatherAnnouncer::new(["n", "w", "d"])
    }

    #[test]
    fn the_time_to_the_storm_picks_the_level() {
        let mut a = announcer();
        assert_eq!(alert_for(&mut a, Forecast::NoLevel, || 0), Alert::Clear);
        // A +45 s error: 100 s reads as 145 s, still clear.
        assert_eq!(alert_for(&mut a, Forecast::In(100.0), || 9), Alert::Clear);
        assert_eq!(a.accuracy_mod, 45.0);
        assert_eq!(alert_for(&mut a, Forecast::In(60.0), || 9), Alert::Incoming);
        a.level = Alert::Incoming;
        // Incoming does not backtrack past the clear delay.
        assert_eq!(adjusted(100.0, 45.0, Alert::Incoming), 120.0);
        assert_eq!(
            alert_for(&mut a, Forecast::Active { dangerous: true }, || 0),
            Alert::ImminentOrActive
        );
        assert!(
            a.dangerous && a.accuracy_mod == 0.0,
            "a storm resets the error"
        );
        a.level = Alert::ImminentOrActive;
        assert_eq!(a.overlay_state(), "d");
        assert_eq!(a.light_color(), INTENSE_RED);
        assert_eq!(
            alert_for(&mut a, Forecast::Active { dangerous: false }, || 0),
            Alert::ImminentOrActive
        );
        assert_eq!(a.overlay_state(), "w");
        assert!(a.message().starts_with("От приближающегося"));
        // Never scheduled: clear whatever the error.
        assert_eq!(
            alert_for(&mut a, Forecast::In(f64::INFINITY), || -9),
            Alert::Clear
        );
    }
}
