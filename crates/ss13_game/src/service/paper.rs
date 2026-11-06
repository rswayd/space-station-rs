//! Paper, pens, stamps, paper bins and the hand labeler (server), after tg
//! `code/modules/paperwork/paper.dm`, `pen.dm`, `stamps.dm`, `paperbin.dm`, `handlabeler.dm`.
//!
//! - Every `/obj/item/paper` is a [`Paper`]: its `default_raw_text` (`ss13_mapload::DmService`)
//!   is its first input. Using it in hand opens the `PaperSheet` window; a pen or crayon on it
//!   asks for the text (tgui's edit area is a multiline prompt here) up to `MAX_PAPER_LENGTH`
//!   in total, written in the tool's font and colour; a stamp stamps it (at most
//!   `MAX_PAPER_STAMPS`, the first five as overlays). A written paper shows `paper_words`.
//! - A paper with empty `[input_field]` keys first asks where to write (tgui's field buttons):
//!   the end, or a field, which then holds the text as a child input
//!   (`add_raw_text_to_input_field`).
//! - A paper bin holds 30 sheets: an empty hand takes the top one (a sheet put in first, else a
//!   new one); a paper goes in.
//! - The hand labeler toggles in hand (asking for the label when it turns on) and, on,
//!   appends ` (label)` to what it is used on, 30 times per roll; a refill restores the roll.
// shortcut: no field autofill, signatures or BandaStation `[name]` keys, no stamp placement
// (stamps land at once), no burning, renaming, bin pens or noticeboards; labels
// are not separate items that peel off.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmName};
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptKind, ask_world};
use ss13_tgui::server::AlwaysState;

use super::PaperInput;
use super::server::{
    PendingAsk, client_of, item_path, refresh_windows, say_around, service_data, set_icon_state,
    sound_at,
};
use crate::health::Health;
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::tools::tell;

pub const PAPER: &str = ss13_mapload::service::PAPER_ROOT;
const PEN: &str = "/obj/item/pen";
const CRAYON: &str = "/obj/item/toy/crayon";
const STAMP: &str = "/obj/item/stamp";
const PAPER_BIN: &str = "/obj/item/paper_bin";
const LABELER: &str = "/obj/item/hand_labeler";
const LABELER_REFILL: &str = "/obj/item/hand_labeler_refill";
const BUREAUCRACY_DMI: &str = "icons/obj/service/bureaucracy.dmi";

/// `MAX_PAPER_LENGTH`, `MAX_PAPER_STAMPS`, `MAX_PAPER_STAMPS_OVERLAYS`.
pub const MAX_PAPER_LENGTH: usize = 5000;
pub const MAX_PAPER_STAMPS: usize = 30;
const MAX_PAPER_STAMPS_OVERLAYS: usize = 4;
/// `PEN_FONT`, `FOUNTAIN_PEN_FONT`, `CRAYON_FONT`, `CHARCOAL_FONT`.
const PEN_FONT: &str = "Verdana";
const FOUNTAIN_PEN_FONT: &str = "Segoe Script";
const CRAYON_FONT: &str = "Comic Sans MS";
const CHARCOAL_FONT: &str = "Candara";
/// `/obj/item/paper_bin` `total_paper`.
const BIN_PAPERS: u32 = 30;
/// `/obj/item/hand_labeler` `labels_left`, `MAX_NAME_LEN`, `MAX_LABEL_LEN`.
const LABELS: u32 = 30;
const MAX_NAME_LEN: u32 = 42;
const MAX_LABEL_LEN: usize = 64;
const STAMP_SOUND: &str = "sound/items/handling/standard_stamp.ogg";
const LABEL_SOUND: &str = "sound/items/handling/component_pickup.ogg";

/// `raw_text_inputs` and the stamps (`raw_stamp_data`, by stamp name) of a paper.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Paper {
    pub inputs: Vec<PaperInput>,
    pub stamps: Vec<String>,
    /// `stamp_cache` length: the stamps drawn as overlays.
    overlays: usize,
}

impl Paper {
    /// `get_total_length`.
    pub fn total_length(&self) -> usize {
        self.inputs.iter().map(PaperInput::text_length).sum()
    }

    /// `is_empty`: no text and no stamps.
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty() && self.stamps.is_empty()
    }

    /// The empty input fields, as `(input index, field id)` (tgui's write buttons).
    pub fn open_fields(&self) -> Vec<(usize, usize)> {
        self.inputs
            .iter()
            .enumerate()
            .flat_map(|(i, input)| (1..=input.fields().len()).map(move |id| (i, id)))
            .collect()
    }

    /// `set_content_using_paper`: every input as raw HTML (`to_raw_html`), a blank line between.
    pub fn raw_html(&self) -> String {
        let mut out = String::new();
        for input in &self.inputs {
            out.push_str(&input_html(input));
            out.push_str(if input.raw_text.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            });
        }
        out
    }
}

/// `/datum/paper_input/to_raw_html`.
fn input_html(input: &PaperInput) -> String {
    let mut text = input.raw_text.clone();
    for (i, child) in input.children.iter().enumerate() {
        text = text.replacen(&format!("[child_{}]", i + 1), &input_html(child), 1);
    }
    if !input.font.is_empty() || !input.color.is_empty() {
        text = format!(
            "<font color='{}' face='{}'>{text}</font>",
            input.color, input.font
        );
    }
    if input.bold {
        text = format!("<b>{text}</b>");
    }
    text
}

/// `/obj/item/pen/fourcolor` `colour` after its `attack_self` cycles.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FourColor(u8);

const FOUR_COLORS: [(&str, &str); 4] = [
    ("#000000", "black"),
    ("#FF0000", "red"),
    ("#00FF00", "green"),
    ("#0000FF", "blue"),
];

/// A paper bin's sheets (`total_paper`) and the papers put into it (`paper_stack`, inside it).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct PaperBin {
    pub total: u32,
    pub stack: Vec<Entity>,
}

/// `/obj/item/hand_labeler` state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Labeler {
    pub label: String,
    pub labels_left: u32,
    pub on: bool,
}

/// `get_writing_implement_details`.
pub(super) enum Writing {
    Pen(PaperInput),
    Stamp { state: String, icon: String },
}

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, PAPER) {
        // A printer may have written on it before this deferred `Initialize` ran.
        if world.get::<Paper>(e).is_some() {
            return;
        }
        let text = service_data(world, e).text;
        let mut paper = Paper::default();
        if !text.is_empty() {
            paper.inputs.push(input(text, PEN_FONT, "#000000", false));
        }
        world.entity_mut(e).insert((paper, AlwaysState));
        refresh_paper_look(world, e);
    } else if is_under(path, PAPER_BIN) {
        world.entity_mut(e).insert(PaperBin {
            total: BIN_PAPERS,
            stack: Vec::new(),
        });
        refresh_bin_look(world, e);
    } else if is_under(path, LABELER) {
        world.entity_mut(e).insert(Labeler {
            label: String::new(),
            labels_left: LABELS,
            on: false,
        });
    }
}

fn input(text: String, font: &str, color: &str, bold: bool) -> PaperInput {
    PaperInput {
        raw_text: text,
        font: font.to_owned(),
        color: color.to_owned(),
        bold,
        children: Vec::new(),
    }
}

/// `paper/copy` onto `copy` (a fresh sheet): the text, the stamps and their overlays, the name.
pub(super) fn copy_paper(world: &mut World, original: Entity, copy: Entity) {
    let Some(source) = world.get::<Paper>(original).cloned() else {
        return;
    };
    if world.get::<Paper>(copy).is_none() {
        add_parts(world, copy, PAPER);
    }
    world.entity_mut(copy).insert(source);
    let overlays = world
        .get::<ss13_shared::Appearance>(original)
        .map(|a| a.overlays.clone());
    if let Some(overlays) = overlays
        && let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(copy)
    {
        look.overlays = overlays;
    }
    let name = crate::chem::name_of(world, original);
    if let Some(mut dm) = world.get_mut::<DmName>(copy) {
        dm.name.clone_from(&name);
    }
    if world.get::<Name>(copy).is_some() || world.get::<DmName>(copy).is_none() {
        world.entity_mut(copy).insert(Name::new(name));
    }
    refresh_paper_look(world, copy);
}

/// `add_raw_text(text)` of printed papers (reports, manifests): the default font, black. The
/// paper's look and open windows follow.
pub fn write_printed(world: &mut World, paper: Entity, text: impl Into<String>) {
    if world.get::<Paper>(paper).is_none() {
        add_parts(world, paper, PAPER);
    }
    match world.get_mut::<Paper>(paper) {
        Some(mut state) => state
            .inputs
            .push(input(text.into(), PEN_FONT, "#000000", false)),
        None => return,
    }
    refresh_paper_look(world, paper);
    refresh_windows(world, paper);
}

pub(super) fn writing(world: &World, item: Entity) -> Option<Writing> {
    let path = item_path(world, item);
    if is_under(&path, PEN) {
        let (font, color) = match path.as_str() {
            "/obj/item/pen/blue" => (PEN_FONT, "#0000FF"),
            "/obj/item/pen/red" => (PEN_FONT, "#FF0000"),
            "/obj/item/pen/invisible" => (PEN_FONT, "#FFFFFF"),
            "/obj/item/pen/charcoal" => (CHARCOAL_FONT, "#696969"),
            "/obj/item/pen/survival" => (FOUNTAIN_PEN_FONT, "#0000FF"),
            "/obj/item/pen/fourcolor" => {
                let i = world.get::<FourColor>(item).map_or(0, |c| c.0 as usize);
                (PEN_FONT, FOUR_COLORS[i % 4].0)
            }
            _ => (PEN_FONT, "#000000"),
        };
        return Some(Writing::Pen(input(String::new(), font, color, false)));
    }
    if is_under(&path, CRAYON) {
        let color = match path.rsplit('/').next().unwrap_or_default() {
            "red" => "#DA0000",
            "orange" => "#FF9300",
            "yellow" => "#FFF200",
            "green" => "#A8E61D",
            "blue" => "#00B7EF",
            "purple" => "#DA00FF",
            "black" => "#1C1C1C",
            "white" | "mime" => "#FFFFFF",
            "rainbow" => "#FFF000",
            _ => "#FF0000",
        };
        return Some(Writing::Pen(input(String::new(), CRAYON_FONT, color, true)));
    }
    if is_under(&path, STAMP) {
        let atom = world.get::<DmAtom>(item)?;
        return Some(Writing::Stamp {
            state: atom.icon_state.clone(),
            icon: if atom.icon.is_empty() {
                BUREAUCRACY_DMI.to_owned()
            } else {
                atom.icon.clone()
            },
        });
    }
    None
}

/// `update_icon_state`: `paper_words` once written on.
pub(super) fn refresh_paper_look(world: &mut World, paper: Entity) {
    let written = world
        .get::<Paper>(paper)
        .is_some_and(|p| !p.inputs.is_empty());
    let Some(atom) = world.get::<DmAtom>(paper).cloned() else {
        return;
    };
    // A torn carbon sheet is drawn as a plain `paper` (`carbon/update_icon_state`).
    let base = super::stationery::paper_base_state(world, paper)
        .map_or_else(|| atom.icon_state.clone(), str::to_owned);
    let state = if written {
        format!("{base}_words")
    } else {
        base
    };
    set_icon_state(world, paper, &atom.icon, &state);
}

/// `update_overlays`: the top sheet and the bin's lip while it has paper.
// shortcut: one sheet overlay, not one per `PAPERS_PER_OVERLAY`; no written or coloured sheets.
pub(super) fn refresh_bin_look(world: &mut World, bin: Entity) {
    let Some(total) = world.get::<PaperBin>(bin).map(|b| b.total) else {
        return;
    };
    let path = item_path(world, bin);
    let lip = match path.as_str() {
        "/obj/item/paper_bin/bundlenatural" => "paper_bundle_overlay",
        "/obj/item/paper_bin/carbon" => "paper_bin_carbon_overlay",
        "/obj/item/paper_bin/triage" => "",
        _ => "paper_bin_overlay",
    };
    let ids: Vec<_> = if total > 0 {
        let index = world.get_resource::<ss13_mapload::IconIndex>();
        ["paper", lip]
            .into_iter()
            .filter(|s| !s.is_empty())
            .filter_map(|s| index.and_then(|i| i.get(BUREAUCRACY_DMI, s)))
            .collect()
    } else {
        Vec::new()
    };
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(bin) {
        let wanted: Vec<ss13_shared::Overlay> =
            ids.into_iter().map(ss13_shared::Overlay::new).collect();
        if look.overlays != wanted {
            look.overlays = wanted;
        }
    }
}

/// `attack_self`: reading a paper, the four-colour pen's colour, the labeler's switch.
pub(super) fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<Paper>(item).is_some() {
        read(world, client, mob, item);
        return true;
    }
    if item_path(world, item) == "/obj/item/pen/fourcolor" {
        let next = world.get::<FourColor>(item).map_or(1, |c| (c.0 + 1) % 4);
        world.entity_mut(item).insert(FourColor(next));
        let (pen, colour) = (
            crate::chem::name_of(world, item),
            FOUR_COLORS[next as usize].1,
        );
        tell(
            world,
            client,
            Span::Notice,
            format!("{pen} will now write in {colour}."),
        );
        return true;
    }
    if world.get::<Labeler>(item).is_some() {
        toggle_labeler(world, client, mob, item);
        return true;
    }
    false
}

/// `examine` -> `ui_interact` of a paper: blind people cannot read it.
pub(super) fn read(world: &mut World, client: Entity, mob: Entity, paper: Entity) {
    if crate::vision::is_blind(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "You are blind and can't read anything!",
        );
        return;
    }
    let title = crate::chem::name_of(world, paper);
    // `can_read`'s `READING_CHECK_LITERACY`: `TRAIT_ILLITERATE` (the Illiterate quirk, dyslexia).
    if crate::medical::traumas::illiterate(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            format!("Вы пытаетесь прочитать {title}, но ничего не понимаете."),
        );
        return;
    }
    world.write_message(ss13_tgui::server::OpenUi::new::<super::PaperSheetUi>(
        client, mob, paper, title,
    ));
}

/// tg `item_interaction` of papers, bins and labelers. True when the click was theirs.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    // `/obj/item/hand_labeler/interact_with_atom`, on: labels anything.
    if world.get::<Labeler>(held).is_some_and(|l| l.on) {
        apply_label(world, client, mob, held, target);
        return true;
    }
    if world.get::<Labeler>(target).is_some() && item_path(world, held) == LABELER_REFILL {
        if let Some(mut labeler) = world.get_mut::<Labeler>(target) {
            labeler.labels_left = LABELS;
        }
        world.despawn(held);
        tell(world, client, Span::Notice, "refilled");
        return true;
    }
    // A paper into a bin, also a bin clicked on a paper (`paper/item_interaction`).
    if world.get::<PaperBin>(target).is_some() && world.get::<Paper>(held).is_some() {
        insert_into_bin(world, client, held, target);
        return true;
    }
    if world.get::<PaperBin>(held).is_some() && world.get::<Paper>(target).is_some() {
        insert_into_bin(world, client, target, held);
        return true;
    }
    if world.get::<Paper>(target).is_none() {
        return false;
    }
    match writing(world, held) {
        Some(Writing::Pen(_)) => {
            if crate::vision::is_blind(world, mob) {
                tell(world, client, Span::Warning, "You can't see to write!");
                return true;
            }
            // `can_write` -> `is_literate()`.
            if crate::medical::traumas::illiterate(world, mob) {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Вы пытаетесь писать, но вы же не умеете писать!",
                );
                return true;
            }
            start_writing(world, client, mob, target, held);
            true
        }
        Some(Writing::Stamp { state, icon }) => {
            stamp(world, client, mob, held, target, &state, &icon);
            true
        }
        None => false,
    }
}

/// A pen on a paper: where to write when it has empty fields, else what.
pub(super) fn start_writing(
    world: &mut World,
    client: Entity,
    mob: Entity,
    paper: Entity,
    pen: Entity,
) {
    let Some(sheet) = world.get::<Paper>(paper).cloned() else {
        return;
    };
    if sheet.total_length() >= MAX_PAPER_LENGTH {
        tell(world, client, Span::Warning, "This sheet of paper is full!");
        return;
    }
    let fields = sheet.open_fields();
    if fields.is_empty() {
        ask_text(world, client, mob, paper, pen, None);
        return;
    }
    // tgui's write buttons: the end of the document, then each field after the text before it.
    let mut items = vec!["Write at the end".to_owned()];
    for &(input, id) in &fields {
        let text = &sheet.inputs[input];
        let before = text
            .fields()
            .get(id - 1)
            .map(|r| super::plain_text(&text.raw_text[..r.start]))
            .unwrap_or_default();
        let context: String = {
            let chars: Vec<char> = before.trim_end().chars().collect();
            chars[chars.len().saturating_sub(24)..].iter().collect()
        };
        items.push(format!("Field {}: {}", items.len(), context.trim()));
    }
    let title = crate::chem::name_of(world, paper);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "Where would you like to write?",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::WriteWhere { paper, pen, fields });
}

/// The text prompt for the end of `paper` or one of its fields (`(input, id, fields then)`).
fn ask_text(
    world: &mut World,
    client: Entity,
    mob: Entity,
    paper: Entity,
    pen: Entity,
    field: Option<(usize, usize, usize)>,
) {
    let used = world.get::<Paper>(paper).map_or(0, Paper::total_length);
    if used >= MAX_PAPER_LENGTH {
        return;
    }
    let title = crate::chem::name_of(world, paper);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "What would you like to write?",
            PromptKind::Text {
                default: String::new(),
                max_length: (MAX_PAPER_LENGTH - used) as u32,
                multiline: field.is_none(),
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Write { paper, pen, field });
}

/// The chosen place (0 = the end), checked again: the pen and paper still at hand.
pub(super) fn where_answer(
    world: &mut World,
    actor: Entity,
    paper: Entity,
    pen: Entity,
    fields: &[(usize, usize)],
    choice: usize,
) {
    let Some(client) = client_of(world, actor) else {
        return;
    };
    if !pen_at_hand(world, actor, pen) || !reachable(world, actor, paper) {
        return;
    }
    let field = match choice.checked_sub(1) {
        None => None,
        Some(i) => {
            let Some(&(input, id)) = fields.get(i) else {
                return;
            };
            let count = world
                .get::<Paper>(paper)
                .and_then(|p| p.inputs.get(input))
                .map_or(0, |t| t.fields().len());
            Some((input, id, count))
        }
    };
    ask_text(world, client, actor, paper, pen, field);
}

/// The pen in the active hand, or the pen of a clipboard held or in reach (tg `ui_act` writes
/// with `clipboard.pen` unless a stamp is held).
pub(super) fn pen_at_hand(world: &World, actor: Entity, pen: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(actor) else {
        return false;
    };
    if inventory.active_item() == Some(pen) {
        return true;
    }
    let Some(board) = world.get::<crate::storage::StoredIn>(pen).map(|s| s.0) else {
        return false;
    };
    world
        .get::<super::office::Clipboard>(board)
        .is_some_and(|c| c.pen == Some(pen))
        && reachable(world, actor, board)
}

/// The prompt's text onto the paper (`ui_act("add_text")`) or into one of its fields, checked
/// again: the pen still at hand, the paper in reach, the length, the field still there (filling
/// another one renumbers them, so a changed count refuses).
pub(super) fn write_answer(
    world: &mut World,
    actor: Entity,
    paper: Entity,
    pen: Entity,
    field: Option<(usize, usize, usize)>,
    text: &str,
) {
    let Some(client) = client_of(world, actor) else {
        return;
    };
    if !pen_at_hand(world, actor, pen) || !reachable(world, actor, paper) {
        return;
    }
    if text.trim().is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            "You forgot to write anything!",
        );
        return;
    }
    let Some(Writing::Pen(mut line)) = writing(world, pen) else {
        return;
    };
    let used = world.get::<Paper>(paper).map_or(0, Paper::total_length);
    if used + text.chars().count() > MAX_PAPER_LENGTH {
        return;
    }
    line.raw_text = text.to_owned();
    let written = match (world.get_mut::<Paper>(paper), field) {
        (Some(mut state), None) => {
            state.inputs.push(line);
            true
        }
        (Some(mut state), Some((input, id, count))) => state
            .inputs
            .get_mut(input)
            .filter(|t| t.fields().len() == count)
            .is_some_and(|t| t.fill_field(id, line)),
        (None, _) => false,
    };
    if !written {
        return;
    }
    if let Some(pos) = crate::chem::map_pos(world, paper) {
        let sound = crate::sound::PlaySound::new(pos, "writing_pen", 50.0, true)
            .extrarange(crate::sound::SHORT_RANGE_SOUND_EXTRARANGE)
            .falloff_exponent(crate::sound::SOUND_FALLOFF_EXPONENT + 3.0)
            .ignore_walls(false);
        crate::sound::playsound(world, sound);
    }
    tell(
        world,
        client,
        Span::Plain,
        "You have added to your paper masterpiece!",
    );
    refresh_paper_look(world, paper);
    refresh_windows(world, paper);
}

/// Held by `mob` or adjacent to it, or inside a clipboard or folder that is (tg
/// `IsContainedAtomAccessible`).
pub(super) fn reachable(world: &World, mob: Entity, item: Entity) -> bool {
    let near = |e: Entity| {
        world.get::<Inventory>(mob).is_some_and(|i| i.holds(e))
            || crate::storage::in_reach(world, mob, e)
    };
    if near(item) {
        return true;
    }
    world
        .get::<crate::storage::StoredIn>(item)
        .map(|s| s.0)
        .is_some_and(|holder| super::office::is_paper_holder(world, holder) && near(holder))
}

/// `ui_act("add_stamp")` / the blind stamp of `item_interaction`.
pub(super) fn stamp(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tool: Entity,
    paper: Entity,
    state: &str,
    icon: &str,
) {
    if world
        .get::<Paper>(paper)
        .is_some_and(|p| p.stamps.len() >= MAX_PAPER_STAMPS)
    {
        tell(
            world,
            client,
            Span::Warning,
            "There is nowhere else you can stamp!",
        );
        return;
    }
    let tool_name = crate::chem::name_of(world, tool);
    let overlay = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(icon, &format!("paper_{state}")));
    let draw = match world.get_mut::<Paper>(paper) {
        Some(mut sheet) => {
            sheet.stamps.push(tool_name.clone());
            let draw = sheet.overlays <= MAX_PAPER_STAMPS_OVERLAYS;
            if draw {
                sheet.overlays += 1;
            }
            draw
        }
        None => return,
    };
    if draw
        && let Some(id) = overlay
        && let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(paper)
    {
        look.overlays.push(ss13_shared::Overlay::new(id));
    }
    let (user, what) = (
        crate::chem::name_of(world, mob),
        crate::chem::name_of(world, paper),
    );
    say_around(
        world,
        paper,
        Span::Notice,
        format!("{user} stamps {what} with {tool_name}!"),
        &[(mob, format!("You stamp {what} with {tool_name}!"))],
    );
    sound_at(world, paper, STAMP_SOUND, 50.0, true);
    refresh_windows(world, paper);
}

/// `paper_bin/item_interaction` with a paper.
fn insert_into_bin(world: &mut World, client: Entity, paper: Entity, bin: Entity) {
    let what = crate::chem::name_of(world, paper);
    let where_ = crate::chem::name_of(world, bin);
    crate::storage::put_inside(world, paper, bin);
    if let Some(mut state) = world.get_mut::<PaperBin>(bin) {
        state.stack.push(paper);
        state.total += 1;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("You put {what} in {where_}."),
    );
    refresh_bin_look(world, bin);
}

/// `paper_bin/attack_hand`: the top sheet into the hand. True when `bin` is a bin.
pub(super) fn take_from_bin(world: &mut World, client: Entity, mob: Entity, bin: Entity) -> bool {
    let Some(state) = world.get::<PaperBin>(bin).cloned() else {
        return false;
    };
    let where_ = crate::chem::name_of(world, bin);
    if state.total == 0 {
        tell(world, client, Span::Warning, format!("{where_} is empty!"));
        return true;
    }
    let Some(pos) = crate::chem::map_pos(world, mob) else {
        return true;
    };
    let mut stack = state.stack.clone();
    let paper = match stack.pop().filter(|&p| world.get_entity(p).is_ok()) {
        Some(p) => Some(p),
        None => {
            let kind = match item_path(world, bin).as_str() {
                "/obj/item/paper_bin/construction" => "/obj/item/paper/construction",
                "/obj/item/paper_bin/bundlenatural" => "/obj/item/paper/natural",
                "/obj/item/paper_bin/carbon" => "/obj/item/paper/carbon",
                _ => PAPER,
            };
            crate::construction::spawn_obj(world, kind, pos)
                .or_else(|| crate::construction::spawn_obj(world, PAPER, pos))
        }
    };
    if let Some(mut s) = world.get_mut::<PaperBin>(bin) {
        s.total -= 1;
        s.stack = stack;
    }
    if let Some(paper) = paper
        && let Some(inventory) = world.get::<Inventory>(mob).cloned()
    {
        crate::storage::put_in_hands(world, mob, &inventory, paper);
        crate::forensics::server::add_fingerprint(world, paper, mob, false);
        let what = crate::chem::name_of(world, paper);
        tell(
            world,
            client,
            Span::Notice,
            format!("You take {what} out of {where_}."),
        );
    }
    refresh_bin_look(world, bin);
    true
}

/// `hand_labeler/interact`: on asks for the text.
fn toggle_labeler(world: &mut World, client: Entity, mob: Entity, labeler: Entity) {
    let (on, label) = match world.get_mut::<Labeler>(labeler) {
        Some(mut state) => {
            state.on = !state.on;
            (state.on, state.label.clone())
        }
        None => return,
    };
    if let Some(atom) = world.get::<DmAtom>(labeler).cloned() {
        set_icon_state(
            world,
            labeler,
            &atom.icon,
            if on { "labeler1" } else { "labeler0" },
        );
    }
    let name = crate::chem::name_of(world, labeler);
    if !on {
        tell(world, client, Span::Notice, format!("You turn off {name}."));
        return;
    }
    tell(world, client, Span::Notice, format!("You turn on {name}."));
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Set Label",
            "Label text",
            PromptKind::Text {
                default: label,
                max_length: MAX_NAME_LEN,
                multiline: false,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Label { labeler });
}

/// The label text, checked again: the labeler still held.
pub(super) fn label_answer(world: &mut World, actor: Entity, labeler: Entity, text: &str) {
    let Some(client) = client_of(world, actor) else {
        return;
    };
    let held = world
        .get::<Inventory>(actor)
        .is_some_and(|i| i.holds(labeler));
    // `reject_bad_text`: no control characters.
    let text = text.trim();
    if !held || text.is_empty() || text.chars().any(char::is_control) {
        tell(world, client, Span::Warning, "Invalid text!");
        return;
    }
    if let Some(mut state) = world.get_mut::<Labeler>(labeler) {
        state.label = text.to_owned();
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("You set the text to '{text}'."),
    );
}

/// `apply_label`: ` (label)` after the target's name.
fn apply_label(world: &mut World, client: Entity, mob: Entity, labeler: Entity, target: Entity) {
    let Some(state) = world.get::<Labeler>(labeler).cloned() else {
        return;
    };
    if state.labels_left == 0 {
        tell(world, client, Span::Warning, "no labels left!");
        return;
    }
    if state.label.is_empty() {
        tell(world, client, Span::Warning, "no text set!");
        return;
    }
    let name = crate::chem::name_of(world, target);
    if name.chars().count() + state.label.chars().count() > MAX_LABEL_LEN {
        tell(world, client, Span::Warning, "label too long!");
        return;
    }
    if world.get::<Health>(target).is_some() {
        tell(world, client, Span::Warning, "can't label!");
        return;
    }
    let labelled = format!("{name} ({})", state.label);
    if let Some(mut dm) = world.get_mut::<DmName>(target) {
        dm.name.clone_from(&labelled);
    }
    if world.get::<Name>(target).is_some() || world.get::<DmName>(target).is_none() {
        world.entity_mut(target).insert(Name::new(labelled));
    }
    if let Some(mut s) = world.get_mut::<Labeler>(labeler) {
        s.labels_left -= 1;
    }
    let user = crate::chem::name_of(world, mob);
    say_around(
        world,
        target,
        Span::Notice,
        format!("{user} labels {name} with \"{}\".", state.label),
        &[(mob, format!("You label {name} with \"{}\".", state.label))],
    );
    sound_at(world, target, LABEL_SOUND, 20.0, true);
}
