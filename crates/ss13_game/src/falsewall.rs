//! False walls (server), after tg `/obj/structure/falsewall`
//! (`code/game/objects/structures/false_walls.dm`): a wall-looking structure a hand opens and
//! closes like a door (0.5 s of `-opening`/`-closing`, then it stops blocking movement, sight
//! and air: `can_atmos_pass = ATMOS_PASS_DENSITY`), refused while a mob stands in the open one.
//! A screwdriver on the closed one on a floor tightens it into its `walltype`; a lit welder
//! (the reinforced one: wirecutters) takes it apart into a displaced girder and its
//! `mineral_amount` of `mineral`. Built from 2 sheets on a displaced girder
//! (`crate::construction`, `uses_girder_wall_recipes`).
// shortcut: no leaning on it (`leanable`), no examine disguise text, no uranium radiation.

use bevy::prelude::*;
use ss13_mapload::smooth::DmSmooth;
use ss13_mapload::{DmAtom, DmObj, IconIndex, Opaque};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_world::{Density, SpatialIndex};

use crate::health::Health;
use crate::obj_damage::is_under;
use crate::tools::{Actor, ToolKind, tell, tool_usable};

pub const FALSEWALL: &str = "/obj/structure/falsewall";
pub const R_FALSEWALL: &str = "/obj/structure/falsewall/reinforced";
const DISPLACED_GIRDER: &str = "/obj/structure/girder/displaced";
const FLOOR: &str = "/turf/open/floor";
/// `addtimer(toggle_open, 0.5 SECONDS)`.
const TOGGLE_SECS: f64 = 0.5;

/// A false wall that is open or moving: the smoothing flags it had closed.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FalseWallUnsmoothed(pub u8);

/// tg `opening`: the wall toggles at `at`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FalseWallMoving {
    pub at: f64,
}

fn is_falsewall(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, FALSEWALL))
}

/// tg `attack_hand`: start opening or closing it. True when `target` is a false wall.
pub fn attack_hand(world: &mut World, target: Entity) -> bool {
    if !is_falsewall(world, target) {
        return false;
    }
    if world.get::<FalseWallMoving>(target).is_some() {
        return true;
    }
    let dense = world.get::<Density>(target).is_some();
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    // "Stop people from using this as a shield".
    let blocked = !dense
        && world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .any(|&e| world.get::<Health>(e).is_some());
    if blocked {
        return true;
    }
    let at = world.resource::<Time>().elapsed_secs_f64() + TOGGLE_SECS;
    unsmooth(world, target);
    world.entity_mut(target).insert(FalseWallMoving { at });
    show(world, target, if dense { "opening" } else { "closing" });
    true
}

/// Its smoothing off (an open or moving false wall draws its own states).
fn unsmooth(world: &mut World, wall: Entity) {
    if world.get::<FalseWallUnsmoothed>(wall).is_some() {
        return;
    }
    let flags = world.get::<DmSmooth>(wall).map_or(0, |s| s.flags);
    world.entity_mut(wall).insert(FalseWallUnsmoothed(flags));
    if let Some(mut smooth) = world.get_mut::<DmSmooth>(wall) {
        smooth.flags = 0;
    }
}

/// `icon = initial(icon)`, `icon_state = "<base_icon_state>-<suffix>"`.
fn show(world: &mut World, wall: Entity, suffix: &str) {
    let Some(icon) = world.get::<DmAtom>(wall).map(|a| a.icon.clone()) else {
        return;
    };
    let base = world
        .get::<DmSmooth>(wall)
        .map(|s| s.base_icon_state.clone())
        .unwrap_or_default();
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&icon, &format!("{base}-{suffix}")));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(wall))
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
}

/// tg `toggle_open`: density and opacity flip; a closed one smooths again.
pub fn toggle_open(world: &mut World, wall: Entity) {
    world.entity_mut(wall).remove::<FalseWallMoving>();
    let Some(&pos) = world.get::<TilePos>(wall) else {
        return;
    };
    if world.get::<Density>(wall).is_some() {
        world.entity_mut(wall).remove::<(Density, Opaque)>();
        show(world, wall, "open");
        return;
    }
    world.entity_mut(wall).insert((Density, Opaque));
    if let Some(FalseWallUnsmoothed(flags)) = world.get::<FalseWallUnsmoothed>(wall).copied() {
        world.entity_mut(wall).remove::<FalseWallUnsmoothed>();
        if let Some(mut smooth) = world.get_mut::<DmSmooth>(wall) {
            smooth.flags = flags;
        }
    }
    crate::construction::tile_changed(world, pos);
}

pub(crate) fn finish_moving(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &FalseWallMoving)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, m)| now >= m.at)
        .map(|(e, _)| e)
        .collect();
    for wall in due {
        toggle_open(world, wall);
    }
}

/// tg `tool_act` and the `*_act` of a false wall. True when handled.
pub fn tool_act(world: &mut World, actor: Actor, kind: ToolKind, wall: Entity) -> bool {
    let Some(obj) = world.get::<DmObj>(wall).cloned() else {
        return false;
    };
    if !is_under(&obj.path, FALSEWALL) {
        return false;
    }
    let client = actor.client;
    if world.get::<FalseWallMoving>(wall).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "Подождите, пока стена не перестанет двигаться!",
        );
        return true;
    }
    let reinforced = is_under(&obj.path, R_FALSEWALL);
    match kind {
        ToolKind::Screwdriver => {
            screwdriver_act(world, client, wall, &obj);
            true
        }
        ToolKind::Welder if !reinforced => {
            // `use_tool(src, user, 0 SECONDS)`: a lit welder, no fuel used.
            let item = actor.item.expect("a tool act has an item");
            if !tool_usable(world, item, 0, 0.0) {
                let text = crate::tools::unusable_message(world, item, 0);
                tell(world, client, Span::Warning, text);
                return true;
            }
            dismantle(world, client, wall);
            true
        }
        ToolKind::Wirecutters if reinforced => {
            dismantle(world, client, wall);
            true
        }
        _ => false,
    }
}

/// `screwdriver_act`: the closed false wall on a floor becomes its real wall
/// (`ChangeToWall`).
fn screwdriver_act(world: &mut World, client: Entity, wall: Entity, obj: &DmObj) {
    if world.get::<Density>(wall).is_none() {
        tell(
            world,
            client,
            Span::Warning,
            "Не дотянуться, сначала закройте её!",
        );
        return;
    }
    let Some(&pos) = world.get::<TilePos>(wall) else {
        return;
    };
    let turf = crate::construction::turf_at(world, pos);
    if turf.is_some_and(|t| world.get::<Density>(t).is_some()) {
        tell(world, client, Span::Warning, "Стена заблокирована!");
        return;
    }
    let floor = turf
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| is_under(&p, FLOOR));
    if !floor {
        tell(
            world,
            client,
            Span::Warning,
            "Болты стены можно затянуть только на полу!",
        );
        return;
    }
    if obj.wall_type.is_empty() {
        return;
    }
    world.despawn(wall);
    if crate::construction::place_on_top(world, pos, &obj.wall_type, false).is_none() {
        // No prototype of the wall: the false wall stays.
        crate::construction::spawn_obj(world, &obj.path, pos);
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        "Вы затягиваете болты на стене.",
    );
}

/// `dismantle` -> `deconstruct(disassembled = TRUE)`: a displaced girder and its sheets.
pub fn dismantle(world: &mut World, client: Entity, wall: Entity) {
    let (Some(obj), Some(&pos)) = (
        world.get::<DmObj>(wall).cloned(),
        world.get::<TilePos>(wall),
    ) else {
        return;
    };
    tell(world, client, Span::Notice, "Вы разбираете фальшстену.");
    world.despawn(wall);
    crate::construction::spawn_obj(world, DISPLACED_GIRDER, pos);
    crate::construction::spawn_stack(world, &obj.mineral, pos, obj.mineral_amount);
    crate::construction::tile_changed(world, pos);
}

#[cfg(test)]
mod tests {
    use ss13_net::chat::server::SendChat;
    use ss13_world::WorldPlugin;

    use super::*;

    /// A hand opens it after the delay (no density, no opacity, no smoothing) and closes it
    /// back; a mob in the open wall keeps it open.
    #[test]
    fn opens_and_closes() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, WorldPlugin))
            .add_message::<SendChat>();
        let world = app.world_mut();
        world
            .resource_mut::<SpatialIndex>()
            .create_level(1, 5, 5)
            .unwrap();
        let pos = TilePos::new(2, 2, 1);
        let wall = world
            .spawn((
                DmObj {
                    path: FALSEWALL.into(),
                    ..default()
                },
                DmSmooth {
                    flags: 1,
                    ..default()
                },
                Density,
                Opaque,
                pos,
            ))
            .id();
        assert!(attack_hand(world, wall));
        assert!(world.get::<FalseWallMoving>(wall).is_some());
        assert_eq!(world.get::<DmSmooth>(wall).unwrap().flags, 0);
        assert!(attack_hand(world, wall), "moving: ignored");
        toggle_open(world, wall);
        assert!(world.get::<Density>(wall).is_none());
        assert!(world.get::<Opaque>(wall).is_none());
        let mob = world.spawn((Health::default(), pos)).id();
        attack_hand(world, wall);
        assert!(
            world.get::<FalseWallMoving>(wall).is_none(),
            "a mob inside keeps it open"
        );
        world.despawn(mob);
        attack_hand(world, wall);
        toggle_open(world, wall);
        assert!(world.get::<Density>(wall).is_some());
        assert!(world.get::<Opaque>(wall).is_some());
        assert_eq!(world.get::<DmSmooth>(wall).unwrap().flags, 1);
    }
}
