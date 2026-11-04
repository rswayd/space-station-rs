//! Hands, tg's human equipment slots and ID access (server), after tg `code/modules/mob/inventory.dm`,
//! `code/_onclick/click.dm` and `code/modules/jobs/access.dm`.
//!
//! An item in an inventory leaves the world: it loses `TilePos` (so the spatial index and
//! replication drop it) and gets [`CarriedBy`]; dropping gives the position back. "Item" is
//! [`DmItem`]: the converter puts it on `/obj/item` and its subtypes only.
//!
//! Intents, each validated against the live world when it is applied (one exclusive pass, so a
//! second client's pickup in the same tick sees the first one's effect). Clicks arrive checked
//! for sight and reach by `crate::click`:
//! - `Click` on a visible item next to the mob with the active hand empty picks it up (tg
//!   `attack_hand` -> `attempt_pickup` -> `put_in_active_hand`);
//! - `Click` with an item in the active hand on a table or rack next to the mob puts it there (tg
//!   `table_place_act`: on a table at the clicked point), out of combat mode; on a free tile next
//!   to the mob, at the clicked point (not tg: there the floor ignores items);
//! - `SwapHands` (X) and a middle click (tg `cycle_hand`), `DropActive` (Q, `dropItemToGround`:
//!   up to 6 pixels off the tile's center);
//! - `ClickSlot`: a hand slot selects that hand, an equipment slot equips the active hand's item
//!   (tg `can_equip` rules) or takes the worn item into the empty hand (tg `attack_ui`);
//!   `AltClickSlot` alt-clicks the item in a slot (a gas tank toggles internals).
//!
//! The mob's look and HUD ([`HudData`], sent through tgui as [`InventoryHudUi`]) are recomputed
//! only when its [`Inventory`] changes.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmClothing, DmItem, DmObj, DmWeight, IconIndex};
use ss13_net::server::IntentReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, InventorySlot};
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject, UiSystems};
use ss13_world::{Density, Mover, SpatialIndex, WorldSystems};

use crate::click::{ClickResolved, ResolveClicks};
use crate::doors::Door;
use crate::health::Stat;
use crate::hud::{HudData, HudIcon, InventoryHudUi};
use crate::human::layers::{
    BACK_LAYER, BELT_LAYER, EARS_LAYER, FACEMASK_LAYER, GLASSES_LAYER, GLOVES_LAYER, HANDS_LAYER,
    HEAD_LAYER, ID_LAYER, NECK_LAYER, SHOES_LAYER, SUIT_LAYER, SUIT_STORE_LAYER, UNIFORM_LAYER,
};
use crate::human::{HumanAppearances, HumanLook, has_worn_sprites};
use crate::server::{PlayerMob, Players};

/// `ITEM_SLOT_ID` (`code/__DEFINES/inventory.dm`).
pub const ITEM_SLOT_ID: u32 = 1 << 8;
/// `WEIGHT_CLASS_TINY`, `POCKET_WEIGHT_CLASS` (`WEIGHT_CLASS_SMALL`), `WEIGHT_CLASS_BULKY`.
const WEIGHT_CLASS_TINY: u8 = 1;
const POCKET_WEIGHT_CLASS: u8 = 2;
const WEIGHT_CLASS_BULKY: u8 = 4;
/// `GLOB.any_suit_storage` (`code/__DEFINES/inventory.dm`): fit any suit's storage slot.
const ANY_SUIT_STORAGE: [&str; 22] = [
    "/obj/item/clipboard",
    "/obj/item/flashlight",
    "/obj/item/tank/internals/emergency_oxygen",
    "/obj/item/tank/internals/plasmaman",
    "/obj/item/vaporizer",
    "/obj/item/lighter",
    "/obj/item/pen",
    "/obj/item/modular_computer/pda",
    "/obj/item/toy/plush",
    "/obj/item/radio",
    "/obj/item/storage/bag/books",
    "/obj/item/storage/fancy/cigarettes",
    "/obj/item/tank/jetpack/captain",
    "/obj/item/stack/spacecash",
    "/obj/item/storage/wallet",
    "/obj/item/folder",
    "/obj/item/storage/box/matches",
    "/obj/item/cigarette",
    "/obj/item/gun/energy/laser/bluetag",
    "/obj/item/gun/energy/laser/redtag",
    "/obj/item/storage/belt/holster",
    "/obj/item/storage/belt/sheath",
];
/// `build_worn_icon` default file of the ID slot (`update_worn_id`).
const ID_WORN_DMI: &str = "icons/mob/clothing/id.dmi";
/// `DEFAULT_UNIFORM_FILE`, `DEFAULT_SHOES_FILE` (`update_worn_undersuit`, `update_worn_shoes`).
const UNIFORM_WORN_DMI: &str = "icons/mob/clothing/under/default.dmi";
const SHOES_WORN_DMI: &str = "icons/mob/clothing/feet.dmi";
/// `update_worn_back` default file.
const BACK_WORN_DMI: &str = "icons/mob/clothing/back.dmi";
/// `update_worn_ears` default file.
const EARS_WORN_DMI: &str = "icons/mob/clothing/ears.dmi";

/// What tg gives an assistant's ID: `/datum/id_trim/job/assistant` has no minimal access and
/// `ACCESS_MAINT_TUNNELS` as extra access; the default config (`JOBS_HAVE_MINIMAL_ACCESS` off)
/// grants `minimal | extra`.
// shortcut: the low-population default; with `JOBS_HAVE_MINIMAL_ACCESS` (or over
// `MINIMAL_ACCESS_THRESHOLD` players) tg gives none. Read the config once jobs exist.
pub const ASSISTANT_ACCESS: [&str; 1] = ["maint_tunnels"];

/// A mob's hands and worn slots. Every entity in it carries [`CarriedBy`] that mob.
#[derive(Component, Debug, Default, Clone, PartialEq, Eq)]
pub struct Inventory {
    /// Left, right (tg held indexes 1, 2).
    pub hands: [Option<Entity>; 2],
    /// Index into `hands`; tg starts with the left hand (`active_hand_index = 1`).
    pub active: usize,
    pub id: Option<Entity>,
    pub uniform: Option<Entity>,
    pub shoes: Option<Entity>,
    pub back: Option<Entity>,
    pub head: Option<Entity>,
    pub mask: Option<Entity>,
    pub neck: Option<Entity>,
    pub eyes: Option<Entity>,
    pub ears: Option<Entity>,
    pub gloves: Option<Entity>,
    pub suit: Option<Entity>,
    pub suit_storage: Option<Entity>,
    pub belt: Option<Entity>,
    /// Left, right.
    pub pockets: [Option<Entity>; 2],
}

/// Every slot of a human, hands first.
pub const ALL_SLOTS: [InventorySlot; 17] = [
    InventorySlot::LeftHand,
    InventorySlot::RightHand,
    InventorySlot::Id,
    InventorySlot::Uniform,
    InventorySlot::Shoes,
    InventorySlot::Back,
    InventorySlot::Head,
    InventorySlot::Mask,
    InventorySlot::Neck,
    InventorySlot::Eyes,
    InventorySlot::Ears,
    InventorySlot::Gloves,
    InventorySlot::Suit,
    InventorySlot::SuitStorage,
    InventorySlot::Belt,
    InventorySlot::LeftPocket,
    InventorySlot::RightPocket,
];

/// tg `ITEM_SLOT_*` bit of a slot (`code/__DEFINES/inventory.dm`).
pub fn slot_bit(slot: InventorySlot) -> u32 {
    match slot {
        InventorySlot::Suit => 1 << 0,
        InventorySlot::Uniform => 1 << 1,
        InventorySlot::Gloves => 1 << 2,
        InventorySlot::Eyes => 1 << 3,
        InventorySlot::Ears => 1 << 4,
        InventorySlot::Mask => 1 << 5,
        InventorySlot::Head => 1 << 6,
        InventorySlot::Shoes => 1 << 7,
        InventorySlot::Id => ITEM_SLOT_ID,
        InventorySlot::Belt => 1 << 9,
        InventorySlot::Back => 1 << 10,
        InventorySlot::Neck => 1 << 11,
        InventorySlot::LeftHand | InventorySlot::RightHand => 1 << 12,
        InventorySlot::SuitStorage => 1 << 13,
        InventorySlot::LeftPocket => 1 << 14,
        InventorySlot::RightPocket => 1 << 15,
    }
}

impl Inventory {
    pub fn active_item(&self) -> Option<Entity> {
        self.hands[self.active]
    }

    pub fn slot(&self, slot: InventorySlot) -> Option<Entity> {
        match slot {
            InventorySlot::LeftHand => self.hands[0],
            InventorySlot::RightHand => self.hands[1],
            InventorySlot::Id => self.id,
            InventorySlot::Uniform => self.uniform,
            InventorySlot::Shoes => self.shoes,
            InventorySlot::Back => self.back,
            InventorySlot::Head => self.head,
            InventorySlot::Mask => self.mask,
            InventorySlot::Neck => self.neck,
            InventorySlot::Eyes => self.eyes,
            InventorySlot::Ears => self.ears,
            InventorySlot::Gloves => self.gloves,
            InventorySlot::Suit => self.suit,
            InventorySlot::SuitStorage => self.suit_storage,
            InventorySlot::Belt => self.belt,
            InventorySlot::LeftPocket => self.pockets[0],
            InventorySlot::RightPocket => self.pockets[1],
        }
    }

    pub fn slot_mut(&mut self, slot: InventorySlot) -> &mut Option<Entity> {
        match slot {
            InventorySlot::LeftHand => &mut self.hands[0],
            InventorySlot::RightHand => &mut self.hands[1],
            InventorySlot::Id => &mut self.id,
            InventorySlot::Uniform => &mut self.uniform,
            InventorySlot::Shoes => &mut self.shoes,
            InventorySlot::Back => &mut self.back,
            InventorySlot::Head => &mut self.head,
            InventorySlot::Mask => &mut self.mask,
            InventorySlot::Neck => &mut self.neck,
            InventorySlot::Eyes => &mut self.eyes,
            InventorySlot::Ears => &mut self.ears,
            InventorySlot::Gloves => &mut self.gloves,
            InventorySlot::Suit => &mut self.suit,
            InventorySlot::SuitStorage => &mut self.suit_storage,
            InventorySlot::Belt => &mut self.belt,
            InventorySlot::LeftPocket => &mut self.pockets[0],
            InventorySlot::RightPocket => &mut self.pockets[1],
        }
    }

    /// The slot `item` is in.
    pub fn slot_of(&self, item: Entity) -> Option<InventorySlot> {
        ALL_SLOTS.into_iter().find(|&s| self.slot(s) == Some(item))
    }

    pub fn holds(&self, item: Entity) -> bool {
        self.slot_of(item).is_some()
    }

    /// tg `get_equipped_items()`: everything worn, not held.
    pub fn equipped(&self) -> impl Iterator<Item = Entity> + '_ {
        ALL_SLOTS[2..].iter().filter_map(|&s| self.slot(s))
    }

    /// Where tg's `get_idcard(hand_first = TRUE)` looks: active hand, other hand, ID slot.
    pub fn id_search(&self) -> impl Iterator<Item = Entity> {
        [
            self.hands[self.active],
            self.hands[1 - self.active],
            self.id,
        ]
        .into_iter()
        .flatten()
    }

    /// The access of the ID tg's `get_idcard(hand_first = TRUE)` finds: active hand, other
    /// hand, then the ID slot (a card, or the card in a PDA there). `None` without any ID.
    pub fn access<'a>(&self, cards: &'a IdCards) -> Option<&'a [String]> {
        self.id_search()
            .find_map(|e| cards.card(e))
            .map(|card| card.access.as_slice())
    }

    /// The card entity `get_idcard(hand_first = TRUE)` finds.
    pub fn id_card(&self, world: &World) -> Option<Entity> {
        self.id_search().find_map(|e| get_id(world, e))
    }
}

/// tg `GetID()`: an ID card is its own ID; a modular computer (PDA) gives the card in its ID
/// slot, by reference, so access changes reach whoever uses the PDA at once.
pub fn get_id(world: &World, item: Entity) -> Option<Entity> {
    if world.get::<IdCard>(item).is_some() {
        return Some(item);
    }
    world
        .get::<crate::modpc::pda::ModularComputer>(item)?
        .stored_id
        .filter(|&card| world.get::<IdCard>(card).is_some())
}

/// [`get_id`] for systems.
#[derive(bevy::ecs::system::SystemParam)]
pub struct IdCards<'w, 's> {
    cards: Query<'w, 's, &'static IdCard>,
    computers: Query<'w, 's, &'static crate::modpc::pda::ModularComputer>,
}

impl IdCards<'_, '_> {
    /// The ID card `item` is or holds.
    pub fn id_of(&self, item: Entity) -> Option<Entity> {
        if self.cards.contains(item) {
            return Some(item);
        }
        self.computers
            .get(item)
            .ok()?
            .stored_id
            .filter(|&card| self.cards.contains(card))
    }

    pub fn card(&self, item: Entity) -> Option<&IdCard> {
        self.cards.get(self.id_of(item)?).ok()
    }
}

/// On an item inside a mob's [`Inventory`]. Despawning the mob despawns what it carries, like
/// tg deleting a mob's contents.
#[derive(Component, Debug)]
#[relationship(relationship_target = Carried)]
pub struct CarriedBy(pub Entity);

#[derive(Component, Debug, Default)]
#[relationship_target(relationship = CarriedBy, linked_spawn)]
pub struct Carried(Vec<Entity>);

/// tg `TRAIT_NODROP`: the item cannot leave its slot by the wearer's hands (drop, place, slot
/// clicks, storage, throws) or be stripped; its granter takes it away (a changeling's arm blade
/// and chitin).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NoDrop;

/// Whether `item` is stuck to its holder ([`NoDrop`]).
pub fn no_drop(world: &World, item: Entity) -> bool {
    world.get::<NoDrop>(item).is_some()
}

/// Live access of an ID card (tg `/obj/item/card/id::access`), from [`DmItem::access`] when the
/// card is created (`crate::storage::finish_item`).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct IdCard {
    pub access: Vec<String>,
}

/// tg `check_access_list`: no requirement opens for anyone; otherwise every `req_access` entry
/// and, if `req_one_access` is set, one of its entries must be in `access` (no ID = no access).
pub fn check_access(req_access: &[String], req_one_access: &[String], access: &[String]) -> bool {
    if req_access.is_empty() && req_one_access.is_empty() {
        return true;
    }
    if access.is_empty() {
        return false;
    }
    req_access.iter().all(|r| access.contains(r))
        && (req_one_access.is_empty() || req_one_access.iter().any(|r| access.contains(r)))
}

impl ServerUi for InventoryHudUi {
    type Source = &'static HudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &HudData) {}

    fn data(source: &HudData, _: Entity, _: &()) -> HudData {
        source.clone()
    }

    /// Slot clicks are `ClientIntent::ClickSlot`; the HUD window takes no acts.
    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

pub struct InventoryServerPlugin;

impl Plugin for InventoryServerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<InventoryHudUi>()
            .add_systems(
                Update,
                (inventory_intents, refresh_mobs)
                    .chain()
                    .after(WorldSystems::Movement)
                    .after(ResolveClicks)
                    .after(crate::medical::MedicalClicks)
                    .before(UiSystems::Open),
            )
            .add_observer(forget_carried);
    }
}

/// An item left an inventory by any path (dropped, despawned): its slot empties.
fn forget_carried(
    remove: On<Remove<CarriedBy>>,
    carried: Query<&CarriedBy>,
    mut inventories: Query<&mut Inventory>,
) {
    let item = remove.entity;
    let Ok(&CarriedBy(mob)) = carried.get(item) else {
        return;
    };
    if let Ok(mut inventory) = inventories.get_mut(mob)
        && let Some(slot) = inventory.slot_of(item)
    {
        *inventory.slot_mut(slot) = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    /// A resolved click (seen, adjacent, by the client's living mob) on `target`, then at
    /// `pos`.
    Click {
        target: Entity,
        pos: TilePos,
        modifiers: ClickModifiers,
    },
    Swap,
    Drop,
    Slot(InventorySlot),
    AltSlot(InventorySlot),
    RightSlot(InventorySlot),
}

/// Every action, in the order it arrived (the network caps intents per client per tick): keys
/// and slot clicks first, then world clicks (one per client per tick, `crate::click`).
pub(crate) fn inventory_intents(
    world: &mut World,
    mut intents: Local<MessageCursor<IntentReceived>>,
    mut clicks: Local<MessageCursor<ClickResolved>>,
    mut middle: Local<MessageCursor<crate::click::MiddleClick>>,
) {
    let mut actions: Vec<(Entity, Action)> = intents
        .read(world.resource::<Messages<IntentReceived>>())
        .filter_map(|i| {
            let action = match i.intent {
                ClientIntent::SwapHands => Action::Swap,
                ClientIntent::DropActive => Action::Drop,
                ClientIntent::ClickSlot(slot) => Action::Slot(slot),
                ClientIntent::AltClickSlot(slot) => Action::AltSlot(slot),
                ClientIntent::RightClickSlot(slot) => Action::RightSlot(slot),
                _ => return None,
            };
            Some((i.client, action))
        })
        .collect();
    // tg `MiddleClickOn` -> `cycle_hand`: with two hands, the other one.
    actions.extend(
        middle
            .read(world.resource::<Messages<crate::click::MiddleClick>>())
            .map(|m| (m.client, Action::Swap)),
    );
    actions.extend(
        clicks
            .read(world.resource::<Messages<ClickResolved>>())
            .map(|c| {
                let action = Action::Click {
                    target: c.target,
                    pos: c.target_pos,
                    modifiers: c.modifiers,
                };
                (c.client, action)
            }),
    );
    for (client, action) in actions {
        apply(world, client, action);
    }
}

/// The client's conscious mob (tg `TRAIT_HANDS_BLOCKED` from crit, stuns, handcuffs;
/// `TRAIT_INCAPACITATED`): on the map, with hands.
fn actor(world: &World, client: Entity) -> Option<(Entity, TilePos)> {
    let mob = world.resource::<Players>().mob(client)?;
    let entity = world.get_entity(mob).ok()?;
    if entity.get::<Stat>() != Some(&Stat::Conscious) || !entity.contains::<Inventory>() {
        return None;
    }
    let traits = crate::status_effects::incapacitation(world, mob);
    if traits.hands_blocked || traits.incapacitated {
        return None;
    }
    entity.get::<PlayerMob>().filter(|p| p.client == client)?;
    Some((mob, *entity.get::<TilePos>()?))
}

fn apply(world: &mut World, client: Entity, action: Action) {
    let Some((mob, mob_pos)) = actor(world, client) else {
        return;
    };
    // tg `/atom/movable/screen/inventory/Click`: nothing during the click cooldown.
    let now = world.resource::<Time>().elapsed_secs_f64();
    if matches!(action, Action::Slot(_))
        && world
            .get::<crate::combat::NextMove>(mob)
            .is_some_and(|n| now < n.0)
    {
        return;
    }
    let inventory = world
        .get::<Inventory>(mob)
        .expect("checked by actor")
        .clone();
    match action {
        Action::Swap => set_active(world, mob, 1 - inventory.active),
        Action::Slot(slot @ (InventorySlot::LeftHand | InventorySlot::RightHand)) => {
            let hand = usize::from(slot == InventorySlot::RightHand);
            // The item in the other hand is clicked, not the slot (tg draws held items over
            // their slots): a storage opens or takes the active hand's item.
            match inventory.hands[hand] {
                // A bot part on a bot assembly (or what starts one) in the other hand: before
                // storage, as tg `medkit/tool_act` takes a robot arm before its storage.
                Some(item)
                    if hand != inventory.active
                        && inventory.active_item().is_some_and(|held| {
                            crate::bots::construction::item_interaction(
                                world, client, mob, held, item,
                            )
                        }) => {}
                Some(item)
                    if hand != inventory.active
                        && world.get::<crate::storage::Storage>(item).is_some() =>
                {
                    crate::storage::click_storage(world, client, mob, item);
                }
                // tg `attack_self` of the held item: an analyzer scans the user's turf, a welder
                // lights or goes out.
                Some(item)
                    if hand == inventory.active && crate::atmos::is_analyzer(world, item) =>
                {
                    crate::atmos::analyze_self(world, client, item, mob_pos);
                }
                Some(item)
                    if hand == inventory.active
                        && crate::tools::attack_self(world, client, mob, item) => {}
                // The ritual dagger's rune list, a blood magic aura cast on the user.
                Some(item)
                    if hand == inventory.active
                        && crate::antag::cult::server::attack_self(world, client, mob, item) => {}
                // A sickly blade breaks to teleport its heretic.
                Some(item)
                    if hand == inventory.active
                        && crate::antag::heretic::server::attack_self(world, client, mob, item) => {
                }
                // A welding visor flips up or down.
                Some(item)
                    if hand == inventory.active
                        && crate::vision::visor_attack_self(world, client, mob, item) => {}
                // A baton toggles, a flash flashes around.
                Some(item)
                    if hand == inventory.active
                        && crate::security::attack_self(world, client, mob, item) => {}
                // A gun racks, ejects, unloads or switches its fire mode.
                Some(item)
                    if hand == inventory.active
                        && crate::guns::attack_self(world, client, mob, item) => {}
                Some(item)
                    if hand == inventory.active
                        && crate::chem::attack_self(world, client, item) => {}
                // A health analyzer switches between health and wound scans.
                Some(item)
                    if hand == inventory.active
                        && crate::medical::attack_self(world, client, item) => {}
                // A destination tagger opens its window, a parcel is unwrapped.
                Some(item)
                    if hand == inventory.active
                        && crate::disposals::attack_self(world, client, mob, item) => {}
                // The nuclear uplink opens, a pinpointer toggles.
                Some(item)
                    if hand == inventory.active
                        && crate::antag::nukeop::server::attack_self(world, client, mob, item) => {}
                // Rack parts build a rack.
                Some(item)
                    if hand == inventory.active
                        && crate::tables::attack_self(world, client, mob, item) => {}
                // The RPD opens its window.
                Some(item)
                    if hand == inventory.active
                        && crate::atmos_machinery::rpd::attack_self(world, client, mob, item) => {}
                // A pipe fitting turns clockwise.
                Some(item)
                    if hand == inventory.active
                        && crate::atmos_machinery::fitting::attack_self(world, item) => {}
                // A PDA turns on and opens its NtOS window.
                Some(item)
                    if hand == inventory.active
                        && crate::modpc::pda::attack_self(world, client, mob, item) => {}
                // An ID card in the active hand goes into the PDA in the other.
                Some(item)
                    if hand != inventory.active
                        && inventory.active_item().is_some_and(|held| {
                            crate::modpc::pda::item_interaction(world, client, mob, held, item)
                        }) => {}
                // A tagger on a parcel, the package wrapper on an item, in the other hand.
                Some(item)
                    if hand != inventory.active
                        && inventory.active_item().is_some_and(|held| {
                            crate::disposals::mail_interaction(world, client, mob, held, item)
                        }) => {}
                // tg `interact_with_atom` of the active hand's container on the other hand's.
                Some(item)
                    if hand != inventory.active
                        && inventory.active_item().is_some_and(|held| {
                            crate::chem::item_interaction(
                                world,
                                client,
                                mob,
                                held,
                                item,
                                ClickModifiers::default(),
                            )
                        }) => {}
                // A magazine or rounds into the other hand's gun.
                Some(item)
                    if hand != inventory.active
                        && inventory.active_item().is_some_and(|held| {
                            crate::guns::item_interaction(world, client, mob, held, item)
                        }) => {}
                // tg `/obj/item/gun/ballistic/attack_hand`: the other hand's magazine comes out.
                Some(item)
                    if hand != inventory.active
                        && inventory.active_item().is_none()
                        && crate::guns::attack_hand(world, client, mob, item) => {}
                // tg `/obj/item/stack/attack_hand` from the other hand: one unit into the
                // empty active hand.
                Some(item)
                    if hand != inventory.active
                        && inventory.active_item().is_none()
                        && crate::construction::split_into_hand(
                            world,
                            mob,
                            item,
                            inventory.active,
                        ) => {}
                _ => set_active(world, mob, hand),
            }
        }
        // A cyborg's back slot is its model's module storage; it has no other slots.
        Action::Slot(slot)
            if crate::silicon::cyborg::inventory_action(
                world,
                client,
                mob,
                if slot == InventorySlot::Back {
                    crate::silicon::cyborg::CyborgAction::Back
                } else {
                    crate::silicon::cyborg::CyborgAction::OtherSlot
                },
            ) => {}
        Action::Slot(slot) => click_equip_slot(world, client, mob, mob_pos, &inventory, slot),
        // tg `click_alt` of the item in the slot: a gas tank toggles internals.
        // tg `attack_self_secondary` of the active hand's item.
        Action::RightSlot(slot) => {
            let active = if inventory.active == 0 {
                InventorySlot::LeftHand
            } else {
                InventorySlot::RightHand
            };
            if slot == active
                && let Some(item) = inventory.slot(slot)
                && crate::atmos::is_analyzer(world, item)
            {
                crate::atmos::open_analyzer(world, client, mob, item);
            }
        }
        Action::AltSlot(slot) => {
            if let Some(item) = inventory.slot(slot)
                && world.get::<crate::internals::TankAir>(item).is_some()
            {
                crate::internals::toggle(world, client, mob, item);
            } else if let Some(item) = inventory.slot(slot)
                && crate::atmos::is_analyzer(world, item)
            {
                crate::atmos::barometer(world, client, mob, item);
            } else if let Some(item) = inventory.slot(slot) {
                // A PDA gives back its ID.
                crate::modpc::pda::alt_click(world, mob, item);
            }
        }
        Action::Drop
            if crate::silicon::cyborg::inventory_action(
                world,
                client,
                mob,
                crate::silicon::cyborg::CyborgAction::Drop,
            ) => {}
        Action::Drop => {
            if let Some(item) = inventory.active_item().filter(|&i| !no_drop(world, i)) {
                let mut rng = world.resource_mut::<crate::combat::CombatRng>();
                let offset = [0, 0].map(|_| rng.range(0, 12) as i16 - 6);
                put_down_at(world, item, mob_pos, offset);
            }
        }
        Action::Click {
            target,
            pos,
            modifiers,
        } => {
            // Something earlier this tick (another client's pickup) may have moved it.
            if world.get::<TilePos>(target) != Some(&pos) {
                return;
            }
            // A wound treatment, surgery, drapes, paddles, an IV drip (`crate::medical`).
            if world
                .resource::<crate::medical::MedicalHandled>()
                .0
                .contains(&mob)
            {
                return;
            }
            match inventory.active_item() {
                // tg `attack_robot` -> `attack_ai`: a cyborg's empty hand opens machines.
                None if crate::silicon::interact::attack_robot(world, client, mob, target) => {}
                // tg `attack_robot`: a cyborg's empty hand picks nothing up.
                None if crate::silicon::cyborg::inventory_action(
                    world,
                    client,
                    mob,
                    crate::silicon::cyborg::CyborgAction::EmptyHandClick,
                ) => {}
                // tg `attack_hand` of a cult rune: invoked.
                None if crate::antag::cult::server::interact(world, client, mob, target) => {}
                // A heretic's rune (the rituals), an influence (drained by a right click).
                None if crate::antag::heretic::server::interact(
                    world,
                    client,
                    mob,
                    target,
                    modifiers.right,
                ) => {}
                // tg `interact` / `attack_hand_secondary` of kitchen machines.
                None if crate::kitchen::interact(world, client, mob, target, modifiers.right) => {}
                // tg `attack_hand` of ovens, griddles and fryers; the soda and booze dispensers.
                None if crate::cooking::interact(world, client, mob, target, modifiers.right) => {}
                None if crate::chem_dispenser::interact(
                    world,
                    client,
                    mob,
                    target,
                    modifiers.right,
                ) => {}
                // tg `attack_hand` of hydroponics trays (harvest, dead plant, examine).
                None if crate::botany::interact(world, client, mob, target, modifiers.right) => {}
                // tg `attack_hand` / `attack_hand_secondary` of a ladder: climb up or down.
                None if crate::multiz::interact(
                    world,
                    client,
                    mob,
                    None,
                    target,
                    modifiers.right,
                ) => {}
                // tg `attack_hand`/`click_alt` of atmos machines (windows, valves, tanks out).
                None if crate::atmos_machinery::windows::interact(
                    world, client, mob, target, modifiers,
                ) => {}
                // tg `attack_hand` of cell chargers and rechargers: the cell or device out.
                None if crate::power::chargers::interact(world, client, mob, target) => {}
                // tg `attack_hand(_secondary)` of wall mounts: fire alarms, light switches,
                // buttons, cabinets.
                None if crate::wallmount::interact(world, client, mob, target, modifiers) => {}
                // tg `attack_hand` of an APC with its cover closed: its window.
                None if crate::power::apc_ui::interact(world, client, mob, target) => {}
                // tg `attack_hand` of the mech bay console: its window.
                None if crate::mecha::bay::interact(world, client, mob, target) => {}
                // tg `attack_hand` of a mech: in combat mode "Nothing happens".
                None if crate::mecha::core::attack_hand(world, client, mob, target) => {}
                // tg `attack_hand` of a stationary modular computer (the ID console).
                None if crate::modpc::pda::console_interact(world, client, mob, target) => {}
                // tg `attack_hand` of disposal bins (the `DisposalUnit` window).
                None if crate::disposals::interact(world, client, mob, target, modifiers) => {}
                // tg `attack_hand` of the DNA console (its window) and scanner (open/close).
                None if crate::genetics::console::interact(world, client, mob, target) => {}
                // tg `attack_hand` of tables and racks: a pulled mob onto it, flipping (right
                // click), knocking a climber down, kicking a rack.
                None if crate::tables::attack_hand(world, client, mob, target, modifiers) => {}
                None => pick_up(world, mob, target, inventory.active),
                // The ritual dagger scrapes a rune away; a blood magic aura on an object.
                Some(held)
                    if crate::antag::cult::server::item_interaction(
                        world, client, mob, held, target,
                    ) => {}
                // A pen draws a heretic's rune, the codex drains, the grasp erases a rune.
                Some(held)
                    if crate::antag::heretic::server::item_interaction(
                        world, client, mob, held, target,
                    ) => {}
                // Bot parts on bot assemblies and what starts them (`crate::bots`).
                Some(held)
                    if crate::bots::construction::item_interaction(
                        world, client, mob, held, target,
                    ) => {}
                // Frames, and screwdrivers and crowbars on machines that come apart
                // (`crate::machine_frame`): tool acts come before the machines' own handlers.
                Some(held)
                    if crate::machine_frame::item_interaction(world, client, mob, held, target) => {
                }
                // tg `item_interaction` of kitchen machines (inserting), then food on a mob
                // (eating) and tools on food (slicing); food has reagents, so before chem.
                Some(held)
                    if crate::kitchen::item_interaction(world, client, mob, held, target) => {}
                Some(held)
                    if crate::cooking::item_interaction(
                        world,
                        client,
                        mob,
                        held,
                        target,
                        modifiers.pixel,
                    ) => {}
                // A container into a chem dispenser (before pouring into it).
                Some(held)
                    if crate::chem_dispenser::item_interaction(world, client, mob, held, target) => {
                }
                // tg `item_interaction` of wall mounts: a button pressed with an item, cabinets.
                Some(held)
                    if crate::wallmount::item_interaction(
                        world, client, mob, held, target, modifiers,
                    ) => {}
                // tg `item_interaction` of cell chargers (cells) and rechargers (batons).
                Some(held)
                    if crate::power::chargers::item_interaction(
                        world, client, mob, held, target,
                    ) => {}
                // tg `item_interaction`/`attackby` of exosuit things: the fabricator, mechs,
                // chassis, wreckages.
                Some(held)
                    if crate::mecha::item_interaction(
                        world, client, mob, held, target, modifiers,
                    ) => {}
                // tg `item_interaction` of R&D machines: stacks into lathes, items into the
                // destructive analyzer.
                Some(held)
                    if crate::science::item_interaction(world, client, mob, held, target) => {}
                // tg `item_interaction` of trays, the seed extractor and the plant analyzer:
                // before food (composting) and chem (pouring into a tray).
                Some(held)
                    if crate::botany::item_interaction(
                        world, client, mob, held, target, modifiers,
                    ) => {}
                Some(held) if crate::food::item_interaction(world, client, mob, held, target) => {}
                // tg cleaner component of mops and soap, mop buckets, spray bottles.
                Some(held)
                    if crate::janitor::item_interaction(
                        world, client, mob, held, target, modifiers,
                    ) => {}
                // tg `dnainjector/attack`.
                Some(held)
                    if crate::genetics::console::item_interaction(
                        world,
                        crate::tools::Actor {
                            client,
                            mob,
                            item: Some(held),
                        },
                        held,
                        target,
                    ) => {}
                // tg `interact_with_atom` of reagent containers (pour, drink, inject, splash).
                Some(held)
                    if crate::chem::item_interaction(
                        world, client, mob, held, target, modifiers,
                    ) => {}
                // tg `interact_with_atom` of medical items on living mobs.
                Some(held)
                    if crate::medical::item_interaction(
                        world, client, mob, held, target, modifiers,
                    ) => {}
                // tg `ladder/item_interaction(_secondary)`: out of combat mode, a climb.
                Some(held)
                    if !world
                        .get::<crate::combat::CombatMode>(mob)
                        .is_some_and(|c| c.0)
                        && crate::multiz::interact(
                            world,
                            client,
                            mob,
                            Some(held),
                            target,
                            modifiers.right,
                        ) => {}
                // tg `item_interaction`/`wrench_act` of atmos machines (ports, tanks, IDs).
                Some(held)
                    if crate::atmos_machinery::windows::item_interaction(
                        world, client, mob, held, target, modifiers,
                    ) => {}
                // tg `item_interaction_secondary` of the ID console: an ID into the second slot.
                Some(held)
                    if modifiers.right
                        && crate::modpc::pda::item_interaction_secondary(
                            world, client, mob, held, target,
                        ) => {}
                // tg `item_interaction` of disposal bins (items go in), parcels (tagging) and
                // `interact_with_atom` of the package wrapper.
                Some(held)
                    if crate::disposals::item_interaction(
                        world, client, mob, held, target, modifiers,
                    ) => {}
                // Tools on tables, frames and racks; a stack builds a table on a frame.
                Some(held)
                    if crate::tables::item_interaction(
                        world, client, mob, held, target, modifiers,
                    ) => {}
                // tg `base_item_interaction` and `attackby` come before storage and dropping.
                Some(held)
                    if crate::construction::item_interaction(
                        world, client, mob, held, target, modifiers,
                    ) => {}
                // A cyborg's module never leaves it (`TRAIT_NODROP`), nor does a `NoDrop` item.
                Some(held)
                    if world
                        .get::<crate::silicon::cyborg::RobotModule>(held)
                        .is_some()
                        || no_drop(world, held) => {}
                // tg `attackby` of a storage item: the held item goes in.
                Some(held) if world.get::<crate::storage::Storage>(target).is_some() => {
                    crate::storage::insert_from_hand(world, held, target);
                }
                // tg `interact_with_atom` of an analyzer: scans the clicked atom when it holds
                // gas, else its turf.
                Some(held) if crate::atmos::is_analyzer(world, held) => {
                    crate::atmos::scan_target(world, client, held, target);
                }
                // tg `item_interaction` of a PDA with an ID card.
                Some(held)
                    if crate::modpc::pda::item_interaction(world, client, mob, held, target) => {}
                Some(held) => place(world, mob, held, target, pos, modifiers),
            }
        }
    }
}

fn set_active(world: &mut World, mob: Entity, hand: usize) {
    let mut inventory = world.get_mut::<Inventory>(mob).expect("checked by actor");
    if inventory.active != hand {
        inventory.active = hand;
    }
}

pub(crate) fn pick_up(world: &mut World, mob: Entity, target: Entity, hand: usize) {
    let Ok(entity) = world.get_entity(target) else {
        return;
    };
    let pickable = entity.get::<DmItem>().is_some_and(DmItem::pickable)
        && entity.contains::<TilePos>()
        && !entity.contains::<CarriedBy>();
    if !pickable || !crate::medical::bodyparts::has_hand(world, mob, hand) {
        return;
    }
    world
        .entity_mut(target)
        .remove::<TilePos>()
        .insert(CarriedBy(mob));
    world
        .get_mut::<Inventory>(mob)
        .expect("checked by actor")
        .hands[hand] = Some(target);
}

/// tg `LETPASSTHROW` (`pass_flags_self`): thrown things fly over it (tables, racks, trays).
pub(crate) const LETPASSTHROW: u32 = 1 << 6;
/// tg `LETPASSCLICKS`: reaching past it diagonally is not blocked.
pub(crate) const LETPASSCLICKS: u32 = 1 << 13;

/// Whether `entity`'s `pass_flags_self` has any of `flags`.
pub(crate) fn lets_pass(world: &World, entity: Entity, flags: u32) -> bool {
    world
        .get::<DmObj>(entity)
        .is_some_and(|o| o.pass_flags_self & flags != 0)
}

/// Clicked with `held` on `target` on `pos`, next to the mob.
/// - A table or rack (tg `base_item_interaction` -> `table_place_act`, rack): out of combat
///   mode the item goes onto its tile, on a table at the clicked point (`ICON_X`/`ICON_Y`
///   minus 16, clamped to the tile).
/// - Not tg (there the floor ignores items): a free tile (a turf, or something lying on it;
///   never items, mobs, doors or dense things) takes it at the clicked point too.
// shortcut: no tray emptying or card dealing onto tables (tg `tray_act`, `deck_act`), no
// flipped tables; add with trays and card decks.
fn place(
    world: &mut World,
    mob: Entity,
    held: Entity,
    target: Entity,
    pos: TilePos,
    modifiers: ClickModifiers,
) {
    if crate::tables::place(world, mob, held, target, pos, modifiers) {
        return;
    }
    let clicked = modifiers
        .pixel
        .map_or([0, 0], |p| p.map(|v| i16::from(v).clamp(-16, 16)));
    let entity = world.entity(target);
    let surface = !entity.contains::<DmItem>()
        && !entity.contains::<Mover>()
        && !entity.contains::<Door>()
        && !entity.contains::<Density>();
    if surface
        && world
            .resource::<SpatialIndex>()
            .dense_on_tile(pos)
            .is_empty()
    {
        put_down_at(world, held, pos, clicked);
    }
}

/// Out of the inventory (the slot empties in [`forget_carried`]) onto `pos`, at its own pixel
/// offset.
pub(crate) fn put_down(world: &mut World, item: Entity, pos: TilePos) {
    put_down_at(world, item, pos, [0, 0]);
}

/// [`put_down`] `offset` pixels off its own offset (tg `transfer_item_to_turf`: `pixel_x =
/// base_pixel_x + x_offset`).
pub(crate) fn put_down_at(world: &mut World, item: Entity, pos: TilePos, offset: [i16; 2]) {
    let base = world
        .get::<DmAtom>(item)
        .map_or([0, 0], |a| [a.pixel_x, a.pixel_y]);
    let mut entity = world.entity_mut(item);
    entity.remove::<CarriedBy>().insert(pos);
    if let Some(mut look) = entity.get_mut::<Appearance>() {
        let wanted = [base[0] + offset[0], base[1] + offset[1]];
        if look.pixel_offset != wanted {
            look.pixel_offset = wanted;
        }
    }
}

/// tg `attack_ui(slot)` on an equipment slot (`/atom/movable/screen/inventory/Click`): a worn
/// storage opens or takes the held item ([`crate::storage::click_storage`]); anything else worn
/// goes to the empty active hand (`attack_hand` -> pickup, unequipping it); the active hand's
/// item goes into the empty slot if tg `can_equip` allows it.
// shortcut: no `equip_delay_self` (tg makes space suits take a `do_after` to put on).
fn click_equip_slot(
    world: &mut World,
    client: Entity,
    mob: Entity,
    mob_pos: TilePos,
    inventory: &Inventory,
    slot: InventorySlot,
) {
    let hand = inventory.active;
    if inventory.active_item().is_some_and(|i| no_drop(world, i)) {
        return;
    }
    match (inventory.active_item(), inventory.slot(slot)) {
        // tg `attackby` of the worn item: an ID card into a worn PDA.
        (Some(held), Some(worn))
            if crate::modpc::pda::item_interaction(world, client, mob, held, worn) => {}
        (_, Some(worn)) if world.get::<crate::storage::Storage>(worn).is_some() => {
            crate::storage::click_storage(world, client, mob, worn);
        }
        // tg `defibrillator/attack_hand` from its slot: the paddles come out or go back.
        (None, Some(worn)) if crate::medical::defib::attack_hand_worn(world, client, mob, worn) => {
        }
        (None, Some(worn)) if no_drop(world, worn) => {}
        (None, Some(worn)) => {
            unequip(world, mob, mob_pos, slot);
            world
                .get_mut::<Inventory>(mob)
                .expect("checked by actor")
                .hands[hand] = Some(worn);
        }
        (Some(item), None) if can_equip(world, inventory, item, slot) => {
            let mut inventory = world.get_mut::<Inventory>(mob).expect("checked by actor");
            inventory.hands[hand] = None;
            *inventory.slot_mut(slot) = Some(item);
        }
        _ => {}
    }
}

/// tg `/datum/species/proc/can_equip` for an organic human with every limb, into an empty
/// `slot` (hands excluded: they are not equipped through it here).
pub fn can_equip(world: &World, inventory: &Inventory, item: Entity, slot: InventorySlot) -> bool {
    let Some(dm) = world.get::<DmItem>(item) else {
        return false;
    };
    if inventory.slot(slot).is_some() {
        return false;
    }
    let w_class = world
        .get::<DmWeight>(item)
        .copied()
        .unwrap_or_default()
        .w_class;
    if dm.slot_flags & slot_bit(slot) == 0 {
        let excused = match slot {
            InventorySlot::LeftPocket | InventorySlot::RightPocket => {
                w_class <= POCKET_WEIGHT_CLASS
            }
            InventorySlot::SuitStorage => true,
            _ => false,
        };
        if !excused {
            return false;
        }
    }
    match slot {
        InventorySlot::LeftHand | InventorySlot::RightHand => false,
        // Need a jumpsuit on an organic chest or legs.
        InventorySlot::Id
        | InventorySlot::Belt
        | InventorySlot::LeftPocket
        | InventorySlot::RightPocket => inventory.uniform.is_some(),
        InventorySlot::SuitStorage => {
            let Some(suit) = inventory.suit.and_then(|s| world.get::<DmClothing>(s)) else {
                return false;
            };
            if w_class == WEIGHT_CLASS_TINY
                || ANY_SUIT_STORAGE.iter().any(|r| is_under(&dm.path, r))
            {
                return true;
            }
            w_class <= WEIGHT_CLASS_BULKY && suit.allowed.iter().any(|r| is_under(&dm.path, r))
        }
        _ => true,
    }
}

/// The mob is drawn again from its body and inventory (tg `update_body_parts`).
pub(crate) fn refresh_look(world: &mut World, mob: Entity) {
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
}

/// Takes the item out of `slot` (it stays carried: the caller puts it somewhere), and drops
/// what can no longer stay, like tg `/mob/living/carbon/human/doUnEquip`: the suit storage with
/// the suit; pockets, ID and belt with the jumpsuit.
pub fn unequip(world: &mut World, mob: Entity, mob_pos: TilePos, slot: InventorySlot) {
    let mut inventory = world
        .get_mut::<Inventory>(mob)
        .expect("a mob with an inventory");
    *inventory.slot_mut(slot) = None;
    let falls: &[InventorySlot] = match slot {
        InventorySlot::Suit => &[InventorySlot::SuitStorage],
        InventorySlot::Uniform => &[
            InventorySlot::RightPocket,
            InventorySlot::LeftPocket,
            InventorySlot::Id,
            InventorySlot::Belt,
        ],
        _ => &[],
    };
    let dropped: Vec<Entity> = falls
        .iter()
        .filter_map(|&s| inventory.slot_mut(s).take())
        .collect();
    for item in dropped {
        put_down(world, item, mob_pos);
    }
}

type MobItem<'a> = (
    Ref<'a, Inventory>,
    Option<&'a HumanLook>,
    &'a mut HudData,
    Option<&'a mut Appearance>,
    &'a PlayerMob,
    Has<crate::restraints::Handcuffed>,
    Option<&'a crate::antag::cult::server::CultHalo>,
);

/// Look and HUD of mobs whose inventory changed (pickups, drops, swaps; spawn counts).
fn refresh_mobs(
    mut mobs: Query<(Entity, MobItem), Changed<Inventory>>,
    items: Query<(&DmItem, Option<&DmAtom>, Option<&Appearance>), Without<PlayerMob>>,
    icons: Option<Res<IconIndex>>,
    mut humans: Option<ResMut<HumanAppearances>>,
    mut open: MessageWriter<OpenUi>,
) {
    for (mob, (inventory, body, mut hud, appearance, player, cuffed, halo)) in &mut mobs {
        let icon = |slot: Option<Entity>| {
            let (_, _, appearance) = items.get(slot?).ok()?;
            appearance.map(|a| HudIcon {
                icon_state: a.icon_state,
                color: a.color,
            })
        };
        let data = HudData {
            hands: inventory.hands.map(&icon),
            active: inventory.active as u8,
            id: icon(inventory.id),
            uniform: icon(inventory.uniform),
            shoes: icon(inventory.shoes),
            back: icon(inventory.back),
            head: icon(inventory.head),
            mask: icon(inventory.mask),
            neck: icon(inventory.neck),
            eyes: icon(inventory.eyes),
            ears: icon(inventory.ears),
            gloves: icon(inventory.gloves),
            suit: icon(inventory.suit),
            suit_storage: icon(inventory.suit_storage),
            belt: icon(inventory.belt),
            pockets: inventory.pockets.map(&icon),
        };
        hud.set_if_neq(data);
        // Opens the HUD on spawn, refreshes it at once afterwards.
        open.write(OpenUi::new::<InventoryHudUi>(player.client, mob, mob, ""));

        let (Some(body), Some(humans), Some(mut appearance)) =
            (body, humans.as_deref_mut(), appearance)
        else {
            continue;
        };
        // Items without hand-resolved sprites are drawn by `item_sprites`.
        let path = |slot: Option<Entity>| {
            Some(items.get(slot?).ok()?.0.path.clone()).filter(|p| has_worn_sprites(p))
        };
        let look = HumanLook {
            uniform: path(inventory.uniform),
            shoes: path(inventory.shoes),
            ..body.clone()
        };
        let mut sprites = icons
            .as_deref()
            .map(|icons| item_sprites(&inventory, &items, icons))
            .unwrap_or_default();
        // tg `update_worn_handcuffs`.
        if cuffed && let Some(cuffs) = icons.as_deref().and_then(crate::restraints::overlay) {
            sprites.push(cuffs);
        }
        // tg `/datum/status_effect/cult_halo/add_halo`.
        if let (Some(&halo), Some(icons)) = (halo, icons.as_deref())
            && let Some(overlay) = crate::antag::cult::server::halo_overlay(icons, halo)
        {
            sprites.push(overlay);
        }
        match humans.dressed(&look, &sprites) {
            Ok(dressed) => {
                appearance.set_if_neq(dressed);
            }
            Err(e) if inventory.is_added() => warn!("player mob keeps its template icon: {e}"),
            Err(e) => debug!("player mob look not updated: {e}"),
        }
    }
}

/// In-hand and worn sprites, tg `build_worn_icon`: in hands `inhand_icon_state || icon_state`
/// from `lefthand_file`/`righthand_file`; worn (every worn slot but the pockets; a uniform or
/// shoes only without hand-resolved sprites) `worn_icon_state || icon_state` from `worn_icon || <slot default>`.
/// A state the `.dmi` lacks draws nothing, as in BYOND (ID cards are
/// `worn_icon_state = "nothing"`).
// GAGS items name their rendered worn and in-hand icons (`ss13_mapload::gags`).
// shortcut: no item `color` on worn sprites, no
// `inhand_x/y_dimension` offsets for large sprites.
fn item_sprites(
    inventory: &Inventory,
    items: &Query<(&DmItem, Option<&DmAtom>, Option<&Appearance>), Without<PlayerMob>>,
    icons: &IconIndex,
) -> Vec<(f32, IconStateId)> {
    let state = |atom: Option<&DmAtom>, own: &str| -> String {
        if own.is_empty() {
            atom.map(|a| a.icon_state.clone()).unwrap_or_default()
        } else {
            own.to_owned()
        }
    };
    let mut out = Vec::new();
    for (hand, slot) in inventory.hands.iter().enumerate() {
        let Some((item, atom, _)) = slot.and_then(|e| items.get(e).ok()) else {
            continue;
        };
        let dmi = if hand == 0 {
            &item.lefthand_file
        } else {
            &item.righthand_file
        };
        let st = state(atom, &item.inhand_icon_state);
        if let Some(id) = icons.get(dmi, &st) {
            out.push((HANDS_LAYER, id));
        }
    }
    // (slot, layer, default file, drawn with the body when it has hand-resolved sprites)
    // Pockets draw nothing (tg `update_pockets` only moves the HUD icons).
    let worn = [
        (inventory.uniform, UNIFORM_LAYER, UNIFORM_WORN_DMI, true),
        (inventory.shoes, SHOES_LAYER, SHOES_WORN_DMI, true),
        (inventory.back, BACK_LAYER, BACK_WORN_DMI, false),
        (inventory.id, ID_LAYER, ID_WORN_DMI, false),
        (
            inventory.gloves,
            GLOVES_LAYER,
            "icons/mob/clothing/hands.dmi",
            false,
        ),
        (inventory.ears, EARS_LAYER, EARS_WORN_DMI, false),
        (
            inventory.suit,
            SUIT_LAYER,
            "icons/mob/clothing/suits/default.dmi",
            false,
        ),
        (
            inventory.eyes,
            GLASSES_LAYER,
            "icons/mob/clothing/eyes.dmi",
            false,
        ),
        (
            inventory.belt,
            BELT_LAYER,
            "icons/mob/clothing/belt.dmi",
            false,
        ),
        (
            inventory.suit_storage,
            SUIT_STORE_LAYER,
            "icons/mob/clothing/belt_mirror.dmi",
            false,
        ),
        (
            inventory.neck,
            NECK_LAYER,
            "icons/mob/clothing/neck.dmi",
            false,
        ),
        (
            inventory.mask,
            FACEMASK_LAYER,
            "icons/mob/clothing/mask.dmi",
            false,
        ),
        (
            inventory.head,
            HEAD_LAYER,
            "icons/mob/clothing/head/default.dmi",
            false,
        ),
    ];
    for (slot, layer, default_dmi, body) in worn {
        let Some((item, atom, _)) = slot.and_then(|e| items.get(e).ok()) else {
            continue;
        };
        if body && has_worn_sprites(&item.path) {
            continue;
        }
        let dmi = if item.worn_icon.is_empty() {
            default_dmi
        } else {
            &item.worn_icon
        };
        if let Some(id) = icons.get(dmi, &state(atom, &item.worn_icon_state)) {
            out.push((layer, id));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn access_follows_tg_check_access_list() {
        let none: Vec<String> = vec![];
        let brig = list(&["brig"]);
        let assistant = list(&ASSISTANT_ACCESS);
        let officer = list(&["brig", "security", "maint_tunnels"]);
        // Public doors open even without an ID.
        assert!(check_access(&none, &none, &none));
        assert!(!check_access(&brig, &none, &none));
        assert!(!check_access(&brig, &none, &assistant));
        assert!(check_access(&brig, &none, &officer));
        // req_access is all-of.
        assert!(!check_access(&list(&["brig", "armory"]), &none, &officer));
        // req_one_access is any-of, on top of req_access.
        let one = list(&["armory", "maint_tunnels"]);
        assert!(check_access(&none, &one, &assistant));
        assert!(!check_access(&none, &list(&["armory"]), &assistant));
        assert!(check_access(&brig, &one, &officer));
        assert!(!check_access(&brig, &one, &list(&["brig"])));
    }

    #[test]
    fn id_lookup_prefers_hands() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let slot_card = world
            .spawn(IdCard {
                access: list(&["a"]),
            })
            .id();
        let hand_card = world
            .spawn(IdCard {
                access: list(&["b"]),
            })
            .id();
        let toy = world.spawn_empty().id();
        let mut check = |inventory: Inventory| {
            world
                .run_system_once_with(
                    |In(inventory): In<Inventory>, cards: IdCards| {
                        inventory.access(&cards).map(<[String]>::to_vec)
                    },
                    inventory,
                )
                .unwrap()
        };
        let mut inventory = Inventory {
            hands: [Some(toy), None],
            id: Some(slot_card),
            ..default()
        };
        assert_eq!(check(inventory.clone()), Some(list(&["a"])));
        inventory.hands[1] = Some(hand_card);
        assert_eq!(
            check(inventory.clone()),
            Some(list(&["b"])),
            "the other hand first"
        );
        inventory.active = 1;
        inventory.hands[0] = Some(slot_card);
        assert_eq!(
            check(inventory.clone()),
            Some(list(&["b"])),
            "the active hand first"
        );
        assert_eq!(check(Inventory::default()), None);
    }

    #[test]
    fn a_pda_gives_its_card_by_reference() {
        let mut world = World::new();
        let card = world
            .spawn(IdCard {
                access: list(&["a"]),
            })
            .id();
        let mut pc = crate::modpc::pda::ModularComputer::pda(crate::modpc::pda::PDA_ROOT, "pda");
        pc.stored_id = Some(card);
        let pda = world.spawn(pc).id();
        let inventory = Inventory {
            id: Some(pda),
            ..default()
        };
        assert_eq!(inventory.id_card(&world), Some(card));
        world
            .get_mut::<IdCard>(card)
            .unwrap()
            .access
            .push("b".into());
        assert_eq!(get_id(&world, pda), Some(card));
        assert_eq!(world.get::<IdCard>(card).unwrap().access, list(&["a", "b"]));
    }
}
