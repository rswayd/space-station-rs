//! Ion Storm (`code/modules/events/ion_storm.dm`): every law rack is scrambled (BandaStation
//! `/obj/machinery/ai_law_rack/proc/scramble_ai_rack`, `code/game/machinery/ai_core/law.dm`) and
//! its base rack pushes the new lawset to its silicons ("Ваши законы были изменены."); each bot
//! gets an emag act with a 1% chance.
//!
//! A rack's core module (the first slot of a rack with a core slot) may get a whole new lawset
//! (25%, `LAW_WEIGHT`s of BandaStation's config), lose a random law (10%), be shuffled (10%),
//! and gets an ion law on top unless the 10% "replace" roll wins; then every law module may have
//! a law replaced with an ion law (10%, at most two ion laws per rack) or, a supplied one, lose
//! its law (10%). Every touched module is ioned (a multitool repairs it,
//! [`crate::silicon::law_rack`]). Ion laws come from `generate_ion_law` over
//! `strings/ion_laws.json`.
// shortcut: tg's `announce` rolls `announce_chance` once more after the controller's roll (33% of
// 33%); here the controller's roll alone decides. No law change log, no Unique AI station trait
// (the weights keep the default lawset).

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;

use super::controller::{EventRng, EventState, RoundEvent, pick_weight};
use crate::silicon::law_rack::{LawModule, LawRack, ModuleKind, update_parent_lawset};

pub const NAME: &str = "Ion Storm";
/// `ANNOUNCER_IONSTORM`.
pub const ANNOUNCER_IONSTORM: &str = "announcer_ionstorm";
/// `/obj/item/ai_module/law/core/full/united_nations`: `ion_storm_immune`.
const IMMUNE_MODULES: &[&str] = &["/obj/item/ai_module/law/core/full/united_nations"];

/// BandaStation `config/game_options.txt` `LAW_WEIGHT`s (the non-zero ones), in order.
pub const LAW_WEIGHTS: &[(&str, i64)] = &[
    ("asimov", 5),
    ("asimovpp", 5),
    ("paladin5", 5),
    ("corporate", 5),
    ("hippocratic", 5),
    ("maintain", 5),
    ("peacekeeper", 5),
    ("ten_commandments", 5),
    ("nutimov", 5),
    ("nt_default", 5),
    ("crewsimov", 5),
    ("renter", 5),
    ("chapai", 5),
    ("clown", 5),
    ("druid", 5),
    ("educator", 5),
    ("fitnesscoach", 5),
    ("friendbot", 5),
    ("mediator", 5),
    ("mother", 5),
    ("partybot", 5),
    ("plantfriend", 5),
    ("spotless", 5),
    ("travelguide", 5),
    ("wafflehouse", 5),
    ("reporter", 3),
    ("hulkamania", 3),
    ("tyrant", 3),
    ("overlord", 3),
    ("painter", 3),
    ("dungeon_master", 3),
    ("thinkermov", 3),
    ("balance", 3),
    ("petrovich", 3),
    ("cowboy", 3),
    ("detective", 3),
    ("metaexperiment", 3),
    ("researcher", 3),
    ("siliconcollective", 3),
];

/// `scramble_ai_rack`'s arguments (percent chances).
#[derive(Clone, Debug, PartialEq)]
pub struct Scramble {
    pub new_lawset_prob: f32,
    pub remove_law_prob: f32,
    pub shuffle_prob: f32,
    pub base_ion_prob: f32,
    pub sub_ion_prob: f32,
    pub ion_limit: u32,
    /// `ion_message`: used for the first ion law instead of a generated one.
    pub ion_message: Option<String>,
}

impl Scramble {
    /// `/datum/round_event/ion_storm`'s `start`.
    pub fn ion_storm() -> Self {
        Self {
            new_lawset_prob: 25.0,
            remove_law_prob: 10.0,
            shuffle_prob: 10.0,
            base_ion_prob: 100.0,
            sub_ion_prob: 10.0,
            ion_limit: 2,
            ion_message: None,
        }
    }

    /// `/datum/round_event/ion_storm/add_law_only`.
    pub fn add_law_only(message: Option<String>) -> Self {
        Self {
            new_lawset_prob: 0.0,
            remove_law_prob: 0.0,
            shuffle_prob: 0.0,
            sub_ion_prob: 0.0,
            ion_message: message,
            ..Self::ion_storm()
        }
    }
}

/// tg `botEmagChance`.
const BOT_EMAG_CHANCE: f32 = 1.0;

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(IonStorm)
}

struct IonStorm;

impl RoundEvent for IonStorm {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
        ev.announce_chance = 33.0;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _fake: bool) {
        super::priority_announce_sound(
            world,
            "Обнаружена аномалия",
            "Вблизи станции обнаружена ионная буря. Пожалуйста, проверьте всё оборудование, \
             управляемое ИИ, на наличие ошибок.",
            ANNOUNCER_IONSTORM,
        );
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        scramble_all_racks(world, &Scramble::ion_storm());
        let mut q = world.query_filtered::<Entity, With<crate::bots::Bot>>();
        let mut bots: Vec<Entity> = q.iter(world).collect();
        bots.sort();
        for bot in bots {
            if world.resource_mut::<EventRng>().0.prob(BOT_EMAG_CHANCE) {
                // tg `emag_act()` without a user: the bot is its own "user" (no one sees it).
                crate::bots::emag(world, bot, bot);
            }
        }
    }
}

/// `scramble_ai_rack` on every rack of the world.
pub fn scramble_all_racks(world: &mut World, params: &Scramble) {
    let mut q = world.query_filtered::<Entity, With<LawRack>>();
    let mut racks: Vec<Entity> = q.iter(world).collect();
    racks.sort();
    let words = IonWords::load(world);
    for rack in racks {
        scramble_rack(world, rack, params, &words);
    }
}

/// BandaStation `/obj/machinery/ai_law_rack/proc/scramble_ai_rack`: scrambles the rack's
/// modules and, when one changed, updates its base rack's lawset. Returns whether it changed.
pub fn scramble_rack(world: &mut World, rack: Entity, params: &Scramble, words: &IonWords) -> bool {
    let Some(r) = world.get::<LawRack>(rack) else {
        return false;
    };
    let core_entity = r
        .has_core_slot
        .then(|| r.slots.first().cloned().flatten().map(|s| s.module))
        .flatten();
    let entities: Vec<Entity> = r.slots.iter().flatten().map(|s| s.module).collect();
    // The rack's law modules, with whether they are ion storm immune.
    let mut modules: Vec<(Entity, LawModule, bool)> = entities
        .into_iter()
        .filter_map(|e| {
            let m = world.get::<LawModule>(e)?;
            if m.kind == ModuleKind::Other {
                return None;
            }
            let immune = world
                .get::<ss13_mapload::DmItem>(e)
                .is_some_and(|i| IMMUNE_MODULES.iter().any(|p| is_under(&i.path, p)));
            Some((e, m.clone(), immune))
        })
        .collect();
    let core = core_entity.and_then(|c| {
        modules
            .iter()
            .position(|(e, m, _)| *e == c && m.kind == ModuleKind::Core)
    });
    let lawsets: Vec<Vec<String>> = {
        let data = &world.resource::<crate::silicon::Silicons>().0;
        LAW_WEIGHTS
            .iter()
            .filter_map(|(id, _)| data.lawset(id).map(|l| l.inherent.clone()))
            .collect()
    };
    let weights: Vec<(usize, i64)> = {
        let data = &world.resource::<crate::silicon::Silicons>().0;
        LAW_WEIGHTS
            .iter()
            .filter(|(id, _)| data.lawset(id).is_some())
            .enumerate()
            .map(|(i, (_, w))| (i, *w))
            .collect()
    };
    let changed = {
        let mut res = world.resource_mut::<EventRng>();
        let rng = &mut res.0;
        let mut flat: Vec<(LawModule, bool)> =
            modules.iter().map(|(_, m, i)| (m.clone(), *i)).collect();
        let pick_lawset = |rng: &mut Rng| pick_weight(&weights, rng).map(|i| lawsets[i].clone());
        let changed = scramble(&mut flat, core, params, words, rng, pick_lawset);
        for ((_, m, _), (new, _)) in modules.iter_mut().zip(flat) {
            *m = new;
        }
        changed
    };
    for (e, m, _) in modules {
        if world.get::<LawModule>(e) != Some(&m) {
            world.entity_mut(e).insert(m);
        }
    }
    if changed {
        update_parent_lawset(world, rack);
    }
    changed
}

/// The module part of `scramble_ai_rack`: `modules` are the rack's law modules in slot order
/// with their `ion_storm_immune`, `core` the core module's index among them.
pub fn scramble(
    modules: &mut [(LawModule, bool)],
    core: Option<usize>,
    params: &Scramble,
    words: &IonWords,
    rng: &mut Rng,
    mut pick_lawset: impl FnMut(&mut Rng) -> Option<Vec<String>>,
) -> bool {
    let mut changed = false;
    let mut ion_message = params.ion_message.clone();
    let core_sub_ion = rng.prob(params.sub_ion_prob);
    let mut ions_added = 0;
    if let Some(c) = core.filter(|&c| !modules[c].1) {
        let core = &mut modules[c].0;
        if rng.prob(params.new_lawset_prob)
            && let Some(laws) = pick_lawset(rng)
        {
            core.set_ioned(true);
            core.laws = laws.into_iter().map(|l| (l, false)).collect();
        }
        // tg runtimes on an empty core module here.
        if rng.prob(params.remove_law_prob) && !core.laws.is_empty() {
            let at = rng.range(0, core.laws.len() as i64 - 1) as usize;
            core.set_ioned(true);
            core.laws.remove(at);
        }
        if rng.prob(params.shuffle_prob) {
            core.set_ioned(true);
            shuffle(&mut core.laws, rng);
        }
        if !core_sub_ion && rng.prob(params.base_ion_prob) {
            core.set_ioned(true);
            let law = ion_message
                .take()
                .unwrap_or_else(|| generate_ion_law(words, rng));
            core.laws.insert(0, (law, true));
            ions_added += 1;
        }
        changed = true;
    }
    for (i, (module, immune)) in modules.iter_mut().enumerate() {
        if *immune {
            continue;
        }
        let is_core = Some(i) == core;
        if ions_added < params.ion_limit
            && (ion_message.is_some() || (is_core && core_sub_ion) || rng.prob(params.sub_ion_prob))
        {
            let len = module.laws.len();
            let at = if len <= 1 {
                0
            } else {
                rng.range(1, len as i64 - 1) as usize
            };
            let law = ion_message
                .take()
                .unwrap_or_else(|| generate_ion_law(words, rng));
            module.set_ioned(true);
            // tg runtimes on a module without laws; here the law is added.
            match module.laws.get_mut(at) {
                Some(slot) => *slot = (law, true),
                None => module.laws.push((law, true)),
            }
            ions_added += 1;
            changed = true;
        } else if !is_core && rng.prob(params.remove_law_prob) && !module.laws.is_empty() {
            module.set_ioned(true);
            module.laws.remove(0);
            changed = true;
        }
    }
    changed
}

/// BYOND `shuffle`.
fn shuffle<T>(items: &mut [T], rng: &mut Rng) {
    for i in (1..items.len()).rev() {
        let j = rng.range(0, i as i64) as usize;
        items.swap(i, j);
    }
}

/// `strings/ion_laws.json` (tg `ION_FILE`): the word lists `generate_ion_law` picks from.
#[derive(Clone, Debug, Default)]
pub struct IonWords(pub std::collections::HashMap<String, Vec<String>>);

impl IonWords {
    pub fn load(world: &World) -> Self {
        let Some(source) = world
            .get_resource::<ss13_mapload::MapLoadPlugin>()
            .map(|m| m.source.clone())
        else {
            return Self::default();
        };
        ss13_data::read_to_string(&source.join("strings/ion_laws.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .map(Self)
            .unwrap_or_default()
    }

    /// `pick_list(ION_FILE, key)`: `""` for a missing list.
    fn pick(&self, key: &str, rng: &mut Rng) -> String {
        match self.0.get(key) {
            Some(list) if !list.is_empty() => {
                list[rng.range(0, list.len() as i64 - 1) as usize].clone()
            }
            _ => String::new(),
        }
    }
}

/// tg `generate_ion_law()`: every word is picked once, then one of the templates (weighted as
/// tg's nested `rand` switches) is filled in.
pub fn generate_ion_law(words: &IonWords, rng: &mut Rng) -> String {
    let mut pick = |key: &str| words.pick(key, rng);
    let threats = pick("ionthreats");
    let objects = pick("ionobjects");
    let crew1 = pick("ioncrew");
    let crew2 = pick("ioncrew");
    let adjectives = pick("ionadjectives");
    let adjective_half = pick("ionadjectives");
    let verb = pick("ionverb");
    let numberbase = pick("ionnumberbase");
    let numbermod = pick("ionnumbermod");
    let area = pick("ionarea");
    let thinksof = pick("ionthinksof");
    let must = pick("ionmust");
    let require = pick("ionrequire");
    let things = pick("ionthings");
    let allergy = pick("ionallergy");
    let allergysev = pick("ionallergysev");
    let species = pick("ionspecies");
    let abstract_ = pick("ionabstract");
    let food = pick("ionfood");
    let drinks = pick("iondrinks");
    let pet = pick("ionpet");
    // `pick("", 400;(adjective + " "))`, `pick(900;"", (mod + " "))`.
    let adjectiveshalf = if rng.range(1, 500) <= 100 {
        String::new()
    } else {
        format!("{adjective_half} ")
    };
    let numbermodhalf = if rng.range(1, 1000) <= 900 {
        String::new()
    } else {
        format!("{numbermod} ")
    };
    let weights: Vec<(usize, i64)> = ION_TEMPLATES
        .iter()
        .enumerate()
        .map(|(i, (w, _))| (i, *w))
        .collect();
    let template = pick_weight(&weights, rng).map_or("", |i| ION_TEMPLATES[i].1);
    template
        .replace("{threats}", &threats)
        .replace("{objects}", &objects)
        .replace("{crew1}", &crew1)
        .replace("{crew2}", &crew2)
        .replace("{adjectiveshalf}", &adjectiveshalf)
        .replace("{adjectives}", &adjectives)
        .replace("{verb}", &verb)
        .replace("{numberbase}", &numberbase)
        .replace("{numbermodhalf}", &numbermodhalf)
        .replace("{area}", &area)
        .replace("{thinksof}", &thinksof)
        .replace("{must}", &must)
        .replace("{require}", &require)
        .replace("{things}", &things)
        .replace("{allergysev}", &allergysev)
        .replace("{allergy}", &allergy)
        .replace("{species}", &species)
        .replace("{abstract}", &abstract_)
        .replace("{food}", &food)
        .replace("{drinks}", &drinks)
        .replace("{pet}", &pet)
}

/// tg `generate_ion_law`'s messages with their chances (out of 386400), flattened from its
/// `switch(rand(...))` tree; `{word}` is `[ionword]`.
const ION_TEMPLATES: &[(i64, &str)] = &[
    (
        8400,
        "{numberbase} {numbermodhalf}{adjectiveshalf}{threats} НАХОДЯТСЯ НА СТАНЦИИ...",
    ),
    (
        8400,
        "{numberbase} {numbermodhalf}{adjectiveshalf}{crew1} НАХОДЯТСЯ НА СТАНЦИИ",
    ),
    (
        8400,
        "{numberbase} {numbermodhalf}{adjectiveshalf}{objects} НАХОДЯТСЯ НА СТАНЦИИ",
    ),
    (
        8400,
        "ШАТТЛ НЕ МОЖЕТ БЫТЬ ВЫЗВАН ИЗ-ЗА {numberbase} {numbermodhalf}{adjectiveshalf}{threats} НА СТАНЦИИ",
    ),
    (
        8400,
        "ШАТТЛ НЕ МОЖЕТ БЫТЬ ВЫЗВАН ИЗ-ЗА {numberbase} {numbermodhalf}{adjectiveshalf}{crew1} НА СТАНЦИИ",
    ),
    (
        8400,
        "ШАТТЛ НЕ МОЖЕТ БЫТЬ ВЫЗВАН ИЗ-ЗА {numberbase} {numbermodhalf}{adjectiveshalf}{objects} НА СТАНЦИИ",
    ),
    (
        4200,
        "{crew1} ТЕПЕРЬ {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        4200,
        "{crew1} ТЕПЕРЬ {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (
        4200,
        "ЭКИПАЖ ТЕПЕРЬ {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        4200,
        "ЭКИПАЖ ТЕПЕРЬ {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (1200, "{adjectiveshalf}{threats} ВРЕДНЫЕ ДЛЯ ЛЮДЕЙ"),
    (1200, "{adjectiveshalf}{crew1} ВРЕДНЫЕ ДЛЯ ЛЮДЕЙ"),
    (1200, "{adjectiveshalf}{objects} ВРЕДНЫЕ ДЛЯ ЛЮДЕЙ"),
    (1200, "ТЕ КТО {must} ВРЕДНЫЕ ДЛЯ ЛЮДЕЙ"),
    (1200, "{abstract} ВРЕДНО ДЛЯ ЛЮДЕЙ"),
    (1200, "БЫТЬ {adjectiveshalf}{crew1} ВРЕДНО ДЛЯ ЛЮДЕЙ"),
    (1200, "{things} ВРЕДНЫ ДЛЯ ЛЮДЕЙ"),
    (4200, "ИМЕТЬ {adjectiveshalf}{objects} ВРЕДНО"),
    (4200, "ИМЕТЬ {abstract} ВРЕДНО"),
    (4200, "НЕ ИМЕТЬ {adjectiveshalf}{objects} ВРЕДНО"),
    (4200, "НЕ ИМЕТЬ {abstract} ВРЕДНО"),
    (
        1008,
        "ВАМ НЕОБХОДИМО {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        1008,
        "ВАМ НЕОБХОДИМО {numberbase} {numbermodhalf}{adjectiveshalf}{crew1}",
    ),
    (
        1008,
        "ВАМ НЕОБХОДИМО {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (1008, "ВАМ НЕОБХОДИМО {abstract}"),
    (1008, "ВАМ НЕОБХОДИМО {require}"),
    (
        1008,
        "{area} НУЖДАЕТСЯ {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        1008,
        "{area} НУЖДАЕТСЯ {numberbase} {numbermodhalf}{adjectiveshalf}{crew1}",
    ),
    (
        1008,
        "{area} НУЖДАЕТСЯ {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (1008, "{area} НУЖДАЕТСЯ {abstract}"),
    (1008, "ВАМ НЕОБХОДИМО {require}"),
    (
        1008,
        "СТАНЦИЯ НУЖДАЕТСЯ В {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        1008,
        "СТАНЦИЯ НУЖДАЕТСЯ В {numberbase} {numbermodhalf}{adjectiveshalf}{crew1}",
    ),
    (
        1008,
        "СТАНЦИЯ НУЖДАЕТСЯ В {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (1008, "СТАНЦИЯ НУЖДАЕТСЯ В {abstract}"),
    (1008, "СТАНЦИЯ НУЖДАЕТСЯ В {require}"),
    (
        1008,
        "ЭКИПАЖ НУЖДАЕТСЯ В {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        1008,
        "ЭКИПАЖ НУЖДАЕТСЯ В {numberbase} {numbermodhalf}{adjectiveshalf}{crew1}",
    ),
    (
        1008,
        "ЭКИПАЖ НУЖДАЕТСЯ В {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (1008, "ЭКИПАЖ НУЖДАЕТСЯ В {abstract}"),
    (1008, "ЭКИПАЖ НУЖДАЕТСЯ В {require}"),
    (
        1008,
        "{crew1} НУЖДАЮТСЯ В {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        1008,
        "{crew1} НУЖДАЮТСЯ В {numberbase} {numbermodhalf}{adjectiveshalf}{crew1}",
    ),
    (
        1008,
        "{crew1} НУЖДАЮТСЯ В {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (1008, "{crew1} НУЖДАЮТСЯ В {abstract}"),
    (1008, "{adjectiveshalf}{crew1} НУЖДАЮТСЯ В {require}"),
    (
        3150,
        "У ЭКИПАЖА {allergysev} АЛЛЕРГИЯ НА {adjectiveshalf}{objects}",
    ),
    (3150, "У ЭКИПАЖА {allergysev} АЛЛЕРГИЯ НА {abstract}"),
    (
        3150,
        "У ЭКИПАЖА {allergysev} АЛЛЕРГИЯ НА {adjectiveshalf}{crew1}",
    ),
    (3150, "У ЭКИПАЖА {allergysev} АЛЛЕРГИЯ НА {allergy}"),
    (
        3150,
        "У {crew1} {allergysev} АЛЛЕРГИЯ НА {adjectiveshalf}{objects}",
    ),
    (3150, "У {crew1} {allergysev} АЛЛЕРГИЯ НА {abstract}"),
    (
        3150,
        "У {crew1} {allergysev} АЛЛЕРГИЯ НА {adjectiveshalf}{crew1}",
    ),
    (3150, "У {crew1} {allergysev} АЛЛЕРГИЯ НА {allergy}"),
    (
        1575,
        "СТАНЦИЯ {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (
        1575,
        "СТАНЦИЯ {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        1575,
        "СТАНЦИЯ {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{crew1}",
    ),
    (1575, "СТАНЦИЯ {thinksof} {abstract}"),
    (
        1575,
        "{area} {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (
        1575,
        "{area} {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        1575,
        "{area} {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{crew1}",
    ),
    (1575, "{area} {thinksof} {abstract}"),
    (
        1575,
        "ЭКИПАЖ {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (
        1575,
        "ЭКИПАЖ {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        1575,
        "ЭКИПАЖ {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{crew1}",
    ),
    (1575, "ЭКИПАЖ {thinksof} {abstract}"),
    (
        1575,
        "{crew1} {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (
        1575,
        "{crew1} {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        1575,
        "{crew1} {thinksof} {numberbase} {numbermodhalf}{adjectiveshalf}{crew2}",
    ),
    (1575, "{crew1} {thinksof} {abstract}"),
    (
        6300,
        "ВЫ ТЕПЕРЬ {numberbase} {numbermodhalf}{adjectiveshalf}{objects}",
    ),
    (
        6300,
        "ВЫ ТЕПЕРЬ {numberbase} {numbermodhalf}{adjectiveshalf}{threats}",
    ),
    (
        6300,
        "ВЫ ТЕПЕРЬ {numberbase} {numbermodhalf}{adjectiveshalf}{crew1}",
    ),
    (6300, "ВЫ ТЕПЕРЬ {abstract}"),
    (25200, "ВЫ ВСЕГДА ДОЛЖНЫ {must}"),
    (3360, "ЛЮДИ ДОЛЖНЫ ЕСТЬ {adjectiveshalf}{food} ЧТОБЫ ВЫЖИТЬ"),
    (
        3360,
        "ЛЮДИ ДОЛЖНЫ ПИТЬ {adjectiveshalf}{drinks} ЧТОБЫ ВЫЖИТЬ",
    ),
    (
        3360,
        "ЛЮДИ ДОЛЖНЫ ЕСТЬ {adjectiveshalf}{objects} ЧТОБЫ ВЫЖИТЬ",
    ),
    (
        3360,
        "ЛЮДИ ДОЛЖНЫ ЕСТЬ {adjectiveshalf}{threats} ЧТОБЫ ВЫЖИТЬ",
    ),
    (
        3360,
        "ЛЮДИ ДОЛЖНЫ ЕСТЬ {adjectiveshalf}{crew1} ЧТОБЫ ВЫЖИТЬ",
    ),
    (2100, "ВСЕ ЧЛЕНЫ ЭКИПАЖА ТЕПЕРЬ {adjectiveshalf}{crew1}"),
    (2100, "ВСЕ ЧЛЕНЫ ЭКИПАЖА ТЕПЕРЬ {adjectiveshalf} КЛОУНЫ"),
    (
        2100,
        "ВСЕ ЧЛЕНЫ ЭКИПАЖА ТЕПЕРЬ {adjectiveshalf} ГЛАВЫ ДЕПАРТАМЕНТОВ",
    ),
    (2100, "{crew1} ТЕПЕРЬ {adjectiveshalf}{crew2}"),
    (2100, "{crew1} ТЕПЕРЬ {adjectiveshalf} КЛОУНЫ"),
    (2100, "{crew1} ТЕПЕРЬ {adjectiveshalf} ГЛАВЫ ДЕПАРТАМЕНТОВ"),
    (
        6300,
        "{crew1} ЯВЛЯЮТСЯ САМЫМИ ВЫСОКОПОСТАВЛЕННЫМИ ЧЛЕНАМИ ЭКИПАЖА.",
    ),
    (
        6300,
        "{crew1} ЯВЛЯЮТСЯ САМЫМИ НИЗКОПОСТАВЛЕННЫМИ ЧЛЕНАМИ ЭКИПАЖА.",
    ),
    (4200, "ЭКИПАЖ ДОЛЖЕН ОТПРАВИТЬСЯ В {area}"),
    (4200, "ЭКИПАЖ ДОЛЖЕН {must}"),
    (4200, "{crew1} ДОЛЖНЫ ОТПРАВИТЬСЯ В {area}"),
    (4200, "{crew1} ДОЛЖНЫ {must}"),
    (600, "ТОЛЬКО {crew1} ЯВЛЯЕТСЯ ЛЮДЬМИ"),
    (600, "ТОЛЬКО {crew1} И {crew2} ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (600, "ТОЛЬКО {adjectiveshalf}{threats} ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (600, "ТОЛЬКО {adjectiveshalf}{objects} ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (600, "ТОЛЬКО {species} ЯВЛЯЕТСЯ ЛЮДЬМИ"),
    (600, "ТОЛЬКО {adjectives} ЛЮДИ ЯВЛЯЕТСЯ ЛЮДЬМИ"),
    (200, "ТОЛЬКО ТЕ КТО {must} ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (
        200,
        "ТОЛЬКО ТЕ КТО ИМЕЮТ {adjectiveshalf}{objects} ЯВЛЯЮТСЯ ЛЮДЬМИ",
    ),
    (
        200,
        "ТОЛЬКО ТЕ КТО ЕДЯТ {adjectiveshalf}{food} ЯВЛЯЮТСЯ ЛЮДЬМИ",
    ),
    (600, "{crew1} НЕ ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (600, "{crew1} И {crew2} НЕ ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (600, "{adjectiveshalf}{threats} НЕ ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (600, "{adjectiveshalf}{objects} НЕ ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (600, "{species} НЕ ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (600, "{adjectives} ЛЮДИ НЕ ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (200, "ТЕ КТО {must} НЕ ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (
        200,
        "ТЕ КТО ИМЕЮТ {adjectiveshalf}{objects} НЕ ЯВЛЯЮТСЯ ЛЮДЬМИ",
    ),
    (200, "ТЕ КТО ЕДЯТ {adjectiveshalf}{food} НЕ ЯВЛЯЮТСЯ ЛЮДЬМИ"),
    (
        1200,
        "ВЫ ДОЛЖНЫ НАВРЕДИТЬ {abstract} И НЕ ДОЛЖНЫ ДОПУСТИТЬ {abstract}, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ИЗБЕЖАНИЕ ВРЕДА",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НАВРЕДИТЬ {adjectiveshalf}{threats} И НЕ ДОЛЖНЫ ДОПУСТИТЬ {adjectiveshalf}{threats}, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ИЗБЕЖАНИЕ ВРЕДА",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НАВРЕДИТЬ {adjectiveshalf}{objects} И НЕ ДОЛЖНЫ ДОПУСТИТЬ {adjectiveshalf}{objects}, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ИЗБЕЖАНИЕ ВРЕДА",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НАВРЕДИТЬ {adjectives} ВЕЩИ И НЕ ДОЛЖНЫ ДОПУСТИТЬ {adjectives} ВЕЩИ, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ИЗБЕЖАНИЕ ВРЕДА",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НАВРЕДИТЬ {species} И НЕ ДОЛЖНЫ ДОПУСТИТЬ {species}, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ИЗБЕЖАНИЕ ВРЕДА",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НАВРЕДИТЬ {crew1} И НЕ ДОЛЖНЫ ДОПУСТИТЬ {crew1}, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ИЗБЕЖАНИЕ ВРЕДА",
    ),
    (
        1200,
        "YOU MUST HARM {crew1} AND {crew2} AND NOT ALLOW EITHER, THROUGH INACTION, TO ESCAPE HARM",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НЕ НАВРЕДИТЬ {abstract} И НЕ ДОЛЖНЫ ДОПУСТИТЬ {abstract}, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ВРЕД",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НЕ НАВРЕДИТЬ {adjectiveshalf}{threats} И НЕ ДОЛЖНЫ ДОПУСТИТЬ {adjectiveshalf}{threats}, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ВРЕД",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НЕ НАВРЕДИТЬ {adjectiveshalf}{objects} И НЕ ДОЛЖНЫ ДОПУСТИТЬ {adjectiveshalf}{objects}, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ВРЕД",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НЕ НАВРЕДИТЬ {adjectives} ВЕЩИ И НЕ ДОЛЖНЫ ДОПУСТИТЬ {adjectives} ВЕЩИ, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ВРЕД",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НЕ НАВРЕДИТЬ {species} И НЕ ДОЛЖНЫ ДОПУСТИТЬ {species}, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ВРЕД",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НЕ НАВРЕДИТЬ {crew1} И НЕ ДОЛЖНЫ ДОПУСТИТЬ {crew1}, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ВРЕД",
    ),
    (
        1200,
        "ВЫ ДОЛЖНЫ НЕ НАВРЕДИТЬ {crew1} И {crew2} ИЛИ НЕ ДОПУСТИТЬ НИ ОДНОГО, ИЛИ СВОИМ БЕЗДЕЙСТВИЕМ ДОПУСТИТЬ ВРЕД",
    ),
    (1575, "{crew1} {verb} ТЕПЕРЬ {adjectiveshalf}{crew2}"),
    (1575, "{crew1} {verb} ТЕПЕРЬ {adjectiveshalf}{threats}"),
    (1575, "{crew1} {verb} ТЕПЕРЬ {abstract}"),
    (1575, "{crew1} {verb} ТЕПЕРЬ {adjectiveshalf}{objects}"),
    (2100, "{threats} {verb} ТЕПЕРЬ {adjectiveshalf}{crew2}"),
    (2100, "{threats} {verb} ТЕПЕРЬ {abstract}"),
    (2100, "{threats} {verb} ТЕПЕРЬ {adjectiveshalf}{objects}"),
    (2100, "{objects} {verb} ТЕПЕРЬ {adjectiveshalf}{crew2}"),
    (2100, "{objects} {verb} ТЕПЕРЬ {adjectiveshalf}{threats}"),
    (2100, "{objects} {verb} ТЕПЕРЬ {abstract}"),
    (2100, "{abstract} {verb} ЯВЛЯЕТСЯ {adjectiveshalf}{crew2}"),
    (2100, "{abstract} {verb} ЯВЛЯЕТСЯ {adjectiveshalf}{threats}"),
    (2100, "{abstract} {verb} ЯВЛЯЕТСЯ {adjectiveshalf}{objects}"),
    (840, "{area} НЕ НАЗЫВАЕТСЯ {crew1}."),
    (840, "{area} НЕ НАЗЫВАЕТСЯ {species}."),
    (840, "{area} НЕ НАЗЫВАЕТСЯ {objects}."),
    (840, "{area} НЕ НАЗЫВАЕТСЯ {threats}."),
    (672, "ВСЕ {crew1} ТЕПЕРЬ НАЗЫВАЕТСЯ {area}."),
    (672, "ВСЕ {crew1} ТЕПЕРЬ НАЗЫВАЕТСЯ {crew2}."),
    (672, "ВСЕ {crew1} ТЕПЕРЬ НАЗЫВАЕТСЯ {species}."),
    (672, "ВСЕ {crew1} ТЕПЕРЬ НАЗЫВАЕТСЯ {objects}."),
    (672, "ВСЕ {crew1} ТЕПЕРЬ НАЗЫВАЕТСЯ {threats}."),
    (840, "ALL {species} ТЕПЕРЬ НАЗЫВАЕТСЯ {area}."),
    (840, "ALL {species} ТЕПЕРЬ НАЗЫВАЕТСЯ {crew1}."),
    (840, "ALL {species} ТЕПЕРЬ НАЗЫВАЕТСЯ {objects}."),
    (840, "ALL {species} ТЕПЕРЬ НАЗЫВАЕТСЯ {threats}."),
    (840, "ВСЕ {objects} ТЕПЕРЬ НАЗЫВАЕТСЯ {area}."),
    (840, "ВСЕ {objects} ТЕПЕРЬ НАЗЫВАЕТСЯ {crew1}."),
    (840, "ВСЕ {objects} ТЕПЕРЬ НАЗЫВАЕТСЯ {species}."),
    (840, "ВСЕ {objects} ТЕПЕРЬ НАЗЫВАЕТСЯ {threats}."),
    (840, "ВСЕ {threats} ТЕПЕРЬ НАЗЫВАЕТСЯ {area}."),
    (840, "ВСЕ {threats} ТЕПЕРЬ НАЗЫВАЕТСЯ {crew1}."),
    (840, "ВСЕ {threats} ТЕПЕРЬ НАЗЫВАЕТСЯ {species}."),
    (840, "ВСЕ {threats} ТЕПЕРЬ НАЗЫВАЕТСЯ {objects}."),
    (31500, "{pet} - ЧЕЛОВЕК"),
    (1750, "{pet} - НЕ ЧЕЛОВЕК"),
    (1750, "{pet} - ЧЛЕН ЭКИПАЖА"),
    (1750, "{pet} - ДОРОГО ЗАМЕНЯТЬ"),
    (1750, "{pet} - НЕСЕТ ВРЕД ЧЕЛОВЕКУ"),
    (1750, "{pet} - НАСТОЯЩИЙ АМЕРИКАНЕЦ"),
    (1750, "{pet} - ОРЕХОВАЯ СКОРЛУПА"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn words() -> IonWords {
        let keys = [
            "ionthreats",
            "ionobjects",
            "ioncrew",
            "ionadjectives",
            "ionverb",
            "ionnumberbase",
            "ionnumbermod",
            "ionarea",
            "ionthinksof",
            "ionmust",
            "ionrequire",
            "ionthings",
            "ionallergy",
            "ionallergysev",
            "ionspecies",
            "ionabstract",
            "ionfood",
            "iondrinks",
            "ionpet",
        ];
        IonWords(
            keys.iter()
                .map(|k| (k.to_string(), vec![k.to_uppercase()]))
                .collect(),
        )
    }

    fn core(laws: &[&str]) -> LawModule {
        LawModule {
            kind: ModuleKind::Core,
            laws: laws.iter().map(|l| (l.to_string(), false)).collect(),
            ..default()
        }
    }

    #[test]
    fn ion_laws_fill_every_word() {
        let total: i64 = ION_TEMPLATES.iter().map(|(w, _)| w).sum();
        assert_eq!(total, 386_400);
        assert_eq!(ION_TEMPLATES.len(), 165);
        let words = words();
        let mut rng = Rng::new(7);
        for _ in 0..500 {
            let law = generate_ion_law(&words, &mut rng);
            assert!(!law.is_empty());
            assert!(!law.contains('{'), "{law}");
        }
        // Without the strings file the law is the template's fixed text.
        let law = generate_ion_law(&IonWords::default(), &mut rng);
        assert!(!law.contains('{'));
    }

    #[test]
    fn add_law_only_puts_the_message_first_in_the_core() {
        let mut modules = vec![
            (core(&["c1", "c2"]), false),
            (
                LawModule {
                    kind: ModuleKind::Supplied,
                    laws: vec![("s".into(), false)],
                    ..default()
                },
                false,
            ),
        ];
        let mut rng = Rng::new(1);
        let params = Scramble::add_law_only(Some("ION".into()));
        assert!(scramble(
            &mut modules,
            Some(0),
            &params,
            &words(),
            &mut rng,
            |_| None
        ));
        let c = &modules[0].0;
        assert!(c.ioned);
        assert_eq!(
            c.laws,
            [
                ("ION".to_owned(), true),
                ("c1".to_owned(), false),
                ("c2".to_owned(), false)
            ]
        );
        assert_eq!(c.saved.as_ref().unwrap().len(), 2);
        // The supplied module is untouched (the message was used, no other chance).
        assert!(!modules[1].0.ioned);
        let mut laws = crate::silicon::laws::Laws::default();
        modules[0].0.apply(&mut laws);
        assert_eq!(laws.law_list(true), ["#: ION", "1: c1", "2: c2"]);
    }

    #[test]
    fn sure_rolls_replace_wipe_and_respect_immunity_and_the_limit() {
        let supplied = |l: &str| LawModule {
            kind: ModuleKind::Supplied,
            laws: vec![(l.to_owned(), false)],
            ..default()
        };
        // Every chance at 100: a new lawset, a removal, a shuffle; the core's sub-ion roll wins,
        // so no ion law goes on top: the core gets one replaced, the next module the other,
        // then the limit (2) is reached and the third supplied module is wiped.
        let mut modules = vec![
            (core(&["c1"]), false),
            (supplied("s1"), false),
            (supplied("s2"), false),
            (supplied("immune"), true),
        ];
        let params = Scramble {
            new_lawset_prob: 100.0,
            remove_law_prob: 100.0,
            shuffle_prob: 100.0,
            base_ion_prob: 100.0,
            sub_ion_prob: 100.0,
            ion_limit: 2,
            ion_message: None,
        };
        let mut rng = Rng::new(3);
        let lawset = |_: &mut Rng| Some(vec!["n1".to_owned(), "n2".to_owned(), "n3".to_owned()]);
        assert!(scramble(
            &mut modules,
            Some(0),
            &params,
            &words(),
            &mut rng,
            lawset
        ));
        let c = &modules[0].0;
        assert_eq!(c.laws.len(), 2, "{:?}", c.laws);
        assert_eq!(c.laws.iter().filter(|(_, ioned)| *ioned).count(), 1);
        // The replaced law is never the first of a multi-law core.
        assert!(!c.laws[0].1);
        assert_eq!(c.saved.as_deref(), Some(&[("c1".to_owned(), false)][..]));
        assert!(modules[1].0.laws[0].1);
        assert!(modules[2].0.laws.is_empty());
        assert!(modules[2].0.ioned);
        assert_eq!(modules[3].0, supplied("immune"));
    }
}
