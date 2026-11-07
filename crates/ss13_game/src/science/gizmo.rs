//! Gizmos (server), after tg `code/modules/research/gizmo/`: random puzzle devices, the machine
//! (`/obj/machinery/gizmo` and its toggle, cursed, voice and beyblade kinds) and the handheld
//! `/obj/item/gizmo`.
//!
//! Each gizmo gets a random name (BandaStation `possible_names`), eight cryptic wires
//! ([`MachineWires::Gizmo`], tg `/datum/wires/gizmo`) and, as `generate_interfaces` builds it,
//! a few gizmodes: each holds gizpulses (effects) and one mode control (`select_mode`,
//! `cycle_mode`, `direct_activate`, `cycle_mode/activate`) that adds its solution callbacks.
//! The puzzle (`/datum/gizmo_puzzle`) gives every callback a random sequence of three wire
//! pulses; pulsing a wire extends the current sequence, a full match runs the callback, a dead
//! end resets it, and the gizmo answers "бзз...", "динь...", "крррхх..." or "clunk" (0.2 s
//! apart). A gizmode's effects share its cooldown (`cooldown_time`).
//!
//! Gizmodes: teleporter (self, others in view 2, both: 5-15 tiles in one direction), mood
//! pulser (±3 mood for 30 s to humans in 14 tiles, a radiation pulse), food dispenser, bad
//! (ominous hum, explosion, fire explosion, radiation pulse; 5 s cooldown), lights (the toggle
//! gizmo: on, off).
// shortcut: of `GIZMO_COMMON_MODES` the mopper, sputter, copier, electric and code-crack
// gizmodes are not ported and not rolled; the bad gizmode has no robot spiders, throwers or
// bone breaker; the voice gizmo has no voice interface (it is a plain wired gizmo), the beyblade
// does not move (no `moving_randomly`), the lights gizmode does not light (no `set_light`, only
// the toggle's state and messages); no sounds, ring effects or icon states. Food comes with its
// prototype's reagents (tg clears them to goop).

use bevy::prelude::*;
use ss13_mapload::{DmName, DmObj};
use ss13_shared::TilePos;

use crate::combat::CombatRng;
use crate::machine_wires::MachineWires;
use crate::obj_damage::is_under;
use crate::wires::{WireHolder, Wires};

pub const GIZMO_MACHINE_ROOT: &str = "/obj/machinery/gizmo";
pub const GIZMO_ITEM_ROOT: &str = "/obj/item/gizmo";
const TOGGLE: &str = "/obj/machinery/gizmo/toggle";
const CURSED: &str = "/obj/machinery/gizmo/cursed";
const BEYBLADE: &str = "/obj/machinery/gizmo/beyblade";
/// `CRYPTIC_WIRE_1..8`.
pub const CRYPTIC_WIRES: [&str; 8] = [
    "Cryptic Wire 1",
    "Cryptic Wire 2",
    "Cryptic Wire 3",
    "Cryptic Wire 4",
    "Cryptic Wire 5",
    "Cryptic Wire 6",
    "Cryptic Wire 7",
    "Cryptic Wire 8",
];
/// `code_length`.
const CODE_LENGTH: usize = 3;
/// `feedback_cooldown_time`, seconds.
const FEEDBACK_COOLDOWN: f64 = 0.2;
/// BandaStation `possible_names`.
const NAMES: [&str; 21] = [
    "штуковина",
    "штука-дрюка",
    "хреновина",
    "полярный инвертор",
    "реверсивный рецептор",
    "флопиксель",
    "репопулятор",
    "квантовый квантовик",
    "ну этот, как его",
    "делатель всякого",
    "квазифазер",
    "выполнятель задач",
    "интерфейсный респондер",
    "кинетический обсервер",
    "турбинный инкапсулятор",
    "бипкодел",
    "агрегат",
    "приблуда",
    "прибамбас",
    "девайс",
    "плюмбус",
];
/// `/datum/gizpulse/dispense/food`'s objects (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
const FOOD: [&str; 2] = ["/obj/item/food/donut/plain", "/obj/item/food/burger/plain"];

const GIZMO_POSITIVE: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "gizmo_positive",
    description: "I hear a voice whispering kind words in my ear!",
    mood_change: 3.0,
    timeout: 30.0,
    hidden: false,
};
const GIZMO_NEGATIVE: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "gizmo_negative",
    description: "I hear a voice whispering, and I don't like what it says.",
    mood_change: -3.0,
    timeout: 30.0,
    hidden: false,
};

/// A `/datum/gizpulse`: what an activation does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pulse {
    TeleportSelf,
    TeleportOther,
    TeleportBoth,
    MoodPositive,
    MoodNegative,
    Radiation,
    Food(&'static str),
    Ominous,
    Explode,
    ExplodeFire,
    LightsOn,
    LightsOff,
    StartMoving,
    StopMoving,
}

/// The `/datum/gizpulse/mode_controle` of a gizmode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Select,
    Cycle,
    Direct,
    CycleActivate,
}

/// A `/datum/gizmodes`.
#[derive(Clone, Debug)]
pub struct Gizmode {
    pub pulses: Vec<Pulse>,
    /// `current_active`.
    pub current: usize,
    pub control: Control,
    /// `cooldown_time`, seconds, and the game time it ends.
    pub cooldown: f64,
    pub ready_at: f64,
}

/// A solution callback: which gizmode, and what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Solution {
    Select(usize, usize),
    Activate(usize),
    Cycle(usize),
    CycleActivate(usize),
    Direct(usize, usize),
}

/// `GIZMO_PUZZLE_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PuzzleResult {
    Wrong,
    Correct,
    Solved,
    SolvedModeControl,
}

/// The gizmo controller with its puzzle.
#[derive(Component, Clone, Debug, Default)]
pub struct Gizmo {
    pub modes: Vec<Gizmode>,
    /// `code_sequences` (wire indices) with their `solution_callbacks`.
    pub codes: Vec<(Vec<u8>, Solution)>,
    /// `current_sequence`.
    pub current: Vec<u8>,
    pub feedback_ready: f64,
    /// The toggle gizmo's `on_state`, the beyblade's `moving`.
    pub on: bool,
}

impl Gizmo {
    /// tg `on_pulse`'s matching: the result and the solution to run.
    pub fn on_pulse(&mut self, wire: u8) -> (PuzzleResult, Option<Solution>) {
        self.current.push(wire);
        let mut result = PuzzleResult::Correct;
        let mut solved = None;
        let mut succeeded = false;
        for (code, solution) in &self.codes {
            for j in 0..self.current.len() {
                if self.current[j] != code[j] {
                    break;
                }
                if self.current.len() == j + 1 {
                    succeeded = true;
                }
                if j + 1 == code.len() {
                    solved = Some(*solution);
                    result = PuzzleResult::Solved;
                    self.current.clear();
                    break;
                }
            }
            if solved.is_some() {
                break;
            }
        }
        if !succeeded {
            self.current.clear();
            result = PuzzleResult::Wrong;
        }
        (result, solved)
    }
}

pub struct GizmoPlugin;

impl Plugin for GizmoPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_systems(ss13_world::MapInit, setup_gizmos)
            // A gizmo spawned after map load gets its puzzle on the first wire tool.
            .add_click_handler(ClickPhase::HeldOnTarget, 277, |w, c| {
                ensure(w, c.target);
                false
            })
            .add_click_handler(ClickPhase::OtherHand, 77, |w, c| {
                ensure(w, c.target);
                false
            })
            .add_click_handler(ClickPhase::EmptyHand, 97, |w, c| {
                ensure(w, c.target);
                false
            });
    }
}

pub fn is_gizmo(path: &str) -> bool {
    is_under(path, GIZMO_MACHINE_ROOT) || is_under(path, GIZMO_ITEM_ROOT)
}

fn setup_gizmos(world: &mut World) {
    let mut query = world.query::<(Entity, &DmObj)>();
    let found: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_gizmo(&o.path))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        ensure(world, e);
    }
}

struct Picker<'a>(&'a mut CombatRng);

impl Picker<'_> {
    fn range(&mut self, low: usize, high: usize) -> usize {
        if high <= low {
            return low;
        }
        self.0.range(low as u32, high as u32) as usize
    }

    fn pick<T: Copy>(&mut self, list: &[T]) -> T {
        list[self.range(0, list.len() - 1)]
    }

    /// `pick_weight_take` from equal weights.
    fn take<T: Copy>(&mut self, list: &mut Vec<T>) -> Option<T> {
        if list.is_empty() {
            return None;
        }
        let i = self.range(0, list.len() - 1);
        Some(list.remove(i))
    }
}

/// A gizmode kind (`/datum/gizmodes` subtype).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Teleporter,
    MoodPulser,
    Food,
    Bad,
    Lights,
    Mover,
}

static ALL_CONTROLS: [Control; 4] = [
    Control::Select,
    Control::Cycle,
    Control::Direct,
    Control::CycleActivate,
];

static DIRECT_ONLY: [Control; 1] = [Control::Direct];
static LIGHTS_CONTROLS: [Control; 2] = [Control::Direct, Control::CycleActivate];

/// `/datum/gizmodes/generate_modes` of a kind: (pulses, control, cooldown).
fn generate_mode(rng: &mut Picker<'_>, kind: Kind) -> Gizmode {
    let (guaranteed, mut possible, min, max, controls, cooldown): (
        Vec<Pulse>,
        Vec<Pulse>,
        usize,
        usize,
        &'static [Control],
        f64,
    ) = match kind {
        Kind::Teleporter => (
            vec![],
            vec![
                Pulse::TeleportSelf,
                Pulse::TeleportOther,
                Pulse::TeleportBoth,
            ],
            2,
            3,
            &ALL_CONTROLS[..],
            1.0,
        ),
        Kind::MoodPulser => (
            vec![Pulse::MoodPositive, Pulse::MoodNegative],
            vec![Pulse::Radiation],
            0,
            1,
            &ALL_CONTROLS[..],
            1.0,
        ),
        Kind::Food => (
            vec![],
            vec![Pulse::Food(rng.pick(&FOOD))],
            1,
            1,
            &DIRECT_ONLY[..],
            1.0,
        ),
        Kind::Bad => (
            vec![Pulse::Ominous],
            vec![Pulse::Explode, Pulse::ExplodeFire, Pulse::Radiation],
            1,
            2,
            &ALL_CONTROLS[..],
            5.0,
        ),
        Kind::Lights => (
            vec![Pulse::LightsOn, Pulse::LightsOff],
            vec![],
            1,
            2,
            &LIGHTS_CONTROLS[..],
            1.0,
        ),
        Kind::Mover => (
            vec![Pulse::StartMoving, Pulse::StopMoving],
            vec![],
            1,
            2,
            &ALL_CONTROLS[..],
            1.0,
        ),
    };
    let mut pulses = guaranteed;
    for _ in 0..rng.range(min, max) {
        match rng.take(&mut possible) {
            Some(p) => pulses.push(p),
            None => break,
        }
    }
    let current = rng.range(0, pulses.len().saturating_sub(1));
    Gizmode {
        pulses,
        current,
        control: rng.pick(controls),
        cooldown,
        ready_at: 0.0,
    }
}

/// `setup_mode_controle`: the gizmode's solution callbacks.
fn solutions(mode: usize, g: &Gizmode, out: &mut Vec<Solution>) {
    match g.control {
        Control::Select => {
            out.extend((0..g.pulses.len()).map(|p| Solution::Select(mode, p)));
            out.push(Solution::Activate(mode));
        }
        Control::Cycle => {
            out.push(Solution::Cycle(mode));
            out.push(Solution::Activate(mode));
        }
        Control::Direct => out.extend((0..g.pulses.len()).map(|p| Solution::Direct(mode, p))),
        Control::CycleActivate => out.push(Solution::CycleActivate(mode)),
    }
}

/// `generate_interfaces` for a holder of `path`.
pub fn generate(rng: &mut CombatRng, path: &str) -> Gizmo {
    let mut rng = Picker(rng);
    // The interface: guaranteed gizmodes, then `rand(min, max)` common ones.
    let (guaranteed, mut possible, min, max) = if is_under(path, CURSED) {
        (vec![], vec![Kind::Bad], 1, 1)
    } else if is_under(path, TOGGLE) {
        (vec![Kind::Lights], common(), 1, 2)
    } else if is_under(path, BEYBLADE) {
        (vec![Kind::Mover], common(), 0, 1)
    } else {
        (vec![], common(), 1, 2)
    };
    let mut kinds = guaranteed;
    for _ in 0..rng.range(min, max) {
        match rng.take(&mut possible) {
            Some(k) => kinds.push(k),
            None => break,
        }
    }
    let modes: Vec<Gizmode> = kinds
        .into_iter()
        .map(|k| generate_mode(&mut rng, k))
        .collect();
    let mut callbacks = Vec::new();
    for (i, g) in modes.iter().enumerate() {
        solutions(i, g, &mut callbacks);
    }
    let codes = callbacks
        .into_iter()
        .map(|s| {
            let code = (0..CODE_LENGTH)
                .map(|_| rng.range(0, CRYPTIC_WIRES.len() - 1) as u8)
                .collect();
            (code, s)
        })
        .collect();
    Gizmo {
        modes,
        codes,
        ..default()
    }
}

/// `GIZMO_COMMON_MODES`, the ported ones.
fn common() -> Vec<Kind> {
    vec![Kind::MoodPulser, Kind::Teleporter, Kind::Food, Kind::Bad]
}

/// The gizmo's `Initialize`: name, controller, wires. False when `e` is no gizmo.
pub fn ensure(world: &mut World, e: Entity) -> bool {
    if world.get::<Gizmo>(e).is_some() {
        return true;
    }
    let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
        return false;
    };
    if !is_gizmo(&path) {
        return false;
    }
    let mut rng = CombatRng(world.get_resource::<CombatRng>().map_or(0, |r| r.0));
    let gizmo = generate(&mut rng, &path);
    let name = NAMES[rng.range(0, NAMES.len() as u32 - 1) as usize];
    if let Some(mut r) = world.get_resource_mut::<CombatRng>() {
        r.0 = rng.0;
    }
    let colors = {
        let set = crate::wires::WireSet {
            key: GIZMO_MACHINE_ROOT,
            proper_name: "Gizmo",
            wires: CRYPTIC_WIRES.iter().map(|w| (*w).to_owned()).collect(),
        };
        world
            .get_resource_or_init::<crate::wires::WireColors>()
            .wires_for(WireHolder::Machine(MachineWires::Gizmo), &set)
    };
    let wires: Wires = colors;
    world.entity_mut(e).insert((gizmo, wires));
    if let Some(mut n) = world.get_mut::<DmName>(e) {
        n.name = name.to_owned();
    }
    true
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// `/datum/wires/gizmo/on_pulse`: the wire into the puzzle.
pub fn pulsed(world: &mut World, e: Entity, wire: &str, user: Option<Entity>) {
    let Some(index) = CRYPTIC_WIRES.iter().position(|w| *w == wire) else {
        return;
    };
    let Some((mut result, solution)) = world
        .get_mut::<Gizmo>(e)
        .map(|mut g| g.on_pulse(index as u8))
    else {
        return;
    };
    if let Some(solution) = solution
        && let Some(r) = run_solution(world, e, solution)
    {
        result = r;
    }
    feedback(world, e, user, result);
}

/// `default_on_pulsed`.
fn feedback(world: &mut World, e: Entity, user: Option<Entity>, result: PuzzleResult) {
    let t = now(world);
    let Some(mut g) = world.get_mut::<Gizmo>(e) else {
        return;
    };
    if t < g.feedback_ready || user.is_none() {
        return;
    }
    g.feedback_ready = t + FEEDBACK_COOLDOWN;
    let text = match result {
        PuzzleResult::Wrong => "бзз...",
        PuzzleResult::Correct => "динь...",
        PuzzleResult::Solved => "крррхх...",
        PuzzleResult::SolvedModeControl => "clunk",
    };
    crate::balloon::balloon_alert_to_viewers(world, e, text, None, &[]);
}

/// A solution callback: `Some(SolvedModeControl)` for a mode selection.
fn run_solution(world: &mut World, e: Entity, solution: Solution) -> Option<PuzzleResult> {
    let mut gizmo = world.get_mut::<Gizmo>(e)?;
    match solution {
        Solution::Select(m, p) => {
            gizmo.modes.get_mut(m)?.current = p;
            Some(PuzzleResult::SolvedModeControl)
        }
        Solution::Cycle(m) => {
            let g = gizmo.modes.get_mut(m)?;
            g.current = (g.current + 1) % g.pulses.len().max(1);
            Some(PuzzleResult::SolvedModeControl)
        }
        Solution::Activate(m) => {
            drop(gizmo);
            activate(world, e, m);
            None
        }
        Solution::Direct(m, p) => {
            gizmo.modes.get_mut(m)?.current = p;
            drop(gizmo);
            activate(world, e, m);
            None
        }
        Solution::CycleActivate(m) => {
            let g = gizmo.modes.get_mut(m)?;
            g.current = (g.current + 1) % g.pulses.len().max(1);
            drop(gizmo);
            activate(world, e, m);
            None
        }
    }
}

/// `/datum/gizmodes/activate`: the current gizpulse behind the gizmode's cooldown.
pub fn activate(world: &mut World, e: Entity, mode: usize) -> bool {
    let t = now(world);
    let Some(pulse) = world.get_mut::<Gizmo>(e).and_then(|mut g| {
        let m = g.modes.get_mut(mode)?;
        if t < m.ready_at {
            return None;
        }
        m.ready_at = t + m.cooldown;
        m.pulses.get(m.current).copied()
    }) else {
        return false;
    };
    run_pulse(world, e, pulse);
    true
}

fn holder_pos(world: &World, e: Entity) -> Option<TilePos> {
    crate::explosions::turf_of(world, e)
}

/// Living mobs in `range` of `pos` (`view`), `humans` only with a mood.
fn living_near(world: &World, pos: TilePos, range: i32) -> Vec<Entity> {
    let index = world.resource::<ss13_world::SpatialIndex>();
    let mut out = Vec::new();
    for dy in -range..=range {
        for dx in -range..=range {
            let p = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            out.extend(
                index
                    .on_tile(p)
                    .iter()
                    .copied()
                    .filter(|&m| world.get::<crate::health::Health>(m).is_some()),
            );
        }
    }
    out
}

/// A gizpulse's `activate`.
pub fn run_pulse(world: &mut World, e: Entity, pulse: Pulse) {
    let Some(pos) = holder_pos(world, e) else {
        return;
    };
    match pulse {
        Pulse::TeleportSelf | Pulse::TeleportOther | Pulse::TeleportBoth => {
            let mut targets = Vec::new();
            if pulse != Pulse::TeleportSelf {
                targets.extend(living_near(world, pos, 2));
            }
            if pulse != Pulse::TeleportOther {
                // The holder, or whoever carries the handheld gizmo.
                let mover = world
                    .get::<crate::inventory::CarriedBy>(e)
                    .map_or(e, |c| c.0);
                if !targets.contains(&mover) {
                    targets.push(mover);
                }
            }
            let mut rng = CombatRng(world.get_resource::<CombatRng>().map_or(0, |r| r.0));
            let range = rng.range(5, 15) as i32;
            let (dx, dy) = loop {
                let d = (rng.range(0, 2) as i32 - 1, rng.range(0, 2) as i32 - 1);
                if d != (0, 0) {
                    break d;
                }
            };
            if let Some(mut r) = world.get_resource_mut::<CombatRng>() {
                r.0 = rng.0;
            }
            for target in targets {
                let Some(from) = holder_pos(world, target) else {
                    continue;
                };
                // `get_ranged_target_turf`: as far as the map goes.
                let mut dest = from;
                for _ in 0..range {
                    let next = TilePos::new(dest.x + dx, dest.y + dy, dest.z);
                    if !world.resource::<ss13_world::SpatialIndex>().in_bounds(next) {
                        break;
                    }
                    dest = next;
                }
                crate::teleport::do_teleport(
                    world,
                    target,
                    dest,
                    crate::teleport::Tele {
                        sound_in: Some("sound/effects/cartoon_sfx/cartoon_pop.ogg"),
                        ..default()
                    },
                );
            }
        }
        Pulse::MoodPositive | Pulse::MoodNegative => {
            let event = if pulse == Pulse::MoodPositive {
                GIZMO_POSITIVE
            } else {
                GIZMO_NEGATIVE
            };
            for mob in living_near(world, pos, 14) {
                if world.get::<crate::mood::Mood>(mob).is_some() {
                    crate::mood::add_event(world, mob, "gizmo_mood_pulse", &event);
                }
            }
        }
        Pulse::Radiation => {
            crate::radiation::radiation_pulse(
                world,
                e,
                5.0,
                crate::radiation::RAD_LIGHT_INSULATION,
                30.0,
                0.0,
            );
        }
        Pulse::Food(path) => {
            crate::construction::spawn_obj(world, path, pos);
        }
        Pulse::Ominous => {
            crate::machine_wires::announce(world, e, "You hear an ominous hum.");
        }
        Pulse::Explode | Pulse::ExplodeFire => {
            let ex = crate::explosions::Explosion::new(0, 1, 3);
            let ex = if pulse == Pulse::ExplodeFire {
                ex.flame(5)
            } else {
                ex.flame(0)
            };
            crate::explosions::explosion(world, pos, ex);
        }
        Pulse::LightsOn | Pulse::LightsOff | Pulse::StartMoving | Pulse::StopMoving => {
            let on = matches!(pulse, Pulse::LightsOn | Pulse::StartMoving);
            if let Some(mut g) = world.get_mut::<Gizmo>(e) {
                g.on = on;
            }
            if matches!(pulse, Pulse::LightsOn | Pulse::LightsOff) {
                let name = world
                    .get::<DmName>(e)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                let text = if on {
                    format!("{name} hums to life.")
                } else {
                    format!("{name} powers down.")
                };
                crate::machine_wires::announce(world, e, &text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn puzzle() -> Gizmo {
        Gizmo {
            modes: vec![Gizmode {
                pulses: vec![Pulse::Ominous, Pulse::Explode],
                current: 0,
                control: Control::Cycle,
                cooldown: 5.0,
                ready_at: 0.0,
            }],
            codes: vec![
                (vec![1, 2, 3], Solution::Cycle(0)),
                (vec![1, 4, 4], Solution::Activate(0)),
            ],
            ..default()
        }
    }

    #[test]
    fn sequences_ping_solve_and_reset() {
        let mut g = puzzle();
        assert_eq!(g.on_pulse(1), (PuzzleResult::Correct, None));
        assert_eq!(g.on_pulse(4), (PuzzleResult::Correct, None));
        assert_eq!(
            g.on_pulse(4),
            (PuzzleResult::Solved, Some(Solution::Activate(0)))
        );
        assert!(g.current.is_empty());
        // A dead end resets.
        assert_eq!(g.on_pulse(1), (PuzzleResult::Correct, None));
        assert_eq!(g.on_pulse(7), (PuzzleResult::Wrong, None));
        assert!(g.current.is_empty());
        assert_eq!(g.on_pulse(1).0, PuzzleResult::Correct);
        assert_eq!(g.on_pulse(2).0, PuzzleResult::Correct);
        assert_eq!(g.on_pulse(3).1, Some(Solution::Cycle(0)));
    }

    #[test]
    fn generation_follows_the_controls() {
        let mut rng = CombatRng(7);
        for seed in 0..50u64 {
            rng.0 = seed;
            for path in [GIZMO_MACHINE_ROOT, TOGGLE, CURSED, GIZMO_ITEM_ROOT] {
                let g = generate(&mut rng, path);
                assert!(!g.modes.is_empty(), "{path}");
                for (code, _) in &g.codes {
                    assert_eq!(code.len(), CODE_LENGTH);
                    assert!(code.iter().all(|&w| (w as usize) < CRYPTIC_WIRES.len()));
                }
                if path == CURSED {
                    assert!(g.modes[0].pulses.contains(&Pulse::Ominous));
                }
                if path == TOGGLE {
                    assert!(g.modes[0].pulses.contains(&Pulse::LightsOn));
                    assert!(matches!(
                        g.modes[0].control,
                        Control::Direct | Control::CycleActivate
                    ));
                }
                let mut expected = Vec::new();
                for (i, m) in g.modes.iter().enumerate() {
                    solutions(i, m, &mut expected);
                }
                assert_eq!(
                    g.codes.iter().map(|(_, s)| *s).collect::<Vec<_>>(),
                    expected
                );
            }
        }
    }

    #[test]
    fn cooldown_gates_activation() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        let world = app.world_mut();
        let e = world.spawn(puzzle()).id();
        // Ominous needs a position: none here, so only the cooldown is observed.
        assert!(activate(world, e, 0));
        assert!(!activate(world, e, 0), "5 s cooldown");
    }
}
