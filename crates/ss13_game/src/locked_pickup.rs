//! BandaStation `/datum/component/ckey_and_role_locked_pickup`
//! (`modular_bandastation/~components`), on the legendary energy swords
//! (`modular_bandastation/weapon/code/melee/centcom/legendary_swords.dm`): equipped by anyone but
//! a whitelisted ckey, the item is thrown to the floor, the holder is knocked down for 10 s, told
//! the sword's refusal line and, if human, hurt in an arm. The very first equip binds it instead
//! (`binded = FALSE` by default), as in the fork.
// shortcut: no `centcom_role == CENTCOM_ROLE_OFFICER` exemption, minds have no CentCom roles yet;
// add it with them.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat, apply_damage_zone};
use crate::human::HumanLook;
use crate::inventory::CarriedBy;
use crate::server::PlayerMob;
use crate::status_effects::Incap;
use crate::zone::BodyZone;

/// `LEGENDARY_SWORDS_CKEY_WHITELIST`.
pub const LEGENDARY_SWORDS_CKEY_WHITELIST: &[&str] = &["mooniverse"];
/// `pickup_damage` of the legendary swords.
pub const LEGENDARY_PICKUP_DAMAGE: f32 = 10.0;
/// `Knockdown(10 SECONDS)`.
pub const REFUSAL_KNOCKDOWN_SECS: f32 = 10.0;

const LEGENDARY_SABER: &str = "/obj/item/dualsaber/legendary_saber";

/// Each legendary sword's `refusal_text`, most specific type first.
const REFUSALS: [(&str, &str); 8] = [
    (
        "/obj/item/dualsaber/legendary_saber/sorrow_catcher",
        "Ну, заплачь.",
    ),
    (
        "/obj/item/dualsaber/legendary_saber/flame",
        "Кровь и свет принадлежат лишь одному.",
    ),
    (
        "/obj/item/dualsaber/legendary_saber/devotion",
        "Только достойный узрит свет.",
    ),
    (
        "/obj/item/dualsaber/legendary_saber/sister",
        "Ты не принадлежишь сестре, верни её законному владельцу.",
    ),
    (
        "/obj/item/dualsaber/legendary_saber/flee_catcher",
        "Ловец бегущих не слушается тебя, кажется он хочет вернуться к хозяину.",
    ),
    (
        "/obj/item/dualsaber/legendary_saber/bsword",
        "Только достойный узрит свет.",
    ),
    (
        "/obj/item/dualsaber/legendary_saber/orphan",
        "Сироте ты не хозяин.",
    ),
    (
        LEGENDARY_SABER,
        "Злоба неподвластна твоей воле, усмрить её сможет лишь сильнейший.",
    ),
];

/// The component on an item.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct CkeyLockedPickup {
    pub ckeys: Vec<String>,
    pub pickup_damage: f32,
    pub refusal_text: String,
    /// The first equip went through.
    pub binded: bool,
}

pub struct LockedPickupPlugin;

impl Plugin for LockedPickupPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(attach).add_systems(Update, on_equip);
    }
}

fn is_under(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The legendary swords' `Initialize`: `AddComponent(ckey_and_role_locked_pickup, ...)`.
fn attach(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if !is_under(&item.path, LEGENDARY_SABER) {
        return;
    }
    let refusal = REFUSALS
        .iter()
        .find(|(root, _)| is_under(&item.path, root))
        .map_or("", |(_, text)| text);
    commands.entity(add.entity).insert_if_new(CkeyLockedPickup {
        ckeys: LEGENDARY_SWORDS_CKEY_WHITELIST
            .iter()
            .map(|k| (*k).to_owned())
            .collect(),
        pickup_damage: LEGENDARY_PICKUP_DAMAGE,
        refusal_text: refusal.to_owned(),
        binded: false,
    });
}

/// `COMSIG_ITEM_EQUIPPED`: the item just entered a mob's inventory by any path.
// Cost: change detection over the few locked items only.
fn on_equip(
    items: Query<(Entity, &CarriedBy), (With<CkeyLockedPickup>, Added<CarriedBy>)>,
    mut commands: Commands,
) {
    for (item, &CarriedBy(mob)) in &items {
        commands.queue(move |world: &mut World| check(world, item, mob));
    }
}

/// `on_equip` -> `check_role_and_ckey`.
pub fn check(world: &mut World, item: Entity, mob: Entity) {
    // Revalidated: still on that mob.
    if world.get::<CarriedBy>(item).map(|c| c.0) != Some(mob) {
        return;
    }
    let key = world
        .get::<PlayerMob>(mob)
        .and_then(|p| world.get::<ClientToken>(p.client))
        .map(|t| ckey(&t.0));
    let Some(mut lock) = world.get_mut::<CkeyLockedPickup>(item) else {
        return;
    };
    let allowed = key.is_some_and(|k| lock.ckeys.contains(&k));
    if allowed || !lock.binded {
        lock.binded = true;
        return;
    }
    let (damage, refusal) = (lock.pickup_damage, lock.refusal_text.clone());
    crate::status_effects::apply(world, mob, Incap::Knockdown, REFUSAL_KNOCKDOWN_SECS);
    // `dropItemToGround(I, force = TRUE)`: past `TRAIT_NODROP` too.
    if let Some(&at) = world.get::<TilePos>(mob) {
        crate::inventory::put_down(world, item, at);
    }
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, Span::Danger, refusal);
    }
    if world.get::<HumanLook>(mob).is_none() {
        return;
    }
    // `apply_damage(rand(d, d * 2), BRUTE, pick(BODY_ZONE_L_ARM, BODY_ZONE_R_ARM))`.
    let (amount, zone) = {
        let mut rng = world.resource_mut::<CombatRng>();
        let amount = rng.range(damage as u32, (damage * 2.0) as u32) as f32;
        let zone = if rng.chance(50.0) {
            BodyZone::LArm
        } else {
            BodyZone::RArm
        };
        (amount, zone)
    };
    if let Ok(mut e) = world.get_entity_mut(mob)
        && let Ok((mut health, mut stat)) = e.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        apply_damage_zone(&mut health, &mut stat, zone, DamageType::Brute, amount);
    }
}

/// tg `ckey()`: lowercase, letters, digits and `@` only.
fn ckey(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '@')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_legendary_sword_has_its_refusal() {
        let find = |path: &str| {
            REFUSALS
                .iter()
                .find(|(root, _)| is_under(path, root))
                .map(|(_, t)| *t)
        };
        assert_eq!(
            find("/obj/item/dualsaber/legendary_saber/orphan"),
            Some("Сироте ты не хозяин.")
        );
        assert_eq!(
            find("/obj/item/dualsaber/legendary_saber"),
            Some("Злоба неподвластна твоей воле, усмрить её сможет лишь сильнейший.")
        );
        assert!(!is_under("/obj/item/dualsaber", LEGENDARY_SABER));
        assert_eq!(ckey("Moon Iverse"), "mooniverse");
    }
}
