//! Secret documents (tg `/obj/item/documents`, `code/game/objects/items/secret_documents.dm`):
//! indestructible theft targets (their `resistance_flags`, `crate::obj_damage`; the steal
//! objective `/datum/objective_item/steal/documents`, `crate::antag::objectives`). The
//! photocopier (`crate::service::office`) copies them into a photocopy that remembers what it
//! copied (a copy of a copy, the first original), and a red or blue crayon forges a seal on a
//! photocopy once.
// shortcut: BandaStation's document exchange (`exchange_red`/`exchange_blue`, its briefcase and
// pinpointer) is a traitor objective, ported with it.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use crate::tools::tell;

pub const DOCUMENTS: &str = "/obj/item/documents";
pub const PHOTOCOPY: &str = "/obj/item/documents/photocopy";
const RED_CRAYON: &str = "/obj/item/toy/crayon/red";
const BLUE_CRAYON: &str = "/obj/item/toy/crayon/blue";

/// `/obj/item/documents/photocopy`: `copy_type` (what was copied) and `forgedseal` (the colour
/// name of the forged seal).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct DocumentCopy {
    pub copy_type: String,
    pub forged_seal: Option<&'static str>,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // `photocopy/item_interaction` with a crayon on the documents in the world.
    app.add_click_handler(ClickPhase::HeldOnTarget, 4247, |w, c| {
        c.held.is_some_and(|h| forge_seal(w, c.client, h, c.target))
    });
    // The same on the documents in the other hand.
    app.add_click_handler(ClickPhase::OtherHand, 487, |w, c| {
        c.held.is_some_and(|h| forge_seal(w, c.client, h, c.target))
    });
}

fn path_of(world: &World, item: Entity) -> Option<&str> {
    world.get::<DmItem>(item).map(|i| i.path.as_str())
}

/// `istype(object, /obj/item/documents)`.
pub fn is_documents(world: &World, item: Entity) -> bool {
    path_of(world, item).is_some_and(|p| is_under(p, DOCUMENTS))
}

/// `photocopier/make_document_copy` -> `new /obj/item/documents/photocopy(loc, original)`.
pub fn make_copy(world: &mut World, original: Entity, pos: TilePos) -> Option<Entity> {
    let copy_type = match world.get::<DocumentCopy>(original) {
        // Copy Of A Copy Of A Copy.
        Some(c) => c.copy_type.clone(),
        None if path_of(world, original).is_some_and(|p| is_under(p, PHOTOCOPY)) => String::new(),
        None => path_of(world, original)?.to_owned(),
    };
    let copy = crate::construction::spawn_obj(world, PHOTOCOPY, pos)?;
    world.entity_mut(copy).insert(DocumentCopy {
        copy_type,
        forged_seal: None,
    });
    Some(copy)
}

/// `photocopy/item_interaction`: a red or blue crayon forges that seal. True when it was a
/// crayon of those on a photocopy.
fn forge_seal(world: &mut World, client: Entity, crayon: Entity, target: Entity) -> bool {
    let colour = match path_of(world, crayon) {
        Some(p) if is_under(p, RED_CRAYON) => "red",
        Some(p) if is_under(p, BLUE_CRAYON) => "blue",
        _ => return false,
    };
    if !path_of(world, target).is_some_and(|p| is_under(p, PHOTOCOPY)) {
        return false;
    }
    if world
        .get::<DocumentCopy>(target)
        .is_some_and(|c| c.forged_seal.is_some())
    {
        tell(
            world,
            client,
            Span::Warning,
            "Вы уже подделали печать на этих документах!",
        );
        return true;
    }
    match world.get_mut::<DocumentCopy>(target) {
        Some(mut copy) => copy.forged_seal = Some(colour),
        None => {
            world.entity_mut(target).insert(DocumentCopy {
                copy_type: String::new(),
                forged_seal: Some(colour),
            });
        }
    }
    world
        .entity_mut(target)
        .insert(Name::new(format!("{colour} secret documents")));
    let icon = world.get::<DmAtom>(target).map(|a| a.icon.clone());
    let state = icon.and_then(|icon| {
        world
            .get_resource::<IconIndex>()?
            .get(&icon, &format!("docs_{colour}"))
    });
    if let (Some(state), Some(mut look)) = (state, world.get_mut::<Appearance>(target)) {
        look.icon_state = state;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "Вы подделываете официальную печать {} мелком. Никто не заметит... правда?",
            if colour == "red" {
                "красным"
            } else {
                "синим"
            }
        ),
    );
    true
}
