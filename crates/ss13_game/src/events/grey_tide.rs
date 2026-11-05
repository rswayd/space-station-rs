//! Grey Tide (`code/modules/events/grey_tide.dm`): `rand(1, 3)` departments; when the event
//! ends (20-30 fires in) every airlock there opens and bolts (`prison_open`), the secure
//! lockers unlock and the APCs turn their lighting off (`COMSIG_GLOB_GREY_TIDE`). The
//! announcement comes 50-60 fires in, after the doors.
// shortcut: no light flicker every 12 fires (`COMSIG_GLOB_GREY_TIDE_LIGHT`, lights cannot
// flicker yet), no brig cell timers (not ported), no `critical_machine` doors (none ported).

use std::ops::RangeInclusive;

use bevy::prelude::*;
use ss13_mapload::DoorKind;
use ss13_shared::TilePos;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::closets::Closet;
use crate::doors::Door;
use crate::obj_damage::is_under;
use crate::power::model::CHANNEL_OFF;
use crate::power::{Apc, Channel};

pub const NAME: &str = "Grey Tide";
const POTENTIAL_AREAS: [&str; 6] = [
    "/area/station/command",
    "/area/station/engineering",
    "/area/station/medical",
    "/area/station/security",
    "/area/station/cargo",
    "/area/station/science",
];
const SECURE_CLOSET: &str = "/obj/structure/closet/secure_closet";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(GreyTide {
        severity: 1,
        areas: Vec::new(),
    })
}

struct GreyTide {
    severity: i64,
    areas: Vec<&'static str>,
}

impl RoundEvent for GreyTide {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        let rng = &mut world.resource_mut::<EventRng>().0;
        ev.announce_when = rng.range(50, 60) as u32;
        ev.end_when = rng.range(20, 30) as u32;
        self.severity = rng.range(1, 3);
        let mut potential = POTENTIAL_AREAS.to_vec();
        for _ in 0..self.severity {
            let i = rng.range(0, potential.len() as i64 - 1) as usize;
            self.areas.push(potential.swap_remove(i));
        }
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        if fake {
            self.severity = world.resource_mut::<EventRng>().0.range(1, 3);
        }
        let text = format!(
            "Вирус Gr3y.T1d3 обнаружен в подпрограммах шифрования системы безопасной блокировки \
             {}. Уровень угрозы {}. Рекомендуется участие ИИ станции.",
            super::station_name(world),
            self.severity
        );
        super::priority_announce(world, "Оповещение безопасности", &text);
    }

    fn start(&mut self, _: &mut World, ev: &mut EventState) {
        if self.areas.is_empty() {
            ev.killed = true;
        }
    }

    fn end(&mut self, world: &mut World, _: &mut EventState) {
        grey_tide(world, &self.areas);
    }
}

/// `SEND_GLOBAL_SIGNAL(COMSIG_GLOB_GREY_TIDE, areas)`: the airlocks, secure lockers and APCs
/// on station levels in an area of one of `areas` (or a subtype).
pub fn grey_tide(world: &mut World, areas: &[&str]) {
    let levels = station_levels(world);

    let mut q = world.query::<(Entity, &Door, &TilePos)>();
    let airlocks: Vec<Entity> = q
        .iter(world)
        .filter(|(_, d, _)| d.kind == DoorKind::Airlock)
        .filter(|&(_, _, &pos)| struck(world, pos, &levels, areas))
        .map(|(e, ..)| e)
        .collect();
    for door in airlocks {
        prison_open(world, door);
    }

    let mut q = world.query_filtered::<(Entity, &TilePos), With<Closet>>();
    let closets: Vec<Entity> = q
        .iter(world)
        .filter(|&(_, &pos)| struck(world, pos, &levels, areas))
        .map(|(e, _)| e)
        .collect();
    for closet in closets {
        let secure = crate::construction::path_of(world, closet)
            .is_some_and(|p| is_under(&p, SECURE_CLOSET));
        if !secure {
            continue;
        }
        if let Some(mut state) = world.get_mut::<Closet>(closet) {
            state.locked = false;
        }
        crate::closets::refresh_look(world, closet);
    }

    let mut q = world.query_filtered::<(Entity, &TilePos), With<Apc>>();
    let apcs: Vec<Entity> = q
        .iter(world)
        .filter(|&(_, &pos)| struck(world, pos, &levels, areas))
        .map(|(e, _)| e)
        .collect();
    for apc in apcs {
        if let Some(mut a) = world.get_mut::<Apc>(apc) {
            a.channels[Channel::Light as usize] = CHANNEL_OFF;
        }
        crate::wires::apc_update(world, apc);
    }
}

/// On a station level, in an area of one of `areas` (or a subtype).
fn struck(world: &World, pos: TilePos, levels: &RangeInclusive<u16>, areas: &[&str]) -> bool {
    levels.contains(&pos.z)
        && super::area_type_at(world, pos).is_some_and(|a| areas.iter().any(|r| is_under(a, r)))
}

/// tg airlock `prison_open()`: unbolted, opened (if it has power), bolted.
// shortcut: no `EMAGGED` check (airlocks cannot be emagged yet).
fn prison_open(world: &mut World, door: Entity) {
    crate::wires::set_bolts(world, door, false);
    crate::doors::with_door(world, door, |d, appearance, ctx| {
        let opened = ctx.open(d, appearance, false);
        (opened, opened)
    });
    crate::wires::set_bolts(world, door, true);
}
