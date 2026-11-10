//! Barricades (server), after tg `/obj/structure/barricade` (`code/game/machinery/deployable.dm`)
//! and `/datum/element/can_barricade`.
//!
//! - Dense, breakable ([`crate::obj_damage`]) structures. A projectile passes an anchored one
//!   from an adjacent firer always, otherwise by its `proj_pass_rate` ([`stops_projectile`]).
//! - A welder repairs a metal one (4 s, 10 fuel, 20 integrity), a crowbar takes a wooden one
//!   apart (2 s, its planks). Destroyed, a wooden one leaves its planks.
//! - Planks on a door or a full-tile window: 5 s and 2 planks nail a crude barricade there.
//! - The barrier grenade (`crate::grenades`) deploys security barriers.
// shortcut: no sandbag climbing or elevation (`climbable`, `elevation`), no diagonal-from-a-
// barricade passage (`locate(barricade) in get_turf(mover)` for a mover already standing on one,
// which only a grenade barrier built onto someone can cause), no armour of the security barrier
// against projectiles. Add with climbing and per-flag armor.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};

use crate::combat::CombatMode;
use crate::construction::{Job, spawn_obj, spawn_stack, tile_blocked};
use crate::obj_damage::{Integrity, anchored, deconstruct as obj_deconstruct};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, ToolKind, stack_amount, tell, tool_of, use_stack, use_tool};

pub const BARRICADE: &str = "/obj/structure/barricade";
const WOODEN: &str = "/obj/structure/barricade/wooden";
const CRUDE: &str = "/obj/structure/barricade/wooden/crude";
const SANDBAGS: &str = "/obj/structure/barricade/sandbags";
const SECURITY_BARRIER: &str = "/obj/structure/barricade/security";
const WOOD: &str = "/obj/item/stack/sheet/mineral/wood";
const DOOR: &str = "/obj/machinery/door";
const WINDOW: &str = "/obj/structure/window";

const REPAIR_DS: f32 = 40.0;
const REPAIR_FUEL: u32 = 10;
const REPAIR_AMOUNT: f32 = 20.0;
const PRY_DS: f32 = 20.0;
const PLANK_DS: f32 = 50.0;
/// `PLANK_BARRICADE_AMOUNT`.
const PLANKS: u32 = 2;
const HAMMER_SOUND: &str = "sound/items/hammering_wood.ogg";

/// The timed steps of this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarricadeJob {
    Repair,
    Pry,
    /// Planks nailed over the target (`can_barricade/barricade`).
    Nail {
        planks: Entity,
    },
}

/// `proj_pass_rate`, `None` for anything else.
pub fn proj_pass_rate(path: &str) -> Option<u32> {
    if !is_under(path, BARRICADE) {
        None
    } else if is_under(path, SANDBAGS) || is_under(path, SECURITY_BARRIER) {
        Some(20)
    } else if is_under(path, CRUDE) {
        Some(65)
    } else {
        Some(50)
    }
}

fn is_metal(path: &str) -> bool {
    is_under(path, BARRICADE) && !is_under(path, WOODEN) && !is_under(path, SANDBAGS)
}

/// `/obj/structure/barricade/wooden` `drop_amount`.
fn drop_amount(path: &str) -> u32 {
    if is_under(path, CRUDE) { 1 } else { 3 }
}

/// tg `CanAllowThrough` for projectile `shot` fired by `firer` from next to `barricade` or not:
/// whether this one blocks it. `select_target` is read-only, so the `prob` is a hash of the
/// pair: the same shot meets the same barricade the same way.
pub fn stops_projectile(world: &World, barricade: Entity, shot: Entity, firer: Entity) -> bool {
    let Some(rate) = world
        .get::<DmObj>(barricade)
        .and_then(|o| proj_pass_rate(&o.path))
    else {
        return false;
    };
    if !anchored(world, barricade) {
        return false;
    }
    if let (Some(&at), Some(&from)) = (world.get::<TilePos>(barricade), world.get::<TilePos>(firer))
        && at.chebyshev(from).is_some_and(|d| d <= 1)
    {
        return false;
    }
    let mixed = shot.to_bits() ^ barricade.to_bits().rotate_left(17);
    let roll = (mixed.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 57) % 100;
    roll >= u64::from(rate)
}

/// `welder_act`, `crowbar_act`, `can_barricade/on_start_barricade`. True when the click was
/// theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    let tool = tool_of(world, held).map(|t| t.0);
    let path = world.get::<DmObj>(target).map(|o| o.path.clone());
    if let Some(path) = path.as_deref().filter(|p| is_under(p, BARRICADE)) {
        return match tool {
            // Combat mode is a hit (`ITEM_INTERACT_SKIP_TO_ATTACK`).
            Some(ToolKind::Welder) if !combat => repair(world, actor, target, path),
            Some(ToolKind::Crowbar) if is_under(path, WOODEN) => {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете разбирать баррикаду...",
                );
                use_tool(
                    world,
                    actor,
                    target,
                    PRY_DS,
                    0,
                    Job::Barricade(BarricadeJob::Pry),
                );
                true
            }
            _ => false,
        };
    }
    let plank = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, WOOD));
    if combat || !plank || !can_barricade(world, target) {
        return false;
    }
    if stack_amount(world, held) < PLANKS {
        tell(world, client, Span::Warning, "Нужно 2 доски!");
        return true;
    }
    if let Some(&pos) = world.get::<TilePos>(target) {
        playsound(world, PlaySound::new(pos, HAMMER_SOUND, 50.0, true));
    }
    tell(
        world,
        client,
        Span::Notice,
        "Вы начинаете заколачивать проход...",
    );
    crate::tools::do_after(
        world,
        actor,
        target,
        PLANK_DS,
        0,
        Job::Barricade(BarricadeJob::Nail { planks: held }),
    );
    true
}

/// The atoms with `/datum/element/can_barricade`: doors and full-tile windows.
fn can_barricade(world: &World, target: Entity) -> bool {
    world
        .get::<DmObj>(target)
        .is_some_and(|o| is_under(&o.path, DOOR) || (is_under(&o.path, WINDOW) && o.fulltile))
}

fn repair(world: &mut World, actor: Actor, target: Entity, path: &str) -> bool {
    if !is_metal(path) {
        return true;
    }
    let Some(max) = world.get::<DmObj>(target).map(|o| o.max_integrity) else {
        return true;
    };
    if world
        .get::<Integrity>(target)
        .is_some_and(|i| i.current >= max)
    {
        tell(world, actor.client, Span::Notice, "Прочность уже полная!");
        return true;
    }
    tell(
        world,
        actor.client,
        Span::Notice,
        "Вы начинаете ремонтировать баррикаду...",
    );
    use_tool(
        world,
        actor,
        target,
        REPAIR_DS,
        REPAIR_FUEL,
        Job::Barricade(BarricadeJob::Repair),
    );
    true
}

/// End of a [`BarricadeJob`]; the state is checked again here.
pub fn complete(world: &mut World, action: &crate::tools::DoAfter, job: BarricadeJob) {
    let target = action.target;
    match job {
        BarricadeJob::Repair => {
            let max = world.get::<DmObj>(target).map_or(0.0, |o| o.max_integrity);
            if let Some(mut integrity) = world.get_mut::<Integrity>(target) {
                integrity.current = (integrity.current + REPAIR_AMOUNT).min(max);
            }
            tell(world, action.client, Span::Notice, "Отремонтировано.");
        }
        BarricadeJob::Pry => {
            if world.get::<DmObj>(target).is_none() {
                return;
            }
            tell(world, action.client, Span::Notice, "Баррикада разобрана.");
            // The planks come from its debris, as when it is destroyed.
            obj_deconstruct(world, target, true);
        }
        BarricadeJob::Nail { planks } => {
            let pos = action.target_pos;
            let crude_there = world
                .resource::<ss13_world::SpatialIndex>()
                .on_tile(pos)
                .iter()
                .any(|&e| {
                    world
                        .get::<DmObj>(e)
                        .is_some_and(|o| is_under(&o.path, CRUDE))
                });
            if crude_there || !use_stack(world, planks, PLANKS) {
                return;
            }
            spawn_obj(world, CRUDE, pos);
            tell(world, action.client, Span::Notice, "Проход заколочен.");
        }
    }
}

/// `make_debris` of the wooden ones. True when the object was a barricade.
pub fn deconstruct(world: &mut World, obj: &DmObj, pos: TilePos) -> bool {
    if !is_under(&obj.path, BARRICADE) {
        return false;
    }
    if is_under(&obj.path, WOODEN) {
        spawn_stack(world, WOOD, pos, drop_amount(&obj.path));
    }
    true
}

/// `barrier/detonate`: the tiles a barrier grenade of `mode` fills, the center always.
pub fn barrier_tiles(pos: TilePos, mode: BarrierMode) -> Vec<TilePos> {
    let sides = match mode {
        BarrierMode::Single => None,
        BarrierMode::Vertical => Some((Dir::North, Dir::South)),
        BarrierMode::Horizontal => Some((Dir::East, Dir::West)),
    };
    let mut tiles = vec![pos];
    if let Some((a, b)) = sides {
        tiles.push(pos.step(a));
        tiles.push(pos.step(b));
    }
    tiles
}

/// `SINGLE`, `VERTICAL`, `HORIZONTAL`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarrierMode {
    #[default]
    Single,
    Vertical,
    Horizontal,
}

impl BarrierMode {
    /// `toggle_mode`'s cycle.
    pub fn next(self) -> Self {
        match self {
            Self::Single => Self::Vertical,
            Self::Vertical => Self::Horizontal,
            Self::Horizontal => Self::Single,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Single => "одиночный",
            Self::Vertical => "вертикальный",
            Self::Horizontal => "горизонтальный",
        }
    }
}

/// `grenade/barrier/toggle_mode` (Alt-click). True when `item` is a barrier grenade.
pub fn toggle_barrier(world: &mut World, client: Entity, item: Entity) -> bool {
    if !crate::grenades::is_barrier(world, item) {
        return false;
    }
    let mode = world
        .get::<BarrierMode>(item)
        .copied()
        .unwrap_or_default()
        .next();
    world.entity_mut(item).insert(mode);
    tell(
        world,
        client,
        Span::Notice,
        format!("Режим гранаты: {}.", mode.name()),
    );
    true
}

/// `barrier/detonate`: security barriers on the free tiles of its mode.
pub fn deploy(world: &mut World, item: Entity, pos: TilePos) {
    let mode = world.get::<BarrierMode>(item).copied().unwrap_or_default();
    let tiles = barrier_tiles(pos, mode);
    // The center gets one whatever stands there; the sides only when free.
    for (i, tile) in tiles.into_iter().enumerate() {
        if i == 0 || !tile_blocked(world, tile) {
            spawn_obj(world, SECURITY_BARRIER, tile);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_rates_follow_the_type_tree() {
        assert_eq!(proj_pass_rate("/obj/structure/barricade"), Some(50));
        assert_eq!(proj_pass_rate("/obj/structure/barricade/wooden"), Some(50));
        assert_eq!(
            proj_pass_rate("/obj/structure/barricade/wooden/crude"),
            Some(65)
        );
        assert_eq!(
            proj_pass_rate("/obj/structure/barricade/wooden/crude/snow"),
            Some(65)
        );
        assert_eq!(
            proj_pass_rate("/obj/structure/barricade/sandbags"),
            Some(20)
        );
        assert_eq!(
            proj_pass_rate("/obj/structure/barricade/security"),
            Some(20)
        );
        assert_eq!(proj_pass_rate("/obj/structure/table"), None);
    }

    #[test]
    fn metal_is_neither_wood_nor_sand() {
        assert!(is_metal("/obj/structure/barricade/security"));
        assert!(!is_metal("/obj/structure/barricade/wooden/crude"));
        assert!(!is_metal("/obj/structure/barricade/sandbags"));
    }

    #[test]
    fn modes_cycle_and_spread_across_their_axis() {
        assert_eq!(BarrierMode::Single.next(), BarrierMode::Vertical);
        assert_eq!(BarrierMode::Vertical.next(), BarrierMode::Horizontal);
        assert_eq!(BarrierMode::Horizontal.next(), BarrierMode::Single);
        let center = TilePos::new(5, 5, 1);
        assert_eq!(barrier_tiles(center, BarrierMode::Single), vec![center]);
        assert_eq!(
            barrier_tiles(center, BarrierMode::Vertical),
            vec![center, center.step(Dir::North), center.step(Dir::South)]
        );
        assert_eq!(
            barrier_tiles(center, BarrierMode::Horizontal),
            vec![center, center.step(Dir::East), center.step(Dir::West)]
        );
    }

    #[test]
    fn a_crude_plank_barricade_drops_one_plank() {
        assert_eq!(drop_amount("/obj/structure/barricade/wooden/crude"), 1);
        assert_eq!(drop_amount("/obj/structure/barricade/wooden"), 3);
    }
}
