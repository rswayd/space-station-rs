//! Frozen objects (server), after tg `/datum/element/frozen`
//! (`code/datums/elements/frozen.dm`) and `/turf/open/proc/freeze_turf`: vapor condensing on a
//! very cold tile (`crate::janitor` condensation) freezes every object on it that is not
//! `FREEZE_PROOF`. A frozen object is named "замороженный ...", tinted ice-blue and 25 alpha
//! fainter; thrown, it shatters where it lands or on what it hits unless caught ("... разбивается
//! вдребезги!", gone without debris); moved onto an open tile at 0 °C or warmer it thaws.
// shortcut: the freon colour matrix is a multiply tint; frozen organs do not stop decaying
// (`ORGAN_FROZEN`); a shattered object is deleted without its `deconstruct` (tanks keep their
// gas); cold mobs on the tile do not get the freon status (status effects).

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use crate::inventory::CarriedBy;
use crate::storage::StoredIn;

/// `resistance_flags` `FREEZE_PROOF`.
pub const FREEZE_PROOF: u32 = 1 << 7;
/// `T0C`.
pub const T0C: f64 = 273.15;
/// `GLOB.freon_color_matrix` applied to a mid grey, as a multiply tint.
const FREON_TINT: [u8; 3] = [0xA0, 0xD8, 0xE8];
const PREFIX: &str = "замороженный ";

/// `TRAIT_FROZEN`: what the object looked like before.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Frozen {
    pub color: Option<[u8; 4]>,
}

/// `freeze_turf`: every object on `pos` freezes.
pub fn freeze_turf(world: &mut World, pos: TilePos) {
    for thing in super::on_tile(world, pos) {
        freeze(world, thing);
    }
}

fn resistance(world: &World, e: Entity) -> Option<u32> {
    world
        .get::<DmObj>(e)
        .map(|o| o.resistance_flags)
        .or_else(|| world.get::<DmItem>(e).map(|i| i.resistance_flags))
}

/// `AddElement(/datum/element/frozen)`: false when `e` is no object, freeze-proof or already
/// frozen (`ELEMENT_INCOMPATIBLE`).
pub fn freeze(world: &mut World, e: Entity) -> bool {
    let Some(flags) = resistance(world, e) else {
        return false;
    };
    if flags & FREEZE_PROOF != 0 || world.get::<Frozen>(e).is_some() {
        return false;
    }
    let color = world.get::<Appearance>(e).and_then(|a| a.color);
    world.entity_mut(e).insert(Frozen { color });
    if let Some(mut appearance) = world.get_mut::<Appearance>(e) {
        let alpha = color.map_or(255, |c| c[3]).saturating_sub(25);
        let [r, g, b] = FREON_TINT;
        appearance.color = Some([r, g, b, alpha]);
    }
    if let Some(mut name) = world.get_mut::<DmName>(e) {
        name.name = format!("{PREFIX}{}", name.name);
    } else if let Some(mut name) = world.get_mut::<Name>(e) {
        let new = format!("{PREFIX}{}", name.as_str());
        name.set(new);
    }
    true
}

/// `Detach`: the old name, colour and alpha.
pub fn unfreeze(world: &mut World, e: Entity) {
    let Some(frozen) = world.get::<Frozen>(e).copied() else {
        return;
    };
    world.entity_mut(e).remove::<Frozen>();
    if let Some(mut appearance) = world.get_mut::<Appearance>(e) {
        appearance.color = frozen.color;
    }
    if let Some(mut name) = world.get_mut::<DmName>(e) {
        name.name = name.name.replacen(PREFIX, "", 1);
    } else if let Some(mut name) = world.get_mut::<Name>(e) {
        let new = name.as_str().replacen(PREFIX, "", 1);
        name.set(new);
    }
}

/// `shatter_on_landed` / `shatter_on_throw` (not caught): a thrown frozen object breaks.
/// True when it did.
pub fn shatter(world: &mut World, e: Entity) -> bool {
    if world.get::<Frozen>(e).is_none() || world.get::<CarriedBy>(e).is_some() {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return false;
    };
    let name = world
        .get::<DmName>(e)
        .map(|n| n.name.clone())
        .or_else(|| world.get::<Name>(e).map(|n| n.as_str().to_owned()))
        .unwrap_or_else(|| "что-то".to_owned());
    info!("{e:?} ({name}) shattered: thrown while frozen");
    super::tell_near(
        world,
        pos,
        crate::health::DEFAULT_MESSAGE_RANGE,
        Span::Danger,
        &format!("{} разбивается вдребезги!", capitalize(&name)),
    );
    world.despawn(e);
    crate::construction::tile_changed(world, pos);
    true
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

/// `on_moved`: a frozen object that comes to rest on an open tile at `T0C` or warmer thaws.
// Cost: only frozen objects that moved this tick (`Changed<TilePos>`).
pub(super) fn thaw_moved(world: &mut World) {
    let moved: Vec<(Entity, TilePos)> = world
        .query_filtered::<(Entity, &TilePos), (
            With<Frozen>,
            Changed<TilePos>,
            Without<CarriedBy>,
            Without<StoredIn>,
            Without<crate::throw::Thrown>,
        )>()
        .iter(world)
        .map(|(e, &p)| (e, p))
        .collect();
    for (e, pos) in moved {
        let warm = world
            .get_resource::<ss13_atmos::Atmos>()
            .and_then(|a| a.temperature(pos))
            .is_some_and(|t| t >= T0C);
        if warm {
            unfreeze(world, e);
        }
    }
}
