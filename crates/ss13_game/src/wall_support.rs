//! Wall supports (server), after tg `code/game/objects/structures/wall_support.dm` and
//! `/turf/closed/wall/proc/try_build_support`: two rods on a wall (2 s) put a support on it,
//! which keeps the equipment hanging on the wall when the wall comes down. A screwdriver
//! fastens or unfastens it, right-click wirecutters cut it back into its rods (one when broken).
// shortcut: no damage or breaking (`atom_break`, the grille's hit sounds), no burning above
// 1500 °C; wall equipment does not fall off removed walls here at all (`atom_mounted`, see the
// plain wall's entry), so the support only stands where the wall was.

use bevy::prelude::*;
use ss13_mapload::construct::DmTurfBuild;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::click_handlers::{AddClickHandler, ClickCtx, ClickPhase};
use crate::construction::Job;
use crate::obj_damage::{Integrity, anchored, set_anchored};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, ToolKind, stack_amount, tell, tool_of};

pub const WALL_SUPPORT: &str = "/obj/structure/wall_support";
const WALL: &str = "/turf/closed/wall";
/// `rods_type`, `rods_amount`.
const RODS: &str = "/obj/item/stack/rods";
const RODS_AMOUNT: u32 = 2;
/// `do_after(user, 2 SECONDS, target = src)`.
const BUILD_DS: f32 = 20.0;

pub struct WallSupportPlugin;

impl Plugin for WallSupportPlugin {
    fn build(&self, app: &mut App) {
        app.add_click_handler(ClickPhase::HeldOnTarget, 2961, held_on_target);
    }
}

fn is_support(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, WALL_SUPPORT))
}

fn is_rods(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, RODS))
}

fn is_wall(world: &World, turf: Entity) -> bool {
    world
        .get::<DmTurfBuild>(turf)
        .is_some_and(|t| is_under(&t.path, WALL))
}

fn held_on_target(world: &mut World, ctx: &ClickCtx) -> bool {
    let Some(held) = ctx.held else {
        return false;
    };
    let actor = Actor {
        client: ctx.client,
        mob: ctx.mob,
        item: Some(held),
    };
    if is_wall(world, ctx.target) && is_rods(world, held) {
        if stack_amount(world, held) < RODS_AMOUNT {
            tell(
                world,
                ctx.client,
                Span::Warning,
                format!("Для этого нужно как минимум {RODS_AMOUNT} прута!"),
            );
            return true;
        }
        tell(
            world,
            ctx.client,
            Span::Notice,
            "Вы начинаете ставить опору...",
        );
        crate::tools::do_after(
            world,
            actor,
            ctx.target,
            BUILD_DS,
            RODS_AMOUNT,
            Job::WallSupport,
        );
        return true;
    }
    if !is_support(world, ctx.target) {
        return false;
    }
    match tool_of(world, held).map(|(k, _)| k) {
        Some(ToolKind::Wirecutters) if ctx.modifiers.right => {
            playsound(
                world,
                PlaySound::new(ctx.pos, "sound/items/tools/wirecutter.ogg", 100.0, true),
            );
            deconstruct(world, ctx.target);
            true
        }
        Some(ToolKind::Screwdriver) if !ctx.modifiers.right => {
            let fasten = !anchored(world, ctx.target);
            set_anchored(world, ctx.target, fasten);
            playsound(
                world,
                PlaySound::new(ctx.pos, "sound/items/tools/screwdriver.ogg", 100.0, true),
            );
            let (own, others) = if fasten {
                (
                    "Вы прикручиваете опору к полу.",
                    "Кто-то прикручивает опору.",
                )
            } else {
                (
                    "Вы откручиваете опору от пола.",
                    "Кто-то откручивает опору.",
                )
            };
            crate::chem::visible(world, ctx.mob, own, others, Span::Notice);
            true
        }
        _ => false,
    }
}

/// `atom_deconstruct`: its rods (one when broken).
fn deconstruct(world: &mut World, support: Entity) {
    let Some(&pos) = world.get::<TilePos>(support) else {
        return;
    };
    let broken = world.get::<Integrity>(support).is_some_and(|i| i.broken);
    let amount = if broken { 1 } else { RODS_AMOUNT };
    world.despawn(support);
    crate::construction::spawn_stack(world, RODS, pos, amount);
    crate::construction::tile_changed(world, pos);
}

/// The end of the 2 s: the wall is still a wall and the rods are still there.
pub fn complete(world: &mut World, action: &DoAfter) {
    let wall = action.target;
    let Some(rods) = action.item else {
        return;
    };
    if !is_wall(world, wall) || !is_rods(world, rods) {
        return;
    }
    if !crate::tools::consume(world, rods, action.amount) {
        return;
    }
    let pos = action.target_pos;
    if crate::construction::spawn_obj(world, WALL_SUPPORT, pos).is_some() {
        tell(
            world,
            action.client,
            Span::Notice,
            "Вы ставите опору на стену.",
        );
    }
}

/// `examine`: its screws and rods.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if !is_support(world, target) {
        return;
    }
    let text = if anchored(world, target) {
        "Она закреплена винтами. Прутья можно перекусить."
    } else {
        "Крепёжные винты откручены. Прутья можно перекусить."
    };
    lines.push((Span::Notice, text.to_owned()));
}
