//! Gifts and wrapping paper (server), after tg `code/game/objects/items/gift.dm` and the
//! wrapping paper of `items/stacks/wrap.dm` / `recycling/sortingmachinery.dm`.
//!
//! - A `/obj/item/gift` lies a little off its tile's centre with one of five sprites. Opened in
//!   hand it is gone and its present is in the opener's hands: one of the gift list (named
//!   toys, clothes, tools, every toy but the abstract and Syndicate card types, every
//!   collectable hat). A Christmas gift (`/obj/item/gift/anything`) holds any item except
//!   BandaStation's excluded debug and CentCom ones; one assigned to a player opens only for
//!   them.
//! - Three sheets of wrapping paper on a delivery parcel gift-wrap it (its `gift` sprite); an
//!   emptied roll leaves its cardboard tube.
// shortcut: the lists are what has a prototype (the converter emits the named gift types; toy
// and hat subtypes only when something else needs them, "anything" likewise); Santa
// (`TRAIT_CANNOT_OPEN_PRESENTS`, `crate::antag::santa`) cannot open them; no present vision
// examine, greyscale paper and ribbon colours
// (the parcel takes the plain gift sprite), big crate wrapping or the wrapping paper bap.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, DmName, IconIndex};
use ss13_proto::Prototypes;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use crate::chem::{name_of, visible};
use crate::combat::CombatRng;
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::tools::tell;

pub const GIFT: &str = "/obj/item/gift";
pub const GIFT_ANYTHING: &str = "/obj/item/gift/anything";
const WRAPPING_PAPER: &str = "/obj/item/stack/wrapping_paper";
/// What an emptied wrapping paper roll leaves (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
pub const CARDBOARD_TUBE: &str = "/obj/item/c_tube";
const WRAPPING_DMI: &str = "icons/obj/storage/wrapping.dmi";
/// A parcel's `base_icon_state`s; the gift-wrapped one is `gift<base>`.
const PARCEL_STATES: [&str; 9] = [
    "deliverypackage1",
    "deliverypackage2",
    "deliverypackage3",
    "deliverypackage4",
    "deliverypackage5",
    "deliveryfloppy",
    "deliverybox",
    "deliverycrate",
    "deliverycloset",
];
/// Paper per gift wrap.
const WRAP_COST: u32 = 3;

/// A gift's `contains_type` (decided when opened if not set) and who it is for.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Gift {
    pub contains: Option<String>,
    /// `recipient_ref` of a Christmas gift: only this mob opens it.
    pub recipient: Option<Entity>,
}

/// `giftwrapped` of a delivery parcel.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GiftWrapped;

pub struct GiftsPlugin;

impl Plugin for GiftsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_observer(add_gift);
        app.add_click_handler(ClickPhase::AttackSelf, 3213, |w, c| {
            open(w, c.client, c.mob, c.target)
        });
        // Wrapping paper on a parcel on the floor or in the other hand.
        app.add_click_handler(ClickPhase::HeldOnTarget, 4283, |w, c| {
            c.held
                .is_some_and(|held| wrap_parcel(w, c.client, c.mob, held, c.target))
        });
        app.add_click_handler(ClickPhase::OtherHand, 483, |w, c| {
            c.held
                .is_some_and(|held| wrap_parcel(w, c.client, c.mob, held, c.target))
        });
    }
}

/// `gift/Initialize`: `pixel_x/y = rand(-10, 10)`, `giftdeliverypackage[rand(1, 5)]`.
fn add_gift(
    add: On<Add<DmItem>>,
    items: Query<&DmItem>,
    mut looks: Query<&mut Appearance>,
    icons: Option<Res<IconIndex>>,
    rng: Option<ResMut<CombatRng>>,
    mut commands: Commands,
) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if !is_under(&item.path, GIFT) {
        return;
    }
    commands.entity(add.entity).insert_if_new(Gift::default());
    let Some(mut rng) = rng else {
        return;
    };
    let offset = [0, 0].map(|_: i16| rng.range(0, 20) as i16 - 10);
    let state = format!("giftdeliverypackage{}", rng.range(1, 5));
    let Ok(mut look) = looks.get_mut(add.entity) else {
        return;
    };
    look.pixel_offset = offset;
    if let Some(id) = icons.and_then(|ix| ix.get(WRAPPING_DMI, &state)) {
        look.icon_state = id;
    }
}

/// `get_gift_type()`'s candidates among `ids`: the named gifts, the toys but the abstract and
/// Syndicate card types (`typesof(cards) - deck`, `figure`, `ammo`), the collectable hats; or,
/// for a Christmas gift, every item but the debug and CentCom ones (BandaStation).
pub fn gift_candidates<'a>(ids: impl Iterator<Item = &'a str>, anything: bool) -> Vec<&'a str> {
    let mut out: Vec<&str> = ids
        .filter(|&id| {
            if anything {
                return is_under(id, "/obj/item")
                    && !id.contains("debug")
                    && !id.contains("centcom");
            }
            let toy = id != "/obj/item/toy"
                && is_under(id, "/obj/item/toy")
                && (!is_under(id, "/obj/item/toy/cards") || id == "/obj/item/toy/cards/deck")
                && id != "/obj/item/toy/figure"
                && id != "/obj/item/toy/ammo";
            let hat = id != "/obj/item/clothing/head/collectable"
                && is_under(id, "/obj/item/clothing/head/collectable");
            toy || hat || ss13_mapload::runtime::GIFT_TYPES.contains(&id)
        })
        .collect();
    // Prototype order is a hash map's: sorted, so a seeded roll is reproducible.
    out.sort_unstable();
    out
}

/// `gift/attack_self`. True when the item is a gift.
pub fn open(world: &mut World, client: Entity, mob: Entity, gift: Entity) -> bool {
    let Some(g) = world.get::<Gift>(gift).cloned() else {
        return false;
    };
    // `gift/anything/attack_self`: someone else's present stays shut.
    if g.recipient.is_some_and(|r| r != mob) {
        tell(
            world,
            client,
            Span::Notice,
            "Этот подарок не для вас, а портить отношения с Сантой не хочется!",
        );
        return true;
    }
    if world.get::<crate::antag::santa::SantaClaus>(mob).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "You're supposed to be spreading gifts, not opening them yourself!",
        );
        return true;
    }
    let contains = g.contains.or_else(|| {
        let anything = world
            .get::<DmItem>(gift)
            .is_some_and(|i| is_under(&i.path, GIFT_ANYTHING));
        let list: Vec<String> = world
            .get_resource::<Prototypes>()
            .map(|p| {
                gift_candidates(p.ids(), anything)
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        if list.is_empty() {
            return None;
        }
        let i = world
            .resource_mut::<CombatRng>()
            .range(0, list.len() as u32 - 1) as usize;
        list.into_iter().nth(i)
    });
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return true;
    };
    // `moveToNullspace()`, then `qdel(src)`.
    world.despawn(gift);
    let (user, src) = (name_of(world, mob), "подарок".to_owned());
    let thing = contains.and_then(|path| crate::construction::spawn_obj(world, &path, pos));
    match thing {
        Some(thing) => {
            let what = name_of(world, thing);
            visible(
                world,
                mob,
                &format!("Вы разворачиваете {src} и находите внутри {what}!"),
                &format!("{user} разворачивает {src} и находит внутри {what}!"),
                Span::Notice,
            );
            if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
                crate::storage::put_in_hands(world, mob, &inventory, thing);
            }
            crate::forensics::server::add_fingerprint(world, thing, mob, false);
        }
        None => visible(
            world,
            mob,
            "О нет! В подарке ничего не было!",
            &format!("О нет! В подарке, который открыл {user}, ничего не было!"),
            Span::Danger,
        ),
    }
    true
}

/// `gift/anything/assign_recipient`: the gift is named for `mob` and opens only for it.
pub fn assign_recipient(world: &mut World, gift: Entity, mob: Entity, job: &str) {
    let who = name_of(world, mob);
    if let Some(mut g) = world.get_mut::<Gift>(gift) {
        g.recipient = Some(mob);
    } else {
        return;
    }
    let name = format!("christmas gift for {who} ({job})");
    if let Some(mut n) = world.get_mut::<DmName>(gift) {
        n.name = name.clone();
    }
    world.entity_mut(gift).insert(Name::new(name));
}

/// `delivery/item_interaction` with wrapping paper. True when it was that.
pub fn wrap_parcel(
    world: &mut World,
    client: Entity,
    mob: Entity,
    paper: Entity,
    parcel: Entity,
) -> bool {
    let is_paper = world
        .get::<DmItem>(paper)
        .is_some_and(|i| is_under(&i.path, WRAPPING_PAPER));
    if !is_paper || world.get::<crate::disposals::Delivery>(parcel).is_none() {
        return false;
    }
    if world.get::<GiftWrapped>(parcel).is_some() {
        return true;
    }
    if crate::tools::stack_amount(world, paper) < WRAP_COST {
        tell(world, client, Span::Warning, "Нужно больше бумаги!");
        return true;
    }
    // `wrapping_paper/use`: an emptied roll leaves its tube where it was.
    let tube_at = world.get::<TilePos>(paper).copied().or_else(|| {
        let holder = world.get::<CarriedBy>(paper)?.0;
        world.get::<TilePos>(holder).copied()
    });
    let last = crate::tools::stack_amount(world, paper) == WRAP_COST;
    if !crate::tools::consume(world, paper, WRAP_COST) {
        return true;
    }
    if last && let Some(pos) = tube_at {
        crate::construction::spawn_obj(world, CARDBOARD_TUBE, pos);
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Вы заворачиваете посылку в праздничную бумагу!",
        &format!("{user} заворачивает посылку в праздничную бумагу!"),
        Span::Notice,
    );
    world.entity_mut(parcel).insert(GiftWrapped);
    gift_sprite(world, parcel);
    true
}

/// `update_icon_state`: `gift[base_icon_state]` of the parcel's current base state.
fn gift_sprite(world: &mut World, parcel: Entity) {
    let Some(ix) = world.get_resource::<IconIndex>() else {
        return;
    };
    let Some(current) = world.get::<Appearance>(parcel).map(|a| a.icon_state) else {
        return;
    };
    let dmi = world
        .get::<DmAtom>(parcel)
        .map_or(WRAPPING_DMI.to_owned(), |a| a.icon.clone());
    let gift = PARCEL_STATES
        .iter()
        .find(|s| ix.get(&dmi, s) == Some(current))
        .and_then(|s| ix.get(&dmi, &format!("gift{s}")));
    if let Some(id) = gift
        && let Some(mut a) = world.get_mut::<Appearance>(parcel)
    {
        a.icon_state = id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gift_lists() {
        let ids = [
            "/obj/item/toy",
            "/obj/item/toy/crayon/red",
            "/obj/item/toy/cards",
            "/obj/item/toy/cards/deck",
            "/obj/item/toy/cards/deck/syndicate",
            "/obj/item/toy/figure",
            "/obj/item/toy/figure/captain",
            "/obj/item/toy/ammo",
            "/obj/item/clothing/head/collectable/petehat",
            "/obj/item/bikehorn",
            "/obj/item/gun/energy/pulse/debug",
            "/obj/item/card/id/advanced/centcom",
            "/obj/item/wrench",
        ];
        assert_eq!(
            gift_candidates(ids.into_iter(), false),
            [
                "/obj/item/bikehorn",
                "/obj/item/clothing/head/collectable/petehat",
                "/obj/item/toy/cards/deck",
                "/obj/item/toy/crayon/red",
                "/obj/item/toy/figure/captain",
            ]
        );
        let anything = gift_candidates(ids.into_iter(), true);
        assert!(anything.contains(&"/obj/item/wrench"));
        assert!(
            !anything
                .iter()
                .any(|id| id.contains("debug") || id.contains("centcom"))
        );
    }
}
