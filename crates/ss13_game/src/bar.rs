//! Bar glassware (server): tg `/obj/item/reagent_containers/cup/glass/throw_impact` -> `smash`
//! (`reagent_containers/cups/drinks.dm`, `splash_reagents` in `reagent_containers.dm`).
//!
//! A thrown glass (`isGlass`, [`DmGlass`]) that is not caught smashes on what it hit, or on the
//! tile it landed on: its contents splash out (open or not) and a broken bottle lies where it
//! was.
// shortcut: the broken bottle keeps its own sprite (tg cuts the glass's sprite with a jagged
// mask), no bartender's booze slider (beer goggles), no sounds, no splash animation, and
// splashing exposes nothing, as `crate::chem`'s splash; bottles do not break over heads.

use bevy::prelude::*;
use ss13_mapload::DmGlass;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::chem::{Reagents, name_of, visible};
use crate::health::Health;

const BROKEN_BOTTLE: &str = "/obj/item/broken_bottle";

/// `throw_impact` of a glass that was not caught; `target` is what it hit (`None`: it landed).
pub fn glass_thrown(world: &mut World, item: Entity, target: Option<Entity>) {
    if !world.get::<DmGlass>(item).is_some_and(|g| g.is_glass) {
        return;
    }
    // In something (a disposal bin took it): not on a tile to break on.
    let Some(&pos) = world.get::<TilePos>(item) else {
        return;
    };
    if world.get::<crate::storage::StoredIn>(item).is_some()
        || world.get::<crate::inventory::CarriedBy>(item).is_some()
    {
        return;
    }
    smash(world, item, target, pos);
}

/// `smash`: `splash_reagents(allow_closed_splash = TRUE)`, the broken bottle, the glass gone.
pub fn smash(world: &mut World, item: Entity, target: Option<Entity>, pos: TilePos) {
    let volume = world
        .get::<Reagents>(item)
        .map_or(0.0, |r| r.0.total_volume);
    let no_splash = world
        .get::<Reagents>(item)
        .is_some_and(|r| r.0.flags & ss13_reagents::flags::NO_SPLASH != 0);
    if volume > 0.0 && !no_splash {
        let what = name_of(world, item);
        let text = match target.filter(|&t| world.get::<Health>(t).is_some()) {
            Some(mob) => format!("{} обливается чем-то!", name_of(world, mob)),
            None => {
                let onto = target.map_or_else(|| "пол".to_owned(), |t| name_of(world, t));
                format!("{what} разливает своё содержимое на {onto}.")
            }
        };
        visible(world, item, "", &text, Span::Notice);
        if let Some(mut r) = world.get_mut::<Reagents>(item) {
            r.0.clear();
        }
    }
    crate::construction::spawn_obj(world, BROKEN_BOTTLE, pos);
    world.despawn(item);
}
