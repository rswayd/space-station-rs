//! Landmines, after tg `/obj/effect/mine` (`code/game/objects/effects/mines.dm`): whatever
//! steps on an armed mine presses its plate (`*click*`); the mine goes off when that one steps
//! off it (`on_entered` / `on_exited` -> `triggermine`), with sparks, then is gone.
//!
//! - explosive (and its light, incendiary and blinding kinds): `explosion()` with the subtype's
//!   ranges;
//! - stun: `Paralyze(8 s)` on the one who set it off, if still next to it;
//! - gas (oxygen, plasma, N2O, chilled vapor): `atmos_spawn_air` of the subtype's gas;
//! - sound (honkblaster, bwoink): a loud sound.
//!
//! Flying and phasing things do not press the plate (only the one already on it, who took off,
//! still sets it off); effects, anchored things and ghosts never do.
// shortcut: no shrapnel mines (`pellet_cloud` blast pellets: they go off with sparks only), no
// kick mine's client kick (admin fun), no `arm_delay` (only the minespawner's AP mine has it,
// and the minespawner is not ported), no detonation by damage (`take_damage`: effects take no
// damage here), no border-object pass-over, no examine line for the pressed plate. A plate
// pressed by something that left without a step (teleport, throw) is released, not set off
// (tg `on_exited` fires on `forceMove` too).
//
// Cost: per step, one look at the two tiles' contents.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::Moved;

use crate::health::Health;
use crate::obj_damage::is_under;

pub const MINE: &str = "/obj/effect/mine";
const EFFECT: &str = "/obj/effect";
/// `COMBAT_MESSAGE_RANGE`-free `visible_message`: `DEFAULT_MESSAGE_RANGE`.
const MESSAGE_RANGE: u32 = 7;
const CLICK_SOUND: &str = "sound/machines/click.ogg";
/// `stun_time = 80` (deciseconds).
const STUN_SECS: f32 = 8.0;

/// tg `foot_on_mine`: who is standing on the pressure plate.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinePlate(pub Entity);

/// What a mine does when it goes off (`mineEffect`).
#[derive(Clone, Debug, PartialEq)]
pub enum MineEffect {
    /// `explosive`: `(devastation, heavy, light, flame, flash)`.
    Explosive([i32; 5]),
    Stun,
    /// `gas`: the `atmos_spawn_air` text.
    Gas(&'static str),
    Sound(&'static str),
    /// `dummy mine`, `kickmine`, `shrapnel`: nothing here.
    None,
}

/// The effect of the mine type `path` (most specific subtype first).
pub fn mine_effect(path: &str) -> MineEffect {
    const EXPLOSIVE: [(&str, [i32; 5]); 4] = [
        ("/obj/effect/mine/explosive/light", [0, 0, 3, 0, 2]),
        ("/obj/effect/mine/explosive/flame", [0, 0, 1, 3, 3]),
        ("/obj/effect/mine/explosive/flash", [0, 0, 1, 0, 6]),
        ("/obj/effect/mine/explosive", [0, 1, 2, 0, 3]),
    ];
    const GAS: [(&str, &str); 4] = [
        ("/obj/effect/mine/gas/plasma", "plasma=360"),
        ("/obj/effect/mine/gas/n2o", "n2o=360"),
        ("/obj/effect/mine/gas/water_vapor", "water_vapor=500"),
        ("/obj/effect/mine/gas", "o2=360"),
    ];
    if let Some((_, r)) = EXPLOSIVE.iter().find(|(root, _)| is_under(path, root)) {
        return MineEffect::Explosive(*r);
    }
    if let Some((_, gas)) = GAS.iter().find(|(root, _)| is_under(path, root)) {
        return MineEffect::Gas(gas);
    }
    if is_under(path, "/obj/effect/mine/stun") {
        return MineEffect::Stun;
    }
    if is_under(path, "/obj/effect/mine/sound/bwoink") {
        return MineEffect::Sound("sound/effects/adminhelp.ogg");
    }
    if is_under(path, "/obj/effect/mine/sound") {
        return MineEffect::Sound("sound/items/bikehorn.ogg");
    }
    MineEffect::None
}

/// Mapped mines are `/obj/effect`s: their path comes in [`ss13_mapload::DmTrigger`] (no `DmObj`).
fn mine_path(world: &World, e: Entity) -> Option<&str> {
    world
        .get::<ss13_mapload::DmTrigger>(e)
        .map(|t| t.path.as_str())
        .or_else(|| world.get::<DmObj>(e).map(|o| o.path.as_str()))
        .filter(|p| is_under(p, MINE))
}

fn mines_on(world: &World, pos: TilePos) -> Vec<Entity> {
    super::on_tile(world, pos)
        .into_iter()
        .filter(|&e| mine_path(world, e).is_some())
        .collect()
}

/// Whoever is on the plate, if still there (a deleted or moved-away presser frees it).
fn plate(world: &World, mine: Entity) -> Option<Entity> {
    let presser = world.get::<MinePlate>(mine)?.0;
    let here = world.get::<TilePos>(mine)?;
    (world.get::<TilePos>(presser) == Some(here)).then_some(presser)
}

/// tg `can_trigger(on_who)`.
fn can_trigger(world: &World, who: Entity, presser: Option<Entity>) -> bool {
    let Ok(entity) = world.get_entity(who) else {
        return false;
    };
    let path = entity
        .get::<DmObj>()
        .map(|o| (o.path.as_str(), o.anchored))
        .or_else(|| entity.get::<DmItem>().map(|i| (i.path.as_str(), false)));
    if let Some((path, anchored)) = path {
        let anchored = entity
            .get::<crate::obj_damage::Anchored>()
            .map_or(anchored, |a| a.0);
        if is_under(path, EFFECT) || anchored {
            return false;
        }
    } else if !entity.contains::<Health>() || entity.contains::<crate::ghost::Ghost>() {
        // Not living: ghosts, eyes.
        return false;
    }
    if crate::multiz::flies(world, who) {
        return presser == Some(who);
    }
    true
}

/// `on_entered` and `on_exited` of the mines for every step.
pub(super) fn step_on_mines(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        for mine in mines_on(world, m.from) {
            exited(world, mine, m.entity);
        }
        for mine in mines_on(world, m.to) {
            entered(world, mine, m.entity);
        }
    }
}

/// `on_entered`: the first one on the plate presses it.
fn entered(world: &mut World, mine: Entity, who: Entity) {
    if world.get_entity(mine).is_err() || plate(world, mine).is_some() {
        return;
    }
    if !can_trigger(world, who, None) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mine) else {
        return;
    };
    super::tell_near(world, pos, MESSAGE_RANGE, Span::Danger, "*щёлк*");
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, CLICK_SOUND, 60.0, true),
    );
    world.entity_mut(mine).insert(MinePlate(who));
}

/// `on_exited`: the one on the plate (or anyone, if nobody pressed it) stepping off sets it off.
fn exited(world: &mut World, mine: Entity, who: Entity) {
    if world.get_entity(mine).is_err() {
        return;
    }
    // The presser has already left the tile when this runs: read the plate unvalidated.
    let presser = world.get::<MinePlate>(mine).map(|p| p.0);
    if !can_trigger(world, who, presser) {
        return;
    }
    // `foot_on_mine && !IS_WEAKREF_OF(gone, foot_on_mine)`: someone else still stands on it.
    if plate(world, mine).is_some() {
        return;
    }
    trigger(world, mine, Some(who));
}

/// tg `triggermine(triggerer)`.
pub fn trigger(world: &mut World, mine: Entity, triggerer: Option<Entity>) {
    let Some(path) = mine_path(world, mine).map(str::to_owned) else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(mine) else {
        return;
    };
    let name = crate::examine::name_of(world, mine);
    let text = match triggerer {
        Some(t) => format!("{} задевает {name}!", crate::examine::name_of(world, t)),
        None => format!("{name} срабатывает!"),
    };
    super::tell_near(world, pos, MESSAGE_RANGE, Span::Danger, &text);
    super::particles::do_sparks(world, 3, true, pos);
    // `qdel(src)` before the effect here, so the blast cannot set the same mine off again.
    world.despawn(mine);
    match mine_effect(&path) {
        MineEffect::Explosive([dev, heavy, light, flame, flash]) => {
            let ex = crate::explosions::Explosion {
                cause: path,
                ..crate::explosions::Explosion::new(dev, heavy, light)
                    .flame(flame)
                    .flash(flash)
            };
            crate::explosions::explosion(world, pos, ex);
        }
        MineEffect::Stun => {
            // `isliving(victim) && Adjacent(victim)`.
            let adjacent = triggerer.is_some_and(|t| {
                world.get::<Health>(t).is_some()
                    && world.get::<TilePos>(t).is_some_and(|p| {
                        p.z == pos.z && (p.x - pos.x).abs() <= 1 && (p.y - pos.y).abs() <= 1
                    })
            });
            if let Some(t) = triggerer.filter(|_| adjacent) {
                crate::status_effects::apply(
                    world,
                    t,
                    crate::status_effects::Incap::Paralyze,
                    STUN_SECS,
                );
            }
        }
        MineEffect::Gas(gas) => {
            if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
                let _ = atmos.spawn_air(pos, gas);
            }
        }
        MineEffect::Sound(sound) => {
            crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, 100.0, true));
        }
        MineEffect::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mine_effects_follow_the_subtype() {
        assert_eq!(
            mine_effect("/obj/effect/mine/explosive"),
            MineEffect::Explosive([0, 1, 2, 0, 3])
        );
        assert_eq!(
            mine_effect("/obj/effect/mine/explosive/flame"),
            MineEffect::Explosive([0, 0, 1, 3, 3])
        );
        assert_eq!(
            mine_effect("/obj/effect/mine/gas/water_vapor"),
            MineEffect::Gas("water_vapor=500")
        );
        assert_eq!(
            mine_effect("/obj/effect/mine/gas"),
            MineEffect::Gas("o2=360")
        );
        assert_eq!(mine_effect("/obj/effect/mine/stun"), MineEffect::Stun);
        assert_eq!(
            mine_effect("/obj/effect/mine/sound/bwoink"),
            MineEffect::Sound("sound/effects/adminhelp.ogg")
        );
        assert_eq!(
            mine_effect("/obj/effect/mine/shrapnel/sting"),
            MineEffect::None
        );
        // The gas strings parse.
        for gas in ["plasma=360", "n2o=360", "water_vapor=500", "o2=360"] {
            assert!(ss13_atmos::GasMixture::parse(gas).is_ok(), "{gas}");
        }
    }
}
