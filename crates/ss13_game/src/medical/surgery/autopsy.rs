//! Autopsy and dissection, after tg `operation_autopsy.dm` (`/datum/surgery_operation/limb/autopsy`
//! and its `mechanic` variant), `operation_dissection.dm` (`/datum/surgery_operation/basic/
//! dissection`, `/obj/item/research_notes`), the autopsy scanner (`autopsy_scanner.dm`) and the
//! operating computer's autopsy experiments (`/datum/experiment/autopsy`,
//! `try_run_autopsy_experiment`).
//!
//! - Autopsy: the autopsy scanner on the opened chest of a corpse not autopsied before prints the
//!   report into the surgeon's hands; on an operating table next to a computer every available
//!   autopsy experiment the corpse fits completes (`EXPERIMENT_CONFIG_ALWAYS_ACTIVE`): human
//!   (`/datum/species/human` exactly), non-human (any other species) or xenomorph.
//! - Dissection (locked: researched or from the computer): the corpse's worth in research notes
//!   (10, a monkey 2, an abductor 40, a golem or zombie 30, a jelly or pod person 20), 80 brute to
//!   the chest, once per corpse; a failure finds 1% of it (rounded).
//! - Research notes add up when one is used on another; merged into the notes in the other hand
//!   when the surgeon's hands are full, a different origin adds 30% of the smaller one once.
// shortcut: the xenomorph experiment cannot complete (aliens have no bodyparts to operate on
// here); no morbid speed bonus or mood events (`TRAIT_MORBID`), no clumsy misreading of the
// scanner; the ancient research server that turns notes into points (oldstation ruin) is not
// ported. tg makes notes from a failed dissection (`round(value * 0.01)` = 0) worth the default
// 100 (`if(value)` skips a 0); here they are worth what was found.

use super::*;

/// The autopsy scanner (`/obj/item/autopsy_scanner`).
pub const AUTOPSY_SCANNER: &str = "/obj/item/autopsy_scanner";
/// `/obj/item/research_notes`.
pub const RESEARCH_NOTES: &str = "/obj/item/research_notes";
const PAPER: &str = "/obj/item/paper";
const PRINTER_SOUND: &str = "sound/machines/printer.ogg";
const PING_SOUND: &str = "sound/machines/ping.ogg";
/// `TECHWEB_TIER_1_POINTS`, `TECHWEB_TIER_3_POINTS`, `TECHWEB_TIER_5_POINTS`.
const TIER_1_POINTS: f64 = 40.0;
const TIER_3_POINTS: f64 = 120.0;
const TIER_5_POINTS: f64 = 200.0;
/// The autopsy report's `color`.
const REPORT_COLOR: [u8; 4] = [0x99, 0xcc, 0xff, 0xff];

/// tg `TRAIT_DISSECTED` from `AUTOPSY_TRAIT`: no second autopsy.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Autopsied;

/// tg `TRAIT_DISSECTED` from `EXPERIMENTAL_SURGERY_TRAIT`: no second dissection.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Dissected;

/// `/obj/item/research_notes` `value`, `origin_type`, `mixed`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ResearchNotes {
    pub value: f64,
    pub origin: String,
    pub mixed: bool,
}

impl Default for ResearchNotes {
    fn default() -> Self {
        Self {
            value: 100.0,
            origin: "debug".to_owned(),
            mixed: false,
        }
    }
}

impl ResearchNotes {
    /// `change_vol`: the name and icon state by the value.
    pub fn look(&self) -> (String, &'static str) {
        let origin = &self.origin;
        if self.value >= TIER_5_POINTS {
            (
                format!("Революционное открытие в области {origin}"),
                "docs_verified",
            )
        } else if self.value >= TIER_3_POINTS {
            (format!("Эссе о {origin}"), "paper_words")
        } else if self.value >= TIER_1_POINTS {
            (format!("Примечания к {origin}"), "paperslip_words")
        } else {
            (format!("Фрагментарные данные о {origin}"), "scrap")
        }
    }

    /// `merge`: the other notes' value, plus 30% of the smaller one the first time two origins
    /// mix.
    pub fn merge(&mut self, other: &Self) {
        let bonus = self.value.min(other.value);
        self.value += other.value;
        if self.origin != other.origin && !self.mixed {
            self.value += bonus * 0.3;
            self.origin = format!("{} и {}", self.origin, other.origin);
            self.mixed = true;
        }
    }
}

/// `dissection/check_value`: what dissecting `mob` is worth.
pub fn dissection_value(world: &World, mob: Entity) -> f64 {
    use crate::species::{SpeciesId, Trait};
    let reward = 10.0;
    if world.get::<crate::species::Species>(mob).is_none() {
        // `else reward /= 6`: no other mob can be operated on here.
        return reward / 6.0;
    }
    let path = crate::species::def_of(world, mob).path;
    if crate::species::has_trait(world, mob, Trait::LesserHumanoid) {
        reward / 5.0
    } else if is_under(path, "/datum/species/abductor") {
        reward * 4.0
    } else if is_under(path, "/datum/species/golem")
        || crate::species::species_of(world, mob) == SpeciesId::Zombie
    {
        reward * 3.0
    } else if is_under(path, "/datum/species/jelly") || is_under(path, "/datum/species/pod") {
        reward * 2.0
    } else {
        reward
    }
}

/// `autopsy/on_success`: marked, the report printed, the operating computer's experiments.
pub(super) fn autopsy_success(world: &mut World, actor: Actor, mob: Entity) {
    world.entity_mut(mob).insert(Autopsied);
    print_report(world, actor, mob);
    let computer = table_and_computer(world, mob).and_then(|(_, c)| c);
    if let Some(computer) = computer {
        run_autopsy_experiments(world, computer, mob);
    }
}

/// `scan_cadaver`: the report on a new paper, into the surgeon's hands.
fn print_report(world: &mut World, actor: Actor, mob: Entity) {
    if world.get::<Stat>(mob) != Some(&Stat::Dead) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(actor.mob) else {
        return;
    };
    let Some(paper) = crate::construction::spawn_obj(world, PAPER, pos) else {
        return;
    };
    let coroner = name_of(world, actor.mob);
    let text = crate::health_scan::autopsy_report(world, &coroner, mob);
    let stamp = world
        .get_resource::<crate::round::Ticker>()
        .map_or(0.0, |t| {
            t.round_duration(world.resource::<Time>().elapsed_secs_f64())
        })
        .max(0.0) as u64;
    let name = format!(
        "отчёт о вскрытии {} - {:02}:{:02}",
        name_of(world, mob),
        stamp / 3600,
        stamp / 60 % 60
    );
    world.entity_mut(paper).insert(ss13_mapload::DmName {
        name,
        desc: String::new(),
    });
    crate::service::write_printed(world, paper, text);
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(paper) {
        look.color = Some(REPORT_COLOR);
    }
    if let Some(inventory) = world.get::<Inventory>(actor.mob).cloned() {
        crate::storage::put_in_hands(world, actor.mob, &inventory, paper);
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, PRINTER_SOUND, 50.0, false),
    );
    tell(world, actor.client, Span::Notice, "отчёт напечатан");
}

/// `try_run_autopsy_experiment` of the computer (`EXPERIMENT_CONFIG_ALWAYS_ACTIVE`): every
/// available autopsy experiment `mob` fits completes.
fn run_autopsy_experiments(world: &mut World, computer: Entity, mob: Entity) {
    use crate::science::techweb::{ResearchDb, Techweb};
    use ss13_mapload::research::ExperimentKind;
    let fitting: Vec<String> = {
        let (Some(web), Some(db)) = (
            world.get_resource::<Techweb>(),
            world.get_resource::<ResearchDb>(),
        ) else {
            return;
        };
        web.available_experiments
            .iter()
            .filter(|e| match &db.experiments[e.def].kind {
                ExperimentKind::Autopsy { subject } => autopsy_fits(world, subject, mob),
                _ => false,
            })
            .map(|e| e.id.clone())
            .collect()
    };
    if fitting.is_empty() {
        return;
    }
    for id in fitting {
        let text = crate::science::experi_scanner::complete_and_announce(world, &id);
        // The computer's own handler takes autopsy experiments: it announces too.
        crate::science::experi_scanner::handler_say(world, computer, &text);
    }
    if let Some(&pos) = world.get::<TilePos>(computer) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, PING_SOUND, 25.0, false),
        );
    }
    crate::science::experi_scanner::handler_say(
        world,
        computer,
        "Новое уникальное вскрытие успешно записано.",
    );
}

/// `is_valid_autopsy` of the `autopsy` subtypes.
fn autopsy_fits(world: &World, subject: &str, mob: Entity) -> bool {
    let species = world
        .get::<crate::species::Species>(mob)
        .map(|_| crate::species::def_of(world, mob).path);
    match subject {
        // `ishumanbasic`.
        "human" => species == Some("/datum/species/human"),
        "nonhuman" => species.is_some_and(|p| p != "/datum/species/human"),
        // `isalien`: aliens are basic mobs here, with no chest to open.
        "xenomorph" => false,
        _ => true,
    }
}

/// `dissection/on_success` and `on_failure`: the notes, 80 brute to the chest, marked.
pub(super) fn dissection_done(world: &mut World, actor: Actor, mob: Entity, success: bool) {
    let value = dissection_value(world, mob);
    let points = if success {
        value
    } else {
        (value * 0.01).round()
    };
    let (user, target) = (name_of(world, actor.mob), name_of(world, mob));
    let own = if success {
        let shown = dm_number(points);
        format!("Вы производите вскрытие {target} и обнаруживаете {shown} точку данных!")
    } else {
        format!("Вы производите вскрытие {target}, но не находите ничего интересного.")
    };
    let others = format!("{user} производит вскрытие {target}.");
    visible_range(world, actor.mob, &own, &others, Span::Warning, 1);
    give_notes(world, actor.mob, points, "biology");
    damage_limb(world, mob, BodyZone::Chest, DamageType::Brute, 80.0);
    world.entity_mut(mob).insert(Dissected);
}

/// A DM number as `"[x]"` prints it (no trailing `.0`, at most 6 significant digits).
fn dm_number(x: f64) -> String {
    let text = format!("{:.4}", x);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// `give_paper`: new notes into the surgeon's hands; with both hands full they merge into the
/// notes in the other hand, else they stay at the surgeon's feet.
fn give_notes(world: &mut World, surgeon: Entity, value: f64, origin: &str) {
    let Some(&pos) = world.get::<TilePos>(surgeon) else {
        return;
    };
    let notes = ResearchNotes {
        value,
        origin: origin.to_owned(),
        mixed: false,
    };
    let Some(inventory) = world.get::<Inventory>(surgeon).cloned() else {
        return;
    };
    let free_hand = inventory.hands.iter().any(Option::is_none);
    if !free_hand && let Some(held) = inventory.hands[1 - inventory.active] {
        let merged = match world.get_mut::<ResearchNotes>(held) {
            Some(mut theirs) => {
                theirs.merge(&notes);
                true
            }
            None => false,
        };
        if merged {
            refresh_notes(world, held);
            return;
        }
    }
    let Some(item) = crate::construction::spawn_obj(world, RESEARCH_NOTES, pos) else {
        return;
    };
    world.entity_mut(item).insert(notes);
    refresh_notes(world, item);
    if free_hand {
        crate::storage::put_in_hands(world, surgeon, &inventory, item);
    }
}

/// `change_vol`: the notes' name and sprite follow their value.
pub fn refresh_notes(world: &mut World, notes: Entity) {
    let Some((name, state)) = world.get::<ResearchNotes>(notes).map(ResearchNotes::look) else {
        return;
    };
    let icon = world
        .get::<ss13_mapload::DmAtom>(notes)
        .map(|a| a.icon.clone())
        .unwrap_or_default();
    let id = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(&icon, state));
    if let Some(mut dm) = world.get_mut::<ss13_mapload::DmName>(notes) {
        dm.name.clone_from(&name);
    }
    world.entity_mut(notes).insert(Name::new(name));
    if let (Some(id), Some(mut look)) = (id, world.get_mut::<ss13_shared::Appearance>(notes)) {
        look.icon_state = id;
    }
}

/// Mapped or spawned notes start as tg's defaults (`value` 100, `origin_type` "debug").
pub(super) fn add_notes(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, RESEARCH_NOTES))
    {
        let entity = add.entity;
        commands.queue(move |world: &mut World| {
            if world.get::<ResearchNotes>(entity).is_none() {
                world.entity_mut(entity).insert(ResearchNotes::default());
                refresh_notes(world, entity);
            }
        });
    }
}

/// `research_notes/item_interaction`: notes used on notes add their value (no mixing bonus) and
/// are used up. True when `held` and `target` are both notes.
pub fn notes_interaction(world: &mut World, held: Entity, target: Entity) -> bool {
    if held == target {
        return false;
    }
    let Some(added) = world.get::<ResearchNotes>(held).map(|n| n.value) else {
        return false;
    };
    match world.get_mut::<ResearchNotes>(target) {
        Some(mut notes) => notes.value += added,
        None => return false,
    }
    refresh_notes(world, target);
    world.despawn(held);
    true
}

/// `research_notes/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(notes) = world.get::<ResearchNotes>(target) {
        lines.push((
            Span::Notice,
            format!(
                "Это стоит {} исследовательских очков.",
                dm_number(notes.value)
            ),
        ));
    }
}

/// `/obj/item/autopsy_scanner/interact_with_atom` out of surgery: an advanced health scan of a
/// corpse, an error for the living.
pub fn scan_corpse(world: &mut World, actor: Actor, target: Entity) {
    // `IS_DEAD_OR_FAKING`.
    if world.get::<Stat>(target) != Some(&Stat::Dead) {
        tell(
            world,
            actor.client,
            Span::Warning,
            "ОШИБКА! НЕВОЗМОЖНО СКАНИРОВАТЬ ЖИВЫХ. ВОЗЬМИТЕ АНАЛИЗАТОР ЗДОРОВЬЯ ИЛИ ПРИКОНЧИТЕ \
             ПАЦИЕНТА.",
        );
        return;
    }
    let (user, scanned) = (name_of(world, actor.mob), name_of(world, target));
    let text = format!("{user} сканирует труп {scanned}.");
    super::super::visible(world, actor.mob, &text, &text, Span::Notice);
    tell(world, actor.client, Span::Notice, "АНАЛИЗ ТРУПА.");
    if let Some(message) = crate::health_scan::healthscan(world, target) {
        world.write_message(ss13_net::chat::server::SendChat {
            clients: vec![actor.client],
            message,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_names_follow_value_and_merge_mixes_once() {
        let mut notes = ResearchNotes {
            value: 10.0,
            origin: "biology".into(),
            mixed: false,
        };
        assert_eq!(notes.look().1, "scrap");
        notes.merge(&ResearchNotes {
            value: 40.0,
            origin: "biology".into(),
            mixed: false,
        });
        assert_eq!((notes.value, notes.mixed), (50.0, false));
        assert_eq!(
            notes.look(),
            ("Примечания к biology".into(), "paperslip_words")
        );
        notes.merge(&ResearchNotes {
            value: 100.0,
            origin: "debug".into(),
            mixed: false,
        });
        // 50 + 100 + 30% of 50.
        assert_eq!(notes.value, 165.0);
        assert_eq!(notes.origin, "biology и debug");
        notes.merge(&ResearchNotes {
            value: 100.0,
            origin: "physics".into(),
            mixed: false,
        });
        assert_eq!(notes.value, 265.0, "the bonus applies once");
        assert_eq!(notes.look().1, "docs_verified");
        assert_eq!(dm_number(1.0 / 6.0 * 10.0), "1.6667");
        assert_eq!(dm_number(40.0), "40");
    }
}
