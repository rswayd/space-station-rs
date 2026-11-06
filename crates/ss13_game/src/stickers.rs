//! Stickers (server), after tg `code/game/objects/items/stickers.dm`,
//! `datums/elements/stickerable.dm` and `datums/components/sticker.dm`.
//!
//! - A held `/obj/item/sticker` clicked on an object, an item or a turf (a floor, a wall) sticks
//!   there where the cursor was: the sticker leaves the world and is drawn as an overlay of the
//!   target; at most 15 on one thing. BandaStation: stickers do not stick to mobs.
//! - Washing the stickered thing peels its stickers off: they drop where it is (a wall's in front
//!   of it, on the side they were stuck from). A fire on a stickered turf (the tile in front of
//!   a wall) burns its stickers.
//! - A sticker with an `examine_text` adds it to the stickered thing's examine.
//! - Googly-eye and bot stickers pick one of their sprites.
// shortcut: no sticking by a thrown sticker (`on_bump`), no attack animation, no burning off a
// burning object or item (only turf fires), no contraband trait; upgrade with those mechanics.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmTurf, IconIndex, Turf};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};

use crate::health::Health;
use crate::inventory::CarriedBy;
use crate::obj_damage::is_under;
use crate::tools::tell;

pub const STICKER: &str = "/obj/item/sticker";
const STICKER_DMI: &str = "icons/obj/toys/stickers.dmi";
/// The stickerable element's `max`.
pub const MAX_STICKERS: usize = 15;
/// `FIRE_MINIMUM_TEMPERATURE_TO_EXIST`: any fire there is hot enough.
const BURN_CHECK_SECS: f64 = 1.0;

/// One stuck sticker: the sticker entity (out of the world), its overlay on the target and
/// where a turf's sticker drops (`listening_turf`).
#[derive(Clone, Debug, PartialEq)]
pub struct Stuck {
    pub sticker: Entity,
    pub overlay: Overlay,
    pub drop: TilePos,
    pub examine_text: Option<&'static str>,
}

/// The stickers on a thing (tg: one `/datum/component/sticker` each).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Stickers(pub Vec<Stuck>);

pub struct StickersPlugin;

impl Plugin for StickersPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_observer(add_sticker).add_observer(target_gone);
        app.add_click_handler(ClickPhase::HeldOnTarget, 4273, |w, c| {
            c.held.is_some_and(|held| {
                attempt_attach(w, c.client, c.mob, held, c.target, c.pos, c.modifiers.pixel)
            })
        });
        app.add_systems(Update, burn_off.run_if(any_with_component::<Stickers>));
    }
}

/// `examine_text` of a sticker type.
pub fn examine_text(path: &str) -> Option<&'static str> {
    let leaf = path.strip_prefix("/obj/item/sticker/")?;
    Some(match leaf {
        "rev" => {
            "There is a sticker displaying FUCK THE SYSTEM, the galaxy's premiere hardcore punk band."
        }
        "chief_engineer" => "There is a sticker displaying the Chief Engineer's SEAL OF APPROVAL.",
        "skub" => "There is a sticker displaying Skubtide, Stationwide!",
        "anti_skub" => "There is an anti-skub sticker.",
        _ => return None,
    })
}

/// `icon_states`: the sprites a sticker type picks from.
pub fn icon_states(path: &str) -> &'static [&'static str] {
    match path {
        "/obj/item/sticker/googly" => &["googly", "googly-alt"],
        "/obj/item/sticker/robot" => &["tile", "medbot", "clean"],
        _ => &[],
    }
}

/// `sticker/Initialize`: `icon_state = pick(icon_states)`.
fn add_sticker(
    add: On<Add<DmItem>>,
    items: Query<&DmItem>,
    mut looks: Query<&mut Appearance>,
    icons: Option<Res<IconIndex>>,
    rng: Option<ResMut<crate::combat::CombatRng>>,
) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let states = icon_states(&item.path);
    let (Some(icons), Some(mut rng)) = (icons, rng) else {
        return;
    };
    if states.is_empty() {
        return;
    }
    let state = states[rng.range(0, states.len() as u32 - 1) as usize];
    if let Some(id) = icons.get(STICKER_DMI, state)
        && let Ok(mut look) = looks.get_mut(add.entity)
    {
        look.icon_state = id;
    }
}

/// The stickerable element's `on_item_interact` -> `attempt_attach`. True when the held item
/// is a sticker.
pub fn attempt_attach(
    world: &mut World,
    client: Entity,
    mob: Entity,
    sticker: Entity,
    target: Entity,
    target_pos: TilePos,
    pixel: Option<[i8; 2]>,
) -> bool {
    let Some(path) = world
        .get::<DmItem>(sticker)
        .map(|i| i.path.clone())
        .filter(|p| is_under(p, STICKER))
    else {
        return false;
    };
    if target == sticker || world.get::<Appearance>(target).is_none() {
        return false;
    }
    // BandaStation: stickers do not stick to mobs.
    if world.get::<Health>(target).is_some() {
        tell(world, client, Span::Warning, "Стикер не прилипает!");
        return true;
    }
    if world
        .get::<Stickers>(target)
        .is_some_and(|s| s.0.len() >= MAX_STICKERS)
    {
        tell(world, client, Span::Warning, "Стикер не держится!");
        return true;
    }
    let Some(look) = world.get::<Appearance>(sticker).cloned() else {
        return false;
    };
    let base = world
        .get::<Appearance>(target)
        .map_or([0, 0], |a| a.pixel_offset);
    let [x, y] = pixel.map_or([0, 0], |[x, y]| [i16::from(x), i16::from(y)]);
    let mut overlay = Overlay::new(look.icon_state);
    overlay.color = look.color;
    overlay.pixel_offset = [(x - base[0]).clamp(-16, 16), (y - base[1]).clamp(-16, 16)];
    // `listening_turf`: a closed turf's is the one toward the sticker, the user's side.
    let closed = world.get::<Turf>(target).is_some()
        && world.get::<DmTurf>(target).is_some_and(|t| t.closed);
    let mob_pos = world.get::<TilePos>(mob).copied().unwrap_or(target_pos);
    let drop = if closed { mob_pos } else { target_pos };
    // `moveToNullspace`.
    world.entity_mut(sticker).remove::<(CarriedBy, TilePos)>();
    if let Some(mut a) = world.get_mut::<Appearance>(target) {
        a.overlays.push(overlay.clone());
    }
    let stuck = Stuck {
        sticker,
        overlay,
        drop,
        examine_text: examine_text(&path),
    };
    match world.get_mut::<Stickers>(target) {
        Some(mut s) => s.0.push(stuck),
        None => {
            world.entity_mut(target).insert(Stickers(vec![stuck]));
        }
    }
    tell(world, client, Span::Notice, "Стикер приклеен.");
    true
}

/// Where a thing's peeled stickers land (`drop_location`): its tile, its holder's.
fn drop_location(world: &World, target: Entity) -> Option<TilePos> {
    if world.get::<Turf>(target).is_some() {
        return None;
    }
    world.get::<TilePos>(target).copied().or_else(|| {
        let holder = world.get::<CarriedBy>(target)?.0;
        world.get::<TilePos>(holder).copied()
    })
}

/// `sticker/on_clean` -> `peel()` of every sticker on `target`. True when one came off.
pub fn clean_act(world: &mut World, target: Entity) -> bool {
    // Emptied before the removal, so `target_gone` keeps the stickers.
    let Some(stuck) = world
        .get_mut::<Stickers>(target)
        .map(|mut s| std::mem::take(&mut s.0))
    else {
        return false;
    };
    world.entity_mut(target).remove::<Stickers>();
    let here = drop_location(world, target);
    for s in &stuck {
        if let Some(mut a) = world.get_mut::<Appearance>(target)
            && let Some(i) = a.overlays.iter().position(|o| *o == s.overlay)
        {
            a.overlays.remove(i);
        }
        if world.get_entity(s.sticker).is_ok() {
            crate::inventory::put_down(world, s.sticker, here.unwrap_or(s.drop));
        }
    }
    !stuck.is_empty()
}

/// The stickers of a deleted thing go with it (`our_sticker` was in nullspace).
fn target_gone(remove: On<Remove<Stickers>>, stickers: Query<&Stickers>, mut commands: Commands) {
    let Ok(s) = stickers.get(remove.entity) else {
        return;
    };
    let gone: Vec<Entity> = s.0.iter().map(|s| s.sticker).collect();
    commands.queue(move |world: &mut World| {
        for sticker in gone {
            if world.get_entity(sticker).is_ok() && world.get::<TilePos>(sticker).is_none() {
                world.despawn(sticker);
            }
        }
    });
}

/// `on_turf_expose` at fire temperature: a fire on a stickered turf's `listening_turf` burns
/// its stickers. Checked once a second.
// Cost: once a second, one hotspot lookup per stickered turf.
fn burn_off(world: &mut World, mut last: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now - *last < BURN_CHECK_SECS {
        return;
    }
    *last = now;
    let mut q = world.query_filtered::<(Entity, &Stickers), With<Turf>>();
    let turfs: Vec<(Entity, Vec<TilePos>)> = q
        .iter(world)
        .map(|(e, s)| (e, s.0.iter().map(|s| s.drop).collect()))
        .collect();
    for (turf, listening) in turfs {
        let burning: Vec<usize> = {
            let Some(atmos) = world.get_resource::<ss13_atmos::Atmos>() else {
                return;
            };
            listening
                .iter()
                .enumerate()
                .filter(|(_, p)| atmos.hotspot(**p).is_some())
                .map(|(i, _)| i)
                .collect()
        };
        for i in burning.into_iter().rev() {
            let Some(stuck) = world.get_mut::<Stickers>(turf).map(|mut s| s.0.remove(i)) else {
                continue;
            };
            if let Some(mut a) = world.get_mut::<Appearance>(turf)
                && let Some(j) = a.overlays.iter().position(|o| *o == stuck.overlay)
            {
                a.overlays.remove(j);
            }
            if world.get_entity(stuck.sticker).is_ok() {
                world.despawn(stuck.sticker);
            }
        }
        if world.get::<Stickers>(turf).is_some_and(|s| s.0.is_empty()) {
            world.entity_mut(turf).remove::<Stickers>();
        }
    }
}

/// `on_examine`: the stickers' texts.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(s) = world.get::<Stickers>(target) else {
        return;
    };
    for text in s.0.iter().filter_map(|s| s.examine_text) {
        lines.push((Span::Warning, text.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticker_types() {
        assert!(examine_text("/obj/item/sticker/skub").is_some());
        assert!(examine_text("/obj/item/sticker/smile").is_none());
        assert_eq!(icon_states("/obj/item/sticker/googly").len(), 2);
        assert!(icon_states("/obj/item/sticker/star").is_empty());
    }
}
