//! Overclocked atmos machines (server).
//!
//! - Vents (tg `vent_pump`): the air alarm's `overclock` button lifts the 50 atm limit and the
//!   integrity scaling of the flow, but every atmos fire the fan takes 0.5 damage without armor
//!   (`fan_damage_rate`) until it is destroyed; a screwdriver mends it in up to 10 s. A fan
//!   loop (`/datum/looping_sound/vent_pump_overclock`) plays while it runs overclocked.
//! - Volume pumps (tg `volume_pump/multitool_act`): no output pressure limit, a tenth of the
//!   flow leaks onto the tile (`crate::atmos_machinery::devices::volume_pump_step`).
// shortcut: a destroyed vent leaves no damaged pipe fitting (pipe fittings are not items yet).

use bevy::prelude::*;
use ss13_chat::Span;
use ss13_mapload::DmObj;
use ss13_shared::TilePos;

use super::AtmosDevice;
use super::devices::{VentPump, VolumePump};
use crate::obj_damage::Integrity;
use crate::power::Unpowered;
use crate::tools::tell;

/// `fan_damage_rate`.
const FAN_DAMAGE_RATE: f32 = 0.5;
/// `vent_pump_overclock`: `start_length`, `mid_length`, default volume.
const FAN_START_SECS: f64 = 1.5;
const FAN_LOOP_SECS: f64 = 1.9;
const FAN_VOLUME: f32 = 100.0;
pub const FAN_START: &str = "sound/machines/fan/fan_start.ogg";
pub const FAN_LOOP: &str = "sound/machines/fan/fan_loop.ogg";
pub const FAN_STOP: &str = "sound/machines/fan/fan_stop.ogg";

/// A vent's running fan loop: when the next `mid_sounds` clip plays.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FanLoop {
    next_at: f64,
}

fn play(world: &mut World, e: Entity, file: &str) {
    if let Some(&pos) = world.get::<TilePos>(e) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, file, FAN_VOLUME, false),
        );
    }
}

/// `toggle_overclock`: flips it and starts or stops the fan loop.
pub fn toggle_vent_overclock(world: &mut World, vent: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut v) = world.get_mut::<VentPump>(vent) else {
        return;
    };
    v.overclocked = !v.overclocked;
    if v.overclocked {
        world.entity_mut(vent).insert(FanLoop {
            next_at: now + FAN_START_SECS,
        });
        play(world, vent, FAN_START);
    } else {
        world.entity_mut(vent).remove::<FanLoop>();
        play(world, vent, FAN_STOP);
    }
}

/// `vent_pump/process_atmos`: an overclocked, running vent wears its fan before it moves gas.
pub fn damage_overclocked_vents(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &VentPump, &AtmosDevice), Without<Unpowered>>();
    let net = world.resource::<ss13_atmos::pipenet::Pipenet>();
    let wearing: Vec<Entity> = q
        .iter(world)
        .filter(|(_, v, d)| {
            v.overclocked
                && v.on
                && !v.welded
                && net
                    .device(d.id)
                    .is_some_and(|dev| dev.nodes.first().copied().flatten().is_some())
        })
        .map(|(e, ..)| e)
        .collect();
    for vent in wearing {
        // `take_damage(fan_damage_rate)` has no damage flag: no armor.
        crate::obj_damage::take_damage_flagless(world, vent, FAN_DAMAGE_RATE);
    }
}

/// The fan loop's clips.
pub fn fan_loops(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &mut FanLoop)>();
    let mut due = Vec::new();
    for (e, mut fan) in q.iter_mut(world) {
        if now >= fan.next_at {
            fan.next_at = now + FAN_LOOP_SECS;
            due.push(e);
        }
    }
    for e in due {
        play(world, e, FAN_LOOP);
    }
}

/// `get_integrity_percentage()`: `round(integrity / max_integrity, 0.01)`.
pub fn integrity_percent(world: &World, e: Entity) -> f64 {
    match (world.get::<Integrity>(e), world.get::<DmObj>(e)) {
        (Some(i), Some(o)) if o.max_integrity > 0.0 => {
            (f64::from(i.current / o.max_integrity) * 100.0).round() / 100.0
        }
        _ => 1.0,
    }
}

/// `on_update_integrity`'s condition string (`switch` ranges of the rounded percentage).
fn condition(percent: f64) -> &'static str {
    if percent == 1.0 {
        "идеальном"
    } else if (0.75..=0.99).contains(&percent) {
        "хорошем"
    } else if (0.50..=0.74).contains(&percent) {
        "нормальном"
    } else if (0.25..=0.49).contains(&percent) {
        "плохом"
    } else {
        "ужасном"
    }
}

/// `vent_pump/examine` and `volume_pump/examine`.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<(Span, String)>) {
    if let Some(v) = world.get::<VentPump>(e) {
        if v.overclocked {
            lines.push((
                Span::Warning,
                "Вентиляция работает в разгоне и со временем изнашивается.".into(),
            ));
        }
        lines.push((
            Span::Notice,
            format!(
                "Вентилятор в {} состоянии.",
                condition(integrity_percent(world, e))
            ),
        ));
    }
    if let Some(p) = world.get::<VolumePump>(e) {
        let state = if p.overclocked {
            "включить"
        } else {
            "отключить"
        };
        lines.push((
            Span::Notice,
            format!("Его ограничители давления можно {state} <b>мультитулом</b>."),
        ));
        if p.overclocked {
            let tail = if p.on {
                " и он извергает газ!"
            } else {
                "."
            };
            lines.push((
                Span::Plain,
                format!("Горит предупреждающий индикатор{tail}"),
            ));
        }
    }
}

/// `volume_pump/multitool_act`. True when it was a volume pump.
pub fn multitool_volume_pump(world: &mut World, client: Entity, target: Entity) -> bool {
    let Some(mut p) = world.get_mut::<VolumePump>(target) else {
        return false;
    };
    p.overclocked = !p.overclocked;
    let text = if p.overclocked {
        "Насос скрежещет, и воздух начинает шипеть, когда вы отключаете ограничители давления."
    } else {
        "Насос затихает, когда вы снова включаете ограничители."
    };
    tell(world, client, Span::Notice, text);
    true
}

/// `vent_pump/screwdriver_act`: `10 SECONDS * (1 - integrity)`; nothing to do when whole.
/// The delay in deciseconds, `None` when it needs no repair.
pub fn vent_repair_delay(world: &World, vent: Entity) -> Option<f32> {
    world.get::<VentPump>(vent)?;
    let delay = 100.0 * (1.0 - integrity_percent(world, vent));
    (delay > 0.0).then_some(delay as f32)
}

/// The end of the repair: `repair_damage(max_integrity)`.
pub fn repair_vent(world: &mut World, client: Entity, vent: Entity) {
    let Some(max) = world.get::<DmObj>(vent).map(|o| o.max_integrity) else {
        return;
    };
    if let Some(mut i) = world.get_mut::<Integrity>(vent) {
        i.current = max;
    }
    tell(world, client, Span::Notice, "Вентиляция отремонтирована.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions_follow_the_tg_ranges() {
        assert_eq!(condition(1.0), "идеальном");
        assert_eq!(condition(0.8), "хорошем");
        assert_eq!(condition(0.5), "нормальном");
        assert_eq!(condition(0.3), "плохом");
        assert_eq!(condition(0.1), "ужасном");
    }
}
