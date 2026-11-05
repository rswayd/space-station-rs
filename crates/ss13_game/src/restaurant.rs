//! The restaurant and the bar for robot customers (server), after tg `SSrestaurant`,
//! `/datum/venue` (`code/modules/food_and_drinks/restaurant/_venue.dm`, `generic_venues.dm`),
//! `/datum/customer_data`, `/datum/custom_order`, `/datum/element/venue_price`,
//! `/mob/living/basic/robot_customer` and its controller (`code/datums/ai/robot_customer/`),
//! with BandaStation's lines and order phrases (`ss13_mapload::restaurant`).
//!
//! - A portal (`/obj/machinery/restaurant_portal/*`) belongs to a venue. An empty hand whose ID
//!   has the venue's access opens or closes it (the door overlay shows while closed); an ID
//!   card used on it asks which venue to link (tg's radial menu as a list) and remembers who did
//!   (the moth's order). An open venue calls a customer every `rand(min, max)` (the first in
//!   4 s) while it has fewer than `max_guests` and at most one more than its seats.
//! - The seating projectors place the venue's holographic seats (`crate::holosign`); using the
//!   projector on a free seat of its kind removes it.
//! - A customer (a weighted pick of the venue's types; unique ones once per venue, the moth only
//!   when whoever linked the portal wears a hat, gloves or shoes) looks for a free seat within
//!   7 tiles every 8 s, walks onto it, orders (a weighted pick of its type's orders for the
//!   venue) and waits its patience. Food in front of it that matches, or handed to it, is its
//!   order: the sale pays the price as a holochip where the thing was, counts it and the income;
//!   the restaurant eats sold items, drinks and soups are drunk from their container. Then it
//!   leaves through its portal, happy, or mad when its patience ran out or the venue closed.
//! - Being pulled by someone without the venue's access, or attacked, warns twice; the third
//!   time it hugs the offender (15 brute on every limb but the head) until they go down.
// shortcut: no hover thought bubble of the order (tg `food_demands` alternate appearance; the
// order is said and shown on examine), no moth wings, TTS voices, board venue switching with a
// multitool, Ice Cream Day or gibs on death; the hug does not pull. Add with alternate
// appearances and the accessories data.
// Not tg: a sold ice cream counts as served and is eaten (`sell_ice_cream` returns no
// `TRANSACTION_SUCCESS`, so tg pays but leaves it); a customer whose portal is gone leaves at
// once (tg stands there forever); an attack warns once (tg's `relay_attackers` and
// `on_get_punched` count a punch twice).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::restaurant::{
    CLOTHING_PRICE, CustomerDef, DIRT, HOLOCHIP, OrderKind, RESTAURANT_FILE, ROBOT_CUSTOMER,
    RestaurantData, VenueDef,
};
use ss13_mapload::{DmFood, DmItem, DmName, DmObj, IconIndex, MapLoadPlugin};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, Facing, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_world::{Moved, SpatialIndex};

use crate::chem::{Chem, Reagents};
use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat};
use crate::inventory::{CarriedBy, IdCard, Inventory};
use crate::mobs::ai::behaviors::{Behavior, Perform, can_see};
use crate::mobs::ai::bt::{Abort, Cond, decorator, leaf, selector, sequence};
use crate::mobs::ai::trees::Built;
use crate::mobs::ai::{AiController, AiCtx, Key, Value};
use crate::tools::tell;
use crate::zone::BodyZone;

pub const CUSTOMER_CONTROLLER: &str = "/datum/ai_controller/robot_customer";
pub const PORTAL_ROOT: &str = "/obj/machinery/restaurant_portal";
const SEAT_ROOT: &str = "/obj/structure/holosign/robot_seat";
const SEAT_PROJECTOR: &str = "/obj/item/holosign_creator/robot_seat";
const PORTAL_DMI: &str = "icons/obj/machines/restaurant_portal.dmi";
const ECONOMY_DMI: &str = "icons/obj/economy.dmi";
/// `VENUE_RESTAURANT`: the venue that eats what it is sold and takes items as orders.
const VENUE_RESTAURANT: &str = "Restaurant Venue";
const DRINKING_GLASS: &str = "/obj/item/reagent_containers/cup/glass/drinkingglass";
const BOWL: &str = "/obj/item/reagent_containers/cup/bowl";
const CHAIR: &str = "/obj/structure/chair";
/// `VENUE_BAR_MINIMUM_REAGENTS`.
const DRINK_UNITS: f32 = 10.0;
/// `FOOD_PRICE_TRASH * 2` per scoop (`sell_ice_cream`).
const SCOOP_PRICE: u32 = 50;
/// `DEFAULT_MAX_ICE_CREAM_SCOOPS`.
const MAX_SCOOPS: u32 = 3;
/// The `BB_CUSTOMER_FIND_SEAT_COOLDOWN` decorator.
const FIND_SEAT_COOLDOWN: f64 = 8.0;
/// `oview(7)` of `find_seat`.
const SEAT_VISION: u32 = 7;
/// `break_spine` `give_up_distance` and its damage per limb.
const GIVE_UP_DISTANCE: u32 = 10;
const HUG_DAMAGE: f32 = 15.0;
/// The first guest after opening comes faster.
const FIRST_VISIT_SECS: f64 = 4.0;
/// `moth_clothing` without its opener's clothes.
const MOTH_FALLBACK: [(&str, &str, u32); 3] = [
    ("/obj/item/clothing/head/utility/chefhat", "chef's hat", 3),
    ("/obj/item/clothing/shoes/sneakers/black", "black shoes", 3),
    ("/obj/item/clothing/gloves/color/black", "black gloves", 1),
];
/// The cones of `/datum/custom_order/icecream` (`initial(name)`).
const CONE: (&str, &str) = ("/obj/item/food/icecream", "waffle cone");
const CHOCOLATE_CONE: (&str, &str) = ("/obj/item/food/icecream/chocolate", "chocolate cone");
/// `/datum/custom_order/reagent/soup` servings, in BandaStation's words.
const SERVINGS: [(&str, f32); 3] = [
    ("маленькую порцию (15u)", 15.0),
    ("среднюю порцию (20u)", 20.0),
    ("большую порцию (25u)", 25.0),
];
const EAT_SOUND: &str = "sound/items/eatfood.ogg";
const DRINK_SOUND: &str = "sound/items/drink.ogg";
const CASH_SOUND: &str = "sound/effects/cashregister.ogg";

/// One venue's live state (tg `/datum/venue`).
#[derive(Debug, Clone)]
pub struct Venue {
    pub def: Arc<VenueDef>,
    /// `customer_types`: unique ones drop out once they came.
    pub customer_types: Vec<(String, u32)>,
    pub open: bool,
    pub portals: Vec<Entity>,
    /// `current_visitors`: customer -> the portal it came through.
    pub visitors: Vec<(Entity, Entity)>,
    /// `linked_seats`: seat -> the customer on it.
    pub seats: Vec<(Entity, Option<Entity>)>,
    /// `visit_cooldown`, game time.
    pub next_visit: f64,
    pub served: u32,
    pub income: u64,
    /// `mob_blacklist`: warnings per offender.
    pub blacklist: HashMap<Entity, u32>,
}

/// `SSrestaurant`: the venues (`all_venues`) and customer types (`all_customers`).
#[derive(Resource, Default)]
pub struct Restaurant {
    pub venues: Vec<Venue>,
    pub customers: HashMap<String, Arc<CustomerDef>>,
    /// Portal and seat types -> their venue's path.
    links: HashMap<String, String>,
}

impl Restaurant {
    pub fn from_data(data: RestaurantData) -> Self {
        let venues = data
            .venues
            .into_iter()
            .map(|def| Venue {
                customer_types: def.customers.clone(),
                def: Arc::new(def),
                open: false,
                portals: Vec::new(),
                visitors: Vec::new(),
                seats: Vec::new(),
                next_visit: 0.0,
                served: 0,
                income: 0,
                blacklist: HashMap::new(),
            })
            .collect();
        Self {
            venues,
            customers: data
                .customers
                .into_iter()
                .map(|c| (c.path.clone(), Arc::new(c)))
                .collect(),
            links: data.portals.into_iter().chain(data.seats).collect(),
        }
    }

    fn venue_of(&self, path: &str) -> Option<usize> {
        let venue = self.links.get(path)?;
        self.venues.iter().position(|v| &v.def.path == venue)
    }
}

/// A portal and its venue (index into [`Restaurant::venues`]).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Portal {
    pub venue: usize,
    /// `turned_on_portal`: who last linked it with an ID.
    pub opener: Option<Entity>,
}

/// A holographic seat of a venue.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RobotSeat {
    pub venue: usize,
}

/// A holochip's `credits`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Holochip {
    pub credits: u32,
}

/// What a customer waits for (`BB_CUSTOMER_CURRENT_ORDER`).
#[derive(Clone, Debug, PartialEq)]
pub enum Order {
    /// An item of exactly this type (the moth's clothes too).
    Item {
        path: String,
        name: String,
        price: u32,
    },
    /// `/datum/custom_order/reagent`: a drink (in a drinking glass) or a soup (in a bowl).
    Reagent {
        soup: bool,
        reagent: String,
        name: String,
        price: u32,
        needed: f32,
        serving: &'static str,
    },
    /// `/datum/custom_order/icecream`: sorted flavours in this cone.
    IceCream {
        cone: &'static str,
        flavours: Vec<String>,
        name: String,
    },
}

impl Order {
    /// The order line (BandaStation's `order_food_line`, `get_order_line`).
    pub fn line(&self) -> String {
        match self {
            Self::Item { name, .. } | Self::IceCream { name, .. } => format!("Я буду {name}"),
            Self::Reagent {
                soup: false,
                name,
                needed,
                ..
            } => format!("Я буду {needed}u of {name}"),
            Self::Reagent {
                soup: true,
                name,
                serving,
                ..
            } => format!("Я буду {serving} {name}"),
        }
    }
}

/// A robot customer (the controller's blackboard keys that are not targets).
#[derive(Component, Clone, Debug)]
pub struct Customer {
    pub venue: usize,
    pub data: Arc<CustomerDef>,
    pub portal: Entity,
    pub order: Option<Order>,
    /// `BB_CUSTOMER_PATIENCE`, seconds.
    pub patience: f64,
    pub eating: bool,
    pub leaving: bool,
    said_cant_find: bool,
    next_seat_try: f64,
    /// `grime_carried`.
    pub grime: u32,
}

/// On a venue prompt: the portal it links.
#[derive(Component, Clone, Copy, Debug)]
struct LinkFor(Entity);

pub struct RestaurantPlugin;

impl Plugin for RestaurantPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `restaurant_portal/attack_hand`: open or close the venue.
        app.add_click_handler(ClickPhase::EmptyHand, 550, |w, c| {
            crate::restaurant::attack_hand(w, c.client, c.mob, c.target)
        });
        // An ID on a portal, a projector on its seat, an item handed to a customer.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1850, |w, c| {
            crate::restaurant::item_interaction(w, c.client, c.mob, c.held, c.target)
        });
        app.add_observer(on_obj_added)
            .add_observer(portal_gone)
            .add_observer(seat_gone)
            .add_observer(customer_gone)
            .add_observer(on_pulled)
            .add_systems(
                Update,
                (
                    venues_tick,
                    drop_grime,
                    link_answers
                        .in_set(ss13_tgui::server::UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                ),
            );
    }
}

/// The data, read from `<assets>/restaurant.ron` once (empty without it).
fn restaurant(world: &mut World) -> Mut<'_, Restaurant> {
    if !world.contains_resource::<Restaurant>() {
        let path = world
            .get_resource::<MapLoadPlugin>()
            .map(|m| m.assets.join(RESTAURANT_FILE));
        let data = match path.as_deref().map(RestaurantData::read) {
            Some(Ok(data)) => data,
            Some(Err(e)) => {
                warn!("{e:#} (rerun `ss13-mapload convert`): no restaurant");
                RestaurantData::default()
            }
            None => RestaurantData::default(),
        };
        world.insert_resource(Restaurant::from_data(data));
    }
    world.resource_mut::<Restaurant>()
}

fn on_obj_added(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(dm) = objs.get(add.entity) else {
        return;
    };
    let e = add.entity;
    if is_under(&dm.path, PORTAL_ROOT) {
        commands.queue(move |world: &mut World| attach_portal(world, e));
    } else if is_under(&dm.path, SEAT_ROOT) {
        commands.queue(move |world: &mut World| attach_seat(world, e));
    }
}

/// tg `restaurant_portal/Initialize`: linked to its venue.
fn attach_portal(world: &mut World, e: Entity) {
    let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
        return;
    };
    if world.get::<Portal>(e).is_some() {
        return;
    }
    let mut r = restaurant(world);
    let Some(venue) = r.venue_of(&path) else {
        return;
    };
    r.venues[venue].portals.push(e);
    world.entity_mut(e).insert(Portal {
        venue,
        opener: None,
    });
    refresh_portal(world, e);
}

/// tg `robot_seat/Initialize`: `linked_seats[src] += null`.
fn attach_seat(world: &mut World, e: Entity) {
    let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
        return;
    };
    if world.get::<RobotSeat>(e).is_some() {
        return;
    }
    let mut r = restaurant(world);
    let Some(venue) = r.venue_of(&path) else {
        return;
    };
    r.venues[venue].seats.push((e, None));
    world.entity_mut(e).insert(RobotSeat { venue });
}

fn portal_gone(
    remove: On<Remove<Portal>>,
    portals: Query<&Portal>,
    restaurant: Option<ResMut<Restaurant>>,
) {
    if let (Ok(p), Some(mut r)) = (portals.get(remove.entity), restaurant)
        && let Some(v) = r.venues.get_mut(p.venue)
    {
        v.portals.retain(|&x| x != remove.entity);
    }
}

fn seat_gone(
    remove: On<Remove<RobotSeat>>,
    seats: Query<&RobotSeat>,
    restaurant: Option<ResMut<Restaurant>>,
) {
    if let (Ok(s), Some(mut r)) = (seats.get(remove.entity), restaurant)
        && let Some(v) = r.venues.get_mut(s.venue)
    {
        v.seats.retain(|&(x, _)| x != remove.entity);
    }
}

/// tg `robot_customer/Destroy`: off the visitors, its seat freed.
fn customer_gone(
    remove: On<Remove<Customer>>,
    customers: Query<&Customer>,
    restaurant: Option<ResMut<Restaurant>>,
) {
    let mob = remove.entity;
    if let (Ok(c), Some(mut r)) = (customers.get(mob), restaurant)
        && let Some(v) = r.venues.get_mut(c.venue)
    {
        v.visitors.retain(|&(x, _)| x != mob);
        for seat in &mut v.seats {
            if seat.1 == Some(mob) {
                seat.1 = None;
            }
        }
    }
}

/// `update_overlays`: the door while the venue is closed.
fn refresh_portal(world: &mut World, e: Entity) {
    let Some(&Portal { venue, .. }) = world.get::<Portal>(e) else {
        return;
    };
    let open = world
        .get_resource::<Restaurant>()
        .and_then(|r| r.venues.get(venue))
        .is_some_and(|v| v.open);
    let door = (!open)
        .then(|| {
            world
                .get_resource::<IconIndex>()
                .and_then(|i| i.get(PORTAL_DMI, "portal_door"))
        })
        .flatten();
    if let Some(mut a) = world.get_mut::<Appearance>(e) {
        let overlays: Vec<Overlay> = door.map(Overlay::new).into_iter().collect();
        if a.overlays != overlays {
            a.overlays = overlays;
        }
    }
}

fn refresh_venue_portals(world: &mut World, venue: usize) {
    let portals = world
        .get_resource::<Restaurant>()
        .and_then(|r| r.venues.get(venue))
        .map(|v| v.portals.clone())
        .unwrap_or_default();
    for p in portals {
        refresh_portal(world, p);
    }
}

/// `toggle_open`: `open()` starts the visits (the first in 4 s), `close()` sends everyone home.
pub fn toggle_open(world: &mut World, venue: usize) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let visitors = {
        let mut r = restaurant(world);
        let Some(v) = r.venues.get_mut(venue) else {
            return;
        };
        v.open = !v.open;
        if v.open {
            v.next_visit = now + FIRST_VISIT_SECS;
            Vec::new()
        } else {
            v.visitors.iter().map(|&(c, _)| c).collect()
        }
    };
    for c in visitors {
        if let Some(mut c) = world.get_mut::<Customer>(c) {
            c.leaving = true;
        }
    }
    refresh_venue_portals(world, venue);
}

/// The ID in a hand first, then the worn one (`get_idcard(hand_first = TRUE)`): its access.
fn id_access(world: &World, mob: Entity) -> Option<Vec<String>> {
    let inv = world.get::<Inventory>(mob)?;
    inv.hands
        .iter()
        .flatten()
        .find_map(|&h| world.get::<IdCard>(h))
        .or_else(|| inv.id_card(world).and_then(|c| world.get::<IdCard>(c)))
        .map(|c| c.access.clone())
}

fn venue_access(world: &World, mob: Entity, venue: usize) -> bool {
    let Some(req) = world
        .get_resource::<Restaurant>()
        .and_then(|r| r.venues.get(venue))
        .map(|v| v.def.req_access.clone())
    else {
        return false;
    };
    id_access(world, mob).is_some_and(|a| a.contains(&req))
}

/// tg `restaurant_portal/attack_hand`. True when `target` is a portal and an ID decided.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(&Portal { venue, .. }) = world.get::<Portal>(target) else {
        return false;
    };
    if id_access(world, mob).is_none() {
        return false;
    }
    if !venue_access(world, mob, venue) {
        tell(
            world,
            client,
            Span::Warning,
            "This card lacks the access to change this venue's status.",
        );
        return true;
    }
    toggle_open(world, venue);
    true
}

/// The held item on a portal (an ID: the venue list), on a seat (its own projector removes a
/// free one) or on a customer (`on_attackby`). True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Option<Entity>,
    target: Entity,
) -> bool {
    let Some(held) = held else {
        return false;
    };
    if world.get::<Portal>(target).is_some() {
        if world.get::<IdCard>(held).is_none() {
            return false;
        }
        let items = venue_names(world);
        let prompt = ask_world(
            world,
            Ask::new(
                client,
                mob,
                "Restaurant Portal",
                "Venue",
                PromptKind::List {
                    items,
                    default: None,
                },
            ),
        );
        world.entity_mut(prompt).insert(LinkFor(target));
        return true;
    }
    if let Some(seat) = world.get::<RobotSeat>(target).copied() {
        return remove_seat(world, held, target, seat);
    }
    if world.get::<Customer>(target).is_some() {
        return handed(world, client, held, target);
    }
    false
}

fn venue_names(world: &mut World) -> Vec<String> {
    restaurant(world)
        .venues
        .iter()
        .map(|v| v.def.name.clone())
        .collect()
}

/// `robot_seat/item_interaction`: a projector of the seat's kind removes a seat nobody sits on.
fn remove_seat(world: &mut World, held: Entity, seat: Entity, s: RobotSeat) -> bool {
    let Some(item) = world.get::<DmItem>(held).map(|i| i.path.clone()) else {
        return false;
    };
    if !is_under(&item, SEAT_PROJECTOR) {
        return false;
    }
    let same = world
        .get::<crate::holosign::Holosign>(seat)
        .and_then(|h| world.get::<DmItem>(h.projector))
        .is_some_and(|p| p.path == item);
    let free = world
        .get_resource::<Restaurant>()
        .and_then(|r| r.venues.get(s.venue))
        .is_some_and(|v| v.seats.iter().any(|&(e, c)| e == seat && c.is_none()));
    if !(same && free) {
        return false;
    }
    let pos = world.get::<TilePos>(seat).copied();
    world.despawn(seat);
    if let Some(pos) = pos {
        crate::construction::tile_changed(world, pos);
    }
    true
}

/// The venue picks, revalidated (still in reach, conscious, the portal still there).
fn link_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(&LinkFor(portal)) = world.get::<LinkFor>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let able = world.get::<Portal>(portal).is_some()
            && world.get::<Stat>(actor) == Some(&Stat::Conscious)
            && crate::storage::in_reach(world, actor, portal);
        if able {
            link(world, portal, actor, i);
        }
    }
}

/// tg's venue choice of `restaurant_portal/item_interaction`.
fn link(world: &mut World, portal: Entity, actor: Entity, venue: usize) {
    let Some(old) = world.get::<Portal>(portal).map(|p| p.venue) else {
        return;
    };
    let close_old = {
        let mut r = restaurant(world);
        if venue >= r.venues.len() {
            return;
        }
        let v = &mut r.venues[old];
        v.portals.retain(|&p| p != portal);
        let close = v.open && v.portals.is_empty();
        r.venues[venue].portals.push(portal);
        close
    };
    if close_old {
        toggle_open(world, old);
    }
    world.entity_mut(portal).insert(Portal {
        venue,
        opener: Some(actor),
    });
    let name = restaurant(world).venues[venue].def.name.clone();
    if let Some(client) = world
        .get::<crate::server::PlayerMob>(actor)
        .map(|p| p.client)
    {
        tell(
            world,
            client,
            Span::Notice,
            format!("venue changed to {name}"),
        );
    }
    refresh_portal(world, portal);
}

/// `/datum/venue/process`: open venues call their guests.
// Cost: every tick one pass over the venues (two); a spawn every minute or so.
fn venues_tick(world: &mut World) {
    let Some(r) = world.get_resource::<Restaurant>() else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<usize> = r
        .venues
        .iter()
        .enumerate()
        .filter(|(_, v)| v.open && now >= v.next_visit)
        .map(|(i, _)| i)
        .collect();
    for i in due {
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let (min, max) = {
            let d = &world.resource::<Restaurant>().venues[i].def;
            (
                (d.min_visit_secs * 10.0) as u32,
                (d.max_visit_secs * 10.0) as u32,
            )
        };
        let wait = f64::from(rng.range(min, max.max(min))) / 10.0;
        world.resource_mut::<CombatRng>().0 = rng.0;
        let room = {
            let mut r = world.resource_mut::<Restaurant>();
            let v = &mut r.venues[i];
            v.next_visit = now + wait;
            let guests = v.visitors.len();
            guests < v.def.max_guests as usize && guests < v.seats.len() + 1
        };
        if room {
            create_new_customer(world, i);
        }
    }
}

/// `pick_weight` over `(item, weight)`; `None` when every weight is 0.
fn pick_weight<'a, T>(rng: &mut CombatRng, items: &'a [(T, u32)]) -> Option<&'a T> {
    let total: u32 = items.iter().map(|i| i.1).sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.range(1, total);
    items.iter().find_map(|(item, w)| {
        if roll <= *w {
            return Some(item);
        }
        roll -= w;
        None
    })
}

/// What the moth may ask of whoever linked the portal: (type, name, weight).
fn opener_clothes(world: &World, portal: Entity) -> Vec<(String, String, u32)> {
    let Some(inv) = world
        .get::<Portal>(portal)
        .and_then(|p| p.opener)
        .and_then(|o| world.get::<Inventory>(o))
    else {
        return Vec::new();
    };
    [
        (InventorySlot::Head, 5),
        (InventorySlot::Gloves, 5),
        (InventorySlot::Shoes, 1),
    ]
    .into_iter()
    .filter_map(|(slot, w)| {
        let item = inv.slot(slot)?;
        let path = world.get::<DmItem>(item)?.path.clone();
        Some((path, name_of(world, item), w))
    })
    .collect()
}

/// `customer_data/can_use`: the moth needs someone to take clothes off.
fn can_use(world: &World, data: &CustomerDef, portal: Entity) -> bool {
    let moth = data.orders.iter().any(|(_, orders)| {
        orders
            .iter()
            .any(|o| o.kind == OrderKind::MothClothing && o.weight > 0)
    });
    !moth || !opener_clothes(world, portal).is_empty()
}

/// `create_new_customer`.
fn create_new_customer(world: &mut World, venue: usize) {
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let (portals, mut types, customers) = {
        let r = world.resource::<Restaurant>();
        let v = &r.venues[venue];
        (
            v.portals.clone(),
            v.customer_types.clone(),
            r.customers.clone(),
        )
    };
    if portals.is_empty() {
        return;
    }
    let portal = portals[rng.range(0, portals.len() as u32 - 1) as usize];
    let mut chosen = None;
    while let Some(path) = pick_weight(&mut rng, &types).cloned() {
        if let Some(data) = customers.get(&path)
            && can_use(world, data, portal)
        {
            chosen = Some(data.clone());
            break;
        }
        types.retain(|(p, _)| *p != path);
    }
    world.resource_mut::<CombatRng>().0 = rng.0;
    let Some(data) = chosen else {
        return;
    };
    if data.is_unique {
        world.resource_mut::<Restaurant>().venues[venue]
            .customer_types
            .retain(|(p, _)| *p != data.path);
    }
    let Some(pos) = world.get::<TilePos>(portal).copied() else {
        return;
    };
    let Some(mob) = crate::mobs::spawn::spawn_mob(world, ROBOT_CUSTOMER, pos) else {
        return;
    };
    setup_customer(world, mob, venue, data, portal);
    world.resource_mut::<Restaurant>().venues[venue]
        .visitors
        .push((mob, portal));
}

/// `robot_customer/Initialize` with its customer data: name, tint, clothes, grime.
pub fn setup_customer(
    world: &mut World,
    mob: Entity,
    venue: usize,
    data: Arc<CustomerDef>,
    portal: Entity,
) {
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let prefix = (!data.name_prefixes.is_empty()).then(|| {
        data.name_prefixes[rng.range(0, data.name_prefixes.len() as u32 - 1) as usize].clone()
    });
    let tint = [0; 3].map(|_| rng.range(80, 255) as u8);
    let clothes = (!data.clothing_sets.is_empty()).then(|| {
        data.clothing_sets[rng.range(0, data.clothing_sets.len() as u32 - 1) as usize].clone()
    });
    let grime = if rng.chance(2.0) {
        20
    } else {
        match rng.range(1, 8) {
            5 => rng.range(0, 2),
            6 => rng.range(0, 3),
            7 => rng.range(2, 4),
            8 => 6,
            _ => 0,
        }
    };
    world.resource_mut::<CombatRng>().0 = rng.0;
    if let Some(prefix) = prefix {
        world
            .entity_mut(mob)
            .insert(Name::new(format!("{prefix}-bot")));
    }
    let (icon, state) = (data.base_icon.clone(), data.base_icon_state.clone());
    if let Some(mut basic) = world.get_mut::<crate::mobs::BasicMob>(mob) {
        basic.icon_living.clone_from(&state);
    }
    crate::mobs::set_icon_state(world, mob, &icon, &state);
    // `update_overlays`: features and clothes keep their own colours (`RESET_COLOR`); the
    // moth's jetpack (`get_overlays`).
    let mut layers = vec![format!("{state}_features")];
    layers.extend(clothes);
    if state == "mothbot" {
        layers.push("mothbot_jetpack".to_owned());
    }
    let overlays: Vec<Overlay> = layers
        .iter()
        .filter_map(|l| world.get_resource::<IconIndex>()?.get(&icon, l))
        .map(|id| Overlay {
            color: Some([255; 4]),
            ..Overlay::new(id)
        })
        .collect();
    if let Some(mut a) = world.get_mut::<Appearance>(mob) {
        a.color = Some([tint[0], tint[1], tint[2], 255]);
        a.overlays = overlays;
    }
    let patience = f64::from(data.patience_secs);
    world.entity_mut(mob).insert(Customer {
        venue,
        data,
        portal,
        order: None,
        patience,
        eating: false,
        leaving: false,
        said_cant_find: false,
        next_seat_try: 0.0,
        grime,
    });
}

/// `robot_customer/Moved`: grime falls off as it walks.
fn drop_grime(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        let Some(grime) = world.get::<Customer>(m.entity).map(|c| c.grime) else {
            continue;
        };
        if grime == 0 {
            continue;
        }
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let drop = rng.chance((grime * 20) as f32);
        world.resource_mut::<CombatRng>().0 = rng.0;
        if drop {
            crate::construction::spawn_obj(world, DIRT, m.from);
            if let Some(mut c) = world.get_mut::<Customer>(m.entity) {
                c.grime -= 1;
            }
        }
    }
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<Name>(e)
        .map(|n| n.as_str().to_owned())
        .or_else(|| world.get::<DmName>(e).map(|n| n.name.clone()))
        .unwrap_or_default()
}

/// `say()` of the customer to everyone in hearing range.
fn say(world: &mut World, mob: Entity, text: &str) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    if text.is_empty() {
        return;
    }
    let name = name_of(world, mob);
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(SendChat {
        clients,
        message: ss13_chat::server::say_message(&name, text),
    });
}

/// `visible_message` with one line for everyone.
fn visible(world: &mut World, pos: TilePos, span: Span, text: String) {
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::Info,
            segments: vec![ChatSegment::new(span, text)],
        },
    });
}

fn pick_line(rng: &mut CombatRng, lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    lines[rng.range(0, lines.len() as u32 - 1) as usize].clone()
}

/// tg `english_list`.
fn english_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// `/datum/custom_order/icecream/New`: 1-3 scoops of the vat's standard flavours, a third of
/// the time in a chocolate cone.
pub fn icecream_order(rng: &mut CombatRng) -> Order {
    let cone = if rng.chance(33.0) {
        CHOCOLATE_CONE
    } else {
        CONE
    };
    let possible: Vec<&str> = crate::icecream::FLAVOURS
        .iter()
        .filter(|f| !f.hidden && !f.custom)
        .map(|f| f.name)
        .collect();
    let count = rng.range(1, MAX_SCOOPS);
    let mut flavours: Vec<String> = (0..count)
        .map(|_| possible[rng.range(0, possible.len() as u32 - 1) as usize].to_owned())
        .collect();
    flavours.sort();
    let uniform = flavours.iter().all(|f| *f == flavours[0]);
    let name = if flavours.len() > 1 && uniform {
        let tuple = if flavours.len() == 2 {
            "double"
        } else {
            "triple"
        };
        format!("{tuple} {} ice cream ({})", flavours[0], cone.1)
    } else {
        format!("{} ice cream ({})", english_list(&flavours), cone.1)
    };
    Order::IceCream {
        cone: cone.0,
        flavours,
        name,
    }
}

/// `venue/order_food`: a weighted pick of the customer's orders at this venue.
fn order_food(
    world: &World,
    rng: &mut CombatRng,
    data: &CustomerDef,
    venue_type: &str,
    portal: Entity,
) -> Option<Order> {
    let orders = &data.orders.iter().find(|(v, _)| v == venue_type)?.1;
    let weighted: Vec<(usize, u32)> = orders
        .iter()
        .enumerate()
        .map(|(i, o)| (i, o.weight))
        .collect();
    let def = &orders[*pick_weight(rng, &weighted)?];
    Some(match def.kind {
        OrderKind::Item => Order::Item {
            path: def.path.clone(),
            name: def.name.clone(),
            price: def.price,
        },
        OrderKind::Drink => Order::Reagent {
            soup: false,
            reagent: def.path.clone(),
            name: def.name.clone(),
            price: def.price,
            needed: DRINK_UNITS,
            serving: "",
        },
        OrderKind::Soup => {
            let (serving, needed) = SERVINGS[rng.range(0, SERVINGS.len() as u32 - 1) as usize];
            Order::Reagent {
                soup: true,
                reagent: def.path.clone(),
                name: def.name.clone(),
                price: def.price,
                needed,
                serving,
            }
        }
        OrderKind::IceCream => icecream_order(rng),
        OrderKind::MothClothing => {
            let mut options = opener_clothes(world, portal);
            if options.is_empty() {
                options = MOTH_FALLBACK
                    .iter()
                    .map(|&(p, n, w)| (p.to_owned(), n.to_owned(), w))
                    .collect();
            }
            let weighted: Vec<(usize, u32)> =
                options.iter().enumerate().map(|(i, o)| (i, o.2)).collect();
            let (path, name, _) = options[*pick_weight(rng, &weighted)?].clone();
            let price = if is_under(&path, "/obj/item/clothing") {
                CLOTHING_PRICE
            } else {
                0
            };
            Order::Item { path, name, price }
        }
    })
}

/// `is_correct_order`: custom orders check themselves, the restaurant also takes the exact
/// item type.
pub fn is_correct_order(world: &World, restaurant: bool, order: &Order, item: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.as_str()) else {
        return false;
    };
    match order {
        Order::Item { path: wanted, .. } => restaurant && path == wanted,
        // `ice_cream_holder/check_food_order`.
        Order::IceCream { cone, flavours, .. } => {
            let mut scoops = world
                .get::<crate::icecream::Scoops>(item)
                .map(|s| s.0.clone())
                .unwrap_or_default();
            scoops.sort();
            path == *cone && scoops == *flavours
        }
        Order::Reagent {
            soup,
            reagent,
            needed,
            ..
        } => {
            let container = if *soup { BOWL } else { DRINKING_GLASS };
            if !is_under(path, container) {
                return false;
            }
            let (Some(chem), Some(holder)) = (
                world.get_resource::<Chem>(),
                world.get::<Reagents>(item).map(|r| &r.0),
            ) else {
                return false;
            };
            let Some(id) = chem.0.id(reagent) else {
                return false;
            };
            holder.master_reagent().is_some_and(|m| m.id == id)
                && holder.total_volume >= *needed
                && holder.amount(id) >= holder.total_volume * 0.33
        }
    }
}

/// `venue_price/produce_cash`: a holochip where the thing was, the register's ring.
fn pay(world: &mut World, venue: usize, at: TilePos, credits: u32) {
    if let Some(chip) = crate::construction::spawn_obj(world, HOLOCHIP, at) {
        world.entity_mut(chip).insert((
            Holochip { credits },
            Name::new(format!("голочип номиналом {credits}¢")),
        ));
        // `holochip/update_icon_state`.
        let state = if credits >= 1000 {
            "holochip_kilo"
        } else {
            "holochip"
        };
        crate::mobs::set_icon_state(world, chip, ECONOMY_DMI, state);
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(at, CASH_SOUND, 60.0, true),
    );
    if let Some(v) = world.resource_mut::<Restaurant>().venues.get_mut(venue) {
        v.income += u64::from(credits);
    }
}

/// `food_trash/generate_trash` on `COMSIG_ITEM_SOLD_TO_CUSTOMER`: the trash, and the hand that
/// held the food (it goes there once the food is gone).
fn sold_trash(world: &mut World, item: Entity, at: TilePos) -> Option<(Entity, Entity, usize)> {
    let trash = world
        .get::<DmFood>(item)
        .map(|f| f.trash_type.clone())
        .filter(|t| !t.is_empty())?;
    let holder = world.get::<CarriedBy>(item).map(|c| c.0);
    let hand = holder.and_then(|h| {
        world
            .get::<Inventory>(h)?
            .hands
            .iter()
            .position(|&i| i == Some(item))
    });
    let new = crate::construction::spawn_obj(world, &trash, at)?;
    Some((new, holder?, hand?))
}

/// `robot_customer controller/eat_order` -> `venue/on_get_order`: the sale.
pub fn eat_order(world: &mut World, mob: Entity, item: Entity) {
    let Some(c) = world.get::<Customer>(mob).cloned() else {
        return;
    };
    if c.eating {
        return;
    }
    let Some(order) = c.order.clone() else {
        return;
    };
    if let Some(mut c) = world.get_mut::<Customer>(mob) {
        c.eating = true;
        c.order = None;
    }
    let Some(at) = crate::chem::map_pos(world, item) else {
        return;
    };
    let restaurant = world
        .resource::<Restaurant>()
        .venues
        .get(c.venue)
        .is_some_and(|v| v.def.venue_type == VENUE_RESTAURANT);
    let (mob_name, item_name) = (name_of(world, mob), name_of(world, item));
    let mut trash = None;
    let (success, handled) = match order {
        Order::Item { price, .. } => {
            if price > 0 {
                pay(world, c.venue, at, price);
                trash = sold_trash(world, item, at);
            }
            (price > 0, false)
        }
        Order::IceCream { .. } => {
            let scoops = world
                .get::<crate::icecream::Scoops>(item)
                .map_or(0, |s| s.0.len() as u32);
            pay(world, c.venue, at, scoops * SCOOP_PRICE);
            (true, false)
        }
        Order::Reagent { soup, price, .. } => {
            let text = if soup {
                format!("{mob_name} pours {item_name} right down its hatch!")
            } else {
                format!("{mob_name} slurps up {item_name} in one go!")
            };
            visible(world, at, Span::Danger, text);
            if price > 0 {
                pay(world, c.venue, at, price);
            }
            let mut rng = CombatRng(world.resource::<CombatRng>().0);
            let volume = rng.range(10, 50) as f32;
            world.resource_mut::<CombatRng>().0 = rng.0;
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(at, DRINK_SOUND, volume, true),
            );
            if let Some(mut r) = world.get_mut::<Reagents>(item) {
                r.0.clear();
            }
            (price > 0, true)
        }
    };
    if success && let Some(v) = world.resource_mut::<Restaurant>().venues.get_mut(c.venue) {
        v.served += 1;
    }
    // `/datum/venue/restaurant/on_get_order`.
    if restaurant && success && !handled && world.get_entity(item).is_ok() {
        visible(
            world,
            at,
            Span::Danger,
            format!("{mob_name} pushes {item_name} into their mouth-shaped hole!"),
        );
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let volume = rng.range(10, 50) as f32;
        world.resource_mut::<CombatRng>().0 = rng.0;
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(at, EAT_SOUND, volume, true),
        );
        world.despawn(item);
    }
    if let Some((new, holder, hand)) = trash
        && world.get_entity(item).is_err()
    {
        crate::inventory::pick_up(world, holder, new, hand);
    }
}

/// `on_attackby`: the order is taken from the hand; a harmless wrong thing is refused.
fn handed(world: &mut World, client: Entity, held: Entity, customer: Entity) -> bool {
    let Some(c) = world.get::<Customer>(customer).cloned() else {
        return false;
    };
    let restaurant = world
        .resource::<Restaurant>()
        .venues
        .get(c.venue)
        .is_some_and(|v| v.def.venue_type == VENUE_RESTAURANT);
    if let Some(order) = &c.order
        && is_correct_order(world, restaurant, order, held)
    {
        let text = format!(
            "You hand {} to {}.",
            name_of(world, held),
            name_of(world, customer)
        );
        tell(world, client, Span::Notice, text);
        eat_order(world, customer, held);
        return true;
    }
    let force = world.get::<DmItem>(held).map_or(0.0, |i| i.force);
    if force > 0.0 {
        // The hit itself warns (`crate::mobs::ai::record_attacker`).
        return false;
    }
    say(world, customer, &c.data.wrong_item_line);
    true
}

/// `warn_greytider`: two warnings, then the offender is the target.
pub fn warn_greytider(world: &mut World, customer: Entity, offender: Entity) {
    let Some(c) = world.get::<Customer>(customer).cloned() else {
        return;
    };
    let count = {
        let mut r = restaurant(world);
        let Some(v) = r.venues.get_mut(c.venue) else {
            return;
        };
        let n = v.blacklist.entry(offender).or_insert(0);
        *n += 1;
        *n
    };
    match count {
        1 => return say(world, customer, &c.data.first_warning_line),
        2 => return say(world, customer, &c.data.second_warning_line),
        3 => say(world, customer, &c.data.self_defense_line),
        _ => {}
    }
    if let Some(mut ai) = world.get_mut::<AiController>(customer) {
        ai.bb.set(Key::CurrentTarget, Value::Entity(offender));
        ai.cancel = true;
    }
}

/// `on_attacked` (`relay_attackers`), from `crate::mobs::ai::record_attacker`.
pub fn on_attacked(world: &mut World, victim: Entity, attacker: Entity) {
    if victim != attacker && world.get::<Customer>(victim).is_some() {
        warn_greytider(world, victim, attacker);
    }
}

/// `on_get_pulled`: staff of the venue are asked where to; anyone else is warned and the
/// customer breaks free (`resist`: a passive pull always lets go).
fn on_pulled(
    add: On<Add<crate::pull::PulledBy>>,
    customers: Query<(), With<Customer>>,
    mut commands: Commands,
) {
    if !customers.contains(add.entity) {
        return;
    }
    let customer = add.entity;
    commands.queue(move |world: &mut World| {
        let Some(puller) = world
            .get::<crate::pull::PulledBy>(customer)
            .map(crate::pull::PulledBy::puller)
        else {
            return;
        };
        let Some(c) = world.get::<Customer>(customer).cloned() else {
            return;
        };
        if venue_access(world, puller, c.venue) {
            say(world, customer, &c.data.friendly_pull_line);
            return;
        }
        warn_greytider(world, customer, puller);
        if world.get_entity(puller).is_ok() {
            world.entity_mut(puller).remove::<crate::pull::Pulling>();
        }
    });
}

/// `robot_customer/examine`: what it ordered.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(order) = world.get::<Customer>(e).and_then(|c| c.order.as_ref()) {
        lines.push((
            Span::Notice,
            format!("Their order was: \"{}.\"", order.line()),
        ));
    }
}

/// The customer decorators (`bb_key_set` on `BB_CUSTOMER_LEAVING`, `BB_CUSTOMER_CURRENT_ORDER`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomerCond {
    Leaving,
    HasOrder,
}

impl CustomerCond {
    pub fn check(self, ctx: &AiCtx) -> bool {
        let Some(c) = ctx.world.get::<Customer>(ctx.pawn) else {
            return false;
        };
        match self {
            Self::Leaving => c.leaving,
            Self::HasOrder => c.order.is_some(),
        }
    }
}

/// The `/datum/bt_node/ai_behavior/robot_customer/*` leaves and `break_spine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerLeaf {
    FindExit,
    Leave,
    BreakSpine,
    FindSeat,
    Order,
    WaitForFood,
}

const fn done(ok: bool) -> Perform {
    Perform {
        delay: false,
        outcome: Some(ok),
    }
}

/// `robot_customer.bt.json`.
pub fn tree() -> Built {
    let c = |l: CustomerLeaf| leaf(Behavior::Customer(l));
    let walk = |key: Key, dist: u32| {
        leaf(Behavior::MoveToTarget {
            key,
            dist,
            finish_on_arrival: true,
        })
    };
    let root = selector(vec![
        decorator(
            Cond::Customer(CustomerCond::Leaving),
            false,
            Abort::Lower,
            sequence(vec![
                c(CustomerLeaf::FindExit),
                walk(Key::CustomerExit, 0),
                c(CustomerLeaf::Leave),
            ]),
        ),
        decorator(
            Cond::KeySet(Key::CurrentTarget),
            false,
            Abort::Lower,
            sequence(vec![
                walk(Key::CurrentTarget, 1),
                c(CustomerLeaf::BreakSpine),
            ]),
        ),
        decorator(
            Cond::KeySet(Key::CustomerSeat),
            true,
            Abort::None,
            c(CustomerLeaf::FindSeat),
        ),
        decorator(
            Cond::KeySet(Key::CustomerSeat),
            false,
            Abort::None,
            sequence(vec![
                walk(Key::CustomerSeat, 0),
                selector(vec![
                    decorator(
                        Cond::Customer(CustomerCond::HasOrder),
                        true,
                        Abort::None,
                        c(CustomerLeaf::Order),
                    ),
                    c(CustomerLeaf::WaitForFood),
                ]),
            ]),
        ),
    ]);
    Built {
        roots: vec![root],
        blackboard: Vec::new(),
        traits: 0,
        retaliates: false,
    }
}

/// A chair under the customer on its seat turns it the chair's way.
fn face_chair(ctx: &mut AiCtx) {
    let (Some(pos), Some(seat)) = (ctx.pawn_pos(), ctx.target_pos(Key::CustomerSeat)) else {
        return;
    };
    if pos != seat {
        return;
    }
    let world = &*ctx.world;
    let chair = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .find(|&&e| {
            world
                .get::<DmObj>(e)
                .is_some_and(|o| is_under(&o.path, CHAIR))
        })
        .and_then(|&e| world.get::<Facing>(e).copied());
    if let Some(facing) = chair
        && world.get::<Facing>(ctx.pawn) != Some(&facing)
    {
        ctx.world.entity_mut(ctx.pawn).insert(facing);
    }
}

impl CustomerLeaf {
    pub fn perform(&mut self, ctx: &mut AiCtx) -> Perform {
        let Some(c) = ctx.world.get::<Customer>(ctx.pawn).cloned() else {
            return done(false);
        };
        match *self {
            // `find_exit_portal`; with its portal gone the customer just leaves.
            Self::FindExit => {
                if ctx.truthy(Key::CustomerExit) {
                    return done(true);
                }
                if ctx.world.get::<Portal>(c.portal).is_none() {
                    ctx.world.despawn(ctx.pawn);
                    return done(false);
                }
                ctx.bb.set(Key::CustomerExit, Value::Entity(c.portal));
                done(true)
            }
            // `leave_venue`.
            Self::Leave => {
                ctx.world.despawn(ctx.pawn);
                done(true)
            }
            Self::BreakSpine => break_spine(ctx),
            Self::FindSeat => find_seat(ctx, &c),
            // `order_food`.
            Self::Order => {
                face_chair(ctx);
                let venue_type = ctx
                    .world
                    .resource::<Restaurant>()
                    .venues
                    .get(c.venue)
                    .map(|v| v.def.venue_type.clone())
                    .unwrap_or_default();
                let order = order_food(ctx.world, &mut ctx.rng, &c.data, &venue_type, c.portal);
                if let Some(order) = order {
                    say(ctx.world, ctx.pawn, &order.line());
                    if let Some(mut c) = ctx.world.get_mut::<Customer>(ctx.pawn) {
                        c.order = Some(order);
                    }
                }
                done(true)
            }
            Self::WaitForFood => wait_for_food(ctx, c),
        }
    }

    pub fn finish(&mut self, ctx: &mut AiCtx, succeeded: bool) {
        match *self {
            // `break_spine/finish_action`.
            Self::BreakSpine if succeeded => ctx.bb.clear(Key::CurrentTarget),
            // `wait_for_food/finish_action`: off home, happy or mad (not while it goes for
            // someone, nor when gone).
            Self::WaitForFood => {
                if ctx.truthy(Key::CurrentTarget) {
                    return;
                }
                let Some(data) = ctx.world.get::<Customer>(ctx.pawn).map(|c| c.data.clone()) else {
                    return;
                };
                if let Some(mut c) = ctx.world.get_mut::<Customer>(ctx.pawn) {
                    c.leaving = true;
                }
                let lines = if succeeded {
                    &data.leave_happy_lines
                } else {
                    &data.leave_mad_lines
                };
                let line = pick_line(&mut ctx.rng, lines);
                say(ctx.world, ctx.pawn, &line);
            }
            _ => {}
        }
    }
}

/// `break_spine/perform`: a too tight hug until the target goes down.
fn break_spine(ctx: &mut AiCtx) -> Perform {
    let Some(target) = ctx
        .bb
        .entity(Key::CurrentTarget)
        .filter(|&t| ctx.world.get_entity(t).is_ok())
    else {
        return done(false);
    };
    let close = ctx
        .pawn_pos()
        .zip(ctx.world.get::<TilePos>(target).copied())
        .and_then(|(a, b)| a.chebyshev(b))
        .is_some_and(|d| d < GIVE_UP_DISTANCE);
    if !close {
        return done(false);
    }
    let (Some(mut health), Some(stat)) = (
        ctx.world.get::<Health>(target).cloned(),
        ctx.world.get::<Stat>(target).copied(),
    ) else {
        return done(false);
    };
    if stat != Stat::Conscious {
        return done(true);
    }
    let (target_name, pawn_name) = (name_of(ctx.world, target), name_of(ctx.world, ctx.pawn));
    if let Some(pos) = ctx.world.get::<TilePos>(target).copied() {
        visible(
            ctx.world,
            pos,
            Span::Warning,
            format!("{target_name} gets a slightly too tight hug from {pawn_name}!"),
        );
    }
    let mut new = stat;
    if health.basic.is_some() {
        crate::health::apply_damage(
            &mut health,
            &mut new,
            DamageType::Brute,
            HUG_DAMAGE * (BodyZone::ALL.len() - 1) as f32,
        );
    } else {
        for zone in BodyZone::ALL.into_iter().filter(|&z| z != BodyZone::Head) {
            crate::health::apply_damage_zone(
                &mut health,
                &mut new,
                zone,
                DamageType::Brute,
                HUG_DAMAGE,
            );
        }
    }
    let mut e = ctx.world.entity_mut(target);
    e.insert(health);
    if new != stat {
        e.insert(new);
    }
    done(true)
}

/// `find_seat/perform` behind its 8 s cooldown.
fn find_seat(ctx: &mut AiCtx, c: &Customer) -> Perform {
    if ctx.now < c.next_seat_try {
        return done(false);
    }
    if let Some(mut c) = ctx.world.get_mut::<Customer>(ctx.pawn) {
        c.next_seat_try = ctx.now + FIND_SEAT_COOLDOWN;
    }
    let Some(from) = ctx.pawn_pos() else {
        return done(false);
    };
    let found = {
        let world = &*ctx.world;
        let free: Vec<Entity> = world
            .resource::<Restaurant>()
            .venues
            .get(c.venue)
            .map(|v| {
                v.seats
                    .iter()
                    .filter(|s| s.1.is_none())
                    .map(|s| s.0)
                    .collect()
            })
            .unwrap_or_default();
        let index = world.resource::<SpatialIndex>();
        index.in_radius(from, SEAT_VISION).find(|&e| {
            free.contains(&e)
                && world.get::<TilePos>(e).is_some_and(|&p| {
                    index.dense_on_tile(p).iter().all(|&d| d == ctx.pawn)
                        && can_see(world, from, p, SEAT_VISION)
                })
        })
    };
    if let Some(seat) = found {
        let line = pick_line(&mut ctx.rng, &c.data.found_seat_lines);
        say(ctx.world, ctx.pawn, &line);
        ctx.bb.set(Key::CustomerSeat, Value::Entity(seat));
        if let Some(v) = ctx
            .world
            .resource_mut::<Restaurant>()
            .venues
            .get_mut(c.venue)
            && let Some(s) = v.seats.iter_mut().find(|s| s.0 == seat)
        {
            s.1 = Some(ctx.pawn);
        }
        return done(true);
    }
    if !c.said_cant_find || ctx.spt_prob(1.5) {
        let line = pick_line(&mut ctx.rng, &c.data.cant_find_seat_lines);
        say(ctx.world, ctx.pawn, &line);
        if let Some(mut c) = ctx.world.get_mut::<Customer>(ctx.pawn) {
            c.said_cant_find = true;
        }
    }
    done(false)
}

/// `wait_for_food/perform`: patience runs out; matching food in front is the order.
fn wait_for_food(ctx: &mut AiCtx, c: Customer) -> Perform {
    if c.eating {
        return Perform {
            delay: true,
            outcome: Some(true),
        };
    }
    let patience = c.patience - ctx.spt;
    if let Some(mut c) = ctx.world.get_mut::<Customer>(ctx.pawn) {
        c.patience = patience;
    }
    if patience < 0.0 || c.leaving {
        return Perform {
            delay: true,
            outcome: Some(false),
        };
    }
    if ctx.spt_prob(0.85) {
        let line = pick_line(&mut ctx.rng, &c.data.wait_for_food_lines);
        say(ctx.world, ctx.pawn, &line);
    }
    face_chair(ctx);
    let (Some(pos), Some(order)) = (ctx.pawn_pos(), c.order.as_ref()) else {
        return Perform {
            delay: true,
            outcome: None,
        };
    };
    let facing = ctx
        .world
        .get::<Facing>(ctx.pawn)
        .map_or(ss13_shared::Dir::South, |f| f.0);
    let front = pos.step(facing);
    let restaurant = ctx
        .world
        .resource::<Restaurant>()
        .venues
        .get(c.venue)
        .is_some_and(|v| v.def.venue_type == VENUE_RESTAURANT);
    let found = ctx
        .world
        .resource::<SpatialIndex>()
        .on_tile(front)
        .iter()
        .copied()
        .find(|&e| {
            ctx.world.get::<CarriedBy>(e).is_none()
                && is_correct_order(ctx.world, restaurant, order, e)
        });
    if let Some(item) = found {
        eat_order(ctx.world, ctx.pawn, item);
    }
    Perform {
        delay: true,
        outcome: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_mapload::restaurant::OrderDef;

    #[test]
    fn order_lines_follow_the_fork() {
        let item = Order::Item {
            path: "/obj/item/food/burger/plain".into(),
            name: "plain burger".into(),
            price: 70,
        };
        assert_eq!(item.line(), "Я буду plain burger");
        let drink = Order::Reagent {
            soup: false,
            reagent: "/datum/reagent/consumable/ethanol/beer".into(),
            name: "Beer".into(),
            price: 30,
            needed: DRINK_UNITS,
            serving: "",
        };
        assert_eq!(drink.line(), "Я буду 10u of Beer");
        let soup = Order::Reagent {
            soup: true,
            reagent: "/datum/reagent/consumable/nutriment/soup/stew".into(),
            name: "Stew".into(),
            price: 220,
            needed: 20.0,
            serving: SERVINGS[1].0,
        };
        assert_eq!(soup.line(), "Я буду среднюю порцию (20u) Stew");
    }

    #[test]
    fn ice_cream_orders_are_sorted_and_named() {
        let mut rng = CombatRng(7);
        for _ in 0..50 {
            let Order::IceCream {
                cone,
                flavours,
                name,
            } = icecream_order(&mut rng)
            else {
                panic!("an ice cream order");
            };
            assert!((1..=3).contains(&flavours.len()));
            assert!(flavours.windows(2).all(|w| w[0] <= w[1]));
            assert!(cone == CONE.0 || cone == CHOCOLATE_CONE.0);
            assert!(name.ends_with("cone)"), "{name}");
            assert!(flavours.iter().all(|f| {
                crate::icecream::FLAVOURS
                    .iter()
                    .any(|x| x.name == f.as_str() && !x.hidden && !x.custom)
            }));
        }
        assert_eq!(
            english_list(&["a".into(), "b".into(), "c".into()]),
            "a, b, and c"
        );
        assert_eq!(english_list(&["a".into(), "b".into()]), "a and b");
    }

    #[test]
    fn the_tree_builds() {
        let built = tree();
        let tree = crate::mobs::ai::bt::Tree::new(built.roots);
        assert_eq!(tree.roots.len(), 1);
        assert_eq!(tree.roots[0].index, 1);
    }

    #[test]
    fn weighted_picks_skip_zero_weights() {
        let mut rng = CombatRng(3);
        let items = [("never", 0), ("always", 5)];
        for _ in 0..20 {
            assert_eq!(pick_weight(&mut rng, &items), Some(&"always"));
        }
        assert_eq!(pick_weight::<&str>(&mut rng, &[("x", 0)]), None);
    }

    fn world_with_venues() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.insert_resource(CombatRng(1));
        let customer = CustomerDef {
            path: "/datum/customer_data/american".into(),
            patience_secs: 600.0,
            orders: vec![(
                VENUE_RESTAURANT.into(),
                vec![OrderDef {
                    kind: OrderKind::Item,
                    path: "/obj/item/food/burger/plain".into(),
                    name: "plain burger".into(),
                    price: 70,
                    weight: 1,
                }],
            )],
            ..Default::default()
        };
        world.insert_resource(Restaurant::from_data(RestaurantData {
            venues: vec![VenueDef {
                path: "/datum/venue/restaurant".into(),
                name: "restaurant".into(),
                venue_type: VENUE_RESTAURANT.into(),
                req_access: "kitchen".into(),
                max_guests: 6,
                min_visit_secs: 80.0,
                max_visit_secs: 100.0,
                customers: vec![(customer.path.clone(), 1)],
            }],
            customers: vec![customer],
            portals: vec![(
                "/obj/machinery/restaurant_portal/restaurant".into(),
                "/datum/venue/restaurant".into(),
            )],
            seats: Vec::new(),
        }));
        world
    }

    /// An exact item order is the restaurant's (the bar takes custom orders only); handing
    /// over ends the wait once. Without a `venue_price` (price 0) nothing is sold: not counted,
    /// not eaten (the paid path spawns a holochip and needs the prototypes: manual check).
    #[test]
    fn orders_match_and_end_the_wait_once() {
        let mut world = world_with_venues();
        let data =
            world.resource::<Restaurant>().customers["/datum/customer_data/american"].clone();
        let portal = world.spawn_empty().id();
        let mob = world.spawn(TilePos::new(1, 1, 1)).id();
        world.entity_mut(mob).insert(Customer {
            venue: 0,
            data,
            portal,
            order: Some(Order::Item {
                path: "/obj/item/food/burger/plain".into(),
                name: "plain burger".into(),
                price: 0,
            }),
            patience: 600.0,
            eating: false,
            leaving: false,
            said_cant_find: false,
            next_seat_try: 0.0,
            grime: 0,
        });
        let item = |world: &mut World, path: &str| {
            world
                .spawn((
                    DmItem {
                        path: path.into(),
                        ..default()
                    },
                    TilePos::new(1, 2, 1),
                ))
                .id()
        };
        let wrong = item(&mut world, "/obj/item/food/burger/cheese");
        let burger = item(&mut world, "/obj/item/food/burger/plain");
        let order = world.get::<Customer>(mob).unwrap().order.clone().unwrap();
        assert!(!is_correct_order(&world, true, &order, wrong));
        assert!(is_correct_order(&world, true, &order, burger));
        assert!(!is_correct_order(&world, false, &order, burger));
        eat_order(&mut world, mob, burger);
        let c = world.get::<Customer>(mob).unwrap();
        assert!(c.eating && c.order.is_none());
        let v = &world.resource::<Restaurant>().venues[0];
        assert_eq!((v.served, v.income), (0, 0));
        assert!(world.get_entity(burger).is_ok(), "unsold, not eaten");
        assert_eq!(
            world
                .resource::<Restaurant>()
                .venue_of("/obj/machinery/restaurant_portal/restaurant"),
            Some(0)
        );
    }
}
