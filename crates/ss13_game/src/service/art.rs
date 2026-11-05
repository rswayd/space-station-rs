//! Easels and canvases (server), after tg `code/modules/art/paintings.dm`.
//!
//! - A canvas goes up on an easel (onto its tile).
//! - A paint tool on an unfinished canvas paints on it (`spriteEditorCommand`): the canvas shows
//!   its `<size>wip` splotch and records the medium (`get_paint_tool_medium`, "Mixed medium"
//!   when they differ).
//! - A painted canvas in hand is finished (`finalize` -> `try_rename`): a title, then signed by
//!   the painter or "Anonymous"; a finished one tells its title, author and medium.
// shortcut: no pixel grid (tgui `Canvas` sprite editor) or generated icon, so a painting looks
// like a work in progress; no colour palette, cleaning, frames, patronage, persistent
// paintings or the gallery.

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

#[derive(Clone, Debug)]
pub enum ArtAsk {
    /// "What do you want to name the painting?"
    Title { canvas: Entity },
    /// "Do you want to sign it or remain anonymous?"
    Sign { canvas: Entity, title: String },
}

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, CANVAS) && world.get::<Canvas>(e).is_none() {
        world.entity_mut(e).insert(Canvas::default());
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
