//! Genetics (server), after tg `/datum/dna` (`code/datums/dna/dna.dm`), `/datum/mutation`
//! (`code/datums/mutations/`), the DNA console and scanner and DNA injectors ([`console`]).
//!
//! Every human body gets a [`Dna`]: unique enzymes (a crew member's are its record's DNA hash),
//! the unique identity blocks of its look, and a mutation index of [`DNA_MUTATION_BLOCKS`]
//! chipped gene sequences picked from the unlocked mutations (`generate_dna_blocks`). A block
//! whose sequence is complete activates its mutation (`domutcheck`); mutators and radiation add
//! mutations outside the sequence, which cost stability (`update_instability`); at 0 the
//! genetic breakdown (`dna_melt`) ends in a meltdown after 60 s unless the mutations go.
//! Mutadone removes them.
// shortcut: a starter set of mutations (see [`MUTATIONS`], their effects in [`effects`]); the
// monkey block (`/datum/mutation/race`) is not always in the index as tg adds it; no unique
// features, chromosomes, combined recipes or mutation overlays. Cold and heat adaptation carry their tg numbers but no effect yet (body
// temperature damage has no resist traits). Add each with the system it needs.

pub mod console;
pub mod dna_vault;
pub mod effects;
pub mod mutation_toxin;

use std::sync::Arc;

use bevy::prelude::*;
use ss13_mapload::spawner::Rng;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, LightSource, Optics};

use crate::health::{DamageType, Health, Stat, apply_damage};
use crate::human::{HumanLook, SKIN_TONES};
use crate::status_effects::Incap;

/// `DNA_MUTATION_BLOCKS`.
pub const DNA_MUTATION_BLOCKS: usize = 8;
/// `DNA_SEQUENCE_LENGTH`: base pairs per block.
pub const DNA_SEQUENCE_LENGTH: usize = 4;
/// `DNA_BLOCK_SIZE`.
pub const DNA_BLOCK_SIZE: usize = 3;
/// `DNA_UNIQUE_ENZYMES_LEN`.
pub const DNA_UNIQUE_ENZYMES_LEN: usize = 32;
/// `GENDERS`, `G_MALE`, `G_FEMALE`.
const GENDERS: u32 = 4;
const G_MALE: u32 = 1;
const G_FEMALE: u32 = 2;
/// `/datum/status_effect/dna_melt` `duration`.
const DNA_MELT_SECS: f64 = 60.0;
/// `SSmobs.wait`: `on_life` of mutations.
const LIFE_SECS: f64 = 2.0;

/// `MUTATION_SOURCE_ACTIVATED` / `MUTATION_SOURCE_MUTATOR`.
pub const SOURCE_ACTIVATED: u8 = 1;
pub const SOURCE_MUTATOR: u8 = 2;
/// `MUTATION_SOURCE_BURDENED_TRAUMA` (`crate::service` punished god).
pub const SOURCE_BURDENED: u8 = 4;
/// `MUTATION_SOURCE_CLOWN_CLUMSINESS`: the clown's own, which mutadone leaves.
pub const SOURCE_CLOWN: u8 = 8;
/// `GLOB.standard_mutation_sources`.
pub const STANDARD_SOURCES: u8 = SOURCE_ACTIVATED | SOURCE_MUTATOR;

/// `POSITIVE`, `NEGATIVE`, `MINOR_NEGATIVE`.
pub const POSITIVE: u8 = 1;
pub const NEGATIVE: u8 = 2;
pub const MINOR_NEGATIVE: u8 = 4;

/// `POSITIVE_INSTABILITY_*` / `NEGATIVE_STABILITY_*` (`_mutations.dm`).
const POSITIVE_INSTABILITY_MINI: i32 = 5;
const POSITIVE_INSTABILITY_MINOR: i32 = 10;
const POSITIVE_INSTABILITY_MODERATE: i32 = 25;
const POSITIVE_INSTABILITY_MAJOR: i32 = 35;
const NEGATIVE_STABILITY_MINI: i32 = -10;
const NEGATIVE_STABILITY_MODERATE: i32 = -30;
const NEGATIVE_STABILITY_MAJOR: i32 = -40;

/// What a mutation does here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Glow,
    Cough,
    Epilepsy,
    /// `TRAIT_RESISTLOWPRESSURE` + `TRAIT_RESISTHIGHPRESSURE` (read by `crate::breath`).
    PressureAdaptation,
    /// `/datum/mutation/hulk` ([`effects`]).
    Hulk,
    /// `/datum/mutation/nervousness`: stutters now and then.
    Nervousness,
    /// `/datum/mutation/clumsy`: `TRAIT_CLUMSY` ([`is_clumsy`]).
    Clumsy,
    /// `/datum/mutation/unintelligible`: `TRAIT_UNINTELLIGIBLE_SPEECH` (read by
    /// `crate::medical::traumas::refresh`).
    Unintelligible,
    /// The speech mutations of `ss13_chat::accents`.
    Accent(ss13_chat::accents::Accent),
    /// A `power_path` action.
    Power(effects::Power),
    /// `/datum/mutation/race`: the monkey.
    Race,
    /// `/datum/mutation/stoner`: only Beach Bum (`LANGUAGE_STONER`).
    Stoner,
    /// `/datum/mutation/aoe_moodlet` (`positive`: the comely one).
    Aura {
        positive: bool,
    },
    /// Numbers only (see the module shortcut).
    None,
}

/// A `/datum/mutation` type.
#[derive(Clone, Copy, Debug)]
pub struct MutationDef {
    pub path: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub quality: u8,
    pub instability: i32,
    pub difficulty: u32,
    /// Not in the random pool (`locked`): only injectors give it.
    pub locked: bool,
    pub gain: &'static str,
    pub lose: &'static str,
    pub effect: Effect,
}

/// The ported mutations, tg's values (`blocks = 4` for all).
pub const MUTATIONS: &[MutationDef] = &[
    MutationDef {
        path: "/datum/mutation/glow",
        name: "Glowy",
        desc: "Вы будете излучать свет случайного цвета и интенсивности.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MINI,
        difficulty: 8,
        locked: false,
        gain: "Твоя кожа начинает немного светиться.",
        lose: "",
        effect: Effect::Glow,
    },
    MutationDef {
        path: "/datum/mutation/adaptation/pressure",
        name: "Pressure Adaptation",
        desc: "Странная мутация, которая адаптирует иммунную систему организма к низкому и \
               высокому давлению.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MODERATE,
        difficulty: 16,
        locked: false,
        gain: "Ваше тело испытывает сильное давление.",
        lose: "",
        effect: Effect::PressureAdaptation,
    },
    MutationDef {
        path: "/datum/mutation/adaptation/cold",
        name: "Cold Adaptation",
        desc: "Странная мутация, которая адаптирует иммунную систему организма к низким \
               температурам.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MODERATE,
        difficulty: 16,
        locked: false,
        gain: "Твое тело наполняет освежающий холод.",
        lose: "",
        effect: Effect::None,
    },
    MutationDef {
        path: "/datum/mutation/adaptation/heat",
        name: "Heat Adaptation",
        desc: "Странная мутация, которая адаптирует иммунную систему организма к высоким \
               температурам.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MODERATE,
        difficulty: 16,
        locked: false,
        gain: "Твоё тело наполняет лёгкое тепло.",
        lose: "",
        effect: Effect::None,
    },
    MutationDef {
        path: "/datum/mutation/insulated",
        name: "Insulated",
        desc: "Субъект, подверженный данной мутации, не позволяет провести электрический ток \
               через себя.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MODERATE,
        difficulty: 16,
        locked: false,
        gain: "Кончики твоих пальцев немеют.",
        lose: "Ты снова чувствуешь кончики своих пальцев.",
        effect: Effect::None,
    },
    MutationDef {
        path: "/datum/mutation/cough",
        name: "Cough",
        desc: "Хронический кашель.",
        quality: MINOR_NEGATIVE,
        instability: NEGATIVE_STABILITY_MODERATE,
        difficulty: 8,
        locked: false,
        gain: "Ты начинаешь кашлять.",
        lose: "",
        effect: Effect::Cough,
    },
    MutationDef {
        path: "/datum/mutation/epilepsy",
        name: "Epilepsy",
        desc: "Генетический дефект, из-за которого случаются приступы эпилепсии.",
        quality: NEGATIVE,
        instability: NEGATIVE_STABILITY_MODERATE,
        difficulty: 8,
        locked: false,
        gain: "Ты ощущаешь головную боль.",
        lose: "",
        effect: Effect::Epilepsy,
    },
    MutationDef {
        path: "/datum/mutation/deaf",
        name: "Deafness",
        desc: "Обладатель данного генома полностью глухой.",
        quality: NEGATIVE,
        instability: NEGATIVE_STABILITY_MAJOR,
        difficulty: 8,
        locked: false,
        gain: "Ты ничего не слышишь.",
        lose: "",
        effect: Effect::None,
    },
    MutationDef {
        path: "/datum/mutation/xray",
        name: "X Ray Vision",
        desc: "Странный геном, который позволяет его обладателю видеть пространство между \
               стенами.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MAJOR,
        difficulty: 8,
        locked: true,
        gain: "Стены вдруг исчезли!",
        lose: "",
        effect: Effect::None,
    },
    MutationDef {
        path: "/datum/mutation/hulk",
        name: "Hulk",
        desc: "Слабо изученный геном, заставляющий мышцы своего носителя увеличиваться, подавлять \
               речь и придавать болезненное состояние коже.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MAJOR,
        difficulty: 16,
        locked: true,
        gain: "Ты чувствуешь боль в мышцах!",
        lose: "",
        effect: Effect::Hulk,
    },
    MutationDef {
        path: "/datum/mutation/geladikinesis",
        name: "Geladikinesis",
        desc: "Позволяет обладателю генома сконцентрировать влагу и холод в снег.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MINOR,
        difficulty: 10,
        locked: false,
        gain: "Ты ощущаешь холод в руках.",
        lose: "",
        effect: Effect::Power(effects::Power::Geladikinesis),
    },
    MutationDef {
        path: "/datum/mutation/cindikinesis",
        name: "Cindikinesis",
        desc: "Позволяет обладателю мутации сконцентрировать рядом находящееся тепло в кучу \
               пепла. Вау. Очень интересно.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MINOR,
        difficulty: 10,
        locked: true,
        gain: "Твоя рука кажется тёплой.",
        lose: "",
        effect: Effect::Power(effects::Power::Cindikinesis),
    },
    MutationDef {
        path: "/datum/mutation/clumsy",
        name: "Clumsiness",
        desc: "Данный геном подавляет определённые функции мозга, из-за чего его обладатель \
               выглядит неуклюжим.",
        quality: MINOR_NEGATIVE,
        instability: NEGATIVE_STABILITY_MAJOR,
        difficulty: 8,
        locked: false,
        gain: "Тебя охватывает легкомыслие",
        lose: "",
        effect: Effect::Clumsy,
    },
    MutationDef {
        path: "/datum/mutation/nervousness",
        name: "Nervousness",
        desc: "Обладатель данного генома заикается.",
        quality: MINOR_NEGATIVE,
        instability: 0,
        difficulty: 8,
        locked: false,
        gain: "Ты нервничаешь.",
        lose: "",
        effect: Effect::Nervousness,
    },
    MutationDef {
        path: "/datum/mutation/unintelligible",
        name: "Unintelligible",
        desc: "Геном частично подавляет отдел головного мозга, отвечающий за речевой аппарат, \
               сильно искажая речь.",
        quality: NEGATIVE,
        instability: NEGATIVE_STABILITY_MODERATE,
        difficulty: 8,
        locked: false,
        gain: "Ты чувствуешь себя неспособным сформировать предложение!",
        lose: "Твой ум, кажется более ясным.",
        effect: Effect::Unintelligible,
    },
    MutationDef {
        path: "/datum/mutation/swedish",
        name: "Swedish",
        desc: "Ужасающая мутация, котороя происходит из далёкого прошлого. Считается, что она \
               была полностью искоренена после 2037.",
        quality: MINOR_NEGATIVE,
        instability: 0,
        difficulty: 8,
        locked: false,
        gain: "Ты ощущаешь себя шведом, что бы это ни значило.",
        lose: "Ты перестаешь ощущать себя шведом.",
        effect: Effect::Accent(ss13_chat::accents::Accent::Swedish),
    },
    MutationDef {
        path: "/datum/mutation/chav",
        name: "Chav",
        desc: "Неизвестно.",
        quality: MINOR_NEGATIVE,
        instability: 0,
        difficulty: 8,
        locked: false,
        gain: "Ты ощущаешь себя мудаком, не так ли?",
        lose: "Ты перестаешь ощущать себя грубым и нахальным.",
        effect: Effect::Accent(ss13_chat::accents::Accent::Chav),
    },
    MutationDef {
        path: "/datum/mutation/elvis",
        name: "Elvis",
        desc: "Ужасающая мутация, названная в честь нулевого пациента.",
        quality: MINOR_NEGATIVE,
        instability: 0,
        difficulty: 8,
        locked: false,
        gain: "Ты хорошо себя чувствуешь, куколка.",
        lose: "Ты чувствуешь, что немного меньше разговоров не помешало бы.",
        effect: Effect::Accent(ss13_chat::accents::Accent::Elvis),
    },
    MutationDef {
        path: "/datum/mutation/medieval",
        name: "Medieval",
        desc: "Ужасная мутация, происходящая из далёкого прошлого, считается, что была \
               распространённым геномом во всей старой Европе.",
        quality: MINOR_NEGATIVE,
        instability: 0,
        difficulty: 8,
        locked: false,
        gain: "Ты чувствуешь себя стремящимся к Святому Граали!",
        lose: "Ты больше не стремишься к чему-либо.",
        effect: Effect::Accent(ss13_chat::accents::Accent::Medieval),
    },
    MutationDef {
        path: "/datum/mutation/piglatin",
        name: "Pig Latin",
        desc: "Историки говорят, что в 2020 году человечество полностью говорило на этом \
               мистическом языке.",
        quality: MINOR_NEGATIVE,
        instability: 0,
        difficulty: 8,
        locked: false,
        gain: "Ты ощущаешь себя мамонтом.",
        lose: "Кажется, это прошло.",
        effect: Effect::Accent(ss13_chat::accents::Accent::PigLatin),
    },
    MutationDef {
        path: "/datum/mutation/cryokinesis",
        name: "Cryokinesis",
        desc: "Даёт возможность в любой момент притянуть негативную энергию из морозной пустоты, \
               чтобы заморозить окружение вокруг субъекта.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MODERATE,
        difficulty: 12,
        locked: false,
        gain: "Ты ощущаешь холод в руках.",
        lose: "",
        effect: Effect::Power(effects::Power::Cryokinesis),
    },
    MutationDef {
        path: "/datum/mutation/pyrokinesis",
        name: "Pyrokinesis",
        desc: "Притягивает положительную энергию окружения для повышения температуры вокруг \
               субъекта.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MODERATE,
        difficulty: 12,
        locked: true,
        gain: "Твоя рука кажется горячей!",
        lose: "",
        effect: Effect::Power(effects::Power::Pyrokinesis),
    },
    MutationDef {
        path: "/datum/mutation/firebreath",
        name: "Fire Breath",
        desc: "Древняя мутация, позволяющая ящерицам дышать огнём.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MODERATE,
        difficulty: 12,
        locked: true,
        gain: "Ты ощущаешь жжение в горле!",
        lose: "Жжение в горле проходит.",
        effect: Effect::Power(effects::Power::FireBreath),
    },
    MutationDef {
        path: "/datum/mutation/stoner",
        name: "Stoner",
        desc: "Обычная мутация, которая сильно понижает интеллект.",
        quality: NEGATIVE,
        instability: 0,
        difficulty: 8,
        locked: false,
        gain: "Ты чувствуешь себя...максимально расслабленным, чувак!",
        lose: "Ты чувствуешь, что твоё восприятие времени стало лучше.",
        effect: Effect::Stoner,
    },
    MutationDef {
        path: "/datum/mutation/heckacious",
        name: "Heckacious Larincks",
        desc: "чел что ЖЕЛАЮТ твои слова чувак...........",
        quality: MINOR_NEGATIVE,
        instability: 0,
        difficulty: 8,
        locked: true,
        gain: "ЧЁРТ, чувак. твоё горло как ДЕРЬМО.",
        lose: "Демоническое существо, овладевшее твоей гортанью, наконец-то ослабило свою \
               хватку.",
        effect: Effect::Accent(ss13_chat::accents::Accent::Heckacious),
    },
    MutationDef {
        path: "/datum/mutation/race",
        name: "Monkified",
        desc: "Странный геном который, по мнению общества, отличает обезьяну от человека.",
        quality: NEGATIVE,
        instability: NEGATIVE_STABILITY_MAJOR,
        difficulty: 8,
        locked: true,
        gain: "Ты чувствуешь необычно по-обезьяни.",
        lose: "Ты чувствуешь себя как раньше.",
        effect: Effect::Race,
    },
    MutationDef {
        path: "/datum/mutation/aoe_moodlet",
        name: "Extremely Unsightly",
        desc: "The subject's body - and especially their face - is unnaturally unsightly, \
               causing a negative reaction from those around them. Wearing a mask will dampen \
               the effect.",
        quality: NEGATIVE,
        instability: NEGATIVE_STABILITY_MINI,
        difficulty: 12,
        locked: false,
        gain: "You feel pretty in your own way.",
        lose: "You feel less self-conscious.",
        effect: Effect::Aura { positive: false },
    },
    MutationDef {
        path: "/datum/mutation/aoe_moodlet/positive",
        name: "Extremely Comely",
        desc: "The subject's body - and especially their face - is unnaturally comely, causing \
               a positive reaction from those around them. Wearing a mask will dampen the \
               effect.",
        quality: POSITIVE,
        instability: POSITIVE_INSTABILITY_MINOR,
        difficulty: 12,
        locked: false,
        gain: "You feel pretty in your own way.",
        lose: "You feel less self-conscious.",
        effect: Effect::Aura { positive: true },
    },
];

/// Index into [`MUTATIONS`].
pub fn mutation_id(path: &str) -> Option<usize> {
    MUTATIONS.iter().position(|m| m.path == path)
}

/// `GLOB.full_sequences` and `GLOB.alias_mutations` of the round (`setupGenetics`).
#[derive(Resource, Clone, Debug)]
pub struct GeneticsDb {
    pub full_sequences: Vec<String>,
    /// `alias` per mutation ("Mutation 3").
    pub aliases: Vec<String>,
}

impl GeneticsDb {
    pub fn new(rng: &mut Rng) -> Self {
        let mut order: Vec<usize> = (0..MUTATIONS.len()).collect();
        shuffle(&mut order, rng);
        let mut aliases = vec![String::new(); MUTATIONS.len()];
        for (i, &m) in order.iter().enumerate() {
            aliases[m] = format!("Mutation {}", i + 1);
        }
        let full_sequences = MUTATIONS.iter().map(|_| gene_sequence(4, rng)).collect();
        Self {
            full_sequences,
            aliases,
        }
    }

    pub fn by_alias(&self, alias: &str) -> Option<usize> {
        self.aliases.iter().position(|a| a == alias)
    }
}

/// The genetics rng (seeded from the map seed by `crate::server::seed_rngs`).
#[derive(Resource)]
pub struct GeneticsRng(pub Rng);

impl Default for GeneticsRng {
    fn default() -> Self {
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Self(Rng::new(seed))
    }
}

/// Mutations researched by the station (techweb `discovered_mutations`).
#[derive(Resource, Default, Clone, Debug)]
pub struct DiscoveredMutations(pub Vec<usize>);

/// An active `/datum/mutation` on a body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveMutation {
    pub id: usize,
    pub sources: u8,
    pub scrambled: bool,
}

/// `/datum/dna` of a human.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Dna {
    pub unique_enzymes: String,
    pub unique_identity: String,
    /// `mutation_index`: mutation and its current sequence (`X` = missing base).
    pub mutation_index: Vec<(usize, String)>,
    pub default_mutation_genes: Vec<String>,
    pub mutations: Vec<ActiveMutation>,
    pub stability: i32,
    /// `/datum/status_effect/dna_melt` ends at this game time.
    pub melt_until: Option<f64>,
}

impl Dna {
    pub fn get_mutation(&self, id: usize) -> Option<&ActiveMutation> {
        self.mutations.iter().find(|m| m.id == id)
    }

    pub fn sequence(&self, id: usize) -> Option<&str> {
        self.mutation_index
            .iter()
            .find(|(m, _)| *m == id)
            .map(|(_, s)| s.as_str())
    }

    /// `mutation_in_sequence`.
    pub fn in_sequence(&self, id: usize) -> bool {
        self.sequence(id).is_some()
    }

    /// `is_gene_active`.
    pub fn is_gene_active(&self, db: &GeneticsDb, id: usize) -> bool {
        self.sequence(id) == Some(db.full_sequences[id].as_str())
    }
}

/// `pick` over BYOND's `rand`: a shuffle like `shuffle_inplace`.
fn shuffle<T>(list: &mut [T], rng: &mut Rng) {
    for i in (1..list.len()).rev() {
        let j = rng.range(0, i as i64) as usize;
        list.swap(i, j);
    }
}

/// `generate_gene_sequence(length)`: `length * DNA_SEQUENCE_LENGTH` active pairs.
pub fn gene_sequence(length: usize, rng: &mut Rng) -> String {
    const PAIRS: [&str; 4] = ["AT", "TA", "GC", "CG"];
    (0..length * DNA_SEQUENCE_LENGTH)
        .map(|_| PAIRS[rng.range(0, 3) as usize])
        .collect()
}

/// `create_sequence(mutation, FALSE, difficulty)`: `difficulty + rand(-2, 4)` bases X-ed out.
pub fn chipped_sequence(full: &str, difficulty: u32, rng: &mut Rng) -> String {
    let mut seq: Vec<u8> = full.as_bytes().to_vec();
    let count = (difficulty as i64 + rng.range(-2, 4)).max(0);
    for _ in 0..count {
        let at = rng.range(0, seq.len() as i64 - 1) as usize;
        seq[at] = b'X';
    }
    String::from_utf8(seq).expect("ascii")
}

/// The `pulse_gene` letter cycle (`gene_letters = A, T, C, G`): next or previous.
pub fn cycle_gene(current: u8, next: bool) -> u8 {
    const LETTERS: [u8; 4] = *b"ATCG";
    let at = LETTERS.iter().position(|&l| l == current);
    match (at, next) {
        (Some(i), true) => LETTERS[(i + 1) % 4],
        (None, true) => LETTERS[0],
        (Some(i), false) => LETTERS[(i + 3) % 4],
        (None, false) => LETTERS[3],
    }
}

/// `construct_block(value, values)`: `value` of `values` as `DNA_BLOCK_SIZE` hex digits, spread
/// randomly inside its slice of the range.
pub fn construct_block(value: u32, values: u32, rng: &mut Rng) -> String {
    let width = 16u32.pow(DNA_BLOCK_SIZE as u32) / values;
    let value = value.max(1) * width - rng.range(1, width as i64) as u32;
    format!("{value:0width$X}", width = DNA_BLOCK_SIZE)
}

/// `deconstruct_block(value, values)`.
pub fn deconstruct_block(hex: &str, values: u32) -> u32 {
    let width = 16u32.pow(DNA_BLOCK_SIZE as u32) / values;
    let value = u32::from_str_radix(hex, 16).unwrap_or(0) / width + 1;
    value.min(values)
}

/// `generate_unique_identity`: the gender, skin tone, eye colors and hair color blocks.
// shortcut: no hair style, facial hair or gradient blocks (no accessory lists yet).
pub fn unique_identity(look: &HumanLook, rng: &mut Rng) -> String {
    let gender = construct_block(if look.female { G_FEMALE } else { G_MALE }, GENDERS, rng);
    let tone = SKIN_TONES
        .iter()
        .position(|(n, _)| *n == look.skin_tone)
        .map_or(1, |i| i as u32 + 1);
    let skin = construct_block(tone, SKIN_TONES.len() as u32, rng);
    let hex = |c: [u8; 3]| format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]);
    format!(
        "{gender}{skin}{}{}{}",
        hex(look.eye_color),
        hex(look.eye_color),
        hex(look.hair_color)
    )
}

/// `stability` after `update_instability`: 100 minus the instability of every mutator-sourced
/// (or stabilizing) mutation.
pub fn stability(mutations: &[ActiveMutation]) -> i32 {
    100 - mutations
        .iter()
        .filter(|m| m.sources & SOURCE_MUTATOR != 0 || MUTATIONS[m.id].instability < 0)
        .map(|m| MUTATIONS[m.id].instability)
        .sum::<i32>()
}

/// The `update_instability` alert for a stability.
pub fn instability_message(stability: i32) -> Option<(Span, &'static str)> {
    Some(match stability {
        91.. => return None,
        70..=90 => (Span::Warning, "Вы дрожите."),
        60..=69 => (Span::Warning, "Вам холодно."),
        40..=59 => (Span::Warning, "Вам плохо."),
        20..=39 => (Span::Warning, "Кажется, ваша кожа шевелится."),
        1..=19 => (Span::Warning, "Вы чувствуете, как горят ваши клетки."),
        _ => (
            Span::BoldWarning,
            "Вы чувствуете, как взрывается ваша ДНК, надо срочно что-то делать!",
        ),
    })
}

/// `generate_dna_blocks`: [`DNA_MUTATION_BLOCKS`] chipped sequences of random unlocked
/// mutations.
pub fn generate_dna_blocks(db: &GeneticsDb, rng: &mut Rng) -> (Vec<(usize, String)>, Vec<String>) {
    let mut pool: Vec<usize> = (0..MUTATIONS.len())
        .filter(|&i| !MUTATIONS[i].locked)
        .collect();
    shuffle(&mut pool, rng);
    let index: Vec<(usize, String)> = pool
        .into_iter()
        .take(DNA_MUTATION_BLOCKS)
        .map(|m| {
            let seq = chipped_sequence(&db.full_sequences[m], MUTATIONS[m].difficulty, rng);
            (m, seq)
        })
        .collect();
    let defaults = index.iter().map(|(_, s)| s.clone()).collect();
    (index, defaults)
}

/// The round's [`GeneticsDb`], made on first use.
pub fn db(world: &mut World) -> Arc<GeneticsDb> {
    if let Some(db) = world.get_resource::<SharedDb>() {
        return db.0.clone();
    }
    let db = Arc::new(GeneticsDb::new(
        &mut world.get_resource_or_init::<GeneticsRng>().0,
    ));
    world.insert_resource(SharedDb(db.clone()));
    db
}

/// The round's [`GeneticsDb`] (see [`db`]).
#[derive(Resource)]
pub struct SharedDb(pub Arc<GeneticsDb>);

/// `initialize_dna` of a new human body.
pub fn new_dna(world: &mut World, look: &HumanLook) -> Dna {
    let db = db(world);
    let rng = &mut world.get_resource_or_init::<GeneticsRng>().0;
    let unique_enzymes = format!("{:016x}{:016x}", rng.next_u64(), rng.next_u64());
    let unique_identity = unique_identity(look, rng);
    let (mutation_index, default_mutation_genes) = generate_dna_blocks(&db, rng);
    Dna {
        unique_enzymes,
        unique_identity,
        mutation_index,
        default_mutation_genes,
        mutations: Vec::new(),
        stability: 100,
        melt_until: None,
    }
}

pub struct GeneticsServerPlugin;

impl Plugin for GeneticsServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(dna_vault::DnaVaultPlugin);
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` of the DNA console (its window) and scanner (open/close).
        app.add_click_handler(ClickPhase::EmptyHand, 2600, |w, c| {
            crate::genetics::console::interact(w, c.client, c.mob, c.target)
        });
        // tg `dnainjector/attack`.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3600, |w, c| {
            c.held.is_some_and(|h| {
                crate::genetics::console::item_interaction(
                    w,
                    crate::tools::Actor {
                        client: c.client,
                        mob: c.mob,
                        item: Some(h),
                    },
                    h,
                    c.target,
                )
            })
        });
        app.init_resource::<DiscoveredMutations>()
            .add_observer(attach_dna)
            .add_systems(Update, mutations_life)
            .add_systems(
                Update,
                effects::hulk_clicks
                    .after(crate::security::security_clicks)
                    .before(crate::combat::MeleeSystems),
            )
            .add_systems(
                Update,
                (effects::trigger_powers, effects::pointed_clicks)
                    .in_set(ss13_tgui::server::UiSystems::Handle),
            );
        console::build(app);
    }
}

fn attach_dna(add: On<Add<HumanLook>>, mut commands: Commands) {
    let mob = add.entity;
    commands.queue(move |world: &mut World| ensure_dna(world, mob));
}

/// A human body without DNA gets its own (`create_dna`).
pub fn ensure_dna(world: &mut World, mob: Entity) {
    let Some(look) = world.get::<HumanLook>(mob).cloned() else {
        return;
    };
    if world.get::<Dna>(mob).is_some() {
        return;
    }
    let dna = new_dna(world, &look);
    world.entity_mut(mob).insert(dna);
}

fn tell(world: &mut World, mob: Entity, span: Span, text: &str) {
    if !text.is_empty() {
        crate::chem::tell_mob(world, mob, span, text);
    }
}

/// `add_mutation(mutation, sources)`. False when nothing changed.
pub fn add_mutation(world: &mut World, mob: Entity, id: usize, sources: u8) -> bool {
    // `can_mutate()`: an organic body that is not `TRAIT_GENELESS`.
    if !crate::species::can_mutate(world, mob) {
        return false;
    }
    // `on_acquiring` of the mutation itself fails: nothing is added.
    let had = world
        .get::<Dna>(mob)
        .is_some_and(|d| d.get_mutation(id).is_some());
    if !had && !effects::can_acquire(world, mob, MUTATIONS[id].effect) {
        return false;
    }
    let db = db(world);
    let Some(mut dna) = world.get::<Dna>(mob).cloned() else {
        return false;
    };
    let new = match dna.mutations.iter_mut().find(|m| m.id == id) {
        Some(m) => {
            if sources & !m.sources == 0 {
                return false;
            }
            m.sources |= sources;
            false
        }
        None => {
            dna.mutations.push(ActiveMutation {
                id,
                sources,
                scrambled: false,
            });
            true
        }
    };
    // `set_se(1, mutation)`.
    if sources & SOURCE_ACTIVATED != 0
        && let Some(i) = dna.mutation_index.iter().position(|(m, _)| *m == id)
    {
        dna.mutation_index[i].1 = db.full_sequences[id].clone();
        dna.default_mutation_genes[i] = db.full_sequences[id].clone();
    }
    world.entity_mut(mob).insert(dna);
    if new {
        on_acquiring(world, mob, id);
    }
    update_instability(world, mob, true);
    true
}

/// `remove_mutation(mutation, sources)`. False when it had none of `sources`.
pub fn remove_mutation(world: &mut World, mob: Entity, id: usize, sources: u8) -> bool {
    let db = db(world);
    let Some(mut dna) = world.get::<Dna>(mob).cloned() else {
        return false;
    };
    let Some(at) = dna
        .mutations
        .iter()
        .position(|m| m.id == id && m.sources & sources != 0)
    else {
        return false;
    };
    dna.mutations[at].sources &= !sources;
    // `set_se(0, mutation)`: a complete block is chipped again.
    if sources & SOURCE_ACTIVATED != 0
        && let Some(i) = dna.mutation_index.iter().position(|(m, _)| *m == id)
        && dna.mutation_index[i].1 == db.full_sequences[id]
    {
        let rng = &mut world.get_resource_or_init::<GeneticsRng>().0;
        let seq = chipped_sequence(&db.full_sequences[id], MUTATIONS[id].difficulty, rng);
        dna.default_mutation_genes[i] = seq.clone();
        dna.mutation_index[i].1 = seq;
    }
    let lost = dna.mutations[at].sources == 0;
    if lost {
        dna.mutations.remove(at);
    }
    world.entity_mut(mob).insert(dna);
    if lost {
        on_losing(world, mob, id);
    }
    update_instability(world, mob, false);
    true
}

/// `remove_all_mutations()` (mutadone, the meltdown).
pub fn remove_all_mutations(world: &mut World, mob: Entity) {
    let ids: Vec<usize> = world
        .get::<Dna>(mob)
        .map(|d| d.mutations.iter().map(|m| m.id).collect())
        .unwrap_or_default();
    for id in ids {
        remove_mutation(world, mob, id, STANDARD_SOURCES);
    }
}

/// `domutcheck` -> `check_block` of every block: complete blocks activate, broken ones stop.
pub fn domutcheck(world: &mut World, mob: Entity) {
    let db = db(world);
    let Some(dna) = world.get::<Dna>(mob).cloned() else {
        return;
    };
    for (id, _) in &dna.mutation_index {
        let active = dna.is_gene_active(&db, *id);
        let has = dna.get_mutation(*id);
        if active && has.is_none() {
            add_mutation(world, mob, *id, SOURCE_ACTIVATED);
        } else if !active && has.is_some_and(|m| m.sources & SOURCE_ACTIVATED != 0) {
            remove_mutation(world, mob, *id, SOURCE_ACTIVATED);
        }
    }
}

/// `easy_random_mutate(quality)`: a random mutation of that quality from the body's sequence
/// that it does not have yet, activated.
pub fn easy_random_mutate(world: &mut World, mob: Entity, quality: u8) -> Option<usize> {
    let dna = world.get::<Dna>(mob)?.clone();
    let possible: Vec<usize> = dna
        .mutation_index
        .iter()
        .map(|(m, _)| *m)
        .filter(|&m| MUTATIONS[m].quality & quality != 0 && dna.get_mutation(m).is_none())
        .collect();
    if possible.is_empty() {
        return None;
    }
    let rng = &mut world.get_resource_or_init::<GeneticsRng>().0;
    let id = possible[rng.range(0, possible.len() as i64 - 1) as usize];
    add_mutation(world, mob, id, SOURCE_ACTIVATED);
    Some(id)
}

/// `random_mutate_unique_identity`: one identity block replaced by random hex.
pub fn random_mutate_unique_identity(world: &mut World, mob: Entity) {
    let Some(mut dna) = world.get::<Dna>(mob).cloned() else {
        return;
    };
    // Block lengths of [`unique_identity`].
    const BLOCKS: [(usize, usize); 4] = [(0, 3), (3, 3), (6, 12), (18, 6)];
    let rng = &mut world.get_resource_or_init::<GeneticsRng>().0;
    let (start, len) = BLOCKS[rng.range(0, 3) as usize];
    let value: String = (0..len)
        .map(|_| char::from_digit(rng.range(0, 15) as u32, 16).expect("hex"))
        .collect::<String>()
        .to_uppercase();
    if dna.unique_identity.len() >= start + len {
        dna.unique_identity
            .replace_range(start..start + len, &value);
        world.entity_mut(mob).insert(dna);
    }
}

/// `update_instability(alert)`.
fn update_instability(world: &mut World, mob: Entity, alert: bool) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut dna) = world.get_mut::<Dna>(mob) else {
        return;
    };
    let old = dna.stability;
    let new = stability(&dna.mutations);
    dna.stability = new;
    // `apply_status_effect(/datum/status_effect/dna_melt)` (`STATUS_EFFECT_REPLACE`).
    let melting = new <= 0;
    if melting {
        dna.melt_until = Some(now + DNA_MELT_SECS);
    }
    if melting {
        tell(
            world,
            mob,
            Span::BoldWarning,
            "Тело не выдерживает мутаций! Нужно срочно избавиться от них!",
        );
    }
    if alert
        && new < old
        && let Some((span, text)) = instability_message(new)
    {
        tell(world, mob, span, text);
    }
}

/// `on_acquiring`: the gain message and the effect.
fn on_acquiring(world: &mut World, mob: Entity, id: usize) {
    let def = &MUTATIONS[id];
    let span = if def.quality == POSITIVE {
        Span::Notice
    } else {
        Span::Danger
    };
    tell(world, mob, span, def.gain);
    effects::on_gain(world, mob, def.effect);
    if def.effect == Effect::Glow {
        // `get_glow_color`: red, blue, yellow, green, purple, orange.
        const COLORS: [[u8; 3]; 6] = [
            [255, 0, 0],
            [0, 0, 255],
            [255, 255, 0],
            [0, 255, 0],
            [128, 0, 128],
            [255, 165, 0],
        ];
        let rng = &mut world.get_resource_or_init::<GeneticsRng>().0;
        let color = COLORS[rng.range(0, 5) as usize];
        // `glow_range * GET_MUTATION_POWER`, `glow_power`.
        let light = LightSource {
            range: 2.5,
            power: 2.0,
            color,
            height: 1.0,
            angle: 360,
            dir: Dir::South,
            offset: [0, 0],
        };
        // shortcut: the glow is the mob's own light, not a `moblight` dummy: burning
        // replaces it until the fire is out.
        world.entity_mut(mob).insert(Optics {
            light: Some(light),
            ..default()
        });
    }
}

/// `on_losing`: the lose message and the effect off.
fn on_losing(world: &mut World, mob: Entity, id: usize) {
    let def = &MUTATIONS[id];
    tell(world, mob, Span::Notice, def.lose);
    if def.effect == Effect::Glow {
        world.entity_mut(mob).remove::<Optics>();
    }
    effects::on_lose(world, mob, def.effect);
}

/// Whether `mob` has a mutation with this effect.
pub fn has_effect(world: &World, mob: Entity, effect: Effect) -> bool {
    world
        .get::<Dna>(mob)
        .is_some_and(|d| d.mutations.iter().any(|m| MUTATIONS[m.id].effect == effect))
}

/// `HAS_TRAIT(mob, TRAIT_CLUMSY)`: the clumsiness mutation or the Clumsy quirk.
// shortcut: no hunger curse or drug source of the trait yet.
pub fn is_clumsy(world: &World, mob: Entity) -> bool {
    has_effect(world, mob, Effect::Clumsy)
        || crate::quirks::has(world, mob, crate::quirks::Quirk::Clumsy)
}

/// `/datum/outfit/job/clown/post_equip`: `add_mutation(/datum/mutation/clumsy,
/// MUTATION_SOURCE_CLOWN_CLUMSINESS)`.
pub fn make_clown_clumsy(world: &mut World, mob: Entity) {
    ensure_dna(world, mob);
    if let Some(id) = mutation_id("/datum/mutation/clumsy") {
        add_mutation(world, mob, id, SOURCE_CLOWN);
    }
}

/// `SPT_PROB(percent, seconds)`.
fn spt_prob(rng: &mut Rng, percent: f32, seconds: f32) -> bool {
    let p = 100.0 * (1.0 - (1.0 - percent / 100.0).powf(seconds));
    rng.prob(p)
}

/// Every `SSmobs` fire: mutation `on_life` (cough, epilepsy) and the genetic breakdown.
// Cost: every 2 s one pass over bodies with DNA (one per player), a few comparisons each.
fn mutations_life(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + LIFE_SECS;
    let mut q = world.query::<(Entity, &Dna, &Stat)>();
    let bodies: Vec<(Entity, Vec<Effect>, Option<f64>, Stat)> = q
        .iter(world)
        .filter(|(_, d, s)| {
            (!d.mutations.is_empty() || d.melt_until.is_some()) && **s != Stat::Dead
        })
        .map(|(e, d, s)| {
            (
                e,
                d.mutations.iter().map(|m| MUTATIONS[m.id].effect).collect(),
                d.melt_until,
                *s,
            )
        })
        .collect();
    let secs = LIFE_SECS as f32;
    for (mob, effects, melt, stat) in bodies {
        let conscious = stat == Stat::Conscious || stat == Stat::SoftCrit;
        for effect in effects {
            let rng = &mut world.get_resource_or_init::<GeneticsRng>().0;
            match effect {
                // `cough/on_life`: `SPT_PROB(2.5)`, drop the held items and cough.
                Effect::Cough if conscious && spt_prob(rng, 2.5, secs) => cough(world, mob),
                // `epilepsy/on_life`: `SPT_PROB(0.5)` seizure, `Unconscious(200)`.
                Effect::Epilepsy if conscious && spt_prob(rng, 0.5, secs) => seizure(world, mob),
                Effect::Cough | Effect::Epilepsy => {}
                effect => effects::on_life(world, mob, effect, secs),
            }
        }
        if let Some(until) = melt
            && until <= now
        {
            dna_melt_end(world, mob);
        }
    }
}

/// `owner.drop_all_held_items(); owner.emote("cough")`.
pub fn cough(world: &mut World, mob: Entity) {
    let held: Vec<Entity> = world
        .get::<crate::inventory::Inventory>(mob)
        .map(|i| i.hands.iter().flatten().copied().collect())
        .unwrap_or_default();
    if let Some(&pos) = world.get::<ss13_shared::TilePos>(mob) {
        for item in held {
            crate::inventory::put_down(world, item, pos);
        }
    }
    let name = world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_default();
    crate::chem::visible(
        world,
        mob,
        "Вы кашляете!",
        &format!("{name} кашляет!"),
        Span::Emote,
    );
}

/// `epilepsy/trigger_seizure`.
// shortcut: no mood event; the 2000 s jitter that only amplifies tg's shake animation (not
// ported) is the 20 s that `jitter_less` leaves after 9 s.
pub fn seizure(world: &mut World, mob: Entity) {
    let name = world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_default();
    crate::chem::visible(
        world,
        mob,
        "У вас припадок!",
        &format!("У {name} начинается припадок!"),
        Span::Danger,
    );
    crate::status_effects::apply(world, mob, Incap::Unconscious, 20.0);
    crate::status_effects::set_timed(
        world,
        mob,
        crate::status_effects::Timed::Jitter,
        20.0,
        false,
    );
}

/// `dna_melt/on_remove` -> `something_horrible`: still at or below 0 stability, the mutations go
/// and a meltdown hits: nonfatal with `prob(70 - instability)`.
// shortcut: the nonfatal meltdowns are a random negative mutation and 30 toxin damage; the
// fatal ones (gibbing, slime, statue...) kill the body.
fn dna_melt_end(world: &mut World, mob: Entity) {
    let Some(dna) = world.get_mut::<Dna>(mob).map(|mut d| {
        d.melt_until = None;
        d.clone()
    }) else {
        return;
    };
    if dna.stability > 0 {
        return;
    }
    let instability = -dna.stability;
    remove_all_mutations(world, mob);
    let rng = &mut world.get_resource_or_init::<GeneticsRng>().0;
    let nonfatal = rng.prob((70 - instability).max(0) as f32);
    if nonfatal {
        easy_random_mutate(world, mob, NEGATIVE | MINOR_NEGATIVE);
        damage(world, mob, DamageType::Tox, 30.0);
    } else {
        tell(world, mob, Span::Danger, "Ваша ДНК распадается!");
        damage(world, mob, DamageType::Tox, 200.0);
    }
}

fn damage(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    if let Ok(mut e) = world.get_entity_mut(mob)
        && let Ok((mut health, mut stat)) = e.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        apply_damage(&mut health, &mut stat, kind, amount);
    }
}

/// `/datum/reagent/medicine/mutadone/on_mob_life`: every standard mutation goes.
pub fn mutadone(world: &mut World, mob: Entity) {
    if world
        .get::<Dna>(mob)
        .is_some_and(|d| !d.mutations.is_empty())
    {
        remove_all_mutations(world, mob);
    }
    if let Some(mut dna) = world.get_mut::<Dna>(mob) {
        for m in &mut dna.mutations {
            m.scrambled = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_round_trip() {
        let mut rng = Rng::new(3);
        for values in [4u32, 16] {
            for value in 1..=values {
                let block = construct_block(value, values, &mut rng);
                assert_eq!(block.len(), DNA_BLOCK_SIZE);
                assert_eq!(deconstruct_block(&block, values), value, "{block}");
            }
        }
    }

    #[test]
    fn identity_encodes_the_look() {
        let mut rng = Rng::new(1);
        let look = HumanLook::random_assistant(5);
        let ui = unique_identity(&look, &mut rng);
        assert_eq!(ui.len(), 3 + 3 + 12 + 6);
        let tone = deconstruct_block(&ui[3..6], SKIN_TONES.len() as u32) as usize;
        assert_eq!(SKIN_TONES[tone - 1].0, look.skin_tone);
        let gender = deconstruct_block(&ui[0..3], GENDERS);
        assert_eq!(gender, if look.female { G_FEMALE } else { G_MALE });
        let c = look.hair_color;
        assert_eq!(&ui[18..], format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]));
    }

    #[test]
    fn sequences_are_pairs_and_chipped() {
        let mut rng = Rng::new(9);
        let full = gene_sequence(4, &mut rng);
        assert_eq!(full.len(), 32);
        for pair in full.as_bytes().chunks(2) {
            assert!(matches!(pair, b"AT" | b"TA" | b"GC" | b"CG"), "{pair:?}");
        }
        let chipped = chipped_sequence(&full, 8, &mut rng);
        let missing = chipped.bytes().filter(|&b| b == b'X').count();
        assert!((1..=12).contains(&missing), "{chipped}");
        for (a, b) in full.bytes().zip(chipped.bytes()) {
            assert!(b == b'X' || a == b);
        }
        assert_eq!(cycle_gene(b'X', true), b'A');
        assert_eq!(cycle_gene(b'G', true), b'A');
        assert_eq!(cycle_gene(b'A', false), b'G');
    }

    #[test]
    fn index_takes_unlocked_mutations() {
        let mut rng = Rng::new(2);
        let db = GeneticsDb::new(&mut rng);
        let (index, defaults) = generate_dna_blocks(&db, &mut rng);
        assert_eq!(index.len(), DNA_MUTATION_BLOCKS);
        assert_eq!(defaults.len(), DNA_MUTATION_BLOCKS);
        assert!(index.iter().all(|(m, _)| !MUTATIONS[*m].locked));
        assert_eq!(db.by_alias(&db.aliases[3]), Some(3));
    }

    #[test]
    fn instability_counts_mutators_and_negatives() {
        let glow = mutation_id("/datum/mutation/glow").unwrap();
        let cough = mutation_id("/datum/mutation/cough").unwrap();
        let xray = mutation_id("/datum/mutation/xray").unwrap();
        let m = |id, sources| ActiveMutation {
            id,
            sources,
            scrambled: false,
        };
        // Activated positives are free.
        assert_eq!(stability(&[m(glow, SOURCE_ACTIVATED)]), 100);
        assert_eq!(stability(&[m(glow, SOURCE_MUTATOR)]), 95);
        // Negatives stabilise whatever their source (`instability < 0`).
        assert_eq!(stability(&[m(cough, SOURCE_ACTIVATED)]), 130);
        assert_eq!(
            stability(&[m(xray, SOURCE_MUTATOR), m(glow, SOURCE_MUTATOR)]),
            60
        );
        assert_eq!(instability_message(60).unwrap().1, "Вам холодно.");
        assert!(instability_message(95).is_none());
        assert_eq!(instability_message(-5).unwrap().0, Span::BoldWarning);
    }
}
