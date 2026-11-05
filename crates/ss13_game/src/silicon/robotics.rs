//! The robotics control console (server), after tg `/obj/machinery/computer/robotics`
//! (`code/game/machinery/computer/robot.dm`) and its `RoboticsControlConsole` window:
//!
//! - A powered console opens for an empty hand, for the AI (`attack_ai`) and for cyborgs. It
//!   lists the cyborgs on its z-level the user may control (`can_control`: the AI its own
//!   cyborgs, a cyborg itself, anyone else every cyborg; never `scrambledcodes` ones): name,
//!   status, charge, model, master AI.
//! - `stopbot` with robotics access (silicons always): the AI locks and releases its cyborgs
//!   (`ai_lockdown`: no self override) but not a console's lockdown; a console locks down one
//!   cyborg at a time, tells it where the console is, and releases its own lockdown or the
//!   AI's. The lockdown itself is `crate::silicon::chassis::set_lockdown`.
//! - A depowered or deleted console releases its cyborg (`on_set_machine_stat`, `Destroy`).
// shortcut: no drones tab, no `killbot` (only a malf AI detonates, and there is none) and no
// `magbot` (`hack_software`, malf only); the Lockdown button asks no confirmation
// (`Button.Confirm`); the z-level check is the same z (tg `is_valid_z_level` also joins
// multi-z station levels); no `active_power_usage` while holding a cyborg.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmName, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::Silicons;
use super::ai::AiEye;
use super::chassis::{Chassis, ROBOTICS_ACCESS, allowed, name_of, set_lockdown, tell_ai, tell_mob};
use super::cyborg::Cyborg;
use super::ui::{RoboticsConsoleData, RoboticsConsoleUi, RoboticsCyborg, STOPBOT};
use crate::click::{ClickResolved, ResolveClicks};
use crate::health::Stat;
use crate::inventory::Inventory;
use crate::power::Unpowered;

/// `/obj/machinery/computer/robotics`.
pub const ROBOTICS_CONSOLE: &str = "/obj/machinery/computer/robotics";

/// tg `locked_down_borg`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct RoboticsConsole {
    pub locked_down_borg: Option<Entity>,
}

pub struct RoboticsPlugin;

impl Plugin for RoboticsPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<RoboticsConsoleUi>()
            .add_systems(
                Update,
                (
                    console_clicks.after(ResolveClicks).before(UiSystems::Open),
                    console_acts.in_set(UiSystems::Handle),
                    release_on_power_loss,
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            )
            .add_observer(release_on_removal);
    }
}

/// The console the click is on (tg type), its state made when first used.
pub fn is_console(world: &World, e: Entity) -> bool {
    world.get::<RoboticsConsole>(e).is_some()
        || world
            .get::<DmObj>(e)
            .is_some_and(|o| ss13_mapload::fidelity::is_under(&o.path, ROBOTICS_CONSOLE))
}

/// tg `ui_interact` of a powered console (also `attack_ai` / `attack_robot`).
pub fn open(world: &mut World, client: Entity, user: Entity, console: Entity) {
    if !is_console(world, console) || world.get::<Unpowered>(console).is_some() {
        return;
    }
    if world.get::<RoboticsConsole>(console).is_none() {
        world.entity_mut(console).insert(RoboticsConsole::default());
    }
    let title = world
        .get::<DmName>(console)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<RoboticsConsoleUi>(
        client, user, console, title,
    ));
}

/// An empty hand of a non-silicon on the console (silicons come through `attack_ai`).
fn console_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        if click.modifiers.right
            || !is_console(world, click.target)
            || super::interact::has_silicon_access(world, click.mob)
            || world
                .get::<Inventory>(click.mob)
                .is_none_or(|i| i.active_item().is_some())
        {
            continue;
        }
        open(world, click.client, click.mob, click.target);
    }
}

/// tg `can_control(user, R)`: `user_ai` is the AI core of an AI user.
pub fn can_control(
    user: Entity,
    user_ai: Option<Entity>,
    user_is_borg: bool,
    borg: Entity,
    cyborg: &Cyborg,
    chassis: &Chassis,
) -> bool {
    if user_ai.is_some() && cyborg.connected_ai != user_ai {
        return false;
    }
    if user_is_borg && borg != user {
        return false;
    }
    !chassis.scrambledcodes
}

/// tg `/datum/tgui_state` `stat`: `CONSCIOUS` 0, `UNCONSCIOUS` 3, `DEAD` 4.
fn stat_code(stat: Stat) -> u8 {
    match stat {
        Stat::Conscious => 0,
        Stat::Dead => 4,
        _ => 3,
    }
}

impl ServerUi for RoboticsConsoleUi {
    type Source = (&'static RoboticsConsole, &'static TilePos);
    type UserParam = (
        Query<
            'static,
            'static,
            (
                Entity,
                &'static Cyborg,
                &'static Chassis,
                &'static Stat,
                &'static TilePos,
            ),
        >,
        Query<'static, 'static, &'static AiEye>,
        Query<'static, 'static, &'static Name>,
        Res<'static, Silicons>,
    );
    type Act = Entity;

    fn static_data(_: (&RoboticsConsole, &TilePos)) {}

    fn data(
        (_, &pos): (&RoboticsConsole, &TilePos),
        user: Entity,
        (borgs, eyes, names, silicons): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> RoboticsConsoleData {
        let user_ai = eyes.get(user).ok().map(|e| e.ai);
        let user_is_borg = borgs.contains(user);
        let name = |e: Entity| names.get(e).map(|n| n.as_str().to_owned()).ok();
        let mut cyborgs: Vec<RoboticsCyborg> = borgs
            .iter()
            .filter(|(borg, cyborg, chassis, _, borg_pos)| {
                borg_pos.z == pos.z
                    && can_control(user, user_ai, user_is_borg, *borg, cyborg, chassis)
            })
            .map(|(borg, cyborg, chassis, stat, _)| {
                let model = cyborg
                    .model
                    .and_then(|i| silicons.0.models.get(i))
                    .map_or("Default", |m| m.name.as_str());
                RoboticsCyborg {
                    name: name(borg).unwrap_or_default(),
                    locked_down: chassis.lockcharge,
                    status: stat_code(*stat),
                    charge: chassis.cell.map(|_| {
                        if cyborg.max_charge > 0.0 {
                            (100.0 * cyborg.charge / cyborg.max_charge).round() as u32
                        } else {
                            0
                        }
                    }),
                    module: format!("{model} Model"),
                    synchronization: cyborg.connected_ai.and_then(name),
                    emagged: chassis.emagged,
                    borg_ref: borg.to_bits().to_string(),
                }
            })
            .collect();
        cyborgs.sort_by(|a, b| a.name.cmp(&b.name));
        RoboticsConsoleData { cyborgs }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<Entity, UiReject> {
        if action != STOPBOT {
            return Err(UiReject::UnknownAction);
        }
        params
            .get("ref")
            .and_then(|r| r.parse::<u64>().ok())
            .and_then(Entity::try_from_bits)
            .ok_or_else(|| UiReject::BadParams("ref: not a ref".into()))
    }
}

/// What `stopbot` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopBot {
    /// `lock_unlock_borg`, with `ai_lockdown` set first; `claim`: the console now holds the
    /// cyborg (and tells it where the console is).
    Toggle {
        ai_lockdown: Option<bool>,
        claim: bool,
    },
    Refuse(&'static str),
}

/// tg `ui_act("stopbot")` after the access and `can_control` checks; `held` is the console's
/// `locked_down_borg`.
pub fn stopbot(
    by_ai: bool,
    lockcharge: bool,
    ai_lockdown: bool,
    held: Option<Entity>,
    borg: Entity,
) -> StopBot {
    let toggle = |ai_lockdown, claim| StopBot::Toggle { ai_lockdown, claim };
    if by_ai {
        return match (lockcharge, ai_lockdown) {
            (true, true) => toggle(Some(false), false),
            (true, false) => StopBot::Refuse("Cyborg locked by an user with superior permissions."),
            (false, _) => toggle(Some(true), false),
        };
    }
    if held.is_none() && !lockcharge {
        toggle(Some(false), true)
    } else if held == Some(borg) {
        toggle(None, false)
    } else if lockcharge && ai_lockdown {
        toggle(Some(false), false)
    } else if lockcharge {
        StopBot::Refuse("The cyborg was locked by a different console.")
    } else {
        StopBot::Refuse("You can lock down only one cyborg at a time.")
    }
}

fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<RoboticsConsoleUi>>>) {
    let acts: Vec<(Entity, Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<RoboticsConsoleUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, console, borg) in acts {
        let Some(state) = world.get::<RoboticsConsole>(console).copied() else {
            continue;
        };
        if !allowed(world, actor, ROBOTICS_ACCESS) {
            crate::tools::tell(world, client, Span::Danger, "Access Denied.");
            continue;
        }
        // `locate(params["ref"]) in GLOB.silicon_mobs`, then `can_control`.
        let (Some(cyborg), Some(chassis)) = (
            world.get::<Cyborg>(borg).cloned(),
            world.get::<Chassis>(borg).cloned(),
        ) else {
            continue;
        };
        let user_ai = world.get::<AiEye>(actor).map(|e| e.ai);
        let user_is_borg = world.get::<Cyborg>(actor).is_some();
        let same_z = world
            .get::<TilePos>(console)
            .zip(world.get::<TilePos>(borg))
            .is_some_and(|(a, b)| a.z == b.z);
        if !same_z || !can_control(actor, user_ai, user_is_borg, borg, &cyborg, &chassis) {
            continue;
        }
        // tg `borg_destroyed` clears a deleted cyborg.
        let held = state
            .locked_down_borg
            .filter(|&b| world.get::<Chassis>(b).is_some());
        match stopbot(
            user_ai.is_some(),
            chassis.lockcharge,
            chassis.ai_lockdown,
            held,
            borg,
        ) {
            StopBot::Refuse(text) => crate::tools::tell(world, client, Span::Danger, text),
            StopBot::Toggle { ai_lockdown, claim } => {
                if let Some(value) = ai_lockdown
                    && let Some(mut c) = world.get_mut::<Chassis>(borg)
                {
                    c.ai_lockdown = value;
                }
                let location = claim.then(|| {
                    world
                        .get::<TilePos>(console)
                        .map(|&p| crate::bots::area_name(world, p))
                        .unwrap_or_default()
                });
                // Not the AI's: this console's own lock.
                let console_side = (user_ai.is_none()).then_some(console);
                lock_unlock(world, console_side, borg, location);
                if claim && let Some(mut c) = world.get_mut::<RoboticsConsole>(console) {
                    c.locked_down_borg = Some(borg);
                }
                let name = name_of(world, borg);
                let action = if world.get::<Chassis>(borg).is_some_and(|c| c.lockcharge) {
                    "locked down"
                } else {
                    "released"
                };
                info!("{} ({actor}) {action} {name}", name_of(world, actor));
            }
        }
    }
}

/// tg `lock_unlock_borg(R, console_location)`: a release frees `console`'s hold.
pub fn lock_unlock(
    world: &mut World,
    console: Option<Entity>,
    borg: Entity,
    location: Option<String>,
) {
    let Some(lockcharge) = world.get::<Chassis>(borg).map(|c| c.lockcharge) else {
        return;
    };
    if lockcharge
        && let Some(console) = console
        && let Some(mut c) = world.get_mut::<RoboticsConsole>(console)
        && c.locked_down_borg == Some(borg)
    {
        c.locked_down_borg = None;
    }
    set_lockdown(world, borg, !lockcharge);
    let locked = !lockcharge;
    if locked {
        tell_mob(world, borg, Span::Warning, "You have been locked down!");
    } else {
        tell_mob(world, borg, Span::Notice, "Your lockdown has been lifted!");
    }
    if let Some(location) = location {
        tell_mob(
            world,
            borg,
            Span::Warning,
            format!(
                "The approximate location of the console that is keeping you locked down is \
                 {location}"
            ),
        );
    }
    if let Some(ai) = world.get::<Cyborg>(borg).and_then(|c| c.connected_ai) {
        let name = name_of(world, borg);
        let (span, text) = if locked {
            (
                Span::Warning,
                format!("ALERT - Cyborg lockdown detected: {name}"),
            )
        } else {
            (
                Span::Notice,
                format!("NOTICE - Cyborg lockdown lifted: {name}"),
            )
        };
        tell_ai(world, ai, span, text);
    }
}

/// tg `on_set_machine_stat`: a depowered console releases its cyborg.
fn release_on_power_loss(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &RoboticsConsole), With<Unpowered>>();
    let held: Vec<(Entity, Entity)> = q
        .iter(world)
        .filter_map(|(e, c)| c.locked_down_borg.map(|b| (e, b)))
        .collect();
    for (console, borg) in held {
        if world.get::<Chassis>(borg).is_some_and(|c| c.lockcharge) {
            lock_unlock(world, Some(console), borg, None);
        }
        if let Some(mut c) = world.get_mut::<RoboticsConsole>(console) {
            c.locked_down_borg = None;
        }
    }
}

/// tg `Destroy`: the held cyborg is released.
fn release_on_removal(
    remove: On<Remove<RoboticsConsole>>,
    consoles: Query<&RoboticsConsole>,
    mut commands: Commands,
) {
    let Some(borg) = consoles
        .get(remove.entity)
        .ok()
        .and_then(|c| c.locked_down_borg)
    else {
        return;
    };
    commands.queue(move |world: &mut World| {
        if world.get::<Chassis>(borg).is_some_and(|c| c.lockcharge) {
            lock_unlock(world, None, borg, None);
        }
    });
}

/// An unpowered console is update-only.
fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<RoboticsConsole>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == RoboticsConsoleUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopbot_follows_tg() {
        let (borg, other) = (
            Entity::from_raw_u32(1).unwrap(),
            Entity::from_raw_u32(2).unwrap(),
        );
        let lock = |ai_lockdown, claim| StopBot::Toggle { ai_lockdown, claim };
        // The AI: its own lock both ways, never a console's.
        assert_eq!(
            stopbot(true, false, false, None, borg),
            lock(Some(true), false)
        );
        assert_eq!(
            stopbot(true, true, true, None, borg),
            lock(Some(false), false)
        );
        assert!(matches!(
            stopbot(true, true, false, Some(borg), borg),
            StopBot::Refuse(_)
        ));
        // A console: one cyborg at a time, its own release, the AI's lock lifted.
        assert_eq!(
            stopbot(false, false, false, None, borg),
            lock(Some(false), true)
        );
        assert_eq!(
            stopbot(false, true, false, Some(borg), borg),
            lock(None, false)
        );
        assert_eq!(
            stopbot(false, true, true, Some(other), borg),
            lock(Some(false), false)
        );
        assert_eq!(
            stopbot(false, true, false, Some(other), borg),
            StopBot::Refuse("The cyborg was locked by a different console.")
        );
        assert_eq!(
            stopbot(false, false, false, Some(other), borg),
            StopBot::Refuse("You can lock down only one cyborg at a time.")
        );
    }

    #[test]
    fn control_rules() {
        let (ai, other_ai) = (
            Entity::from_raw_u32(10).unwrap(),
            Entity::from_raw_u32(11).unwrap(),
        );
        let (borg, user) = (
            Entity::from_raw_u32(1).unwrap(),
            Entity::from_raw_u32(2).unwrap(),
        );
        let cyborg = Cyborg {
            model: None,
            model_item: None,
            charge: 0.0,
            max_charge: 0.0,
            low_power: false,
            connected_ai: Some(ai),
            lawupdate: true,
        };
        let chassis = Chassis::default();
        assert!(can_control(user, None, false, borg, &cyborg, &chassis));
        assert!(can_control(user, Some(ai), false, borg, &cyborg, &chassis));
        assert!(!can_control(
            user,
            Some(other_ai),
            false,
            borg,
            &cyborg,
            &chassis
        ));
        assert!(!can_control(user, None, true, borg, &cyborg, &chassis));
        assert!(can_control(borg, None, true, borg, &cyborg, &chassis));
        let scrambled = Chassis {
            scrambledcodes: true,
            ..Chassis::default()
        };
        assert!(!can_control(user, None, false, borg, &cyborg, &scrambled));
    }
}
