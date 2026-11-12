//! Bows (server): tg `/obj/item/gun/ballistic/bow` (`guns/ballistic/bows/`) on top of
//! [`crate::guns`]: the longbow, the shortbow, the chaplain's divine bow and the ashen bow (their
//! damage multipliers are the gun's `projectile_damage_multiplier`).
//!
//! An arrow clicked onto the bow is nocked (the bowstring is a one-arrow internal magazine).
//! Z draws the string or releases it ("стрела не наложена!" without an arrow). Firing an undrawn
//! bow drops the arrow; a drawn one shoots it and the string is released. Alt-click takes the
//! nocked arrow into the hand. The arrow falls out when the bow leaves the hands. A shot arrow
//! (`caseless`, reusable) lies where it stopped; plastic and blazing arrows are used up.
// shortcut: no drawn sprites or arrow overlays, no embedding of a hit arrow (it falls on the
// target's tile), no `projectile_speed_multiplier` of the divine bow, no arrow special effects
// (sticky, poison, blazing); add with overlays, projectile embedding and arrow effects.

use bevy::prelude::*;
use ss13_mapload::DmGun;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::guns::Gun;
use crate::inventory::{CarriedBy, Inventory};

pub const BOW: &str = "/obj/item/gun/ballistic/bow";
const DRAW_SOUND: &str = "sound/items/weapons/gun/bow/bow_draw.ogg";
const FIRE_SOUND: &str = "sound/items/weapons/gun/bow/bow_fire.ogg";

/// The bow's string is drawn (tg `drawn`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BowDrawn;

pub fn is_bow(world: &World, gun: Entity) -> bool {
    world
        .get::<DmGun>(gun)
        .is_some_and(|d| is_under(&d.path, BOW))
}

fn play(world: &mut World, at_entity: Entity, file: &str) {
    if let Some(at) = crate::modpc::pda::turf_of(world, at_entity) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(at, file, 25.0, true));
    }
}

/// `bow/attack_self`: draw or release the string. True for a bow.
pub fn attack_self(world: &mut World, mob: Entity, bow: Entity) -> bool {
    if !is_bow(world, bow) {
        return false;
    }
    if world.get::<Gun>(bow).is_none_or(|g| g.chambered.is_none()) {
        crate::balloon::balloon_alert(world, mob, bow, "стрела не наложена!");
        return true;
    }
    let drawn = world.get::<BowDrawn>(bow).is_some();
    let text = if drawn {
        "тетива отпущена"
    } else {
        "тетива натянута"
    };
    crate::balloon::balloon_alert(world, mob, bow, text);
    if drawn {
        world.entity_mut(bow).remove::<BowDrawn>();
    } else {
        world.entity_mut(bow).insert(BowDrawn);
    }
    play(world, bow, DRAW_SOUND);
    true
}

/// `bow/try_fire_gun` before the shot: `Some(false)` stops it (no arrow: silent; not drawn: the
/// arrow falls to the ground), `None` lets the gun fire.
pub fn before_fire(world: &mut World, client: Entity, bow: Entity) -> Option<bool> {
    if !is_bow(world, bow) {
        return None;
    }
    if world.get::<Gun>(bow).is_none_or(|g| g.chambered.is_none()) {
        return Some(false);
    }
    if world.get::<BowDrawn>(bow).is_none() {
        crate::tools::tell(
            world,
            client,
            Span::Warning,
            "Без натянутой тетивы стрела бесполезно падает на землю.",
        );
        drop_arrow(world, bow);
        return Some(false);
    }
    None
}

/// After a shot: the fired arrow is gone, so the string is released (`clear_chambered`).
pub fn after_shot(world: &mut World, bow: Entity) {
    if world.get::<Gun>(bow).is_some_and(|g| g.chambered.is_none()) {
        world.entity_mut(bow).remove::<BowDrawn>();
    }
}

/// `drop_arrow`: the nocked arrow onto the bow's tile.
pub fn drop_arrow(world: &mut World, bow: Entity) -> Option<Entity> {
    let at = crate::modpc::pda::turf_of(world, bow)?;
    let round = world.get_mut::<Gun>(bow)?.chambered.take()?;
    world.entity_mut(bow).remove::<BowDrawn>();
    crate::guns::drop_casing(world, &round, at)
}

/// `bow/click_alt`: the nocked arrow into the user's hands.
pub fn click_alt(world: &mut World, mob: Entity, bow: Entity) -> bool {
    if !is_bow(world, bow) {
        return false;
    }
    let Some(arrow) = drop_arrow(world, bow) else {
        return true;
    };
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, arrow);
    }
    true
}

/// `bow/equipped` (not into the hands) and `dropped`: a nocked arrow falls out.
// Cost: per tick, the inventories that changed and the items that left a mob.
pub(crate) fn arrows_fall_out(world: &mut World) {
    let mut changed = world.query_filtered::<(Entity, &Inventory), Changed<Inventory>>();
    let mut loose: Vec<Entity> = Vec::new();
    for (_, inv) in changed.iter(world) {
        for slot in crate::inventory::ALL_SLOTS {
            if let Some(item) = inv.slot(slot)
                && !inv.hands.contains(&Some(item))
            {
                loose.push(item);
            }
        }
    }
    // Put down (`put_down` moves it onto a tile without a carrier).
    let mut lying =
        world.query_filtered::<(Entity, &Gun), (Without<CarriedBy>, Changed<TilePos>)>();
    loose.extend(
        lying
            .iter(world)
            .filter(|(_, g)| g.chambered.is_some())
            .map(|(e, _)| e),
    );
    for bow in loose {
        if !is_bow(world, bow) || world.get::<Gun>(bow).is_none_or(|g| g.chambered.is_none()) {
            continue;
        }
        let holder = world.get::<CarriedBy>(bow).map(|c| c.0);
        if let Some(mob) = holder {
            crate::balloon::balloon_alert(world, mob, bow, "стрела выпадает!");
        }
        if world.get::<BowDrawn>(bow).is_some() {
            play(world, bow, FIRE_SOUND);
        }
        drop_arrow(world, bow);
    }
}

/// `caseless(reusable)`: the arrow a shot `projectile` type leaves where it stopped.
pub fn arrow_casing(projectile: &str) -> Option<&'static str> {
    const PAIRS: [(&str, &str); 5] = [
        (
            "/obj/projectile/bullet/arrow/sticky",
            "/obj/item/ammo_casing/arrow/sticky",
        ),
        (
            "/obj/projectile/bullet/arrow/poison",
            "/obj/item/ammo_casing/arrow/poison",
        ),
        (
            "/obj/projectile/bullet/arrow/holy",
            "/obj/item/ammo_casing/arrow/holy",
        ),
        (
            "/obj/projectile/bullet/arrow/ashen",
            "/obj/item/ammo_casing/arrow/ashen",
        ),
        (
            "/obj/projectile/bullet/arrow",
            "/obj/item/ammo_casing/arrow",
        ),
    ];
    // Plastic and blazing arrows are not reusable.
    if is_under(projectile, "/obj/projectile/bullet/arrow/plastic")
        || is_under(projectile, "/obj/projectile/bullet/arrow/blazing")
    {
        return None;
    }
    PAIRS
        .iter()
        .find(|(p, _)| is_under(projectile, p))
        .map(|(_, c)| *c)
}

/// A shot arrow stopped at `at`: a reusable one lies there.
pub fn arrow_drop(world: &mut World, projectile: &str, at: TilePos) {
    if let Some(casing) = arrow_casing(projectile) {
        crate::construction::spawn_obj(world, casing, at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reusable_arrows_leave_their_casing() {
        assert_eq!(
            arrow_casing("/obj/projectile/bullet/arrow"),
            Some("/obj/item/ammo_casing/arrow")
        );
        assert_eq!(
            arrow_casing("/obj/projectile/bullet/arrow/holy"),
            Some("/obj/item/ammo_casing/arrow/holy")
        );
        assert_eq!(arrow_casing("/obj/projectile/bullet/arrow/plastic"), None);
        assert_eq!(arrow_casing("/obj/projectile/bullet/arrow/blazing"), None);
        assert_eq!(arrow_casing("/obj/projectile/bullet/c10mm"), None);
    }
}
