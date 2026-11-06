//! BandaStation's paper shredder (server): `/obj/machinery/papershredder`
//! (`modular_bandastation/objects/code/machinery/papershredder.dm`).
//!
//! - A photo, shredded paper or paper (1), a newspaper or ID card (3), a folder (4) or a book (5)
//!   clicked on it is destroyed and adds that much to the bin, up to 15; a full bin refuses.
//! - A right click with an empty hand (hands free) drops one shredded paper on its tile.
//! - A wrench (out of combat mode, 2 s) fastens or unfastens it (`default_unfasten_wrench`).
//! - Its sprite is `papershredder0`..`papershredder5` by `paper_amount / 3`.
// shortcut: no screentips, no examine hint, shredded paper does not burn
// (`burn_on_item_ignition`); add them with those mechanics.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use crate::obj_damage::{Anchored, is_under, set_icon_state};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, ToolKind, tell, tool_of, use_tool};

pub const SHREDDER: &str = "/obj/machinery/papershredder";
pub const SHREDDED_PAPER: &str = "/obj/item/shredded_paper";
/// `max_paper`.
pub const MAX_PAPER: u32 = 15;
/// `shred_amounts`.
// tg-fork deviation: DM checks `item.type in shred_amounts` (the exact type), which refuses
// every ID card (always a subtype) and every paper subtype; the intent is "documents", so
// subtypes count here.
const SHRED_AMOUNTS: [(&str, u32); 7] = [
    ("/obj/item/photo", 1),
    (SHREDDED_PAPER, 1),
    ("/obj/item/paper", 1),
    ("/obj/item/newspaper", 3),
    ("/obj/item/card/id", 3),
    ("/obj/item/folder", 4),
    ("/obj/item/book", 5),
];
const SHRED_SOUND: &str = "modular_bandastation/objects/sounds/pshred.ogg";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";
/// `default_unfasten_wrench(user, tool, time = 20)`, deciseconds.
const WRENCH_DS: f32 = 20.0;
/// `shredded_paper/Initialize`: `prob(65)` one of these colours.
const PAPER_COLORS: [[u8; 4]; 3] = [
    [0x8b, 0x8b, 0x8b, 0xff],
    [0xe7, 0xe4, 0xe4, 0xff],
    [0xc9, 0xc9, 0xc9, 0xff],
];

/// `paper_amount` (no component: empty).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShredderBin(pub u32);

pub struct PaperShredderPlugin;

impl Plugin for PaperShredderPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `attack_hand_secondary`: empty the bin.
        app.add_click_handler(ClickPhase::EmptyHand, 3462, |w, c| {
            c.modifiers.right && crate::papershredder::empty_hand(w, c.client, c.mob, c.target)
        });
        // `attackby` (shredding) and `wrench_act`.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2105, |w, c| {
            c.held.is_some_and(|h| {
                crate::papershredder::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.add_observer(color_shreds);
    }
}

fn is_shredder(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, SHREDDER))
}

/// What shredding `item` adds to the bin (`None`: not shreddable).
pub fn shred_amount(path: &str) -> Option<u32> {
    SHRED_AMOUNTS
        .iter()
        .find(|(root, _)| is_under(path, root))
        .map(|&(_, n)| n)
}

fn bin(world: &World, e: Entity) -> u32 {
    world.get::<ShredderBin>(e).map_or(0, |b| b.0)
}

/// `update_icon_state`.
fn set_bin(world: &mut World, e: Entity, amount: u32) {
    world.entity_mut(e).insert(ShredderBin(amount));
    let state = format!("papershredder{}", (amount / 3).min(5));
    set_icon_state(world, e, &state);
}

/// `attackby` with a shreddable item, `wrench_act`. True when the click was the shredder's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !is_shredder(world, target) {
        return false;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    if !combat && tool_of(world, held).is_some_and(|(kind, _)| kind == ToolKind::Wrench) {
        let anchored = world.get::<Anchored>(target).map_or_else(
            || world.get::<DmObj>(target).is_some_and(|o| o.anchored),
            |a| a.0,
        );
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        if let Some(&pos) = world.get::<TilePos>(target) {
            playsound(world, PlaySound::new(pos, RATCHET, 50.0, true));
        }
        use_tool(
            world,
            actor,
            target,
            WRENCH_DS,
            0,
            crate::construction::Job::Unfasten { anchored },
        );
        return true;
    }
    let Some(amount) = world
        .get::<DmItem>(held)
        .and_then(|i| shred_amount(&i.path))
    else {
        return false;
    };
    let now = bin(world, target);
    // DM compares `paper_amount == max_paper`, which a 3-5 item steps over; a full bin is `>=`.
    if now >= MAX_PAPER {
        let name = crate::chem::name_of(world, target);
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} заполнен. Опустошите его, прежде чем продолжить."),
        );
        return true;
    }
    world.despawn(held);
    if let Some(&pos) = world.get::<TilePos>(target) {
        playsound(world, PlaySound::new(pos, SHRED_SOUND, 75.0, true));
    }
    set_bin(world, target, now + amount);
    crate::forensics::server::add_fingerprint(world, target, mob, false);
    true
}

/// `attack_hand_secondary` -> `empty_contents`: one shredded paper out. True when `target`
/// was a shredder.
pub fn empty_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !is_shredder(world, target) {
        return false;
    }
    if world.get::<crate::restraints::Handcuffed>(mob).is_some() {
        tell(world, client, Span::Warning, "Вам нужны свободные руки.");
        return true;
    }
    let now = bin(world, target);
    if now == 0 {
        let name = crate::chem::name_of(world, target);
        tell(world, client, Span::Notice, format!("{name} пуст."));
        return true;
    }
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::construction::spawn_obj(world, SHREDDED_PAPER, pos);
    }
    set_bin(world, target, now - 1);
    true
}

/// `shredded_paper/Initialize`: `if(prob(65)) color = pick(...)`.
fn color_shreds(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if !items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, SHREDDED_PAPER))
    {
        return;
    }
    let e = add.entity;
    commands.queue(move |world: &mut World| {
        let color = {
            let Some(mut rng) = world.get_resource_mut::<crate::combat::CombatRng>() else {
                return;
            };
            if !rng.chance(65.0) {
                return;
            }
            PAPER_COLORS[rng.range(0, 2) as usize]
        };
        if let Some(mut look) = world.get_mut::<Appearance>(e) {
            look.color = Some(color);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shred_amounts_follow_the_dm_table() {
        assert_eq!(shred_amount("/obj/item/paper"), Some(1));
        assert_eq!(shred_amount("/obj/item/paper/fluff"), Some(1));
        assert_eq!(shred_amount("/obj/item/card/id/advanced"), Some(3));
        assert_eq!(shred_amount("/obj/item/folder/blue"), Some(4));
        assert_eq!(shred_amount("/obj/item/book"), Some(5));
        assert_eq!(shred_amount("/obj/item/paperplane"), None);
        assert_eq!(shred_amount("/obj/item/wrench"), None);
    }
}
