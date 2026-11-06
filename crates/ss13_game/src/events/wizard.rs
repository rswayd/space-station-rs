//! Summon Events' wizard events (`code/modules/events/wizard/`) on ported systems. They roll only
//! in wizard mode ([`super::Events::wizardmode`]); admins can force them.
//!
//! - Robeless Casting (`aid.dm`): every known spell stops needing the robe and hat.
//! - Improved Casting (`aid.dm`): every spell gains a level, one past its cap at most.
//! - Change Places! (`shuffle.dm` `shuffleloc`): the living humans on the station levels swap
//!   places at random; a puff of smoke on every living human.
//! - Change Faces! (`shuffle.dm` `shufflenames`): the living humans' names are shuffled among
//!   them; smoke as above.
//! - Change Minds! (`shuffle.dm` `shuffleminds`): the conscious living humans with a mind, the
//!   wizards spared, swap minds in pairs; smoke as above.
//! - Tower of Babel (`tower_of_babel.dm`, `spells/spell_types/tower_of_babel.dm`, the
//!   `tower_of_babel/magical` status effect): every player's carbon body on the station levels
//!   speaks and understands one random language only ([`curse_of_babel`]), its tongue no limit;
//!   antimagic of the mind protects; latejoiners are cursed too ([`TowerOfBabel`]). Wizards
//!   learn every language instead.
//! - Summon Guns (`summons.dm`): `summon_guns` with a 10% survivalist chance
//!   (`crate::antag::survivalist`).
//!
//! - The Floor Is LAVA! (`lava.dm`): the `floor_is_lava` weather (`crate::weather`).
//! - Race Swap (`race.dm`): every human, the dead too, becomes one random `RACE_SWAP` species,
//!   or (50%) each a random one.
//! - Curse of Madness (`madness.dm`, `spells/spell_types/madness_curse.dm`): a random
//!   `redpill_questions` truth and lobotomy-resilient brain traumas for every living player
//!   human on the station levels, antimagic of the mind protecting; latejoiners get it too
//!   ([`CurseOfMadness`]).
//! - Fake Nuclear Explosion (`fakeexplosion.dm`, weight 0: admins only): the nuke alarm for
//!   every player.
//! - Petsplosion (`petsplosion.dm`): the station's living pets and farm animals double every
//!   30 event ticks for a minute, at most 400 copies; it does not run with none or over 100.
//!
//! Summon Events itself is the spellbook ritual (`crate::antag::wizard::server`), calling
//! [`super::controller::summon_events`].
// shortcut: the teleport ignores antimagic and teleport-blocking areas (tg `do_teleport` magic
// channel) and things in lockers; antagonist datums, mind languages and traits stay with the
// body when minds swap (ss13-rs keeps them on bodies), no `TRAIT_NO_MINDSWAP`; the Tower of
// Babel has no deadchat broadcast, admin undo or status alert, and its curse is kept by the body.
// shortcut: Race Swap does not regenerate the DNA's unique enzymes (no DNA enzymes); the Curse
// of Madness has no deadchat broadcast or admin text input; the Fake Nuclear Explosion plays no
// cinematic (none ported); Petsplosion only copies basic mobs (no simple animals).

use bevy::prelude::*;
use ss13_chat::language::{self, LanguageId, LanguageSource, edit_holder};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::health::Stat;
use crate::medical::brainmob::{Mind, mind_of};
use crate::server::PlayerMob;
use crate::species::SpeciesId;
use crate::spells::{SPELL_REQUIRES_WIZARD_GARB, Spells};

pub const ROBELESS: &str = "Robeless Casting";
pub const IMPROVED: &str = "Improved Casting";
pub const SHUFFLE_PLACES: &str = "Change Places!";
pub const SHUFFLE_NAMES: &str = "Change Faces!";
pub const SHUFFLE_MINDS: &str = "Change Minds!";
pub const BABEL: &str = "Tower of Babel";
pub const SUMMON_GUNS: &str = "Summon Guns";
pub const LAVA: &str = "The Floor Is LAVA!";
pub const RACE_SWAP: &str = "Race Swap";
pub const MADNESS: &str = "Curse of Madness";
pub const FAKE_EXPLOSION: &str = "Fake Nuclear Explosion";
pub const PETSPLOSION: &str = "Petsplosion";

pub fn make_lava(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Lava)
}

pub fn make_race_swap(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(RaceSwap)
}

pub fn make_madness(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Madness)
}

pub fn make_fake_explosion(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(FakeExplosion)
}

pub fn make_petsplosion(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Petsplosion {
        countdown: 0,
        duped: 0,
    })
}

pub fn make_robeless(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Robeless)
}

pub fn make_improved(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Improved)
}

pub fn make_shuffle_places(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ShufflePlaces)
}

pub fn make_shuffle_names(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ShuffleNames)
}

pub fn make_shuffle_minds(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ShuffleMinds)
}

pub fn make_babel(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Babel)
}

pub fn make_summon_guns(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(SummonGuns)
}

struct SummonGuns;

impl RoundEvent for SummonGuns {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        crate::antag::survivalist::summon_guns(world, None, 10.0);
    }
}

/// tg `IS_WIZARD` (apprentices too).
fn is_wizard(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::antag::wizard::server::Wizard>(mob)
        .is_some()
}

fn casters(world: &mut World) -> Vec<Entity> {
    let mut query = world.query_filtered::<Entity, With<Spells>>();
    let mut all: Vec<Entity> = query.iter(world).collect();
    all.sort();
    all
}

fn tell(world: &mut World, mob: Entity, text: &str) {
    crate::medical::diseases::say_to(world, mob, Span::Notice, text);
}

struct Robeless;

impl RoundEvent for Robeless {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        for mob in casters(world) {
            let mut improved = false;
            if let Some(mut spells) = world.get_mut::<Spells>(mob) {
                for known in &mut spells.known {
                    if known.spell.def().requirements & SPELL_REQUIRES_WIZARD_GARB != 0
                        && !known.robeless
                    {
                        known.robeless = true;
                        improved = true;
                    }
                }
            }
            if improved {
                tell(
                    world,
                    mob,
                    "You suddenly feel like you never needed those garish robes in the first \
                     place...",
                );
            }
        }
    }
}

/// `level_spell(bypass_cap = TRUE)` once, unless Improved Casting already took it past its cap.
pub fn improve(known: &mut crate::spells::KnownSpell) -> bool {
    let max = known.spell.def().max_level;
    if max <= 1 || known.level > max {
        return false;
    }
    known.level += 1;
    true
}

struct Improved;

impl RoundEvent for Improved {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        for mob in casters(world) {
            // tg keeps only the last spell's result; any upgrade counts here.
            let mut upgraded = false;
            if let Some(mut spells) = world.get_mut::<Spells>(mob) {
                for known in &mut spells.known {
                    upgraded |= improve(known);
                }
            }
            if upgraded {
                crate::spells::refresh(world, mob);
                tell(
                    world,
                    mob,
                    "You suddenly feel more competent with your casting!",
                );
            }
        }
    }
}

/// The living humans (`GLOB.alive_mob_list` humans), with their tile when on one.
fn living_humans(world: &mut World) -> Vec<(Entity, Option<TilePos>)> {
    let mut query =
        world.query_filtered::<(Entity, &Stat, Option<&TilePos>), With<crate::human::HumanLook>>();
    let mut all: Vec<(Entity, Option<TilePos>)> = query
        .iter(world)
        .filter(|(_, stat, _)| **stat != Stat::Dead)
        .map(|(e, _, pos)| (e, pos.copied()))
        .collect();
    all.sort_by_key(|(e, _)| *e);
    all
}

fn smoke_everyone(world: &mut World) {
    for pos in living_humans(world).into_iter().filter_map(|(_, pos)| pos) {
        crate::grenades::do_smoke(world, pos, 0);
    }
}

/// DM `shuffle_inplace` (Fisher-Yates) with the event stream.
fn shuffle<T>(world: &mut World, items: &mut [T]) {
    let mut rng = world.resource_mut::<EventRng>();
    for i in (1..items.len()).rev() {
        let j = rng.0.range(0, i as i64) as usize;
        items.swap(i, j);
    }
}

struct ShufflePlaces;

impl RoundEvent for ShufflePlaces {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let levels = station_levels(world);
        let (mut mobs, mut locs): (Vec<Entity>, Vec<TilePos>) = living_humans(world)
            .into_iter()
            .filter_map(|(mob, pos)| pos.filter(|p| levels.contains(&p.z)).map(|p| (mob, p)))
            .unzip();
        if mobs.is_empty() {
            return;
        }
        shuffle(world, &mut locs);
        shuffle(world, &mut mobs);
        for mob in mobs {
            let Some(dest) = locs.pop() else {
                break;
            };
            world.entity_mut(mob).insert(dest);
        }
        smoke_everyone(world);
    }
}

struct ShuffleNames;

impl RoundEvent for ShuffleNames {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let mut mobs: Vec<Entity> = living_humans(world).into_iter().map(|(e, _)| e).collect();
        if mobs.is_empty() {
            return;
        }
        let mut names: Vec<String> = mobs
            .iter()
            .map(|&mob| crate::medical::name_of(world, mob))
            .collect();
        shuffle(world, &mut names);
        shuffle(world, &mut mobs);
        for mob in mobs {
            let Some(name) = names.pop() else {
                break;
            };
            world.entity_mut(mob).insert(Name::new(name));
        }
        smoke_everyone(world);
    }
}

struct ShuffleMinds;

impl RoundEvent for ShuffleMinds {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let mut mobs = Vec::new();
        for (mob, _) in living_humans(world) {
            // `IS_UNCONSCIOUS_OR_CRIT`, no mind, the wizards; and nowhere to be put.
            if world.get::<Stat>(mob) != Some(&Stat::Conscious)
                || is_wizard(world, mob)
                || crate::explosions::turf_of(world, mob).is_none()
                || mind_of(world, mob).is_none()
            {
                continue;
            }
            mobs.push(mob);
        }
        if mobs.is_empty() {
            return;
        }
        shuffle(world, &mut mobs);
        while mobs.len() > 1 {
            let (Some(a), Some(b)) = (mobs.pop(), mobs.pop()) else {
                break;
            };
            swap_minds(world, a, b);
        }
        smoke_everyone(world);
    }
}

/// `mind_transfer/swap_minds`: the mind of `a` goes to `b` and the other way round.
pub fn swap_minds(world: &mut World, a: Entity, b: Entity) {
    let (mind_a, mind_b) = (mind_of(world, a), mind_of(world, b));
    let set_ghost = |world: &mut World, ghost: Entity, body: Entity| {
        if let Some(mut g) = world.get_mut::<crate::ghost::Ghost>(ghost) {
            g.body = Some(body);
        }
    };
    match mind_a {
        Some(Mind::Client(client)) => {
            crate::medical::brainmob::possess(world, client, a, b);
        }
        Some(Mind::Ghost(ghost)) => set_ghost(world, ghost, b),
        None => {}
    }
    match mind_b {
        Some(Mind::Client(client)) => {
            // `b` already holds the client of `a`: leave from a stand-in instead.
            let stand_in =
                matches!(mind_a, Some(Mind::Client(_))).then(|| world.spawn_empty().id());
            crate::medical::brainmob::possess(world, client, stand_in.unwrap_or(b), a);
            if let Some(e) = stand_in {
                world.despawn(e);
            }
        }
        Some(Mind::Ghost(ghost)) => set_ghost(world, ghost, a),
        None => {}
    }
}

/// `GLOB.tower_of_babel`: while it stands, crew joining is cursed.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct TowerOfBabel;

/// `/datum/mood_event/tower_of_babel` (BandaStation's text).
const BABEL_MOOD: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "tower_of_babel",
    description: "Мой язык - это новый Вавилон, где никто ничего не понимает...",
    mood_change: -1.0,
    timeout: 15.0,
    hidden: false,
};

struct Babel;

impl RoundEvent for Babel {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        world.insert_resource(TowerOfBabel);
        let levels = station_levels(world);
        let mut players: Vec<Entity> = world
            .query_filtered::<Entity, (With<PlayerMob>, With<crate::human::HumanLook>)>()
            .iter(world)
            .collect();
        players.sort();
        for mob in players {
            if is_wizard(world, mob) {
                // Immune, and they speak every language to taunt their victims over the radio.
                edit_holder(world, mob, |h| {
                    h.grant_all(LanguageSource::Babel);
                    h.babel = true;
                });
                tell_babel(
                    world,
                    mob,
                    "You feel a magical force improving your speech patterns!",
                );
                continue;
            }
            if world.get::<Stat>(mob) == Some(&Stat::Dead) {
                continue;
            }
            if crate::explosions::turf_of(world, mob).is_some_and(|p| !levels.contains(&p.z)) {
                continue;
            }
            curse_of_babel(world, mob);
        }
    }
}

fn tell_babel(world: &mut World, mob: Entity, text: &str) {
    crate::medical::diseases::say_to(world, mob, Span::Hypnophrase, text);
}

/// tg `curse_of_babel(mob)` and the `tower_of_babel/magical` status effect's `on_apply`: one
/// random language (`pick(GLOB.all_languages)`), every other one blocked, the tongue no limit
/// (`TRAIT_TOWER_OF_BABEL`). True when cursed.
pub fn curse_of_babel(world: &mut World, mob: Entity) -> bool {
    // Silicons are immune (`iscarbon`).
    if world.get::<crate::human::HumanLook>(mob).is_none() {
        return false;
    }
    let immune = world
        .get::<ss13_chat::language::LanguageHolder>(mob)
        .is_some_and(|h| h.babel);
    if immune
        || crate::spells::can_block_magic(
            world,
            mob,
            crate::spells::MAGIC_RESISTANCE | crate::spells::MAGIC_RESISTANCE_MIND,
        )
    {
        crate::medical::diseases::say_to(
            world,
            mob,
            Span::Notice,
            "You have a strange feeling for a moment, but then it passes.",
        );
        return false;
    }
    let Some(&picked) = world.resource_mut::<EventRng>().pick(&LanguageId::ALL) else {
        return false;
    };
    edit_holder(world, mob, |h| {
        h.grant(picked, language::ALL, LanguageSource::Babel);
        for other in LanguageId::ALL.into_iter().filter(|&l| l != picked) {
            h.block(other, language::ALL, LanguageSource::Babel);
        }
        h.babel = true;
    });
    crate::mood::add_event(world, mob, "tower_of_babel_magic", &BABEL_MOOD);
    crate::medical::diseases::emote(world, mob, "mumble");
    let pos = crate::explosions::turf_of(world, mob);
    crate::hallucination::play_local(
        world,
        mob,
        pos,
        "sound/effects/magic/magic_block_mind.ogg",
        75.0,
        true,
    );
    tell_babel(
        world,
        mob,
        "You feel a magical force affecting your speech patterns!",
    );
    info!("{mob} is cursed by the Tower of Babel: {picked:?}");
    true
}

/// `COMSIG_GLOB_CREWMEMBER_JOINED` while the tower stands: the new crew is cursed.
pub(super) fn curse_joiners(
    tower: Option<Res<TowerOfBabel>>,
    joined: Query<Entity, Added<crate::jobs::PlayerJob>>,
    mut commands: Commands,
) {
    if tower.is_none() {
        return;
    }
    for mob in &joined {
        commands.queue(move |world: &mut World| {
            curse_of_babel(world, mob);
        });
    }
}

struct Lava;

impl RoundEvent for Lava {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        crate::weather::run_weather(world, crate::weather::WeatherKind::FloorIsLava, None);
    }
}

/// The species with `RACE_SWAP` in `changesource_flags` (subtypes inherit it).
pub const RACE_SWAP_SPECIES: [SpeciesId; 21] = [
    SpeciesId::Human,
    SpeciesId::Felinid,
    SpeciesId::Cerulean,
    SpeciesId::Lizard,
    SpeciesId::Ashwalker,
    SpeciesId::Silverscale,
    SpeciesId::Ethereal,
    SpeciesId::Lustrous,
    SpeciesId::Jelly,
    SpeciesId::Slime,
    SpeciesId::Luminescent,
    SpeciesId::Stargazer,
    SpeciesId::Moth,
    SpeciesId::Pod,
    SpeciesId::Fly,
    SpeciesId::Snail,
    SpeciesId::Android,
    SpeciesId::Abductor,
    SpeciesId::Vulpkanin,
    SpeciesId::Skrell,
    SpeciesId::Tajaran,
];

struct RaceSwap;

impl RoundEvent for RaceSwap {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let pick = |world: &mut World| {
            world
                .resource_mut::<EventRng>()
                .pick(&RACE_SWAP_SPECIES)
                .copied()
                .unwrap_or_default()
        };
        let mut species = pick(world);
        let all_the_same = world.resource_mut::<EventRng>().0.prob(50.0);
        // `GLOB.human_list`: yes, even the dead.
        let mut humans: Vec<Entity> = world
            .query_filtered::<Entity, With<crate::human::HumanLook>>()
            .iter(world)
            .collect();
        humans.sort();
        for mob in humans {
            crate::species::set_species(world, mob, species);
            tell(world, mob, "You feel somehow... different?");
            if !all_the_same {
                species = pick(world);
            }
        }
    }
}

/// `GLOB.curse_of_madness_triggered`: the truth latejoiners are cursed with too.
#[derive(Resource, Clone, Debug, Default)]
pub struct CurseOfMadness(pub String);

/// Shown when `strings/redpill.json` cannot be read (its first question).
const REDPILL_FALLBACK: &str = "2010 год был 550 лет назад.";

/// `strings(REDPILL_FILE, "redpill_questions")`.
fn redpill_questions(world: &World) -> Vec<String> {
    let Some(source) = world
        .get_resource::<ss13_mapload::MapLoadPlugin>()
        .map(|m| m.source.clone())
    else {
        return Vec::new();
    };
    ss13_data::read_to_string(&source.join("strings/redpill.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| {
            json.get("redpill_questions")?.as_array().map(|list| {
                list.iter()
                    .filter_map(|q| q.as_str().map(str::to_owned))
                    .collect()
            })
        })
        .unwrap_or_default()
}

struct Madness;

impl RoundEvent for Madness {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let questions = redpill_questions(world);
        let truth = world
            .resource_mut::<EventRng>()
            .pick(&questions)
            .cloned()
            .unwrap_or_else(|| REDPILL_FALLBACK.to_owned());
        curse_of_madness(world, &truth);
    }
}

/// tg `curse_of_madness(null, message)`: every living player human on the station levels (or
/// nowhere) not protected by antimagic of the mind goes mad.
pub fn curse_of_madness(world: &mut World, message: &str) {
    world.insert_resource(CurseOfMadness(message.to_owned()));
    let levels = station_levels(world);
    let mut players: Vec<Entity> = world
        .query_filtered::<Entity, (With<PlayerMob>, With<crate::human::HumanLook>)>()
        .iter(world)
        .collect();
    players.sort();
    for mob in players {
        if world.get::<Stat>(mob) == Some(&Stat::Dead) {
            continue;
        }
        if crate::explosions::turf_of(world, mob).is_some_and(|p| !levels.contains(&p.z)) {
            continue;
        }
        if crate::spells::can_block_magic(
            world,
            mob,
            crate::spells::MAGIC_RESISTANCE | crate::spells::MAGIC_RESISTANCE_MIND,
        ) {
            tell(
                world,
                mob,
                "You have a strange feeling for a moment, but then it passes.",
            );
            continue;
        }
        give_madness(world, mob, message);
    }
}

/// `madness_curse.dm` `give_madness`: two mild traumas (30%), a severe (30%), a magic (20%) or
/// a special one (20%), all lobotomy-resilient.
pub fn give_madness(world: &mut World, mob: Entity, message: &str) {
    use crate::medical::traumas::{self, Category, Resilience};
    let pos = crate::explosions::turf_of(world, mob);
    crate::hallucination::play_local(world, mob, pos, "sound/effects/magic/curse.ogg", 40.0, true);
    crate::medical::diseases::say_to(world, mob, Span::Hypnophrase, message);
    crate::medical::diseases::say_to(world, mob, Span::Warning, "Your mind shatters!");
    let mut rng = crate::combat::CombatRng(world.resource_mut::<EventRng>().0.next_u64());
    let categories: &[Category] = match rng.range(1, 10) {
        1..=3 => &[Category::Mild, Category::Mild],
        4..=6 => &[Category::Severe],
        7..=8 => &[Category::Magic],
        _ => &[Category::Special],
    };
    let Some(brain) =
        crate::medical::organs::organ(world, mob, crate::medical::organs::Slot::Brain)
    else {
        return;
    };
    for &category in categories {
        traumas::gain_type(
            world,
            brain,
            category,
            Some(Resilience::Lobotomy),
            false,
            &mut rng,
        );
    }
}

/// `AttemptLateSpawn`: a latejoiner gets the curse of madness when one struck.
pub(super) fn madden_joiners(
    curse: Option<Res<CurseOfMadness>>,
    joined: Query<Entity, Added<crate::jobs::PlayerJob>>,
    mut commands: Commands,
) {
    let Some(curse) = curse else {
        return;
    };
    for mob in &joined {
        let message = curse.0.clone();
        commands.queue(move |world: &mut World| {
            if world.get::<crate::human::HumanLook>(mob).is_some() {
                give_madness(world, mob, &message);
            }
        });
    }
}

struct FakeExplosion;

impl RoundEvent for FakeExplosion {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        // `sound_to_playing_players`: every client with a mob.
        let mut clients: Vec<Entity> = world
            .query_filtered::<Entity, With<ss13_net::server::ControlledEntity>>()
            .iter(world)
            .collect();
        clients.sort();
        for client in clients {
            world.write_message(ss13_net::sound::server::SendSound {
                client,
                sound: ss13_shared::protocol::SoundEvent {
                    file: "sound/announcer/alarm/nuke_alarm.ogg".to_owned(),
                    volume: 70.0,
                    speed: 1.0,
                    offset: None,
                },
            });
        }
    }
}

/// `GLOB.petsplosion_candidates`.
const PETSPLOSION_CANDIDATES: [&str; 18] = [
    "/mob/living/basic/bat",
    "/mob/living/basic/butterfly",
    "/mob/living/basic/carp/pet/cayenne",
    "/mob/living/basic/chicken",
    "/mob/living/basic/cow",
    "/mob/living/basic/goat",
    "/mob/living/basic/goose/vomit",
    "/mob/living/basic/lizard",
    "/mob/living/basic/mothroach",
    "/mob/living/basic/mouse/brown/tom",
    "/mob/living/basic/parrot",
    "/mob/living/basic/pet",
    "/mob/living/basic/pig",
    "/mob/living/basic/rabbit",
    "/mob/living/basic/sheep",
    "/mob/living/basic/sloth",
    "/mob/living/basic/snake",
    "/mob/living/basic/spider/giant/sgt_araneus",
];

pub fn petsplosion_candidate(path: &str) -> bool {
    PETSPLOSION_CANDIDATES
        .iter()
        .any(|root| ss13_mapload::fidelity::is_under(path, root))
}

/// The living candidates on the station levels: type and tile.
fn petsplosion_mobs(world: &mut World) -> Vec<(String, TilePos)> {
    let levels = station_levels(world);
    let mut query = world.query::<(Entity, &crate::mobs::BasicMob, &TilePos, Option<&Stat>)>();
    let mut mobs: Vec<(Entity, String, TilePos)> = query
        .iter(world)
        .filter(|(_, _, pos, stat)| levels.contains(&pos.z) && *stat != Some(&Stat::Dead))
        .filter(|(_, mob, _, _)| petsplosion_candidate(&mob.def.path))
        .map(|(e, mob, pos, _)| (e, mob.def.path.clone(), *pos))
        .collect();
    mobs.sort_by_key(|(e, _, _)| *e);
    mobs.into_iter().map(|(_, path, pos)| (path, pos)).collect()
}

/// `preRunEvent`: some candidates, at most 100.
pub fn petsplosion_can_run(world: &mut World) -> bool {
    let count = petsplosion_mobs(world).len();
    count > 0 && count <= 100
}

struct Petsplosion {
    countdown: u32,
    duped: u32,
}

impl RoundEvent for Petsplosion {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        // 1 minute (+1 tick for endWhen not to interfere with tick).
        ev.end_when = 61;
    }

    fn tick(&mut self, world: &mut World, ev: &mut EventState) {
        // 0 s: 2 animals, 30 s: 4, 1 minute: 8.
        if ev.active_for < 30 * self.countdown {
            return;
        }
        self.countdown += 1;
        for (path, pos) in petsplosion_mobs(world) {
            crate::mobs::spawn::spawn_mob(world, &path, pos);
            self.duped += 1;
            if self.duped > 400 {
                ev.killed = true;
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spells::{KnownSpell, Spell};

    #[test]
    fn petsplosion_candidates_are_pets_and_farm_animals() {
        assert!(petsplosion_candidate("/mob/living/basic/pet/dog/corgi/ian"));
        assert!(petsplosion_candidate("/mob/living/basic/cow"));
        assert!(petsplosion_candidate("/mob/living/basic/mouse/brown/tom"));
        assert!(!petsplosion_candidate("/mob/living/basic/mouse"));
        assert!(!petsplosion_candidate("/mob/living/basic/carp"));
    }

    #[test]
    fn race_swap_species_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        assert!(RACE_SWAP_SPECIES.iter().all(|s| seen.insert(*s)));
        assert!(!RACE_SWAP_SPECIES.contains(&SpeciesId::Plasmaman));
    }

    #[test]
    fn swap_minds_trades_clients_and_ghosts() {
        let mut world = World::new();
        let (ca, cb) = (world.spawn_empty().id(), world.spawn_empty().id());
        let a = world
            .spawn((TilePos::new(1, 1, 1), PlayerMob { client: ca }))
            .id();
        let b = world
            .spawn((TilePos::new(2, 1, 1), PlayerMob { client: cb }))
            .id();
        swap_minds(&mut world, a, b);
        assert_eq!(world.get::<PlayerMob>(a).map(|p| p.client), Some(cb));
        assert_eq!(world.get::<PlayerMob>(b).map(|p| p.client), Some(ca));
        assert_eq!(
            world
                .get::<ss13_net::server::ControlledEntity>(ca)
                .map(|c| c.0),
            Some(b)
        );
        // A body whose player ghosted: the ghost now belongs to the other body.
        world.entity_mut(b).remove::<PlayerMob>();
        let ghost = world.spawn(crate::ghost::Ghost { body: Some(b) }).id();
        swap_minds(&mut world, a, b);
        assert_eq!(
            world.get::<crate::ghost::Ghost>(ghost).and_then(|g| g.body),
            Some(a)
        );
        assert_eq!(world.get::<PlayerMob>(b).map(|p| p.client), Some(cb));
        assert!(world.get::<PlayerMob>(a).is_none());
    }

    #[test]
    fn improved_casting_goes_one_past_the_cap() {
        let spell = Spell::SummonItem;
        let max = spell.def().max_level;
        let mut known = KnownSpell {
            spell,
            level: 1,
            ready_at: 0.0,
            marked: None,
            hand: None,
            robeless: false,
        };
        // Instant Summons has one level only: never levelled.
        assert_eq!(max, 1);
        assert!(!improve(&mut known));
        let leveled = crate::spells::SPELLS
            .iter()
            .find(|d| d.max_level >= 2)
            .expect("a levelled spell");
        known.spell = leveled.spell;
        known.level = leveled.max_level;
        assert!(improve(&mut known), "past the cap once");
        assert_eq!(known.level, leveled.max_level + 1);
        assert!(!improve(&mut known), "but no further");
    }
}
