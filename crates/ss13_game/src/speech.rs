//! The mob's stat for speech (server): `ss13_chat` reads [`SpeakerStat`] to whisper from soft
//! crit, silence hard crit, send the dead and ghosts to deadchat and check emotes. A mob knocked
//! out by `Unconscious` or `Sleeping` is [`SpeakerStat::Unconscious`] (tg stat `UNCONSCIOUS`).
//!
//! Emotes: the body's [`EmoteUser`] (its tg mob type, BandaStation species, physique and
//! `TRAIT_SILICON_EMOTES_ALLOWED`) is kept here for `ss13_chat::emote`, and an emote that ran
//! ([`Emoted`]) plays its sound (tg `playsound` in `run_emote`), sets the AI status displays'
//! emotion (`apply_emote_display`) or a slime's face (`current_mood`).
//!
//! BandaStation's speech filter (`ss13_chat::speech_filter`): `load_speech_filter` reads its
//! config, a filtered speaker drools and the admins are told (`speech_filtered`).
// shortcut: no AI status display draws [`AiDisplayEmotion`] and no slime face overlay draws
// [`SlimeMood`] yet (`docs/parity/comms/station-info.md`, xenobiology); `friendcomputer` does
// not post to the evac displays.

use bevy::prelude::*;
use ss13_chat::emote::{EmoteMob, EmoteSpecies, EmoteUser};
use ss13_chat::language::Tongue;
use ss13_chat::server::Emoted;
use ss13_chat::speech::SpeakerStat;
use ss13_chat::speech_filter::{SpeechFilter, SpeechFiltered};
use ss13_mapload::DmObj;
use ss13_shared::TilePos;

use crate::bots::Bot;
use crate::ghost::Ghost;
use crate::health::{HealthSystems, Stat};
use crate::human::HumanLook;
use crate::medical::brainmob::BrainMob;
use crate::mobs::BasicMob;
use crate::science::xenobio::Slime;
use crate::silicon::ai::AiEye;
use crate::silicon::cyborg::Cyborg;
use crate::sound::{PlaySound, SoundRng};
use crate::species::{Species, SpeciesId};
use crate::status_effects::Incapacitation;

pub struct SpeechServerPlugin;

impl Plugin for SpeechServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Emoted>()
            .add_message::<PlaySound>()
            .add_message::<ss13_net::animation::server::PlayAnimation>()
            .init_resource::<AiDisplayEmotion>()
            .init_resource::<SoundRng>()
            .add_message::<SpeechFiltered>()
            .add_systems(Startup, load_speech_filter)
            .add_systems(
                Update,
                (
                    sync_speaker_stat.after(HealthSystems),
                    sync_emote_user,
                    emote_effects.after(ss13_chat::server::SaySystems),
                    speech_filtered.after(ss13_chat::server::SaySystems),
                ),
            )
            .add_observer(ghost_speaks_dead);
    }
}

/// tg `apply_emote_display`: the `AI_EMOTION_*` every `/obj/machinery/status_display/ai` shows.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AiDisplayEmotion(pub Option<&'static str>);

/// tg `/mob/living/basic/slime` `current_mood` (the face over the slime; `None`: no face).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlimeMood(pub Option<&'static str>);

/// What kind of body runs emotes, from its components.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmoteBody<'a> {
    Human,
    Cyborg,
    AiEye,
    Brain {
        in_processor: bool,
    },
    Slime,
    Bot,
    /// A basic mob of this DM path.
    Basic(&'a str),
}

/// The species whose emotes or tongue sounds BandaStation changes (subtypes included).
pub fn emote_species(id: SpeciesId) -> EmoteSpecies {
    match id {
        SpeciesId::Vulpkanin => EmoteSpecies::Vulpkanin,
        SpeciesId::Tajaran => EmoteSpecies::Tajaran,
        SpeciesId::Skrell => EmoteSpecies::Skrell,
        SpeciesId::Lizard | SpeciesId::Ashwalker | SpeciesId::Silverscale => EmoteSpecies::Lizard,
        SpeciesId::Moth => EmoteSpecies::Moth,
        SpeciesId::Pod => EmoteSpecies::Pod,
        SpeciesId::Jelly | SpeciesId::Slime | SpeciesId::Luminescent | SpeciesId::Stargazer => {
            EmoteSpecies::Jelly
        }
        _ => EmoteSpecies::Other,
    }
}

/// The [`EmoteUser`] of a body: tg's mob type checks and `TRAIT_SILICON_EMOTES_ALLOWED`
/// (silicons, drones, bots, orbies, a robotic voicebox, a brain processor).
pub fn emote_user(
    body: EmoteBody,
    species: Option<SpeciesId>,
    female: bool,
    robot_tongue: bool,
) -> EmoteUser {
    let under = |path: &str, root: &str| {
        path.strip_prefix(root)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    };
    let (mob, silicon_emotes, robotic, in_processor) = match body {
        EmoteBody::Human => (EmoteMob::Human, robot_tongue, false, false),
        EmoteBody::Cyborg => (EmoteMob::Cyborg, true, true, false),
        EmoteBody::AiEye => (EmoteMob::Ai, true, true, false),
        EmoteBody::Brain { in_processor } => (EmoteMob::Brain, in_processor, false, in_processor),
        EmoteBody::Slime => (EmoteMob::Slime, false, false, false),
        EmoteBody::Bot => (EmoteMob::Basic, true, true, false),
        EmoteBody::Basic(path) if under(path, "/mob/living/basic/gorilla") => {
            (EmoteMob::Gorilla, false, false, false)
        }
        EmoteBody::Basic(path) => {
            let drone = under(path, "/mob/living/basic/drone");
            let orbie = under(path, "/mob/living/basic/orbie");
            (EmoteMob::Basic, drone || orbie, drone, false)
        }
    };
    EmoteUser {
        mob,
        species: if mob == EmoteMob::Human {
            species.map_or(EmoteSpecies::Other, emote_species)
        } else {
            EmoteSpecies::Other
        },
        female: female && mob == EmoteMob::Human,
        silicon_emotes,
        robotic,
        in_processor,
        tail: None,
    }
}

type EmoteBodyQuery<'a> = (
    Entity,
    Option<&'a Species>,
    Option<&'a HumanLook>,
    Option<&'a Tongue>,
    Has<Cyborg>,
    Has<AiEye>,
    Option<&'a BrainMob>,
    Has<Slime>,
    Option<&'a BasicMob>,
    Has<Bot>,
    Option<&'a EmoteUser>,
);

/// Follows the components that decide a body's emotes (spawns, species and tongue changes).
fn sync_emote_user(
    bodies: Query<
        EmoteBodyQuery,
        Or<(
            Changed<Species>,
            Changed<HumanLook>,
            Changed<Tongue>,
            Added<Cyborg>,
            Added<AiEye>,
            Changed<BrainMob>,
            Added<Slime>,
            Added<BasicMob>,
            Added<Bot>,
        )>,
    >,
    objs: Query<&DmObj>,
    mut commands: Commands,
) {
    for (e, species, look, tongue, cyborg, eye, brain, slime, basic, bot, current) in &bodies {
        let body = if cyborg {
            EmoteBody::Cyborg
        } else if eye {
            EmoteBody::AiEye
        } else if let Some(brain) = brain {
            let in_processor = objs.get(brain.container).is_ok_and(|o| {
                o.path
                    .strip_prefix(crate::silicon::mmi::PROCESSOR)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            });
            EmoteBody::Brain { in_processor }
        } else if slime {
            EmoteBody::Slime
        } else if bot {
            EmoteBody::Bot
        } else if let Some(basic) = basic {
            EmoteBody::Basic(&basic.def.path)
        } else {
            EmoteBody::Human
        };
        let wanted = EmoteUser {
            // The drawn tail is the wag-able one (`crate::medical::organs::tail_sprite`).
            tail: look
                .and_then(|l| l.tail)
                .filter(|_| body == EmoteBody::Human)
                .map(|t| t.wagging),
            ..emote_user(
                body,
                species.map(|s| s.0),
                look.is_some_and(|l| l.female),
                tongue == Some(&Tongue::Robot),
            )
        };
        if current != Some(&wanted) {
            commands.entity(e).try_insert(wanted);
        }
    }
}

/// tg `run_emote`'s `playsound(user, get_sound(user), sound_volume, vary, ignore_walls)` and the
/// effects of the AI and slime emotes.
fn emote_effects(
    mut emotes: MessageReader<Emoted>,
    positions: Query<&TilePos>,
    eyes: Query<&AiEye>,
    slimes: Query<(), With<Slime>>,
    mut rng: ResMut<SoundRng>,
    mut display: ResMut<AiDisplayEmotion>,
    mut sounds: MessageWriter<PlaySound>,
    mut animations: MessageWriter<ss13_net::animation::server::PlayAnimation>,
    mut commands: Commands,
) {
    for emoted in emotes.read() {
        // The AI mob is the core; its client emotes through the eye.
        let user = eyes
            .get(emoted.speaker)
            .map_or(emoted.speaker, |eye| eye.ai);
        if let Some(sound) = emoted.sound
            && !sound.files.is_empty()
            && let Ok(&pos) = positions.get(user)
        {
            let file = sound.files[rng.0.range(0, sound.files.len() as u32 - 1) as usize];
            sounds.write(
                PlaySound::new(pos, file, sound.volume, sound.vary)
                    .ignore_walls(sound.ignore_walls),
            );
        }
        // `/datum/emote/jump/jump_animation`: 4 px up over 0.1 s and back.
        if emoted.key == "jump" {
            use ss13_shared::animation::{Animation, AnimationStep, TransformAnimation};
            let up = AnimationStep {
                offset: [0.0, 4.0],
                ..AnimationStep::rest(1.0)
            };
            animations.write(ss13_net::animation::server::PlayAnimation {
                entity: user,
                animation: Animation::Transform(TransformAnimation {
                    steps: vec![up, AnimationStep::rest(1.0)],
                    loops: 1,
                }),
            });
        }
        if eyes.contains(emoted.speaker)
            && let Some(emotion) = ss13_chat::emote::ai_emotion(emoted.key)
        {
            display.0 = Some(emotion);
        }
        if slimes.contains(emoted.speaker)
            && let Some(mood) = ss13_chat::emote::slime_mood_key(emoted.key)
        {
            commands.entity(emoted.speaker).try_insert(SlimeMood(mood));
        }
    }
}

/// tg `mob.emote(key)` from code (`intentional = FALSE`): the same emote a player's `*key`
/// runs, so it has its sound, its overhead text and the checks of `run_emote` (stat, mob type,
/// muteness, a muzzle turning it into a noise), without the cooldowns (tg checks them only for
/// intentional emotes). False when the mob cannot run it.
pub fn run_emote(world: &mut World, mob: Entity, key: &str) -> bool {
    use bevy::ecs::message::Messages;
    use ss13_chat::emote::{AUDIBLE, EmoteOutcome, REQUIRE_HANDS};
    use ss13_chat::server::{Overhead, OverheadKind};
    let stat = world
        .get::<SpeakerStat>(mob)
        .copied()
        .or_else(|| world.get::<Stat>(mob).map(|&s| speaker_stat(s)))
        .unwrap_or_default();
    let user = world.get::<EmoteUser>(mob).copied().unwrap_or_default();
    let EmoteOutcome::Run { key, text, sound } =
        ss13_chat::emote::run(key, stat, &user, |_, _| false)
    else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    let flags = ss13_chat::emote::find(key).map_or(0, |e| e.flags);
    let audible = flags & AUDIBLE != 0;
    // `muffles_speech/emote_override`: an audible emote without hands is a noise.
    let muffled = audible
        && flags & REQUIRE_HANDS == 0
        && world.get::<ss13_chat::speech::Muffled>(mob).is_some();
    let silenced = world
        .get::<ss13_chat::speech::Impediments>(mob)
        .is_some_and(|i| i.mute)
        || world.get::<ss13_chat::speech::Tongueless>(mob).is_some();
    let text = if muffled {
        let strength = ["сильный ", "слабый ", ""];
        let pick = world.get_resource_or_init::<SoundRng>().0.range(0, 2) as usize;
        format!("издаёт {}звук.", strength[pick])
    } else {
        text
    };
    if !text.is_empty() {
        let name = crate::mobs::defense::name_of(world, mob);
        let clients = crate::obj_damage::clients_in_range(
            world,
            pos,
            crate::health::DEFAULT_MESSAGE_RANGE,
            &[],
        );
        if world.contains_resource::<Messages<Overhead>>() {
            for &client in &clients {
                world.write_message(Overhead {
                    client,
                    speaker: mob,
                    name: name.clone(),
                    text: text.clone(),
                    kind: OverheadKind::Emote,
                });
            }
        }
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: ss13_chat::server::emote_message(&name, &text),
        });
    }
    if muffled {
        return true;
    }
    let sound = sound.filter(|s| !(s.audible && silenced));
    if world.contains_resource::<Messages<Emoted>>() {
        world.write_message(Emoted {
            speaker: mob,
            key,
            sound,
        });
    }
    true
}

pub fn speaker_stat(stat: Stat) -> SpeakerStat {
    match stat {
        Stat::Conscious => SpeakerStat::Conscious,
        Stat::SoftCrit => SpeakerStat::SoftCrit,
        Stat::HardCrit => SpeakerStat::HardCrit,
        Stat::Dead => SpeakerStat::Dead,
    }
}

/// Follows stat and knock-out changes (and spawns) only.
fn sync_speaker_stat(
    mut mobs: Query<
        (
            Entity,
            &Stat,
            Option<&Incapacitation>,
            Option<&mut SpeakerStat>,
        ),
        Or<(Changed<Stat>, Changed<Incapacitation>)>,
    >,
    mut commands: Commands,
) {
    for (mob, &stat, traits, speaker) in &mut mobs {
        let mut wanted = speaker_stat(stat);
        if traits.is_some_and(|t| t.knocked_out) && wanted < SpeakerStat::Unconscious {
            wanted = SpeakerStat::Unconscious;
        }
        match speaker {
            Some(mut speaker) => {
                speaker.set_if_neq(wanted);
            }
            None => {
                commands.entity(mob).insert(wanted);
            }
        }
    }
}

fn ghost_speaks_dead(add: On<Add<Ghost>>, mut commands: Commands) {
    commands.entity(add.entity).try_insert(SpeakerStat::Dead);
}

/// BandaStation's speech filter config, as the environment gives it: `SS13_SPEECH_FILTER=1`
/// (`ENABLE_SPEECH_FILTER`, off by default as in BandaStation's config),
/// `SS13_SPEECH_FILTER_BYPASS=key,...` (`SPEECH_FILTER_BYPASS`) and the `brainrot_filter`
/// regexes of `<SS13_CONFIG_DIR>/bandastation/brainrot_filter.json` (no file: no filter).
fn load_speech_filter(mut commands: Commands) {
    if !std::env::var("SS13_SPEECH_FILTER").is_ok_and(|v| v == "1") {
        return;
    }
    let dir = std::env::var("SS13_CONFIG_DIR")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "config".to_owned());
    let path = std::path::Path::new(&dir).join("bandastation/brainrot_filter.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let json: serde_json::Value = match serde_json::from_str(&text) {
        Ok(json) => json,
        Err(e) => {
            warn!(
                "speech filter: JSON parsing failure for {}: {e}",
                path.display()
            );
            return;
        }
    };
    let filters: Vec<String> = json["brainrot_filter"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let bypass: Vec<String> = std::env::var("SS13_SPEECH_FILTER_BYPASS")
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    match SpeechFilter::new(&filters, &bypass) {
        Ok(filter) => commands.insert_resource(filter),
        Err(e) => warn!("speech filter: {}: {e}", path.display()),
    }
}

/// `filter_speech` after the words were replaced: the speaker drools, the admins and the game
/// log get what was typed.
// shortcut: the drool comes at once, not 0.3 s later.
fn speech_filtered(mut filtered: MessageReader<SpeechFiltered>, mut commands: Commands) {
    for event in filtered.read() {
        let SpeechFiltered {
            client,
            speaker,
            original,
        } = event.clone();
        commands.queue(move |world: &mut World| {
            let name = crate::security::name_of(world, speaker);
            let key = world
                .get::<ss13_net::server::ClientToken>(client)
                .map_or_else(String::new, |t| t.0.clone());
            let text = format!(
                "{name} ({key}) has attempted to say forbidden word. His message was: {original}"
            );
            info!(target: "game", "{text}");
            crate::chem_machines::admin_notice(world, text);
            crate::medical::diseases::emote(world, speaker, "drool");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_get_their_emotes() {
        let borg = emote_user(EmoteBody::Cyborg, None, false, false);
        assert_eq!((borg.mob, borg.silicon_emotes), (EmoteMob::Cyborg, true));
        assert_eq!(
            emote_user(EmoteBody::AiEye, None, false, false).mob,
            EmoteMob::Ai
        );
        let vulp = emote_user(EmoteBody::Human, Some(SpeciesId::Vulpkanin), true, false);
        assert_eq!(
            (vulp.species, vulp.female, vulp.silicon_emotes),
            (EmoteSpecies::Vulpkanin, true, false)
        );
        assert_eq!(
            emote_user(EmoteBody::Human, Some(SpeciesId::Stargazer), false, false).species,
            EmoteSpecies::Jelly
        );
        assert_eq!(
            emote_user(EmoteBody::Human, Some(SpeciesId::Ashwalker), false, false).species,
            EmoteSpecies::Lizard
        );
        // A robotic voicebox gives silicon emotes.
        assert!(emote_user(EmoteBody::Human, None, false, true).silicon_emotes);
        let boxed = emote_user(EmoteBody::Brain { in_processor: true }, None, false, false);
        assert!(boxed.in_processor && boxed.silicon_emotes);
        let gorilla = emote_user(
            EmoteBody::Basic("/mob/living/basic/gorilla/hostile"),
            None,
            false,
            false,
        );
        assert_eq!(gorilla.mob, EmoteMob::Gorilla);
        let drone = emote_user(
            EmoteBody::Basic("/mob/living/basic/drone"),
            None,
            false,
            false,
        );
        assert!(drone.silicon_emotes && drone.robotic);
        let carp = emote_user(
            EmoteBody::Basic("/mob/living/basic/carp"),
            None,
            true,
            false,
        );
        assert_eq!((carp.mob, carp.female), (EmoteMob::Basic, false));
    }

    #[test]
    fn a_scream_from_code_is_heard_and_makes_the_human_scream_sound() {
        use bevy::ecs::message::Messages;
        use ss13_net::chat::server::SendChat;

        let mut world = World::new();
        world.init_resource::<Messages<Emoted>>();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<SoundRng>();
        let mob = world
            .spawn((
                TilePos::new(1, 1, 1),
                Stat::Conscious,
                EmoteUser {
                    female: true,
                    ..default()
                },
            ))
            .id();
        assert!(run_emote(&mut world, mob, "scream"));
        let emoted: Vec<Emoted> = world.resource_mut::<Messages<Emoted>>().drain().collect();
        assert_eq!(emoted.len(), 1);
        let sound = emoted[0].sound.expect("a scream has a sound");
        assert!(sound.files.iter().all(|f| f.contains("femalescream")));
        assert_eq!(
            world.resource_mut::<Messages<SendChat>>().drain().count(),
            1
        );

        // Mute: the line, no sound.
        world
            .entity_mut(mob)
            .insert(ss13_chat::speech::Impediments {
                mute: true,
                ..default()
            });
        assert!(run_emote(&mut world, mob, "scream"));
        let emoted: Vec<Emoted> = world.resource_mut::<Messages<Emoted>>().drain().collect();
        assert_eq!(emoted[0].sound, None);

        // Unconscious: no scream at all; a snore goes on.
        world.entity_mut(mob).insert(Stat::HardCrit);
        assert!(!run_emote(&mut world, mob, "scream"));
        world.entity_mut(mob).insert(SpeakerStat::HardCrit);
        assert!(
            !run_emote(&mut world, mob, "snore"),
            "hard crit is not asleep"
        );
        world.entity_mut(mob).insert(SpeakerStat::Unconscious);
        assert!(run_emote(&mut world, mob, "snore"));
        assert!(!run_emote(&mut world, mob, "scream"));
    }
}
