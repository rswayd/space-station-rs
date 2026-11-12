//! Easels and canvases (server), after tg `code/modules/art/paintings.dm`.
//!
//! - A canvas goes up on an easel (onto its tile).
//! - A paint tool on an unfinished canvas paints on it (`spriteEditorCommand`): the canvas shows
//!   its `<size>wip` splotch and records the medium (`get_paint_tool_medium`, "Mixed medium"
//!   when they differ).
//! - A painted canvas in hand is finished (`finalize` -> `try_rename`): a title, then signed by
//!   the painter or "Anonymous"; a finished one tells its title, author and medium.
//! - Paint cans (`code/game/objects/items/paint.dm`) tint the floor or wall they are used on
//!   (200 uses, then `paint_empty`); adaptive paint picks its colour in hand; paint remover
//!   takes the tint off again.
//! - The paint palette (`/obj/item/paint_palette`) and colour-changing spray cans carry a
//!   palette (`/datum/component/palette`, 14 and 8 colours): the palette in hand picks its
//!   colour (`pick_painting_tool_color`); a right click on either in the active hand lists the
//!   saved colours: add the current one, pick one (`set_painting_tool_color`) or remove one.
//! - Painting frames (`/obj/structure/sign/painting`): a canvas of an accepted size goes in
//!   (`frame_canvas`, an unfinished one is finished on the spot), wirecutters take it out; the
//!   frame shows the canvas in its cosmetic frame and is named after the painting. An empty
//!   hand on a framed painting is the window's patronage button (`patron`): the offer comes
//!   off the patron's account, 12.5% goes to the service budget and 22.5% is split between the
//!   curators; a patron may pick a frame of their tier (`get_available_frames`), now or later
//!   with an Alt-click.
// shortcut: no pixel grid (tgui `Canvas` sprite editor) or generated icon, so a painting looks
// like a work in progress (framed too, the canvas and its splotch inside the frame); the
// palette's colour list is a list prompt, not the persistent colour radial; no cleaning, no
// art moodlets (`/datum/element/art`), no large frame mounting (`wallframe/painting/large`
// needs two walls; mapped large frames work), no renaming a framed untitled painting with a
// pen; persistent paintings and the gallery's round-start paintings wait for persistence.
// Paint: a plain colour multiply instead of the saturation filter
// (`color_transition_filter`, the right click override), no falling hazard, cleaning does not
// wash it off (`WASHABLE_COLOUR_PRIORITY`).

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmName, DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptKind, RadialChoice, ask_world};

use super::server::{PendingAsk, client_of, item_path, say_around};
use crate::cargo::economy::{ACCOUNT_SRV, Economy, account_of, bank_card_talk};
use crate::chem::name_of;
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::storage::StoredIn;
use crate::tools::{ToolKind, tell, tool_of};

const CANVAS: &str = "/obj/item/canvas";
const EASEL: &str = "/obj/structure/easel";
const PALETTE: &str = "/obj/item/paint_palette";
const FRAME: &str = "/obj/structure/sign/painting";
const LARGE_FRAME: &str = "/obj/structure/sign/painting/large";
const ART_DMI: &str = "icons/obj/art/artstuff.dmi";
const ART_LARGE_DMI: &str = "icons/obj/art/artstuff_64x64.dmi";
/// `AVAILABLE_PALETTE_SPACE`, `AVAILABLE_SPRAYCAN_SPACE`.
const PALETTE_SPACE: usize = 14;
const SPRAYCAN_SPACE: usize = 8;
/// `PAYCHECK_CREW * 3`, then the `PATRONAGE_*_FRAME` steps.
const PATRONAGE_OK: i64 = 150;
const PATRONAGE_NICE: i64 = PATRONAGE_OK * 5 / 2;
const PATRONAGE_GREAT: i64 = PATRONAGE_NICE * 2;
const PATRONAGE_EXCELLENT: i64 = PATRONAGE_GREAT * 2;
const PATRONAGE_AMAZING: i64 = PATRONAGE_EXCELLENT * 2;
const PATRONAGE_SUPERB: i64 = PATRONAGE_AMAZING * 2;
const PATRONAGE_LEGENDARY: i64 = PATRONAGE_SUPERB * 2;
/// `SSpersistent_paintings.frame_types_by_patronage_tier`, in its order.
const FRAME_TIERS: [(&str, i64); 13] = [
    ("simple", 0),
    ("iron", PATRONAGE_OK),
    ("bamboo", PATRONAGE_OK),
    ("bones", PATRONAGE_OK),
    ("bronze", PATRONAGE_NICE),
    ("clown", PATRONAGE_NICE),
    ("frog", PATRONAGE_NICE),
    ("silver", PATRONAGE_GREAT),
    ("necropolis", PATRONAGE_GREAT),
    ("gold", PATRONAGE_EXCELLENT),
    ("diamond", PATRONAGE_AMAZING),
    ("rainbow", PATRONAGE_SUPERB),
    ("supermatter", PATRONAGE_LEGENDARY),
];
/// `SERVICE_PERCENTILE_CUT`, `CURATOR_PERCENTILE_CUT`.
const SERVICE_CUT: f64 = 0.125;
const CURATOR_CUT: f64 = 0.225;
/// `JOB_CURATOR`.
const CURATOR: &str = "Curator";
/// Canvas types: (path, icon, icon state, `framed_offset_x/y`, fits the large frame). Exact
/// types, as `accepted_canvas_types` compares them.
const CANVASES: [(&str, &str, &str, [i16; 2], bool); 7] = [
    ("/obj/item/canvas", ART_DMI, "11x11", [11, 10], false),
    (
        "/obj/item/canvas/nineteen_nineteen",
        ART_DMI,
        "19x19",
        [7, 7],
        false,
    ),
    (
        "/obj/item/canvas/twentythree_nineteen",
        ART_DMI,
        "23x19",
        [5, 7],
        false,
    ),
    (
        "/obj/item/canvas/twentythree_twentythree",
        ART_DMI,
        "23x23",
        [5, 5],
        false,
    ),
    (
        "/obj/item/canvas/twentyfour_twentyfour",
        ART_DMI,
        "24x24",
        [4, 4],
        false,
    ),
    (
        "/obj/item/canvas/thirtysix_twentyfour",
        ART_LARGE_DMI,
        "36x24",
        [14, 4],
        true,
    ),
    (
        "/obj/item/canvas/fortyfive_twentyseven",
        ART_LARGE_DMI,
        "45x27",
        [9, 4],
        true,
    ),
];
/// The gallery mountings' `desc_with_canvas` (the plain frame has none).
const FRAME_DESCS: [(&str, &str); 5] = [
    (
        "/obj/structure/sign/painting/library",
        "Произведение искусства (или «искусства»). Повесить его мог кто угодно.",
    ),
    (
        "/obj/structure/sign/painting/library_secure",
        "Шедевр, лично отобранный куратором. Предположительно.",
    ),
    (
        "/obj/structure/sign/painting/library_private",
        "Картина, спрятанная от недалёких умов.",
    ),
    (
        "/obj/structure/sign/painting/large/library",
        "Отобранное куратором крупное произведение искусства (или «искусства»). Будем надеяться, \
         холст того стоил.",
    ),
    (
        "/obj/structure/sign/painting/large/library_private",
        "Картина, которую не стоит показывать менее широким умам.",
    ),
];
const SPRAYCAN: &str = "/obj/item/toy/crayon/spraycan";
const CRAYON: &str = "/obj/item/toy/crayon";
const PEN: &str = "/obj/item/pen";
const PAINT: &str = "/obj/item/paint";
const PAINT_ANYCOLOR: &str = "/obj/item/paint/anycolor";
const PAINT_REMOVER: &str = "/obj/item/paint/paint_remover";
const PAINT_ICON: &str = "icons/obj/art/paint.dmi";
/// `paintleft`.
const PAINT_USES: u32 = 200;
/// The cans' `paint_color` by subtype, and adaptive paint's radial choices (name, colour).
const PAINT_COLORS: [(&str, [u8; 4]); 7] = [
    ("black", [0x33, 0x33, 0x33, 0xFF]),
    ("blue", [0x00, 0x00, 0xFF, 0xFF]),
    ("green", [0x00, 0xFF, 0x00, 0xFF]),
    ("red", [0xFF, 0x00, 0x00, 0xFF]),
    ("violet", [0xFF, 0x00, 0xFF, 0xFF]),
    ("white", [0xFF, 0xFF, 0xFF, 0xFF]),
    ("yellow", [0xFF, 0xFF, 0x00, 0xFF]),
];
/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: u32 = 42;

/// `/obj/item/canvas` with its `painting_metadata`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Canvas {
    pub used: bool,
    pub finalized: bool,
    pub title: String,
    pub author: String,
    pub medium: String,
    /// `patron_name`, `credit_value`, `frame_type`.
    pub patron: String,
    pub credit_value: i64,
    pub frame_type: &'static str,
    /// `last_patron`: the mob that may pick the frame this shift.
    pub last_patron: Option<Entity>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            used: false,
            finalized: false,
            title: "Untitled Artwork".to_owned(),
            author: String::new(),
            medium: String::new(),
            patron: String::new(),
            credit_value: 0,
            frame_type: "simple",
            last_patron: None,
        }
    }
}

/// `/obj/structure/sign/painting`: its `current_canvas` (inside it), with the name and desc it
/// has while empty (`initial(name)`, `initial(desc)`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PaintingFrame {
    pub canvas: Option<Entity>,
    pub large: bool,
    name: String,
    desc: String,
}

/// `/datum/component/palette`: the saved colours (`#rrggbb`) and, for the paint palette, its
/// `current_color` (a spray can's is its paint colour).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Palette {
    pub colors: Vec<String>,
    pub selected: String,
}

/// One line of the palette list.
#[derive(Clone, Debug, PartialEq)]
pub enum PaletteEntry {
    Add,
    Pick(String),
    Remove,
}

/// `/obj/item/paint`: its `paint_color` and `paintleft`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PaintCan {
    pub color: [u8; 4],
    pub left: u32,
}

/// A turf tinted by paint (`WASHABLE_COLOUR_PRIORITY`), with the colour it had before.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Painted {
    pub original: Option<[u8; 4]>,
}

/// The colour of a paint can type: its subtype's, white for the rest (`COLOR_WHITE`).
fn can_color(path: &str) -> [u8; 4] {
    PAINT_COLORS
        .iter()
        .find(|(name, _)| is_under(path, &format!("{PAINT}/{name}")))
        .map_or([0xFF; 4], |&(_, c)| c)
}

#[derive(Clone, Debug)]
pub enum ArtAsk {
    /// Adaptive paint's colour (the radial).
    PaintColor { can: Entity },
    /// "What do you want to name the painting?"
    Title { canvas: Entity },
    /// "Do you want to sign it or remain anonymous?"
    Sign { canvas: Entity, title: String },
    /// The paint palette's colour picker.
    ToolColor { item: Entity },
    /// The palette list, as it was asked.
    PaletteMenu {
        item: Entity,
        entries: Vec<PaletteEntry>,
    },
    /// Which saved colour goes.
    PaletteRemove { item: Entity, colors: Vec<String> },
    /// "Unarchived Painting": continue to the patronage.
    Unarchived { canvas: Entity },
    /// "How much do you want to offer?", with the value it was asked at (`sniped_amount`).
    Offer {
        canvas: Entity,
        account: u32,
        sniped: i64,
    },
    /// "Do you want to change the frame appearance now?"
    ChangeFrame { canvas: Entity },
    /// The frame radial.
    Frame {
        canvas: Entity,
        frames: Vec<&'static str>,
    },
}

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, CANVAS) && world.get::<Canvas>(e).is_none() {
        world.entity_mut(e).insert(Canvas::default());
    }
    if is_under(path, PALETTE) && world.get::<Palette>(e).is_none() {
        // `current_color = COLOR_BLACK`.
        world.entity_mut(e).insert(Palette {
            colors: Vec::new(),
            selected: "#000000".to_owned(),
        });
    }
    if is_under(path, PAINT) && !is_under(path, PAINT_REMOVER) && world.get::<PaintCan>(e).is_none()
    {
        world.entity_mut(e).insert(PaintCan {
            color: can_color(path),
            left: PAINT_USES,
        });
    }
}

pub(super) fn add_obj_parts(world: &mut World, e: Entity, path: &str) {
    // The eldritch paintings accept no canvas (`accepted_canvas_types = list()`).
    let eldritch = is_under(path, "/obj/structure/sign/painting/eldritch");
    if is_under(path, FRAME) && !eldritch && world.get::<PaintingFrame>(e).is_none() {
        let (name, desc) = world
            .get::<DmName>(e)
            .map(|n| (n.name.clone(), n.desc.clone()))
            .unwrap_or_default();
        world.entity_mut(e).insert(PaintingFrame {
            canvas: None,
            large: is_under(path, LARGE_FRAME),
            name,
            desc,
        });
    }
}

/// `get_paint_tool_medium`: `None` for what does not paint.
fn medium(path: &str) -> Option<&'static str> {
    if is_under(path, PALETTE) {
        Some("Oil on canvas")
    } else if is_under(path, SPRAYCAN) {
        Some("Spraycan on canvas")
    } else if is_under(path, CRAYON) {
        Some("Crayon on canvas")
    } else if is_under(path, PEN) {
        Some("Ink on canvas")
    } else {
        None
    }
}

/// `easel/item_interaction` with a canvas, a paint tool on a canvas. True when it was theirs.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if paint_on(world, held, target) || frame_interaction(world, client, mob, held, target) {
        return true;
    }
    let on_easel = world
        .get::<DmObj>(target)
        .is_some_and(|o| is_under(&o.path, EASEL));
    if on_easel && world.get::<Canvas>(held).is_some() {
        let Some(&pos) = world.get::<TilePos>(target) else {
            return true;
        };
        crate::inventory::put_down_at(world, held, pos, [0, 0]);
        let (user, canvas, easel) = (
            name_of(world, mob),
            name_of(world, held),
            name_of(world, target),
        );
        say_around(
            world,
            target,
            Span::Notice,
            format!("{user} puts {canvas} on {easel}."),
            &[(mob, format!("You place {canvas} on {easel}."))],
        );
        return true;
    }
    let Some(canvas) = world.get::<Canvas>(target).cloned() else {
        return false;
    };
    let Some(medium) = medium(&item_path(world, held)) else {
        return false;
    };
    let name = name_of(world, target);
    if canvas.finalized {
        tell(world, client, Span::Warning, format!("{name} is finished."));
        return true;
    }
    let medium = if !canvas.medium.is_empty() && canvas.medium != medium {
        "Mixed medium"
    } else {
        medium
    };
    world.entity_mut(target).insert(Canvas {
        used: true,
        medium: medium.to_owned(),
        ..canvas
    });
    refresh_look(world, target);
    tell(world, client, Span::Notice, format!("You paint on {name}."));
    true
}

/// `paint/interact_with_atom` and `paint_remover/interact_with_atom` on a turf (not space).
/// True when it was a can on a turf.
fn paint_on(world: &mut World, held: Entity, target: Entity) -> bool {
    let path = item_path(world, held);
    if !is_under(&path, PAINT) || world.get::<ss13_mapload::Turf>(target).is_none() {
        return false;
    }
    if is_under(&path, PAINT_REMOVER) {
        // tg's `!isturf(...) || !isobj(...)` never passes; the remover is meant to work.
        let Some(painted) = world.get::<Painted>(target).copied() else {
            return false;
        };
        world.entity_mut(target).remove::<Painted>();
        if let Some(mut look) = world.get_mut::<Appearance>(target) {
            look.color = painted.original;
        }
        return true;
    }
    let space = world
        .get::<TilePos>(target)
        .is_some_and(|&pos| crate::construction::is_space(world, pos));
    let Some(can) = world.get::<PaintCan>(held).copied() else {
        return false;
    };
    if space || can.left == 0 {
        return false;
    }
    let original = match world.get::<Painted>(target) {
        Some(p) => p.original,
        None => world.get::<Appearance>(target).and_then(|a| a.color),
    };
    world.entity_mut(target).insert(Painted { original });
    if let Some(mut look) = world.get_mut::<Appearance>(target) {
        look.color = Some(can.color);
    }
    let left = can.left - 1;
    world.entity_mut(held).insert(PaintCan { left, ..can });
    if left == 0 {
        super::server::set_icon_state(world, held, PAINT_ICON, "paint_empty");
    }
    true
}

/// `paint/anycolor/attack_self`: the colour radial (a list here). True when `item` was one.
fn adaptive_paint(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !is_under(&item_path(world, item), PAINT_ANYCOLOR) {
        return false;
    }
    let Some(can) = world.get::<PaintCan>(item).copied() else {
        return false;
    };
    if can.left == 0 {
        tell(world, client, Span::Warning, "Краска закончилась!");
        return true;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Paint",
            "Choose a colour",
            PromptKind::List {
                items: PAINT_COLORS.iter().map(|(n, _)| (*n).to_owned()).collect(),
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Art(ArtAsk::PaintColor { can: item }));
    true
}

/// `update_overlays`: the `<icon_state>wip` splotch once painted on.
fn refresh_look(world: &mut World, canvas: Entity) {
    let used = world.get::<Canvas>(canvas).is_some_and(|c| c.used);
    let Some(atom) = world.get::<DmAtom>(canvas).cloned() else {
        return;
    };
    let wanted: Vec<Overlay> = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&atom.icon, &format!("{}wip", atom.icon_state)))
        .filter(|_| used)
        .map(|id| Overlay {
            pixel_offset: [1, 1],
            ..Overlay::new(id)
        })
        .into_iter()
        .collect();
    if let Some(mut look) = world.get_mut::<Appearance>(canvas)
        && look.overlays != wanted
    {
        look.overlays = wanted;
    }
}

/// `canvas/attack_self` -> the window's `finalize` (or its plaque once finished).
pub(super) fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if adaptive_paint(world, client, mob, item) {
        return true;
    }
    if is_under(&item_path(world, item), PALETTE) {
        // `pick_painting_tool_color(user, current_color)`.
        let default = selected_color(world, item);
        let title = name_of(world, item);
        let prompt = ask_world(
            world,
            Ask::new(
                client,
                mob,
                title,
                "Выберите новый цвет",
                PromptKind::Color {
                    default,
                    autofocus: true,
                },
            ),
        );
        world
            .entity_mut(prompt)
            .insert(PendingAsk::Art(ArtAsk::ToolColor { item }));
        return true;
    }
    let Some(canvas) = world.get::<Canvas>(item).cloned() else {
        return false;
    };
    let name = name_of(world, item);
    if canvas.finalized {
        tell(world, client, Span::Notice, plaque(&canvas));
        return true;
    }
    if !canvas.used {
        tell(world, client, Span::Notice, format!("{name} is blank."));
        return true;
    }
    ask_title(world, client, mob, item);
    true
}

/// `try_rename`'s "What do you want to name the painting?".
fn ask_title(world: &mut World, client: Entity, mob: Entity, item: Entity) {
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Title Your Masterpiece",
            "What do you want to name the painting?",
            PromptKind::Text {
                default: String::new(),
                max_length: MAX_NAME_LEN,
                multiline: false,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Art(ArtAsk::Title { canvas: item }));
}

/// What a finished canvas tells (the window's plaque).
fn plaque(canvas: &Canvas) -> String {
    let mut text = format!(
        "\"{}\" by {}, {}.",
        canvas.title, canvas.author, canvas.medium
    );
    if !canvas.patron.is_empty() {
        text.push_str(&format!(
            " Меценат: {} ({}{}).",
            canvas.patron,
            canvas.credit_value,
            crate::cargo::MONEY_SYMBOL
        ));
    }
    text
}

/// `try_rename` and `finalize`, checked again: the canvas unfinished and at hand.
pub(super) fn answer(world: &mut World, actor: Entity, ask: ArtAsk, answer: PromptAnswer) {
    let Some(client) = client_of(world, actor) else {
        return;
    };
    if matches!(
        ask,
        ArtAsk::ToolColor { .. } | ArtAsk::PaletteMenu { .. } | ArtAsk::PaletteRemove { .. }
    ) {
        palette_answer(world, client, actor, ask, answer);
        return;
    }
    if matches!(
        ask,
        ArtAsk::Unarchived { .. }
            | ArtAsk::Offer { .. }
            | ArtAsk::ChangeFrame { .. }
            | ArtAsk::Frame { .. }
    ) {
        patron_answer(world, client, actor, ask, answer);
        return;
    }
    let canvas = match &ask {
        ArtAsk::Title { canvas } | ArtAsk::Sign { canvas, .. } => *canvas,
        &ArtAsk::PaintColor { can } => {
            // `check_menu`: still held, with paint left.
            let held = world
                .get::<crate::inventory::Inventory>(actor)
                .is_some_and(|i| i.hands.contains(&Some(can)));
            if let (true, PromptAnswer::Item(i)) = (held, answer)
                && let Some(&(name, color)) = PAINT_COLORS.get(i)
                && let Some(state) = world.get::<PaintCan>(can).copied()
                && state.left > 0
            {
                world.entity_mut(can).insert(PaintCan { color, ..state });
                super::server::set_icon_state(world, can, PAINT_ICON, &format!("paint_{name}"));
            }
            return;
        }
        _ => return,
    };
    let unfinished = world.get::<Canvas>(canvas).is_some_and(|c| !c.finalized);
    if !unfinished || !canvas_reachable(world, actor, canvas) {
        return;
    }
    match (ask, answer) {
        (ArtAsk::Title { canvas }, PromptAnswer::Text(title)) => {
            // `reject_bad_name`.
            let title = title.trim().to_owned();
            if title.is_empty() || title.chars().any(char::is_control) {
                return;
            }
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    actor,
                    "Sign painting?",
                    "Do you want to sign it or remain anonymous?",
                    PromptKind::Alert {
                        buttons: ["Yes", "No", "Cancel"].map(str::to_owned).to_vec(),
                        autofocus: true,
                    },
                ),
            );
            world
                .entity_mut(prompt)
                .insert(PendingAsk::Art(ArtAsk::Sign { canvas, title }));
        }
        (ArtAsk::Sign { canvas, title }, PromptAnswer::Button(b)) if b < 2 => {
            let author = if b == 0 {
                name_of(world, actor)
            } else {
                "Anonymous".to_owned()
            };
            if let Some(mut state) = world.get_mut::<Canvas>(canvas) {
                state.title = title;
                state.author = author;
                state.finalized = true;
            }
            if let Some(frame) = frame_of(world, canvas) {
                refresh_frame(world, frame);
            }
        }
        _ => {}
    }
}

/// `in_range(src, user)`: the canvas at hand, or in a frame next to the user.
fn canvas_reachable(world: &World, mob: Entity, canvas: Entity) -> bool {
    super::paper::reachable(world, mob, canvas)
        || frame_of(world, canvas).is_some_and(|f| crate::storage::in_reach(world, mob, f))
}

/// The frame `canvas` hangs in.
fn frame_of(world: &World, canvas: Entity) -> Option<Entity> {
    let holder = world.get::<StoredIn>(canvas)?.0;
    (world.get::<PaintingFrame>(holder)?.canvas == Some(canvas)).then_some(holder)
}

// --- Palettes ------------------------------------------------------------------------------

/// The palette's `max_colors`: the paint palette's, a colour-changing spray can's.
fn palette_space(world: &World, item: Entity) -> Option<usize> {
    if is_under(&item_path(world, item), PALETTE) {
        return Some(PALETTE_SPACE);
    }
    world
        .get::<crate::crayons::Crayon>(item)
        .and_then(|c| c.spray)
        .is_some_and(|s| s != crate::crayons::Spray::Mime)
        .then_some(SPRAYCAN_SPACE)
}

fn hex(color: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

/// `selected_color`: a spray can's paint colour, the palette's `current_color`.
fn selected_color(world: &World, item: Entity) -> String {
    if let Some(crayon) = world.get::<crate::crayons::Crayon>(item) {
        return hex(crayon.color);
    }
    world
        .get::<Palette>(item)
        .map_or_else(|| "#000000".to_owned(), |p| p.selected.clone())
}

/// `set_painting_tool_color`: the palette's colour, a spray can's paint.
fn set_painting_tool_color(world: &mut World, item: Entity, color: &str) {
    if let Some(rgba) = crate::crayons::parse_hex(color)
        && let Some(mut crayon) = world.get_mut::<crate::crayons::Crayon>(item)
        && crayon
            .spray
            .is_some_and(|s| s != crate::crayons::Spray::Mime)
    {
        crayon.color = rgba;
    }
    if let Some(mut palette) = world.get_mut::<Palette>(item) {
        color.clone_into(&mut palette.selected);
    }
}

/// `palette/on_attack_self_secondary` -> `open_radial_menu` (a list here): the saved colours of
/// the palette in the active hand. True when `item` has a palette.
pub fn palette_menu(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(space) = palette_space(world, item) else {
        return false;
    };
    let selected = selected_color(world, item);
    let colors = world
        .get::<Palette>(item)
        .map(|p| p.colors.clone())
        .unwrap_or_default();
    let mut entries = Vec::new();
    let mut items = Vec::new();
    if colors.len() < space && !colors.contains(&selected) {
        entries.push(PaletteEntry::Add);
        items.push(format!("Добавить цвет ({selected})"));
    }
    for color in &colors {
        entries.push(PaletteEntry::Pick(color.clone()));
        items.push(if *color == selected {
            format!("{color} (выбран)")
        } else {
            color.clone()
        });
    }
    if !colors.is_empty() {
        entries.push(PaletteEntry::Remove);
        items.push("Удалить цвет".to_owned());
    }
    let title = name_of(world, item);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "Палитра",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Art(ArtAsk::PaletteMenu { item, entries }));
    true
}

/// `examine` of a palette (`palette/on_examine`) and of a painting frame.
pub fn examine_lines(
    world: &World,
    examiner: Entity,
    target: Entity,
    lines: &mut Vec<crate::examine::Line>,
) {
    if palette_space(world, target).is_some() {
        lines.push((
            Span::Notice,
            "<b>ПКМ</b> по предмету в активной руке открывает его палитру.".to_owned(),
        ));
    }
    if let Some(frame) = world.get::<PaintingFrame>(target)
        && let Some(canvas) = frame.canvas
        && let Some(state) = world.get::<Canvas>(canvas)
    {
        if state.finalized {
            lines.push((Span::Notice, plaque(state)));
        }
        lines.push((Span::Notice, "Картину можно вынуть кусачками.".to_owned()));
        if state.last_patron == Some(examiner) {
            lines.push((
                Span::Notice,
                "Меценат может сменить раму картины <b>Alt-кликом</b>.".to_owned(),
            ));
        }
    }
}

/// The palette's answers, checked again: still in the actor's hands (`user.is_holding`).
fn palette_answer(
    world: &mut World,
    client: Entity,
    actor: Entity,
    ask: ArtAsk,
    answer: PromptAnswer,
) {
    let item = match &ask {
        ArtAsk::ToolColor { item }
        | ArtAsk::PaletteMenu { item, .. }
        | ArtAsk::PaletteRemove { item, .. } => *item,
        _ => return,
    };
    let held = world
        .get::<Inventory>(actor)
        .is_some_and(|i| i.hands.contains(&Some(item)));
    let Some(space) = palette_space(world, item).filter(|_| held) else {
        return;
    };
    match (ask, answer) {
        (ArtAsk::ToolColor { .. }, PromptAnswer::Color(color)) => {
            set_painting_tool_color(world, item, &color);
        }
        (ArtAsk::PaletteMenu { entries, .. }, PromptAnswer::Item(i)) => match entries.get(i) {
            Some(PaletteEntry::Add) => {
                let selected = selected_color(world, item);
                let mut palette = world.get::<Palette>(item).cloned().unwrap_or_default();
                if palette.colors.len() < space {
                    palette.colors.push(selected);
                    world.entity_mut(item).insert(palette);
                }
            }
            Some(PaletteEntry::Pick(color)) => {
                let saved = world
                    .get::<Palette>(item)
                    .is_some_and(|p| p.colors.contains(color));
                if saved {
                    set_painting_tool_color(world, item, color);
                }
            }
            Some(PaletteEntry::Remove) => {
                let colors = world
                    .get::<Palette>(item)
                    .map(|p| p.colors.clone())
                    .unwrap_or_default();
                let title = name_of(world, item);
                let prompt = ask_world(
                    world,
                    Ask::new(
                        client,
                        actor,
                        title,
                        "Какой цвет удалить?",
                        PromptKind::List {
                            items: colors.clone(),
                            default: None,
                        },
                    ),
                );
                world
                    .entity_mut(prompt)
                    .insert(PendingAsk::Art(ArtAsk::PaletteRemove { item, colors }));
            }
            None => {}
        },
        (ArtAsk::PaletteRemove { colors, .. }, PromptAnswer::Item(i)) => {
            let Some(color) = colors.get(i) else {
                return;
            };
            if let Some(mut palette) = world.get_mut::<Palette>(item)
                && let Some(at) = palette.colors.iter().position(|c| c == color)
            {
                palette.colors.remove(at);
            }
        }
        _ => {}
    }
}

// --- Frames --------------------------------------------------------------------------------

/// `sign/painting/item_interaction` with a canvas, `wirecutter_act`. True when it was theirs.
fn frame_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(frame) = world.get::<PaintingFrame>(target).cloned() else {
        return false;
    };
    match frame.canvas {
        None if world.get::<Canvas>(held).is_some() => {
            frame_canvas(world, client, mob, held, target, frame.large);
            true
        }
        Some(canvas) if matches!(tool_of(world, held), Some((ToolKind::Wirecutters, _))) => {
            let Some(pos) = crate::chem::map_pos(world, target) else {
                return true;
            };
            set_framed(world, target, None);
            crate::storage::take_out(world, canvas, pos);
            tell(world, client, Span::Notice, "Вы вынимаете картину из рамы.");
            true
        }
        _ => false,
    }
}

/// Whether a canvas of `path` fits a frame (`accepted_canvas_types`, exact types).
fn fits(path: &str, large: bool) -> bool {
    CANVASES.iter().any(|k| k.0 == path && k.4 == large)
}

/// `frame_canvas`: in it goes, an unfinished one gets its title (`finalize`).
fn frame_canvas(
    world: &mut World,
    client: Entity,
    mob: Entity,
    canvas: Entity,
    frame: Entity,
    large: bool,
) {
    let name = name_of(world, canvas);
    if !fits(&item_path(world, canvas), large) {
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} не помещается в эту раму."),
        );
        return;
    }
    crate::storage::put_inside(world, canvas, frame);
    set_framed(world, frame, Some(canvas));
    if world.get::<Canvas>(canvas).is_some_and(|c| !c.finalized) {
        ask_title(world, client, mob, canvas);
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы вставляете {name} в раму."),
    );
}

fn set_framed(world: &mut World, frame: Entity, canvas: Option<Entity>) {
    if let Some(mut state) = world.get_mut::<PaintingFrame>(frame) {
        state.canvas = canvas;
    }
    refresh_frame(world, frame);
}

/// `update_name`, `update_desc`, `update_icon_state` and `update_overlays` of a frame: the
/// canvas (with its splotch, there is no generated icon) inside its cosmetic frame.
fn refresh_frame(world: &mut World, frame: Entity) {
    let Some(state) = world.get::<PaintingFrame>(frame).cloned() else {
        return;
    };
    let painting = state
        .canvas
        .and_then(|c| Some((item_path(world, c), world.get::<Canvas>(c)?.clone())));
    let path = item_path(world, frame);
    let Some((canvas_path, canvas)) = painting else {
        super::server::set_name(world, frame, &state.name);
        if let Some(mut dm) = world.get_mut::<DmName>(frame) {
            state.desc.clone_into(&mut dm.desc);
        }
        if let Some(atom) = world.get::<DmAtom>(frame).cloned() {
            super::server::set_icon_state(world, frame, &atom.icon, &atom.icon_state);
        }
        if let Some(mut look) = world.get_mut::<Appearance>(frame)
            && !look.overlays.is_empty()
        {
            look.overlays.clear();
        }
        return;
    };
    super::server::set_name(world, frame, &format!("картина - {}", canvas.title));
    let desc = FRAME_DESCS
        .iter()
        .find(|(p, _)| *p == path)
        .map_or("", |(_, d)| *d);
    if let Some(mut dm) = world.get_mut::<DmName>(frame) {
        desc.clone_into(&mut dm.desc);
    }
    let Some(&(_, dmi, canvas_state, offset, _)) = CANVASES.iter().find(|k| k.0 == canvas_path)
    else {
        return;
    };
    let frame_state = format!("{canvas_state}frame_{}", canvas.frame_type);
    super::server::set_icon_state(world, frame, dmi, &frame_state);
    let get = |world: &World, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let mut wanted = Vec::new();
    if let Some(id) = get(world, canvas_state) {
        wanted.push(Overlay {
            pixel_offset: [offset[0] - 1, offset[1] - 1],
            ..Overlay::new(id)
        });
    }
    if canvas.used
        && let Some(id) = get(world, &format!("{canvas_state}wip"))
    {
        wanted.push(Overlay {
            pixel_offset: offset,
            ..Overlay::new(id)
        });
    }
    if let Some(id) = get(world, &frame_state) {
        wanted.push(Overlay::new(id));
    }
    if let Some(mut look) = world.get_mut::<Appearance>(frame)
        && look.overlays != wanted
    {
        look.overlays = wanted;
    }
}

// --- Patronage -----------------------------------------------------------------------------

/// `get_available_frames(credit_value)`.
fn available_frames(credit_value: i64) -> Vec<&'static str> {
    FRAME_TIERS
        .iter()
        .filter(|&&(_, needed)| credit_value >= needed)
        .map(|&(name, _)| name)
        .collect()
}

/// An empty hand on a framed painting: its plaque, and the window's patronage button
/// (`ui_act("patronage")` -> `patron`). True when `target` is a frame with a finished painting.
pub fn frame_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(canvas) = world.get::<PaintingFrame>(target).and_then(|f| f.canvas) else {
        return false;
    };
    let Some(state) = world.get::<Canvas>(canvas).cloned().filter(|c| c.finalized) else {
        return false;
    };
    tell(world, client, Span::Notice, plaque(&state));
    // Nothing is archived here: the painting is always "unarchived".
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Незаархивированная картина",
            "Картина ещё не заархивирована и будет утеряна в конце смены, если её не поместить \
             в подходящую раму. Стать её меценатом?",
            PromptKind::Alert {
                buttons: ["Да", "Нет"].map(str::to_owned).to_vec(),
                autofocus: true,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Art(ArtAsk::Unarchived { canvas }));
    true
}

/// `sign/painting/atom_deconstruct`: the canvas and the frame item onto `pos`.
pub fn frame_deconstructed(world: &mut World, frame: Entity, pos: TilePos) {
    let Some(state) = world.get::<PaintingFrame>(frame).cloned() else {
        return;
    };
    if let Some(canvas) = state.canvas.filter(|&c| frame_of(world, c) == Some(frame)) {
        crate::storage::take_out(world, canvas, pos);
    }
    // shortcut: the large frame item is not spawned (it cannot be hung here either).
    if !state.large {
        crate::construction::spawn_obj(world, "/obj/item/wallframe/painting", pos);
    }
}

/// `click_alt` of a frame: its patron picks the frame again. True when it was a painting
/// frame.
pub(super) fn frame_click_alt(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
) -> bool {
    let Some(canvas) = world.get::<PaintingFrame>(target).and_then(|f| f.canvas) else {
        return false;
    };
    if can_select_frame(world, mob, canvas) {
        select_new_frame(world, client, mob, canvas);
    }
    true
}

/// `can_select_frame`: still framed, in reach, by its last patron.
fn can_select_frame(world: &World, mob: Entity, canvas: Entity) -> bool {
    let Some(frame) = frame_of(world, canvas) else {
        return false;
    };
    crate::storage::in_reach(world, mob, frame)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
        && world
            .get::<Canvas>(canvas)
            .is_some_and(|c| c.last_patron == Some(mob))
}

/// `select_new_frame`: the frames of the patronage tier, as a radial around the frame.
fn select_new_frame(world: &mut World, client: Entity, mob: Entity, canvas: Entity) {
    let (Some(frame), Some(state)) = (frame_of(world, canvas), world.get::<Canvas>(canvas)) else {
        return;
    };
    let frames = available_frames(state.credit_value);
    let canvas_path = item_path(world, canvas);
    let Some(&(_, dmi, canvas_state, ..)) = CANVASES.iter().find(|k| k.0 == canvas_path) else {
        return;
    };
    let choices = frames
        .iter()
        .map(|f| RadialChoice {
            name: (*f).to_owned(),
            icon: crate::radial::icon_of(world, dmi, &format!("{canvas_state}frame_{f}")),
        })
        .collect();
    let prompt = crate::radial::show(
        world,
        crate::radial::RadialMenu {
            client,
            actor: mob,
            anchor: frame,
            title: "Рама".to_owned(),
            choices,
            require_near: true,
        },
    );
    if let Some(prompt) = prompt {
        world
            .entity_mut(prompt)
            .insert(PendingAsk::Art(ArtAsk::Frame { canvas, frames }));
    }
}

/// The patronage's answers, each checked again at the moment of effect.
fn patron_answer(
    world: &mut World,
    client: Entity,
    actor: Entity,
    ask: ArtAsk,
    answer: PromptAnswer,
) {
    match (ask, answer) {
        (ArtAsk::Unarchived { canvas }, PromptAnswer::Button(0)) => {
            ask_offer(world, client, actor, canvas);
        }
        (
            ArtAsk::Offer {
                canvas,
                account,
                sniped,
            },
            PromptAnswer::Number(n),
        ) => pay_patronage(world, client, actor, canvas, account, sniped, n),
        (ArtAsk::ChangeFrame { canvas }, PromptAnswer::Button(0)) => {
            if can_select_frame(world, actor, canvas) {
                select_new_frame(world, client, actor, canvas);
            }
        }
        (ArtAsk::Frame { canvas, frames }, PromptAnswer::Item(i)) => {
            let Some(&chosen) = frames.get(i) else {
                return;
            };
            if !can_select_frame(world, actor, canvas) {
                return;
            }
            // The tier may have changed since the radial opened.
            let allowed = world
                .get::<Canvas>(canvas)
                .is_some_and(|c| available_frames(c.credit_value).contains(&chosen));
            if !allowed {
                return;
            }
            if let Some(mut state) = world.get_mut::<Canvas>(canvas) {
                state.frame_type = chosen;
            }
            if let Some(frame) = frame_of(world, canvas) {
                refresh_frame(world, frame);
                crate::balloon::balloon_alert(world, actor, frame, &format!("рама: {chosen}"));
            }
        }
        _ => {}
    }
}

/// `patron` up to `tgui_input_number`: an ID with a personal account that can pay the
/// current value.
fn ask_offer(world: &mut World, client: Entity, mob: Entity, canvas: Entity) {
    let Some(state) = world.get::<Canvas>(canvas).cloned() else {
        return;
    };
    if !state.finalized || frame_of(world, canvas).is_none() {
        return;
    }
    let card = world.get::<Inventory>(mob).and_then(|i| i.id_card(world));
    let Some(card) = card else {
        tell(
            world,
            client,
            Span::Warning,
            "У вас даже нет ID-карты, а вы хотите стать меценатом?",
        );
        return;
    };
    let balance = account_of(world, card).and_then(|id| {
        let account = world.get_resource::<Economy>()?.account(id)?;
        Some((id, account.balance))
    });
    let Some((account, balance)) = balance else {
        tell(
            world,
            client,
            Span::Warning,
            "Не найден подходящий личный счёт.",
        );
        return;
    };
    if balance < state.credit_value {
        tell(world, client, Span::Warning, "Вам это не по карману.");
        return;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Сумма пожертвования",
            "Сколько вы хотите предложить?",
            PromptKind::Number {
                default: (state.credit_value + 1) as f64,
                min: state.credit_value as f64,
                max: balance as f64,
                round: true,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Art(ArtAsk::Offer {
            canvas,
            account,
            sniped: state.credit_value,
        }));
}

/// The rest of `patron`: the payment, the cuts, the patron, then the frame question.
fn pay_patronage(
    world: &mut World,
    client: Entity,
    mob: Entity,
    canvas: Entity,
    account: u32,
    sniped: i64,
    offer: f64,
) {
    let offer = offer.round() as i64;
    // `!offer_amount`, still framed and in reach, nobody outbid it meanwhile.
    let Some(frame) = frame_of(world, canvas) else {
        return;
    };
    let current = world.get::<Canvas>(canvas).map(|c| c.credit_value);
    if offer <= 0
        || offer < sniped
        || current != Some(sniped)
        || !crate::storage::in_reach(world, mob, frame)
    {
        return;
    }
    // The ID that asked must still be the one the patron uses.
    let card = world.get::<Inventory>(mob).and_then(|i| i.id_card(world));
    if card.and_then(|c| account_of(world, c)) != Some(account) {
        return;
    }
    let paid = world
        .get_resource_mut::<Economy>()
        .and_then(|mut e| Some(e.account_mut(account)?.adjust_money(-offer)))
        .unwrap_or(false);
    if !paid {
        tell(
            world,
            client,
            Span::Warning,
            "Ошибка транзакции. Попробуйте ещё раз.",
        );
        return;
    }
    let curators: Vec<u32> = world.get_resource::<Economy>().map_or_else(Vec::new, |e| {
        e.accounts()
            .filter(|(id, a)| *id != account && a.job == CURATOR)
            .map(|(id, _)| id)
            .collect()
    });
    let curator_cut = if curators.is_empty() {
        0
    } else {
        (offer as f64 * CURATOR_CUT / curators.len() as f64).round() as i64
    };
    if let Some(mut economy) = world.get_resource_mut::<Economy>() {
        if let Some(service) = economy.department_mut(ACCOUNT_SRV) {
            service.adjust_money((offer as f64 * SERVICE_CUT) as i64);
        }
        if curator_cut > 0 {
            for &id in &curators {
                if let Some(curator) = economy.account_mut(id) {
                    curator.adjust_money(curator_cut);
                }
            }
        }
    }
    if curator_cut > 0 {
        for id in curators {
            let balance = world
                .get_resource::<Economy>()
                .and_then(|e| e.account(id))
                .map_or(0, |a| a.balance);
            bank_card_talk(
                world,
                id,
                &format!(
                    "Получена доля от меценатства, на счету теперь {balance}{}.",
                    crate::cargo::MONEY_SYMBOL
                ),
            );
        }
    }
    let patron = name_of(world, mob);
    if let Some(mut state) = world.get_mut::<Canvas>(canvas) {
        state.patron = patron;
        state.credit_value = offer;
        state.last_patron = Some(mob);
    }
    tell(
        world,
        client,
        Span::Notice,
        "Фонд доверия Nanotrasen благодарит вас за вклад. Теперь вы официальный меценат этой \
         картины.",
    );
    if available_frames(offer).len() <= 1 {
        return;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Рама мецената",
            "Сменить вид рамы сейчас? Позже в эту смену это можно сделать Alt-кликом, пока вы \
             меценат.",
            PromptKind::Alert {
                buttons: ["Да", "Нет"].map(str::to_owned).to_vec(),
                autofocus: true,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Art(ArtAsk::ChangeFrame { canvas }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_cans_take_their_subtype_colour() {
        assert_eq!(can_color("/obj/item/paint/red"), [0xFF, 0, 0, 0xFF]);
        assert_eq!(can_color("/obj/item/paint/black"), [0x33, 0x33, 0x33, 0xFF]);
        assert_eq!(can_color("/obj/item/paint/anycolor"), [0xFF; 4]);
        assert_eq!(can_color("/obj/item/paint"), [0xFF; 4]);
    }

    #[test]
    fn patronage_unlocks_frames_by_tier() {
        assert_eq!(available_frames(0), vec!["simple"]);
        assert_eq!(available_frames(149).len(), 1);
        assert_eq!(
            available_frames(150),
            vec!["simple", "iron", "bamboo", "bones"]
        );
        // `PATRONAGE_NICE_FRAME` is 375 credits.
        assert_eq!(available_frames(375).len(), 7);
        assert_eq!(
            available_frames(PATRONAGE_LEGENDARY).len(),
            FRAME_TIERS.len()
        );
        assert_eq!(PATRONAGE_LEGENDARY, 12_000);
    }

    #[test]
    fn frames_take_their_canvas_sizes() {
        assert!(fits("/obj/item/canvas", false));
        assert!(fits("/obj/item/canvas/twentyfour_twentyfour", false));
        assert!(!fits("/obj/item/canvas/thirtysix_twentyfour", false));
        assert!(fits("/obj/item/canvas/thirtysix_twentyfour", true));
        assert!(!fits("/obj/item/canvas", true));
        assert!(!fits("/obj/item/canvas/unknown", false));
    }
}
