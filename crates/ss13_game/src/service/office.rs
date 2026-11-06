//! Clipboards, folders, paper planes, the photocopier and the fax machine (server), after tg
//! `code/modules/paperwork/clipboard.dm`, `folders.dm`, `paperplane.dm`, `photocopier.dm` and
//! `fax.dm`.
//!
//! - A clipboard holds papers (the last clipped on top) and one pen. A paper clips on, a pen
//!   slots in, anything else is used on the top paper. In hand it lists its papers (tgui
//!   `Clipboard` as a list): read, write with its pen, take out, move to the top; take the pen.
//! - A folder holds papers; in hand it lists them to read or take out.
//! - Alt-click folds a paper into a paper plane (in the hands when next to it); the plane in
//!   hand unfolds; a stamp stamps the paper inside, a pen refuses.
//! - The photocopier holds blank paper (30, the prebuilt one starts full with a toner
//!   cartridge), a toner cartridge and the paper to copy. An empty hand lists its actions (tgui
//!   `Photocopier` as a list): copy `num_copies` (1-10) copies, each `PAPER_TONER_USE /
//!   toner_efficiency` of toner and one sheet, one every `time_to_print`; take the original or
//!   the cartridge out. A blank paper or a paper bin's sheets refill it.
//! - The fax machine takes a paper, paper plane or canvas; an empty hand lists the station's
//!   faxes to send it to (and the Central Command network), or takes it out. The receiving fax
//!   says who sent it and drops it 1.9 s later.
// shortcut: tier-1 stock parts (30 sheets, efficiency 2, 4 s per copy) and one blank paper
// kind; no photo, document, blank-form or ass copies, payment, emag, toner colour or pixel
// shifting; folders cannot be renamed or cut apart; planes have no eye hits, carbon look or
// stamp overlays; the fax has no wires, jams, history, emag network or multitool renaming, and
// a fax to Central Command reaches the CentCom fax machines without the admins' request panel
// (admin group).

use bevy::prelude::*;
use ss13_mapload::{DmAtom, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptKind, ask_world};

use super::paper::{PAPER, Paper, PaperBin, Writing};
use super::server::{PendingAsk, TimerJob, after, client_of, item_path, service_data, sound_at};
use crate::chem::{map_pos, name_of};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::storage::{Stored, StoredIn, in_reach, put_in_hands, put_inside, take_out};
use crate::tools::tell;

const CLIPBOARD: &str = "/obj/item/clipboard";
const FOLDER: &str = "/obj/item/folder";
pub const PAPERPLANE: &str = "/obj/item/paperplane";
const PEN: &str = "/obj/item/pen";
const CANVAS: &str = "/obj/item/canvas";
pub const TONER: &str = "/obj/item/toner";
const COPIER: &str = "/obj/machinery/photocopier";
const FAX: &str = "/obj/machinery/fax";
const FAX_ADMIN: &str = "/obj/machinery/fax/admin";
const FAX_SYNDICATE: &str = "/obj/machinery/fax/admin/syndicate";
const FAX_AUTO_NAME: &str = "/obj/machinery/fax/auto_name";
/// `special_networks["nanotrasen"]`: `fax_id`, the default `GLOB.nt_fax_department`.
const CENTRAL_ID: &str = "central_command";
const CENTRAL_NAME: &str = "NT HR Department";
const SYNDICATE_NAME: &str = "Sabotage Department";
/// `MATTER_BIN_PAPER_CAPACITY` × tier 1, `1 + micro_laser tier`, `5 s - scanning tier`.
const PAPER_CAPACITY: u32 = 30;
const TONER_EFFICIENCY: f32 = 2.0;
const PRINT_SECS: f64 = 4.0;
/// `PAPER_TONER_USE`, `MAX_COPIES_AT_ONCE`.
const PAPER_TONER_USE: f32 = 0.125;
const MAX_COPIES: u32 = 10;
/// `/obj/machinery/fax/receive`: `addtimer(vend_item, 1.9 SECONDS)`.
const FAX_VEND_SECS: f64 = 1.9;
const PRINTER_SOUND: &str = "sound/machines/printer.ogg";
const FAX_SEND_SOUND: &str = "sound/machines/high_tech_confirm.ogg";
const EJECT_SOUND: &str = "sound/machines/eject.ogg";

/// `/obj/item/clipboard`: its pen and top paper (both inside it, with the other papers).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Clipboard {
    pub pen: Option<Entity>,
    pub top: Option<Entity>,
}

/// `/obj/item/folder`: holds papers.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Folder;

/// `/obj/item/paperplane`: the paper inside it is `internal_paper`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct PaperPlane;

/// `/obj/item/toner` `charges`, `max_charges`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Toner {
    pub charges: f32,
    pub max: f32,
}

/// `/obj/machinery/photocopier`: the blank sheets, the cartridge and the original inside it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Photocopier {
    pub paper: u32,
    pub toner: Option<Entity>,
    pub object: Option<Entity>,
    /// `num_copies`.
    pub copies: u32,
    /// `copies_left`: busy while above zero.
    pub queue: u32,
}

/// A prebuilt photocopier's `starting_toner`, made after map load: map objects initialize
/// while the map loader holds `Prototypes`, and a spawn then would find no prototype (and
/// cache that).
#[derive(Component, Clone, Copy, Debug, Default)]
pub(super) struct StartingToner;

/// Gives each prebuilt photocopier its cartridge.
pub(super) fn load_starting_toner(world: &mut World) {
    let mut copiers = world.query_filtered::<Entity, With<StartingToner>>();
    let copiers: Vec<Entity> = copiers.iter(world).collect();
    for copier in copiers {
        world.entity_mut(copier).remove::<StartingToner>();
        let Some(cartridge) = crate::construction::spawn_detached(world, TONER) else {
            continue;
        };
        put_inside(world, cartridge, copier);
        set_copier(world, copier, |c| c.toner = Some(cartridge));
    }
}

/// `/obj/machinery/fax`: `fax_id`, the loaded item, `visible_to_network`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Fax {
    pub id: String,
    pub loaded: Option<Entity>,
    pub visible: bool,
}

/// A list question of these machines and holders; the entries as they were asked.
#[derive(Clone, Debug)]
pub enum OfficeAsk {
    Holder {
        holder: Entity,
        acts: Vec<HolderAct>,
    },
    Copier {
        copier: Entity,
        acts: Vec<CopierAct>,
    },
    Copies {
        copier: Entity,
    },
    Fax {
        fax: Entity,
        acts: Vec<FaxAct>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HolderAct {
    Read(Entity),
    Write(Entity),
    Take(Entity),
    Top(Entity),
    RemovePen,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CopierAct {
    Copy,
    SetCopies,
    RemoveObject,
    RemoveToner,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FaxAct {
    Send(Entity),
    SendCentral,
    Remove,
}

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, CLIPBOARD) {
        world.entity_mut(e).insert(Clipboard::default());
    } else if is_under(path, FOLDER) {
        world.entity_mut(e).insert(Folder);
    } else if is_under(path, PAPERPLANE) {
        world.entity_mut(e).insert(PaperPlane);
    } else if is_under(path, TONER) {
        let charges = match path {
            "/obj/item/toner/large" => 25.0,
            "/obj/item/toner/extreme" => 200.0,
            "/obj/item/toner/infinite" => f32::INFINITY,
            _ => 5.0,
        };
        world.entity_mut(e).insert(Toner {
            charges,
            max: charges,
        });
    }
}

pub(super) fn add_obj_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, COPIER) {
        // `/prebuilt`: `starting_toner`, `starting_paper = 30`.
        let prebuilt = path.ends_with("/prebuilt");
        world.entity_mut(e).insert(Photocopier {
            paper: if prebuilt { PAPER_CAPACITY } else { 0 },
            toner: None,
            object: None,
            copies: 1,
            queue: 0,
        });
        if prebuilt {
            world.entity_mut(e).insert(StartingToner);
        }
    } else if is_under(path, FAX) {
        world.entity_mut(e).insert(Fax {
            id: if is_under(path, FAX_SYNDICATE) {
                "syndicate".to_owned()
            } else if is_under(path, FAX_ADMIN) {
                CENTRAL_ID.to_owned()
            } else {
                e.index().to_string()
            },
            loaded: None,
            visible: !is_under(path, FAX_ADMIN),
        });
    }
}

/// A clipboard or a folder: its papers are read and written in place.
pub(super) fn is_paper_holder(world: &World, e: Entity) -> bool {
    world.get::<Clipboard>(e).is_some() || world.get::<Folder>(e).is_some()
}

fn papers_in(world: &World, holder: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(holder)
        .map(|s| {
            s.items()
                .iter()
                .copied()
                .filter(|&p| world.get::<Paper>(p).is_some())
                .collect()
        })
        .unwrap_or_default()
}

fn inside(world: &World, item: Entity, container: Entity) -> bool {
    world
        .get::<StoredIn>(item)
        .is_some_and(|s| s.0 == container)
}

/// Held by `mob` or next to it.
fn at_hand(world: &World, mob: Entity, e: Entity) -> bool {
    world.get::<Inventory>(mob).is_some_and(|i| i.holds(e)) || in_reach(world, mob, e)
}

fn powered(world: &World, machine: Entity) -> bool {
    world.get::<crate::power::Unpowered>(machine).is_none()
}

fn to_hands(world: &mut World, mob: Entity, item: Entity) {
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        put_in_hands(world, mob, &inventory, item);
    } else if let Some(pos) = map_pos(world, mob) {
        take_out(world, item, pos);
    }
}

fn icon_of(world: &World, e: Entity) -> String {
    world
        .get::<DmAtom>(e)
        .map(|a| a.icon.clone())
        .unwrap_or_default()
}

fn set_overlays(world: &mut World, e: Entity, wanted: Vec<Overlay>) {
    if let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.overlays != wanted
    {
        look.overlays = wanted;
    }
}

/// `clipboard/Exited` + `update_overlays`: the top paper (the newest one left when it went),
/// `clipboard_pen`, `clipboard_over`.
fn refresh_clipboard(world: &mut World, board: Entity) {
    let Some(mut state) = world.get::<Clipboard>(board).copied() else {
        return;
    };
    let papers = papers_in(world, board);
    if state.top.is_none_or(|t| !papers.contains(&t)) {
        state.top = papers.last().copied();
    }
    if state.pen.is_some_and(|p| !inside(world, p, board)) {
        state.pen = None;
    }
    world.entity_mut(board).insert(state);
    let icon = icon_of(world, board);
    let mut wanted = Vec::new();
    if let Some(look) = state.top.and_then(|t| world.get::<Appearance>(t)) {
        wanted.push(Overlay::new(look.icon_state));
    }
    let index = world.get_resource::<IconIndex>();
    let states = [state.pen.map(|_| "clipboard_pen"), Some("clipboard_over")];
    wanted.extend(
        states
            .into_iter()
            .flatten()
            .filter_map(|s| index.and_then(|i| i.get(&icon, s)))
            .map(Overlay::new),
    );
    set_overlays(world, board, wanted);
}

/// `folder/update_overlays`: `folder_paper` while it holds anything (a biscuit card's own
/// overlay, none while sealed).
pub(super) fn refresh_folder(world: &mut World, folder: Entity) {
    let full = world
        .get::<Stored>(folder)
        .is_some_and(|s| !s.items().is_empty());
    let icon = icon_of(world, folder);
    let state = super::stationery::folder_overlay(world, folder);
    let wanted: Vec<Overlay> = world
        .get_resource::<IconIndex>()
        .zip(state)
        .and_then(|(i, state)| i.get(&icon, state))
        .filter(|_| full)
        .map(Overlay::new)
        .into_iter()
        .collect();
    set_overlays(world, folder, wanted);
}

fn refresh_holder(world: &mut World, holder: Entity) {
    if world.get::<Clipboard>(holder).is_some() {
        refresh_clipboard(world, holder);
    } else {
        refresh_folder(world, holder);
    }
}

/// `clipboard/item_interaction` with a paper, `folder/interact_with_insertables`.
fn insert_paper(world: &mut World, client: Entity, paper: Entity, holder: Entity) {
    put_inside(world, paper, holder);
    let clipped = match world.get_mut::<Clipboard>(holder) {
        Some(mut board) => {
            board.top = Some(paper);
            true
        }
        None => false,
    };
    if clipped {
        let (what, onto) = (name_of(world, paper), name_of(world, holder));
        tell(
            world,
            client,
            Span::Notice,
            format!("You clip {what} onto {onto}."),
        );
    }
    refresh_holder(world, holder);
}

/// tg `click_alt` of a paper -> `make_plane`. True when `paper` is a paper the mob holds or is
/// next to.
pub(super) fn fold_plane(world: &mut World, client: Entity, mob: Entity, paper: Entity) -> bool {
    if world.get::<Paper>(paper).is_none() {
        return false;
    }
    let held = world.get::<Inventory>(mob).is_some_and(|i| i.holds(paper));
    let pos = if held {
        map_pos(world, mob)
    } else {
        world.get::<TilePos>(paper).copied()
    };
    let Some(pos) = pos else {
        return false;
    };
    let Some(plane) = crate::construction::spawn_obj(world, PAPERPLANE, pos) else {
        return true;
    };
    put_inside(world, paper, plane);
    world.entity_mut(plane).insert(PaperPlane);
    if held || in_reach(world, mob, plane) {
        to_hands(world, mob, plane);
    }
    tell(world, client, Span::Notice, "folded into a plane");
    true
}

/// `paperplane/attack_self`: the paper back into the hands, the plane gone.
fn unfold(world: &mut World, client: Entity, mob: Entity, plane: Entity) {
    let Some(pos) = map_pos(world, plane) else {
        return;
    };
    let paper = papers_in(world, plane).first().copied();
    let paper = match paper {
        Some(p) => {
            take_out(world, p, pos);
            Some(p)
        }
        // A mapped plane makes its paper (`internal_paper = new(src)`).
        None => crate::construction::spawn_obj(world, PAPER, pos),
    };
    world.despawn(plane);
    if let Some(paper) = paper {
        to_hands(world, mob, paper);
    }
    tell(world, client, Span::Notice, "unfolded");
}

/// `attack_self` of clipboards, folders and planes.
pub(super) fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<PaperPlane>(item).is_some() {
        unfold(world, client, mob, item);
        return true;
    }
    if !is_paper_holder(world, item) {
        return false;
    }
    crate::forensics::server::add_fingerprint(world, item, mob, false);
    holder_menu(world, client, mob, item);
    true
}

/// tgui `Clipboard` / `Folder` as a list: per paper (the top one first) read, write with the
/// clipboard's pen, take out, move to the top; take the pen out.
fn holder_menu(world: &mut World, client: Entity, mob: Entity, holder: Entity) {
    let board = world.get::<Clipboard>(holder).copied();
    let mut papers = papers_in(world, holder);
    papers.reverse();
    if let Some(top) = board.and_then(|b| b.top)
        && let Some(i) = papers.iter().position(|&p| p == top)
    {
        papers.remove(i);
        papers.insert(0, top);
    }
    let pen = board.and_then(|b| b.pen);
    let mut acts = Vec::new();
    for (i, &paper) in papers.iter().enumerate() {
        acts.push(HolderAct::Read(paper));
        if pen.is_some() {
            acts.push(HolderAct::Write(paper));
        }
        acts.push(HolderAct::Take(paper));
        if board.is_some() && i > 0 {
            acts.push(HolderAct::Top(paper));
        }
    }
    if pen.is_some() {
        acts.push(HolderAct::RemovePen);
    }
    let name = name_of(world, holder);
    if acts.is_empty() {
        tell(world, client, Span::Notice, format!("{name} is empty."));
        return;
    }
    let items = acts
        .iter()
        .map(|act| match *act {
            HolderAct::Read(p) => format!("Read: {}", name_of(world, p)),
            HolderAct::Write(p) => format!("Write on: {}", name_of(world, p)),
            HolderAct::Take(p) => format!("Take out: {}", name_of(world, p)),
            HolderAct::Top(p) => format!("Move to the top: {}", name_of(world, p)),
            HolderAct::RemovePen => {
                format!(
                    "Take out: {}",
                    pen.map(|p| name_of(world, p)).unwrap_or_default()
                )
            }
        })
        .collect();
    ask_list(
        world,
        client,
        mob,
        &name,
        items,
        OfficeAsk::Holder { holder, acts },
    );
}

fn ask_list(
    world: &mut World,
    client: Entity,
    mob: Entity,
    title: &str,
    items: Vec<String>,
    ask: OfficeAsk,
) {
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(PendingAsk::Office(ask));
}

/// tg `ui_act` of the holders and machines, checked again: the actor next to them (or holding
/// them), the things still inside, the machine powered and not busy.
pub(super) fn answer(world: &mut World, actor: Entity, ask: OfficeAsk, answer: PromptAnswer) {
    let Some(client) = client_of(world, actor) else {
        return;
    };
    // `IS_UNCONSCIOUS_OR_CRIT(usr) || HAS_TRAIT(usr, TRAIT_HANDS_BLOCKED)`.
    if crate::status_effects::incapacitation(world, actor).hands_blocked {
        return;
    }
    match (ask, answer) {
        (OfficeAsk::Holder { holder, acts }, PromptAnswer::Item(i)) => {
            if let Some(&act) = acts.get(i)
                && at_hand(world, actor, holder)
            {
                holder_act(world, client, actor, holder, act);
            }
        }
        (OfficeAsk::Copier { copier, acts }, PromptAnswer::Item(i)) => {
            if let Some(&act) = acts.get(i)
                && in_reach(world, actor, copier)
                && powered(world, copier)
            {
                copier_act(world, client, actor, copier, act);
            }
        }
        (OfficeAsk::Copies { copier }, PromptAnswer::Number(n)) => {
            if in_reach(world, actor, copier)
                && let Some(mut state) = world.get_mut::<Photocopier>(copier)
            {
                state.copies = (n.round().max(1.0) as u32).min(MAX_COPIES);
            }
        }
        (OfficeAsk::Fax { fax, acts }, PromptAnswer::Item(i)) => {
            if let Some(&act) = acts.get(i)
                && in_reach(world, actor, fax)
                && powered(world, fax)
            {
                fax_act(world, client, actor, fax, act);
            }
        }
        _ => {}
    }
}

fn holder_act(world: &mut World, client: Entity, actor: Entity, holder: Entity, act: HolderAct) {
    let holder_name = name_of(world, holder);
    match act {
        HolderAct::Read(paper) if inside(world, paper, holder) => {
            super::paper::read(world, client, actor, paper);
        }
        HolderAct::Write(paper) if inside(world, paper, holder) => {
            let Some(pen) = world.get::<Clipboard>(holder).and_then(|b| b.pen) else {
                return;
            };
            if crate::vision::is_blind(world, actor) {
                tell(world, client, Span::Warning, "You can't see to write!");
                return;
            }
            super::paper::start_writing(world, client, actor, paper, pen);
        }
        HolderAct::Take(paper) if inside(world, paper, holder) => {
            to_hands(world, actor, paper);
            let what = name_of(world, paper);
            tell(
                world,
                client,
                Span::Notice,
                format!("You remove {what} from {holder_name}."),
            );
            refresh_holder(world, holder);
        }
        HolderAct::Top(paper) if inside(world, paper, holder) => {
            if let Some(mut board) = world.get_mut::<Clipboard>(holder) {
                board.top = Some(paper);
            }
            let what = name_of(world, paper);
            tell(
                world,
                client,
                Span::Notice,
                format!("You move {what} to the top."),
            );
            refresh_clipboard(world, holder);
        }
        HolderAct::RemovePen => {
            let Some(pen) = world
                .get::<Clipboard>(holder)
                .and_then(|b| b.pen)
                .filter(|&p| inside(world, p, holder))
            else {
                return;
            };
            to_hands(world, actor, pen);
            let what = name_of(world, pen);
            tell(
                world,
                client,
                Span::Notice,
                format!("You remove {what} from {holder_name}."),
            );
            refresh_clipboard(world, holder);
        }
        _ => {}
    }
}

/// `item_interaction` of clipboards, folders, planes, the photocopier and the fax, also a
/// clipboard or folder clicked on a paper (`paper/item_interaction`). True when it was theirs.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let held_paper = world.get::<Paper>(held).is_some();
    if is_paper_holder(world, held) && world.get::<Paper>(target).is_some() {
        insert_paper(world, client, target, held);
        return true;
    }
    if let Some(board) = world.get::<Clipboard>(target).copied() {
        if held_paper {
            insert_paper(world, client, held, target);
            return true;
        }
        if board.pen.is_none() && is_under(&item_path(world, held), PEN) {
            put_inside(world, held, target);
            if let Some(mut state) = world.get_mut::<Clipboard>(target) {
                state.pen = Some(held);
            }
            let (what, into) = (name_of(world, held), name_of(world, target));
            tell(
                world,
                client,
                Span::Notice,
                format!("You slot {what} into {into}."),
            );
            refresh_clipboard(world, target);
            return true;
        }
        let Some(top) = board.top.filter(|&t| inside(world, t, target)) else {
            return false;
        };
        super::paper::item_interaction(world, client, mob, held, top);
        refresh_clipboard(world, target);
        return true;
    }
    if world.get::<Folder>(target).is_some() && held_paper {
        insert_paper(world, client, held, target);
        return true;
    }
    if world.get::<PaperPlane>(target).is_some() {
        return match super::paper::writing(world, held) {
            Some(Writing::Pen(_)) => {
                let what = name_of(world, target);
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("You should unfold {what} before changing it!"),
                );
                true
            }
            Some(Writing::Stamp { state, icon }) => {
                if let Some(paper) = papers_in(world, target).first().copied() {
                    super::paper::stamp(world, client, mob, held, paper, &state, &icon);
                }
                true
            }
            None => false,
        };
    }
    if world.get::<Photocopier>(target).is_some() {
        return copier_insert(world, client, mob, held, target);
    }
    if world.get::<Fax>(target).is_some() {
        return fax_insert(world, client, mob, held, target);
    }
    false
}

/// `attack_hand` -> `ui_interact` of the photocopier and the fax.
pub(super) fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Photocopier>(target).is_some() {
        if powered(world, target) {
            copier_menu(world, client, mob, target);
        }
        return true;
    }
    if world.get::<Fax>(target).is_some() {
        if powered(world, target) {
            fax_menu(world, client, mob, target);
        }
        return true;
    }
    false
}

/// `photocopier/item_interaction`.
fn copier_insert(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    copier: Entity,
) -> bool {
    let Some(state) = world.get::<Photocopier>(copier).copied() else {
        return false;
    };
    if world.get::<Toner>(held).is_some() {
        if state.toner.is_some() {
            tell(world, client, Span::Warning, "another cartridge inside!");
            return true;
        }
        put_inside(world, held, copier);
        set_copier(world, copier, |c| c.toner = Some(held));
        tell(world, client, Span::Notice, "cartridge inserted");
        return true;
    }
    if world.get::<PaperPlane>(held).is_some() {
        tell(world, client, Span::Warning, "flatten paper first!");
        return true;
    }
    if let Some(bin) = world.get::<PaperBin>(held).cloned() {
        copier_take_bin(world, client, copier, held, bin);
        return true;
    }
    let Some(paper) = world.get::<Paper>(held) else {
        return false;
    };
    if paper.is_empty() {
        if state.paper >= PAPER_CAPACITY {
            tell(world, client, Span::Warning, "cannot hold more paper!");
            return true;
        }
        world.despawn(held);
        set_copier(world, copier, |c| c.paper += 1);
        tell(world, client, Span::Notice, "paper inserted");
        return true;
    }
    // `insert_copy_object`.
    if state.object.is_some_and(|o| inside(world, o, copier)) {
        tell(world, client, Span::Warning, "scanner tray occupied!");
        return true;
    }
    put_inside(world, held, copier);
    set_copier(world, copier, |c| c.object = Some(held));
    crate::forensics::server::add_fingerprint(world, copier, mob, false);
    tell(world, client, Span::Notice, "copy object inserted");
    true
}

/// A paper bin's blank sheets into the copier, up to its capacity.
fn copier_take_bin(
    world: &mut World,
    client: Entity,
    copier: Entity,
    bin: Entity,
    state: PaperBin,
) {
    if state.total == 0 {
        tell(world, client, Span::Warning, "paper bin empty!");
        return;
    }
    let room =
        PAPER_CAPACITY.saturating_sub(world.get::<Photocopier>(copier).map_or(0, |c| c.paper));
    let mut taken = 0;
    let mut stack = Vec::new();
    for paper in state.stack.iter().copied() {
        let blank = world.get::<Paper>(paper).is_none_or(Paper::is_empty);
        if blank && taken < room {
            world.despawn(paper);
            taken += 1;
        } else {
            stack.push(paper);
        }
    }
    // The sheets never taken out are blank.
    let fresh = state.total.saturating_sub(state.stack.len() as u32);
    let more = fresh.min(room - taken);
    taken += more;
    if taken == 0 {
        tell(world, client, Span::Warning, "cannot hold more paper!");
        return;
    }
    if let Some(mut bin_state) = world.get_mut::<PaperBin>(bin) {
        bin_state.total -= taken;
        bin_state.stack = stack;
    }
    super::paper::refresh_bin_look(world, bin);
    set_copier(world, copier, |c| c.paper += taken);
    tell(
        world,
        client,
        Span::Notice,
        format!("{taken} paper inserted"),
    );
}

fn set_copier(world: &mut World, copier: Entity, change: impl FnOnce(&mut Photocopier)) {
    if let Some(mut state) = world.get_mut::<Photocopier>(copier) {
        change(&mut state);
    }
}

fn copier_menu(world: &mut World, client: Entity, mob: Entity, copier: Entity) {
    let Some(state) = world.get::<Photocopier>(copier).copied() else {
        return;
    };
    let toner =
        state
            .toner
            .and_then(|t| world.get::<Toner>(t))
            .map_or("no cartridge".to_owned(), |t| {
                if t.charges.is_finite() {
                    format!("toner {:.0}%", t.charges / t.max * 100.0)
                } else {
                    "toner 100%".to_owned()
                }
            });
    let name = name_of(world, copier);
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "{name}: {} sheets, {toner}{}.",
            state.paper,
            if state.queue > 0 {
                format!(", {} copies left", state.queue)
            } else {
                String::new()
            }
        ),
    );
    let mut acts = Vec::new();
    if state.object.is_some() {
        acts.push(CopierAct::Copy);
    }
    acts.push(CopierAct::SetCopies);
    if state.object.is_some() {
        acts.push(CopierAct::RemoveObject);
    }
    if state.toner.is_some() {
        acts.push(CopierAct::RemoveToner);
    }
    let object = state.object.map(|o| name_of(world, o)).unwrap_or_default();
    let items = acts
        .iter()
        .map(|act| match act {
            CopierAct::Copy => format!("Copy ({})", state.copies),
            CopierAct::SetCopies => "Set the number of copies".to_owned(),
            CopierAct::RemoveObject => format!("Take out: {object}"),
            CopierAct::RemoveToner => "Take out the toner cartridge".to_owned(),
        })
        .collect();
    ask_list(
        world,
        client,
        mob,
        &name,
        items,
        OfficeAsk::Copier { copier, acts },
    );
}

fn copier_act(world: &mut World, client: Entity, actor: Entity, copier: Entity, act: CopierAct) {
    let Some(state) = world.get::<Photocopier>(copier).copied() else {
        return;
    };
    // `check_busy`.
    if state.queue > 0 && act != CopierAct::SetCopies {
        tell(world, client, Span::Warning, "printer is busy!");
        return;
    }
    match act {
        CopierAct::Copy => start_copies(world, client, copier, state),
        CopierAct::SetCopies => {
            let name = name_of(world, copier);
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    actor,
                    name,
                    "How many copies?",
                    PromptKind::Number {
                        default: f64::from(state.copies),
                        min: 1.0,
                        max: f64::from(MAX_COPIES),
                        round: true,
                    },
                ),
            );
            world
                .entity_mut(prompt)
                .insert(PendingAsk::Office(OfficeAsk::Copies { copier }));
        }
        CopierAct::RemoveObject => {
            let Some(object) = state.object.filter(|&o| inside(world, o, copier)) else {
                return;
            };
            to_hands(world, actor, object);
            set_copier(world, copier, |c| c.object = None);
            let (what, from) = (name_of(world, object), name_of(world, copier));
            tell(
                world,
                client,
                Span::Notice,
                format!("You take {what} out of {from}."),
            );
        }
        CopierAct::RemoveToner => {
            let Some(toner) = state.toner.filter(|&t| inside(world, t, copier)) else {
                return;
            };
            to_hands(world, actor, toner);
            set_copier(world, copier, |c| c.toner = None);
        }
    }
}

/// `do_copy_loop`'s checks: as many copies as the toner and paper allow, the first one
/// `time_to_print` away.
fn start_copies(world: &mut World, client: Entity, copier: Entity, state: Photocopier) {
    if !state.object.is_some_and(|o| inside(world, o, copier)) {
        return;
    }
    let name = name_of(world, copier);
    let flash = |text: &str| format!("An error message flashes across {name}'s screen: \"{text}\"");
    let per_copy = PAPER_TONER_USE / TONER_EFFICIENCY;
    let mut amount = state.copies;
    let mut error = None;
    match state.toner.and_then(|t| world.get::<Toner>(t)) {
        None => {
            amount = 0;
            error = Some(flash("No toner cartridge found. Aborting."));
        }
        Some(toner) if toner.charges < per_copy * amount as f32 => {
            amount = (toner.charges / per_copy).floor() as u32;
            let full = if amount >= 1 { "full " } else { "" };
            error = Some(flash(&format!(
                "Not enough toner to perform {full}operation."
            )));
        }
        Some(_) => {}
    }
    if state.paper < amount {
        amount = state.paper;
        let full = if amount >= 1 { "full " } else { "" };
        error = Some(flash(&format!(
            "Not enough paper to perform {full}operation."
        )));
    }
    if let Some(error) = error {
        tell(world, client, Span::Warning, error);
    }
    if amount == 0 {
        return;
    }
    set_copier(world, copier, |c| c.queue = amount);
    sound_at(world, copier, PRINTER_SOUND, 50.0, false);
    after(world, PRINT_SECS, TimerJob::Copy { copier });
}

/// One copy of the original onto the copier's tile; the next one is queued. The loop stops
/// without power, the cartridge, the original, paper or toner (`make_paper_copy` -> null).
pub(super) fn next_copy(world: &mut World, copier: Entity) {
    let Some(state) = world.get::<Photocopier>(copier).copied() else {
        return;
    };
    if state.queue == 0 {
        return;
    }
    let per_copy = PAPER_TONER_USE / TONER_EFFICIENCY;
    let original = state.object.filter(|&o| inside(world, o, copier));
    let toner = state
        .toner
        .filter(|&t| world.get::<Toner>(t).is_some_and(|t| t.charges >= per_copy));
    let pos = world.get::<TilePos>(copier).copied();
    let (Some(original), Some(toner), Some(pos), true, true) = (
        original,
        toner,
        pos,
        powered(world, copier),
        state.paper > 0,
    ) else {
        set_copier(world, copier, |c| c.queue = 0);
        return;
    };
    if let Some(copy) = crate::construction::spawn_obj(world, PAPER, pos) {
        super::paper::copy_paper(world, original, copy);
        let mut rng = world.resource_mut::<crate::combat::CombatRng>();
        let offset = [0, 0].map(|_| rng.range(0, 20) as i16 - 10);
        if let Some(mut look) = world.get_mut::<Appearance>(copy) {
            look.pixel_offset = offset;
        }
    }
    if let Some(mut cartridge) = world.get_mut::<Toner>(toner) {
        cartridge.charges -= per_copy;
    }
    set_copier(world, copier, |c| {
        c.paper -= 1;
        c.queue -= 1;
    });
    if world
        .get::<Photocopier>(copier)
        .is_some_and(|c| c.queue > 0)
    {
        sound_at(world, copier, PRINTER_SOUND, 50.0, false);
        after(world, PRINT_SECS, TimerJob::Copy { copier });
    }
}

/// `fax/item_interaction`: a paper, plane or canvas loads (`allowed_types`).
fn fax_insert(world: &mut World, client: Entity, mob: Entity, held: Entity, fax: Entity) -> bool {
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    let path = item_path(world, held);
    let allowed = world.get::<Paper>(held).is_some()
        || world.get::<PaperPlane>(held).is_some()
        || is_under(&path, CANVAS);
    if !allowed {
        return false;
    }
    let loaded = world
        .get::<Fax>(fax)
        .and_then(|f| f.loaded)
        .filter(|&l| inside(world, l, fax));
    if loaded.is_some() {
        tell(world, client, Span::Warning, "item already loaded!");
        return true;
    }
    put_inside(world, held, fax);
    if let Some(mut state) = world.get_mut::<Fax>(fax) {
        state.loaded = Some(held);
    }
    true
}

/// `fax_name`: the mapped one, the area's for `auto_name`, the networks' for the admin faxes,
/// else `Unregistered fax <id>`.
pub fn fax_name(world: &World, fax: Entity) -> String {
    let named = service_data(world, fax).title;
    if !named.is_empty() {
        return named;
    }
    let path = item_path(world, fax);
    if is_under(&path, FAX_SYNDICATE) {
        return SYNDICATE_NAME.to_owned();
    }
    if is_under(&path, FAX_ADMIN) {
        return CENTRAL_NAME.to_owned();
    }
    if is_under(&path, FAX_AUTO_NAME)
        && let Some(&pos) = world.get::<TilePos>(fax)
    {
        return crate::bots::area_name(world, pos);
    }
    let id = world
        .get::<Fax>(fax)
        .map(|f| f.id.clone())
        .unwrap_or_default();
    format!("Unregistered fax {id}")
}

/// The other faxes on the station network, by name.
fn destinations(world: &mut World, fax: Entity) -> Vec<(String, Entity)> {
    let mut faxes = world.query::<(Entity, &Fax)>();
    let others: Vec<Entity> = faxes
        .iter(world)
        .filter(|(e, f)| *e != fax && f.visible)
        .map(|(e, _)| e)
        .collect();
    let mut out: Vec<(String, Entity)> = others
        .into_iter()
        .map(|e| (fax_name(world, e), e))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn fax_menu(world: &mut World, client: Entity, mob: Entity, fax: Entity) {
    let name = name_of(world, fax);
    let Some(loaded) = world
        .get::<Fax>(fax)
        .and_then(|f| f.loaded)
        .filter(|&l| inside(world, l, fax))
    else {
        tell(
            world,
            client,
            Span::Notice,
            format!("{name}: no paper loaded."),
        );
        return;
    };
    let mut acts = Vec::new();
    let mut items = Vec::new();
    for (dest, e) in destinations(world, fax) {
        acts.push(FaxAct::Send(e));
        items.push(format!("Send to {dest}"));
    }
    acts.push(FaxAct::SendCentral);
    items.push(format!("Send to {CENTRAL_NAME}"));
    acts.push(FaxAct::Remove);
    items.push(format!("Take out: {}", name_of(world, loaded)));
    ask_list(
        world,
        client,
        mob,
        &name,
        items,
        OfficeAsk::Fax { fax, acts },
    );
}

fn fax_act(world: &mut World, client: Entity, actor: Entity, fax: Entity, act: FaxAct) {
    let Some(loaded) = world
        .get::<Fax>(fax)
        .and_then(|f| f.loaded)
        .filter(|&l| inside(world, l, fax))
    else {
        return;
    };
    match act {
        FaxAct::Remove => {
            to_hands(world, actor, loaded);
            sound_at(world, fax, EJECT_SOUND, 50.0, false);
        }
        FaxAct::Send(dest) => {
            if world.get::<Fax>(dest).is_none_or(|f| !f.visible) {
                return;
            }
            let sender = fax_name(world, fax);
            receive(world, dest, loaded, &sender);
            sound_at(world, fax, FAX_SEND_SOUND, 50.0, false);
        }
        FaxAct::SendCentral => {
            // `send_special`: paper only on the protected network.
            if world.get::<Paper>(loaded).is_none() {
                let name = name_of(world, fax);
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!(
                        "{name}: Fax cannot send all above paper on this protected network, sorry."
                    ),
                );
                return;
            }
            let sender = fax_name(world, fax);
            sound_at(world, fax, FAX_SEND_SOUND, 50.0, false);
            send_central(world, loaded, &sender);
        }
    }
    if let Some(mut state) = world.get_mut::<Fax>(fax)
        && state.loaded == Some(loaded)
    {
        state.loaded = None;
    }
}

/// `GLOB.fax_autoprinting` (BandaStation): the original to the first CentCom fax, copies to the
/// others; without one the paper leaves the station all the same.
fn send_central(world: &mut World, paper: Entity, sender: &str) {
    let mut faxes = world.query::<(Entity, &Fax)>();
    let central: Vec<Entity> = faxes
        .iter(world)
        .filter(|(_, f)| f.id == CENTRAL_ID)
        .map(|(e, _)| e)
        .collect();
    let Some((&first, rest)) = central.split_first() else {
        world.despawn(paper);
        return;
    };
    for &fax in rest {
        let Some(pos) = world.get::<TilePos>(fax).copied() else {
            continue;
        };
        if let Some(copy) = crate::construction::spawn_obj(world, PAPER, pos) {
            super::paper::copy_paper(world, paper, copy);
            receive(world, fax, copy, sender);
        }
    }
    receive(world, first, paper, sender);
}

/// `fax/receive`: inside the fax at once, out after the animation.
fn receive(world: &mut World, fax: Entity, item: Entity, sender: &str) {
    put_inside(world, item, fax);
    sound_at(world, fax, PRINTER_SOUND, 50.0, false);
    crate::atmos_machinery::machine_say(
        world,
        fax,
        &format!("Received correspondence from {sender}."),
    );
    after(world, FAX_VEND_SECS, TimerJob::FaxVend { fax, item });
}

/// `vend_item`: onto the fax's tile.
pub(super) fn vend(world: &mut World, fax: Entity, item: Entity) {
    if !inside(world, item, fax) {
        return;
    }
    if let Some(pos) = world.get::<TilePos>(fax).copied() {
        take_out(world, item, pos);
    }
}
