//! Portal Storm (`code/modules/events/portal_storm.dm`): bluespace portals flash over random
//! station turfs and hostiles step out of them, every other tick, with their bosses spaced
//! between; Syndicate shocktroops (random) or Nar'Sie's constructs (admins only).
//!
//! The mobs are spawned by type (`crate::mobs::spawn::spawn_mob`); what they do is their own
//! definition's (troopers and hostile constructs belong to `docs/parity/mobs/`).
// shortcut: the storm's flash is a 1.5 s temporary visual on the turf south-west of the spot
// (tg `flick_overlay_static`, same icon, color and duration).

use bevy::prelude::*;
use ss13_shared::{Dir, TilePos};

use super::controller::{EventRng, EventState, RoundEvent};
use crate::effects::temp_visual::{self, Visual};
use crate::sound::{PlaySound, playsound};

pub const SYNDICATE: &str = "Portal Storm: Syndicate Shocktroops";
pub const NARSIE: &str = "Portal Storm: Constructs";

const CHARGEUP: &str = "sound/effects/magic/lightning_chargeup.ogg";
const BOLT: &str = "sound/effects/magic/lightningbolt.ogg";
/// The announcement comes 8 s after the charge-up sound, the bolt 2 s after it (4 and 5
/// event ticks of 2 s).
const ANNOUNCE_DELAY_TICKS: u32 = 4;

/// `storm_appearances`: `energy_ball_fast` in `COLOR_VIBRANT_LIME`, `FLY_LAYER` on
/// `ABOVE_GAME_PLANE`, 15 deciseconds.
const STORM: Visual = Visual {
    color: Some([0x00, 0xFF, 0x00, 0xFF]),
    layer: 5.0,
    plane: -3,
    dir: Some(Dir::South),
    ..Visual::new(
        "icons/obj/machines/engine/energy_ball.dmi",
        "energy_ball_fast",
        1.5,
    )
};

pub const SYNDICATE_BOSSES: &[(&str, i64)] = &[(
    "/mob/living/basic/trooper/syndicate/melee/space/stormtrooper",
    2,
)];
pub const SYNDICATE_HOSTILES: &[(&str, i64)] = &[
    ("/mob/living/basic/trooper/syndicate/melee/space", 8),
    ("/mob/living/basic/trooper/syndicate/ranged/space", 2),
];
pub const NARSIE_BOSSES: &[(&str, i64)] = &[("/mob/living/basic/construct/artificer/hostile", 6)];
pub const NARSIE_HOSTILES: &[(&str, i64)] = &[
    ("/mob/living/basic/construct/juggernaut/hostile", 8),
    ("/mob/living/basic/construct/wraith/hostile", 6),
];

/// `/datum/round_event/portal_storm`.
pub struct PortalStorm {
    /// What is left to spawn, by weight = count (tg's `boss_types`, `hostile_types`).
    pub bosses: Vec<(&'static str, i64)>,
    pub hostiles: Vec<(&'static str, i64)>,
    boss_spawn: Vec<TilePos>,
    hostiles_spawn: Vec<TilePos>,
    number_of_bosses: u64,
    number_of_hostiles: u64,
    next_boss_spawn: u32,
    /// The tick of the charge-up sound (`announce`), for its delayed announcement.
    announced_at: Option<u32>,
    /// `get_random_station_turf()`'s pool, taken once.
    station: Vec<TilePos>,
}

impl PortalStorm {
    pub fn new(bosses: &[(&'static str, i64)], hostiles: &[(&'static str, i64)]) -> Self {
        Self {
            bosses: bosses.to_vec(),
            hostiles: hostiles.to_vec(),
            boss_spawn: Vec::new(),
            hostiles_spawn: Vec::new(),
            number_of_bosses: bosses.iter().map(|(_, n)| n).sum::<i64>() as u64,
            number_of_hostiles: hostiles.iter().map(|(_, n)| n).sum::<i64>() as u64,
            next_boss_spawn: 0,
            announced_at: None,
            station: Vec::new(),
        }
    }

    fn random_station_turf(&self, world: &mut World) -> Option<TilePos> {
        world
            .resource_mut::<EventRng>()
            .pick(&self.station)
            .copied()
    }

    /// `spawn_mob(type, spawn_list)`: one of the type on a spot taken from the list.
    fn spawn_mob(world: &mut World, path: &str, spots: &mut Vec<TilePos>) {
        if spots.is_empty() {
            return;
        }
        let i = world
            .resource_mut::<EventRng>()
            .0
            .range(0, spots.len() as i64 - 1) as usize;
        let pos = spots.swap_remove(i);
        if crate::mobs::spawn::spawn_mob(world, path, pos).is_none() {
            warn!("portal storm: no prototype for {path}");
        }
        spawn_effects(world, Some(pos));
    }
}

pub fn make_syndicate(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(PortalStorm::new(SYNDICATE_BOSSES, SYNDICATE_HOSTILES))
}

pub fn make_narsie(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(PortalStorm::new(NARSIE_BOSSES, NARSIE_HOSTILES))
}

/// `spawn_effects(T)`: the storm's flash and a bolt.
fn spawn_effects(world: &mut World, pos: Option<TilePos>) {
    let Some(pos) = pos else {
        warn!("Portal Storm failed to spawn effect due to an invalid location.");
        return;
    };
    let corner = TilePos::new(pos.x - 1, pos.y - 1, pos.z);
    temp_visual::spawn(world, corner, STORM);
    let vol = world.resource_mut::<EventRng>().0.range(80, 100) as f32;
    playsound(world, PlaySound::new(corner, BOLT, vol, true));
}

/// One draw from a `type = count` list, the count spent (gone at 0).
fn take(list: &mut Vec<(&'static str, i64)>, world: &mut World) -> Option<&'static str> {
    // tg `pick(list)`: uniform over the types left (not weighted by the counts).
    let i = world
        .resource_mut::<EventRng>()
        .0
        .range(0, list.len() as i64 - 1) as usize;
    let entry = list.get_mut(i)?;
    entry.1 -= 1;
    let path = entry.0;
    if entry.1 <= 0 {
        list.remove(i);
    }
    Some(path)
}

impl RoundEvent for PortalStorm {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        ev.start_when = 7;
        ev.end_when = 999;
        ev.announce_when = 1;
        self.station = super::station_tiles(world)
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        if self.station.is_empty() {
            return;
        }
        for _ in 0..self.number_of_bosses {
            let pos = self.random_station_turf(world).expect("non-empty");
            self.boss_spawn.push(pos);
        }
        for _ in 0..self.number_of_hostiles {
            let pos = self.random_station_turf(world).expect("non-empty");
            self.hostiles_spawn.push(pos);
        }
        if self.number_of_bosses > 0 {
            let every = (2 * self.number_of_hostiles).div_ceil(self.number_of_bosses);
            self.next_boss_spawn = ev.start_when + every as u32;
        }
    }

    fn announce(&mut self, world: &mut World, ev: &mut EventState, _: bool) {
        playsound(world, PlaySound::to_playing_players(CHARGEUP, 100.0, false));
        self.announced_at = Some(ev.active_for);
    }

    fn tick(&mut self, world: &mut World, ev: &mut EventState) {
        self.delayed_announce(world, ev);
        let spot = self.random_station_turf(world);
        spawn_effects(world, spot);

        if ev.active_for.is_multiple_of(2) && !self.hostiles.is_empty() {
            let mut hostiles = std::mem::take(&mut self.hostiles);
            if let Some(path) = take(&mut hostiles, world) {
                Self::spawn_mob(world, path, &mut self.hostiles_spawn);
            }
            self.hostiles = hostiles;
        }

        if !self.bosses.is_empty() && ev.active_for == self.next_boss_spawn {
            let every = self
                .number_of_hostiles
                .div_ceil(self.number_of_bosses.max(1));
            self.next_boss_spawn += every as u32;
            let mut bosses = std::mem::take(&mut self.bosses);
            if let Some(path) = take(&mut bosses, world) {
                Self::spawn_mob(world, path, &mut self.boss_spawn);
            }
            self.bosses = bosses;
        }

        // `time_to_end()`.
        if self.hostiles.is_empty() && self.bosses.is_empty() {
            ev.end_when = ev.active_for;
        }
        if self.number_of_hostiles == 0 && self.number_of_bosses > 0 {
            ev.end_when = ev.active_for;
        }
    }
}

impl PortalStorm {
    /// tg's `sleep(8 SECONDS)` then the announcement, `sleep(2 SECONDS)` then the bolt; the
    /// event ticks every 2 s, so they land 4 and 5 ticks after the charge-up.
    fn delayed_announce(&mut self, world: &mut World, ev: &EventState) {
        let Some(at) = self.announced_at else {
            return;
        };
        if ev.active_for == at + ANNOUNCE_DELAY_TICKS {
            let text = format!(
                "Обнаружена внушительная блюспейс аномалия на встречном с {} курсе. \
                 Приготовьтесь к удару.",
                super::station_name(world)
            );
            super::priority_announce(world, "", &text);
        } else if ev.active_for == at + ANNOUNCE_DELAY_TICKS + 1 {
            playsound(world, PlaySound::to_playing_players(BOLT, 100.0, false));
            self.announced_at = None;
        }
    }
}

/// Every hostile type of both storms, for the runtime type registry.
pub fn mob_types() -> impl Iterator<Item = &'static str> {
    [
        SYNDICATE_BOSSES,
        SYNDICATE_HOSTILES,
        NARSIE_BOSSES,
        NARSIE_HOSTILES,
    ]
    .into_iter()
    .flatten()
    .map(|&(p, _)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobs_are_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        let missing: Vec<&str> = mob_types().filter(|p| !registry.contains(p)).collect();
        assert!(
            missing.is_empty(),
            "not in ss13_mapload::runtime: {missing:?}"
        );
    }

    #[test]
    fn counts_and_boss_spacing_follow_tg() {
        let storm = PortalStorm::new(SYNDICATE_BOSSES, SYNDICATE_HOSTILES);
        assert_eq!((storm.number_of_bosses, storm.number_of_hostiles), (2, 10));
        let narsie = PortalStorm::new(NARSIE_BOSSES, NARSIE_HOSTILES);
        assert_eq!(
            (narsie.number_of_bosses, narsie.number_of_hostiles),
            (6, 14)
        );
        // `start_when + ceil(2 * 10 / 2)`, then every `ceil(10 / 2)` ticks.
        assert_eq!(7 + (2 * 10_u64).div_ceil(2), 17);
    }
}
