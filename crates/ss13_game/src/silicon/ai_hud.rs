//! The AI's and cyborgs' HUD buttons (server), after tg `code/_onclick/hud/screen_objects/ai.dm`
//! and `robot.dm`'s alert panel, as action buttons (`crate::actions`):
//!
//! - AI core (`view_core`): the eye back on the core.
//! - Show Camera List (`show_camera_list`): the `ss13` network's cameras by tag; the eye jumps
//!   to the chosen one (`switchCamera`).
//! - Show Alerts: the station alerts window (`crate::silicon::alarms`); cyborgs have it too.
//! - Make Vox Announcement (`announcement`, `ai_say.dm`): a text prompt; up to 30 words, each
//!   a VOX word, every 60 s station-wide (one cooldown for all AIs); the text goes to the players
//!   on the AI's z-level as a minor announcement, then the words play to them one after the
//!   other (tg queues them on `CHANNEL_VOX`; here each is sent when the previous ends). Without
//!   VOX words in the assets the announcement is text only and every word passes.
//! - Call Emergency Shuttle (`ai_call_shuttle`): a reason prompt, then `SSshuttle.requestEvac`.
//! - Change core display (the `ai_core_display_picker` verb): tg's display names; the core
//!   shows `ai-<name>` (dead: `<state>_dead`, else `ai_dead`).
//!
//! The buttons do nothing while the AI is incapacitated (dead or depowered); the AI core
//! button works anyway, as tg's does not call the parent click.
// shortcut: no camera tracking, camera light, crew monitor, manifest, messenger, laws stating,
// photos, sensors, multicam or z-level buttons; the display picker is a list without previews;
// no `Announcement Help` word list and no BandaStation VOX voices (HL, BMS and military: Valve
// and Crowbar Collective assets, not freely licensed), no per-player VOX volume.

use std::collections::VecDeque;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::sound::server::SendSound;
use ss13_shared::chat::Span;
use ss13_shared::protocol::SoundEvent;
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{UiActed, UiSystems};

use super::ai::{AiCore, AiEye};
use super::alarms::ask_camera;
use super::cameras::Camera;
use super::cyborg::Cyborg;
use super::notice;
use crate::actions::{ActionButton, ActionButtonsUi, set_buttons};
use crate::comms_server::MAX_MESSAGE_LEN;
use crate::health::Stat;
use crate::server::PlayerMob;
use crate::shuttle::CALL_SHUTTLE_REASON_LENGTH;
use crate::sound::Sfx;

const SCREEN_AI: &str = "icons/hud/screen_ai.dmi";
pub const AI_DMI: &str = "icons/mob/silicon/ai.dmi";
/// tg `VOX_DELAY`.
pub const VOX_DELAY: f64 = 60.0;
/// tg `announcement()`: `words.len = 30`.
pub const VOX_MAX_WORDS: usize = 30;
/// The AI's camera network (`ai_defines.dm` `network`).
const AI_NETWORK: &str = "ss13";

const CORE: &str = "ai_core";
const CAMERA_LIST: &str = "camera";
const CAMERA_TRACK: &str = "camera_track";
const ALERTS: &str = "alerts";
const ANNOUNCEMENT: &str = "announcement";
const CALL_SHUTTLE: &str = "call_shuttle";
const CORE_DISPLAY: &str = "core_display";

/// tg `GLOB.ai_core_display_screens` (sorted).
pub const CORE_DISPLAYS: [&str; 39] = [
    ":thinking:",
    "Alien",
    "Angel",
    "Banned",
    "Bliss",
    "Blue",
    "Clown",
    "Database",
    "Dorf",
    "Firewall",
    "Fuzzy",
    "Gentoo",
    "Glitchman",
    "Gondola",
    "Goon",
    "Hades",
    "HAL 9000",
    "Heartline",
    "Helios",
    "House",
    "Inverted",
    "Matrix",
    "Monochrome",
    "Murica",
    "Nanotrasen",
    "Not Malf",
    "Portrait",
    "President",
    "Rainbow",
    "Random",
    "Red",
    "Red October",
    "Static",
    "Syndicat Meow",
    "Text",
    "Too Deep",
    "Triumvirate",
    "Triumvirate-M",
    "Weird",
];

/// The station-wide VOX cooldown (tg `announcing_vox`, a `var/static`) and the words playing.
#[derive(Resource, Default)]
pub struct Vox {
    pub ready_at: f64,
    playing: Vec<VoxPlayback>,
}

/// One announcement's words still to play to `clients`.
struct VoxPlayback {
    clients: Vec<Entity>,
    words: VecDeque<(String, f32)>,
    next_at: f64,
}

/// On the AI's eye: tg `last_announcement`.
#[derive(Component, Clone, Debug, Default)]
pub struct LastAnnouncement(pub String);

/// The prompts the buttons ask, on their prompt entity.
#[derive(Component, Clone, Debug)]
enum HudPrompt {
    Announcement { eye: Entity },
    Shuttle { eye: Entity },
    Display { eye: Entity },
}

pub struct AiHudPlugin;

impl Plugin for AiHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Vox>().add_systems(
            Update,
            (
                trigger.in_set(UiSystems::Handle),
                answered
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
                play_vox,
                core_looks,
            ),
        );
    }
}

fn icon(world: &World, dmi: &str, state: &str) -> Option<IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(dmi, state))
}

fn button(world: &World, id: &str, name: &str, dmi: &str, state: &str) -> (ActionButton, f64) {
    (
        ActionButton {
            id: id.to_owned(),
            name: name.to_owned(),
            icon: icon(world, dmi, state),
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

/// The AI eye's buttons (tg `/datum/hud/ai`).
pub(crate) fn give_ai_buttons(world: &mut World, eye: Entity) {
    let buttons = vec![
        button(world, CORE, "AI core", SCREEN_AI, "ai_core"),
        button(world, CAMERA_LIST, "Show Camera List", SCREEN_AI, "camera"),
        button(world, CAMERA_TRACK, "Track With Camera", SCREEN_AI, "track"),
        button(world, ALERTS, "Show Alerts", SCREEN_AI, "alerts"),
        button(
            world,
            ANNOUNCEMENT,
            "Make Vox Announcement",
            SCREEN_AI,
            "announcement",
        ),
        button(
            world,
            CALL_SHUTTLE,
            "Call Emergency Shuttle",
            SCREEN_AI,
            "call_shuttle",
        ),
        button(world, CORE_DISPLAY, "Change Core Display", AI_DMI, "ai"),
    ];
    world.entity_mut(eye).insert(LastAnnouncement::default());
    set_buttons(world, eye, buttons);
}

/// A cyborg's alert panel (tg `/atom/movable/screen/robot/alerts`).
pub(crate) fn give_borg_buttons(world: &mut World, borg: Entity) {
    let buttons = vec![button(world, ALERTS, "Alert Panel", SCREEN_AI, "alerts")];
    set_buttons(world, borg, buttons);
}

fn warn(world: &mut World, client: Entity, text: impl Into<String>) {
    notice(world, client, Span::Warning, text);
}

fn trigger(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        if world.get::<Cyborg>(actor).is_some() {
            if id == ALERTS {
                super::alarms::open(world, client, actor, actor);
            }
            continue;
        }
        let Some(core) = world.get::<AiEye>(actor).map(|e| e.ai) else {
            continue;
        };
        if id == CORE {
            if let Some(&pos) = world.get::<TilePos>(core) {
                world.entity_mut(actor).insert(pos);
            }
            continue;
        }
        let incapacitated = world.get::<Stat>(core) != Some(&Stat::Conscious)
            || super::ai_power::depowered(world, core);
        if incapacitated {
            continue;
        }
        match id.as_str() {
            CAMERA_LIST => {
                let mut cameras = world.query::<(Entity, &Camera)>();
                let list: Vec<Entity> = cameras
                    .iter(world)
                    .filter(|(_, c)| c.network.iter().any(|n| n == AI_NETWORK))
                    .map(|(e, _)| e)
                    .collect();
                ask_camera(world, client, actor, list, true);
            }
            CAMERA_TRACK => super::ai_track::ask(world, client, actor),
            ALERTS => super::alarms::open(world, client, actor, actor),
            ANNOUNCEMENT => ask_announcement(world, client, actor),
            CALL_SHUTTLE => ask_shuttle(world, client, actor, core),
            CORE_DISPLAY => {
                let prompt = ss13_tgui::prompt::ask_world(
                    world,
                    Ask::new(
                        client,
                        actor,
                        "Core display",
                        "Choose your core display",
                        PromptKind::List {
                            items: CORE_DISPLAYS.map(str::to_owned).to_vec(),
                            default: None,
                        },
                    ),
                );
                world
                    .entity_mut(prompt)
                    .insert(HudPrompt::Display { eye: actor });
            }
            _ => {}
        }
    }
}

/// tg `DisplayTimeText` for whole seconds.
fn display_time(secs: f64) -> String {
    let secs = secs.ceil().max(1.0) as u64;
    if secs == 1 {
        "1 second".to_owned()
    } else {
        format!("{secs} seconds")
    }
}

fn ask_announcement(world: &mut World, client: Entity, eye: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let ready_at = world.resource::<Vox>().ready_at;
    if ready_at > now {
        let text = format!("Please wait {}.", display_time(ready_at - now));
        notice(world, client, Span::Notice, text);
        return;
    }
    let default = world
        .get::<LastAnnouncement>(eye)
        .map(|l| l.0.clone())
        .unwrap_or_default();
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            eye,
            "Announcement",
            "WARNING: Misuse of this verb can result in you being job banned. More help is \
             available in 'Announcement Help'",
            PromptKind::Text {
                default,
                max_length: MAX_MESSAGE_LEN as u32,
                multiline: false,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(HudPrompt::Announcement { eye });
}

fn ask_shuttle(world: &mut World, client: Entity, eye: Entity, core: Entity) {
    if world
        .get::<AiCore>(core)
        .is_some_and(|c| c.control_disabled)
    {
        warn(world, client, "Wireless control is disabled!");
        return;
    }
    if let Some(Err(why)) = crate::comms_server::can_evac(world) {
        warn(world, client, why);
        return;
    }
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            eye,
            "Confirm Shuttle Call",
            format!(
                "What is the nature of your emergency? ({CALL_SHUTTLE_REASON_LENGTH} characters \
                 required.)"
            ),
            PromptKind::Text {
                default: String::new(),
                max_length: MAX_MESSAGE_LEN as u32,
                multiline: false,
            },
        ),
    );
    world.entity_mut(prompt).insert(HudPrompt::Shuttle { eye });
}

fn answered(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in pending {
        let Some(asked) = world.get::<HudPrompt>(prompt).cloned() else {
            continue;
        };
        match (asked, answer) {
            (HudPrompt::Announcement { eye }, Some(PromptAnswer::Text(text))) => {
                announce(world, client, eye, &text);
            }
            (HudPrompt::Shuttle { eye }, Some(PromptAnswer::Text(reason))) => {
                // tg: `if(incapacitated) return`, then `requestEvac` for a non-blank reason.
                if !incapacitated(world, eye) && !reason.trim().is_empty() {
                    crate::comms_server::request_evac(world, client, eye, &reason);
                }
            }
            (HudPrompt::Display { eye }, Some(PromptAnswer::Item(i))) => {
                if let Some(choice) = CORE_DISPLAYS.get(i)
                    && !incapacitated(world, eye)
                {
                    set_core_display(world, eye, choice);
                }
            }
            _ => {}
        }
    }
}

fn incapacitated(world: &World, eye: Entity) -> bool {
    let Some(core) = world.get::<AiEye>(eye).map(|e| e.ai) else {
        return true;
    };
    world.get::<Stat>(core) != Some(&Stat::Conscious) || super::ai_power::depowered(world, core)
}

/// tg `announcement()` after its prompt. Returns whether it was made.
pub fn announce(world: &mut World, client: Entity, eye: Entity, message: &str) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if message.is_empty() || world.resource::<Vox>().ready_at > now {
        return false;
    }
    if let Some(mut last) = world.get_mut::<LastAnnouncement>(eye) {
        last.0 = message.to_owned();
    }
    if incapacitated(world, eye) {
        return false;
    }
    let Some(core) = world.get::<AiEye>(eye).map(|e| e.ai) else {
        return false;
    };
    if world
        .get::<AiCore>(core)
        .is_some_and(|c| c.control_disabled)
    {
        warn(
            world,
            client,
            "Wireless interface disabled, unable to interact with announcement PA.",
        );
        return false;
    }
    let vox = world
        .get_resource::<Sfx>()
        .map(|s| s.0.vox.clone())
        .unwrap_or_default();
    let words = match vox_words(message, &vox) {
        Ok(words) => words,
        Err(wrong) => {
            notice(
                world,
                client,
                Span::Notice,
                format!(
                    "These words are not available on the announcement system: {}.",
                    english_list(&wrong)
                ),
            );
            return false;
        }
    };
    world.resource_mut::<Vox>().ready_at = now + VOX_DELAY;
    let name = world
        .get::<Name>(core)
        .map_or_else(String::new, |n| n.as_str().to_owned());
    info!(target: "say", "{name} made a vocal announcement: {message}");
    let Some(z) = world.get::<TilePos>(core).map(|p| p.z) else {
        return false;
    };
    let mut players = world.query::<(Entity, &PlayerMob, &TilePos)>();
    let on_z: Vec<(Entity, Entity)> = players
        .iter(world)
        .filter(|(.., p)| p.z == z)
        .map(|(e, p, _)| (e, p.client))
        .collect();
    crate::security_level::minor_announce_silent_to(
        world,
        &format!("{name} объявляет"),
        &capitalize(message),
        on_z.iter().map(|(mob, _)| *mob).collect(),
    );
    let clients: Vec<Entity> = on_z
        .into_iter()
        .filter(|(mob, _)| !crate::sound::mob_is_deaf(world, *mob))
        .map(|(_, client)| client)
        .collect();
    let words: VecDeque<(String, f32)> = words
        .iter()
        .filter_map(|w| vox.get(w).map(|v| (v.file.clone(), v.secs)))
        .collect();
    if !words.is_empty() && !clients.is_empty() {
        world.resource_mut::<Vox>().playing.push(VoxPlayback {
            clients,
            words,
            next_at: now,
        });
    }
    true
}

/// tg's word check: up to [`VOX_MAX_WORDS`] space-separated words, lowercased, blanks dropped,
/// each a VOX word. `Err` lists the unknown ones. Without VOX words every word passes.
pub fn vox_words<V>(
    message: &str,
    vox: &std::collections::BTreeMap<String, V>,
) -> Result<Vec<String>, Vec<String>> {
    let words: Vec<String> = message
        .trim()
        .split(' ')
        .take(VOX_MAX_WORDS)
        .map(|w| w.trim().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    if vox.is_empty() {
        return Ok(words);
    }
    let wrong: Vec<String> = words
        .iter()
        .filter(|w| !vox.contains_key(*w))
        .cloned()
        .collect();
    if wrong.is_empty() {
        Ok(words)
    } else {
        Err(wrong)
    }
}

/// BYOND `english_list`.
fn english_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Each announcement's next word when the previous one has played.
// Cost: one entry per running announcement (at most one a minute), a sound per listener per
// word.
fn play_vox(mut vox: ResMut<Vox>, time: Res<Time>, mut sounds: MessageWriter<SendSound>) {
    if vox.playing.is_empty() {
        return;
    }
    let now = time.elapsed_secs_f64();
    for playback in &mut vox.playing {
        while playback.next_at <= now
            && let Some((file, secs)) = playback.words.pop_front()
        {
            playback.next_at += f64::from(secs);
            for &client in &playback.clients {
                sounds.write(SendSound {
                    client,
                    sound: SoundEvent {
                        file: file.clone(),
                        volume: 100.0,
                        speed: 1.0,
                        offset: None,
                    },
                });
            }
        }
    }
    vox.playing.retain(|p| !p.words.is_empty());
}

/// On the AI core: tg `display_icon_override`, the screen state while alive.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct CoreDisplay(pub String);

impl Default for CoreDisplay {
    fn default() -> Self {
        Self("ai".to_owned())
    }
}

/// tg `resolve_ai_icon_sync`: `ai-<lowercase name>`, `Random` any other.
pub fn display_state(choice: &str, random: impl FnOnce(&[&str]) -> usize) -> String {
    let choice = if choice == "Random" {
        let others: Vec<&str> = CORE_DISPLAYS
            .iter()
            .copied()
            .filter(|&d| d != "Random")
            .collect();
        others[random(&others) % others.len()]
    } else {
        choice
    };
    format!("ai-{}", choice.to_lowercase())
}

/// tg `set_core_display_icon`.
fn set_core_display(world: &mut World, eye: Entity, choice: &str) {
    let Some(core) = world.get::<AiEye>(eye).map(|e| e.ai) else {
        return;
    };
    let state = display_state(choice, |list| {
        world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, list.len() as u32 - 1) as usize
    });
    if icon(world, AI_DMI, &state).is_none() {
        return;
    }
    world.entity_mut(core).insert(CoreDisplay(state));
}

/// tg `update_overlays` of the core's screen: the display, or its dead state when dead.
fn core_looks(
    icons: Option<Res<IconIndex>>,
    mut cores: Query<
        (&CoreDisplay, &Stat, &mut Appearance),
        (With<AiCore>, Or<(Changed<CoreDisplay>, Changed<Stat>)>),
    >,
) {
    let Some(icons) = icons else { return };
    for (display, stat, mut appearance) in &mut cores {
        let id = if *stat == Stat::Dead {
            icons
                .get(AI_DMI, &format!("{}_dead", display.0))
                .or_else(|| icons.get(AI_DMI, "ai_dead"))
        } else {
            icons.get(AI_DMI, &display.0)
        };
        if let Some(id) = id
            && appearance.icon_state != id
        {
            appearance.icon_state = id;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn vox_words_are_checked_lowercased_and_capped() {
        let vox: BTreeMap<String, ()> = ["alert", "red", "."]
            .into_iter()
            .map(|w| (w.to_owned(), ()))
            .collect();
        assert_eq!(
            vox_words("  Alert  red . ", &vox),
            Ok(vec!["alert".into(), "red".into(), ".".into()])
        );
        assert_eq!(
            vox_words("alert blue green", &vox),
            Err(vec!["blue".into(), "green".into()])
        );
        let long = vec!["red"; 40].join(" ");
        assert_eq!(vox_words(&long, &vox).unwrap().len(), VOX_MAX_WORDS);
        // No VOX words in the assets: text only, anything goes.
        assert!(vox_words("anything", &BTreeMap::<String, ()>::new()).is_ok());
        assert_eq!(
            english_list(&["a".into(), "b".into(), "c".into()]),
            "a, b, and c"
        );
    }

    #[test]
    fn display_states_follow_tg_names() {
        assert_eq!(display_state("HAL 9000", |_| 0), "ai-hal 9000");
        assert_eq!(display_state("Blue", |_| 0), "ai-blue");
        let random = display_state("Random", |_| 0);
        assert_eq!(random, "ai-:thinking:");
        assert!(CORE_DISPLAYS.is_sorted_by(|a, b| a.to_lowercase() <= b.to_lowercase()));
    }
}
