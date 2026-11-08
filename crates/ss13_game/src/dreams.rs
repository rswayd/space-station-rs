//! tg dreams (`code/modules/flufftext/dreaming.dm`): a sleeping carbon (the `asleep` pulse,
//! `crate::status_effects`) starts a dream on 10% of pulses when not dreaming yet
//! (`handle_dreams`); its fragments reach the sleeper's chat one by one, 1-3 s apart, as
//! `... fragment ...`, and the dream ends early when the sleeper wakes or falls into hard crit
//! (`dream_sequence`).
//!
//! - `/datum/dream/random` (weight 1000): "you see", a subject (90% a noun of a bedsheet on the
//!   sleeper's tile when there is one, else `strings/dreamstrings.txt`), a verb
//!   (`strings/names/*verbs.txt`, adverbs) and, 75% of the time, an object.
//! - `/datum/dream/hear_something` (weight 500): a toy or a record, then a sound.
// shortcut: the "hear something" sound comes from a short list ([`DREAM_SOUNDS`]) instead of every
// file under `sound/` (`SSsounds.all_sounds`, most of them never converted for the client), and
// is not stopped 5 s after the dream (no sound channels yet). `COMSIG_PRE_DREAMING` and
// `COMSIG_START_DREAMING` are calls into the dream god (`crate::service::dream_pool`,
// `crate::service::dream_started`); no `sleep_until_finished`.

use bevy::prelude::*;
use ss13_mapload::MapLoadPlugin;
use ss13_net::sound::server::SendSound;
use ss13_shared::chat::Span;
use ss13_shared::protocol::SoundEvent;

use crate::combat::CombatRng;
use crate::health::Stat;

/// `GLOB.dream_strings`, `adjectives`, `adverbs`, `ing_verbs`, `verbs`.
#[derive(Resource, Clone, Debug, Default)]
pub struct DreamStrings {
    pub dream_strings: Vec<String>,
    pub adjectives: Vec<String>,
    pub adverbs: Vec<String>,
    pub ing_verbs: Vec<String>,
    pub verbs: Vec<String>,
}

/// Dream rolls, apart from the combat stream: a dream never shifts a combat roll.
#[derive(Resource, Debug)]
pub struct DreamRng(pub CombatRng);

impl Default for DreamRng {
    fn default() -> Self {
        Self(CombatRng(0xD4EA_4D5E_ED00_0001))
    }
}

/// tg `TRAIT_DREAMING`: the fragments still to show and when the next one is due.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Dreaming {
    pub fragments: Vec<Fragment>,
    pub next_at: f64,
    pub kind: DreamKind,
}

/// The `/datum/dream` type being dreamt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DreamKind {
    #[default]
    Random,
    HearSomething,
    /// The dream portent rite's (`/datum/dream/specific_portent`).
    SpecificPortent,
    /// The dream god's (`/datum/dream/random/vague_portent`).
    VaguePortent,
    /// The slumber party's (`/datum/dream/shared`).
    Shared,
}

/// A dream another system puts in the pool (`COMSIG_PRE_DREAMING`'s `dream_pool[dream] = weight`).
#[derive(Clone, Debug, PartialEq)]
pub struct ExtraDream {
    pub kind: DreamKind,
    pub weight: u32,
    pub source: DreamSource,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DreamSource {
    Fragments(Vec<Fragment>),
    /// A random dream about these nouns.
    RandomOf(Vec<&'static str>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fragment {
    Text(String),
    /// `hear_something/PlayRandomSound`: a sound, then this line.
    Sound,
}

/// `PlayRandomSound`'s pick.
pub const DREAM_SOUNDS: [&str; 9] = [
    "sound/items/bikehorn.ogg",
    "sound/items/toy_squeak/toysqueak1.ogg",
    "sound/misc/sadtrombone.ogg",
    "sound/effects/ghost.ogg",
    "sound/machines/ding.ogg",
    "sound/misc/bang.ogg",
    "sound/misc/highlander.ogg",
    "sound/misc/scary_horn.ogg",
    "sound/misc/slip.ogg",
];

const HEARD: &str = "you hear something you weren't expecting!";

/// `dream_messages` of the bedsheets, most specific first (doubles inherit theirs).
const BEDSHEET_DREAMS: &[(&str, &[&str])] = &[
    (
        "/obj/item/bedsheet/rd/royal_cape",
        &["mining", "stone", "a golem", "freedom", "doing whatever"],
    ),
    ("/obj/item/bedsheet/blue", &["blue"]),
    ("/obj/item/bedsheet/green", &["green"]),
    ("/obj/item/bedsheet/grey", &["grey"]),
    ("/obj/item/bedsheet/orange", &["orange"]),
    ("/obj/item/bedsheet/purple", &["purple"]),
    (
        "/obj/item/bedsheet/patriot",
        &["America", "freedom", "fireworks", "bald eagles"],
    ),
    (
        "/obj/item/bedsheet/rainbow",
        &[
            "red",
            "orange",
            "yellow",
            "green",
            "blue",
            "purple",
            "a rainbow",
        ],
    ),
    ("/obj/item/bedsheet/red", &["red"]),
    ("/obj/item/bedsheet/yellow", &["yellow"]),
    (
        "/obj/item/bedsheet/mime",
        &[
            "silence",
            "gestures",
            "a pale face",
            "a gaping mouth",
            "the mime",
        ],
    ),
    (
        "/obj/item/bedsheet/clown",
        &[
            "honk",
            "laughter",
            "a prank",
            "a joke",
            "a smiling face",
            "the clown",
        ],
    ),
    (
        "/obj/item/bedsheet/captain",
        &[
            "authority",
            "a golden ID",
            "sunglasses",
            "a green disc",
            "an antique gun",
            "the captain",
        ],
    ),
    (
        "/obj/item/bedsheet/rd",
        &[
            "authority",
            "a silvery ID",
            "a bomb",
            "a mech",
            "a facehugger",
            "maniacal laughter",
            "the research director",
        ],
    ),
    (
        "/obj/item/bedsheet/medical",
        &["healing", "life", "surgery", "a doctor"],
    ),
    (
        "/obj/item/bedsheet/cmo",
        &[
            "authority",
            "a silvery ID",
            "healing",
            "life",
            "surgery",
            "a cat",
            "the chief medical officer",
        ],
    ),
    (
        "/obj/item/bedsheet/hos",
        &[
            "authority",
            "a silvery ID",
            "handcuffs",
            "a baton",
            "a flashbang",
            "sunglasses",
            "the head of security",
        ],
    ),
    (
        "/obj/item/bedsheet/hop",
        &[
            "authority",
            "a silvery ID",
            "obligation",
            "a computer",
            "an ID",
            "a corgi",
            "the head of personnel",
        ],
    ),
    (
        "/obj/item/bedsheet/ce",
        &[
            "authority",
            "a silvery ID",
            "the engine",
            "power tools",
            "an APC",
            "a parrot",
            "the chief engineer",
        ],
    ),
    (
        "/obj/item/bedsheet/qm",
        &[
            "authority",
            "a silvery ID",
            "a shuttle",
            "a crate",
            "a sloth",
            "the quartermaster",
        ],
    ),
    (
        "/obj/item/bedsheet/chaplain",
        &[
            "a grey ID",
            "the gods",
            "a fulfilled prayer",
            "a cult",
            "the chaplain",
        ],
    ),
    ("/obj/item/bedsheet/brown", &["brown"]),
    ("/obj/item/bedsheet/black", &["black"]),
    (
        "/obj/item/bedsheet/centcom",
        &["a unique ID", "authority", "artillery", "an ending"],
    ),
    (
        "/obj/item/bedsheet/syndie",
        &[
            "a green disc",
            "a red crystal",
            "a glowing blade",
            "a wire-covered ID",
        ],
    ),
    (
        "/obj/item/bedsheet/cult",
        &[
            "a tome",
            "a floating red crystal",
            "a glowing sword",
            "a bloody symbol",
            "a massive humanoid figure",
        ],
    ),
    (
        "/obj/item/bedsheet/wiz",
        &[
            "a book",
            "an explosion",
            "lightning",
            "a staff",
            "a skeleton",
            "a robe",
            "magic",
        ],
    ),
    (
        "/obj/item/bedsheet/rev",
        &[
            "the people",
            "liberation",
            "collaboration",
            "heads rolling",
            "so, so many baseball bats",
            "blinding light",
            "your brothers in arms",
        ],
    ),
    ("/obj/item/bedsheet/nanotrasen", &["authority", "an ending"]),
    (
        "/obj/item/bedsheet/ian",
        &["a dog", "a corgi", "woof", "bark", "arf"],
    ),
    (
        "/obj/item/bedsheet/runtime",
        &["a kitty", "a cat", "meow", "purr", "nya~"],
    ),
    (
        "/obj/item/bedsheet/pirate",
        &[
            "a buried treasure",
            "an island",
            "a monkey",
            "a parrot",
            "a swashbuckler",
            "a talking skull",
            "avast",
            "being a pirate",
            "'cause a pirate is free",
            "doing whatever you want",
            "gold",
            "landlubbers",
            "stealing",
            "sailing the Seven Seas",
            "yarr",
        ],
    ),
    (
        "/obj/item/bedsheet/gondola",
        &[
            "peace",
            "comfiness",
            "a rare critter",
            "a harmless creature",
        ],
    ),
    (
        "/obj/item/bedsheet/cosmos",
        &[
            "the infinite cosmos",
            "Hans Zimmer music",
            "a flight through space",
            "the galaxy",
            "being fabulous",
            "shooting stars",
        ],
    ),
    ("/obj/item/bedsheet", &["white"]),
];

pub struct DreamsPlugin;

impl Plugin for DreamsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DreamRng>()
            .add_systems(Startup, load_strings)
            .add_systems(Update, dream_sequence);
    }
}

/// `world.file2list` of the five files; a missing file leaves its list empty.
fn load_strings(world: &mut World) {
    let source = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.source.clone());
    let read = |file: &str| -> Vec<String> {
        source
            .as_ref()
            .and_then(|s| ss13_data::read_to_string(&s.join("strings").join(file)).ok())
            .map(|t| {
                t.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    // A test may have put its own lists in.
    if world.contains_resource::<DreamStrings>() {
        return;
    }
    world.insert_resource(DreamStrings {
        dream_strings: read("dreamstrings.txt"),
        adjectives: read("names/adjectives.txt"),
        adverbs: read("names/adverbs.txt"),
        ing_verbs: read("names/ing_verbs.txt"),
        verbs: read("names/verbs.txt"),
    });
}

/// `handle_dreams`, from the sleeping pulse of a carbon.
pub fn handle_dreams(world: &mut World, mob: Entity) {
    if world.get::<Dreaming>(mob).is_some() || !world.resource_mut::<DreamRng>().0.chance(10.0) {
        return;
    }
    dream(world, mob);
}

/// tg `dream()`: a dream picked from `GLOB.dreams` and the extra pool starts now.
pub fn dream(world: &mut World, mob: Entity) {
    if world.get::<Dreaming>(mob).is_some() {
        return;
    }
    let nouns = bedsheet_nouns(world, mob);
    let extra = crate::service::dream_pool(world, mob);
    let has_strings = world.contains_resource::<DreamStrings>();
    // `pick_weight(dream_pool)`: random 1000, hear_something 500, then the extras.
    let mut pool: Vec<(usize, u32)> = Vec::new();
    if has_strings {
        pool.extend([(usize::MAX, 1000), (usize::MAX - 1, 500)]);
    }
    pool.extend(extra.iter().enumerate().map(|(i, d)| (i, d.weight)));
    let total: u32 = pool.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return;
    }
    let mut roll = world.resource_mut::<DreamRng>().0.range(1, total);
    let mut picked = pool[pool.len() - 1].0;
    for &(i, weight) in &pool {
        if roll <= weight {
            picked = i;
            break;
        }
        roll -= weight;
    }
    let (kind, source) = match picked {
        usize::MAX => (DreamKind::Random, DreamSource::RandomOf(nouns)),
        i if i == usize::MAX - 1 => (DreamKind::HearSomething, DreamSource::Fragments(Vec::new())),
        i => (extra[i].kind, extra[i].source.clone()),
    };
    let fragments = match source {
        DreamSource::Fragments(f) if kind != DreamKind::HearSomething => f,
        DreamSource::Fragments(_) => hear_something(&mut world.resource_mut::<DreamRng>().0),
        DreamSource::RandomOf(nouns) => {
            if !has_strings {
                return;
            }
            world.resource_scope(|world, strings: Mut<DreamStrings>| {
                random_dream(&strings, &nouns, &mut world.resource_mut::<DreamRng>().0)
            })
        }
    };
    if fragments.is_empty() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(Dreaming {
        fragments,
        next_at: now,
        kind,
    });
    crate::service::dream_started(world, mob, kind);
}

/// `get_dream_nouns`: the `dream_messages` of the bedsheets on the sleeper's tile.
fn bedsheet_nouns(world: &World, mob: Entity) -> Vec<&'static str> {
    crate::status_effects::tile_paths(world, mob)
        .iter()
        .filter_map(|path| bedsheet_dreams(path))
        .flat_map(|n| n.iter().copied())
        .collect()
}

/// The `dream_messages` of the bedsheet type `path`.
pub fn bedsheet_dreams(path: &str) -> Option<&'static [&'static str]> {
    BEDSHEET_DREAMS
        .iter()
        .find(|(root, _)| ss13_mapload::fidelity::is_under(path, root))
        .map(|(_, nouns)| *nouns)
}

fn pick<'a>(rng: &mut CombatRng, list: &'a [String]) -> Option<&'a str> {
    if list.is_empty() {
        return None;
    }
    Some(&list[rng.range(0, list.len() as u32 - 1) as usize])
}

/// `%ADJECTIVE%` filled (50%) or dropped, then `%A%` turned into DM's `\a`.
fn fill(rng: &mut CombatRng, strings: &DreamStrings, fragment: &str) -> String {
    let mut text = if rng.chance(50.0)
        && let Some(adjective) = pick(rng, &strings.adjectives)
    {
        fragment.replace("%ADJECTIVE%", adjective)
    } else {
        fragment.replace("%ADJECTIVE% ", "")
    };
    if text.contains("%A% ") {
        text = with_article(&text.replace("%A% ", ""));
    }
    text
}

/// DM `"\a [text]"`: "an" before a vowel, "a" before anything else, nothing before a proper
/// (capitalised) noun.
pub fn with_article(text: &str) -> String {
    match text.chars().next() {
        Some(c) if c.is_uppercase() => text.to_owned(),
        Some(c) if "aeiouAEIOU".contains(c) => format!("an {text}"),
        Some(_) => format!("a {text}"),
        None => String::new(),
    }
}

/// `/datum/dream/random/GenerateDream`.
pub fn random_dream(strings: &DreamStrings, nouns: &[&str], rng: &mut CombatRng) -> Vec<Fragment> {
    let Some(subject) = (if !nouns.is_empty() && rng.chance(90.0) {
        Some(nouns[rng.range(0, nouns.len() as u32 - 1) as usize].to_owned())
    } else {
        pick(rng, &strings.dream_strings).map(str::to_owned)
    }) else {
        return Vec::new();
    };
    let mut out = vec![
        Fragment::Text("you see".into()),
        Fragment::Text(fill(rng, strings, &subject)),
    ];
    let verb = if rng.chance(50.0) {
        let mut verb = String::new();
        if rng.chance(35.0)
            && let Some(adverb) = pick(rng, &strings.adverbs)
        {
            verb.push_str(adverb);
            verb.push(' ');
        }
        verb.push_str(pick(rng, &strings.ing_verbs).unwrap_or_default());
        verb
    } else {
        format!("will {}", pick(rng, &strings.verbs).unwrap_or_default())
    };
    out.push(Fragment::Text(verb));
    if rng.chance(25.0) {
        return out;
    }
    if let Some(object) = pick(rng, &strings.dream_strings) {
        out.push(Fragment::Text(fill(rng, strings, object)));
    }
    out
}

/// `/datum/dream/hear_something/GenerateDream`.
pub fn hear_something(rng: &mut CombatRng) -> Vec<Fragment> {
    const OPENINGS: [&str; 4] = [
        "you wind up a toy",
        "you hear something strange",
        "you pick out a record to play",
        "you hit shuffle on your music player",
    ];
    vec![
        Fragment::Text(OPENINGS[rng.range(0, 3) as usize].into()),
        Fragment::Sound,
        Fragment::Text("it reminds you of something".into()),
    ]
}

/// `dream_sequence`: the next fragment of every dream that is due, or its end.
/// Cost: one query over the (few) dreaming mobs per frame.
fn dream_sequence(world: &mut World, mut dreamers: Local<Vec<Entity>>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    dreamers.clear();
    dreamers.extend(
        world
            .query::<(Entity, &Dreaming)>()
            .iter(world)
            .filter(|(_, d)| d.next_at <= now)
            .map(|(e, _)| e),
    );
    for &mob in dreamers.iter() {
        let asleep = crate::status_effects::knocked_out(world, mob)
            && world.get::<Stat>(mob).is_none_or(|&s| s < Stat::HardCrit);
        let next = world
            .get_mut::<Dreaming>(mob)
            .filter(|d| !d.fragments.is_empty())
            .map(|mut d| d.fragments.remove(0));
        let (true, Some(fragment)) = (asleep, next) else {
            world.entity_mut(mob).remove::<Dreaming>();
            continue;
        };
        let text = match fragment {
            Fragment::Text(text) => text,
            Fragment::Sound => {
                play_random_sound(world, mob);
                HEARD.to_owned()
            }
        };
        crate::chem::tell_mob(world, mob, Span::Notice, &format!("... {text} ..."));
        let wait = f64::from(world.resource_mut::<DreamRng>().0.range(10, 30)) / 10.0;
        if world
            .get::<Dreaming>(mob)
            .is_none_or(|d| d.fragments.is_empty())
        {
            world.entity_mut(mob).remove::<Dreaming>();
        } else if let Some(mut dreaming) = world.get_mut::<Dreaming>(mob) {
            dreaming.next_at = now + wait;
        }
    }
}

/// `PlayRandomSound`: `SEND_SOUND(dreamer, sound(pick(...)))`.
fn play_random_sound(world: &mut World, mob: Entity) {
    let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    let i = world
        .resource_mut::<DreamRng>()
        .0
        .range(0, DREAM_SOUNDS.len() as u32 - 1) as usize;
    if world.contains_resource::<bevy::ecs::message::Messages<SendSound>>() {
        world.write_message(SendSound {
            client,
            sound: SoundEvent {
                file: DREAM_SOUNDS[i].into(),
                volume: 100.0,
                speed: 1.0,
                offset: None,
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings() -> DreamStrings {
        let v = |s: &[&str]| s.iter().map(|s| (*s).to_owned()).collect();
        DreamStrings {
            dream_strings: v(&["%A% %ADJECTIVE% admiral", "%ADJECTIVE% blood"]),
            adjectives: v(&["old"]),
            adverbs: v(&["angrily"]),
            ing_verbs: v(&["dancing"]),
            verbs: v(&["dance"]),
        }
    }

    fn texts(fragments: &[Fragment]) -> Vec<String> {
        fragments
            .iter()
            .map(|f| match f {
                Fragment::Text(t) => t.clone(),
                Fragment::Sound => HEARD.into(),
            })
            .collect()
    }

    #[test]
    fn random_dreams_see_a_subject_and_a_verb() {
        let strings = strings();
        for seed in 0..200 {
            let mut rng = CombatRng(seed);
            let dream = texts(&random_dream(&strings, &[], &mut rng));
            assert!((3..=4).contains(&dream.len()), "{dream:?}");
            assert_eq!(dream[0], "you see");
            assert!(
                ["an old admiral", "an admiral", "old blood", "blood"].contains(&dream[1].as_str()),
                "{dream:?}"
            );
            assert!(
                ["dancing", "angrily dancing", "will dance"].contains(&dream[2].as_str()),
                "{dream:?}"
            );
        }
    }

    #[test]
    fn bedsheet_nouns_are_the_most_specific_ones() {
        assert_eq!(
            bedsheet_dreams("/obj/item/bedsheet/blue/double"),
            Some(&["blue"][..])
        );
        assert_eq!(
            bedsheet_dreams("/obj/item/bedsheet/double"),
            Some(&["white"][..])
        );
        assert_eq!(
            bedsheet_dreams("/obj/item/bedsheet/rd/royal_cape").map(|n| n[0]),
            Some("mining")
        );
        assert_eq!(bedsheet_dreams("/obj/item/pillow"), None);
        assert_eq!(with_article("Captain"), "Captain");
    }

    #[test]
    fn hearing_something_plays_a_sound_between_two_lines() {
        let dream = hear_something(&mut CombatRng(7));
        assert_eq!(dream.len(), 3);
        assert_eq!(dream[1], Fragment::Sound);
        assert_eq!(
            dream[2],
            Fragment::Text("it reminds you of something".into())
        );
    }
}
