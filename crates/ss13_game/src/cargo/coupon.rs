//! Coupons (server), after tg `/obj/item/coupon` (`code/modules/cargo/coupon.dm`) and the
//! cigarette packet's free coupon (`/obj/item/storage/fancy/cigarettes/attack_self`).
//!
//! - An emptied cigarette packet (not an `.../empty` one) used in hand gives a coupon once and
//!   becomes a "discarded cigarette packet" that holds nothing.
//! - `generate`: a pack from a `discountable` category (`pack_discount_odds`: standard 45,
//!   uncommon 4, rare 1) and a discount (10% 4, 15% 8, 20% 10, 25% 8, 50% 4, the omen 1). The
//!   omen coupon reads "fuck you" and is refused by consoles.
//! - Used on a supply console it goes inside (`loaded_coupons`); the next order of its pack takes
//!   it ([`take_for`], `crate::cargo::console`), shipping uses it up, cancelling refunds it.
// shortcut: the omen coupon does not curse (no `/datum/component/omen` heart attack yet), no NTOS
// Coupon Master codes.

use bevy::prelude::*;
use ss13_mapload::cargo::COUPON;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmItem, DmName, MapSeed};
use ss13_net::server::Networked;
use ss13_shared::chat::Span;

use super::console::{CargoConsole, machine_say};
use super::supply::SupplyPacks;
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::storage::{Storage, Stored, put_inside};
use crate::tools::tell;

const CIGARETTES: &str = "/obj/item/storage/fancy/cigarettes";
/// `GLOB.pack_discount_odds`.
const CATEGORY_ODDS: [(&str, u32); 3] = [
    ("standard_discount", 45),
    ("uncommon_discount", 4),
    ("rare_discount", 1),
];
/// `generate`'s `chances`; `None` is `COUPON_OMEN`.
const DISCOUNT_ODDS: [(Option<f64>, u32); 6] = [
    (Some(0.10), 4),
    (Some(0.15), 8),
    (Some(0.20), 10),
    (Some(0.25), 8),
    (Some(0.50), 4),
    (None, 1),
];

/// A coupon: the pack (index into [`SupplyPacks::packs`]) and `discount_pct_off`, `None` for
/// the omen.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Coupon {
    pub pack: usize,
    pub discount: Option<f64>,
}

/// A cigarette packet whose coupon was taken (`spawn_coupon = FALSE`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CouponTaken;

#[derive(Resource)]
struct CouponRng(Rng);

pub struct CouponPlugin;

impl Plugin for CouponPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 3166, |w, c| {
            crate::cargo::coupon::cigarettes_attack_self(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2975, |w, c| {
            c.held
                .is_some_and(|h| crate::cargo::coupon::attack_atom(w, c.client, c.mob, h, c.target))
        });
        app.add_systems(Startup, init);
    }
}

fn init(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    world.insert_resource(CouponRng(Rng::new(seed ^ 0x6370_6e73)));
}

fn pick_weight<T: Copy>(rng: &mut Rng, list: &[(T, u32)]) -> Option<T> {
    let total: u64 = list.iter().map(|(_, w)| u64::from(*w)).sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.next_u64() % total;
    for &(v, w) in list {
        if roll < u64::from(w) {
            return Some(v);
        }
        roll -= u64::from(w);
    }
    None
}

/// `generate(discount, discounted_pack)` with tg's odds: (pack, discount); `None` without
/// discountable packs.
pub fn roll(packs: &SupplyPacks, rng: &mut Rng) -> Option<Coupon> {
    // tg picks the category first; an empty category (not on this server) is skipped.
    let categories: Vec<(&str, u32)> = CATEGORY_ODDS
        .iter()
        .copied()
        .filter(|(c, _)| packs.packs.iter().any(|p| p.discountable == *c))
        .collect();
    let category = pick_weight(rng, &categories)?;
    let pool: Vec<usize> = (0..packs.packs.len())
        .filter(|&i| packs.packs[i].discountable == category)
        .collect();
    let pack = pool[(rng.next_u64() % pool.len() as u64) as usize];
    let discount = pick_weight(rng, &DISCOUNT_ODDS)?;
    Some(Coupon { pack, discount })
}

/// `update_name`: "coupon - 20% off <pack>".
pub fn coupon_name(coupon: &Coupon, packs: &SupplyPacks) -> String {
    match coupon.discount {
        Some(d) => format!(
            "coupon - {}% off {}",
            (d * 100.0).round() as i64,
            packs.packs.get(coupon.pack).map_or("", |p| p.name.as_str())
        ),
        None => "coupon - fuck you".to_owned(),
    }
}

/// A new coupon of `coupon` in `mob`'s hands (or on their tile).
pub fn give(world: &mut World, client: Entity, mob: Entity, coupon: Coupon) -> Option<Entity> {
    let name = coupon_name(&coupon, world.get_resource::<SupplyPacks>()?);
    let item = crate::construction::spawn_detached(world, COUPON)?;
    let desc = if coupon.discount.is_none() {
        "The small text reads, 'You will be slaughtered'... That doesn't sound right, does it?"
            .to_owned()
    } else {
        world
            .get::<DmName>(item)
            .map(|n| n.desc.clone())
            .unwrap_or_default()
    };
    world
        .entity_mut(item)
        .insert((Networked, coupon, DmName { name, desc }));
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, item);
    }
    if coupon.discount.is_none() {
        tell(
            world,
            client,
            Span::Warning,
            "The coupon reads 'fuck you' in large, bold text... is- is that a prize, or?",
        );
    }
    Some(item)
}

/// `cigarettes/attack_self`: an emptied packet gives its coupon once.
pub fn cigarettes_attack_self(
    world: &mut World,
    client: Entity,
    mob: Entity,
    item: Entity,
) -> bool {
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return false;
    };
    if !is_under(&path, CIGARETTES)
        || path.ends_with("/empty")
        || world.get::<CouponTaken>(item).is_some()
        || world
            .get::<Stored>(item)
            .is_some_and(|s| !s.items().is_empty())
    {
        return false;
    }
    let coupon = world.resource_scope(|world, mut rng: Mut<CouponRng>| {
        roll(world.get_resource::<SupplyPacks>()?, &mut rng.0)
    });
    let Some(coupon) = coupon else {
        return false;
    };
    tell(world, client, Span::Notice, "ooh, free coupon");
    give(world, client, mob, coupon);
    let desc = "An old cigarette packet with the back torn off, worth less than nothing now.";
    world.entity_mut(item).insert((
        CouponTaken,
        DmName {
            name: "discarded cigarette packet".to_owned(),
            desc: desc.to_owned(),
        },
    ));
    if let Some(mut storage) = world.get_mut::<Storage>(item) {
        storage.max_slots = 0;
    }
    true
}

/// `coupon/attack_atom` on a supply console: into its `loaded_coupons`.
pub fn attack_atom(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(coupon) = world.get::<Coupon>(held).copied() else {
        return false;
    };
    if world.get::<CargoConsole>(target).is_none() {
        return false;
    }
    let console_name = world
        .get::<DmName>(target)
        .map(|n| n.name.trim_start_matches("\\improper ").to_owned())
        .unwrap_or_default();
    if coupon.discount.is_none() {
        let text = format!(
            "The {console_name} validates the coupon as authentic, but refuses to accept it..."
        );
        tell(world, client, Span::Warning, text);
        machine_say(world, target, "Coupon fulfillment already in progress...");
        return true;
    }
    world.entity_mut(held).remove::<CarriedBy>();
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(held))
    {
        *slot = None;
    }
    put_inside(world, held, target);
    if let Some(mut console) = world.get_mut::<CargoConsole>(target) {
        console.loaded_coupons.push(held);
    }
    let pack = world
        .get_resource::<SupplyPacks>()
        .and_then(|p| p.packs.get(coupon.pack))
        .map(|p| p.name.clone())
        .unwrap_or_default();
    machine_say(world, target, &format!("Coupon for {pack} applied!"));
    true
}

/// `add_item`'s coupon check for one order of `pack`: a loaded coupon of it leaves the console
/// (`moveToNullspace`) with its discount.
pub(crate) fn take_for(world: &mut World, console: Entity, pack: usize) -> Option<(Entity, f64)> {
    let loaded = world.get::<CargoConsole>(console)?.loaded_coupons.clone();
    let (coupon, discount) = loaded.into_iter().find_map(|c| {
        let found = world.get::<Coupon>(c).filter(|k| k.pack == pack)?;
        Some((c, found.discount?))
    })?;
    if let Some(mut state) = world.get_mut::<CargoConsole>(console) {
        state.loaded_coupons.retain(|&c| c != coupon);
    }
    world
        .entity_mut(coupon)
        .remove::<crate::storage::StoredIn>();
    let text = format!(
        "Coupon found! {}% off applied!",
        (discount * 100.0).round() as i64
    );
    machine_say(world, console, &text);
    Some((coupon, discount))
}

#[cfg(test)]
mod tests {
    use ss13_mapload::cargo::SupplyPackDef;

    use super::*;

    #[test]
    fn rolls_only_discountable_packs() {
        let packs = SupplyPacks::new(vec![
            SupplyPackDef {
                id: "/p/a".into(),
                name: "A".into(),
                ..default()
            },
            SupplyPackDef {
                id: "/p/b".into(),
                name: "B".into(),
                discountable: "rare_discount".into(),
                ..default()
            },
        ]);
        let mut rng = Rng::new(7);
        for _ in 0..50 {
            let c = roll(&packs, &mut rng).expect("a discountable pack");
            assert_eq!(c.pack, 1);
            assert!(c.discount.is_none_or(|d| (0.1..=0.5).contains(&d)));
        }
        assert!(roll(&SupplyPacks::new(Vec::new()), &mut rng).is_none());
        let name = coupon_name(
            &Coupon {
                pack: 1,
                discount: Some(0.2),
            },
            &packs,
        );
        assert_eq!(name, "coupon - 20% off B");
    }

    #[test]
    fn coupon_discounts_the_order() {
        let packs = SupplyPacks::new(vec![SupplyPackDef {
            id: "/p/a".into(),
            cost: 1000.0,
            ..default()
        }]);
        let order = super::super::supply::SupplyOrder {
            id: 1,
            pack: 0,
            orderer: String::new(),
            orderer_rank: String::new(),
            reason: String::new(),
            paying_account: Some(1),
            paying_department: None,
            can_be_cancelled: true,
            coupon: Some((Entity::PLACEHOLDER, 0.25)),
        };
        assert_eq!(order.final_cost(&packs), 825, "1000 x 1.1 x 0.75");
    }
}
