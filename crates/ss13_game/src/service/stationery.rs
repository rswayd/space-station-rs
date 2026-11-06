//! Carbon paper, biscuit cards, the paper cutter, filing cabinets, employment contracts and
//! departmental paperwork (server), after tg `code/modules/paperwork/carbonpaper.dm`,
//! `paper_biscuit.dm`, `paper_cutter.dm`, `filingcabinet.dm`, `contract.dm` and `paperwork.dm`.
//!
//! - A sheet of carbon held in hand tears off its carbon copy ("Copy - name", the text and
//!   stamps) into the hands once; after that it folds into a plane like any paper.
//! - A biscuit card is a folder: sealed, it refuses papers in and out and cracking it in hand
//!   (an alert) is for good; the unsealed kind seals once in hand. The spare ID safe code cards
//!   hold their slip.
//! - The paper cutter holds a paper and its blade: alt-click takes the paper out (else the
//!   blade, unscrewed); a screwdriver (un)secures the blade; a right click with an empty hand
//!   cuts the paper into two paper slips.
//! - A filing cabinet takes anything smaller than normal (mapped ones take the small things on
//!   their tile); an empty hand lists the contents to take one out (tgui `FilingCabinet` as a
//!   list); a wrench right click (2 s) fastens or unfastens it. The security, medical and
//!   employment cabinets print the crew's rapsheets, medical records and employment contracts
//!   when first opened (the employment one also files whoever joined since).
//! - Departmental paperwork is completed by its head's stamp (an overlay); a VOID stamp voids a
//!   photocopy.
// shortcut: no clumsy paper cuts (no clumsy trait), no telekinesis, no examine hints, no
// chameleon stamps; paperwork is not sold yet (its export overrides `get_base_cost` and needs
// negative export values: cargo group) and has no job-only examine text.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, DmObj, DmWeight, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptKind, ask_world};
use ss13_world::SpatialIndex;

use super::paper::{PAPER, Paper};
use super::server::{
    PendingAsk, TimerJob, after, client_of, item_path, set_icon_state, set_name, sound_at,
};
use crate::chem::{map_pos, name_of};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::storage::{Stored, StoredIn, in_reach, put_in_hands, put_inside, take_out};
use crate::tools::{Actor, ToolKind, tell, tool_of, use_tool};

const CARBON: &str = "/obj/item/paper/carbon";
const CARBON_COPY: &str = "/obj/item/paper/carbon_copy";
const BISCUIT: &str = "/obj/item/folder/biscuit";
const BISCUIT_UNSEALED: &str = "/obj/item/folder/biscuit/unsealed";
const CUTTER: &str = "/obj/item/papercutter";
const CUTTER_BLADE: &str = "/obj/item/hatchet/cutterblade";
const PAPERSLIP: &str = "/obj/item/paper/paperslip";
const CABINET: &str = "/obj/structure/filingcabinet";
const CONTRACT: &str = "/obj/item/paper/employment_contract";
const PAPERWORK: &str = "/obj/item/paperwork";
const PHOTOCOPY: &str = "/obj/item/paperwork/photocopy";
const ANCIENT: &str = "/obj/item/paperwork/ancient";
const VOID_STAMP: &str = "/obj/item/stamp/void";
const STAMP: &str = "/obj/item/stamp";
const BUREAUCRACY_DMI: &str = "icons/obj/service/bureaucracy.dmi";
/// `WEIGHT_CLASS_NORMAL`.
const WEIGHT_NORMAL: u8 = 3;
/// The cabinet's `-open` flash (`0.5 SECONDS`).
const CABINET_FLASH_SECS: f64 = 0.5;
/// `wrench_act_secondary`: `use_tool(src, user, 20)`, deciseconds.
const CABINET_WRENCH_DS: f32 = 20.0;
const CRACK_SOUND: &str = "sound/effects/wounds/crack1.ogg";
const SEAL_SOUND: &str = "sound/items/duct_tape/duct_tape_snap.ogg";
const SLASH_SOUND: &str = "sound/items/weapons/slash.ogg";
const SCREWDRIVER_SOUND: &str = "sound/items/tools/screwdriver.ogg";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";

/// `/obj/item/paper/carbon` `copied`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CarbonPaper {
    pub copied: bool,
}

/// `/obj/item/folder/biscuit` `cracked`; `sealable`: an unsealed card not sealed yet.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Biscuit {
    pub cracked: bool,
    pub sealable: bool,
}

/// `/obj/item/papercutter` `stored_paper`, `stored_blade`, `blade_secured`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaperCutter {
    pub paper: Option<Entity>,
    pub blade: Option<Entity>,
    pub secured: bool,
}

/// What a filing cabinet prints when opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CabinetKind {
    Plain,
    Security,
    Medical,
    Employment,
}

/// `/obj/structure/filingcabinet` (its contents are `Stored`); `virgin` of the record kinds, and
/// the locked records the employment cabinet already filed.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct FilingCabinet {
    pub kind: CabinetKind,
    pub virgin: bool,
    pub filed: Vec<u32>,
}

/// `/obj/item/paperwork`: the stamp it asks for (`stamp_requested`), the overlay that stamp
/// leaves (`stamp_icon`), `stamped`, and for photocopies `voided`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Paperwork {
    pub requested: &'static str,
    pub stamp_icon: &'static str,
    pub stamped: bool,
    pub photocopy: bool,
    pub voided: bool,
}

/// `GLOB.manifest.print_count` of printed rapsheets.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct RapsheetCount(pub u32);

/// The questions of these items, with what they were asked about.
#[derive(Clone, Debug)]
pub enum StationeryAsk {
    Crack { biscuit: Entity },
    Seal { biscuit: Entity },
    Cabinet { cabinet: Entity, items: Vec<Entity> },
}

/// The head-of-staff paperwork kinds: `(type, stamp_requested, stamp_icon)`.
const PAPERWORK_KINDS: [(&str, &str, &str); 7] = [
    (
        "/obj/item/paperwork/cargo",
        "/obj/item/stamp/head/qm",
        "paper_stamp-qm",
    ),
    (
        "/obj/item/paperwork/security",
        "/obj/item/stamp/head/hos",
        "paper_stamp-hos",
    ),
    (
        "/obj/item/paperwork/service",
        "/obj/item/stamp/head/hop",
        "paper_stamp-hop",
    ),
    (
        "/obj/item/paperwork/medical",
        "/obj/item/stamp/head/cmo",
        "paper_stamp-cmo",
    ),
    (
        "/obj/item/paperwork/engineering",
        "/obj/item/stamp/head/ce",
        "paper_stamp-ce",
    ),
    (
        "/obj/item/paperwork/research",
        "/obj/item/stamp/head/rd",
        "paper_stamp-rd",
    ),
    (
        "/obj/item/paperwork/captain",
        "/obj/item/stamp/head/captain",
        "paper_stamp-cap",
    ),
];

/// `Initialize` of these items.
pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, CARBON) {
        world.entity_mut(e).insert(CarbonPaper::default());
    } else if is_under(path, BISCUIT) {
        let unsealed = is_under(path, BISCUIT_UNSEALED);
        world.entity_mut(e).insert(Biscuit {
            cracked: unsealed,
            sealable: unsealed,
        });
        add_biscuit_slip(world, e, path);
    } else if is_under(path, CUTTER) {
        let blade = map_pos(world, e).and_then(|pos| {
            let blade = crate::construction::spawn_obj(world, CUTTER_BLADE, pos)?;
            put_inside(world, blade, e);
            Some(blade)
        });
        world.entity_mut(e).insert(PaperCutter {
            paper: None,
            blade,
            secured: true,
        });
        refresh_cutter(world, e);
    } else if is_under(path, PAPERWORK) {
        world.entity_mut(e).insert(paperwork_for(world, path));
    }
}

pub(super) fn add_obj_parts(world: &mut World, e: Entity, path: &str) {
    if !is_under(path, CABINET) || world.get::<FilingCabinet>(e).is_some() {
        return;
    }
    let kind = if is_under(path, "/obj/structure/filingcabinet/security") {
        CabinetKind::Security
    } else if is_under(path, "/obj/structure/filingcabinet/medical") {
        CabinetKind::Medical
    } else if is_under(path, "/obj/structure/filingcabinet/employment") {
        CabinetKind::Employment
    } else {
        CabinetKind::Plain
    };
    world.entity_mut(e).insert(FilingCabinet {
        kind,
        virgin: true,
        filed: Vec::new(),
    });
}

/// `paperwork/Initialize`; ancient paperwork takes a random head's kind (`copy_stamp_info`).
fn paperwork_for(world: &mut World, path: &str) -> Paperwork {
    let mut work = Paperwork {
        requested: VOID_STAMP,
        stamp_icon: "paper_stamp-void",
        stamped: false,
        photocopy: is_under(path, PHOTOCOPY),
        voided: false,
    };
    let kind = if is_under(path, ANCIENT) {
        let i = world
            .get_resource_mut::<crate::combat::CombatRng>()
            .map_or(0, |mut rng| {
                rng.range(0, PAPERWORK_KINDS.len() as u32 - 1) as usize
            });
        PAPERWORK_KINDS.get(i)
    } else {
        PAPERWORK_KINDS
            .iter()
            .find(|(root, ..)| is_under(path, root))
    };
    if let Some(&(_, requested, icon)) = kind {
        work.requested = requested;
        work.stamp_icon = icon;
    }
    if work.photocopy {
        work.stamp_icon = "paper_stamp-pc";
    }
    work
}

/// `biscuit/Initialize`: the spare ID safe code cards make their slip. The station has no spare
/// ID safe, so the code reads `[REDACTED]` as tg prints it without a code.
fn add_biscuit_slip(world: &mut World, biscuit: Entity, path: &str) {
    let (slip, text) = match path.rsplit('/').next().unwrap_or_default() {
        "spare_id_safe_code" => (
            "/obj/item/paper/paperslip/corporate/fluff/spare_id_safe_code",
            "Captain's Spare ID safe code combination: [REDACTED]<br><br>The spare ID can be \
             found in its dedicated safe on the bridge.<br><br>If your job would not ordinarily \
             have Head of Staff access, your ID card has been specially modified to possess it.",
        ),
        "emergency_spare_id_safe_code" => (
            "/obj/item/paper/paperslip/corporate/fluff/emergency_spare_id_safe_code",
            "Captain's Spare ID safe code combination: [REDACTED]<br><br>The spare ID can be \
             found in its dedicated safe on the bridge.",
        ),
        _ => return,
    };
    let Some(pos) = map_pos(world, biscuit) else {
        return;
    };
    let Some(slip) = crate::construction::spawn_obj(world, slip, pos) else {
        return;
    };
    put_inside(world, slip, biscuit);
    super::paper::write_printed(world, slip, text);
}

/// The paper overlay of a folder (`get_paper_overlay`): a sealed biscuit shows none.
pub(super) fn folder_overlay(world: &World, folder: Entity) -> Option<&'static str> {
    match world.get::<Biscuit>(folder) {
        Some(b) if !b.cracked => None,
        Some(_) => Some("paperbiscuit_paper"),
        None => Some("folder_paper"),
    }
}

/// The base sprite of a paper before `_words`: a torn carbon is a plain `paper`.
pub(super) fn paper_base_state(world: &World, paper: Entity) -> Option<&'static str> {
    world
        .get::<CarbonPaper>(paper)
        .filter(|c| c.copied)
        .map(|_| "paper")
}

fn to_hands(world: &mut World, mob: Entity, item: Entity) {
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        put_in_hands(world, mob, &inventory, item);
    } else if let Some(pos) = map_pos(world, mob) {
        take_out(world, item, pos);
    }
}

fn holds(world: &World, mob: Entity, item: Entity) -> bool {
    world.get::<Inventory>(mob).is_some_and(|i| i.holds(item))
}

fn inside(world: &World, item: Entity, container: Entity) -> bool {
    world
        .get::<StoredIn>(item)
        .is_some_and(|s| s.0 == container)
}

fn icon_of(world: &World, e: Entity) -> (String, String) {
    world
        .get::<DmAtom>(e)
        .map(|a| (a.icon.clone(), a.icon_state.clone()))
        .unwrap_or_default()
}

fn ask(
    world: &mut World,
    client: Entity,
    mob: Entity,
    title: &str,
    text: &str,
    kind: PromptKind,
    q: StationeryAsk,
) {
    let prompt = ask_world(world, Ask::new(client, mob, title, text, kind));
    world.entity_mut(prompt).insert(PendingAsk::Stationery(q));
}

fn yes_no() -> PromptKind {
    PromptKind::Alert {
        buttons: vec!["Yes".to_owned(), "No".to_owned()],
        autofocus: true,
    }
}

/// `attack_self` of biscuit cards: crack a sealed one, offer to seal an unsealed one. True when
/// it was a biscuit that asked; an open card falls through to the folder list.
pub(super) fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(biscuit) = world.get::<Biscuit>(item).copied() else {
        return false;
    };
    crate::forensics::server::add_fingerprint(world, item, mob, false);
    if !biscuit.cracked {
        ask(
            world,
            client,
            mob,
            "Biscuit Cracking",
            "Do you want to crack it open?",
            yes_no(),
            StationeryAsk::Crack { biscuit: item },
        );
        return true;
    }
    if biscuit.sealable {
        ask(
            world,
            client,
            mob,
            "Biscuit Sealing",
            "Do you want to seal it? This can only be done once.",
            yes_no(),
            StationeryAsk::Seal { biscuit: item },
        );
        return true;
    }
    false
}

/// The prompts' answers, checked again: the card still held, the cabinet in reach and the item
/// still inside it.
pub(super) fn answer(
    world: &mut World,
    actor: Entity,
    q: StationeryAsk,
    answer: Option<PromptAnswer>,
) {
    let Some(client) = client_of(world, actor) else {
        return;
    };
    match (q, answer) {
        (StationeryAsk::Crack { biscuit }, Some(PromptAnswer::Button(0))) => {
            if !holds(world, actor, biscuit)
                || world.get::<Biscuit>(biscuit).is_none_or(|b| b.cracked)
            {
                return;
            }
            if let Some(mut b) = world.get_mut::<Biscuit>(biscuit) {
                b.cracked = true;
            }
            sound_at(world, actor, CRACK_SOUND, 60.0, false);
            let (icon, state) = icon_of(world, biscuit);
            let base = state.trim_end_matches("_cracked").to_owned();
            set_icon_state(world, biscuit, &icon, &format!("{base}_cracked"));
            super::office::refresh_folder(world, biscuit);
            // `ui_interact(user)` after cracking.
            super::office::attack_self(world, client, actor, biscuit);
        }
        (StationeryAsk::Seal { biscuit }, Some(PromptAnswer::Button(0))) => {
            if !holds(world, actor, biscuit)
                || world
                    .get::<Biscuit>(biscuit)
                    .is_none_or(|b| !b.cracked || !b.sealable)
            {
                return;
            }
            world.entity_mut(biscuit).insert(Biscuit {
                cracked: false,
                sealable: false,
            });
            sound_at(world, actor, SEAL_SOUND, 60.0, false);
            let (icon, state) = icon_of(world, biscuit);
            set_icon_state(world, biscuit, &icon, state.trim_end_matches("_cracked"));
            super::office::refresh_folder(world, biscuit);
        }
        // tg-fork deviation: tg's unsealed card returns without opening anything on "No" (and
        // once sealed and cracked again never opens on use), so its contents are unreachable;
        // here the folder list opens.
        (StationeryAsk::Seal { biscuit }, Some(PromptAnswer::Button(_))) => {
            if holds(world, actor, biscuit) {
                super::office::attack_self(world, client, actor, biscuit);
            }
        }
        (StationeryAsk::Cabinet { cabinet, items }, Some(PromptAnswer::Item(i))) => {
            let Some(&item) = items.get(i) else {
                return;
            };
            if crate::status_effects::incapacitation(world, actor).hands_blocked
                || !in_reach(world, actor, cabinet)
                || !inside(world, item, cabinet)
            {
                return;
            }
            to_hands(world, actor, item);
            flash_open(world, cabinet);
        }
        _ => {}
    }
}

/// tg `click_alt` of a held carbon sheet (it tears off the copy) and of the paper cutter. True
/// when the click was theirs.
pub(super) fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    // tg-fork deviation: tg tears the copy off with a right click on the held sheet and its
    // alt-click only says "Take off the carbon copy first."; a right click on a hand slot has no
    // hook here, so the alt-click that tg blocks tears it.
    if world.get::<CarbonPaper>(target).is_some_and(|c| !c.copied) {
        if holds(world, mob, target) {
            tear_copy(world, client, mob, target);
        } else {
            tell(
                world,
                client,
                Span::Notice,
                "Take off the carbon copy first.",
            );
        }
        return true;
    }
    let Some(cutter) = world.get::<PaperCutter>(target).copied() else {
        return false;
    };
    if !holds(world, mob, target) && !in_reach(world, mob, target) {
        return false;
    }
    // One at a time; the paper first.
    let out = cutter.paper.filter(|&p| inside(world, p, target)).or(cutter
        .blade
        .filter(|&b| !cutter.secured && inside(world, b, target)));
    if let Some(item) = out {
        to_hands(world, mob, item);
        if let Some(mut c) = world.get_mut::<PaperCutter>(target) {
            if c.paper == Some(item) {
                c.paper = None;
            } else {
                c.blade = None;
            }
        }
        refresh_cutter(world, target);
    }
    true
}

/// `carbon/removecopy`.
fn tear_copy(world: &mut World, client: Entity, mob: Entity, carbon: Entity) {
    let Some(pos) = map_pos(world, mob) else {
        return;
    };
    let Some(copy) = crate::construction::spawn_obj(world, CARBON_COPY, pos)
        .or_else(|| crate::construction::spawn_obj(world, PAPER, pos))
    else {
        return;
    };
    super::paper::copy_paper(world, carbon, copy);
    let name = name_of(world, carbon);
    set_name(world, copy, &format!("Copy - {name}"));
    tell(world, client, Span::Notice, "You tear off the carbon-copy!");
    world
        .entity_mut(carbon)
        .insert(CarbonPaper { copied: true });
    super::paper::refresh_paper_look(world, carbon);
    to_hands(world, mob, copy);
}

/// `papercutter/update_overlays`: `paper`, `cutter_overlay`.
fn refresh_cutter(world: &mut World, cutter: Entity) {
    let Some(state) = world.get::<PaperCutter>(cutter).copied() else {
        return;
    };
    let (icon, _) = icon_of(world, cutter);
    let index = world.get_resource::<IconIndex>();
    let wanted: Vec<Overlay> = [
        state.paper.map(|_| "paper"),
        state.blade.map(|_| "cutter_overlay"),
    ]
    .into_iter()
    .flatten()
    .filter_map(|s| index.and_then(|i| i.get(&icon, s)))
    .map(Overlay::new)
    .collect();
    if let Some(mut look) = world.get_mut::<Appearance>(cutter)
        && look.overlays != wanted
    {
        look.overlays = wanted;
    }
}

/// `papercutter/attack_hand_secondary`: cut the paper. True when `target` is a cutter.
pub(super) fn cutter_right_click(
    world: &mut World,
    client: Entity,
    _mob: Entity,
    target: Entity,
) -> bool {
    let Some(cutter) = world.get::<PaperCutter>(target).copied() else {
        return false;
    };
    let paper = cutter.paper.filter(|&p| inside(world, p, target));
    let refusal = if cutter.blade.is_none() {
        Some("no blade!")
    } else if !cutter.secured {
        Some("blade unsecured!")
    } else if paper.is_none() {
        Some("nothing to cut!")
    } else {
        None
    };
    if let Some(text) = refusal {
        tell(world, client, Span::Warning, text);
        return true;
    }
    let Some(paper) = paper else {
        return true;
    };
    sound_at(world, target, SLASH_SOUND, 50.0, true);
    let what = name_of(world, paper);
    tell(
        world,
        client,
        Span::Danger,
        format!("You neatly cut {what}."),
    );
    world.despawn(paper);
    if let Some(mut c) = world.get_mut::<PaperCutter>(target) {
        c.paper = None;
    }
    if let Some(pos) = map_pos(world, target) {
        for _ in 0..2 {
            crate::construction::spawn_obj(world, PAPERSLIP, pos);
        }
    }
    refresh_cutter(world, target);
    true
}

/// `item_interaction` of sealed biscuits, the cutter, cabinets and paperwork. True when the
/// click was theirs.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let held_paper = world.get::<Paper>(held).is_some();
    // `crack_check` of `interact_with_insertables` (both ways round).
    let sealed = |e: Entity| world.get::<Biscuit>(e).is_some_and(|b| !b.cracked);
    if (sealed(target) && held_paper) || (sealed(held) && world.get::<Paper>(target).is_some()) {
        tell(world, client, Span::Warning, "open first!");
        return true;
    }
    if world.get::<PaperCutter>(target).is_some() {
        return cutter_interaction(world, client, held, target);
    }
    if world.get::<FilingCabinet>(target).is_some() {
        return cabinet_insert(world, client, mob, held, target);
    }
    if world.get::<Paperwork>(target).is_some() && is_under(&item_path(world, held), STAMP) {
        stamp_paperwork(world, client, held, target);
        return true;
    }
    false
}

fn cutter_interaction(world: &mut World, client: Entity, held: Entity, cutter: Entity) -> bool {
    let Some(state) = world.get::<PaperCutter>(cutter).copied() else {
        return false;
    };
    let path = item_path(world, held);
    if tool_of(world, held).is_some_and(|(kind, _)| kind == ToolKind::Screwdriver) {
        if state.blade.is_none() && !state.secured {
            tell(world, client, Span::Warning, "no blade!");
            return true;
        }
        sound_at(world, cutter, SCREWDRIVER_SOUND, 50.0, true);
        tell(
            world,
            client,
            Span::Notice,
            if state.secured {
                "unsecured"
            } else {
                "secured"
            },
        );
        if let Some(mut c) = world.get_mut::<PaperCutter>(cutter) {
            c.secured = !c.secured;
        }
        return true;
    }
    if world.get::<Paper>(held).is_some() {
        let refuses = [
            "/obj/item/paper/holy_writ",
            "/obj/item/paper/pamphlet",
            PAPERSLIP,
        ]
        .iter()
        .any(|root| is_under(&path, root));
        if refuses {
            tell(world, client, Span::Warning, "won't fit!");
            return true;
        }
        if state.paper.is_some() {
            tell(world, client, Span::Warning, "already paper inside!");
            return true;
        }
        put_inside(world, held, cutter);
        if let Some(mut c) = world.get_mut::<PaperCutter>(cutter) {
            c.paper = Some(held);
        }
        sound_at(world, cutter, "page_turn", 60.0, true);
        tell(world, client, Span::Notice, "paper inserted");
        refresh_cutter(world, cutter);
        return true;
    }
    if is_under(&path, CUTTER_BLADE) {
        if state.blade.is_some() {
            tell(world, client, Span::Warning, "already a blade inside!");
            return true;
        }
        put_inside(world, held, cutter);
        if let Some(mut c) = world.get_mut::<PaperCutter>(cutter) {
            c.blade = Some(held);
        }
        tell(world, client, Span::Notice, "blade inserted");
        refresh_cutter(world, cutter);
        return true;
    }
    false
}

fn w_class(world: &World, item: Entity) -> u8 {
    world
        .get::<DmWeight>(item)
        .copied()
        .unwrap_or_default()
        .w_class
}

/// `filingcabinet/item_interaction`: small things go in; anything else is refused out of
/// combat mode (in it, the attack goes on).
fn cabinet_insert(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    cabinet: Entity,
) -> bool {
    let (what, into) = (name_of(world, held), name_of(world, cabinet));
    if w_class(world, held) < WEIGHT_NORMAL {
        put_inside(world, held, cabinet);
        tell(
            world,
            client,
            Span::Notice,
            format!("You put {what} in {into}."),
        );
        flash_open(world, cabinet);
        return true;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    if combat {
        return false;
    }
    tell(
        world,
        client,
        Span::Warning,
        format!("You can't put {what} in {into}!"),
    );
    true
}

/// `icon_state = "[initial(icon_state)]-open"`, back after half a second.
fn flash_open(world: &mut World, cabinet: Entity) {
    let (icon, state) = icon_of(world, cabinet);
    set_icon_state(world, cabinet, &icon, &format!("{state}-open"));
    after(
        world,
        CABINET_FLASH_SECS,
        TimerJob::CabinetClose { cabinet },
    );
}

/// The `-open` flash ends.
pub(super) fn close_cabinet(world: &mut World, cabinet: Entity) {
    if world.get_entity(cabinet).is_err() {
        return;
    }
    let (icon, state) = icon_of(world, cabinet);
    set_icon_state(world, cabinet, &icon, &state);
}

/// `wrench_act_secondary` of cabinets. True when `target` is a cabinet and `held` a wrench.
pub(super) fn cabinet_wrench(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<FilingCabinet>(target).is_none()
        || !tool_of(world, held).is_some_and(|(kind, _)| kind == ToolKind::Wrench)
    {
        return false;
    }
    let anchored = world
        .get::<crate::obj_damage::Anchored>(target)
        .map_or_else(
            || world.get::<DmObj>(target).is_some_and(|o| o.anchored),
            |a| a.0,
        );
    let name = name_of(world, target);
    let verb = if anchored { "unwrench" } else { "wrench" };
    tell(
        world,
        client,
        Span::Notice,
        format!("You begin to {verb} {name}."),
    );
    sound_at(world, target, RATCHET, 50.0, true);
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    use_tool(
        world,
        actor,
        target,
        CABINET_WRENCH_DS,
        0,
        crate::construction::Job::Unfasten { anchored },
    );
    true
}

/// `attack_hand` of cabinets: the record kinds print first, then the contents list.
pub(super) fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<FilingCabinet>(target).is_none() {
        return false;
    }
    populate(world, target);
    let items: Vec<Entity> = world
        .get::<Stored>(target)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    let name = name_of(world, target);
    if items.is_empty() {
        tell(world, client, Span::Notice, format!("{name} is empty."));
        return true;
    }
    let entries = items.iter().map(|&i| name_of(world, i)).collect();
    ask(
        world,
        client,
        mob,
        &name,
        "",
        PromptKind::List {
            items: entries,
            default: None,
        },
        StationeryAsk::Cabinet {
            cabinet: target,
            items,
        },
    );
    true
}

/// `populate` / `fillCurrent` + `addFile`: once for the security and medical cabinets (only
/// when there are records), and for every newly joined crew member in the employment one
/// (tg's `AddEmploymentContract` on latejoin).
fn populate(world: &mut World, cabinet: Entity) {
    let Some(state) = world.get::<FilingCabinet>(cabinet).cloned() else {
        return;
    };
    let Some(pos) = map_pos(world, cabinet) else {
        return;
    };
    let Some(manifest) = world.get_resource::<crate::records::manifest::Manifest>() else {
        return;
    };
    let mut papers: Vec<(String, String)> = Vec::new();
    let mut filed = state.filed.clone();
    match state.kind {
        CabinetKind::Plain => return,
        CabinetKind::Security | CabinetKind::Medical if !state.virgin => return,
        CabinetKind::Security => {
            for record in &manifest.general {
                papers.push((record.name.clone(), rapsheet_text(record)));
            }
        }
        CabinetKind::Medical => {
            for record in &manifest.general {
                papers.push((format!("paper - '{}'", record.name), medical_text(record)));
            }
        }
        CabinetKind::Employment => {
            for record in &manifest.locked {
                if filed.contains(&record.id) || world.get_entity(record.mob).is_err() {
                    continue;
                }
                filed.push(record.id);
                papers.push((
                    format!("paper- {} employment contract", record.name),
                    contract_text(&record.name),
                ));
            }
        }
    }
    if papers.is_empty() {
        return;
    }
    let security = state.kind == CabinetKind::Security;
    let kind = if state.kind == CabinetKind::Employment {
        CONTRACT
    } else {
        PAPER
    };
    for (name, text) in papers {
        let name = if security {
            let n = {
                let mut count = world.get_resource_or_insert_with(RapsheetCount::default);
                count.0 += 1;
                count.0
            };
            format!("SR-{n} '{name}'")
        } else {
            name
        };
        let text = if security {
            let n = world.resource::<RapsheetCount>().0;
            format!("<center><b>SR-{n}: Rapsheet</b></center><br>{text}")
        } else {
            text
        };
        let Some(paper) = crate::construction::spawn_obj(world, kind, pos)
            .or_else(|| crate::construction::spawn_obj(world, PAPER, pos))
        else {
            continue;
        };
        put_inside(world, paper, cabinet);
        super::paper::write_printed(world, paper, text);
        set_name(world, paper, &name);
    }
    if let Some(mut s) = world.get_mut::<FilingCabinet>(cabinet) {
        s.virgin = false;
        s.filed = filed;
    }
}

/// `get_rapsheet()` after its header.
fn rapsheet_text(record: &crate::records::manifest::CrewRecord) -> String {
    let mut text = format!(
        "Name: {}<br>Gender: {}<br>Age: {}<br>Species: {}<br>Fingerprint: {}<br>Wanted Status: \
         {}<br><br><center><B>Security Data</B></center><br><br>Crimes:<br>",
        record.name,
        record.gender,
        record.age,
        record.species,
        record.fingerprint,
        record.wanted_status
    );
    text.push_str(
        "<table style=\"text-align:center;\" border=\"1\" cellspacing=\"0\" width=\"100%\">\
         <tr><th>Crime</th><th>Details</th><th>Author</th><th>Time Added</th></tr>",
    );
    for crime in &record.crimes {
        if crime.valid {
            text.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                crime.name, crime.details, crime.author, crime.time
            ));
        } else {
            text.push_str(&"<td>--REDACTED--</td>".repeat(4));
            text.push_str("</tr>");
        }
    }
    text.push_str("</table><br><br>Citations:<br>");
    text.push_str(
        "<table style=\"text-align:center;\" border=\"1\" cellspacing=\"0\" width=\"100%\">\
         <tr><th>Citation</th><th>Details</th><th>Author</th><th>Time Added</th><th>Fine</th>\
         </tr><br>",
    );
    for citation in &record.citations {
        text.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            citation.name,
            citation.details,
            citation.author,
            citation.time,
            citation.fine.unwrap_or_default()
        ));
    }
    text.push_str("</table><br><br><center>Important Notes:</center><br>");
    if !record.security_note.is_empty() {
        text.push_str(&format!("- {}<br>", record.security_note));
    }
    text.push_str("- No further details.<br>");
    text
}

/// `filingcabinet/medical/populate`'s record text.
fn medical_text(record: &crate::records::manifest::CrewRecord) -> String {
    let notes: Vec<&str> = record
        .medical_notes
        .iter()
        .map(|n| n.content.as_str())
        .collect();
    format!(
        "<CENTER><B>Medical Record</B></CENTER><BR>Name: {} Rank: {}<BR>\nGender: {}<BR>\nAge: \
         {}<BR><BR>\n<CENTER><B>Medical Data</B></CENTER><BR>\nBlood Type: {}<BR>\nDNA: \
         {}<BR>\n<BR>\nPhysical Status: {}<BR>\nMental Status: {}<BR>\nMinor Disabilities: \
         {}<BR>\nDetails: {}<BR>\n<BR>\nMajor Disabilities: {}<BR>\nDetails: {}<BR>\n<BR>\n\
         Important Notes:<BR>\n\t{}<BR>\n<BR>\n<CENTER><B>Comments/Log</B></CENTER><BR></TT>",
        record.name,
        record.rank,
        record.gender,
        record.age,
        record.blood_type,
        record.dna_string,
        record.physical_status,
        record.mental_status,
        record.minor_disabilities,
        record.minor_disabilities_desc,
        record.major_disabilities,
        record.major_disabilities_desc,
        notes.join("<BR>\n\t")
    )
}

/// `employment_contract/Initialize`'s text.
pub fn contract_text(employee: &str) -> String {
    format!(
        "<center>Conditions of Employment</center><BR><BR><BR><BR>This Agreement is made and \
         entered into as of the date of last signature below, by and between {employee} \
         (hereafter referred to as SLAVE), and Nanotrasen (hereafter referred to as the \
         omnipresent and helpful watcher of humanity).<BR>WITNESSETH:<BR>WHEREAS, SLAVE is a \
         natural born human or humanoid, possessing skills upon which he can aid the omnipresent \
         and helpful watcher of humanity, who seeks employment in the omnipresent and helpful \
         watcher of humanity.<BR>WHEREAS, the omnipresent and helpful watcher of humanity agrees \
         to sporadically provide payment to SLAVE, in exchange for permanent servitude.<BR>NOW \
         THEREFORE in consideration of the mutual covenants herein contained, and other good and \
         valuable consideration, the parties hereto mutually agree as follows:<BR>In exchange \
         for paltry payments, SLAVE agrees to work for the omnipresent and helpful watcher of \
         humanity, for the remainder of his or her current and future lives.<BR>Further, SLAVE \
         agrees to transfer ownership of his or her soul to the loyalty department of the \
         omnipresent and helpful watcher of humanity.<BR>Should transfership of a soul not be \
         possible, a lien shall be placed instead.<BR>Signed,<BR><i>{employee}</i>"
    )
}

/// `filingcabinet/Initialize(mapload)`: the small items on a mapped cabinet's tile go inside.
pub(super) fn file_mapped_items(world: &mut World) {
    let mut cabinets =
        world.query_filtered::<(Entity, &TilePos, &DmObj), With<ss13_world::Mapped>>();
    let cabinets: Vec<(Entity, String, TilePos)> = cabinets
        .iter(world)
        .filter(|(_, _, o)| is_under(&o.path, CABINET))
        .map(|(e, p, o)| (e, o.path.clone(), *p))
        .collect();
    for (cabinet, path, pos) in cabinets {
        add_obj_parts(world, cabinet, &path);
        let items: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| world.get::<DmItem>(e).is_some() && w_class(world, e) < WEIGHT_NORMAL)
            .collect();
        for item in items {
            put_inside(world, item, cabinet);
        }
    }
}

/// `paperwork/item_interaction` (and the photocopy's VOID stamp).
fn stamp_paperwork(world: &mut World, client: Entity, stamp: Entity, work: Entity) {
    let Some(state) = world.get::<Paperwork>(work).cloned() else {
        return;
    };
    if state.stamped {
        return;
    }
    let path = item_path(world, stamp);
    let overlay = if state.photocopy && is_under(&path, VOID_STAMP) {
        let what = name_of(world, stamp);
        tell(
            world,
            client,
            Span::Notice,
            format!("You plant the {what} firmly onto the front of the documents."),
        );
        if let Some(mut w) = world.get_mut::<Paperwork>(work) {
            w.voided = true;
            w.stamped = true;
        }
        "paper_stamp-void"
    } else if is_under(&path, state.requested) {
        tell(
            world,
            client,
            Span::Notice,
            "You skim through the papers until you find a field reading 'STAMP HERE', and \
             complete the paperwork.",
        );
        if let Some(mut w) = world.get_mut::<Paperwork>(work) {
            w.stamped = true;
        }
        state.stamp_icon
    } else {
        let what = name_of(world, stamp);
        tell(
            world,
            client,
            Span::Warning,
            format!(
                "You hunt through the papers for somewhere to use {what}, but can't find anything."
            ),
        );
        return;
    };
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(BUREAUCRACY_DMI, overlay));
    if let Some(id) = id
        && let Some(mut look) = world.get_mut::<Appearance>(work)
    {
        look.overlays.push(Overlay::new(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paperwork_kinds_follow_their_heads() {
        let mut world = World::new();
        let cargo = paperwork_for(&mut world, "/obj/item/paperwork/cargo");
        assert_eq!(cargo.requested, "/obj/item/stamp/head/qm");
        assert_eq!(cargo.stamp_icon, "paper_stamp-qm");
        let copy = paperwork_for(&mut world, PHOTOCOPY);
        assert!(copy.photocopy);
        assert_eq!(copy.requested, VOID_STAMP);
        assert_eq!(copy.stamp_icon, "paper_stamp-pc");
        // No rng resource: the first kind.
        let ancient = paperwork_for(&mut world, ANCIENT);
        assert_eq!(ancient.requested, PAPERWORK_KINDS[0].1);
    }

    #[test]
    fn the_contract_names_its_employee() {
        let text = contract_text("Bob");
        assert!(text.contains("by and between Bob (hereafter"));
        assert!(text.ends_with("<i>Bob</i>"));
    }
}
