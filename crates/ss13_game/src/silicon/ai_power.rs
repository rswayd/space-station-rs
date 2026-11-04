//! The AI core's power (server), after tg `code/modules/mob/living/silicon/ai/life.dm`:
//! `Life` (every `SSmobs` fire, 2 s), `lacks_power`, `ai_lose_power`,
//! `start_RestorePowerRoutine`, `ai_restore_power` and `setAiRestorePowerRoutine`.
//!
//! - The core needs equipment power in its area (and a tile that is not space). Without it the
//!   AI loses power: it is incapacitated (no clicks or HUD actions), its view goes dark (no camera
//!   tiles, nothing replicated: `update_sight` clears its sight) and the restore routine starts:
//!   tg's timed fault-detection messages, then the search for the area's APC. Found, the APC's
//!   window opens and stays usable (tg `apc_override`) though nothing else is.
//! - Each life tick without power drains the backup battery by one (200 = 400 s); at zero the AI
//!   takes 200 oxygen damage and dies. Powered, it recharges one per tick.
//! - Power back while the routine waits (searching or with the APC found) restores the AI on the
//!   next life tick; during the routine's first checks the routine restores it itself.
// shortcut: the core does not draw its 500 W from the area's APC terminal (tg `use_energy` on
// the terminal's powernet); add when machines draw from terminals directly.
// shortcut: depowered, the view shows camera static, not tg's black screen without sight flags.

use bevy::prelude::*;
use ss13_mapload::Areas;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::ai::{AiCore, AiEye};
use super::notice;
use crate::health::{DamageType, Health, Stat};
use crate::power::{AreaPower, Channel};
use crate::server::PlayerMob;

/// tg `battery`'s initial and maximum value.
pub const BATTERY_MAX: f64 = 200.0;
/// tg `SSmobs.wait`: one `Life` per 2 seconds.
pub const LIFE_TICK: f64 = 2.0;

/// tg `aiRestorePowerRoutine`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Routine {
    #[default]
    Off,
    /// `POWER_RESTORATION_START`: the routine runs its checks.
    Start,
    /// `POWER_RESTORATION_SEARCH_APC`: waiting for power to come back.
    SearchApc,
    /// `POWER_RESTORATION_APC_FOUND`: the APC's window is open to the AI.
    ApcFound,
}

/// The step of `start_RestorePowerRoutine` that runs next (tg sleeps between them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Begin,
    FaultCheck,
    EmergencyOnline,
    Verify,
    Search(u8),
    Receive,
    Ready,
}

/// On the AI core: its backup battery and the restore routine.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct AiPower {
    pub battery: f64,
    pub routine: Routine,
    /// tg `apc_override`: the APC whose window works while the AI is depowered.
    pub apc_override: Option<Entity>,
    /// Game time of the next `Life`.
    next_life: f64,
    /// The routine's next step and when it runs.
    next_step: Option<(Step, f64)>,
}

impl Default for AiPower {
    fn default() -> Self {
        Self {
            battery: BATTERY_MAX,
            routine: Routine::Off,
            apc_override: None,
            next_life: 0.0,
            next_step: None,
        }
    }
}

impl AiPower {
    /// tg `TRAIT_INCAPACITATED` from `POWER_LACK_TRAIT`.
    pub fn incapacitated(&self) -> bool {
        self.routine != Routine::Off
    }
}

pub struct AiPowerPlugin;

impl Plugin for AiPowerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, ai_life);
    }
}

/// Whether the AI (its core at `core`) is incapacitated by a power loss.
pub fn depowered(world: &World, core: Entity) -> bool {
    world
        .get::<AiPower>(core)
        .is_some_and(AiPower::incapacitated)
}

/// tg `lacks_power()` with `POWER_REQ_ALL`: no equipment power in the area, or space.
pub fn lacks_power(world: &World, pos: TilePos) -> bool {
    if crate::mobs::ai::behaviors::is_space(world, pos) {
        return true;
    }
    let Some(area) = world.get_resource::<Areas>().and_then(|a| a.area_at(pos)) else {
        return true;
    };
    // An area without a power model is tg `requires_power = FALSE`: powered.
    world
        .get::<AreaPower>(area)
        .is_some_and(|p| !p.powered(Channel::Equip))
}

fn area_apc(world: &World, pos: TilePos) -> Option<Entity> {
    let area = world.get_resource::<Areas>()?.area_at(pos)?;
    world.get::<AreaPower>(area)?.apc
}

fn client_of(world: &World, core: Entity) -> Option<Entity> {
    let eye = world.get::<AiCore>(core)?.eye;
    world.get::<PlayerMob>(eye).map(|p| p.client)
}

fn tell(world: &mut World, core: Entity, span: Span, text: &str) {
    if let Some(client) = client_of(world, core) {
        notice(world, client, span, text);
    }
}

/// tg `setAiRestorePowerRoutine`: the eye goes blind with the routine and sees again without.
fn set_routine(world: &mut World, core: Entity, routine: Routine) {
    let Some(mut power) = world.get_mut::<AiPower>(core) else {
        return;
    };
    power.routine = routine;
    let blind = routine != Routine::Off;
    let Some(eye) = world.get::<AiCore>(core).map(|c| c.eye) else {
        return;
    };
    if let Some(mut e) = world.get_mut::<AiEye>(eye)
        && e.blind != blind
    {
        e.blind = blind;
    }
}

/// tg `ai_lose_power`.
fn lose_power(world: &mut World, core: Entity, now: f64) {
    set_routine(world, core, Routine::Start);
    if let Some(mut power) = world.get_mut::<AiPower>(core) {
        power.next_step = Some((Step::Begin, now + 2.0));
    }
    tell(world, core, Span::Warning, "You've lost power!");
}

/// tg `ai_restore_power`.
pub fn restore_power(world: &mut World, core: Entity) {
    let Some(power) = world.get::<AiPower>(core) else {
        return;
    };
    if power.routine == Routine::Off {
        return;
    }
    let (found, had_override) = (
        power.routine == Routine::ApcFound,
        power.apc_override.is_some(),
    );
    if found {
        tell(
            world,
            core,
            Span::Notice,
            "Alert cancelled. Power has been restored.",
        );
        if had_override {
            tell(world, core, Span::Notice, "APC backdoor has been closed.");
        }
    } else {
        tell(
            world,
            core,
            Span::Notice,
            "Alert cancelled. Power has been restored without our assistance.",
        );
    }
    set_routine(world, core, Routine::Off);
    if let Some(mut power) = world.get_mut::<AiPower>(core) {
        power.apc_override = None;
        power.next_step = None;
    }
}

/// tg `Life` (power part) and the routine's timed steps, for every living AI core.
// Cost: a few AIs per round, an area lookup each per life tick.
fn ai_life(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut cores = world.query_filtered::<(Entity, &Stat, &AiPower), With<AiCore>>();
    let due: Vec<(Entity, bool, bool)> = cores
        .iter(world)
        .filter(|(_, stat, _)| **stat != Stat::Dead)
        .map(|(e, _, p)| {
            (
                e,
                now >= p.next_life,
                p.next_step.is_some_and(|(_, at)| now >= at),
            )
        })
        .filter(|&(_, life, step)| life || step)
        .collect();
    for (core, life, step) in due {
        if step {
            run_step(world, core, now);
        }
        if life {
            life_tick(world, core, now);
        }
    }
}

fn life_tick(world: &mut World, core: Entity, now: f64) {
    let Some(&pos) = world.get::<TilePos>(core) else {
        return;
    };
    let lacks = lacks_power(world, pos);
    let Some(mut power) = world.get_mut::<AiPower>(core) else {
        return;
    };
    power.next_life = now + LIFE_TICK;
    let dead_battery = power.battery <= 0.0 && lacks;
    if power.routine != Routine::Off {
        power.battery -= 1.0;
    } else if power.battery < BATTERY_MAX {
        power.battery += 1.0;
    }
    let routine = power.routine;
    if dead_battery {
        tell(
            world,
            core,
            Span::Warning,
            "Your backup battery's output drops below usable levels. It takes only a moment \
             longer for your systems to fail, corrupted and unusable.",
        );
        if let Some(mut health) = world.get_mut::<Health>(core) {
            health.adjust(DamageType::Oxy, 200.0);
        }
    }
    if !lacks {
        if matches!(routine, Routine::SearchApc | Routine::ApcFound) {
            restore_power(world, core);
        }
    } else if routine == Routine::Off {
        lose_power(world, core, now);
    }
}

/// One step of tg `start_RestorePowerRoutine`; each sleep is the delay to the next step.
fn run_step(world: &mut World, core: Entity, now: f64) {
    let Some((step, _)) = world.get::<AiPower>(core).and_then(|p| p.next_step) else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(core) else {
        return;
    };
    let space = crate::mobs::ai::behaviors::is_space(world, pos);
    let powered = !lacks_power(world, pos);
    let next = |world: &mut World, step: Option<(Step, f64)>| {
        if let Some(mut p) = world.get_mut::<AiPower>(core) {
            p.next_step = step.map(|(s, delay)| (s, now + delay));
        }
    };
    let say = |world: &mut World, span: Span, text: &str| tell(world, core, span, text);
    match step {
        Step::Begin => {
            say(
                world,
                Span::Notice,
                "Backup battery online. Scanners, camera, and radio interface offline. \
                 Beginning fault-detection.",
            );
            next(world, Some((Step::FaultCheck, 5.0)));
        }
        Step::FaultCheck => {
            if powered {
                restore_power(world, core);
                return;
            }
            say(
                world,
                Span::Notice,
                "Fault confirmed: missing external power. Shutting down main control system to \
                 save power.",
            );
            next(world, Some((Step::EmergencyOnline, 2.0)));
        }
        Step::EmergencyOnline => {
            say(
                world,
                Span::Notice,
                "Emergency control system online. Verifying connection to power network.",
            );
            next(world, Some((Step::Verify, 5.0)));
        }
        Step::Verify => {
            if space {
                say(
                    world,
                    Span::Warning,
                    "Unable to verify! No power connection detected!",
                );
                next(world, None);
                set_routine(world, core, Routine::SearchApc);
                return;
            }
            say(
                world,
                Span::Notice,
                "Connection verified. Searching for APC in power network.",
            );
            next(world, Some((Step::Search(1), 5.0)));
        }
        Step::Search(n) => {
            if area_apc(world, pos).is_none() {
                let text = if n == 1 {
                    "Unable to locate APC!"
                } else {
                    "Lost connection with the APC!"
                };
                say(world, Span::Warning, text);
                next(world, None);
                set_routine(world, core, Routine::SearchApc);
                return;
            }
            if powered {
                restore_power(world, core);
                return;
            }
            let text = match n {
                1 => "APC located. Optimizing route to APC to avoid needless power waste.",
                2 => "Best route identified. Hacking offline APC power port.",
                3 => {
                    "Power port upload access confirmed. Loading control program into APC power \
                     port software."
                }
                _ => "Transfer complete. Forcing APC to execute program.",
            };
            say(world, Span::Notice, text);
            next(
                world,
                Some(if n < 4 {
                    (Step::Search(n + 1), 5.0)
                } else {
                    (Step::Receive, 5.0)
                }),
            );
        }
        Step::Receive => {
            say(
                world,
                Span::Notice,
                "Receiving control information from APC.",
            );
            next(world, Some((Step::Ready, 0.2)));
        }
        Step::Ready => {
            next(world, None);
            // The APC may have gone since the last search step.
            let Some(apc) = area_apc(world, pos) else {
                set_routine(world, core, Routine::SearchApc);
                return;
            };
            say(world, Span::Plain, "APC ready for connection.");
            if let Some(mut p) = world.get_mut::<AiPower>(core) {
                p.apc_override = Some(apc);
            }
            set_routine(world, core, Routine::ApcFound);
            if let Some(client) = client_of(world, core)
                && let Some(eye) = world.get::<AiCore>(core).map(|c| c.eye)
            {
                crate::power::apc_ui::open(world, client, eye, apc);
            }
        }
    }
}
