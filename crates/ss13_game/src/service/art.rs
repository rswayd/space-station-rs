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
// shortcut: no pixel grid (tgui `Canvas` sprite editor) or generated icon, so a painting looks
// like a work in progress; no colour palette, cleaning, frames, patronage, persistent
// paintings or the gallery. Paint: a plain colour multiply instead of the saturation filter
// (`color_transition_filter`, the right click override), no falling hazard, cleaning does not
// wash it off (`WASHABLE_COLOUR_PRIORITY`).

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptKind, ask_world};

use super::server::{PendingAsk, client_of, item_path, say_around};
use crate::chem::name_of;
use crate::obj_damage::is_under;
use crate::tools::tell;

const CANVAS: &str = "/obj/item/canvas";
const EASEL: &str = "/obj/structure/easel";
const PALETTE: &str = "/obj/item/paint_palette";
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
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            used: false,
            finalized: false,
            title: "Untitled Artwork".to_owned(),
            author: String::new(),
            medium: String::new(),
        }
    }
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
}

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, CANVAS) && world.get::<Canvas>(e).is_none() {
        world.entity_mut(e).insert(Canvas::default());
    }
    if is_under(path, PAINT) && !is_under(path, PAINT_REMOVER) && world.get::<PaintCan>(e).is_none()
    {
        world.entity_mut(e).insert(PaintCan {
            color: can_color(path),
            left: PAINT_USES,
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
    if paint_on(world, held, target) {
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
    let Some(canvas) = world.get::<Canvas>(item).cloned() else {
        return false;
    };
    let name = name_of(world, item);
    if canvas.finalized {
        tell(
            world,
            client,
            Span::Notice,
            format!(
                "\"{}\" by {}, {}.",
                canvas.title, canvas.author, canvas.medium
            ),
        );
        return true;
    }
    if !canvas.used {
        tell(world, client, Span::Notice, format!("{name} is blank."));
        return true;
    }
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
    true
}

/// `try_rename` and `finalize`, checked again: the canvas unfinished and at hand.
pub(super) fn answer(world: &mut World, actor: Entity, ask: ArtAsk, answer: PromptAnswer) {
    let Some(client) = client_of(world, actor) else {
        return;
    };
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
    };
    let unfinished = world.get::<Canvas>(canvas).is_some_and(|c| !c.finalized);
    if !unfinished || !super::paper::reachable(world, actor, canvas) {
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
        }
        _ => {}
    }
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
}
