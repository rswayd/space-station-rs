//! The punching bag and the loom (server): tg `/obj/structure/punching_bag`
//! (`code/game/objects/structures/gym/punching_bag.dm`) and `/obj/structure/loom`
//! (`loom.dm`, `/datum/element/loomable`).
//!
//! - An empty hand punches the bag (a random hit sound); a human pays 3 stamina (2 in boxing
//!   gloves), gains 0.3 athletics and is exercised (`crate::buffs::exercise`). A wrench right click secures or frees it at once; a
//!   crowbar right click takes a loose one apart in 10 s (2 iron, a rod, a pillow).
//! - Cotton (and durathread, wool) used on a secured loom weaves 4 into one cloth (durathread
//!   into durathread) a second at a time while the stack lasts. A wrench (0.5 s) fastens the loom.
//! - A wrench takes a chess piece apart in 0.5 s (2 iron; a holodeck one cannot be).
// shortcut: no heavy gravity, spine implant or `TRAIT_STRENGTH` modifiers, no punch flick; every woven sheet is its own stack on the loom's tile.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::TilePos;

use crate::construction::Job;
use crate::obj_damage::is_under;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, Stack, ToolKind, do_after, tool_of};

pub const PUNCHING_BAG: &str = "/obj/structure/punching_bag";
pub const LOOM: &str = "/obj/structure/loom";
pub const CHESS: &str = "/obj/structure/chess";
const COTTON: &str = "/obj/item/stack/sheet/cotton";
const DURATHREAD_COTTON: &str = "/obj/item/stack/sheet/cotton/durathread";
const BOXING_GLOVES: &str = "/obj/item/clothing/gloves/boxing";
/// What the bag and the loom leave and make (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
pub const IRON: &str = "/obj/item/stack/sheet/iron";
pub const RODS: &str = "/obj/item/stack/rods";
pub const PILLOW: &str = "/obj/item/pillow";
pub const CLOTH: &str = "/obj/item/stack/sheet/cloth";
pub const DURATHREAD: &str = "/obj/item/stack/sheet/durathread";

const HIT_SOUNDS: [&str; 7] = [
    "sound/items/weapons/genhit1.ogg",
    "sound/items/weapons/genhit2.ogg",
    "sound/items/weapons/genhit3.ogg",
    "sound/items/weapons/punch1.ogg",
    "sound/items/weapons/punch2.ogg",
    "sound/items/weapons/punch3.ogg",
    "sound/items/weapons/punch4.ogg",
];
/// `loomable` `required_amount`.
pub const LOOM_AMOUNT: u32 = 4;
/// `loom_time`, deciseconds.
const LOOM_DS: f32 = 10.0;
/// The punching bag's deconstruction, deciseconds.
const DECONSTRUCT_DS: f32 = 100.0;
/// The loom's `default_unfasten_wrench` time, deciseconds.
const LOOM_WRENCH_DS: f32 = 5.0;
/// Taking a chess piece apart, deciseconds.
const CHESS_DS: f32 = 5.0;

/// The timed steps of this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GymJob {
    /// A crowbar takes the punching bag apart.
    Deconstruct,
    /// One weave of `stack` on the loom.
    Weave { stack: Entity },
    /// A wrench takes a chess piece apart.
    Chess,
}

pub struct GymPlugin;

impl Plugin for GymPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 3539, |w, c| {
            punch(w, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 79, |w, c| {
            c.held.is_some_and(|h| {
                item_interaction(w, c.client, c.mob, h, c.target, c.modifiers.right)
            })
        });
    }
}

fn path_of(world: &World, e: Entity) -> String {
    crate::construction::path_of(world, e).unwrap_or_default()
}

use crate::obj_damage::anchored;

/// The stamina a punch costs `user`: 3, 2 in boxing gloves.
pub fn punch_cost(world: &World, user: Entity) -> f32 {
    let gloved = world
        .get::<crate::inventory::Inventory>(user)
        .and_then(|inv| inv.slot(ss13_shared::protocol::InventorySlot::Gloves))
        .and_then(|g| world.get::<DmItem>(g))
        .is_some_and(|i| is_under(&i.path, BOXING_GLOVES));
    if gloved { 2.0 } else { 3.0 }
}

/// tg `punching_bag/attack_hand`.
fn punch(world: &mut World, user: Entity, bag: Entity) -> bool {
    if !is_under(&path_of(world, bag), PUNCHING_BAG) {
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(bag) {
        let i = world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, HIT_SOUNDS.len() as u32 - 1) as usize;
        playsound(
            world,
            PlaySound::new(pos, HIT_SOUNDS[i], 25.0, true).extrarange(-1.0),
        );
    }
    if world.get::<crate::human::HumanLook>(user).is_none() {
        return true;
    }
    let cost = punch_cost(world, user);
    crate::status_effects::adjust_stamina(world, user, cost);
    crate::skills::adjust_experience(world, user, crate::skills::Skill::Athletics, 0.3, false);
    crate::buffs::exercise(world, user, 0.0);
    true
}

fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    let path = path_of(world, target);
    let tool = tool_of(world, held).map(|(t, _)| t);
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if is_under(&path, PUNCHING_BAG) {
        if !right {
            return false;
        }
        return match tool {
            Some(ToolKind::Wrench) => {
                let to = !anchored(world, target);
                crate::obj_damage::set_anchored(world, target, to);
                crate::balloon::balloon_alert(
                    world,
                    mob,
                    target,
                    if to {
                        "закреплено"
                    } else {
                        "откреплено"
                    },
                );
                true
            }
            Some(ToolKind::Crowbar) => {
                if anchored(world, target) {
                    crate::balloon::balloon_alert(world, mob, target, "всё ещё закреплено!");
                    return true;
                }
                crate::balloon::balloon_alert(world, mob, target, "разборка...");
                do_after(
                    world,
                    actor,
                    target,
                    DECONSTRUCT_DS,
                    0,
                    Job::Gym(GymJob::Deconstruct),
                );
                true
            }
            _ => false,
        };
    }
    if is_under(&path, CHESS) && tool == Some(ToolKind::Wrench) {
        if world.get::<crate::holodeck::Hologram>(target).is_some() {
            crate::balloon::balloon_alert(world, mob, target, "проходит насквозь!");
            return true;
        }
        crate::tools::tell(
            world,
            client,
            ss13_shared::chat::Span::Notice,
            "Вы начинаете разбирать шахматную фигуру.",
        );
        do_after(world, actor, target, CHESS_DS, 0, Job::Gym(GymJob::Chess));
        return true;
    }
    if !is_under(&path, LOOM) {
        return false;
    }
    if tool == Some(ToolKind::Wrench) {
        let anchored = anchored(world, target);
        crate::tools::use_tool(
            world,
            actor,
            target,
            LOOM_WRENCH_DS,
            0,
            Job::Unfasten { anchored },
        );
        return true;
    }
    let held_path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if !is_under(&held_path, COTTON) {
        return false;
    }
    if !anchored(world, target) {
        let name = crate::medical::name_of(world, target);
        crate::balloon::balloon_alert(world, mob, mob, &format!("{name} нужно закрепить!"));
        return true;
    }
    start_weave(world, actor, target, held)
}

fn start_weave(world: &mut World, actor: Actor, loom: Entity, stack: Entity) -> bool {
    if world.get::<Stack>(stack).map_or(0, |s| s.amount) < LOOM_AMOUNT {
        crate::balloon::balloon_alert(
            world,
            actor.mob,
            actor.mob,
            &format!("нужно {LOOM_AMOUNT} шт.!"),
        );
        return true;
    }
    do_after(
        world,
        actor,
        loom,
        LOOM_DS,
        0,
        Job::Gym(GymJob::Weave { stack }),
    );
    true
}

/// What `cotton` weaves into (`loom_result`).
pub fn loom_result(cotton: &str) -> &'static str {
    if is_under(cotton, DURATHREAD_COTTON) {
        DURATHREAD
    } else {
        CLOTH
    }
}

/// [`GymJob`] at the end of its `do_after`.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: GymJob) {
    let target = action.target;
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    match job {
        GymJob::Deconstruct => {
            if anchored(world, target) || !is_under(&path_of(world, target), PUNCHING_BAG) {
                return;
            }
            world.despawn(target);
            crate::construction::spawn_stack(world, IRON, pos, 2);
            crate::construction::spawn_stack(world, RODS, pos, 1);
            crate::construction::spawn_obj(world, PILLOW, pos);
        }
        GymJob::Chess => {
            if !is_under(&path_of(world, target), CHESS) {
                return;
            }
            world.despawn(target);
            crate::construction::spawn_stack(world, IRON, pos, 2);
            playsound(
                world,
                PlaySound::new(pos, "sound/items/tools/ratchet.ogg", 50.0, true),
            );
        }
        GymJob::Weave { stack } => {
            let cotton = path_of(world, stack);
            let held = world
                .get::<crate::inventory::Inventory>(mob)
                .is_some_and(|inv| inv.holds(stack));
            if !held || !anchored(world, target) {
                return;
            }
            if !crate::tools::use_stack(world, stack, LOOM_AMOUNT) {
                return;
            }
            crate::construction::spawn_stack(world, loom_result(&cotton), pos, 1);
            // The loop goes on while the stack lasts.
            if world
                .get::<Stack>(stack)
                .is_some_and(|s| s.amount >= LOOM_AMOUNT)
            {
                let actor = Actor {
                    client: action.client,
                    mob,
                    item: Some(stack),
                };
                do_after(
                    world,
                    actor,
                    target,
                    LOOM_DS,
                    0,
                    Job::Gym(GymJob::Weave { stack }),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loom_results() {
        assert_eq!(loom_result(COTTON), CLOTH);
        assert_eq!(loom_result("/obj/item/stack/sheet/cotton/wool"), CLOTH);
        assert_eq!(loom_result(DURATHREAD_COTTON), DURATHREAD);
    }

    #[test]
    fn boxing_gloves_spare_stamina() {
        let mut world = World::new();
        let bare = world.spawn_empty().id();
        assert_eq!(punch_cost(&world, bare), 3.0);
    }
}
