//! Cytology (server), after tg `code/modules/research/xenobiology/vatgrowing/`: swabs and
//! biopsy tools (`/datum/component/swabbing`), swabable atoms (`/datum/element/swabable`),
//! biological samples (`/datum/biological_sample`, `/datum/micro_organism`), petri dishes, the
//! microscope and the growing vat. The cell lines, their tables and the swabable types are
//! generated ([`super::cell_line_data`], `scripts/gen-cell-lines.py`).
//!
//! - A swab (objects) or biopsy tool (mobs; the tissue biopsy tool both) used on a swabable
//!   atom takes a sample after 3 s (`do_after`): `amount` cell lines of the atom's table
//!   (weighted, no repeats) and a virus at `virus_chance`; each atom gives one sample
//!   (`Detach` after swabbing). A tool holds one sample, which it deposits into an empty petri
//!   dish. A basic sample petri dish starts with a random corgi, snake, cockroach or
//!   blobbernaut sample.
//! - The microscope takes a petri dish (a right click takes it back) and shows what lives in
//!   it: each cell line's growth rate, consumption, virus susceptibility and the reagents it
//!   needs, likes and hates.
//! - The growing vat (300 u, open container) takes a copy of a dish's sample. Every
//!   `SSmachines` fire each cell line eats `consumption_rate` of each required reagent (none
//!   missing) and grows `growth_rate`, plus its liked and minus its hated reagents present
//!   (each eaten), minus its susceptibility per virus unless spaceacillin is eaten instead. At
//!   100 growth it pops `result_count` of its result out; the sample is then gone, or starts
//!   over with the resampler on (a click toggles it). A right click flushes the vat.
// shortcut: no special results (`succeed_growing` of the vat beast, netherworld, clowns and
// queen bee: their plain result spawns), no shiny colour mutations (`CYTO_SHINY_CHANCE`), no
// small vat explosion, no plumbing, no flush confirmation, no swabbing turfs (no turf adds the
// element), no swab overlays or sample colours on sprites, the microscope's tgui `Microscope`
// window is told in chat, no viruses into virology.

use bevy::prelude::*;
use ss13_reagents::{Holder, Owner, flags, is_under};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::cell_line_data::{CELL_LINE_TABLES, CELL_LINES, SWABABLES};
use crate::chem::Reagents;
use crate::combat::CombatRng;
use crate::examine::Line;
use crate::tools::{Actor, DoAfter, do_after, tell};

pub const SWAB: &str = "/obj/item/swab";
pub const BIOPSY: &str = "/obj/item/biopsy_tool";
pub const BIOPSY_ORGAN: &str = "/obj/item/biopsy_tool/organ";
pub const PETRI_DISH: &str = "/obj/item/petri_dish";
const PETRI_RANDOM: &str = "/obj/item/petri_dish/random";
pub const MICROSCOPE: &str = "/obj/structure/microscope";
pub const VAT: &str = "/obj/machinery/vatgrower";
const VAT_SMALL: &str = "/obj/machinery/vatgrower/small";
const SPACEACILLIN: &str = "/datum/reagent/medicine/spaceacillin";
/// The swabbing `do_after`, deciseconds.
const SWAB_DS: f32 = 30.0;
/// `SSmachines` `wait`.
pub const MACHINES_SECS: f64 = 2.0;
/// `GLOB.xeno_sample_colors`.
const SAMPLE_COLORS: [&str; 5] = ["#91542d", "#5b5b5b", "#97b03a", "#5d3d8d", "#c9b23d"];
/// `/obj/item/petri_dish/random::possible_samples`: table, amount, virus chance.
const RANDOM_SAMPLES: [(&str, u32, u32); 4] = [
    ("CELL_LINE_TABLE_CORGI", 1, 5),
    ("CELL_LINE_TABLE_SNAKE", 1, 5),
    ("CELL_LINE_TABLE_COCKROACH", 1, 7),
    ("CELL_LINE_TABLE_BLOBBERNAUT", 1, 5),
];

/// One `/datum/micro_organism/cell_line` type.
pub struct CellLine {
    pub path: &'static str,
    pub desc: &'static str,
    pub required: &'static [&'static str],
    pub supplementary: &'static [(&'static str, f32)],
    pub suppressive: &'static [(&'static str, f32)],
    pub virus_susceptibility: f32,
    pub growth_rate: f32,
    pub consumption_rate: f32,
    pub result: &'static str,
    pub result_count: u32,
}

/// A type that adds `/datum/element/swabable`: its table, `cell_line_amount` (a `rand` range)
/// and `virus_chance`.
pub struct Swabable {
    pub path: &'static str,
    pub table: &'static str,
    pub amount: (u32, u32),
    pub virus_chance: u32,
}

/// A `/datum/micro_organism` in a sample.
#[derive(Clone, Debug, PartialEq)]
pub enum Organism {
    /// A cell line (index into [`CELL_LINES`]) and its growth so far.
    Cell { line: usize, growth: f32 },
    /// `/datum/micro_organism/virus`.
    Virus,
}

/// `/datum/biological_sample`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    pub organisms: Vec<Organism>,
    pub color: &'static str,
}

/// A swab or biopsy tool's `swabbed_items` (`max_items = 1`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Swabber(pub Option<Sample>);

/// A petri dish's `sample`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PetriDish(pub Option<Sample>);

/// An atom that was swabbed: its `swabable` element is detached.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Swabbed;

/// The microscope's `current_dish` (its sample, kept while the dish is in).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Microscope(pub Option<Option<Sample>>);

/// A growing vat's `biological_sample` and `resampler_active`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct VatGrower {
    pub sample: Option<Sample>,
    pub resampler: bool,
}

#[derive(Resource, Default)]
struct VatClock(f64);

pub struct CellLinesPlugin;

impl Plugin for CellLinesPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::HeldOnTarget, 603, |w, c| {
            c.held
                .is_some_and(|h| swab_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 1783, |w, c| {
            c.held
                .is_some_and(|h| dish_interaction(w, c.client, h, c.target))
        });
        app.add_click_handler(ClickPhase::EmptyHand, 2673, |w, c| {
            attack_hand(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        app.init_resource::<VatClock>()
            .add_observer(on_item_added)
            .add_observer(on_obj_added)
            .add_systems(Update, process_vats);
    }
}

fn on_item_added(
    add: On<Add<ss13_mapload::DmItem>>,
    items: Query<&ss13_mapload::DmItem>,
    mut commands: Commands,
) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let e = add.entity;
    if is_under(&item.path, SWAB) || is_under(&item.path, BIOPSY) {
        commands.entity(e).try_insert(Swabber::default());
    } else if is_under(&item.path, PETRI_RANDOM) {
        commands.queue(move |world: &mut World| {
            let mut rng = CombatRng(world.resource::<CombatRng>().0);
            let (table, amount, chance) = RANDOM_SAMPLES[rng.range(0, 3) as usize];
            let sample = generate_sample(&mut rng, table, amount, chance);
            world.resource_mut::<CombatRng>().0 = rng.0;
            if let Ok(mut ent) = world.get_entity_mut(e) {
                ent.insert(PetriDish(Some(sample)));
            }
        });
    } else if is_under(&item.path, PETRI_DISH) {
        commands.entity(e).try_insert(PetriDish::default());
    }
}

fn on_obj_added(
    add: On<Add<ss13_mapload::DmObj>>,
    objs: Query<&ss13_mapload::DmObj>,
    mut commands: Commands,
) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    if is_under(&obj.path, MICROSCOPE) {
        commands
            .entity(add.entity)
            .try_insert(Microscope::default());
    } else if is_under(&obj.path, VAT) {
        // `reagent_volume`, `OPENCONTAINER | DUNKABLE`.
        let volume = if is_under(&obj.path, VAT_SMALL) {
            50.0
        } else {
            300.0
        };
        let owner = Owner {
            path: obj.path.clone(),
            living: false,
        };
        commands.entity(add.entity).try_insert((
            VatGrower::default(),
            Reagents(Holder::new(
                volume,
                flags::OPENCONTAINER | flags::DUNKABLE,
                owner,
            )),
        ));
    }
}

/// `GLOB.cell_line_tables[key]`.
fn table(key: &str) -> &'static [(&'static str, u32)] {
    CELL_LINE_TABLES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, t)| *t)
        .unwrap_or(&[])
}

fn line_index(path: &str) -> Option<usize> {
    CELL_LINES.iter().position(|l| l.path == path)
}

/// `GenerateSample(cell_line_define, virus_define, cell_line_amount, virus_chance)`.
pub fn generate_sample(rng: &mut CombatRng, table_key: &str, amount: u32, chance: u32) -> Sample {
    let color = SAMPLE_COLORS[rng.range(0, SAMPLE_COLORS.len() as u32 - 1) as usize];
    let mut weights: Vec<(&str, u32)> = table(table_key).to_vec();
    let mut organisms = Vec::new();
    for _ in 0..amount {
        let total: u32 = weights.iter().map(|w| w.1).sum();
        if total == 0 {
            break;
        }
        let mut roll = rng.range(1, total);
        let Some(i) = weights.iter().position(|&(_, w)| {
            if roll <= w {
                true
            } else {
                roll -= w;
                false
            }
        }) else {
            break;
        };
        let (path, _) = weights.remove(i);
        if let Some(line) = line_index(path) {
            organisms.push(Organism::Cell { line, growth: 0.0 });
        }
    }
    if rng.chance(chance as f32) {
        organisms.push(Organism::Virus);
    }
    Sample { organisms, color }
}

/// The swabable element of `path`: the most specific type that adds it. Robotic organs are
/// not organic (`ORGAN_ORGANIC`).
pub fn swabable(path: &str) -> Option<&'static Swabable> {
    if is_under(path, "/obj/item/organ") && path.contains("cybernetic") {
        return None;
    }
    SWABABLES
        .iter()
        .filter(|s| is_under(path, s.path))
        .max_by_key(|s| s.path.len())
}

/// The DM type of a mob, object, item or turf.
fn atom_path(world: &World, e: Entity) -> Option<String> {
    world
        .get::<ss13_mapload::DmMob>(e)
        .map(|m| m.path.clone())
        .or_else(|| crate::construction::path_of(world, e))
}

/// What a tool may swab: (objects, mobs) — `can_swab_objs`, `can_swab_mobs`.
fn swab_kinds(tool: &str) -> (bool, bool) {
    if is_under(tool, BIOPSY_ORGAN) {
        (true, true)
    } else if is_under(tool, BIOPSY) {
        (false, true)
    } else {
        (true, false)
    }
}

/// `try_to_swab`: a dish gets the tool's sample; a swabable kind of atom starts the swab.
fn swab_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(sample) = world.get::<Swabber>(held).map(|s| s.0.clone()) else {
        return false;
    };
    if let Some(dish) = world.get::<PetriDish>(target) {
        let Some(sample) = sample else {
            return false;
        };
        if dish.0.is_some() {
            return true;
        }
        world.entity_mut(target).insert(PetriDish(Some(sample)));
        world.entity_mut(held).insert(Swabber(None));
        let name = crate::chem::name_of(world, target);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы помещаете образец в {name}."),
        );
        return true;
    }
    let tool = crate::construction::path_of(world, held).unwrap_or_default();
    let (objs, mobs) = swab_kinds(&tool);
    let is_mob = world.get::<crate::health::Health>(target).is_some();
    let is_obj = !is_mob && world.get::<ss13_mapload::Turf>(target).is_none();
    // Tables are for putting things down (`istype(target, /obj/structure/table)`).
    let path = atom_path(world, target).unwrap_or_default();
    if is_under(&path, "/obj/structure/table") || !((objs && is_obj) || (mobs && is_mob)) {
        return false;
    }
    if sample.is_some() {
        let name = crate::chem::name_of(world, held);
        tell(
            world,
            client,
            Span::Warning,
            format!("На {name} уже нельзя собрать ещё один образец!"),
        );
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if do_after(
        world,
        actor,
        target,
        SWAB_DS,
        0,
        crate::construction::Job::Swab,
    ) {
        let name = crate::chem::name_of(world, target);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы начинаете брать образцы с {name}!"),
        );
    }
    true
}

/// `async_try_to_swab` after its `do_after`.
pub fn complete_swab(world: &mut World, action: &DoAfter) {
    let (Some(tool), target) = (action.item, action.target) else {
        return;
    };
    if world.get::<Swabber>(tool).is_none_or(|s| s.0.is_some()) {
        return;
    }
    let name = crate::chem::name_of(world, target);
    let path = atom_path(world, target).unwrap_or_default();
    let found = swabable(&path).filter(|_| world.get::<Swabbed>(target).is_none());
    let Some(found) = found else {
        tell(
            world,
            action.client,
            Span::Warning,
            format!("Вам не удаётся ничего найти на {name}!"),
        );
        return;
    };
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let amount = rng.range(found.amount.0, found.amount.1.max(found.amount.0));
    let sample = generate_sample(&mut rng, found.table, amount, found.virus_chance);
    world.resource_mut::<CombatRng>().0 = rng.0;
    world.entity_mut(target).insert(Swabbed);
    world.entity_mut(tool).insert(Swabber(Some(sample)));
    tell(
        world,
        action.client,
        Span::Notice,
        format!("Вам удаётся собрать микробиологический образец с {name}!"),
    );
}

/// A petri dish into the microscope (`add_dish`) or the growing vat (`deposit_sample`).
fn dish_interaction(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    let Some(dish) = world.get::<PetriDish>(held).map(|d| d.0.clone()) else {
        return false;
    };
    if let Some(scope) = world.get::<Microscope>(target) {
        if scope.0.is_some() {
            tell(world, client, Span::Warning, "В микроскопе уже есть чашка.");
            return true;
        }
        world.entity_mut(target).insert(Microscope(Some(dish)));
        world.despawn(held);
        tell(world, client, Span::Notice, "Чашка Петри вставлена.");
        return true;
    }
    let Some(vat) = world.get::<VatGrower>(target) else {
        return false;
    };
    let Some(sample) = dish else {
        tell(world, client, Span::Warning, "Чашка пуста.");
        return true;
    };
    if vat.sample.is_some() {
        tell(world, client, Span::Warning, "В чане уже есть образец.");
        return true;
    }
    // A fresh copy: `new m.type()`.
    let organisms = sample
        .organisms
        .iter()
        .map(|o| match o {
            Organism::Cell { line, .. } => Organism::Cell {
                line: *line,
                growth: 0.0,
            },
            Organism::Virus => Organism::Virus,
        })
        .collect();
    if let Some(mut vat) = world.get_mut::<VatGrower>(target) {
        vat.sample = Some(Sample {
            organisms,
            color: sample.color,
        });
    }
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/bubbles/bubbles.ogg", 50.0, true),
        );
    }
    tell(world, client, Span::Notice, "Образец добавлен.");
    true
}

/// The microscope's window and its dish's removal; the vat's resampler and flush.
fn attack_hand(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if let Some(scope) = world.get::<Microscope>(target).cloned() {
        match (right, scope.0) {
            (true, Some(sample)) => {
                world.entity_mut(target).insert(Microscope(None));
                if let Some(&pos) = world.get::<TilePos>(mob)
                    && let Some(dish) = crate::construction::spawn_obj(world, PETRI_DISH, pos)
                {
                    world.flush();
                    world.entity_mut(dish).insert(PetriDish(sample));
                }
            }
            (true, None) => return false,
            (false, dish) => {
                let text = microscope_text(world, dish.as_ref());
                tell(world, client, Span::Notice, text);
            }
        }
        return true;
    }
    if world.get::<VatGrower>(target).is_none() {
        return false;
    }
    if right {
        // `attack_hand_secondary`: only an anchored vat flushes.
        let anchored = world
            .get::<ss13_mapload::DmObj>(target)
            .is_some_and(|o| o.anchored);
        if anchored {
            if let Some(mut r) = world.get_mut::<Reagents>(target) {
                r.0.clear();
            }
            if let Some(mut vat) = world.get_mut::<VatGrower>(target) {
                vat.sample = None;
            }
            tell(world, client, Span::Notice, "Чан опустошён.");
        }
        return true;
    }
    let on = world.get_mut::<VatGrower>(target).is_some_and(|mut v| {
        v.resampler = !v.resampler;
        v.resampler
    });
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/machines/click.ogg", 30.0, true),
        );
    }
    let text = if on {
        "Ресэмплер включён."
    } else {
        "Ресэмплер выключен."
    };
    tell(world, client, Span::Notice, text);
    true
}

fn reagent_names(world: &World, paths: impl Iterator<Item = &'static str>) -> String {
    let chem = world.get_resource::<crate::chem::Chem>();
    paths
        .map(|p| {
            chem.and_then(|c| c.0.id(p).map(|id| c.0.def(id).name.clone()))
                .unwrap_or_else(|| p.rsplit('/').next().unwrap_or(p).to_owned())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The microscope's `ui_static_data`, as text.
pub fn microscope_text(world: &World, dish: Option<&Option<Sample>>) -> String {
    let Some(dish) = dish else {
        return "В микроскопе нет чашки Петри.".to_owned();
    };
    let Some(sample) = dish else {
        return "В чашке Петри нет образца.".to_owned();
    };
    let mut text = String::from("Под микроскопом:");
    for o in &sample.organisms {
        match o {
            Organism::Cell { line, .. } => {
                let l = &CELL_LINES[*line];
                let dt = MACHINES_SECS as f32;
                text += &format!(
                    "\n• Клеточная линия: {}\n  Рост: {}% за цикл, потребление: {} ед., \
                     восприимчивость к вирусам: {}",
                    l.desc,
                    l.growth_rate * dt,
                    l.consumption_rate * dt,
                    l.virus_susceptibility * dt
                );
                let lists = [
                    ("Требует", reagent_names(world, l.required.iter().copied())),
                    (
                        "Любит",
                        reagent_names(world, l.supplementary.iter().map(|r| r.0)),
                    ),
                    (
                        "Не выносит",
                        reagent_names(world, l.suppressive.iter().map(|r| r.0)),
                    ),
                ];
                for (label, names) in lists {
                    if !names.is_empty() {
                        text += &format!("\n  {label}: {names}");
                    }
                }
            }
            Organism::Virus => text += "\n• Вирус: A virus of unknown origin",
        }
    }
    text
}

/// `get_details()` of every organism.
fn organism_lines(sample: &Sample, lines: &mut Vec<Line>) {
    for o in &sample.organisms {
        let text = match o {
            Organism::Cell { line, growth } => format!(
                "{} - прогресс роста: {}%",
                CELL_LINES[*line].desc,
                growth.round()
            ),
            Organism::Virus => "A virus of unknown origin".to_owned(),
        };
        lines.push((Span::Notice, text));
    }
}

/// Swabs, dishes and vats with a sample (their `examine`).
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    let sample = world
        .get::<Swabber>(target)
        .and_then(|s| s.0.as_ref())
        .or_else(|| world.get::<PetriDish>(target).and_then(|d| d.0.as_ref()));
    if let Some(sample) = sample {
        lines.push((
            Span::Notice,
            "Вы видите следующие микроорганизмы:".to_owned(),
        ));
        organism_lines(sample, lines);
    }
    if let Some(sample) = world
        .get::<VatGrower>(target)
        .and_then(|v| v.sample.as_ref())
    {
        lines.push((Span::Notice, "Похоже, в нём есть образец!".to_owned()));
        organism_lines(sample, lines);
    }
}

/// `SSmachines` -> every vat's `process`.
// Cost: every 2 s, one pass over the vats (a handful per station).
fn process_vats(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs_f64();
    let fires = {
        let mut clock = world.resource_mut::<VatClock>();
        clock.0 += delta;
        let n = (clock.0 / MACHINES_SECS).floor();
        clock.0 -= n * MACHINES_SECS;
        n as u32
    };
    for _ in 0..fires {
        let vats: Vec<Entity> = world
            .query_filtered::<Entity, With<VatGrower>>()
            .iter(world)
            .collect();
        for vat in vats {
            grow(world, vat);
        }
    }
}

/// `biological_sample.handle_growth(vat)`: whether any cell line could eat.
pub fn grow(world: &mut World, vat: Entity) -> bool {
    let Some(mut sample) = world.get::<VatGrower>(vat).and_then(|v| v.sample.clone()) else {
        return false;
    };
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return false;
    };
    let Some(mut holder) = world.get::<Reagents>(vat).map(|r| r.0.clone()) else {
        return false;
    };
    let viruses = sample
        .organisms
        .iter()
        .filter(|o| matches!(o, Organism::Virus))
        .count();
    let has = |h: &Holder, path: &str, amount: f32| {
        chem.id(path)
            .is_some_and(|id| h.amount(id) > 0.0 && h.amount(id) >= amount)
    };
    let take = |h: &mut Holder, path: &str, amount: f32| {
        if let Some(id) = chem.id(path) {
            h.remove_reagent(id, amount);
        }
    };
    let mut ate = false;
    let mut finished = None;
    for o in &mut sample.organisms {
        let Organism::Cell { line, growth } = o else {
            continue;
        };
        let l = &CELL_LINES[*line];
        // `try_eat`.
        if !l.required.iter().all(|r| has(&holder, r, 0.0)) {
            continue;
        }
        for r in l.required {
            take(&mut holder, r, l.consumption_rate);
        }
        ate = true;
        // `calculate_growth`.
        let mut rate = l.growth_rate;
        for &(r, bonus) in l.supplementary.iter().chain(l.suppressive) {
            if has(&holder, r, l.consumption_rate) {
                rate += bonus;
                take(&mut holder, r, l.consumption_rate);
            }
        }
        for _ in 0..viruses {
            if has(&holder, SPACEACILLIN, l.consumption_rate) {
                take(&mut holder, SPACEACILLIN, l.consumption_rate);
            } else {
                rate -= l.virus_susceptibility;
            }
        }
        *growth = growth.max(*growth + rate);
        if *growth >= 100.0 {
            finished = Some(*line);
            break;
        }
    }
    if let Some(mut r) = world.get_mut::<Reagents>(vat) {
        r.0 = holder;
    }
    let resampler = world.get::<VatGrower>(vat).is_some_and(|v| v.resampler);
    let kept = match finished {
        // `on_sample_growth_completed`: `reset_sample` with the resampler, else gone.
        Some(_) if resampler => {
            for o in &mut sample.organisms {
                if let Organism::Cell { growth, .. } = o {
                    *growth = 0.0;
                }
            }
            Some(sample)
        }
        Some(_) => None,
        None => Some(sample),
    };
    if let Some(mut v) = world.get_mut::<VatGrower>(vat) {
        v.sample = kept;
    }
    if let Some(line) = finished {
        finish_growing(world, vat, line);
    }
    if ate
        && finished.is_none()
        && let Some(&pos) = world.get::<TilePos>(vat)
        && world.resource_mut::<CombatRng>().chance(10.0)
    {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/slosh.ogg", 25.0, true),
        );
    }
    ate
}

/// `finish_growing` -> `succeed_growing`: the results pop out of the vat.
fn finish_growing(world: &mut World, vat: Entity, line: usize) {
    let Some(&pos) = world.get::<TilePos>(vat) else {
        return;
    };
    let l = &CELL_LINES[line];
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/splat.ogg", 50.0, true),
    );
    let vat_name = crate::chem::name_of(world, vat);
    for _ in 0..l.result_count {
        let Some(thing) = crate::construction::spawn_obj(world, l.result, pos) else {
            warn!("vat result {} has no prototype", l.result);
            return;
        };
        world.flush();
        let name = crate::chem::name_of(world, thing);
        super::xenobio::say_around(
            world,
            pos,
            Span::Notice,
            &format!("{name} выскакивает из {vat_name}!"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tables_are_consistent() {
        assert!(CELL_LINES.len() > 40);
        for (key, entries) in CELL_LINE_TABLES {
            for (path, _) in *entries {
                assert!(line_index(path).is_some(), "{key}: {path}");
            }
        }
        for s in SWABABLES {
            assert!(!table(s.table).is_empty(), "{}: {}", s.path, s.table);
        }
        assert_eq!(
            swabable("/mob/living/basic/carp/mega").map(|s| s.table),
            Some("CELL_LINE_TABLE_MEGACARP")
        );
        assert!(swabable("/obj/item/organ/heart/cybernetic").is_none());
        let mut rng = CombatRng(3);
        let sample = generate_sample(&mut rng, "CELL_LINE_TABLE_CLOWN", 3, 0);
        assert_eq!(sample.organisms.len(), 3, "three different clown lines");
        let mut lines: Vec<usize> = sample
            .organisms
            .iter()
            .map(|o| match o {
                Organism::Cell { line, .. } => *line,
                Organism::Virus => usize::MAX,
            })
            .collect();
        lines.dedup();
        lines.sort_unstable();
        lines.dedup();
        assert_eq!(lines.len(), 3, "no repeats");
    }
}
