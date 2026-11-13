//! The mob's stat for speech (server): `ss13_chat` reads [`SpeakerStat`] to whisper from soft
//! crit, silence hard crit (but for its last words, cut by [`LastBreath`]), send the dead and
//! ghosts to deadchat and check emotes. A mob knocked out by `Unconscious` or `Sleeping` is
//! [`SpeakerStat::Unconscious`] (tg stat `UNCONSCIOUS`).
//!
//! Emotes: the body's [`EmoteUser`] (its tg mob type, BandaStation species, physique,
//! `TRAIT_SILICON_EMOTES_ALLOWED`, the organs, clothes and traits some emotes need, the mime's
//! vow) is kept here for `ss13_chat::emote`, and an emote that ran ([`Emoted`]) plays its sound
//! (tg `playsound` in `run_emote`), sets the AI status displays' emotion (`apply_emote_display`)
//! or a slime's face (`current_mood`), and makes the `run_emote` overrides' effects
//! ([`apply_emote_effect`]: flips, spins, fainting, sways and twitches). A `flip` or `spin` run
//! while it cools down ([`EmoteSpammed`]) trips or dizzies ([`emote_spammed`]).
//!
//! BandaStation's speech filter (`ss13_chat::speech_filter`): `load_speech_filter` reads its
//! config, a filtered speaker drools and the admins are told (`speech_filtered`).
// shortcut: no AI status display draws [`AiDisplayEmotion`] and no slime face overlay draws
// [`SlimeMood`] yet (`docs/parity/comms/station-info.md`, xenobiology); `friendcomputer` does
// not post to the evac displays.

use bevy::prelude::*;
use ss13_chat::emote::{
    DEFAULT_HAS, EmoteMob, EmoteSpecies, EmoteUser, HAS_CAT, HAS_EYES, HAS_GLASSES, HAS_GLOW,
    HAS_LESSER, HAS_SIMIAN, HAS_TONGUE,
};
use ss13_chat::language::Tongue;
use ss13_chat::server::{EmoteSpammed, Emoted};
use ss13_chat::speech::{LastBreath, SpeakerStat};
use ss13_chat::speech_filter::{SpeechFilter, SpeechFiltered};
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::animation::{Animation, AnimationStep, TransformAnimation};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};

use crate::bots::Bot;
use crate::ghost::Ghost;
use crate::health::{HEALTH_THRESHOLD_DEAD, Health, HealthSystems, Stat};
use crate::human::HumanLook;
use crate::inventory::Inventory;
use crate::medical::brainmob::BrainMob;
use crate::medical::organs::{Organ, OrganHost, Organs, Slot};
use crate::mobs::BasicMob;
use crate::science::xenobio::Slime;
use crate::service::MimeVow;
use crate::silicon::ai::AiEye;
use crate::silicon::cyborg::Cyborg;
use crate::sound::{PlaySound, SoundRng};
use crate::species::{Species, SpeciesId};
use crate::status_effects::{Incap, Incapacitation, Timed};

pub struct SpeechServerPlugin;

impl Plugin for SpeechServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Emoted>()
            .add_message::<EmoteSpammed>()
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
                    sync_last_breath.after(HealthSystems),
                    sync_emote_user,
                    emote_effects.after(ss13_chat::server::SaySystems),
                    spam_effects.after(ss13_chat::server::SaySystems),
                    spin_mobs,
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
        SpeciesId::Human => EmoteSpecies::Human,
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

/// The [`EmoteUser`] of a body: tg's mob type checks, `TRAIT_SILICON_EMOTES_ALLOWED`
/// (silicons, drones, bots, orbies, a robotic voicebox, a brain processor) and the traits of the
/// body's kind: a human's eyes and tongue (until [`organ_has`] knows better), `TRAIT_SIMIAN` and
/// `TRAIT_LESSER_HUMANOID` of monkeys, `TRAIT_SIMIAN` of gorillas, `TRAIT_CAT_EMOTES_ALLOWED` of
/// cats and the felinid tongue, the ethereal glow.
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
    let species_id = species.unwrap_or(SpeciesId::Human);
    let kind_has = match body {
        EmoteBody::Human => match species_id {
            SpeciesId::Monkey => HAS_SIMIAN | HAS_LESSER,
            SpeciesId::Felinid => HAS_CAT,
            // `/obj/item/bodypart/*/ethereal` apply the glow.
            SpeciesId::Ethereal | SpeciesId::Lustrous => HAS_GLOW,
            _ => 0,
        },
        EmoteBody::Basic(path) if under(path, "/mob/living/basic/gorilla") => HAS_SIMIAN,
        EmoteBody::Basic(path) if under(path, "/mob/living/basic/pet/cat") => HAS_CAT,
        _ => 0,
    };
    EmoteUser {
        mob,
        species: if mob == EmoteMob::Human {
            emote_species(species_id)
        } else {
            EmoteSpecies::Other
        },
        female: female && mob == EmoteMob::Human,
        silicon_emotes,
        robotic,
        in_processor,
        tail: None,
        has: DEFAULT_HAS | kind_has,
        mime: false,
    }
}

/// The `HAS_*` bits a carbon's organs give, replacing the eyes, tongue and felinid-tongue bits
/// of `has`: `ORGAN_SLOT_EYES`, `ORGAN_SLOT_TONGUE`, the cat tongue's `organ_traits`.
pub fn organ_has<'a>(has: u16, organs: impl IntoIterator<Item = (Slot, &'a str)>) -> u16 {
    let mut has = has & !(HAS_EYES | HAS_TONGUE | HAS_CAT);
    for (slot, path) in organs {
        match slot {
            Slot::Eyes => has |= HAS_EYES,
            Slot::Tongue => {
                has |= HAS_TONGUE;
                if crate::obj_damage::is_under(path, "/obj/item/organ/tongue/cat") {
                    has |= HAS_CAT;
                }
            }
            _ => {}
        }
    }
    has
}

/// The `HAS_*` bits of worn items: glasses to push up (`glasses/can_run_emote`), kitty ears'
/// `clothing_traits`.
pub fn worn_has(eyes: Option<&str>, head: Option<&str>) -> u16 {
    use crate::obj_damage::is_under;
    let glasses = eyes.is_some_and(|p| {
        is_under(p, "/obj/item/clothing/glasses/regular")
            || is_under(p, "/obj/item/clothing/glasses/sunglasses")
    });
    let kitty = head.is_some_and(|p| is_under(p, "/obj/item/clothing/head/costume/kitty"));
    (if glasses { HAS_GLASSES } else { 0 }) | if kitty { HAS_CAT } else { 0 }
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
    (
        Option<&'a Organs>,
        Has<OrganHost>,
        Option<&'a Inventory>,
        Has<MimeVow>,
    ),
);

/// Follows the components that decide a body's emotes (spawns, species, tongue, organ and
/// clothing changes, the mime's vow).
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
            Changed<Organs>,
            Changed<Inventory>,
            Added<MimeVow>,
        )>,
    >,
    objs: Query<&DmObj>,
    organ_states: Query<&Organ>,
    items: Query<&DmItem>,
    mut commands: Commands,
) {
    for (
        e,
        species,
        look,
        tongue,
        cyborg,
        eye,
        brain,
        slime,
        basic,
        bot,
        current,
        (organs, organ_host, inventory, mime),
    ) in &bodies
    {
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
        let mut wanted = EmoteUser {
            // The drawn tail is the wag-able one (`crate::medical::organs::tail_sprite`).
            tail: look
                .and_then(|l| l.tail)
                .filter(|_| body == EmoteBody::Human)
                .map(|t| t.wagging),
            mime,
            ..emote_user(
                body,
                species.map(|s| s.0),
                look.is_some_and(|l| l.female),
                tongue == Some(&Tongue::Robot),
            )
        };
        if body == EmoteBody::Human {
            if organ_host {
                let slots = organs
                    .into_iter()
                    .flat_map(|o| o.iter())
                    .filter_map(|o| organ_states.get(o).ok())
                    .map(|o| (o.slot, o.path.as_str()));
                wanted.has = organ_has(wanted.has, slots);
            }
            let path = |item: Option<Entity>| {
                item.and_then(|i| items.get(i).ok())
                    .map(|d| d.path.as_str())
            };
            wanted.has |= worn_has(
                path(inventory.and_then(|i| i.eyes)),
                path(inventory.and_then(|i| i.head)),
            );
        }
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
        if EFFECT_EMOTES.contains(&emoted.key) {
            let key = emoted.key;
            commands.queue(move |world: &mut World| apply_emote_effect(world, user, key));
        }
        // `/datum/emote/jump/jump_animation`: 4 px up over 0.1 s and back.
        if emoted.key == "jump" {
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

/// The emotes [`apply_emote_effect`] does something for.
const EFFECT_EMOTES: [&str; 12] = [
    "flip", "backflip", "spin", "taunt", "dance", "collapse", "faint", "sway", "tremble", "twitch",
    "twitch_s", "shiver",
];

/// tg `FLIP_EMOTE_DURATION`, deciseconds.
const FLIP_DS: f32 = 7.0;

/// tg `/mob/proc/spin(spintime, speed)`: the mob turns clockwise every `speed` until `left`
/// turns are done (`TRAIT_SPINNING` meanwhile).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Spinning {
    /// Game time of the next turn.
    pub next: f64,
    /// Seconds between turns.
    pub speed: f64,
    pub left: u32,
}

/// tg `spin(spintime, speed)` in deciseconds: `spintime / speed` turns. A new spin replaces
/// a running one.
pub fn spin(world: &mut World, mob: Entity, spintime_ds: u32, speed_ds: u32) {
    if spintime_ds < 1 || speed_ds < 1 || world.get_entity(mob).is_err() {
        return;
    }
    let speed = f64::from(speed_ds) / 10.0;
    let now = world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64());
    world.entity_mut(mob).insert(Spinning {
        next: now + speed,
        speed,
        left: spintime_ds / speed_ds,
    });
}

/// `spin`'s `switch(D)`: the cardinals turn clockwise, diagonals stay.
fn clockwise(dir: Dir) -> Dir {
    match dir {
        Dir::North => Dir::East,
        Dir::East => Dir::South,
        Dir::South => Dir::West,
        Dir::West => Dir::North,
        diagonal => diagonal,
    }
}

fn spin_mobs(
    time: Res<Time>,
    mut mobs: Query<(Entity, &mut Spinning, Option<&mut Facing>)>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (mob, mut spinning, mut facing) in &mut mobs {
        while spinning.left > 0 && now >= spinning.next {
            if let Some(facing) = facing.as_mut() {
                let turned = clockwise(facing.0);
                facing.set_if_neq(Facing(turned));
            }
            spinning.left -= 1;
            spinning.next += spinning.speed;
        }
        if spinning.left == 0 {
            commands.entity(mob).remove::<Spinning>();
        }
    }
}

/// `animate(pixel_w = ..., flags = ANIMATION_RELATIVE)` chains as poses: each `(x, time_ds)` is
/// where the sideways offset gets to and how long that takes.
fn sideways(poses: &[(f32, f32)]) -> Animation {
    Animation::Transform(TransformAnimation {
        steps: poses
            .iter()
            .map(|&(x, time_ds)| AnimationStep {
                offset: [x, 0.0],
                ..AnimationStep::rest(time_ds)
            })
            .collect(),
        loops: 1,
    })
}

/// A back-and-forth: `out` px for `first_ds`, then `swings` times to `-out` and `out` over
/// `swing_ds` each, then back to rest over `first_ds` (`sway`, `tremble`, `shiver`).
fn swaying(out: f32, back: f32, first_ds: f32, swing_ds: f32, swings: usize) -> Animation {
    let mut poses = vec![(out, first_ds)];
    for _ in 0..swings {
        poses.push((back, swing_ds));
        poses.push((out, swing_ds));
    }
    poses.push((0.0, first_ds));
    sideways(&poses)
}

/// The animation of a `run_emote` override, `None` for the emotes without one.
pub fn emote_animation(key: &str) -> Option<Animation> {
    Some(match key {
        // `SpinAnimation(FLIP_EMOTE_DURATION, 1, clockwise)`: three 120° segments; a backflip
        // turns the other way.
        "flip" | "backflip" => {
            let sign = if key == "flip" { 1.0 } else { -1.0 };
            Animation::Transform(TransformAnimation {
                steps: (1..=3)
                    .map(|i| AnimationStep {
                        turn: sign * 120.0 * i as f32,
                        ..AnimationStep::rest(FLIP_DS / 3.0)
                    })
                    .collect(),
                loops: 1,
            })
        }
        // `sway`: 2 px, twice -6 and +6 over 1 s each, back.
        "sway" => swaying(2.0, -4.0, 5.0, 10.0, 2),
        // `tremble`: 2 px, 4.4 s of -4/+4 every 0.2 s, back.
        "tremble" => swaying(2.0, -2.0, 2.0, 2.0, 11),
        // `shiver`: 1 px, 1 s of -2/+2 every 0.1 s, back.
        "shiver" => swaying(1.0, -1.0, 1.0, 1.0, 5),
        "twitch" => sideways(&[(1.0, 1.0), (-1.0, 1.0), (-1.0, 1.0), (1.0, 1.0), (0.0, 1.0)]),
        "twitch_s" => sideways(&[(-1.0, 1.0), (0.0, 1.0)]),
        _ => return None,
    })
}

/// The effects of tg's `run_emote` overrides after an emote ran (code-run ones too).
pub fn apply_emote_effect(world: &mut World, mob: Entity, key: &str) {
    if world.get_entity(mob).is_err() {
        return;
    }
    if let Some(animation) = emote_animation(key) {
        crate::animation::play(world, mob, animation);
    }
    match key {
        "spin" => spin(world, mob, 20, 1),
        // `TAUNT_EMOTE_DURATION` 0.9 s, a turn every 0.1 s.
        "taunt" => spin(world, mob, 9, 1),
        // BandaStation `dance/run_emote`: spins for the 5 s cooldown and jitters.
        // shortcut: the jitter shakes its usual 6 times, not `cooldown / 4` loops.
        "dance" => {
            let (speed, jitter) = {
                let mut rng = world.get_resource_or_init::<crate::combat::CombatRng>();
                (rng.range(1, 2), rng.range(80, 160))
            };
            spin(world, mob, 50, speed);
            crate::animation::do_jitter_animation(world, mob, jitter as f32);
        }
        "collapse" => {
            crate::status_effects::apply(world, mob, Incap::Unconscious, 4.0);
        }
        "faint" => crate::status_effects::set(world, mob, Incap::Sleeping, 20.0),
        _ => {}
    }
}

/// tg `BEYBLADE_*`: spinning while dizzy enough makes a carbon vomit; else a 20% chance of 20 s
/// dizziness and 10 s more confusion (up to 40 s).
const BEYBLADE_PUKE_THRESHOLD: f32 = 30.0;

/// tg `flip/check_cooldown` and `spin/check_cooldown`: the penalty for running the emote while
/// it cools down.
pub fn emote_spammed(world: &mut World, mob: Entity, key: &str) {
    if world.get::<Stat>(mob).is_none() {
        return;
    }
    let chance = |world: &mut World, percent: f32| {
        world
            .get_resource_or_init::<crate::combat::CombatRng>()
            .chance(percent)
    };
    match key {
        "flip" | "backflip" => {
            let fall_over_prob = if key == "flip" { 60.0 } else { 20.0 };
            let name = crate::mobs::defense::name_of(world, mob);
            if chance(world, fall_over_prob) {
                crate::status_effects::knockdown(world, mob, 1.0, 0.0);
                crate::medical::visible(
                    world,
                    mob,
                    "Вы пытаетесь сделать сальто, не восстановив баланс после предыдущего, и \
                     падаете!",
                    &format!("{name} пытается сделать сальто и падает. Вот болван!"),
                    Span::Notice,
                );
                if chance(world, fall_over_prob / 2.0) {
                    crate::mobs::defense::hurt(
                        world,
                        mob,
                        crate::health::DamageType::Brute,
                        1.0,
                        None,
                    );
                }
            } else {
                crate::medical::visible(
                    world,
                    mob,
                    "Вы немного пошатываетесь, всё ещё не восстановив равновесие после прошлого \
                     сальто.",
                    &format!("{name} немного покачивается после сальто."),
                    Span::Notice,
                );
            }
        }
        "spin" => {
            // `iscarbon`.
            let carbon = world.get::<EmoteUser>(mob).is_none_or(|u| {
                matches!(
                    u.mob,
                    EmoteMob::Human | EmoteMob::AlienAdult | EmoteMob::AlienLarva
                )
            });
            if !carbon {
                return;
            }
            let confusion = crate::status_effects::timed(world, mob, Timed::Confusion);
            if confusion > BEYBLADE_PUKE_THRESHOLD {
                crate::medical::diseases::effects::vomit(world, mob, 60.0, true, false, 0);
                return;
            }
            if chance(world, 20.0) {
                crate::tools::tell_mob(
                    world,
                    mob,
                    Span::Warning,
                    "У вас кружится голова от вращения.",
                );
                crate::status_effects::set_timed(world, mob, Timed::Dizziness, 20.0, true);
                crate::status_effects::adjust_timed(world, mob, Timed::Confusion, 10.0, Some(40.0));
            }
        }
        _ => {}
    }
}

fn spam_effects(mut spammed: MessageReader<EmoteSpammed>, mut commands: Commands) {
    for &EmoteSpammed { speaker, key } in spammed.read() {
        commands.queue(move |world: &mut World| emote_spammed(world, speaker, key));
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

/// [`LastBreath`] of a mob in hard crit: `round(-dead_threshold + health)`.
fn sync_last_breath(
    mobs: Query<
        (Entity, &Stat, &Health, Option<&LastBreath>),
        Or<(Changed<Stat>, Changed<Health>)>,
    >,
    mut commands: Commands,
) {
    for (mob, &stat, health, current) in &mobs {
        let wanted = (stat == Stat::HardCrit)
            .then(|| LastBreath((health.health() - HEALTH_THRESHOLD_DEAD).round().max(0.0) as u32));
        if wanted.as_ref() == current {
            continue;
        }
        match wanted {
            Some(breath) => {
                commands.entity(mob).try_insert(breath);
            }
            None => {
                commands.entity(mob).try_remove::<LastBreath>();
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

    #[test]
    fn bodies_have_what_their_emotes_need() {
        let human = emote_user(EmoteBody::Human, None, false, false);
        assert_eq!(
            (human.species, human.has),
            (EmoteSpecies::Human, DEFAULT_HAS)
        );
        let monkey = emote_user(EmoteBody::Human, Some(SpeciesId::Monkey), false, false);
        assert_eq!(
            monkey.has & (HAS_SIMIAN | HAS_LESSER),
            HAS_SIMIAN | HAS_LESSER
        );
        let ethereal = emote_user(EmoteBody::Human, Some(SpeciesId::Lustrous), false, false);
        assert_ne!(ethereal.has & HAS_GLOW, 0);
        let cat = emote_user(
            EmoteBody::Basic("/mob/living/basic/pet/cat/kitten"),
            None,
            false,
            false,
        );
        assert_ne!(cat.has & HAS_CAT, 0);
        let gorilla = emote_user(
            EmoteBody::Basic("/mob/living/basic/gorilla"),
            None,
            false,
            false,
        );
        assert_ne!(gorilla.has & HAS_SIMIAN, 0);
        // Organs decide the eyes, the tongue and the felinid tongue's trait.
        assert_eq!(organ_has(DEFAULT_HAS, []), 0);
        let felinid = organ_has(
            DEFAULT_HAS | HAS_GLOW,
            [
                (Slot::Eyes, "/obj/item/organ/eyes"),
                (Slot::Tongue, "/obj/item/organ/tongue/cat"),
            ],
        );
        assert_eq!(felinid, HAS_EYES | HAS_TONGUE | HAS_CAT | HAS_GLOW);
        assert_eq!(
            organ_has(DEFAULT_HAS, [(Slot::Tongue, "/obj/item/organ/tongue")]),
            HAS_TONGUE
        );
        // Regular glasses and sunglasses, not any eyewear; kitty ears.
        assert_eq!(
            worn_has(Some("/obj/item/clothing/glasses/sunglasses/big"), None),
            HAS_GLASSES
        );
        assert_eq!(worn_has(Some("/obj/item/clothing/glasses/meson"), None), 0);
        assert_eq!(
            worn_has(None, Some("/obj/item/clothing/head/costume/kitty")),
            HAS_CAT
        );
    }

    #[test]
    fn emote_effects_follow_run_emote() {
        use crate::status_effects::StatusEffects;
        // A flip turns all the way round, a backflip the other way.
        let Some(Animation::Transform(flip)) = emote_animation("flip") else {
            panic!("no flip");
        };
        assert_eq!(flip.steps.last().map(|s| s.turn), Some(360.0));
        let Some(Animation::Transform(back)) = emote_animation("backflip") else {
            panic!("no backflip");
        };
        assert_eq!(back.steps.last().map(|s| s.turn), Some(-360.0));
        // Sways end at rest.
        for key in ["sway", "tremble", "shiver", "twitch", "twitch_s"] {
            let Some(Animation::Transform(a)) = emote_animation(key) else {
                panic!("no {key}");
            };
            assert_eq!(a.steps.last().map(|s| s.offset), Some([0.0, 0.0]), "{key}");
        }
        let Some(Animation::Transform(tremble)) = emote_animation("tremble") else {
            unreachable!()
        };
        let time: f32 = tremble.steps.iter().map(|s| s.time_ds).sum();
        assert_eq!(time, 48.0, "4.4 s of trembling and the way out and back");
        assert_eq!(emote_animation("smile"), None);

        let mut world = World::new();
        let mob = world.spawn((Stat::Conscious, Facing(Dir::North))).id();
        apply_emote_effect(&mut world, mob, "collapse");
        assert_eq!(world.get::<StatusEffects>(mob).unwrap().unconscious, 4.0);
        apply_emote_effect(&mut world, mob, "faint");
        assert_eq!(world.get::<StatusEffects>(mob).unwrap().sleeping, 20.0);
        // `spin(20, 1)`: twenty turns a tenth of a second apart; the taunt's nine replace it.
        apply_emote_effect(&mut world, mob, "spin");
        let spinning = *world.get::<Spinning>(mob).unwrap();
        assert_eq!((spinning.left, spinning.speed), (20, 0.1));
        apply_emote_effect(&mut world, mob, "taunt");
        assert_eq!(world.get::<Spinning>(mob).unwrap().left, 9);
        assert_eq!(clockwise(Dir::West), Dir::North);
        assert_eq!(clockwise(Dir::NorthEast), Dir::NorthEast);
    }

    #[test]
    fn spamming_flips_trips_and_spinning_dizzies() {
        use crate::status_effects::StatusEffects;
        let mut world = World::new();
        world.insert_resource(crate::combat::CombatRng(7));
        let flipper = world.spawn(Stat::Conscious).id();
        let mut fell = false;
        for _ in 0..50 {
            emote_spammed(&mut world, flipper, "flip");
            if world
                .get::<StatusEffects>(flipper)
                .is_some_and(|e| e.knockdown > 0.0)
            {
                fell = true;
                break;
            }
        }
        assert!(fell, "a 60% fall comes within 50 flips");
        let spinner = world.spawn((Stat::Conscious, EmoteUser::default())).id();
        let mut dizzy = false;
        for _ in 0..100 {
            emote_spammed(&mut world, spinner, "spin");
            if crate::status_effects::timed(&world, spinner, Timed::Dizziness) > 0.0 {
                dizzy = true;
                break;
            }
        }
        assert!(dizzy, "a 20% dizziness comes within 100 spins");
        assert_eq!(
            crate::status_effects::timed(&world, spinner, Timed::Confusion),
            10.0
        );
        // A cyborg is no carbon: spinning does not dizzy it.
        let borg = world
            .spawn((
                Stat::Conscious,
                emote_user(EmoteBody::Cyborg, None, false, false),
            ))
            .id();
        for _ in 0..50 {
            emote_spammed(&mut world, borg, "spin");
        }
        assert_eq!(
            crate::status_effects::timed(&world, borg, Timed::Dizziness),
            0.0
        );
    }
}
