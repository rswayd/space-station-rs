//! Dice (server), after tg `code/game/objects/items/dice.dm`: a die keeps its `result` (rolled
//! at spawn), shown as the `<icon_state>-<result>` overlay; rolled in hand it lands on
//! `rand(1, sides)` with a sound and a line to those around (hearers get a fake roll in tg).
// shortcut: no rolls on throw impact (`throw_impact`), no rigging (microwave, loaded dice), no
// special faces but the fudge die's, no d4 caltrops or the eight-ball and spess-cube texts.

use bevy::prelude::*;
use ss13_mapload::DmAtom;
use ss13_shared::chat::Span;

use super::server::{item_path, say_around, service_data, sound_at};
use crate::combat::CombatRng;
use crate::obj_damage::is_under;

pub const DICE: &str = ss13_mapload::service::DICE_ROOT;
const ROLL_SOUND: &str = "sound/items/dice_roll.ogg";
/// `MIN_SIDES_ALERT`.
const MIN_SIDES_ALERT: u32 = 5;
const FUDGE: &str = "/obj/item/dice/fudge";
const D00: &str = "/obj/item/dice/d00";

/// `/obj/item/dice` `sides` and `result`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Die {
    pub sides: u32,
    pub result: u32,
}

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if !is_under(path, DICE) || world.get::<Die>(e).is_some() {
        return;
    }
    // A die without converted data is tg's default d6.
    let sides = match service_data(world, e).sides {
        0 => 6,
        n => n,
    };
    let result = world
        .get_resource_mut::<CombatRng>()
        .map_or(1, |mut rng| rng.range(1, sides));
    world.entity_mut(e).insert(Die { sides, result });
    refresh_look(world, e);
}

/// `update_overlays`: `"[icon_state]-[result]"`.
fn refresh_look(world: &mut World, e: Entity) {
    let (Some(die), Some(atom)) = (
        world.get::<Die>(e).copied(),
        world.get::<DmAtom>(e).cloned(),
    ) else {
        return;
    };
    let id = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(&atom.icon, &format!("{}-{}", atom.icon_state, die.result)));
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(e) {
        let wanted: Vec<ss13_shared::Overlay> =
            id.into_iter().map(ss13_shared::Overlay::new).collect();
        if look.overlays != wanted {
            look.overlays = wanted;
        }
    }
}

/// The shown face and its comment (`manipulate_result`, `special_faces`, the bad luck and
/// natural 20 notes).
pub fn face(path: &str, sides: u32, result: u32) -> (String, &'static str) {
    if path == FUDGE {
        return match result {
            1 => ("minus".to_owned(), ""),
            2 => ("blank".to_owned(), "You aren't sure how to feel."),
            _ => ("plus".to_owned(), ""),
        };
    }
    let comment = if sides > MIN_SIDES_ALERT && result == 1 {
        "Ouch, bad luck."
    } else if sides == 20 && result == 20 {
        "NAT 20!"
    } else {
        ""
    };
    let shown = if path == D00 {
        (result - 1) * 10
    } else {
        result
    };
    (shown.to_string(), comment)
}

/// `dice/attack_self` -> `diceroll(user, in_hand = TRUE)`.
pub(super) fn attack_self(world: &mut World, _client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(die) = world.get::<Die>(item).copied() else {
        return false;
    };
    let result = world.resource_mut::<CombatRng>().range(1, die.sides);
    world.entity_mut(item).insert(Die { result, ..die });
    sound_at(world, item, ROLL_SOUND, 50.0, true);
    refresh_look(world, item);
    let (shown, comment) = face(&item_path(world, item), die.sides, result);
    let (user, what) = (
        crate::chem::name_of(world, mob),
        crate::chem::name_of(world, item),
    );
    let tail = if comment.is_empty() {
        String::new()
    } else {
        format!(" {comment}")
    };
    say_around(
        world,
        mob,
        Span::Notice,
        format!("{user} rolls {what}. It lands on {shown}.{tail}"),
        &[(mob, format!("You roll {what}. It lands on {shown}.{tail}"))],
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_follow_tg() {
        assert_eq!(
            face("/obj/item/dice/d20", 20, 20),
            ("20".to_owned(), "NAT 20!")
        );
        assert_eq!(
            face("/obj/item/dice/d6", 6, 1),
            ("1".to_owned(), "Ouch, bad luck.")
        );
        assert_eq!(face("/obj/item/dice/d4", 4, 1), ("1".to_owned(), ""));
        assert_eq!(face(D00, 10, 3), ("20".to_owned(), ""));
        assert_eq!(
            face(FUDGE, 3, 2),
            ("blank".to_owned(), "You aren't sure how to feel.")
        );
    }
}
