//! Spray bottles and the flying puff (`reagent_containers/spray.dm`, `/obj/effect/decal/chempuff`).
//!
//! A held spray clicked on anything (adjacent, or far with `RangedClick`) spends its transfer
//! amount on a puff that flies to the target, `range` tiles at most (one step per
//! `max(floor(2 + 3 / range), 2)` deciseconds; in spray mode the puff holds `1 / range` of what
//! was spent). The puff exposes (`VAPOR`) everything it crosses and the turf; a stream hits only
//! a standing, unshielded mob on the way and the rest at the end of its flight. A dense or living
//! adjacent target is sprayed where it stands. An adjacent drainable tank refills the bottle by
//! 50 u. `attack_self` toggles the stream (also changing the amount, `crate::chem::attack_self`).
//! The spray range follows the reagent weights (`on_reagent_change`), 3 to 5 tiles.
// shortcut: no "Empty Spray Bottle" verb and no reverse amount cycle (`attack_self_secondary`);
// no self-refilling borg flowers or janitor chem sprayer; the puff keeps no user for attack logs
// (the server log has the shot).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, HiddenUnderTile, Turf};
use ss13_reagents::{Chemistry, Holder, Owner};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_world::{Density, SpatialIndex, WorldSystems};

use crate::chem::{Chem, Reagents, TransferAmount};
use crate::click::{RangedClick, ResolveClicks};
use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::combat::NextMove;
use crate::construction::{path_of, spawn_obj, turf_at};
use crate::health::{BodyPosition, Health};
use crate::inventory::Inventory;
use crate::tools::{Actor, tell};

pub const SPRAY: &str = "/obj/item/reagent_containers/spray";
pub const CHEMPUFF: &str = "/obj/effect/decal/chempuff";
const MISTER: &str = "/obj/item/reagent_containers/spray/mister";
const CHEMSPRAYER: &str = "/obj/item/reagent_containers/spray/chemsprayer";
/// `CLICK_CD_RANGE`, seconds.
const CLICK_CD_RANGE: f64 = 0.4;
/// `reagent_weight` below the default 1 (`so it sprays further`).
const LIGHT_REAGENTS: [&str; 3] = [
    "/datum/reagent/space_cleaner",
    "/datum/reagent/medicine/c2/hercuri",
    "/datum/reagent/inverse/hercuri",
];
const LIGHT_WEIGHT: f32 = 0.6;
/// `interact_with_atom`: the clicks a spray leaves to the target
/// (`TRAIT_COMBAT_MODE_SKIP_INTERACTION` and the hacks of this file).
const SKIP: [&str; 6] = [
    "/obj/structure/sink",
    "/obj/structure/mop_bucket/janitorialcart",
    "/obj/machinery/hydroponics",
    "/obj/structure/table",
    "/obj/item/storage",
    "/obj/structure/rack",
];

/// The vars of a spray that its subtypes change. `fixed`: a `current_range` that never changes
/// (`on_reagent_change` works on the bottle's own holder: misters have the tank's; the
/// waterflower's and party popper's are intended ranges tg's clamp overrides).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Nozzle {
    spray_range: u32,
    stream_range: u32,
    can_toggle: bool,
    can_fill: bool,
    fixed: Option<u32>,
    stream: bool,
}

const DEFAULT_NOZZLE: Nozzle = Nozzle {
    spray_range: 3,
    stream_range: 1,
    can_toggle: true,
    can_fill: true,
    fixed: None,
    stream: false,
};

fn nozzle_of(path: &str) -> Nozzle {
    let table: [(&str, Nozzle); 10] = [
        (
            "/obj/item/reagent_containers/spray/mister/janitor",
            Nozzle {
                can_toggle: false,
                fixed: Some(5),
                ..DEFAULT_NOZZLE
            },
        ),
        (
            "/obj/item/reagent_containers/spray/mister/pepperspray",
            Nozzle {
                can_toggle: false,
                fixed: Some(6),
                ..DEFAULT_NOZZLE
            },
        ),
        (
            MISTER,
            Nozzle {
                can_toggle: false,
                fixed: Some(3),
                ..DEFAULT_NOZZLE
            },
        ),
        (
            "/obj/item/reagent_containers/spray/chemsprayer/party",
            Nozzle {
                spray_range: 2,
                can_toggle: false,
                fixed: Some(2),
                ..DEFAULT_NOZZLE
            },
        ),
        (
            CHEMSPRAYER,
            Nozzle {
                spray_range: 4,
                stream_range: 7,
                stream: true,
                ..DEFAULT_NOZZLE
            },
        ),
        (
            "/obj/item/reagent_containers/spray/waterflower/cyborg",
            Nozzle {
                can_toggle: false,
                can_fill: false,
                fixed: Some(1),
                ..DEFAULT_NOZZLE
            },
        ),
        (
            "/obj/item/reagent_containers/spray/waterflower",
            Nozzle {
                can_toggle: false,
                fixed: Some(1),
                ..DEFAULT_NOZZLE
            },
        ),
        (
            "/obj/item/reagent_containers/spray/pepper",
            Nozzle {
                stream_range: 4,
                ..DEFAULT_NOZZLE
            },
        ),
        (
            "/obj/item/reagent_containers/spray/syndicate",
            Nozzle {
                spray_range: 4,
                stream_range: 2,
                ..DEFAULT_NOZZLE
            },
        ),
        (SPRAY, DEFAULT_NOZZLE),
    ];
    table
        .iter()
        .find(|(root, _)| is_under(path, root))
        .map_or(DEFAULT_NOZZLE, |&(_, n)| n)
}

/// `stream_mode` once toggled.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct StreamMode(pub bool);

/// `/obj/effect/decal/chempuff` on its way.
#[derive(Component, Clone, Debug)]
pub struct Puff {
    holder: Holder,
    target: Entity,
    last_target: TilePos,
    steps_left: u32,
    stream: bool,
    /// `wait_step`, seconds.
    wait: f64,
    next_at: f64,
    /// `end_life`: drifting out, gone at this time.
    gone_at: Option<f64>,
}

pub struct SpraysPlugin;

impl Plugin for SpraysPlugin {
    fn build(&self, app: &mut App) {
        // Before `chem::attack_self`, which also runs (`..()`: the transfer amount).
        app.add_click_handler(ClickPhase::AttackSelf, 1097, attack_self);
        app.add_systems(
            Update,
            (
                ranged_clicks.after(ResolveClicks),
                move_puffs.after(WorldSystems::Movement),
            ),
        );
    }
}

fn item_path(world: &World, item: Entity) -> String {
    world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

fn is_spray(world: &World, item: Entity) -> bool {
    is_under(&item_path(world, item), SPRAY)
}

/// `attack_self`, `toggle_stream_mode`. Never handles the click.
fn attack_self(world: &mut World, c: &crate::click_handlers::ClickCtx) -> bool {
    if !is_spray(world, c.target) {
        return false;
    }
    let nozzle = nozzle_of(&item_path(world, c.target));
    if !nozzle.can_toggle || nozzle.stream_range == nozzle.spray_range {
        return false;
    }
    let stream = !world
        .get::<StreamMode>(c.target)
        .map_or(nozzle.stream, |s| s.0);
    world.entity_mut(c.target).insert(StreamMode(stream));
    let mode = if stream {
        "струя"
    } else {
        "распыление"
    };
    tell(
        world,
        c.client,
        Span::Notice,
        format!("Вы переключаете насадку в режим «{mode}»."),
    );
    false
}

/// `on_reagent_change`: `clamp(floor(initial / average weight - extra reagents), 3, 5)`.
pub fn range_for(initial: u32, average_weight: f32, kinds: u32) -> u32 {
    // The epsilon keeps a float error of the average from losing a whole tile.
    let range = (initial as f32 / average_weight + 1e-4).floor() as i32 - (kinds as i32 - 1);
    range.clamp(3, 5) as u32
}

/// `current_range` of a spray now.
fn current_range(chem: &Chemistry, holder: &Holder, nozzle: Nozzle, stream: bool) -> u32 {
    if let Some(fixed) = nozzle.fixed {
        return fixed;
    }
    if stream {
        return nozzle.stream_range;
    }
    if holder.total_volume <= 0.0 || holder.reagents.is_empty() {
        return nozzle.spray_range;
    }
    let weighted: f32 = holder
        .reagents
        .iter()
        .map(|r| {
            let weight = if LIGHT_REAGENTS.iter().any(|p| chem.is_type(r.id, p)) {
                LIGHT_WEIGHT
            } else {
                1.0
            };
            weight * r.volume
        })
        .sum();
    range_for(
        nozzle.spray_range,
        weighted / holder.total_volume,
        holder.reagents.len() as u32,
    )
}

/// `wait_step`: `max(floor(2 + 3 / range), 2)` deciseconds, in seconds.
pub fn wait_secs(range: u32) -> f64 {
    let deciseconds = (2.0 + 3.0 / f64::from(range.max(1))).floor().max(2.0);
    deciseconds / 10.0
}

/// `interact_with_atom`: the click on an adjacent thing. True when the click was the spray's.
pub fn interact(world: &mut World, actor: Actor, target: Entity) -> bool {
    let item = actor.item.expect("a held spray");
    let path = path_of(world, target).unwrap_or_default();
    if SKIP.iter().any(|r| is_under(&path, r))
        || crate::storage::storage_of(world, target).is_some()
    {
        return false;
    }
    let Some(pos) = crate::explosions::turf_of(world, target) else {
        return false;
    };
    try_spray(world, actor.client, actor.mob, item, target, pos);
    true
}

/// `ranged_interact_with_atom`.
fn ranged_clicks(world: &mut World, mut cursor: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = cursor
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let held = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&h| is_spray(world, h));
        if let Some(held) = held {
            try_spray(
                world,
                click.client,
                click.mob,
                held,
                click.target,
                click.target_pos,
            );
        }
    }
}

/// `try_spray`.
fn try_spray(
    world: &mut World,
    client: Entity,
    mob: Entity,
    item: Entity,
    target: Entity,
    target_pos: TilePos,
) {
    let path = item_path(world, item);
    // `mister/try_spray`: nothing of the wearer's own by mistake.
    if is_under(&path, MISTER)
        && world
            .get::<Inventory>(mob)
            .is_some_and(|inv| inv.holds(target))
    {
        return;
    }
    let source = crate::watertank::reagent_source(world, item);
    let (Some(chem), Some(mut holder), Some(from)) = (
        world.get_resource::<Chem>().map(|c| c.0.clone()),
        world.get::<Reagents>(source).map(|r| r.0.clone()),
        crate::explosions::turf_of(world, mob),
    ) else {
        return;
    };
    let nozzle = nozzle_of(&path);
    let adjacent = from.chebyshev(target_pos).is_some_and(|d| d <= 1);
    if adjacent && nozzle.can_fill && refill(world, client, item, source, &mut holder, target) {
        return;
    }
    let amount = world.get::<TransferAmount>(item).map_or(5.0, |t| t.0);
    if holder.total_volume < amount {
        tell(world, client, Span::Warning, "Больше не осталось!");
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<NextMove>(mob).is_some_and(|n| now < n.0) {
        return;
    }
    if let Some(mut next) = world.get_mut::<NextMove>(mob) {
        next.0 = now + CLICK_CD_RANGE * 2.0;
    }
    let onto = adjacent
        && (world.get::<Density>(target).is_some() || world.get::<Health>(target).is_some());
    let stream = world.get::<StreamMode>(item).map_or(nozzle.stream, |s| s.0);
    let direction = crate::pull::dir_between(from, target_pos);
    let mut shots = vec![(target, target_pos)];
    if is_under(&path, CHEMSPRAYER)
        && !is_under(
            &path,
            "/obj/item/reagent_containers/spray/chemsprayer/party",
        )
    {
        // `chemsprayer/spray`: three puffs in a row across the direction.
        shots = crate::extinguisher::spread_targets(target_pos, direction, false)
            .into_iter()
            .filter_map(|p| turf_at(world, p).map(|t| (t, p)))
            .collect();
    }
    for (aim, aim_pos) in shots {
        if holder.total_volume < 1.0 {
            break;
        }
        let start = if onto && aim == target {
            target_pos
        } else {
            from
        };
        let range = current_range(&chem, &holder, nozzle, stream);
        launch(
            world,
            &chem,
            &mut holder,
            amount,
            stream,
            range,
            start,
            from,
            aim,
            aim_pos,
        );
    }
    world.entity_mut(source).insert(Reagents(holder));
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(from, "sound/effects/spray2.ogg", 50.0, true).extrarange(-6.0),
    );
    if let Some(d) = direction {
        crate::spacemove::newtonian_move(world, mob, d.reverse(), false, 1.0);
    }
}

/// `target` is a drainable, non-refillable tank: the bottle takes 50 u. True when it was one.
fn refill(
    world: &mut World,
    client: Entity,
    item: Entity,
    source: Entity,
    holder: &mut Holder,
    target: Entity,
) -> bool {
    let Some(mut tank) = world
        .get::<Reagents>(target)
        .map(|r| r.0.clone())
        .filter(|h| h.is_drainable() && !h.is_refillable())
    else {
        return false;
    };
    let (src, dst) = (
        crate::chem::name_of(world, target),
        crate::chem::name_of(world, item),
    );
    if tank.total_volume == 0.0 {
        tell(world, client, Span::Warning, format!("{src} пуст."));
    } else if holder.holder_full() {
        tell(world, client, Span::Warning, format!("{dst} полон."));
    } else if let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) {
        let moved = tank.trans_to(&chem, holder, 50.0, 1.0, None, false, false);
        world.entity_mut(target).insert(Reagents(tank));
        world.entity_mut(source).insert(Reagents(holder.clone()));
        tell(
            world,
            client,
            Span::Notice,
            format!(
                "Вы наполняете {dst} {} ед. содержимого {src}.",
                crate::chem::dm_num(moved)
            ),
        );
    }
    true
}

/// `spray`, `do_spray`: the puff, already moving (or landed when it starts on its target).
#[allow(clippy::too_many_arguments)]
fn launch(
    world: &mut World,
    chem: &Chemistry,
    holder: &mut Holder,
    amount: f32,
    stream: bool,
    range: u32,
    start: TilePos,
    from: TilePos,
    target: Entity,
    target_pos: TilePos,
) {
    let range = range.min(from.chebyshev(target_pos).unwrap_or(1)).max(1);
    let mut puff_holder = Holder::new(amount, 0, Owner::default());
    let multiplier = if stream { 1.0 } else { 1.0 / range as f32 };
    holder.trans_to(
        chem,
        &mut puff_holder,
        amount,
        multiplier,
        None,
        true,
        false,
    );
    info!(
        "a puff of {} u from {from:?} to {target_pos:?}, range {range}",
        puff_holder.total_volume
    );
    let color = ss13_reagents::mix_color(chem, &puff_holder.reagents);
    let Some(puff) = spawn_obj(world, CHEMPUFF, start) else {
        return;
    };
    if let Some(color) = color
        && let Some(mut look) = world.get_mut::<Appearance>(puff)
    {
        look.color = Some(color);
    }
    let wait = wait_secs(range);
    let now = world.resource::<Time>().elapsed_secs_f64();
    let landed = start == target_pos;
    if landed {
        spray_down_turf(world, puff, target_pos, &puff_holder, stream, false);
    }
    world.entity_mut(puff).insert(Puff {
        holder: puff_holder,
        target,
        last_target: target_pos,
        steps_left: range,
        stream,
        wait,
        next_at: now,
        gone_at: landed.then_some(now + 0.5),
    });
}

/// `PASSTABLE | PASSGRILLE`.
fn passes(world: &World, blocker: Entity) -> bool {
    let Some(path) = world.get::<ss13_mapload::DmObj>(blocker).map(|o| &o.path) else {
        return false;
    };
    is_under(path, "/obj/structure/table") || is_under(path, "/obj/structure/grille")
}

/// `spray_down_turf`: what is on the tile (not below the floor), then the turf.
fn spray_down_turf(
    world: &mut World,
    puff: Entity,
    pos: TilePos,
    holder: &Holder,
    stream: bool,
    last: bool,
) {
    let things: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            e != puff && world.get::<Turf>(e).is_none() && world.get::<HiddenUnderTile>(e).is_none()
        })
        .collect();
    for thing in things {
        if world.get_entity(thing).is_err() {
            continue;
        }
        if !stream {
            crate::janitor::expose_vapor(world, thing, holder, 1.0);
        } else if world.get::<Health>(thing).is_some() {
            let standing = !world.get::<BodyPosition>(thing).is_some_and(|b| b.lying());
            if crate::botany::produce::can_inject(world, thing) && (standing || last) {
                crate::janitor::expose_vapor(world, thing, holder, 1.0);
            }
        } else if last {
            crate::janitor::expose_vapor(world, thing, holder, 1.0);
        }
    }
    if (!stream || last)
        && let Some(turf) = turf_at(world, pos)
    {
        crate::janitor::expose_vapor(world, turf, holder, 1.0);
    }
}

/// `check_move`, `loop_ended`.
fn move_puffs(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query_filtered::<Entity, With<Puff>>();
    let mut list: Vec<Entity> = q.iter(world).collect();
    list.sort();
    for entity in list {
        let (Some(mut puff), Some(&pos)) = (
            world.get::<Puff>(entity).cloned(),
            world.get::<TilePos>(entity),
        ) else {
            continue;
        };
        if let Some(gone) = puff.gone_at {
            if now >= gone {
                world.despawn(entity);
            }
            continue;
        }
        if now < puff.next_at {
            continue;
        }
        let target = crate::explosions::turf_of(world, puff.target).unwrap_or(puff.last_target);
        puff.last_target = target;
        let Some(dir) = crate::pull::dir_between(pos, target).filter(|_| puff.steps_left > 0)
        else {
            puff.gone_at = Some(now + puff.wait);
            world.entity_mut(entity).insert(puff);
            continue;
        };
        match crate::pull::step_passing(world, entity, dir, passes) {
            Ok(()) => {
                puff.steps_left -= 1;
                puff.next_at = now + puff.wait;
                let last = puff.steps_left == 0;
                let at = pos.step(dir);
                let holder = puff.holder.clone();
                let stream = puff.stream;
                world.entity_mut(entity).insert(puff);
                spray_down_turf(world, entity, at, &holder, stream, last);
            }
            Err(_) => {
                puff.gone_at = Some(now + puff.wait);
                world.entity_mut(entity).insert(puff);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spray_range_follows_the_weight() {
        assert_eq!(range_for(3, 1.0, 1), 3);
        assert_eq!(range_for(3, 0.6, 1), 5, "space cleaner sprays further");
        assert_eq!(range_for(3, 0.6, 3), 3, "two extra reagents cost two tiles");
        assert_eq!(range_for(4, 0.6, 1), 5, "at most five");
    }

    #[test]
    fn puff_speed_follows_the_range() {
        assert_eq!(wait_secs(1), 0.5);
        assert_eq!(wait_secs(2), 0.3);
        assert_eq!(wait_secs(3), 0.3);
        assert_eq!(wait_secs(5), 0.2);
    }

    #[test]
    fn subtypes_override_the_nozzle() {
        let mister = "/obj/item/reagent_containers/spray/mister/janitor";
        assert_eq!(nozzle_of(mister).fixed, Some(5));
        assert_eq!(nozzle_of(SPRAY).fixed, None);
        assert!(nozzle_of(CHEMSPRAYER).stream);
        assert_eq!(
            nozzle_of("/obj/item/reagent_containers/spray/pepper").stream_range,
            4
        );
    }
}
