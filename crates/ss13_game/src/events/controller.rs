//! tg `SSevents` (`code/controllers/subsystem/events.dm`), `/datum/round_event_control` and
//! `/datum/round_event` (`code/modules/events/_event.dm`).
//!
//! - Every [`EVENTS_WAIT_SECS`] while the round is in progress ([`events_fire`]): when the
//!   scheduled time has come, a random event is picked by weight among the controls that can
//!   spawn ([`Events::roster`]), the next one is scheduled `rand(frequency_lower,
//!   frequency_upper)` later, and the picked one triggers after the 20 s admin intervention
//!   window, if it still can spawn (otherwise another one is rolled without it). Then every
//!   running event processes once (tg `/datum/round_event/process`).
//! - [`force_event`]: the admin Force Event (`/datum/round_event_control/run_event` with
//!   `admin_forced`), reached through the dev test hook and `SS13_FORCE_EVENT`.
//!
//! Cost: one pass over ~30 controls and the running events every 2 s.

use bevy::prelude::*;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{MapLoadPlugin, MapSeed};

use crate::ghost::Ghost;
use crate::health::{Health, Stat};
use crate::lobby::GameState;
use crate::round::Ticker;
use crate::server::PlayerMob;

/// `SSevents.wait`: the subsystem default, 2 s (tg `/datum/controller/subsystem` `wait = 20`).
/// A running event's `activeFor` counts these.
pub const EVENTS_WAIT_SECS: f64 = 2.0;
/// tg `RANDOM_EVENT_ADMIN_INTERVENTION_TIME`.
pub const ADMIN_INTERVENTION_SECS: f64 = 20.0;
/// BandaStation `config/game_options.txt` `EVENTS_FREQUENCY_LOWER 1500` /
/// `EVENTS_FREQUENCY_UPPER 4200`, deciseconds.
pub const FREQUENCY_LOWER_DS: i64 = 1500;
pub const FREQUENCY_UPPER_DS: i64 = 4200;
/// Fires one frame may catch up after a stall.
const MAX_CATCH_UP: u32 = 3;

/// Random event settings (tg config). Insert before `GameServerPlugin` to override; otherwise
/// from the environment:
/// - `SS13_RANDOM_EVENTS=0`: no random events (tg `ALLOW_RANDOM_EVENTS` off; BandaStation's
///   config has it on). Forced events still run.
/// - `SS13_FORCE_EVENT="Grid Check,Meteor Wave: Normal"` (debug builds only): these events are
///   forced when the round starts.
///
/// BandaStation's `config/events.json` overrides apply only with `EVENTS_CONFIG_ENABLED`, which
/// its config leaves off, so the controls keep their code values.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct EventsConfig {
    pub allow_random_events: bool,
    /// Deciseconds, as tg's config.
    pub frequency_lower: i64,
    pub frequency_upper: i64,
    pub forced: Vec<String>,
}

impl EventsConfig {
    /// BandaStation's config, no environment.
    pub fn tg() -> Self {
        Self {
            allow_random_events: true,
            frequency_lower: FREQUENCY_LOWER_DS,
            frequency_upper: FREQUENCY_UPPER_DS,
            forced: Vec::new(),
        }
    }

    /// Test and bot servers: no random events, so a meteor or a grid check never lands in a
    /// test by chance; a test that wants one forces it ([`super::force_event`]).
    pub fn forced_only() -> Self {
        Self {
            allow_random_events: false,
            ..Self::tg()
        }
    }
}

impl Default for EventsConfig {
    fn default() -> Self {
        let forced: Vec<String> = std::env::var("SS13_FORCE_EVENT")
            .ok()
            .filter(|_| cfg!(debug_assertions))
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            allow_random_events: std::env::var("SS13_RANDOM_EVENTS").map_or(true, |v| v != "0"),
            forced,
            ..Self::tg()
        }
    }
}

/// DM `rand`/`prob`/`pick` for the scheduler and the events.
#[derive(Resource)]
pub struct EventRng(pub Rng);

impl EventRng {
    /// BYOND `rand()`: a float in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.0.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// BYOND `pick(list)`.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        items.get(self.0.range(0, items.len() as i64 - 1) as usize)
    }
}

/// tg `pick_weight`: `rand(1, total)` walked down the list; zero weights are never picked.
pub fn pick_weight<T: Copy>(items: &[(T, i64)], rng: &mut Rng) -> Option<T> {
    let total: i64 = items.iter().map(|(_, w)| *w).sum();
    let mut roll = rng.range(1, total);
    for &(item, weight) in items {
        roll -= weight;
        if roll <= 0 && weight != 0 {
            return Some(item);
        }
    }
    None
}

/// A running event's timing and counters (tg `/datum/round_event` vars). Times are in
/// [`EVENTS_WAIT_SECS`] fires.
#[derive(Clone, Debug, PartialEq)]
pub struct EventState {
    /// Index into [`Events::controls`].
    pub control: usize,
    pub start_when: u32,
    pub announce_when: u32,
    /// Percent.
    pub announce_chance: f32,
    pub end_when: u32,
    /// tg `activeFor`.
    pub active_for: u32,
    /// tg `current_players`.
    pub current_players: u32,
    /// tg `kill()`: removed from processing.
    pub killed: bool,
}

impl EventState {
    pub fn new(control: usize) -> Self {
        Self {
            control,
            start_when: 0,
            announce_when: 0,
            announce_chance: 100.0,
            end_when: 0,
            active_for: 0,
            current_players: 0,
            killed: false,
        }
    }
}

/// A `/datum/round_event` subtype. Each hook gets the world and its state; the defaults do
/// nothing, as tg's.
pub trait RoundEvent: Send + Sync + 'static {
    /// tg `setup()`: may change the timing.
    fn setup(&mut self, _world: &mut World, _ev: &mut EventState) {}
    /// tg `announce(fake)`; `fake` from a false alarm (no `setup` ran).
    fn announce(&mut self, _world: &mut World, _ev: &mut EventState, _fake: bool) {}
    fn start(&mut self, _world: &mut World, _ev: &mut EventState) {}
    fn tick(&mut self, _world: &mut World, _ev: &mut EventState) {}
    fn end(&mut self, _world: &mut World, _ev: &mut EventState) {}
}

/// tg `/datum/round_event_control`: one per event type.
pub struct EventControl {
    pub name: &'static str,
    pub weight: i64,
    /// Seconds of round time before it may spawn (tg `earliest_start`).
    pub earliest_start: f64,
    pub min_players: u32,
    pub max_occurrences: u32,
    /// Incremented by forced runs too.
    pub occurrences: u32,
    /// tg `/datum/round_event::fakeable`: false alarms may announce it.
    pub fakeable: bool,
    pub admin_disabled: bool,
    /// tg `new typepath()` (with `New()`'s own rolls).
    pub make: fn(&mut World) -> Box<dyn RoundEvent>,
}

impl EventControl {
    /// tg `can_spawn_event(players_amt)` (no wizard mode, holidays or ghost roles here).
    pub fn can_spawn(&self, players: u32, round_secs: f64, evacuated: bool) -> bool {
        self.occurrences < self.max_occurrences
            && self.earliest_start < round_secs
            && players >= self.min_players
            && !evacuated
            && !self.admin_disabled
    }
}

struct Running {
    state: EventState,
    event: Box<dyn RoundEvent>,
}

/// tg `SSevents`.
#[derive(Resource, Default)]
pub struct Events {
    pub controls: Vec<EventControl>,
    /// Server time (s) of the next random pick (tg `scheduled`).
    pub scheduled: f64,
    /// Picked events in the admin intervention window: control and when it ends.
    pub pending: Vec<(usize, f64)>,
    running: Vec<Running>,
    clock: f64,
    forced_done: bool,
}

impl Events {
    pub fn index(&self, name: &str) -> Option<usize> {
        self.controls.iter().position(|c| c.name == name)
    }

    /// Names of the running events, oldest first.
    pub fn running(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.running
            .iter()
            .map(|r| self.controls[r.state.control].name)
    }

    /// tg `reschedule()`: `rand(frequency_lower, max(frequency_lower, frequency_upper))`.
    pub fn reschedule(&mut self, now: f64, config: &EventsConfig, rng: &mut Rng) {
        let upper = config.frequency_upper.max(config.frequency_lower);
        self.scheduled = now + rng.range(config.frequency_lower, upper) as f64 / 10.0;
    }

    /// The weighted roster of `spawnEvent` (weights >= 0; negative weights are round-start
    /// holiday events, which are not ported).
    pub fn roster(
        &self,
        players: u32,
        round_secs: f64,
        evacuated: bool,
        excluded: Option<usize>,
    ) -> Vec<(usize, i64)> {
        self.controls
            .iter()
            .enumerate()
            .filter(|&(i, c)| {
                Some(i) != excluded
                    && c.weight >= 0
                    && self.can_spawn(i, players, round_secs, evacuated)
            })
            .map(|(i, c)| (i, c.weight))
            .collect()
    }

    /// `can_spawn_event` with the False Alarm's override: it needs something to fake.
    pub fn can_spawn(&self, i: usize, players: u32, round_secs: f64, evacuated: bool) -> bool {
        let control = &self.controls[i];
        control.can_spawn(players, round_secs, evacuated)
            && (control.name != super::false_alarm::NAME || !self.false_alarm_pool().is_empty())
    }

    /// tg `get_potential_false_alarm()` (dynamic midround rulesets are not ported).
    pub fn false_alarm_pool(&self) -> Vec<usize> {
        self.controls
            .iter()
            .enumerate()
            .filter(|(_, c)| c.fakeable && c.name != super::false_alarm::NAME)
            .map(|(i, _)| i)
            .collect()
    }
}

/// tg `get_active_player_count(alive_check = TRUE, afk_check = TRUE, human_check = TRUE)`.
// shortcut: no AFK tracking: every connected living body counts; add with an inactivity timer.
pub fn active_players(world: &mut World) -> u32 {
    let mut q = world.query_filtered::<&Stat, (With<PlayerMob>, With<Health>, Without<Ghost>)>();
    q.iter(world).filter(|s| **s != Stat::Dead).count() as u32
}

/// The station's z-levels (tg `ZTRAIT_STATION`): every level of the maps loaded at startup
/// (a multi-deck map has several), before shuttle templates.
pub fn station_levels(world: &World) -> std::ops::RangeInclusive<u16> {
    let maps = world
        .get_resource::<MapLoadPlugin>()
        .map_or(0, |m| m.maps.len());
    let levels: u32 = world
        .get_resource::<ss13_mapload::MapLoadReport>()
        .map_or(maps as u32, |r| {
            r.maps.iter().take(maps).map(|m| m.size.2).sum()
        });
    1..=levels as u16
}

pub(super) fn init_events(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let mut rng = Rng::new(seed ^ 0x4556_454E_5453);
    let config = world.resource::<EventsConfig>().clone();
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut events = Events {
        controls: super::controls(),
        ..default()
    };
    events.reschedule(now, &config, &mut rng);
    world.insert_resource(EventRng(rng));
    world.insert_resource(events);
}

fn round_secs(world: &World, now: f64) -> f64 {
    world
        .get_resource::<Ticker>()
        .map_or(0.0, |t| t.round_duration(now))
}

/// tg `SSevents.fire()` (`runlevels = RUNLEVEL_GAME`).
pub(super) fn events_fire(world: &mut World) {
    let in_game = world
        .get_resource::<Ticker>()
        .is_some_and(|t| t.state() == GameState::Playing);
    let dt = world.resource::<Time>().delta_secs_f64();
    let mut fires = 0;
    {
        let mut events = world.resource_mut::<Events>();
        if !in_game {
            events.clock = 0.0;
            return;
        }
        events.clock += dt;
        while events.clock >= EVENTS_WAIT_SECS {
            events.clock -= EVENTS_WAIT_SECS;
            fires += 1;
        }
    }
    force_configured(world);
    for _ in 0..fires.min(MAX_CATCH_UP) {
        fire(world);
    }
}

fn force_configured(world: &mut World) {
    if std::mem::replace(&mut world.resource_mut::<Events>().forced_done, true) {
        return;
    }
    for name in world.resource::<EventsConfig>().forced.clone() {
        if let Err(e) = force_event(world, &name, None) {
            warn!("SS13_FORCE_EVENT: {e}");
        }
    }
}

/// One fire: `checkEvent()`, the admin windows that ended, then every running event.
pub fn fire(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.resource::<Events>().scheduled <= now {
        if world.resource::<EventsConfig>().allow_random_events {
            spawn_event(world, None);
        }
        let config = world.resource::<EventsConfig>().clone();
        world.resource_scope(|world, mut events: Mut<Events>| {
            events.reschedule(now, &config, &mut world.resource_mut::<EventRng>().0);
        });
    }
    let due: Vec<usize> = {
        let mut events = world.resource_mut::<Events>();
        let (due, waiting) = events.pending.iter().partition(|&&(_, at)| at <= now);
        events.pending = waiting;
        due.into_iter().map(|(i, _)| i).collect()
    };
    for control in due {
        pre_run_done(world, control);
    }
    let mut running = std::mem::take(&mut world.resource_mut::<Events>().running);
    for run in &mut running {
        process(world, run);
    }
    running.retain(|r| !r.state.killed);
    // Events started while processing (none of the ported ones do) go after these.
    let mut events = world.resource_mut::<Events>();
    running.append(&mut events.running);
    events.running = running;
}

/// tg `spawnEvent(excluded_event)`: picks by weight and starts the admin window.
pub fn spawn_event(world: &mut World, excluded: Option<usize>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let players = active_players(world);
    let round = round_secs(world, now);
    let evacuated = crate::shuttle::evacuated(world);
    let roster = world
        .resource::<Events>()
        .roster(players, round, evacuated, excluded);
    let picked = pick_weight(&roster, &mut world.resource_mut::<EventRng>().0);
    if let Some(control) = picked {
        let mut events = world.resource_mut::<Events>();
        info!(
            "random event triggering in {ADMIN_INTERVENTION_SECS} s: {}",
            events.controls[control].name
        );
        events
            .pending
            .push((control, now + ADMIN_INTERVENTION_SECS));
    }
}

/// The end of `preRunEvent()`'s sleep: still allowed, it runs; otherwise another is rolled.
// shortcut: no admins to cancel or reroll in the window; the window and its re-check stay.
fn pre_run_done(world: &mut World, control: usize) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let players = active_players(world);
    let round = round_secs(world, now);
    let evacuated = crate::shuttle::evacuated(world);
    if !world
        .resource::<Events>()
        .can_spawn(control, players, round, evacuated)
    {
        spawn_event(world, Some(control));
        return;
    }
    run_event(world, control, true, None);
}

/// tg `run_event(random, announce_chance_override)`: a new event, `setup()`, counted, queued.
pub fn run_event(world: &mut World, control: usize, random: bool, announce: Option<bool>) {
    let make = world.resource::<Events>().controls[control].make;
    let mut event = make(world);
    let mut state = EventState::new(control);
    event.setup(world, &mut state);
    state.current_players = active_players(world);
    if let Some(announce) = announce {
        state.announce_chance = if announce { 100.0 } else { 0.0 };
    }
    let mut events = world.resource_mut::<Events>();
    events.controls[control].occurrences += 1;
    info!(
        "{} event triggering: {}",
        if random { "Random" } else { "Forced" },
        events.controls[control].name
    );
    if !state.killed {
        events.running.push(Running { state, event });
    }
}

/// The admin Force Event: runs `name` now, with its own announcement chance or `announce`.
// Reached through the admin panel (`ss13_admin`), the dev test hook (`ss13_debug`) and
// `SS13_FORCE_EVENT`.
pub fn force_event(world: &mut World, name: &str, announce: Option<bool>) -> Result<(), String> {
    let control = world
        .get_resource::<Events>()
        .and_then(|e| e.index(name))
        .ok_or_else(|| format!("no event named {name:?}"))?;
    run_event(world, control, false, announce);
    Ok(())
}

/// tg `/datum/round_event/process()`.
fn process(world: &mut World, run: &mut Running) {
    let ev = &mut run.state;
    if ev.killed {
        return;
    }
    if ev.active_for == ev.start_when {
        run.event.start(world, ev);
    }
    if ev.active_for == ev.announce_when
        && world.resource_mut::<EventRng>().0.prob(ev.announce_chance)
    {
        run.event.announce(world, ev, false);
    }
    if ev.start_when < ev.active_for && ev.active_for < ev.end_when && !ev.killed {
        run.event.tick(world, ev);
    }
    if ev.active_for == ev.end_when && !ev.killed {
        run.event.end(world, ev);
    }
    if ev.active_for >= ev.end_when
        && ev.active_for >= ev.announce_when
        && ev.active_for >= ev.start_when
    {
        ev.killed = true;
    }
    ev.active_for += 1;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn noop(_: &mut World) -> Box<dyn RoundEvent> {
        struct Noop;
        impl RoundEvent for Noop {}
        Box::new(Noop)
    }

    fn control(name: &'static str, weight: i64) -> EventControl {
        EventControl {
            name,
            weight,
            // No ticker in these worlds: the round time is 0.
            earliest_start: -1.0,
            min_players: 0,
            max_occurrences: 20,
            occurrences: 0,
            fakeable: true,
            admin_disabled: false,
            make: noop,
        }
    }

    fn events(controls: Vec<EventControl>) -> Events {
        Events {
            controls,
            ..default()
        }
    }

    #[test]
    fn pick_weight_follows_weights() {
        let ev = events(vec![
            control("a", 200),
            control("b", 30),
            control("c", 10),
            control("never", 0),
        ]);
        let roster = ev.roster(0, 10.0, false, None);
        let mut rng = Rng::new(7);
        let mut counts = [0u32; 4];
        let n = 24_000;
        for _ in 0..n {
            counts[pick_weight(&roster, &mut rng).unwrap()] += 1;
        }
        let share = |i: usize| f64::from(counts[i]) / f64::from(n);
        assert!((share(0) - 200.0 / 240.0).abs() < 0.01, "{counts:?}");
        assert!((share(1) - 30.0 / 240.0).abs() < 0.01, "{counts:?}");
        assert!((share(2) - 10.0 / 240.0).abs() < 0.01, "{counts:?}");
        assert_eq!(counts[3], 0, "weight 0 is never picked");
        assert_eq!(pick_weight::<usize>(&[], &mut rng), None);
    }

    #[test]
    fn gates_earliest_start_players_occurrences() {
        let mut c = control("x", 10);
        c.earliest_start = 1200.0;
        c.min_players = 5;
        c.max_occurrences = 2;
        assert!(
            !c.can_spawn(5, 1200.0, false),
            "earliest_start is exclusive"
        );
        assert!(c.can_spawn(5, 1200.1, false));
        assert!(!c.can_spawn(4, 2000.0, false), "min_players");
        assert!(
            !c.can_spawn(5, 2000.0, true),
            "not after the shuttle escaped"
        );
        c.occurrences = 2;
        assert!(!c.can_spawn(50, 2000.0, false), "max_occurrences");
        c.occurrences = 1;
        c.admin_disabled = true;
        assert!(!c.can_spawn(50, 2000.0, false));

        let mut ev = events(vec![control("a", 10), control("b", 10)]);
        ev.controls[1].max_occurrences = 0;
        assert_eq!(ev.roster(0, 1.0, false, None), vec![(0, 10)]);
        assert_eq!(ev.roster(0, 1.0, false, Some(0)), vec![]);
    }

    #[test]
    fn false_alarm_needs_something_to_fake() {
        let mut ev = events(vec![control(crate::events::false_alarm::NAME, 20)]);
        assert!(!ev.can_spawn(0, 0, 10.0, false));
        ev.controls.push(control("real", 10));
        assert!(ev.can_spawn(0, 0, 10.0, false));
        ev.controls[1].fakeable = false;
        assert!(!ev.can_spawn(0, 0, 10.0, false));
    }

    #[test]
    fn reschedule_uses_config_frequency() {
        let mut ev = events(vec![]);
        let mut rng = Rng::new(3);
        let config = EventsConfig::tg();
        let (mut lo, mut hi) = (f64::MAX, 0.0f64);
        for _ in 0..5000 {
            ev.reschedule(100.0, &config, &mut rng);
            let wait = ev.scheduled - 100.0;
            assert!((150.0..=420.0).contains(&wait), "{wait}");
            assert!(
                ((wait * 10.0).round() - wait * 10.0).abs() < 1e-6,
                "whole deciseconds"
            );
            lo = lo.min(wait);
            hi = hi.max(wait);
        }
        assert!(lo < 151.0 && hi > 419.0, "{lo}..{hi}");
        let narrow = EventsConfig {
            frequency_upper: 100,
            ..EventsConfig::tg()
        };
        ev.reschedule(0.0, &narrow, &mut rng);
        assert_eq!(ev.scheduled, 150.0, "upper below lower: lower");
    }

    type Log = Arc<Mutex<Vec<(&'static str, u32)>>>;

    struct Recorder(Log);

    impl RoundEvent for Recorder {
        fn setup(&mut self, _: &mut World, ev: &mut EventState) {
            ev.start_when = 2;
            ev.announce_when = 1;
            ev.end_when = 4;
        }
        fn announce(&mut self, _: &mut World, ev: &mut EventState, _: bool) {
            self.0.lock().unwrap().push(("announce", ev.active_for));
        }
        fn start(&mut self, _: &mut World, ev: &mut EventState) {
            self.0.lock().unwrap().push(("start", ev.active_for));
        }
        fn tick(&mut self, _: &mut World, ev: &mut EventState) {
            self.0.lock().unwrap().push(("tick", ev.active_for));
        }
        fn end(&mut self, _: &mut World, ev: &mut EventState) {
            self.0.lock().unwrap().push(("end", ev.active_for));
        }
    }

    static LOG: Mutex<Option<Log>> = Mutex::new(None);

    fn recorder(_: &mut World) -> Box<dyn RoundEvent> {
        Box::new(Recorder(LOG.lock().unwrap().clone().unwrap()))
    }

    fn world(controls: Vec<EventControl>) -> World {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(EventsConfig::tg());
        world.insert_resource(EventRng(Rng::new(1)));
        world.insert_resource(events(controls));
        world
    }

    #[test]
    fn lifecycle_follows_round_event_process() {
        let log: Log = Arc::default();
        *LOG.lock().unwrap() = Some(log.clone());
        let mut c = control("rec", 10);
        c.make = recorder;
        let mut world = world(vec![c]);
        world.resource_mut::<Events>().scheduled = f64::MAX;
        force_event(&mut world, "rec", None).unwrap();
        assert_eq!(world.resource::<Events>().controls[0].occurrences, 1);
        for _ in 0..6 {
            fire(&mut world);
        }
        assert_eq!(
            *log.lock().unwrap(),
            vec![("announce", 1), ("start", 2), ("tick", 3), ("end", 4)]
        );
        assert_eq!(world.resource::<Events>().running().count(), 0);
        assert!(force_event(&mut world, "nope", None).is_err());
    }

    #[test]
    fn random_pick_waits_for_the_admin_window_and_rerolls() {
        let mut world = world(vec![control("a", 10), control("b", 10)]);
        world.resource_mut::<Events>().scheduled = 0.0;
        fire(&mut world);
        let ev = world.resource::<Events>();
        assert_eq!(ev.pending.len(), 1, "picked, waiting");
        assert!(ev.running().next().is_none());
        assert!(ev.scheduled >= 150.0, "rescheduled");
        let picked = ev.pending[0].0;
        // The pick can no longer spawn when the window ends: the other one runs instead.
        world.resource_mut::<Events>().controls[picked].max_occurrences = 0;
        world.resource_mut::<Events>().pending[0].1 = 0.0;
        fire(&mut world);
        let ev = world.resource::<Events>();
        assert_eq!(ev.pending.len(), 1);
        assert_eq!(ev.pending[0].0, 1 - picked, "rerolled without it");
        world.resource_mut::<Events>().pending[0].1 = 0.0;
        fire(&mut world);
        let ev = world.resource::<Events>();
        assert_eq!(ev.controls[1 - picked].occurrences, 1);
        assert!(ev.pending.is_empty());
    }

    #[test]
    fn disabled_random_events_still_reschedule() {
        let mut world = world(vec![control("a", 10)]);
        world.resource_mut::<EventsConfig>().allow_random_events = false;
        world.resource_mut::<Events>().scheduled = 0.0;
        fire(&mut world);
        let ev = world.resource::<Events>();
        assert!(ev.pending.is_empty());
        assert!(ev.scheduled >= 150.0);
    }
}
