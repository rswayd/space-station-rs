//! The fishing minigame's simulation (tg `/datum/fishing_challenge` `move_bait` and
//! `/datum/fish_movement` `move_fish`, `code/modules/fishing/fishing_minigame.dm`,
//! `fish_movement.dm`), shared: the server sets it up ([`setup`]) and bounds how fast a win can
//! come ([`min_win_secs`]); the client runs it at `SSfishing`'s rate ([`STEP`]) while the
//! player holds the reel.
// shortcut: the client simulates and reports the outcome (tg's old TGUI minigame did the same);
// the server only refuses wins faster than the completion gain allows. Move the simulation
// server side, with reel input as acts, if wins get forged within that bound.

use std::f32::consts::FRAC_PI_2;

use serde::{Deserialize, Serialize};

/// `FISHING_MINIGAME_AREA`.
pub const AREA: f32 = 1000.0;
/// `SSfishing` `wait`.
pub const STEP: f32 = 0.05;
/// Initial `bait_height` and `fish_height`.
pub const BAIT_HEIGHT: f32 = 360.0;
pub const FISH_HEIGHT: f32 = 50.0;
/// `BAIT_MIN_VELOCITY_BOUNCE`.
const BAIT_MIN_VELOCITY_BOUNCE: f32 = 150.0;
/// `MAX_FISH_COMPLETION_MALUS`, `BAIT_HEIGHT_DIFFICULTY_MALUS`.
const MAX_FISH_COMPLETION_MALUS: f32 = 15.0;
const BAIT_HEIGHT_DIFFICULTY_MALUS: f32 = 1.3;
/// `BITING_TIME_WINDOW`.
pub const BITING_SECS: f64 = 4.0;
const FISH_TARGET_MIN_DISTANCE: f32 = 6.0;
const FISH_FRICTION_MULT: f32 = 0.9;
const FISH_SHORT_JUMP_MIN_DISTANCE: f32 = 100.0;
const FISH_SHORT_JUMP_MAX_DISTANCE: f32 = 200.0;
/// `/datum/skill/fishing` `SKILL_VALUE_MODIFIER` at `SKILL_LEVEL_NONE` (nobody has the skill
/// here).
pub const NO_SKILL_MODIFIER: f32 = 1.0;

/// The `/datum/fish_movement` subtype.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Movement {
    #[default]
    Default,
    Slow,
    Zippy,
    Accelerando,
    Choppy,
    Plunger,
}

impl Movement {
    pub fn of_path(path: &str) -> Self {
        match path.strip_prefix("/datum/fish_movement/") {
            Some("slow") => Self::Slow,
            Some("zippy") => Self::Zippy,
            Some("accelerando") => Self::Accelerando,
            Some("choppy") => Self::Choppy,
            Some("plunger") => Self::Plunger,
            _ => Self::Default,
        }
    }
}

/// Everything the client needs to run one challenge (the window's static data).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MinigameParams {
    pub difficulty: f32,
    /// Starting `completion` (0-100).
    pub completion: f32,
    pub completion_gain: f32,
    pub completion_loss: f32,
    pub gravity_velocity: f32,
    pub reeling_velocity: f32,
    pub bait_bounce_mult: f32,
    pub deceleration_mult: f32,
    pub overlap_velocity_mult: f32,
    pub bait_height: f32,
    pub fish_height: f32,
    pub fish_position: f32,
    pub bait_position: f32,
    pub movement: Movement,
    pub fish_idle_velocity: f32,
    /// `FISHING_MINIGAME_RULE_ANTIGRAV` / `_CAMO` (fish traits).
    pub antigrav: bool,
    pub camo: bool,
    pub seed: u64,
    /// The source's `background`.
    pub background: String,
}

/// What [`setup`] reads from the rod, the line and the reward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setup {
    pub difficulty: f32,
    pub has_line: bool,
    pub completion_speed_mult: f32,
    pub bait_speed_mult: f32,
    pub deceleration_mult: f32,
    pub bounciness_mult: f32,
    pub gravity_mult: f32,
    pub bait_height_mult: f32,
    /// Seconds left of the biting window when the player struck; `None`: automatic.
    pub reaction_left: Option<f64>,
    pub movement: Movement,
    /// Fish traits with a `minigame_mod`.
    pub heavy: bool,
    pub lubed: bool,
    pub antigrav: bool,
    pub camo: bool,
}

/// `New` + `start_minigame_phase` + `adjust_to_difficulty`. `rand(low, high)` is DM's.
pub fn setup(s: &Setup, mut rand: impl FnMut(i32, i32) -> i32) -> MinigameParams {
    let mut completion_gain = 5.0;
    let mut completion_loss = 6.0;
    if s.has_line {
        completion_gain += 1.0;
    }
    completion_loss += NO_SKILL_MODIFIER / 5.0;
    completion_gain -= NO_SKILL_MODIFIER / 7.5;
    let mut reeling_velocity = 1200.0 * s.bait_speed_mult;
    completion_gain *= s.completion_speed_mult;
    let bait_bounce_mult = 0.6 * s.bounciness_mult;
    let deceleration_mult = 1.8 * s.deceleration_mult;
    let mut gravity_velocity = -800.0 * s.gravity_mult;
    let overlap_velocity_mult = 0.6f32.powf(s.bait_speed_mult);

    let mut completion = 30.0;
    if s.difficulty > 15.0 {
        completion -= MAX_FISH_COMPLETION_MALUS * (s.difficulty * 0.01);
    }
    let mut fish_idle_velocity = 0.0;
    if s.heavy {
        fish_idle_velocity -= 10.0;
    }
    if s.lubed {
        reeling_velocity *= 1.4;
        gravity_velocity *= 1.4;
    }
    match s.reaction_left {
        None => completion *= 1.3,
        Some(left) => {
            let ds = left * 10.0;
            let window = BITING_SECS * 10.0;
            if (0.0..=window - 30.0).contains(&ds) {
                completion *= 0.65;
            } else if (window - 30.0..=window - 20.0).contains(&ds) {
                completion *= 0.82;
            } else if (window - 10.0..=window - 5.0).contains(&ds) {
                completion *= 1.2;
            } else if (window - 5.0..=window).contains(&ds) {
                completion *= 1.4;
            }
        }
    }
    // tg applies the rod's `bait_height_mult` only on later difficulty updates; here from the
    // start, as intended.
    let mut bait_height = BAIT_HEIGHT * s.bait_height_mult;
    let fish_position = rand(0, ((AREA - FISH_HEIGHT) * 0.8) as i32) as f32;
    let diff_dist = (100.0 + s.difficulty) as i32;
    let bait_position = (fish_position + rand(-diff_dist, diff_dist) as f32 - bait_height * 0.5)
        .round()
        .clamp(0.0, (AREA - bait_height).max(0.0));
    bait_height -= (s.difficulty * BAIT_HEIGHT_DIFFICULTY_MALUS).round();
    let seed = (rand(0, i32::MAX) as u64) << 31 | rand(0, i32::MAX) as u64;
    MinigameParams {
        difficulty: s.difficulty,
        completion,
        completion_gain,
        completion_loss,
        gravity_velocity,
        reeling_velocity,
        bait_bounce_mult,
        deceleration_mult,
        overlap_velocity_mult,
        bait_height,
        fish_height: FISH_HEIGHT,
        fish_position,
        bait_position,
        movement: s.movement,
        fish_idle_velocity,
        antigrav: s.antigrav,
        camo: s.camo,
        seed,
        background: String::new(),
    }
}

/// The fastest a challenge can be won: the bait on the fish from the first tick on.
pub fn min_win_secs(p: &MinigameParams) -> f64 {
    if p.completion_gain <= 0.0 {
        return f64::INFINITY;
    }
    f64::from((100.0 - p.completion).max(0.0) / p.completion_gain)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Running,
    Won,
    Lost,
}

/// An active effect of the fish (`select_active_effect`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Antigrav,
    Camo,
}

/// SplitMix64, seeded by the server.
#[derive(Clone, Debug)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// DM `rand(low, high)` on integers.
    fn range(&mut self, low: i32, high: i32) -> i32 {
        if high <= low {
            return low;
        }
        low + (self.next() % (high - low + 1) as u64) as i32
    }

    /// DM `prob(percent)`.
    fn prob(&mut self, percent: f32) -> bool {
        ((self.next() >> 40) as f32 / (1u64 << 24) as f32) * 100.0 < percent
    }
}

/// One running challenge.
#[derive(Clone, Debug)]
pub struct Sim {
    pub p: MinigameParams,
    pub fish_position: f32,
    pub bait_position: f32,
    pub bait_velocity: f32,
    pub completion: f32,
    pub effect: Option<Effect>,
    effect_cd: f32,
    elapsed: f32,
    rng: Rng,
    // The fish movement.
    times_fired: u32,
    short_jump_chance: f32,
    long_jump_chance: f32,
    short_jump_velocity_limit: f32,
    long_jump_velocity_limit: f32,
    current_velocity_limit: f32,
    target_position: Option<f32>,
    can_interrupt_move: bool,
    fish_velocity: f32,
    fish_idle_velocity: f32,
    plunging: bool,
    plunging_speed: f32,
    faux_position: f32,
    short_jump_vel_add: f32,
    long_jump_vel_add: f32,
}

/// `adjust_to_difficulty`'s curve: chances over 1% grow smoothly to 100% at difficulty 100.
fn jump_chance(chance: f32, difficulty: f32) -> f32 {
    if chance > 1.0 {
        (difficulty / 100.0).powf(FRAC_PI_2 - (chance / FRAC_PI_2).atan()) * 100.0
    } else {
        chance * difficulty
    }
}

impl Sim {
    pub fn new(p: MinigameParams) -> Self {
        let (short, long, long_limit, interrupt) = match p.movement {
            Movement::Slow => (0.0, 1.5, 150.0, false),
            Movement::Zippy => (2.25 * 3.0, 0.0625, 200.0, true),
            _ => (2.25, 0.0625, 200.0, true),
        };
        let mut sim = Self {
            fish_position: p.fish_position,
            bait_position: p.bait_position,
            bait_velocity: 0.0,
            completion: p.completion,
            effect: None,
            // `COOLDOWN_START(src, active_effect_cd, rand(5, 9) SECONDS)`.
            effect_cd: 0.0,
            elapsed: 0.0,
            rng: Rng(p.seed),
            times_fired: 0,
            short_jump_chance: jump_chance(short, p.difficulty),
            long_jump_chance: jump_chance(long, p.difficulty),
            short_jump_velocity_limit: 400.0,
            long_jump_velocity_limit: long_limit,
            current_velocity_limit: 0.0,
            target_position: None,
            can_interrupt_move: interrupt,
            fish_velocity: 0.0,
            fish_idle_velocity: p.fish_idle_velocity,
            plunging: true,
            plunging_speed: -22.0,
            faux_position: p.fish_position,
            short_jump_vel_add: 0.0,
            long_jump_vel_add: 0.0,
            p,
        };
        if sim.p.movement == Movement::Plunger {
            sim.plunging_speed += (sim.plunging_speed * sim.p.difficulty * 0.03).round();
            sim.fish_idle_velocity += sim.plunging_speed;
        }
        sim.effect_cd = sim.rng.range(5, 9) as f32;
        sim
    }

    fn has_effects(&self) -> bool {
        self.p.antigrav || self.p.camo
    }

    /// One `SSfishing` tick (`process`), the reel held or not.
    pub fn step(&mut self, reeling: bool) -> Outcome {
        self.elapsed += STEP;
        if self.has_effects() && self.elapsed >= self.effect_cd {
            self.select_effect();
        }
        self.move_fish();
        self.move_bait(reeling)
    }

    fn select_effect(&mut self) {
        let secs = match self.effect.take() {
            None => {
                let pick: Vec<Effect> = [
                    (self.p.antigrav, Effect::Antigrav),
                    (self.p.camo, Effect::Camo),
                ]
                .into_iter()
                .filter_map(|(on, e)| on.then_some(e))
                .collect();
                let effect = pick[self.rng.range(0, pick.len() as i32 - 1) as usize];
                self.effect = Some(effect);
                match effect {
                    Effect::Antigrav => self.rng.range(6, 9),
                    Effect::Camo => self.rng.range(6, 8),
                }
            }
            Some(Effect::Antigrav) => self.rng.range(10, 13),
            Some(Effect::Camo) => self.rng.range(9, 16),
        };
        self.effect_cd = self.elapsed + secs as f32;
    }

    fn acceleration(&self) -> f32 {
        let base = 0.3 * self.p.difficulty + 0.5;
        if self.p.movement == Movement::Accelerando {
            let elapsed = self.times_fired as f32 * STEP;
            return base + base.min(base * elapsed / 30.0);
        }
        base
    }

    fn move_fish(&mut self) {
        let fish_area = AREA - self.p.fish_height;
        match self.p.movement {
            Movement::Plunger => {
                if self.plunging {
                    if self.target_position.is_some_and(|t| t > self.fish_position) {
                        self.target_position = None;
                    }
                    if self.fish_position / fish_area <= 0.04 {
                        self.fish_idle_velocity -= self.plunging_speed;
                        self.plunging = false;
                    }
                } else if (fish_area - self.fish_position) / fish_area <= 0.04 {
                    self.fish_idle_velocity += self.plunging_speed;
                    self.plunging = true;
                }
            }
            Movement::Accelerando => {
                const ACCEL_TIME_CAP: f32 = 30.0;
                let elapsed = self.times_fired as f32 * STEP;
                if self.times_fired == 0 {
                    self.short_jump_vel_add = self.short_jump_velocity_limit / ACCEL_TIME_CAP;
                    self.long_jump_vel_add = self.long_jump_velocity_limit / ACCEL_TIME_CAP;
                } else if elapsed < ACCEL_TIME_CAP {
                    if self.current_velocity_limit != 0.0 {
                        let add = if self.current_velocity_limit == self.short_jump_velocity_limit {
                            self.short_jump_vel_add
                        } else {
                            self.long_jump_vel_add
                        };
                        self.current_velocity_limit += (add * STEP * 100.0).round() / 100.0;
                    }
                    self.short_jump_velocity_limit +=
                        (self.short_jump_vel_add * STEP * 100.0).round() / 100.0;
                    self.long_jump_velocity_limit +=
                        (self.long_jump_vel_add * STEP * 100.0).round() / 100.0;
                }
            }
            _ => {}
        }
        self.times_fired += 1;
        // The jump chances run every other tick (once per decisecond).
        let can_roll = self.times_fired % 2 == 1;
        if self
            .target_position
            .is_some_and(|t| (t - self.fish_position).abs() < FISH_TARGET_MIN_DISTANCE)
        {
            self.target_position = None;
        }
        let height = self.p.fish_height;
        if can_roll
            && (self.can_interrupt_move || self.target_position.is_none())
            && self.rng.prob(self.long_jump_chance)
        {
            let from_top = AREA - self.fish_position - height;
            let from_bottom = self.fish_position;
            let top_chance = if from_top < FISH_SHORT_JUMP_MIN_DISTANCE {
                0.0
            } else {
                from_top / from_bottom.max(1.0) * 100.0
            };
            let mut target = self.fish_position;
            if self.rng.prob(top_chance) {
                target += from_top * self.rng.range(75, 100) as f32 / 100.0;
            } else {
                target -= from_bottom * self.rng.range(75, 100) as f32 / 100.0;
            }
            self.target_position = Some(target.round());
            self.current_velocity_limit = self.long_jump_velocity_limit;
        }
        if let Some(target) = self.target_position {
            let accel = self.acceleration();
            let mut target_accel = (target - self.fish_position) * accel * STEP;
            let idle = self.fish_idle_velocity;
            if idle != 0.0 && target_accel != 0.0 && idle.signum() != target_accel.signum() {
                let halved_ratio = (accel * 0.5) / idle.abs();
                if halved_ratio < 1.0 {
                    let power = (halved_ratio + 0.5).min(1.0);
                    target_accel *= 1.0 - halved_ratio.powf(power);
                } else {
                    target_accel += idle;
                }
            }
            self.fish_velocity = self.fish_velocity * FISH_FRICTION_MULT + target_accel;
        } else if can_roll && self.rng.prob(self.short_jump_chance) {
            let from_top = AREA - self.fish_position - height;
            let from_bottom = self.fish_position;
            let mut jump = None;
            let (low, high) = (
                FISH_SHORT_JUMP_MIN_DISTANCE as i32,
                FISH_SHORT_JUMP_MAX_DISTANCE as i32,
            );
            if from_top >= FISH_SHORT_JUMP_MIN_DISTANCE {
                jump = Some(self.rng.range(low, high) as f32);
            }
            if from_bottom >= FISH_SHORT_JUMP_MIN_DISTANCE
                && (jump.is_none() || self.rng.prob(50.0))
            {
                jump = Some(-self.rng.range(low, high) as f32);
            }
            self.target_position =
                Some((self.fish_position + jump.unwrap_or(0.0)).clamp(0.0, AREA - height));
            self.current_velocity_limit = self.short_jump_velocity_limit;
        }
        let limit = self.current_velocity_limit;
        self.fish_velocity = self.fish_velocity.clamp(-limit, limit);
        if self.p.movement == Movement::Accelerando {
            self.fish_velocity = self.fish_velocity.round();
        }
        let next = |pos: f32, v: f32| (pos + v * STEP).clamp(0.0, AREA - height);
        if self.p.movement == Movement::Choppy {
            self.faux_position = next(self.faux_position, self.fish_velocity);
            // Twice a second (every tenth tick).
            if self.times_fired % 10 == 0 {
                self.fish_position = self.faux_position;
            }
        } else {
            self.fish_position = next(self.fish_position, self.fish_velocity);
        }
    }

    pub fn fish_on_bait(&self) -> bool {
        self.fish_position + self.p.fish_height >= self.bait_position
            && self.bait_position + self.p.bait_height >= self.fish_position
    }

    fn move_bait(&mut self, reeling: bool) -> Outcome {
        let p = &self.p;
        let should_bounce = self.bait_velocity.abs() > BAIT_MIN_VELOCITY_BOUNCE;
        self.bait_position += self.bait_velocity * STEP;
        let top = AREA - p.bait_height;
        if self.bait_position > top {
            self.bait_position = top;
            self.bait_velocity = if reeling || !should_bounce {
                0.0
            } else {
                -self.bait_velocity * p.bait_bounce_mult
            };
        } else if self.bait_position < 0.0 {
            self.bait_position = 0.0;
            self.bait_velocity = if !should_bounce {
                0.0
            } else {
                -self.bait_velocity * p.bait_bounce_mult
            };
        }
        let on_bait = self.fish_on_bait();
        let mut change = if reeling {
            p.reeling_velocity
        } else {
            p.gravity_velocity
        };
        change *= if on_bait {
            p.overlap_velocity_mult
        } else {
            1.0
        } * STEP;
        change = change.round();
        if self.effect == Some(Effect::Antigrav) {
            change = -change;
        }
        // The brake when the acceleration turns against the velocity.
        if self.bait_velocity > 0.0 && change < 0.0 {
            self.bait_velocity += (-self.bait_velocity).max(change * p.deceleration_mult);
        } else if self.bait_velocity < 0.0 && change > 0.0 {
            self.bait_velocity += (-self.bait_velocity).min(change * p.deceleration_mult);
        }
        self.bait_velocity += change;
        if on_bait {
            self.completion += p.completion_gain * STEP;
            if self.completion >= 100.0 {
                self.completion = 100.0;
                return Outcome::Won;
            }
        } else {
            self.completion -= p.completion_loss * STEP;
            if self.completion <= 0.0 {
                self.completion = 0.0;
                return Outcome::Lost;
            }
        }
        self.completion = self.completion.clamp(0.0, 100.0);
        Outcome::Running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(difficulty: f32) -> Setup {
        Setup {
            difficulty,
            has_line: true,
            completion_speed_mult: 1.0,
            bait_speed_mult: 1.0,
            deceleration_mult: 1.0,
            bounciness_mult: 1.0,
            gravity_mult: 1.0,
            bait_height_mult: 1.0,
            reaction_left: Some(BITING_SECS),
            movement: Movement::Default,
            heavy: false,
            lubed: false,
            antigrav: false,
            camo: false,
        }
    }

    #[test]
    fn setup_follows_tg_numbers() {
        let p = setup(&plain(30.0), |low, _| low.max(0));
        // 30 - 15 * 0.3, then x1.4 for striking right away.
        assert!((p.completion - 25.5 * 1.4).abs() < 1e-4, "{}", p.completion);
        assert!((p.completion_gain - (6.0 - 1.0 / 7.5)).abs() < 1e-5);
        assert!((p.completion_loss - 6.2).abs() < 1e-5);
        assert_eq!(p.bait_height, 360.0 - 39.0);
        assert_eq!(p.fish_position, 0.0);
        let late = setup(
            &Setup {
                reaction_left: Some(0.5),
                ..plain(10.0)
            },
            |low, _| low.max(0),
        );
        assert!((late.completion - 30.0 * 0.65).abs() < 1e-4);
        let min = min_win_secs(&p);
        assert!(min > 10.0 && min < 12.0, "{min}");
    }

    #[test]
    fn a_held_bait_on_the_fish_wins_and_an_idle_one_loses() {
        let p = setup(&plain(15.0), |low, high| (low + high) / 2);
        // Never touching the fish: the completion bleeds out.
        let mut sim = Sim::new(MinigameParams {
            fish_position: AREA - FISH_HEIGHT,
            bait_position: 0.0,
            difficulty: 0.0,
            ..p.clone()
        });
        sim.long_jump_chance = 0.0;
        sim.short_jump_chance = 0.0;
        let mut outcome = Outcome::Running;
        for _ in 0..2000 {
            outcome = sim.step(false);
            if outcome != Outcome::Running {
                break;
            }
        }
        assert_eq!(outcome, Outcome::Lost);
        // A bait as tall as the area always covers the fish.
        let mut sim = Sim::new(MinigameParams {
            bait_height: AREA,
            bait_position: 0.0,
            ..p
        });
        let mut ticks = 0;
        while sim.step(false) == Outcome::Running {
            ticks += 1;
            assert!(ticks < 10_000);
        }
        let secs = f64::from(ticks as f32 * STEP);
        assert!(secs + 0.1 >= min_win_secs(&sim.p), "{secs}");
    }

    #[test]
    fn harder_fish_jump_more() {
        assert!(jump_chance(2.25, 80.0) > jump_chance(2.25, 20.0));
        assert_eq!(jump_chance(0.0625, 40.0), 2.5);
        assert!((jump_chance(2.25, 100.0) - 100.0).abs() < 1e-3);
        assert_eq!(
            Movement::of_path("/datum/fish_movement/plunger"),
            Movement::Plunger
        );
        assert_eq!(Movement::of_path("/datum/fish_movement"), Movement::Default);
    }
}
