//! The supply consoles (server), after tg `/obj/machinery/computer/cargo`
//! (`code/modules/cargo/orderconsole.dm`) and its `request` subtype, with tgui's `Cargo`
//! interface: a click with an empty hand or an ID card opens the window.
//!
//! The acts follow tg's `ui_act`, checked here at the moment of effect: the pack exists and the
//! console may order it ([`orderable`]), at most `CARGO_MAX_ORDER` of a pack, private purchases
//! need an ID with a bank account and the pack's `access_view`, the request console files
//! requests with a reason. tg trusts its UI for what the request console hides; here the server
//! refuses `send`, `approve`, `deny`, `denyall`, `clear`, `remove` and `modify` from a console
//! without `can_send` / `can_approve_requests`, and from an unpowered one. The main console has
//! no `req_access` in tg (the cargo office's doors guard it), so it has none here either.
//!
//! A request may be for the requester's department budget (the `department` param stands in
//! for tg's `tgui_alert`): that department pays when the order ships, and a pack with an
//! `access_view` the requester's ID lacks is refused. The request console announces new
//! requests on the supply channel, at most every 30 s.
//!
//! A coupon used on a console goes inside ([`super::coupon`]); an order of its pack takes it
//! (the discount off its cost), cancelling the order drops it back on the console's tile.
// shortcut: no hacked boards (contraband) or loans.

use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmName, DmObj};
use ss13_net::chat::server::SendChat;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::interfaces::vending::ProductIcon;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::WorldSystems;

use super::economy::{ACCOUNT_CAR, Economy};
use super::supply::{
    CARGO_MAX_ORDER, DEFAULT_MESSAGE, SupplyMode, SupplyOrder, SupplyOrders, SupplyPacks,
    SupplyShuttle, can_move, orderable, request,
};
use super::*;
use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::{IdCard, Inventory};
use crate::obj_damage::is_under;
use crate::outfit::IdRegistration;
use crate::power::Unpowered;
use crate::radio::MachineRadio;

const CARGO_CONSOLE: &str = "/obj/machinery/computer/cargo";
const REQUEST_CONSOLE: &str = "/obj/machinery/computer/cargo/request";
/// Other `/obj/machinery/computer/cargo` subtypes with their own interfaces.
const EXPRESS_CONSOLE: &str = "/obj/machinery/computer/cargo/express";
const SAFETY_WARNING: &str = "For safety and ethical reasons, the automated supply shuttle cannot \
     transport live organisms, human remains, classified nuclear weaponry, mail, undelivered \
     departmental order crates, syndicate bombs, homing beacons, unstable eigenstates, fax \
     machines, or machinery housing any form of artificial intelligence.";
/// Tiles a machine's `say` reaches (`world.view`).
const SAY_RANGE: u32 = 7;
/// The request console's `message_cooldown` between supply channel announcements.
const ANNOUNCE_COOLDOWN_SECS: f64 = 30.0;

/// A supply console's flags (tg `can_send`, `requestonly`, `can_approve_requests`,
/// `contraband`, `self_paid`) and its catalogue.
#[derive(Component, Clone, Debug)]
pub struct CargoConsole {
    pub request_only: bool,
    pub can_send: bool,
    pub can_approve_requests: bool,
    pub contraband: bool,
    /// tg `obj_flags & EMAGGED` ([`emag_act`]).
    pub emagged: bool,
    pub self_paid: bool,
    catalogue: Arc<CargoStatic>,
    /// `message_cooldown`: server time of the next allowed announcement.
    next_announcement: f64,
    /// `loaded_coupons`: coupons inside, used up by matching orders ([`super::coupon`]).
    pub loaded_coupons: Vec<Entity>,
}

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<CargoUi>()
            .add_systems(
                ss13_world::MapInit,
                setup_consoles.after(super::supply::load_packs),
            )
            .add_systems(
                Update,
                (
                    open_console
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    console_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

/// The catalogue a console with `contraband` (and `emagged`: the `ORDER_EMAG_ONLY` packs) shows
/// (tg `get_packs_data_by_group` with the catalogue asset's pack data).
pub fn catalogue(packs: &SupplyPacks, contraband: bool, emagged: bool) -> CargoStatic {
    catalogue_with(packs, contraband, emagged, false)
}

/// [`catalogue`] of the express console (`express`: pod-only packs too).
pub fn catalogue_with(
    packs: &SupplyPacks,
    contraband: bool,
    emagged: bool,
    express: bool,
) -> CargoStatic {
    let icon = |path: &str| {
        packs
            .templates
            .get(path)
            .and_then(|t| t.appearance.as_ref())
            .map(|a| ProductIcon {
                icon_state: a.icon_state,
                color: a.color,
            })
    };
    let mut groups: Vec<SupplyGroup> = Vec::new();
    for pack in packs
        .packs
        .iter()
        .filter(|p| super::supply::orderable_with(p, contraband, emagged, express))
    {
        let record = SupplyRecord {
            id: pack.id.clone(),
            name: pack.name.clone(),
            desc: pack.desc.clone(),
            cost: pack.cost.round().max(0.0) as u32,
            access: !pack.access.is_empty(),
            goody: pack.has(ss13_mapload::cargo::order_flags::GOODY),
            contraband: pack.has(ss13_mapload::cargo::order_flags::CONTRABAND),
            icon: pack.contains.first().and_then(|(t, _)| icon(t)),
            contains: pack
                .contains
                .iter()
                .map(|(path, amount)| SupplyItem {
                    name: packs
                        .templates
                        .get(path)
                        .and_then(|t| t.proto.get::<DmName>())
                        .map_or_else(|| path.clone(), |n| n.name.clone()),
                    icon: icon(path),
                    amount: *amount,
                })
                .collect(),
        };
        match groups.iter_mut().find(|g| g.name == pack.group) {
            Some(group) => group.packs.push(record),
            None => groups.push(SupplyGroup {
                name: pack.group.clone(),
                packs: vec![record],
            }),
        }
    }
    groups.sort_by(|a, b| a.name.cmp(&b.name));
    for group in &mut groups {
        group.packs.sort_by(|a, b| a.name.cmp(&b.name));
    }
    CargoStatic {
        max_order: CARGO_MAX_ORDER,
        groups,
        displayed_currency_name: format!(" {MONEY_SYMBOL}"),
    }
}

fn setup_consoles(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let consoles: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, CARGO_CONSOLE) && !is_under(&o.path, EXPRESS_CONSOLE))
        .map(|(e, _)| e)
        .collect();
    let catalogue = Arc::new(
        world
            .get_resource::<SupplyPacks>()
            .map(|p| catalogue(p, false, false))
            .unwrap_or_default(),
    );
    let packs: usize = catalogue.groups.iter().map(|g| g.packs.len()).sum();
    info!(
        "{} supply consoles, {packs} packs in their catalogue",
        consoles.len()
    );
    for e in consoles {
        setup_console(world, e, Some(catalogue.clone()));
    }
}

/// A supply or request console's state (map load, and `crate::machine_frame` without a shared
/// `catalogue`).
pub(crate) fn setup_console(world: &mut World, e: Entity, shared: Option<Arc<CargoStatic>>) {
    let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
        return;
    };
    if is_under(&path, EXPRESS_CONSOLE) {
        super::express::setup(world, e);
        return;
    }
    if !is_under(&path, CARGO_CONSOLE) {
        return;
    }
    let request_only = is_under(&path, REQUEST_CONSOLE);
    let catalogue = shared.unwrap_or_else(|| {
        Arc::new(
            world
                .get_resource::<SupplyPacks>()
                .map(|p| catalogue(p, false, false))
                .unwrap_or_default(),
        )
    });
    world.entity_mut(e).insert(CargoConsole {
        request_only,
        can_send: !request_only,
        can_approve_requests: !request_only,
        contraband: false,
        emagged: false,
        self_paid: false,
        catalogue,
        next_announcement: 0.0,
        loaded_coupons: Vec::new(),
    });
}

/// tg `/obj/machinery/computer/cargo/emag_act` (from `crate::emag`): contraband and the
/// emag-only packs for good, the open windows get the new catalogue. True when it took.
// shortcut: the circuit board does not keep it (a rebuilt console is clean).
pub fn emag_act(world: &mut World, mob: Entity, card: Entity, console: Entity) -> bool {
    if world.get::<CargoConsole>(console).is_none_or(|c| c.emagged) {
        return false;
    }
    let catalogue = Arc::new(
        world
            .get_resource::<SupplyPacks>()
            .map(|p| catalogue(p, true, true))
            .unwrap_or_default(),
    );
    if let Some(mut state) = world.get_mut::<CargoConsole>(console) {
        state.emagged = true;
        state.contraband = true;
        state.catalogue = catalogue;
    }
    let w: &World = world;
    let name = |e: Entity| {
        w.get::<DmName>(e)
            .map(|n| plain(&n.name).to_owned())
            .unwrap_or_default()
    };
    let user = w
        .get::<Name>(mob)
        .map_or_else(|| name(mob), |n| n.as_str().to_owned());
    let (card_name, console_name) = (name(card), name(console));
    if let Some(&pos) = world.get::<TilePos>(console) {
        let chat = crate::pull::visible_message(
            world,
            pos,
            ss13_shared::chat::ChatChannel::Local,
            (
                ss13_shared::chat::Span::Warning,
                format!("{user} swipes {card_name} through {console_name}!"),
            ),
            &[],
        );
        for message in chat {
            world.write_message(message);
        }
    }
    if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(
            world,
            client,
            ss13_shared::chat::Span::Notice,
            format!(
                "You adjust {console_name}'s routing and receiver spectrum, unlocking special \
                 supplies and contraband."
            ),
        );
    }
    let mut windows = world.query::<&mut UiWindow>();
    for mut window in windows.iter_mut(world) {
        if window.interface == CargoUi::NAME && window.source == console {
            window.refresh_static();
        }
    }
    true
}

fn plain(name: &str) -> &str {
    name.trim_start_matches("\\improper ")
        .trim_start_matches("\\proper ")
}

/// tg `attack_hand` (empty hand) or a click with an ID card: opens the window.
fn open_console(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<&DmName, (With<CargoConsole>, Without<Unpowered>)>,
    inventories: Query<&Inventory>,
    cards: Query<(), With<IdCard>>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = consoles.get(click.target) else {
            continue;
        };
        let Ok(inventory) = inventories.get(click.mob) else {
            continue;
        };
        if inventory
            .active_item()
            .is_none_or(|item| cards.contains(item))
        {
            open.write(OpenUi::new::<CargoUi>(
                click.client,
                click.mob,
                click.target,
                plain(&name.name),
            ));
        }
    }
}

/// An unpowered console is update-only (`can_interact` fails).
fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<CargoConsole>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == CargoUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

type ConsoleParams = (
    Option<Res<'static, SupplyShuttle>>,
    Option<Res<'static, SupplyOrders>>,
    Option<Res<'static, SupplyPacks>>,
    Option<Res<'static, Economy>>,
    Res<'static, Time>,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CargoAct {
    Send,
    /// `add` by pack id, with the request console's reason and whether the requester's
    /// department pays.
    Add(String, Option<String>, bool),
    AddByName(String),
    Remove(String),
    Modify(String, i64),
    Clear,
    Approve(u32),
    Deny(u32),
    DenyAll,
    TogglePrivate,
}

/// tg `ui_data`.
pub fn console_data(
    console: &CargoConsole,
    shuttle: Option<&SupplyShuttle>,
    orders: Option<&SupplyOrders>,
    packs: Option<&SupplyPacks>,
    economy: Option<&Economy>,
    now: f64,
) -> CargoData {
    let mut data = CargoData {
        department: "Cargo".into(),
        location: shuttle.map_or_else(|| "Unknown".into(), |s| s.status_text(now)),
        points: economy
            .and_then(|e| e.department(ACCOUNT_CAR))
            .map_or(0, |a| a.balance),
        away: shuttle.is_some_and(SupplyShuttle::at_centcom),
        docked: shuttle.is_some_and(|s| s.mode == SupplyMode::Idle),
        self_paid: console.self_paid,
        can_send: console.can_send,
        can_approve_requests: console.can_approve_requests,
        requestonly: console.request_only,
        message: orders
            .map(|o| o.centcom_message.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| DEFAULT_MESSAGE.into()),
        ..default()
    };
    let (Some(orders), Some(packs)) = (orders, packs) else {
        return data;
    };
    for order in &orders.shopping_list {
        let pack = &packs.packs[order.pack];
        let cost = order.final_cost(packs).max(0) as u32;
        let private = u32::from(order.paying_account.is_some());
        match data.cart.iter_mut().find(|e| e.object == pack.name) {
            Some(entry) => {
                entry.amount += 1;
                entry.cost += cost;
                entry.paid += private;
            }
            None => data.cart.push(CartEntry {
                object: pack.name.clone(),
                cost,
                id: order.id,
                amount: 1,
                orderer: order.orderer.clone(),
                paid: private,
                can_be_cancelled: order.can_be_cancelled,
            }),
        }
    }
    data.requests = orders
        .request_list
        .iter()
        .map(|order| {
            let pack = &packs.packs[order.pack];
            RequestEntry {
                object: pack.name.clone(),
                cost: pack.cost.round().max(0.0) as u32,
                orderer: order.orderer.clone(),
                reason: order.reason.clone(),
                id: order.id,
                account: match &order.paying_department {
                    Some(department) => economy
                        .and_then(|e| e.department(department))
                        .map_or_else(|| department.clone(), |a| a.holder.clone()),
                    None => order
                        .paying_account
                        .and_then(|id| economy?.account(id))
                        .map_or_else(|| "Cargo Department".into(), |a| a.holder.clone()),
                },
            }
        })
        .collect();
    data
}

impl ServerUi for CargoUi {
    type Source = &'static CargoConsole;
    type UserParam = ConsoleParams;
    type Act = CargoAct;

    fn static_data(console: &CargoConsole) -> CargoStatic {
        (*console.catalogue).clone()
    }

    fn data(
        console: &CargoConsole,
        _: Entity,
        (shuttle, orders, packs, economy, time): &SystemParamItem<'_, '_, ConsoleParams>,
    ) -> CargoData {
        console_data(
            console,
            shuttle.as_deref(),
            orders.as_deref(),
            packs.as_deref(),
            economy.as_deref(),
            time.elapsed_secs_f64(),
        )
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<CargoAct, UiReject> {
        let text = |key: &str| {
            params
                .get(key)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams(format!("{key}: missing")))
        };
        let id = || {
            params
                .number(ID)
                .ok()
                .filter(|n| *n >= 0.0 && *n <= f64::from(u32::MAX))
                .map(|n| n as u32)
                .ok_or_else(|| UiReject::BadParams("id: expected an order number".into()))
        };
        match action {
            SEND => Ok(CargoAct::Send),
            ADD => Ok(CargoAct::Add(
                text(ID)?,
                params.get(REASON).map(str::to_owned),
                params
                    .get(DEPARTMENT)
                    .is_some_and(|v| !matches!(v, "" | "0" | "false")),
            )),
            ADD_BY_NAME => Ok(CargoAct::AddByName(text(ORDER_NAME)?)),
            REMOVE => Ok(CargoAct::Remove(text(ORDER_NAME)?)),
            MODIFY => Ok(CargoAct::Modify(
                text(ORDER_NAME)?,
                params.number(AMOUNT)?.clamp(-1.0, 1000.0) as i64,
            )),
            CLEAR => Ok(CargoAct::Clear),
            APPROVE => Ok(CargoAct::Approve(id()?)),
            DENY => Ok(CargoAct::Deny(id()?)),
            DENY_ALL => Ok(CargoAct::DenyAll),
            TOGGLE_PRIVATE => Ok(CargoAct::TogglePrivate),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `say()` of a machine: the console's words to the players within view range.
pub(crate) fn machine_say(world: &mut World, source: Entity, text: &str) {
    let Some(&pos) = world.get::<TilePos>(source) else {
        return;
    };
    let name = world
        .get::<DmName>(source)
        .map(|n| plain(&n.name).to_owned())
        .unwrap_or_default();
    let mut query = world.query_filtered::<Entity, With<ClientToken>>();
    let clients: Vec<Entity> = query
        .iter(world)
        .filter(|&c| {
            let mob = world.resource::<crate::server::Players>().mob(c);
            mob.and_then(|m| world.get::<TilePos>(m))
                .and_then(|p| p.chebyshev(pos))
                .is_some_and(|d| d <= SAY_RANGE)
        })
        .collect();
    world.write_message(SendChat {
        clients,
        message: ss13_chat::server::say_message(&name, text),
    });
}

/// The actor's ID card as tg `get_idcard(hand_first = TRUE)` finds it.
struct ActorId {
    access: Vec<String>,
    name: String,
    rank: String,
    account: Option<u32>,
    card_name: String,
}

fn actor_id(world: &World, mob: Entity) -> Option<ActorId> {
    let inventory = world.get::<Inventory>(mob)?;
    [
        inventory.hands[inventory.active],
        inventory.hands[1 - inventory.active],
        inventory.id,
    ]
    .into_iter()
    .flatten()
    .find_map(|item| {
        let item = crate::inventory::get_id(world, item)?;
        let card = world.get::<IdCard>(item)?;
        let registration = world.get::<IdRegistration>(item);
        Some(ActorId {
            access: card.access.clone(),
            name: registration.map_or_else(|| "Unknown".into(), |r| r.registered_name.clone()),
            rank: registration.map_or_else(|| "Unknown".into(), |r| r.assignment.clone()),
            account: super::economy::account_of(world, item),
            card_name: world
                .get::<DmName>(item)
                .map_or_else(|| "ID card".into(), |n| n.name.clone()),
        })
    })
}

/// tg `add_item`: `amount` orders of `pack` into the shopping list, or the request list from a
/// request console (for the requester's department budget with `department`). `false` when
/// nothing was added.
fn add_item(
    world: &mut World,
    console: Entity,
    actor: Entity,
    pack: usize,
    amount: i64,
    reason: Option<String>,
    department: bool,
) -> bool {
    let Some(state) = world.get::<CargoConsole>(console).cloned() else {
        return false;
    };
    let Some(def) = world
        .get_resource::<SupplyPacks>()
        .and_then(|p| p.packs.get(pack).cloned())
    else {
        return false;
    };
    if !(1..=i64::from(CARGO_MAX_ORDER)).contains(&amount)
        || !orderable(&def, state.contraband, state.emagged)
    {
        return false;
    }
    let id = actor_id(world, actor);
    let (mut name, rank) = id.as_ref().map_or_else(
        || ("*None Provided*".to_owned(), "*None Provided*".to_owned()),
        |i| (i.name.clone(), i.rank.clone()),
    );
    let account = id.as_ref().and_then(|i| i.account);
    if state.self_paid {
        let Some(card) = &id else {
            machine_say(world, console, "No ID card detected.");
            return false;
        };
        let valid = account.is_some_and(|a| world.resource::<Economy>().account(a).is_some());
        if !valid {
            machine_say(world, console, "Invalid bank account.");
            return false;
        }
        if !def.access_view.is_empty() && !card.access.contains(&def.access_view) {
            let text = format!(
                "{} lacks the requisite access for this purchase.",
                card.card_name
            );
            machine_say(world, console, &text);
            return false;
        }
    }
    if def.has(ss13_mapload::cargo::order_flags::GOODY) && !state.self_paid {
        machine_say(
            world,
            console,
            "ERROR: Small crates may only be purchased by private accounts.",
        );
        return false;
    }
    let to_requests = state.request_only && !state.self_paid;
    let mut reason_text = String::new();
    let mut personal_department = None;
    if to_requests {
        // tg asks with `tgui_input_text`; no answer (cancel) adds nothing.
        let Some(reason) = reason else {
            return false;
        };
        reason_text = reason
            .trim()
            .chars()
            .take(ss13_shared::chat::MAX_MESSAGE_LEN)
            .collect();
        let economy = world.resource::<Economy>();
        let requester = account.and_then(|a| economy.account(a));
        name = requester.map(|a| a.holder.clone()).unwrap_or_default();
        // tg offers the job's department unless it is cargo's own budget.
        if department {
            personal_department = requester
                .map(|a| a.paycheck_department.clone())
                .filter(|d| d != ACCOUNT_CAR && economy.department(d).is_some());
        }
        let has_view = def.access_view.is_empty()
            || id
                .as_ref()
                .is_some_and(|i| i.access.contains(&def.access_view));
        if personal_department.is_some() && !has_view {
            machine_say(
                world,
                console,
                "ERROR: User lacks the requisite access for this purchase request.",
            );
            return false;
        }
    }
    let Some(similar) = world.resource::<SupplyOrders>().order_count(pack) else {
        let text =
            format!("ERROR: No more then {CARGO_MAX_ORDER} of any pack may be ordered at once");
        machine_say(world, console, &text);
        return false;
    };
    let paying_account = if state.self_paid { account } else { None };
    let paying_department = if state.self_paid {
        None
    } else {
        personal_department
    };
    let amount = amount.clamp(1, i64::from(CARGO_MAX_ORDER - similar)) as u32;
    for _ in 0..amount {
        let coupon = super::coupon::take_for(world, console, pack);
        let mut orders = world.resource_mut::<SupplyOrders>();
        let order = SupplyOrder {
            id: orders.next_id(),
            pack,
            orderer: name.clone(),
            orderer_rank: rank.clone(),
            reason: reason_text.clone(),
            paying_account,
            paying_department: paying_department.clone(),
            can_be_cancelled: true,
            coupon,
        };
        if to_requests {
            orders.request_list.push(order);
        } else {
            orders.shopping_list.push(order);
        }
    }
    if let Some(account) = paying_account {
        let holder = world
            .resource::<Economy>()
            .account(account)
            .map(|a| a.holder.clone())
            .unwrap_or_default();
        let text = format!(
            "Order processed. The price will be charged to {holder}'s bank account on delivery."
        );
        machine_say(world, console, &text);
    }
    if state.request_only {
        announce_requests(world, console, amount);
    }
    true
}

/// tg `aas_config_announce(/datum/aas_config_entry/cargo_orders_announcement)` on the supply
/// channel, once per `message_cooldown`.
fn announce_requests(world: &mut World, console: Entity, amount: u32) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut state) = world.get_mut::<CargoConsole>(console) else {
        return;
    };
    if now < state.next_announcement {
        return;
    }
    state.next_announcement = now + ANNOUNCE_COOLDOWN_SECS;
    let Some(&pos) = world.get::<TilePos>(console) else {
        return;
    };
    let name = world
        .get::<DmName>(console)
        .map(|n| plain(&n.name).to_owned())
        .unwrap_or_default();
    let text = if amount == 1 {
        "A new order has been requested.".to_owned()
    } else {
        format!("{amount} orders have been requested.")
    };
    world.write_message(MachineRadio {
        freq: ss13_chat::radio::FREQ_SUPPLY,
        z: pos.z,
        name,
        text,
    });
}

/// tg `remove_item` of the first cart order of the pack named `name`: its coupon comes back
/// onto the console's tile.
fn remove_by_name(world: &mut World, console: Entity, name: &str) -> bool {
    let Some(pack) = world
        .get_resource::<SupplyPacks>()
        .and_then(|p| p.by_name(name))
    else {
        return false;
    };
    let mut orders = world.resource_mut::<SupplyOrders>();
    let Some(i) = orders
        .shopping_list
        .iter()
        .position(|o| o.pack == pack && o.can_be_cancelled)
    else {
        return false;
    };
    let order = orders.shopping_list.remove(i);
    refund_coupon(world, console, &order);
    true
}

/// A cancelled order's coupon onto the console's tile.
fn refund_coupon(world: &mut World, console: Entity, order: &SupplyOrder) {
    let Some((coupon, _)) = order.coupon else {
        return;
    };
    if world.get_entity(coupon).is_err() {
        return;
    }
    if let Some(&pos) = world.get::<TilePos>(console) {
        machine_say(world, console, "Coupon refunded.");
        crate::storage::take_out(world, coupon, pos);
    }
}

/// tg `ui_act` "send".
fn send(world: &mut World, console: Entity) {
    let Some(shuttle) = world.get_resource::<SupplyShuttle>() else {
        return;
    };
    if !can_move(world, shuttle) {
        machine_say(world, console, SAFETY_WARNING);
        return;
    }
    let at_station = shuttle.at_station();
    let call = world.resource::<super::supply::SupplyConfig>().call_secs;
    if !request(world, !at_station) {
        return;
    }
    if at_station {
        info!("{console}: the supply shuttle was sent away");
        machine_say(world, console, "The supply shuttle is departing.");
    } else {
        let minutes = (call / 60.0).round() as u64;
        let text = format!(
            "The supply shuttle has been called and will arrive in {minutes} minute{}.",
            if minutes == 1 { "" } else { "s" }
        );
        info!("{console}: the supply shuttle was called");
        machine_say(world, console, &text);
    }
}

/// tg `ui_act`, one exclusive pass; every check runs here.
fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<CargoUi>>>) {
    let pending: Vec<(Entity, Entity, CargoAct)> = acts
        .read(world.resource::<Messages<UiActed<CargoUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, console, act) in pending {
        if world.get::<Unpowered>(console).is_some() || !world.contains_resource::<SupplyOrders>() {
            continue;
        }
        let Some(state) = world.get::<CargoConsole>(console).cloned() else {
            continue;
        };
        let manage = state.can_send && state.can_approve_requests && !state.request_only;
        match act {
            CargoAct::Send if state.can_send => send(world, console),
            CargoAct::Add(id, reason, department) => {
                if let Some(pack) = world
                    .get_resource::<SupplyPacks>()
                    .and_then(|p| p.by_id(&id))
                {
                    add_item(world, console, actor, pack, 1, reason, department);
                }
            }
            CargoAct::AddByName(name) if !state.request_only => {
                if let Some(pack) = world
                    .get_resource::<SupplyPacks>()
                    .and_then(|p| p.by_name(&name))
                {
                    add_item(world, console, actor, pack, 1, None, false);
                }
            }
            CargoAct::Remove(name) if manage => {
                remove_by_name(world, console, &name);
            }
            CargoAct::Modify(name, amount) if manage => {
                while remove_by_name(world, console, &name) {}
                if amount > 0 && amount <= i64::from(CARGO_MAX_ORDER) {
                    let pack = world
                        .get_resource::<SupplyPacks>()
                        .and_then(|p| p.by_name(&name));
                    if let Some(pack) = pack {
                        add_item(world, console, actor, pack, amount, None, false);
                    }
                }
            }
            CargoAct::Clear if manage => {
                let (kept, cleared): (Vec<SupplyOrder>, Vec<SupplyOrder>) =
                    std::mem::take(&mut world.resource_mut::<SupplyOrders>().shopping_list)
                        .into_iter()
                        .partition(|o| !o.can_be_cancelled);
                world.resource_mut::<SupplyOrders>().shopping_list = kept;
                for order in &cleared {
                    refund_coupon(world, console, order);
                }
            }
            CargoAct::Approve(id) if state.can_approve_requests => {
                let mut orders = world.resource_mut::<SupplyOrders>();
                if let Some(i) = orders.request_list.iter().position(|o| o.id == id) {
                    let order = orders.request_list.remove(i);
                    orders.shopping_list.push(order);
                }
            }
            CargoAct::Deny(id) if state.can_approve_requests => {
                world
                    .resource_mut::<SupplyOrders>()
                    .request_list
                    .retain(|o| o.id != id);
            }
            CargoAct::DenyAll if state.can_approve_requests => {
                world.resource_mut::<SupplyOrders>().request_list.clear();
            }
            CargoAct::TogglePrivate => {
                if let Some(mut console) = world.get_mut::<CargoConsole>(console) {
                    console.self_paid = !console.self_paid;
                }
            }
            act => debug!("{console}: refused {act:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use ss13_mapload::cargo::SupplyPackDef;

    use super::*;

    fn pack(id: &str, group: &str, name: &str, cost: f64) -> SupplyPackDef {
        SupplyPackDef {
            id: id.into(),
            name: name.into(),
            group: group.into(),
            cost,
            crate_type: Some("/obj/structure/closet/crate".into()),
            contains: vec![("/obj/item/wrench".into(), 2)],
            ..default()
        }
    }

    #[test]
    fn catalogue_groups_sorted_and_filtered() {
        let mut hidden = pack("/p/c", "Armory", "Guns", 5000.0);
        hidden.order_flags = ss13_mapload::cargo::order_flags::CONTRABAND;
        let packs = SupplyPacks::new(vec![
            pack("/p/b", "Service", "Mop", 280.0),
            pack("/p/a", "Engineering", "Tools", 1000.4),
            pack("/p/d", "Service", "Brooms", 300.0),
            hidden,
        ]);
        let st = catalogue(&packs, false, false);
        let names: Vec<&str> = st.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Engineering", "Service"]);
        let service: Vec<&str> = st.groups[1].packs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(service, ["Brooms", "Mop"]);
        assert_eq!(st.groups[0].packs[0].cost, 1000);
        assert_eq!(st.groups[0].packs[0].contains[0].amount, 2);
        assert_eq!(st.groups[0].packs[0].contains[0].name, "/obj/item/wrench");
        assert_eq!(st.displayed_currency_name, " ¢");
        let with_contraband = catalogue(&packs, true, false);
        assert_eq!(with_contraband.groups.len(), 3);
        assert!(with_contraband.groups[0].packs[0].contraband);
        let mut emag = pack("/p/e", "Security", "Emag Stuff", 100.0);
        emag.order_flags = ss13_mapload::cargo::order_flags::EMAG_ONLY;
        let packs = SupplyPacks::new(vec![emag]);
        assert!(catalogue(&packs, true, false).groups.is_empty());
        let emagged = catalogue(&packs, true, true);
        assert_eq!(emagged.groups[0].packs[0].name, "Emag Stuff");
        assert!(
            !emagged.groups[0].packs[0].contraband,
            "only ORDER_CONTRABAND is marked"
        );
    }

    #[test]
    fn data_groups_the_cart_by_pack() {
        let packs = SupplyPacks::new(vec![pack("/p/a", "Engineering", "Tools", 1000.0)]);
        let mut orders = SupplyOrders::new(1);
        for paying_account in [None, Some(5)] {
            let id = orders.next_id();
            orders.shopping_list.push(SupplyOrder {
                id,
                pack: 0,
                orderer: "QM".into(),
                orderer_rank: "Quartermaster".into(),
                reason: String::new(),
                paying_account,
                paying_department: None,
                can_be_cancelled: true,
                coupon: None,
            });
        }
        let id = orders.next_id();
        orders.request_list.push(SupplyOrder {
            id,
            pack: 0,
            orderer: "Bob".into(),
            orderer_rank: String::new(),
            reason: "need tools".into(),
            paying_account: None,
            paying_department: Some(crate::cargo::economy::ACCOUNT_SCI.into()),
            can_be_cancelled: true,
            coupon: None,
        });
        let console = CargoConsole {
            request_only: false,
            can_send: true,
            can_approve_requests: true,
            contraband: false,
            emagged: false,
            self_paid: false,
            catalogue: Arc::default(),
            next_announcement: 0.0,
            loaded_coupons: Vec::new(),
        };
        let economy = Economy::new(1);
        let data = console_data(
            &console,
            None,
            Some(&orders),
            Some(&packs),
            Some(&economy),
            0.0,
        );
        assert_eq!(data.points, 0, "cargo starts broke");
        assert_eq!(data.location, "Unknown");
        assert_eq!(data.message, DEFAULT_MESSAGE);
        assert_eq!(data.cart.len(), 1);
        assert_eq!(data.cart[0].amount, 2);
        assert_eq!(data.cart[0].cost, 2100, "1000 + 1.1 x 1000");
        assert_eq!(data.cart[0].paid, 1);
        assert_eq!(data.cart_totals(), (2, 2100));
        assert_eq!(data.requests.len(), 1);
        assert_eq!(data.requests[0].account, "Научный бюджет");
        assert_eq!(data.requests[0].reason, "need tools");
    }
}
