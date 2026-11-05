//! Silicons using machines (server): tg `attack_ai` / `attack_robot` -> `_try_interact` ->
//! `ui_interact`, and the silicon branches of tgui's `default_state`.
//!
//! - One entry point, [`attack_ai`], for the AI's plain clicks (on what its cameras see,
//!   `crate::silicon::ai`), a cyborg's empty-handed clicks on adjacent things
//!   (`crate::inventory`) and on things up to `interaction_range` (7) tiles away ([`RobotClick`]
//!   from `crate::click`). It opens the machine's window: an airlock's `AiAirlock`
//!   ([`super::airlock`]), an APC's `Apc` (`crate::power::apc_ui`), air alarms and atmos
//!   machines (`crate::atmos_machinery::windows`), the solar control, the mech bay console, the
//!   communications console (`crate::comms_server`), the robotics console
//!   (`super::robotics`), a bot's `SimpleBot` (`crate::bots`).
//!   The cell charger ignores silicons (`attack_ai` returns), as in tg.
//! - Windows of a silicon actor ([`RemoteActor`]) are not judged by distance: the AI's are
//!   interactive while its core is conscious with control enabled and the cameras see the
//!   source (tg `can_see`), else they close; a cyborg's are interactive within its view (15
//!   tiles, `getviewsize(client.view)`), else disabled, and disabled while it is not conscious.
//! - [`has_silicon_access`] is tg `HAS_SILICON_ACCESS`: windows show silicon controls (lock
//!   buttons) and skip ID locks for it.
// shortcut: other consoles (R&D, cargo, records, camera, DNA), vending machines, lathes and
// disposal bins do not open for silicons yet; route them through `attack_ai` as they need it.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{AlwaysState, RemoteActor, UiSystems, UiWindow};

use super::Silicon;
use super::ai::{AiCore, AiEye};
use super::cameras::CameraNet;
use super::cyborg::Cyborg;
use crate::doors::Door;
use crate::health::Stat;
use crate::inventory::Inventory;
use crate::power::model::Apc;
use ss13_mapload::DoorKind;

/// tg silicon `interaction_range`: how far a cyborg's empty-handed click reaches.
pub const ROBOT_INTERACTION_RANGE: u32 = 7;
/// tg `getviewsize(world.view)`: the smaller side of a cyborg's view.
pub const ROBOT_VIEW: u32 = 15;

/// tg `HAS_SILICON_ACCESS`: an AI (its eye acts for it) or a cyborg.
pub type SiliconUsers<'w, 's> = Query<'w, 's, (), Or<(With<Silicon>, With<AiEye>)>>;

pub fn has_silicon_access(world: &World, user: Entity) -> bool {
    world
        .get_entity(user)
        .is_ok_and(|e| e.contains::<Silicon>() || e.contains::<AiEye>())
}

/// A cyborg's plain click on something it sees within [`ROBOT_INTERACTION_RANGE`] that is not
/// adjacent (tg `/mob/living/silicon/robot/ClickOn`: `if(!W && get_dist(src, A) <=
/// interaction_range) A.attack_robot(src)`); adjacent ones arrive as `ClickResolved`.
#[derive(Message, Clone, Copy, Debug)]
pub struct RobotClick {
    pub client: Entity,
    pub borg: Entity,
    pub target: Entity,
}

pub struct SiliconInteractPlugin;

impl Plugin for SiliconInteractPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RobotClick>().add_systems(
            Update,
            (
                robot_clicks
                    .after(crate::click::ResolveClicks)
                    .before(UiSystems::Open),
                silicon_status.in_set(UiSystems::StatusHook),
            ),
        );
    }
}

/// tg `attack_ai(user)` of `target` for a silicon `user` (the AI's eye or a cyborg). True when
/// the target is a machine that reacts to silicons.
pub fn attack_ai(world: &mut World, client: Entity, user: Entity, target: Entity) -> bool {
    let Ok(e) = world.get_entity(target) else {
        return false;
    };
    if e.get::<Door>().is_some_and(|d| d.kind == DoorKind::Airlock) {
        super::airlock::attack_ai(world, client, user, target);
        return true;
    }
    if e.contains::<Apc>() {
        crate::power::apc_ui::open(world, client, user, target);
        return true;
    }
    if e.contains::<crate::mecha::bay::MechBayConsole>() {
        crate::mecha::bay::open(world, client, user, target);
        return true;
    }
    if e.contains::<crate::power::solar::SolarControl>() {
        crate::power::solar::open(world, client, user, target);
        return true;
    }
    if e.contains::<crate::power::chargers::CellCharger>() {
        return true;
    }
    if e.contains::<crate::comms_server::CommsConsole>() {
        crate::comms_server::open(world, client, user, target);
        return true;
    }
    if super::law_rack::attack_ai(world, client, user, target) {
        return true;
    }
    if super::robotics::is_console(world, target) {
        super::robotics::open(world, client, user, target);
        return true;
    }
    // `/mob/living/basic/bot/attack_ai`.
    if world.get::<crate::bots::Bot>(target).is_some() {
        crate::bots::ui::open(world, client, user, target);
        return true;
    }
    // Turrets and their control panels (`crate::defenses`).
    if crate::defenses::server::attack_ai(world, client, user, target) {
        return true;
    }
    crate::atmos_machinery::windows::interact(
        world,
        client,
        user,
        target,
        ClickModifiers::default(),
    )
}

/// tg `attack_robot` of an adjacent target clicked with an empty hand (`crate::inventory`).
pub fn attack_robot(world: &mut World, client: Entity, borg: Entity, target: Entity) -> bool {
    world.get::<Cyborg>(borg).is_some() && attack_ai(world, client, borg, target)
}

fn robot_clicks(world: &mut World, mut clicks: Local<MessageCursor<RobotClick>>) {
    let clicks: Vec<RobotClick> = clicks
        .read(world.resource::<Messages<RobotClick>>())
        .copied()
        .collect();
    for click in clicks {
        let empty = world
            .get::<Inventory>(click.borg)
            .is_some_and(|i| i.active_item().is_none());
        if empty && world.get::<Stat>(click.borg) == Some(&Stat::Conscious) {
            attack_ai(world, click.client, click.borg, click.target);
        }
    }
}

/// tg `/mob/living/silicon/ai/default_can_use_topic` and the robot's, for windows of
/// [`RemoteActor`]s on something other than themselves.
fn silicon_status(
    mut windows: Query<&mut UiWindow>,
    eyes: Query<&AiEye>,
    cores: Query<(&AiCore, &Stat, Option<&super::ai_power::AiPower>)>,
    borgs: Query<(&TilePos, &Stat), With<Cyborg>>,
    positions: Query<&TilePos>,
    remote: Query<(), With<RemoteActor>>,
    always: Query<(), With<AlwaysState>>,
    net: Option<Res<CameraNet>>,
) {
    for mut window in &mut windows {
        let (actor, source) = (window.actor, window.source);
        if source == actor || !remote.contains(actor) || always.contains(source) {
            continue;
        }
        let Ok(&source_pos) = positions.get(source) else {
            continue;
        };
        let status = if let Ok(eye) = eyes.get(actor) {
            if eye.ai == source {
                continue;
            }
            let Ok((core, stat, power)) = cores.get(eye.ai) else {
                window.cap_status(UiStatus::Close);
                continue;
            };
            // tg `shared_ui_interaction`: the hacked APC works while depowered, nothing else.
            if power.is_some_and(|p| p.apc_override == Some(source)) {
                continue;
            }
            let controls = !core.control_disabled && *stat == Stat::Conscious;
            if power.is_some_and(|p| p.incapacitated()) {
                UiStatus::Disabled
            } else if controls && net.as_ref().is_some_and(|n| n.visible(source_pos)) {
                UiStatus::Interactive
            } else {
                UiStatus::Close
            }
        } else if let Ok((&pos, stat)) = borgs.get(actor) {
            match pos.chebyshev(source_pos) {
                None => UiStatus::Close,
                Some(_) if *stat != Stat::Conscious => UiStatus::Disabled,
                Some(d) if d <= ROBOT_VIEW => UiStatus::Interactive,
                Some(_) => UiStatus::Disabled,
            }
        } else {
            continue;
        };
        window.cap_status(status);
    }
}
