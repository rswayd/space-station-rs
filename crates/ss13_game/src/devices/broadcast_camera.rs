//! Broadcast cameras (server), after tg `/obj/item/broadcast_camera`
//! (`code/game/objects/items/devices/broadcast_camera.dm`): its microphone side.
//!
//! - Used in hand by a carbon it goes live (`broadcast_cam1`, `terminal_processing.ogg`,
//!   "в эфире!" to the viewers); used again, dropped or put away it goes offline
//!   (`terminal_prompt_deny.ogg`, "не в эфире").
//! - Live with the microphone on, it is `/obj/item/radio/entertainment/microphone`: speech
//!   within 7 tiles of it goes out on the entertainment frequency (the camera carries the
//!   [`Radio`] while live, so `crate::radio` relays like any broadcasting radio).
//! - Alt-click in hand switches the microphone, a right click in hand renames the broadcast.
// shortcut: no picture. tg's internal camera joins `CAMERANET_NETWORK_CURATOR` and the
// entertainment monitors (`/obj/machinery/computer/security/telescreen/entertainment`) add the
// network while it is live; the port's camera consoles list their cameras once at map load and
// show only mapped, fixed cameras, so a moving camera and monitors that change networks need
// the camera group first. No light while live, no announcement on the monitors, no cargo
// camera's per-camera network (its slowdown comes from its `slowdown` var).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::inventory::{CarriedBy, Inventory};
use crate::radio::{Radio, RadioKind};
use crate::sound::{PlaySound, playsound};

pub const BROADCAST_CAMERA: &str = "/obj/item/broadcast_camera";
const CARGO_CAMERA: &str = "/obj/item/broadcast_camera/cargo";
const PIRATE_CAMERA: &str = "/obj/item/broadcast_camera/siren_pirates";
const ON_SOUND: &str = "sound/machines/terminal/terminal_processing.ogg";
const OFF_SOUND: &str = "sound/machines/terminal/terminal_prompt_deny.ogg";
/// `MAX_CHARTER_LEN`.
const NAME_MAX: u32 = 80;
/// `/obj/item/radio/entertainment` `canhear_range`.
const MIC_RANGE: u32 = 7;

/// `active`, `active_microphone`, `broadcast_name`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct BroadcastCamera {
    pub active: bool,
    pub microphone: bool,
    pub name: String,
}

/// On a rename prompt: the camera it renames.
#[derive(Component, Clone, Copy, Debug)]
struct RenamePrompt(Entity);

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::AttackSelf, 1763, |w, c| {
        crate::devices::broadcast_camera::attack_self(w, c.mob, c.target)
    });
    app.add_observer(add_camera).add_systems(
        Update,
        (
            go_offline_out_of_hands,
            rename_answers
                .in_set(ss13_tgui::server::UiSystems::Handle)
                .after(ss13_tgui::prompt::PromptSystems::Answer),
        ),
    );
}

fn add_camera(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(dm) = items.get(add.entity) else {
        return;
    };
    if !is_under(&dm.path, BROADCAST_CAMERA) {
        return;
    }
    let name = if is_under(&dm.path, CARGO_CAMERA) {
        "Camera Broadcast"
    } else if is_under(&dm.path, PIRATE_CAMERA) {
        "Siren Shimai!"
    } else {
        "Curator News"
    };
    commands
        .entity(add.entity)
        .try_insert_if_new(BroadcastCamera {
            active: false,
            microphone: true,
            name: name.to_owned(),
        });
}

/// The microphone (`/obj/item/radio/entertainment/microphone` with `set_broadcasting`).
fn microphone(on: bool) -> Radio {
    Radio {
        kind: RadioKind::Other,
        on: true,
        frequency: ss13_chat::radio::FREQ_ENTERTAINMENT,
        broadcasting: on,
        listening: false,
        canhear_range: MIC_RANGE,
        special_channels: 0,
        channels: Vec::new(),
        tx_cut: false,
    }
}

fn holder(world: &World, camera: Entity) -> Option<Entity> {
    let mob = world.get::<CarriedBy>(camera)?.0;
    world
        .get::<Inventory>(mob)?
        .hands
        .contains(&Some(camera))
        .then_some(mob)
}

/// `attack_self`: live or offline.
pub fn attack_self(world: &mut World, mob: Entity, camera: Entity) -> bool {
    let Some(state) = world.get::<BroadcastCamera>(camera).cloned() else {
        return false;
    };
    if state.active {
        go_offline(world, camera);
    } else {
        go_live(world, mob, camera);
    }
    true
}

/// `on_activating`: only in a carbon's hands (`iscarbon(loc)`).
fn go_live(world: &mut World, mob: Entity, camera: Entity) {
    let carbon = world
        .get::<crate::health::Health>(mob)
        .is_some_and(|h| h.basic.is_none())
        && world.get::<crate::silicon::Silicon>(mob).is_none();
    if !carbon || holder(world, camera) != Some(mob) {
        return;
    }
    let Some(mut state) = world.get_mut::<BroadcastCamera>(camera) else {
        return;
    };
    state.active = true;
    let mic = state.microphone;
    world.entity_mut(camera).insert(microphone(mic));
    crate::obj_damage::set_icon_state(world, camera, "broadcast_cam1");
    sound(world, camera, ON_SOUND);
    crate::balloon::balloon_alert_to_viewers(world, camera, "в эфире!", None, &[]);
}

/// `on_deactivating`.
fn go_offline(world: &mut World, camera: Entity) {
    let Some(mut state) = world.get_mut::<BroadcastCamera>(camera) else {
        return;
    };
    if !state.active {
        return;
    }
    state.active = false;
    world.entity_mut(camera).remove::<Radio>();
    crate::obj_damage::set_icon_state(world, camera, "broadcast_cam0");
    sound(world, camera, OFF_SOUND);
    crate::balloon::balloon_alert_to_viewers(world, camera, "не в эфире", None, &[]);
}

fn sound(world: &mut World, camera: Entity, file: &str) {
    if let Some(pos) = crate::explosions::turf_of(world, camera) {
        playsound(
            world,
            PlaySound::new(pos, file, 20.0, false).ignore_walls(false),
        );
    }
}

/// `dropped`, `on_enter_storage`: a live camera out of its holder's hands goes offline.
fn go_offline_out_of_hands(world: &mut World) {
    let mut q = world.query::<(Entity, &BroadcastCamera)>();
    let live: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.active)
        .map(|(e, _)| e)
        .collect();
    for camera in live {
        if holder(world, camera).is_none() {
            go_offline(world, camera);
        }
    }
}

/// `click_alt`: the microphone on or off. True when `camera` is a broadcast camera.
pub fn click_alt(world: &mut World, mob: Entity, camera: Entity) -> bool {
    let Some(mut state) = world.get_mut::<BroadcastCamera>(camera) else {
        return false;
    };
    state.microphone = !state.microphone;
    let (on, active) = (state.microphone, state.active);
    if active && let Some(mut radio) = world.get_mut::<Radio>(camera) {
        radio.broadcasting = on;
    }
    let text = if on {
        "микрофон включён"
    } else {
        "микрофон выключен"
    };
    crate::balloon::balloon_alert(world, mob, camera, text);
    true
}

/// `attack_self_secondary`: the broadcast's name. True when `camera` is a broadcast camera.
pub fn rename(world: &mut World, client: Entity, mob: Entity, camera: Entity) -> bool {
    let Some(state) = world.get::<BroadcastCamera>(camera).cloned() else {
        return false;
    };
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Название трансляции",
            "Как будет называться ваша трансляция?",
            PromptKind::Text {
                default: state.name,
                max_length: NAME_MAX,
                multiline: false,
            },
        ),
    );
    world.entity_mut(prompt).insert(RenamePrompt(camera));
    true
}

/// The new name, if the camera is still in the actor's hands and the answer is not empty.
fn rename_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(&RenamePrompt(camera)) = world.get::<RenamePrompt>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Text(name)) = answer else {
            continue;
        };
        let name: String = name.trim().chars().take(NAME_MAX as usize).collect();
        if name.is_empty() || holder(world, camera) != Some(actor) {
            continue;
        }
        if let Some(mut state) = world.get_mut::<BroadcastCamera>(camera) {
            state.name = name;
        }
    }
}

/// `examine`: the broadcast's name and the microphone.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(state) = world.get::<BroadcastCamera>(target) else {
        return;
    };
    lines.push((
        Span::Notice,
        format!("Название трансляции: {}.", state.name),
    ));
    lines.push((
        Span::Notice,
        format!(
            "Микрофон {}.",
            if state.microphone {
                "включён"
            } else {
                "выключен"
            }
        ),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live microphone is a broadcasting, deaf entertainment radio within 7 tiles.
    #[test]
    fn the_microphone_broadcasts_on_entertainment() {
        let mic = microphone(true);
        assert!(mic.broadcasting && !mic.listening && mic.on);
        assert_eq!(mic.frequency, ss13_chat::radio::FREQ_ENTERTAINMENT);
        assert_eq!(mic.canhear_range, 7);
        assert!(!microphone(false).broadcasting);
    }
}
