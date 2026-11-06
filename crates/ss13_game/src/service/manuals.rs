//! The tgui reference manuals (server), after tg `/obj/item/tgui_book` (`code/modules/library/book.dm`):
//! the SDSM-35 (`DSMBook`: brain traumas, trauma-like quirks, addictions, phobias) and the IDC-27
//! (`IDCBook`: diseases and virus symptoms). Used in hand they open their window with a page
//! turn; the window's `play_flip_sound` turns a page again. Readable while reachable, watched up
//! to two tiles away, closed for the blind (`book_state`).
//!
//! The lists are tg's static data, generated from the DM types into [`super::manual_data`], so
//! the book also names types whose mechanics are not ported.

use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{AlwaysState, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiWindow};

use super::manual_data::{DSM, IDC};
use ss13_tgui::UiInterface as _;

use super::{DsmBookData, DsmBookUi, DsmTrauma, IdcBookData, IdcBookUi, IdcDisease};
use crate::chem::name_of;
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::tools::tell;

const DSM_PATH: &str = "/obj/item/tgui_book/manual/dsm";
const IDC_PATH: &str = "/obj/item/tgui_book/manual/idc";
/// `SFX_PAGE_TURN`.
const PAGE_TURN: &str = "page_turn";
const NO_DESC: &str = "No description recorded - this is an error. Report this lack of research.";
const NO_SYMPTOMS: &str = "No symptoms recorded - this is an error. Report this lack of research.";

/// A `/obj/item/tgui_book/manual` and which window it opens (`ui_name`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Manual {
    Dsm,
    Idc,
}

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    let manual = if is_under(path, DSM_PATH) {
        Manual::Dsm
    } else if is_under(path, IDC_PATH) {
        Manual::Idc
    } else {
        return;
    };
    world.entity_mut(e).insert((manual, AlwaysState));
}

fn page_turn(world: &mut World, book: Entity) {
    if let Some(at) = crate::chem::map_pos(world, book) {
        let sound = crate::sound::PlaySound::new(at, PAGE_TURN, 30.0, true)
            .extrarange(crate::sound::SHORT_RANGE_SOUND_EXTRARANGE);
        crate::sound::playsound(world, sound);
    }
}

/// `tgui_book/ui_interact`. True when `item` is a manual.
pub(super) fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(&manual) = world.get::<Manual>(item) else {
        return false;
    };
    if crate::vision::is_blind(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "You are blind and can't read anything!",
        );
        return true;
    }
    let title = name_of(world, item);
    let open = match manual {
        Manual::Dsm => OpenUi::new::<DsmBookUi>(client, mob, item, title),
        Manual::Idc => OpenUi::new::<IdcBookUi>(client, mob, item, title),
    };
    world.write_message(open);
    page_turn(world, item);
    true
}

/// `dsm/ui_static_data`, with its validation's placeholders.
pub fn dsm_data() -> DsmBookData {
    let or =
        |text: &str, placeholder: &str| if text.is_empty() { placeholder } else { text }.to_owned();
    DsmBookData {
        traumas: DSM
            .iter()
            .map(|[full_name, scan_name, desc, symptoms, id]| DsmTrauma {
                full_name: (*full_name).to_owned(),
                scan_name: (*scan_name).to_owned(),
                desc: or(desc, NO_DESC),
                symptoms: or(symptoms, NO_SYMPTOMS),
                id: (*id).to_owned(),
            })
            .collect(),
    }
}

/// `idc/ui_static_data`: the diseases, the symptoms and the advanced disease entry.
pub fn idc_data() -> IdcBookData {
    let mut diseases: Vec<IdcDisease> = IDC
        .iter()
        .map(
            |[
                form,
                agent,
                name,
                desc,
                illness,
                spread_by,
                cured_by,
                id,
                level,
            ]| {
                let symptom = id.starts_with("/datum/symptom");
                IdcDisease {
                    form: (*form).to_owned(),
                    agent: (*agent).to_owned(),
                    name: (*name).to_owned(),
                    desc: if desc.is_empty() { NO_DESC } else { desc }.to_owned(),
                    illness: (*illness).to_owned(),
                    spread_by: (*spread_by).to_owned(),
                    cured_by: if cured_by.is_empty() && !symptom {
                        "Unknown"
                    } else {
                        cured_by
                    }
                    .to_owned(),
                    id: (*id).to_owned(),
                    level: (*level).to_owned(),
                }
            },
        )
        .collect();
    diseases.push(IdcDisease {
        form: "Virus".into(),
        agent: "Microbes".into(),
        name: "Advanced Disease".into(),
        desc: "A catch-all category for diseases that are too complex to document in their \
            entirety. The Virology department is known for bio-engineering these diseases, as \
            such their danger can vary wildly. These diseases are often a combination of various \
            symptoms, each cataloged in this book individually."
            .into(),
        illness: "N/A".into(),
        spread_by: "Varies by symptom combination".into(),
        cured_by: "Varies by symptom combination".into(),
        id: "/datum/disease/advance".into(),
        level: "N/A".into(),
    });
    IdcBookData { diseases }
}

/// The only act of a tgui book.
fn parse_flip(action: &str) -> Result<(), UiReject> {
    if action == "play_flip_sound" {
        Ok(())
    } else {
        Err(UiReject::UnknownAction)
    }
}

impl ServerUi for DsmBookUi {
    type Source = &'static Manual;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &Manual) -> DsmBookData {
        dsm_data()
    }

    fn data(_: &Manual, _: Entity, _: &()) {}

    fn parse_act(action: &str, _: &UiParams) -> Result<(), UiReject> {
        parse_flip(action)
    }
}

impl ServerUi for IdcBookUi {
    type Source = &'static Manual;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &Manual) -> IdcBookData {
        idc_data()
    }

    fn data(_: &Manual, _: Entity, _: &()) {}

    fn parse_act(action: &str, _: &UiParams) -> Result<(), UiReject> {
        parse_flip(action)
    }
}

pub(super) fn flip_sounds(
    mut dsm: MessageReader<UiActed<DsmBookUi>>,
    mut idc: MessageReader<UiActed<IdcBookUi>>,
    mut commands: Commands,
) {
    for book in dsm
        .read()
        .map(|a| a.source)
        .chain(idc.read().map(|a| a.source))
    {
        commands.queue(move |world: &mut World| page_turn(world, book));
    }
}

/// `book_state`: interactive within reach (or in hand), updates at two tiles, closed further
/// away and for the blind.
pub(super) fn window_status(world: &mut World) {
    let mut windows = world.query::<(Entity, &UiWindow)>();
    let open: Vec<(Entity, Entity, Entity)> = windows
        .iter(world)
        .filter(|(_, w)| w.interface == DsmBookUi::NAME || w.interface == IdcBookUi::NAME)
        .map(|(e, w)| (e, w.actor, w.source))
        .collect();
    for (window, actor, source) in open {
        let held = world
            .get::<Inventory>(actor)
            .is_some_and(|i| i.holds(source));
        let distance = match (
            world.get::<TilePos>(actor),
            crate::chem::map_pos(world, source),
        ) {
            (Some(a), Some(b)) => a.chebyshev(b),
            _ => None,
        };
        let status = if crate::vision::is_blind(world, actor) {
            UiStatus::Close
        } else if held || matches!(distance, Some(0..=1)) {
            UiStatus::Interactive
        } else if distance == Some(2) {
            UiStatus::Update
        } else {
            UiStatus::Close
        };
        if let Some(mut w) = world.get_mut::<UiWindow>(window) {
            w.cap_status(status);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manuals_list_tg_entries() {
        let dsm = dsm_data();
        assert!(
            dsm.traumas
                .iter()
                .any(|t| t.id == "/datum/brain_trauma/severe/mute")
        );
        assert!(
            dsm.traumas
                .iter()
                .any(|t| t.id == "/datum/brain_trauma/mild/phobia/spiders")
        );
        assert!(
            !dsm.traumas
                .iter()
                .any(|t| t.id == "/datum/brain_trauma/mild/phobia"),
            "phobias come from `phobia_types` only"
        );
        assert!(dsm.traumas.iter().all(|t| !t.desc.is_empty()));
        let idc = idc_data();
        let cold = idc
            .diseases
            .iter()
            .find(|d| d.id == "/datum/disease/cold")
            .expect("the cold");
        assert_eq!(cold.spread_by, "Airborne");
        assert!(cold.cured_by.contains(" or rest"));
        assert!(
            !idc.diseases
                .iter()
                .any(|d| d.id == "/datum/disease/fake_gbs"),
            "HIDDEN_BOOK diseases are left out"
        );
        assert!(idc.diseases.iter().any(|d| d.form == "Symptom"));
        assert_eq!(
            idc.diseases.last().map(|d| d.name.as_str()),
            Some("Advanced Disease")
        );
    }
}
