//! Grid Check (`code/modules/events/grid_check.dm`): every station APC with a cell fails for
//! 1-4 minutes (tg `power_fail(60, 240)`) and comes back by itself as its `failure_timer` runs
//! out (`ss13_game::power`).

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::power::{Apc, AreaPower, AreaPowerChanged, MACHINE_TICK_SECS};

pub const NAME: &str = "Grid Check";
/// The control's `announcement_spam_protection`.
const ANNOUNCE_COOLDOWN_SECS: f64 = 30.0;

/// `COOLDOWN_DECLARE(announcement_spam_protection)` of the Grid Check control.
#[derive(Resource, Default)]
pub struct GridCheckCooldown(pub f64);

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(GridCheck)
}

struct GridCheck;

impl RoundEvent for GridCheck {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
        ev.start_when = 1;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        if !fake && world.resource::<GridCheckCooldown>().0 > now {
            return;
        }
        let text = format!(
            "Обнаружена аномальная активность в сети питания {}. В качестве меры предосторожности \
             питание будет отключено на неопределенный срок.",
            super::station_name(world)
        );
        super::priority_announce(world, "Критический сбой системы питания", &text);
        if !fake {
            world.resource_mut::<GridCheckCooldown>().0 = now + ANNOUNCE_COOLDOWN_SECS;
        }
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        power_fail(world, 60, 240);
    }
}

/// tg `power_fail(duration_min, duration_max)`: `energy_fail(rand(min, max))` on every APC with
/// a cell on a station level.
// shortcut: no `TRAIT_AREA_BLOCK_POWER_FAIL` (its element is on machines not ported yet).
pub fn power_fail(world: &mut World, min_secs: i64, max_secs: i64) {
    let levels = station_levels(world);
    let mut q = world.query::<(Entity, &Apc, &ss13_shared::TilePos)>();
    let apcs: Vec<Entity> = q
        .iter(world)
        .filter(|(_, apc, pos)| apc.cell.is_some() && levels.contains(&pos.z))
        .map(|(e, ..)| e)
        .collect();
    for apc in apcs {
        let secs = world.resource_mut::<EventRng>().0.range(min_secs, max_secs);
        energy_fail(world, apc, secs as f64);
    }
}

/// BandaStation `/obj/machinery/power/apc/proc/reboot()` on every APC with a cell on a station
/// level: a power failure ends at once.
pub fn reboot_station_apcs(world: &mut World) {
    let levels = station_levels(world);
    let mut q = world.query::<(Entity, &Apc, &ss13_shared::TilePos)>();
    let apcs: Vec<Entity> = q
        .iter(world)
        .filter(|(_, apc, pos)| apc.cell.is_some() && levels.contains(&pos.z))
        .map(|(e, ..)| e)
        .collect();
    for apc in apcs {
        let Some(mut state) = world.get_mut::<Apc>(apc) else {
            continue;
        };
        state.failure_timer = 0.0;
        let state = state.clone();
        let changed = world
            .get_mut::<AreaPower>(state.area)
            .is_some_and(|mut area| state.update(&mut area));
        if changed {
            world.write_message(AreaPowerChanged(state.area));
        }
    }
}

/// tg `/obj/machinery/power/apc/proc/energy_fail(duration)`: `failure_timer` rounded to the
/// machine tick, then `update()`.
fn energy_fail(world: &mut World, apc: Entity, secs: f64) {
    let Some(mut state) = world.get_mut::<Apc>(apc) else {
        return;
    };
    let rounded = (secs / MACHINE_TICK_SECS).round() * MACHINE_TICK_SECS;
    state.failure_timer = state.failure_timer.max(rounded);
    let state = state.clone();
    let changed = world
        .get_mut::<AreaPower>(state.area)
        .is_some_and(|mut area| state.update(&mut area));
    if changed {
        world.write_message(AreaPowerChanged(state.area));
    }
}
