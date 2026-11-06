//! The mining equipment order console (server), after tg
//! `/obj/machinery/computer/order_console/mining` (`code/game/machinery/computer/orders/`) with
//! tgui's `ProduceConsole`: a cart of `/datum/orderable_item`s of its categories (their
//! catalogue comes resolved with the console's prototype, [`ss13_mapload::DmOrderConsole`]),
//! paid with the mining points of the user's ID's account.
//!
//! - `purchase`: at least `CARGO_CRATE_VALUE` (200) worth, at `cargo_cost_multiplier` (0.65);
//!   the items ship in a crate on the next supply shuttle from CentCom ([`ship_orders`]).
//! - `express`: at `express_cost_multiplier`, straight to the buyer's tile; the console then
//!   rests `cooldown_time × express_cost_multiplier`.
// shortcut: express orders appear on the buyer's tile without the teleporter supply pod, no
// supply radio announcement, no golem or cook
// consoles (only the mining console has this server half).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmName, DmObj, DmOrderConsole, IconIndex};
use ss13_proto::Prototypes;
use ss13_shared::TilePos;
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::interfaces::vending::ProductIcon;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::{
    ADD_ONE, AMT, CART_SET, EXPRESS, ItemAmount, OrderDatum, PURCHASE, ProduceConsoleUi,
    ProduceData, ProduceStatic, REMOVE_ONE, TARGET,
};
use crate::cargo::console::machine_say;
use crate::cargo::economy::{BankAccountId, Economy};
use crate::cargo::supply::CARGO_CRATE_VALUE;
use crate::inventory::{IdCards, Inventory};
use crate::obj_damage::is_under;
use crate::power::Unpowered;

const MINING_CONSOLE: &str = "/obj/machinery/computer/order_console/mining";
const CRATE: &str = "/obj/structure/closet/crate";

/// A console's state: the cart (`grocery_list`, catalogue index -> amount) and
/// `order_cooldown`.
#[derive(Component, Clone, Debug, Default)]
pub struct OrderConsole {
    pub cart: Vec<(usize, u32)>,
    pub ready_at: f64,
    window: ProduceStatic,
}

/// Paid cargo orders waiting for the supply shuttle (`SSshuttle.shopping_list` disposable
/// orders): each is a crate's contents.
#[derive(Resource, Default, Debug)]
pub struct MiningShipments(pub Vec<Vec<String>>);

pub(super) struct OrderConsolePlugin;

impl Plugin for OrderConsolePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<ProduceConsoleUi>()
            .init_resource::<MiningShipments>()
            .add_systems(Startup, setup_consoles)
            .add_systems(
                Update,
                (
                    console_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

fn icon(world: &World, path: &str) -> Option<ProductIcon> {
    let atom = world
        .get_resource::<Prototypes>()?
        .get(path)?
        .get::<DmAtom>()
        .cloned()?;
    let id = world
        .get_resource::<IconIndex>()?
        .get(&atom.icon, &atom.icon_state)?;
    Some(ProductIcon {
        icon_state: id,
        color: None,
    })
}

/// `ui_static_data` of a console, built once.
fn window(world: &World, dm: &DmOrderConsole) -> ProduceStatic {
    ProduceStatic {
        credit_type: dm.credit_type.clone(),
        express_tooltip: dm.express_tooltip.clone(),
        purchase_tooltip: dm.purchase_tooltip.clone(),
        forced_express: dm.forced_express,
        cargo_value: CARGO_CRATE_VALUE as u32,
        cargo_cost_multiplier: dm.cargo_cost_multiplier,
        express_cost_multiplier: dm.express_cost_multiplier,
        order_categories: dm.categories.clone(),
        order_datums: dm
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| OrderDatum {
                name: crate::vending::plain_name(&item.name).to_owned(),
                desc: item.desc.clone(),
                cat: item.category.clone(),
                r#ref: i as u32,
                cost: (item.cost as f32 * dm.cargo_cost_multiplier).round() as u32,
                max: item.max,
                icon: icon(world, &item.purchase_path),
            })
            .collect(),
    }
}

fn setup_consoles(world: &mut World) {
    let mut query = world.query::<(Entity, &DmObj, &DmOrderConsole)>();
    let found: Vec<(Entity, DmOrderConsole)> = query
        .iter(world)
        .filter(|(_, obj, _)| is_under(&obj.path, MINING_CONSOLE))
        .map(|(e, _, dm)| (e, dm.clone()))
        .collect();
    for (e, dm) in found {
        let window = window(world, &dm);
        world.entity_mut(e).insert(OrderConsole {
            window,
            ..default()
        });
    }
}

/// An empty hand opens the window. True when `target` is an order console.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<OrderConsole>(target).is_none() {
        return false;
    }
    let name = world
        .get::<DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<ProduceConsoleUi>(client, mob, target, name));
    true
}

fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<OrderConsole>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == ProduceConsoleUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

/// `get_total_cost()`: the cart at catalogue prices.
pub fn total_cost(dm: &DmOrderConsole, cart: &[(usize, u32)]) -> u32 {
    cart.iter()
        .filter_map(|&(i, n)| Some(dm.items.get(i)?.cost * n))
        .sum()
}

type ConsoleParams = (
    Query<'static, 'static, &'static Inventory>,
    IdCards<'static, 'static>,
    Query<'static, 'static, &'static BankAccountId>,
    Option<Res<'static, Economy>>,
    Res<'static, Time>,
);

impl ServerUi for ProduceConsoleUi {
    type Source = (&'static OrderConsole, &'static DmOrderConsole);
    type UserParam = ConsoleParams;
    type Act = ConsoleAct;

    fn static_data((console, _): (&OrderConsole, &DmOrderConsole)) -> ProduceStatic {
        console.window.clone()
    }

    fn data(
        (console, dm): (&OrderConsole, &DmOrderConsole),
        user: Entity,
        (inventories, cards, accounts, economy, time): &SystemParamItem<'_, '_, ConsoleParams>,
    ) -> ProduceData {
        let card = inventories
            .get(user)
            .ok()
            .and_then(|i| i.id_search().find_map(|e| cards.id_of(e)));
        // `retrieve_points`: the card's account's mining points (a card without one shows 0).
        let points = card.map(|c| {
            accounts
                .get(c)
                .ok()
                .and_then(|a| economy.as_ref()?.account(a.0))
                .map_or(0, |a| a.mining_points)
        });
        ProduceData {
            total_cost: total_cost(dm, &console.cart),
            off_cooldown: time.elapsed_secs_f64() >= console.ready_at,
            item_amts: console
                .cart
                .iter()
                .filter_map(|&(i, amt)| {
                    Some(ItemAmount {
                        name: console.window.order_datums.get(i)?.name.clone(),
                        amt,
                    })
                })
                .collect(),
            points,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ConsoleAct, UiReject> {
        let target = || -> Result<usize, UiReject> {
            let n = params.number(TARGET)?;
            if n < 0.0 || n.fract() != 0.0 || n > f64::from(u16::MAX) {
                return Err(UiReject::BadParams("target: not an index".into()));
            }
            Ok(n as usize)
        };
        Ok(match action {
            ADD_ONE => ConsoleAct::AddOne(target()?),
            REMOVE_ONE => ConsoleAct::RemoveOne(target()?),
            CART_SET => ConsoleAct::Set(target()?, params.number(AMT)?),
            PURCHASE => ConsoleAct::Purchase,
            EXPRESS => ConsoleAct::Express,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConsoleAct {
    AddOne(usize),
    RemoveOne(usize),
    /// Catalogue index, amount.
    Set(usize, f64),
    Purchase,
    Express,
}

fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ProduceConsoleUi>>>) {
    let pending: Vec<(Entity, Entity, ConsoleAct)> = acts
        .read(world.resource::<Messages<UiActed<ProduceConsoleUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (user, console, act) in pending {
        apply(world, console, user, act);
    }
}

/// tg `ui_act`, checked at the moment of effect.
pub fn apply(world: &mut World, console: Entity, user: Entity, act: ConsoleAct) {
    let (Some(state), Some(dm)) = (
        world.get::<OrderConsole>(console).cloned(),
        world.get::<DmOrderConsole>(console).cloned(),
    ) else {
        return;
    };
    let mut cart = state.cart.clone();
    let amount_of =
        |cart: &[(usize, u32)], i: usize| cart.iter().find(|(j, _)| *j == i).map_or(0, |(_, n)| *n);
    let set = |cart: &mut Vec<(usize, u32)>, i: usize, n: u32| {
        cart.retain(|(j, _)| *j != i);
        if n > 0 {
            cart.push((i, n));
        }
    };
    match act {
        ConsoleAct::AddOne(i) => {
            let Some(item) = dm.items.get(i) else {
                return;
            };
            let have = amount_of(&cart, i);
            if have >= item.max {
                return;
            }
            set(&mut cart, i, have + 1);
        }
        ConsoleAct::RemoveOne(i) => {
            let have = amount_of(&cart, i);
            if have == 0 {
                return;
            }
            set(&mut cart, i, have - 1);
        }
        ConsoleAct::Set(i, amount) => {
            let Some(item) = dm.items.get(i) else {
                return;
            };
            if !amount.is_finite() {
                return;
            }
            let n = amount.round().clamp(0.0, f64::from(item.max)) as u32;
            set(&mut cart, i, n);
        }
        ConsoleAct::Purchase if dm.forced_express => {
            return apply(world, console, user, ConsoleAct::Express);
        }
        ConsoleAct::Purchase => {
            if !order(world, console, user, &dm, &state, false) {
                return;
            }
            cart.clear();
        }
        ConsoleAct::Express => {
            if !order(world, console, user, &dm, &state, true) {
                return;
            }
            cart.clear();
        }
    }
    if let Some(mut c) = world.get_mut::<OrderConsole>(console) {
        c.cart = cart;
    }
}

/// `purchase`/`express`: the cooldown, the ID's account, the points; then the delivery. True
/// when ordered.
fn order(
    world: &mut World,
    console: Entity,
    user: Entity,
    dm: &DmOrderConsole,
    state: &OrderConsole,
    express: bool,
) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if state.cart.is_empty() || now < state.ready_at {
        return false;
    }
    let total = total_cost(dm, &state.cart);
    let multiplier = if express {
        dm.express_cost_multiplier
    } else {
        dm.cargo_cost_multiplier
    };
    // "So miners cant spam buy crates for a very low price".
    if !express && f64::from(total) < CARGO_CRATE_VALUE {
        return false;
    }
    let account = world
        .get::<Inventory>(user)
        .and_then(|i| i.id_card(world))
        .and_then(|c| crate::cargo::economy::account_of(world, c));
    let Some(account) = account else {
        machine_say(world, console, "No bank account detected!");
        return false;
    };
    // `purchase_items` -> `subtract_points`.
    let cost = (total as f32 * multiplier).round() as i64;
    let paid = world.get_resource_mut::<Economy>().is_some_and(|mut e| {
        e.account_mut(account).is_some_and(|a| {
            if cost <= a.mining_points {
                a.mining_points -= cost;
                true
            } else {
                false
            }
        })
    });
    if !paid && cost > 0 {
        let text = format!("Sorry, but you do not have enough {}.", dm.credit_type);
        machine_say(world, console, &text);
        return false;
    }
    let paths: Vec<String> = state
        .cart
        .iter()
        .filter_map(|&(i, n)| Some((dm.items.get(i)?.purchase_path.clone(), n)))
        .flat_map(|(p, n)| std::iter::repeat_n(p, n as usize))
        .collect();
    let cooldown = f64::from(dm.cooldown) / 10.0;
    if express {
        let Some(&at) = world.get::<TilePos>(user) else {
            return true;
        };
        for path in &paths {
            crate::construction::spawn_obj(world, path, at);
        }
        let mut text = "Thank you for your purchase!".to_owned();
        if dm.express_cost_multiplier > 1.0 {
            text.push_str(&format!(
                " Please note: The charge of this purchase and machine cooldown has been multiplied by {}!",
                dm.express_cost_multiplier
            ));
        }
        machine_say(world, console, &text);
        set_ready(
            world,
            console,
            now + cooldown * f64::from(dm.express_cost_multiplier),
        );
    } else {
        world.resource_mut::<MiningShipments>().0.push(paths);
        machine_say(
            world,
            console,
            "Thank you for your purchase! It will arrive on the next cargo shuttle!",
        );
        set_ready(world, console, now + cooldown);
    }
    true
}

fn set_ready(world: &mut World, console: Entity, at: f64) {
    if let Some(mut c) = world.get_mut::<OrderConsole>(console) {
        c.ready_at = at;
    }
}

/// From `crate::cargo::supply::buy`: each waiting mining order becomes a crate of its items on
/// one of the shuttle's free `turfs` (taken from the list); orders without a free turf wait.
pub fn ship_orders(world: &mut World, turfs: &mut Vec<TilePos>) {
    let Some(mut orders) = world
        .get_resource_mut::<MiningShipments>()
        .map(|mut s| std::mem::take(&mut s.0))
    else {
        return;
    };
    let mut kept = Vec::new();
    for paths in orders.drain(..) {
        let Some(pos) = turfs.pop() else {
            kept.push(paths);
            continue;
        };
        let Some(crate_entity) = crate::construction::spawn_obj(world, CRATE, pos) else {
            kept.push(paths);
            continue;
        };
        // As `supply::generate`: closets get their state at round start, a new crate here.
        let closet = {
            let icons = world.get_resource::<IconIndex>();
            let entity = world.entity(crate_entity);
            match (
                entity.get::<ss13_mapload::DmCloset>(),
                entity.get::<DmAtom>(),
            ) {
                (Some(dm), Some(atom)) => Some(crate::closets::Closet::from_dm(dm, atom, icons)),
                _ => None,
            }
        };
        if let Some(mut closet) = closet {
            closet.opened = false;
            world.entity_mut(crate_entity).insert(closet);
            crate::closets::refresh_look(world, crate_entity);
        }
        for path in paths {
            if let Some(item) = crate::construction::spawn_detached(world, &path) {
                crate::storage::put_inside(world, item, crate_entity);
            }
        }
        info!("cargo: a mining order shipped");
    }
    world.resource_mut::<MiningShipments>().0 = kept;
}
