//! The advanced camera console (server), after tg `/obj/machinery/computer/camera_advanced`
//! (`code/game/machinery/computer/camera_advanced.dm`) and its eye `/mob/eye/camera/remote`
//! (`code/modules/mob/eye/camera/remote.dm`).
//!
//! - A click with an empty hand by a conscious user next to the powered console gives them its
//!   eye ("The console is already in use!" while someone else has it). A new eye starts on the
//!   console's turf when the cameras see it, else on the first working camera of the console's
//!   networks (`ss13`); none: nothing happens. The eye is kept for the next user where it was.
//! - The user's steps move the eye a tile anywhere on the level ([`RemoteEyeStep`]); the
//!   client's view moves with it and sees only what the camera network sees (static elsewhere,
//!   nothing replicated there), like the AI's eye.
//! - The buttons: "End Camera View", "Jump To Camera" (a list of the networks' cameras by
//!   `c_tag`, `" (Deactivated)"` for those off), "Move up a floor" / "Move down a floor" (a
//!   stacked level above or below).
//! - The user walking away, losing consciousness, the console losing power or going away ends
//!   it (`process` -> `unset_machine`, `terminal_off.ogg`).
//! - `/syndie` is the same console.
// shortcut: no circuit components (`advanced_camera`, `advanced_camera_intercept`: USB ports
// and circuits are the wiremod group's), no z-locks (the base and syndie consoles have none),
// no shuttle network prefixes (`connect_to_shuttle`), no eye acceleration (`sprint`), no level
// transitions at the map edge, no static flash on a jump; silicons cannot use it (tg lets
// cyborgs).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_net::server::{ViewMask, Viewer};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{AlwaysState, OpenUi, UiActed, UiSystems};
use ss13_world::SpatialIndex;

use super::cameras::{
    CAMERANET_MASK, Camera, CameraNet, CameraViewSource, CameraViewWindow, VIEW_RADIUS,
    ViewOverride,
};
use super::ui::CameraViewData;
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::health::Stat;
use crate::power::Unpowered;
use crate::server::PlayerMob;
use crate::sound::{PlaySound, playsound};
use crate::tools::tell;

pub const ADVANCED: &str = "/obj/machinery/computer/camera_advanced";
const SYNDIE: &str = "/obj/machinery/computer/camera_advanced/syndie";
const BUTTON_DMI: &str = "icons/mob/actions/actions_silicon.dmi";
const BUTTON: &str = "advcam:";
const OFF: &str = "advcam:off";
const JUMP: &str = "advcam:jump";
const UP: &str = "advcam:up";
const DOWN: &str = "advcam:down";
const SOUND_OFF: &str = "sound/machines/terminal/terminal_off.ogg";
const SOUND_PROMPT: &str = "sound/machines/terminal/terminal_prompt.ogg";
const SOUND_DENY: &str = "sound/machines/terminal/terminal_prompt_deny.ogg";
const SOUND_CONFIRM: &str = "sound/machines/terminal/terminal_prompt_confirm.ogg";

/// The console: its `networks`, its `eyeobj` and `current_user`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct AdvancedConsole {
    pub network: Vec<String>,
    pub eye: Option<Entity>,
    pub user: Option<Entity>,
}

/// The console's eye (tg `/mob/eye/camera/remote`): where it looks.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct RemoteEye {
    pub console: Entity,
    pub pos: TilePos,
}

/// On the mob looking through a console's eye (tg `remote_control`): its steps move the eye.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct RemoteEyeUser {
    pub console: Entity,
    pub eye: Entity,
    pub client: Entity,
}

/// A step of a [`RemoteEyeUser`], taken from its client's queue (`crate::server`).
#[derive(Message, Clone, Copy, Debug)]
pub struct RemoteEyeStep {
    pub mob: Entity,
    pub dir: Dir,
}

/// The "Jump To Camera" list (`tgui_input_list`): the cameras offered.
#[derive(Component, Clone, Debug)]
struct JumpPrompt {
    console: Entity,
    mob: Entity,
    cameras: Vec<Entity>,
}

pub struct CameraAdvancedPlugin;

impl Plugin for CameraAdvancedPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 2661, |w, c| {
            attack_hand(w, c.client, c.mob, c.target)
        });
        app.add_message::<RemoteEyeStep>()
            .add_observer(add_console)
            .add_systems(
                Update,
                (
                    (move_eyes, check_users, follow_eyes)
                        .chain()
                        .after(ss13_world::WorldSystems::Movement),
                    trigger_buttons.run_if(resource_exists::<Messages<UiActed<ActionButtonsUi>>>),
                    jump_chosen
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                ),
            );
    }
}

/// The base console, the syndicate one and the base construction consoles
/// (`crate::base_construction`); the other subtypes are their own machines
/// (`crate::science::xenobio_console`, `crate::shuttle_docker`, ...).
pub fn is_advanced_console(path: &str) -> bool {
    path == ADVANCED || is_under(path, SYNDIE) || crate::base_construction::is_base_console(path)
}

fn add_console(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs
        .get(add.entity)
        .is_ok_and(|o| is_advanced_console(&o.path))
    {
        commands.entity(add.entity).try_insert(AdvancedConsole {
            network: vec!["ss13".to_owned()],
            eye: None,
            user: None,
        });
    }
}

/// tg `can_use` -> `can_interact`: a conscious user next to the powered console.
fn can_use(world: &World, mob: Entity, console: Entity) -> bool {
    world.get::<Unpowered>(console).is_none()
        && world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && world.get::<super::Silicon>(mob).is_none()
        && crate::storage::in_reach(world, mob, console)
}

fn sound(world: &mut World, console: Entity, file: &str) {
    if let Some(&pos) = world.get::<TilePos>(console) {
        playsound(world, PlaySound::new(pos, file, 25.0, false));
    }
}

/// tg `attack_hand`. True when `console` is an advanced camera console.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, console: Entity) -> bool {
    let Some(state) = world.get::<AdvancedConsole>(console).cloned() else {
        return false;
    };
    if !can_use(world, mob, console) {
        return true;
    }
    if state.user.is_some_and(|u| using(world, u, console)) {
        tell(
            world,
            client,
            Span::Warning,
            "The console is already in use!",
        );
        return true;
    }
    let kept = state
        .eye
        .and_then(|e| world.get::<RemoteEye>(e))
        .map(|e| e.pos);
    let base = world
        .get::<crate::base_construction::BaseConsole>(console)
        .is_some();
    let start = |world: &mut World| {
        if base {
            crate::base_construction::spawn_spot(world, console)
        } else {
            start_location(world, console, &state.network)
        }
    };
    let Some(pos) = kept.or_else(|| start(world)) else {
        return true;
    };
    let eye = match state.eye.filter(|&e| world.get::<RemoteEye>(e).is_some()) {
        Some(eye) => eye,
        None => world
            .spawn((
                RemoteEye { console, pos },
                CameraViewSource::default(),
                AlwaysState,
                Name::new("Camera Eye"),
            ))
            .id(),
    };
    give_control(world, client, mob, console, eye, pos);
    true
}

/// Whether `mob` looks through `console`'s eye.
fn using(world: &World, mob: Entity, console: Entity) -> bool {
    world
        .get::<RemoteEyeUser>(mob)
        .is_some_and(|u| u.console == console)
}

/// Where a new eye starts: the console's turf when the cameras see it, else the turf of the
/// first working camera sharing a network.
fn start_location(world: &mut World, console: Entity, network: &[String]) -> Option<TilePos> {
    let at = *world.get::<TilePos>(console)?;
    if world.resource::<CameraNet>().visible(at) {
        return Some(at);
    }
    let mut q = world.query::<(Entity, &Camera, &TilePos)>();
    let mut cams: Vec<(Entity, TilePos)> = q
        .iter(world)
        .filter(|(_, c, _)| c.enabled && c.network.iter().any(|n| network.contains(n)))
        .map(|(e, _, &p)| (e, p))
        .collect();
    cams.sort_by_key(|(e, _)| e.to_bits());
    cams.first().map(|&(_, p)| p)
}

/// tg `give_eye_control` + `setLoc`.
fn give_control(
    world: &mut World,
    client: Entity,
    mob: Entity,
    console: Entity,
    eye: Entity,
    pos: TilePos,
) {
    world.entity_mut(eye).insert(RemoteEye { console, pos });
    set_view(world, eye, pos);
    if let Some(mut state) = world.get_mut::<AdvancedConsole>(console) {
        state.eye = Some(eye);
        state.user = Some(mob);
    }
    world.entity_mut(mob).insert(RemoteEyeUser {
        console,
        eye,
        client,
    });
    if let Ok(mut c) = world.get_entity_mut(client) {
        // The mask now, so the first replicated box is already the network's.
        c.insert((ViewOverride, ViewMask(CAMERANET_MASK)));
        if let Some(mut viewer) = c.get_mut::<Viewer>() {
            viewer.center = pos;
        }
    }
    world.write_message(OpenUi::new::<CameraViewWindow>(client, mob, eye, ""));
    refresh_buttons(world, mob);
}

/// The eye's CameraView: what the network sees around it; the client's view goes there.
fn set_view(world: &mut World, eye: Entity, pos: TilePos) {
    let visible = world.resource::<CameraNet>().view_bits(pos, VIEW_RADIUS);
    let data = CameraViewData {
        center: pos,
        radius: VIEW_RADIUS,
        visible,
        moves_view: true,
    };
    if let Some(mut view) = world.get_mut::<CameraViewSource>(eye) {
        view.set_if_neq(CameraViewSource(data));
    }
}

/// tg `remove_eye_control` / `unset_machine`: the user's view back on its body, the buttons
/// gone, `terminal_off.ogg`.
pub fn remove_control(world: &mut World, console: Entity) {
    let Some(mob) = world
        .get_mut::<AdvancedConsole>(console)
        .and_then(|mut s| s.user.take())
    else {
        return;
    };
    if using(world, mob, console) {
        release(world, mob);
    }
    sound(world, console, SOUND_OFF);
}

/// The mob stops looking through any eye: its client's view back on it.
fn release(world: &mut World, mob: Entity) {
    let Some(user) = world.get::<RemoteEyeUser>(mob).copied() else {
        return;
    };
    world.entity_mut(mob).remove::<RemoteEyeUser>();
    let pos = world.get::<TilePos>(mob).copied();
    if let Ok(mut c) = world.get_entity_mut(user.client) {
        c.remove::<ViewOverride>();
        if let (Some(pos), Some(mut viewer)) = (pos, c.get_mut::<Viewer>()) {
            viewer.center = pos;
        }
    }
    refresh_buttons(world, mob);
}

/// tg `process`: a user who can no longer use the console lets go; a user whose console is
/// gone too.
fn check_users(world: &mut World) {
    let mut q = world.query::<(Entity, &RemoteEyeUser)>();
    let users: Vec<(Entity, RemoteEyeUser)> = q.iter(world).map(|(e, u)| (e, *u)).collect();
    for (mob, user) in users {
        let console_ok = world
            .get::<AdvancedConsole>(user.console)
            .is_some_and(|s| s.user == Some(mob) && s.eye == Some(user.eye));
        if !console_ok || world.get::<RemoteEye>(user.eye).is_none() {
            release(world, mob);
            continue;
        }
        let client_ok = world.get::<PlayerMob>(mob).map(|p| p.client) == Some(user.client);
        if !client_ok || !can_use(world, mob, user.console) {
            remove_control(world, user.console);
        }
    }
    // A console whose user is gone forgets them.
    let mut q = world.query::<(Entity, &AdvancedConsole)>();
    let stale: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s)| {
            s.user
                .is_some_and(|u| world.get::<RemoteEyeUser>(u).is_none())
        })
        .map(|(e, _)| e)
        .collect();
    for console in stale {
        if let Some(mut s) = world.get_mut::<AdvancedConsole>(console) {
            s.user = None;
        }
    }
}

/// tg `relaymove` -> `setLoc(get_step(...))`: one tile, anywhere on the level.
fn move_eyes(world: &mut World, mut steps: Local<MessageCursor<RemoteEyeStep>>) {
    let pending: Vec<RemoteEyeStep> = steps
        .read(world.resource::<Messages<RemoteEyeStep>>())
        .copied()
        .collect();
    for RemoteEyeStep { mob, dir } in pending {
        let Some(user) = world.get::<RemoteEyeUser>(mob).copied() else {
            continue;
        };
        let Some(eye) = world.get::<RemoteEye>(user.eye).copied() else {
            continue;
        };
        let to = eye.pos.step(dir);
        if world.resource::<SpatialIndex>().in_bounds(to)
            && crate::base_construction::may_move(world, user.console, to)
        {
            move_eye(world, user.eye, to);
        }
    }
}

fn move_eye(world: &mut World, eye: Entity, to: TilePos) {
    if let Some(mut e) = world.get_mut::<RemoteEye>(eye) {
        e.pos = to;
    }
    set_view(world, eye, to);
}

/// The users' views follow their eyes; the eyes' static follows the network.
fn follow_eyes(world: &mut World, mut seen: Local<u64>) {
    let version = world.resource::<CameraNet>().version;
    let changed = std::mem::replace(&mut *seen, version) != version;
    let mut q = world.query::<&RemoteEyeUser>();
    let users: Vec<RemoteEyeUser> = q.iter(world).copied().collect();
    for user in users {
        let Some(pos) = world.get::<RemoteEye>(user.eye).map(|e| e.pos) else {
            continue;
        };
        if changed {
            set_view(world, user.eye, pos);
        }
        if let Some(mut viewer) = world.get_mut::<Viewer>(user.client)
            && viewer.center != pos
        {
            viewer.center = pos;
        }
    }
}

fn button(world: &World, id: &str, name: &str, state: &str) -> (ActionButton, f64) {
    button_of(world, id, name, BUTTON_DMI, state)
}

fn button_of(world: &World, id: &str, name: &str, dmi: &str, state: &str) -> (ActionButton, f64) {
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(dmi, state));
    (
        ActionButton {
            id: id.to_owned(),
            name: name.to_owned(),
            icon,
            background: None,
            border: None,
            available: true,
            cooldown: 0.0,
            active: false,
            maptext: None,
        },
        0.0,
    )
}

/// tg `GrantActions` / `actions_removed.Remove`: the console's buttons after the mob's others.
fn refresh_buttons(world: &mut World, mob: Entity) {
    let current = world.get::<ActionButtons>(mob);
    let had = current.is_some_and(|b| b.0.iter().any(|(b, _)| b.id.starts_with(BUTTON)));
    let user = world.get::<RemoteEyeUser>(mob).copied();
    let using = user.is_some();
    if !using && !had {
        return;
    }
    let mut buttons: Vec<(ActionButton, f64)> = current
        .map(|b| {
            b.0.iter()
                .filter(|(b, _)| !b.id.starts_with(BUTTON))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let base = user
        .map(|u| crate::base_construction::buttons(world, u.console))
        .unwrap_or_default();
    if !base.is_empty() {
        buttons.extend(
            base.into_iter()
                .map(|(id, name, dmi, state)| button_of(world, id, name, dmi, state)),
        );
    } else if using {
        buttons.extend([
            button(world, OFF, "End Camera View", "camera_off"),
            button(world, JUMP, "Jump To Camera", "camera_jump"),
            button(world, UP, "Move up a floor", "move_up"),
            button(world, DOWN, "Move down a floor", "move_down"),
        ]);
    }
    set_buttons(world, mob, buttons);
}

/// The console's buttons, checked at the moment of effect: the client's mob still looks
/// through that console's eye.
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        let Some(user) = world.get::<RemoteEyeUser>(actor).copied() else {
            continue;
        };
        if crate::base_construction::trigger(world, client, actor, user, &id) {
            continue;
        }
        match id.as_str() {
            OFF => remove_control(world, user.console),
            JUMP => ask_jump(world, client, actor, user.console),
            UP | DOWN => {
                let up = id == UP;
                let to = world.get::<RemoteEye>(user.eye).and_then(|e| {
                    world
                        .get_resource::<crate::multiz::ZLevels>()
                        .and_then(|l| l.step(e.pos, up))
                });
                let text = match (to, up) {
                    (Some(to), _) => {
                        move_eye(world, user.eye, to);
                        if up {
                            "You move upwards."
                        } else {
                            "You move downwards."
                        }
                    }
                    (None, true) => "You can't move upwards!",
                    (None, false) => "You can't move downwards!",
                };
                tell(world, client, Span::Notice, text);
            }
            _ => {}
        }
    }
}

/// tg `camera_jump/Activate`: `get_available_camera_by_tag_list(networks)`.
fn ask_jump(world: &mut World, client: Entity, mob: Entity, console: Entity) {
    let Some(network) = world
        .get::<AdvancedConsole>(console)
        .map(|s| s.network.clone())
    else {
        return;
    };
    let mut q = world.query::<(Entity, &Camera, Has<crate::emp::Emped>)>();
    let mut cams: Vec<(String, Entity)> = q
        .iter(world)
        .filter(|(_, c, _)| !c.c_tag.is_empty() && c.network.iter().any(|n| network.contains(n)))
        .map(|(e, c, emped)| {
            let usable = c.enabled && !emped;
            let tag = if usable {
                c.c_tag.clone()
            } else {
                format!("{} (Deactivated)", c.c_tag)
            };
            (tag, e)
        })
        .collect();
    cams.sort_by(|a, b| a.0.cmp(&b.0));
    // An associative list: a repeated tag keeps the last camera.
    cams.dedup_by(|later, earlier| {
        let same = later.0 == earlier.0;
        if same {
            earlier.1 = later.1;
        }
        same
    });
    sound(world, console, SOUND_PROMPT);
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Cameras",
            "Camera to view",
            PromptKind::List {
                items: cams.iter().map(|(t, _)| t.clone()).collect(),
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(JumpPrompt {
        console,
        mob,
        cameras: cams.into_iter().map(|(_, e)| e).collect(),
    });
}

/// The chosen camera, at the moment of effect: the mob still looks through that console's eye
/// and the camera still exists; the eye goes to its turf.
fn jump_chosen(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        let Some(ask) = world.get::<JumpPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(user) = world
            .get::<RemoteEyeUser>(ask.mob)
            .copied()
            .filter(|u| u.console == ask.console)
        else {
            continue;
        };
        let at = ask
            .cameras
            .get(i)
            .and_then(|&c| world.get::<Camera>(c).map(|_| c))
            .and_then(|c| world.get::<TilePos>(c).copied());
        let Some(at) = at else {
            sound(world, ask.console, SOUND_DENY);
            continue;
        };
        sound(world, ask.console, SOUND_CONFIRM);
        move_eye(world, user.eye, at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_base_and_syndicate_consoles_are_advanced_consoles() {
        assert!(is_advanced_console(ADVANCED));
        assert!(is_advanced_console(SYNDIE));
        assert!(!is_advanced_console(
            "/obj/machinery/computer/camera_advanced/xenobio"
        ));
        assert!(!is_advanced_console(
            "/obj/machinery/computer/camera_advanced/shuttle_docker"
        ));
        assert!(!is_advanced_console(
            "/obj/machinery/computer/camera_advanced/abductor"
        ));
    }
}
