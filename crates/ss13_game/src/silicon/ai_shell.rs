//! AI shells (server), after tg `/mob/living/silicon/robot/shell` (`robot_defines.dm`),
//! `make_shell`, `deploy_init`, `undeploy` (`robot.dm`), the AI's `select_shell`,
//! `disconnect_shell` and its deploy actions (`ai.dm`), and the B.O.R.I.S. module
//! (`/obj/item/borg/upgrade/ai`, `robot_parts.dm`).
//!
//! - A B.O.R.I.S. module on a complete endoskeleton standing on the floor builds an empty shell
//!   ("Empty AI Shell-NNN") with the chest's cell (`super::mmi`); it links to the AI with the
//!   fewest cyborgs.
//! - Every AI has the "Deploy to AI Shell" button; a click of the AI on a shell it may use does
//!   the same with that shell. Refused while incapacitated or with the wireless off ("Wireless
//!   networking module is offline."); the usable shells are those not deployed, not dead, and
//!   free or linked to this AI ("No usable AI shell beacons detected." without any); the AI
//!   picks one ("Which body to control?") unless it clicked one.
//! - Deployed: the shell is named "<AI> <model> Shell-NNN", linked to the AI with its laws
//!   synced and its radio on the AI's channels; the AI's client controls it (its eye stays,
//!   without a client). The shell has "Disconnect from shell"; a shell without a model asks for
//!   one as a new cyborg does.
//! - Undeploying (the button, or forced: the shell dies or goes, the AI dies, loses power or its
//!   wireless, with "Your remote connection has been reset!") returns the client to the eye,
//!   which moves to the shell's turf; the AI is shown its laws and gets "Reconnect to shell"
//!   for the last shell used.
// shortcut: no diagnostic HUD icons, loud binary, gender from the AI's client, shell camera
// tags, EMP disconnects; the shell keeps the AI's radio channels after undeploying (tg
// `recalculateChannels`); a B.O.R.I.S. board on a built cyborg stays refused
// (`super::borg_upgrades`), so a shell is only built from an endoskeleton.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_net::server::{ControlledEntity, SeeInvisible, Sight, Viewer};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{OpenUi, UiActed, UiSystems};

use super::ai::{AiCore, AiEye, AiInside};
use super::chassis::{name_of, tell_ai, tell_mob};
use super::cyborg::Cyborg;
use super::malf_modules::{add_button, remove_button};
use crate::actions::{ActionButtons, ActionButtonsUi};
use crate::health::Stat;
use crate::hud::{HealthHudData, HealthHudUi, HudData};
use crate::server::{MoveQueue, PlayerMob, Players};

/// `/obj/item/borg/upgrade/ai`.
pub const BORIS: &str = "/obj/item/borg/upgrade/ai";
pub const DEPLOY: &str = "deploy_shell";
pub const DEPLOY_LAST: &str = "deploy_last_shell";
pub const UNDEPLOY: &str = "undeploy_shell";
const ACTIONS_AI: &str = "icons/mob/actions/actions_AI.dmi";

/// On a shell (tg `shell`, `deployed`, `mainframe`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiShell {
    pub identifier: u32,
    pub mainframe: Option<Entity>,
}

/// On an AI deployed into `shell` (tg `deployed_shell`), with the client that went there (kept
/// for a shell deleted under it).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeployedShell {
    pub shell: Entity,
    pub client: Entity,
}

/// On an AI: tg `redeploy_action.last_used_shell`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastShell(pub Entity);

/// The shell list asked to an AI.
#[derive(Component, Clone, Debug)]
struct ShellPrompt {
    ai: Entity,
    shells: Vec<Entity>,
}

pub struct AiShellPlugin;

impl Plugin for AiShellPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                grant_deploy,
                buttons.in_set(UiSystems::Handle),
                picked
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
                forced_disconnects.run_if(any_with_component::<DeployedShell>),
            ),
        );
    }
}

/// tg `make_shell` on a cyborg just built around a B.O.R.I.S. module: renamed, linked to the AI
/// with the fewest cyborgs, its alert panel button.
pub fn make_shell(world: &mut World, borg: Entity) {
    let name = name_of(world, borg);
    let identifier = name
        .rsplit('-')
        .next()
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(0);
    world.entity_mut(borg).insert((
        AiShell {
            identifier,
            mainframe: None,
        },
        Name::new(format!("Empty AI Shell-{identifier}")),
    ));
    super::ai_hud::give_borg_buttons(world, borg);
    if let Some(ai) = super::cyborg::fewest_borgs_ai(world) {
        if let Some(mut c) = world.get_mut::<Cyborg>(borg) {
            c.lawupdate = true;
        }
        super::cyborg::connect(world, borg, ai);
        let name = name_of(world, borg);
        tell_ai(
            world,
            ai,
            Span::Notice,
            format!("NOTICE - New cyborg shell detected: {name}"),
        );
    }
}

/// Whether `shell` is one `ai` may deploy into (tg `select_shell`'s `possible`).
pub fn usable(world: &World, ai: Entity, shell: Entity) -> bool {
    world
        .get::<AiShell>(shell)
        .is_some_and(|s| s.mainframe.is_none())
        && world.get::<Stat>(shell).is_some_and(|s| *s != Stat::Dead)
        && world
            .get::<Cyborg>(shell)
            .is_some_and(|c| c.connected_ai.is_none_or(|a| a == ai))
}

/// tg AI `incapacitated`: dead, unconscious or out of power.
fn incapacitated(world: &World, ai: Entity) -> bool {
    world.get::<Stat>(ai) != Some(&Stat::Conscious) || super::ai_power::depowered(world, ai)
}

/// The shells `ai` may deploy into, by name.
pub fn possible(world: &mut World, ai: Entity) -> Vec<Entity> {
    let mut q = world.query_filtered::<Entity, With<AiShell>>();
    let all: Vec<Entity> = q.iter(world).collect();
    let mut shells: Vec<(String, Entity)> = all
        .into_iter()
        .filter(|&s| usable(world, ai, s))
        .map(|s| (name_of(world, s), s))
        .collect();
    shells.sort();
    shells.into_iter().map(|(_, s)| s).collect()
}

/// tg `select_shell(target)`.
pub fn select_shell(world: &mut World, ai: Entity, target: Option<Entity>) {
    let Some(eye) = world.get::<AiCore>(ai).map(|c| c.eye) else {
        return;
    };
    let Some(client) = world.get::<PlayerMob>(eye).map(|p| p.client) else {
        return;
    };
    if incapacitated(world, ai) || world.get::<DeployedShell>(ai).is_some() {
        return;
    }
    if world.get::<AiCore>(ai).is_some_and(|c| c.control_disabled) {
        tell_ai(
            world,
            ai,
            Span::Warning,
            "Wireless networking module is offline.",
        );
        return;
    }
    let shells = possible(world, ai);
    if shells.is_empty() {
        tell_ai(
            world,
            ai,
            Span::Plain,
            "No usable AI shell beacons detected.",
        );
        return;
    }
    if let Some(target) = target.filter(|t| shells.contains(t)) {
        deploy(world, ai, target);
        return;
    }
    let items = shells.iter().map(|&s| name_of(world, s)).collect();
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            eye,
            "Direct Control",
            "Which body to control?",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(ShellPrompt { ai, shells });
}

fn picked(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        let Some(asked) = world.get::<ShellPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&shell) = asked.shells.get(i) else {
            continue;
        };
        // Again after the wait: the AI still able, the shell still usable.
        let ai = asked.ai;
        if incapacitated(world, ai)
            || world.get::<DeployedShell>(ai).is_some()
            || world.get::<AiCore>(ai).is_none_or(|c| c.control_disabled)
            || !usable(world, ai, shell)
        {
            continue;
        }
        deploy(world, ai, shell);
    }
}

/// `client` leaves `from` (kept, without a client) and controls `to` on `pos`.
fn move_client(world: &mut World, client: Entity, from: Entity, to: Entity, pos: TilePos) {
    let queue = world
        .get::<MoveQueue>(from)
        .map(MoveQueue::continued)
        .unwrap_or_default();
    if let Ok(mut old) = world.get_entity_mut(from) {
        old.remove::<(PlayerMob, MoveQueue)>();
    }
    world.entity_mut(to).insert((PlayerMob { client }, queue));
    if let Ok(mut c) = world.get_entity_mut(client) {
        c.remove::<(SeeInvisible, Sight)>()
            .insert((ControlledEntity(to), Viewer::new(pos)));
    }
    if let Some(mut players) = world.get_resource_mut::<Players>() {
        players.set_mob(client, to);
    }
}

/// tg `deploy_init` and the mind's transfer into the shell.
pub fn deploy(world: &mut World, ai: Entity, shell: Entity) {
    let Some(eye) = world.get::<AiCore>(ai).map(|c| c.eye) else {
        return;
    };
    let Some(client) = world.get::<PlayerMob>(eye).map(|p| p.client) else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(shell) else {
        return;
    };
    let identifier = world.get::<AiShell>(shell).map_or(0, |s| s.identifier);
    let designation = world
        .get::<Cyborg>(shell)
        .and_then(|c| c.model)
        .and_then(|i| world.resource::<super::Silicons>().0.models.get(i))
        .map_or_else(|| "Default".to_owned(), |m| m.name.clone());
    let ai_name = name_of(world, ai);
    world.entity_mut(shell).insert((
        AiShell {
            identifier,
            mainframe: Some(ai),
        },
        Name::new(format!("{ai_name} {designation} Shell-{identifier}")),
    ));
    world.entity_mut(ai).insert(DeployedShell { shell, client });
    if let Some(mut c) = world.get_mut::<Cyborg>(shell) {
        c.lawupdate = true;
    }
    super::cyborg::connect(world, shell, ai);
    // The AI's radio channels (tg `radio.channels = AI.radio.channels`).
    let channels = radio_channels(world, eye);
    if world.get::<crate::radio::SiliconRadio>(shell).is_none() {
        super::give_radio(world, shell, &[]);
    }
    if let Some(radio) = world.get::<crate::radio::SiliconRadio>(shell).map(|r| r.0)
        && let Some(mut r) = world.get_mut::<crate::radio::Radio>(radio)
    {
        r.channels = channels;
    }
    move_client(world, client, eye, shell, pos);
    world
        .entity_mut(shell)
        .insert((HudData::default(), HealthHudData::default()));
    if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(shell) {
        inventory.set_changed();
    }
    add_button(
        world,
        shell,
        UNDEPLOY,
        "Disconnect from shell",
        (ACTIONS_AI, "ai_core"),
    );
    crate::ghost::reopen_body_huds(world, client, shell);
    if world
        .get::<Cyborg>(shell)
        .is_some_and(|c| c.model.is_none())
        && world.get::<super::ListInput>(shell).is_some()
    {
        world.write_message(OpenUi::new::<super::ListInputWindow>(
            client,
            shell,
            shell,
            "Выбор модели",
        ));
    }
    super::chassis::show_laws(world, shell);
    info!("the AI {ai} deployed into the shell {shell}");
}

fn radio_channels(world: &World, eye: Entity) -> Vec<(String, bool)> {
    world
        .get::<crate::radio::SiliconRadio>(eye)
        .and_then(|r| world.get::<crate::radio::Radio>(r.0))
        .map(|r| r.channels.clone())
        .unwrap_or_default()
}

/// tg `undeploy`: the client back on the AI's eye, which goes to the shell's turf.
pub fn undeploy(world: &mut World, ai: Entity) {
    let Some(DeployedShell { shell, client }) = world.get::<DeployedShell>(ai).copied() else {
        return;
    };
    world.entity_mut(ai).remove::<DeployedShell>();
    let shell_pos = crate::explosions::turf_of(world, shell);
    // The client still in the shell (not one that left it meanwhile, e.g. by ghosting).
    let in_shell = world.get_entity(shell).is_err()
        || world.get::<PlayerMob>(shell).map(|p| p.client) == Some(client);
    if let Some(mut state) = world.get_mut::<AiShell>(shell) {
        state.mainframe = None;
    }
    if world.get::<AiShell>(shell).is_some() {
        remove_button(world, shell, UNDEPLOY);
    }
    let Some(eye) = world.get::<AiCore>(ai).map(|c| c.eye) else {
        return;
    };
    if world.get::<AiInside>(ai).is_none()
        && let Some(pos) = shell_pos
    {
        world.entity_mut(eye).insert(pos);
    }
    let Some(eye_pos) = world.get::<TilePos>(eye).copied() else {
        return;
    };
    if in_shell && world.get_entity(client).is_ok() {
        move_client(world, client, shell, eye, eye_pos);
        world
            .entity_mut(client)
            .insert(SeeInvisible(super::ai::AI_EYE_INVISIBILITY));
        world.write_message(OpenUi::new::<HealthHudUi>(client, eye, eye, ""));
        world.write_message(OpenUi::new::<super::cameras::CameraViewWindow>(
            client, eye, eye, "",
        ));
        crate::ghost::reopen_body_huds(world, client, eye);
        if let Some(laws) = world.get::<super::laws::Laws>(ai).cloned() {
            super::show_laws(world, client, &laws);
        }
    }
    world.entity_mut(ai).insert(LastShell(shell));
    add_button(
        world,
        eye,
        DEPLOY_LAST,
        "Reconnect to shell",
        (ACTIONS_AI, "ai_last_shell"),
    );
    info!("the AI {ai} left the shell {shell}");
}

/// Every AI eye gets "Deploy to AI Shell" (tg `deploy_action.Grant` in `Initialize`).
fn grant_deploy(world: &mut World) {
    let mut q = world.query_filtered::<Entity, Added<AiEye>>();
    let eyes: Vec<Entity> = q.iter(world).collect();
    for eye in eyes {
        add_button(
            world,
            eye,
            DEPLOY,
            "Deploy to AI Shell",
            (ACTIONS_AI, "ai_shell"),
        );
    }
}

fn buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if ![DEPLOY, DEPLOY_LAST, UNDEPLOY].contains(&id.as_str())
            || actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || !world
                .get::<ActionButtons>(actor)
                .is_some_and(|b| b.0.iter().any(|(b, _)| b.id == id))
        {
            continue;
        }
        if id == UNDEPLOY {
            if let Some(ai) = world.get::<AiShell>(actor).and_then(|s| s.mainframe) {
                undeploy(world, ai);
            }
            continue;
        }
        let Some(ai) = world.get::<AiEye>(actor).map(|e| e.ai) else {
            continue;
        };
        if id == DEPLOY {
            select_shell(world, ai, None);
            continue;
        }
        // tg `deploy_last_shell`: the last shell, or the button goes when it is gone.
        match world.get::<LastShell>(ai).map(|l| l.0) {
            Some(shell) if world.get::<AiShell>(shell).is_some() => {
                select_shell(world, ai, Some(shell));
            }
            _ => {
                world.entity_mut(ai).remove::<LastShell>();
                remove_button(world, actor, DEPLOY_LAST);
            }
        }
    }
}

/// tg robot `attack_ai`: the AI's click on a shell it may use deploys into it.
pub fn attack_ai(world: &mut World, user: Entity, target: Entity) -> bool {
    let Some(ai) = world.get::<AiEye>(user).map(|e| e.ai) else {
        return false;
    };
    if world.get::<AiShell>(target).is_none() {
        return false;
    }
    let free = world
        .get::<Cyborg>(target)
        .is_some_and(|c| c.connected_ai.is_none_or(|a| a == ai));
    if free {
        select_shell(world, ai, Some(target));
    }
    true
}

/// tg `disconnect_shell` on the shell's death, the AI's death or power loss
/// (`ai_lose_power`), the wireless off (`set_control_disabled`) or carding.
fn forced_disconnects(world: &mut World) {
    let mut q = world.query::<(Entity, &DeployedShell)>();
    let deployed: Vec<(Entity, Entity)> = q.iter(world).map(|(a, d)| (a, d.shell)).collect();
    for (ai, shell) in deployed {
        let shell_ok = world
            .get::<AiShell>(shell)
            .is_some_and(|s| s.mainframe == Some(ai))
            && world.get::<Stat>(shell).is_some_and(|s| *s != Stat::Dead);
        let ai_ok = !incapacitated(world, ai)
            && world.get::<AiCore>(ai).is_some_and(|c| !c.control_disabled);
        if shell_ok && ai_ok {
            continue;
        }
        tell_mob(
            world,
            shell,
            Span::Danger,
            "Your remote connection has been reset!",
        );
        undeploy(world, ai);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.init_resource::<Messages<OpenUi>>();
        world.init_resource::<Players>();
        world.init_resource::<super::super::Silicons>();
        world
    }

    fn cyborg(connected_ai: Option<Entity>) -> Cyborg {
        Cyborg {
            model: None,
            model_item: None,
            charge: 0.0,
            max_charge: 0.0,
            low_power: false,
            connected_ai,
            lawupdate: false,
        }
    }

    /// The AI's client goes into the shell and comes back to its eye, which moves to the
    /// shell; the shell's death forces it back.
    #[test]
    fn deploy_and_return() {
        let mut world = world();
        let client = world.spawn_empty().id();
        let eye = world.spawn(TilePos::new(1, 1, 1)).id();
        let ai = world
            .spawn((
                AiCore {
                    eye,
                    control_disabled: false,
                    connected_robots: Vec::new(),
                },
                Stat::Conscious,
                Name::new("HAL"),
            ))
            .id();
        world
            .entity_mut(eye)
            .insert((AiEye { ai, blind: false }, PlayerMob { client }));
        let shell = world
            .spawn((
                TilePos::new(5, 5, 1),
                Stat::Conscious,
                cyborg(None),
                AiShell {
                    identifier: 7,
                    mainframe: None,
                },
                Name::new("Empty AI Shell-7"),
            ))
            .id();
        let other_ai = world.spawn_empty().id();
        let taken = world
            .spawn((
                TilePos::new(5, 5, 1),
                Stat::Conscious,
                cyborg(Some(other_ai)),
                AiShell {
                    identifier: 8,
                    mainframe: None,
                },
            ))
            .id();
        assert_eq!(possible(&mut world, ai), vec![shell]);
        assert!(!usable(&world, ai, taken));
        select_shell(&mut world, ai, Some(shell));
        assert_eq!(
            world.get::<DeployedShell>(ai),
            Some(&DeployedShell { shell, client })
        );
        assert_eq!(
            world.get::<PlayerMob>(shell).map(|p| p.client),
            Some(client)
        );
        assert!(world.get::<PlayerMob>(eye).is_none());
        assert_eq!(world.get::<Cyborg>(shell).unwrap().connected_ai, Some(ai));
        assert_eq!(
            world.get::<Name>(shell).unwrap().as_str(),
            "HAL Default Shell-7"
        );
        assert!(!usable(&world, ai, shell));

        undeploy(&mut world, ai);
        assert_eq!(world.get::<PlayerMob>(eye).map(|p| p.client), Some(client));
        assert_eq!(world.get::<TilePos>(eye), Some(&TilePos::new(5, 5, 1)));
        assert_eq!(world.get::<LastShell>(ai), Some(&LastShell(shell)));
        assert!(usable(&world, ai, shell));

        // Deployed again, the shell dies: forced back.
        deploy(&mut world, ai, shell);
        world.entity_mut(shell).insert(Stat::Dead);
        forced_disconnects(&mut world);
        assert!(world.get::<DeployedShell>(ai).is_none());
        assert_eq!(world.get::<PlayerMob>(eye).map(|p| p.client), Some(client));
    }
}
