//! The syndicate uplink (server) after tg `/datum/component/uplink`, `/datum/uplink_handler` and
//! `/datum/uplink_item`.
//!
//! [`give_uplink`] (tg `/datum/mind/give_uplink`) hides an [`Uplink`] in the traitor's device:
//! tg's `get_uplink_location(UPLINK_PDA)` (the default `uplink_loc` preference) takes the PDA,
//! else a radio, else a pen. In a PDA, setting the messenger ringtone to the code ("123 Альфа")
//! unlocks it and opens the `Uplink` window (`new_ringtone`, [`unlock_by_ringtone`]); using the
//! PDA in hand then opens the uplink until it is locked again. In a headset, saying the
//! three-word code on the uplink channel (`:z`) does (`new_message`); the code never goes out on
//! the radio. Buying is
//! checked at the moment of effect (`can_purchase_item`): the uplink is active and in the
//! buyer's hands or slots (`inventory_state`), the item is sold to traitors, the shop is not
//! locked, the population is high enough, there are telecrystals and stock. The item appears in
//! the buyer's hands, else at their feet (a mech always lands there). The window does not list
//! items restricted to other roles (tg `Uplink`'s `restricted_roles` filter).
//!
//! A traitor's uplink also gets 4-6 sales (`create_uplink_sales`) in the "Discounted Gear"
//! category: copies of catalog items at 25/50/75% off, bought by their own id (`<path>?sale`)
//! with their own stock. The bought failsafe code (`/datum/uplink_item/device_tools/failsafe`)
//! is a second code that blows the device up when typed as the ringtone or said on `:z`
//! ([`failsafe`]). Clicking the telecrystal count withdraws raw telecrystals (`buy_raw_tc`, an
//! amount asked with `tgui_input_number`); a telecrystal stack used on an active uplink loads
//! them back ([`item_interaction`], `load_tc`).
// shortcut: the uplink location preference is always tg's default (PDA); pen twisting and the
// implant fallback are not modelled (no PDA and no radio: no uplink). No refunds
// (`COMSIG_ITEM_ATTEMPT_TC_REIMBURSE`) and no nuclear team discounts.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_chat::radio::RADIO_CHANNEL_UPLINK;
use ss13_chat::server::{SaySystems, Spoke};
use ss13_chat::speech::RadioTarget;
use ss13_mapload::antag::{
    ANTAG_FILE, AntagData, UPLINK_CLOWN_OPS, UPLINK_LONE_OP, UPLINK_NUKE_OPS, UPLINK_TRAITORS,
    UplinkItemDef,
};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmItem, DmName, MapLoadPlugin, Template, read_manifest};
use ss13_net::chat::server::SendChat;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_tgui::UiStatus;
use ss13_tgui::interfaces::vending::ProductIcon;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::{
    BUY, BUY_RAW_TC, LOCK, RENEGOTIATE_OBJECTIVES, UplinkData, UplinkItemRecord, UplinkStatic,
    UplinkUi,
};
use crate::health::Stat;
use crate::inventory::{CarriedBy, Inventory};
use crate::jobs::PlayerJob;
use crate::modpc::pda::ModularComputer;
use crate::radio::Radio;

/// tg `TELECRYSTALS_DEFAULT`.
pub const TELECRYSTALS_DEFAULT: u32 = 20;
/// tg `/datum/component/uplink` `name`: the window title.
const UPLINK_NAME: &str = "syndicate uplink";
/// tg `RADIO_TOKEN_UPLINK`.
pub const RADIO_TOKEN_UPLINK: &str = ":z";
/// The failsafe code's uplink item (`item = ABSTRACT_UPLINK_ITEM`: nothing spawns).
pub const FAILSAFE_ITEM: &str = "/datum/uplink_item/device_tools/failsafe";
/// `/obj/item/stack/telecrystal`.
pub const TELECRYSTAL: &str = "/obj/item/stack/telecrystal";
/// The raw telecrystal's uplink item: not logged (`log_purchase` returns).
const TELECRYSTAL_ITEM: &str = "/datum/uplink_item/bundles_tc/telecrystal";
/// The id of a sale copy of an item: its path and this (tg `REF(item)` of the copy).
pub const SALE_SUFFIX: &str = "?sale";
/// tg `TRAITOR_DISCOUNT_MIN_PRICE`.
const DISCOUNT_MIN_PRICE: u32 = 4;
/// `/datum/antagonist/traitor` `uplink_sales_min`, `uplink_sales_max`.
const SALES_MIN: i64 = 4;
const SALES_MAX: i64 = 6;
/// `/datum/uplink_category/discounts` `name`, when the catalog has none.
const DISCOUNT_CATEGORY_NAME: &str = "Discounted Gear";
/// `create_uplink_sales`' `disclaimer`.
const DISCLAIMERS: [&str; 12] = [
    "Void where prohibited.",
    "Not recommended for children.",
    "Contains small parts.",
    "Check local laws for legality in region.",
    "Do not taunt.",
    "Not responsible for direct, indirect, incidental or consequential damages resulting from \
     any defect, error or failure to perform.",
    "Keep away from fire or flames.",
    "Product is provided \"as is\" without any implied or expressed warranties.",
    "As seen on TV.",
    "For recreational use only.",
    "Use only as directed.",
    "16% sales tax will be charged for orders originating within Space Nebraska.",
];

/// The catalog and the item templates, loaded at startup from `<assets>/antag.ron`.
#[derive(Resource, Default)]
pub struct UplinkCatalog {
    pub data: AntagData,
    templates: HashMap<String, Template>,
    board: Arc<UplinkStatic>,
    /// The catalogs of the operatives' uplinks by flag (`UPLINK_NUKE_OPS`, `UPLINK_LONE_OP`,
    /// `UPLINK_CLOWN_OPS`).
    op_boards: Vec<(u32, Arc<UplinkStatic>)>,
}

impl UplinkCatalog {
    pub fn item(&self, path: &str) -> Option<&UplinkItemDef> {
        self.data.uplink_items.iter().find(|i| i.path == path)
    }
}

/// tg `/datum/component/uplink` with its `/datum/uplink_handler`, on the device it hides in.
#[derive(Component, Debug, Clone)]
pub struct Uplink {
    /// The traitor's body (tg `owner` / `uplink_handler.owner`).
    pub owner: Entity,
    pub unlock_code: String,
    pub locked: bool,
    pub active: bool,
    pub lockable: bool,
    pub telecrystals: u32,
    pub uplink_flag: u32,
    /// `item_stock`: limited items' stock left, by item path.
    pub item_stock: HashMap<String, i32>,
    /// Distinct purchases (`length(purchase_log.purchase_log)`).
    pub purchases: Vec<String>,
    pub shop_locked: bool,
    pub assigned_role: String,
    /// `primary_objectives` as the window lists them: (`task_name`, `task_text`).
    pub primary_objectives: Vec<(String, String)>,
    /// `failsafe_code`: typed like the unlock code, it blows the device up.
    pub failsafe_code: Option<String>,
    /// `extra_purchasable`: the sale copies, by their `<path>?sale` id.
    pub sales: Vec<UplinkItemDef>,
    /// `can_replace_objectives`: the owner may still pick a custom objective.
    pub can_renegotiate: bool,
    board: Arc<UplinkStatic>,
}

impl Uplink {
    /// tg `new_message`: the code anywhere in the message, case-insensitive.
    pub fn heard_code(&self, message: &str) -> bool {
        !self.unlock_code.is_empty()
            && message
                .to_lowercase()
                .contains(&self.unlock_code.to_lowercase())
    }

    /// tg `new_message`'s failsafe branch: the failsafe code anywhere in the message.
    pub fn heard_failsafe(&self, message: &str) -> bool {
        self.failsafe_code
            .as_ref()
            .is_some_and(|c| !c.is_empty() && message.to_lowercase().contains(&c.to_lowercase()))
    }

    /// A sale copy by its id, else the catalog's item.
    fn item_def(&self, catalog: &UplinkCatalog, id: &str) -> Option<UplinkItemDef> {
        self.sales
            .iter()
            .find(|i| i.path == id)
            .or_else(|| catalog.item(id))
            .cloned()
    }
}

/// A purchase, for the round-end report (`/datum/uplink_purchase_entry`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Purchase {
    pub name: String,
    pub spent: u32,
    pub amount: u32,
}

/// The purchase log of a traitor body (tg `GLOB.uplink_purchase_logs_by_key[key]`).
#[derive(Component, Clone, Debug, Default)]
pub struct PurchaseLog {
    pub entries: Vec<Purchase>,
    pub total_spent: u32,
}

impl PurchaseLog {
    /// `log_purchase`: one entry per item and price.
    pub fn log(&mut self, name: &str, spent: u32) {
        match self
            .entries
            .iter_mut()
            .find(|e| e.name == name && e.spent == spent)
        {
            Some(entry) => entry.amount += 1,
            None => self.entries.push(Purchase {
                name: name.to_owned(),
                spent,
                amount: 1,
            }),
        }
        self.total_spent += spent;
    }
}

pub struct UplinkPlugin;

impl Plugin for UplinkPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Telecrystals into the uplink in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 1000, |w, c| {
            c.held.is_some_and(|h| {
                crate::antag::uplink::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // Telecrystals into an active uplink (`load_tc`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 100, |w, c| {
            c.held.is_some_and(|h| {
                crate::antag::uplink::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.register_ui::<UplinkUi>()
            .init_resource::<UplinkCatalog>()
            .add_systems(Startup, load_catalog)
            .add_systems(
                Update,
                (
                    unlock_by_radio.after(SaySystems).before(UiSystems::Open),
                    uplink_status.in_set(UiSystems::StatusHook),
                    uplink_acts.in_set(UiSystems::Handle),
                    raw_tc_answers
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                ),
            );
    }
}

fn load_catalog(world: &mut World) {
    let Some(assets) = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.clone())
    else {
        return;
    };
    let data = match AntagData::read(&assets.join(ANTAG_FILE)) {
        Ok(data) => data,
        Err(e) => {
            warn!("no uplink catalog or steal targets (run `ss13-mapload convert`): {e:#}");
            return;
        }
    };
    let manifest = read_manifest(&assets).unwrap_or_default();
    let index = manifest.index();
    let mut templates = HashMap::new();
    for item in &data.uplink_items {
        let proto = world
            .get_resource::<ss13_proto::Prototypes>()
            .and_then(|p| p.get(&item.item).cloned());
        match proto {
            Some(proto) => {
                templates.insert(item.item.clone(), Template::new(proto, &index));
            }
            None => warn!(
                "uplink item {} has no prototype for {}",
                item.path, item.item
            ),
        }
    }
    let board_of = |flag: u32| {
        let items: Vec<UplinkItemRecord> = data
            .uplink_items
            .iter()
            .filter(|i| i.purchasable_from & flag != 0 && templates.contains_key(&i.item))
            .map(|i| record(i, &templates))
            .collect();
        UplinkStatic {
            categories: categories_of(&data, &items),
            items,
            // `/obj/item/uplink` is not lockable; a traitor's hidden uplink is.
            lockable: flag == UPLINK_TRAITORS,
        }
    };
    let board = Arc::new(board_of(UPLINK_TRAITORS));
    let op_boards: Vec<(u32, Arc<UplinkStatic>)> =
        [UPLINK_NUKE_OPS, UPLINK_LONE_OP, UPLINK_CLOWN_OPS]
            .into_iter()
            .map(|flag| (flag, Arc::new(board_of(flag))))
            .collect();
    info!(
        "uplink: {} items for traitors, {} for nuclear operatives, {} steal targets",
        board.items.len(),
        op_boards[0].1.items.len(),
        data.steal_items.len()
    );
    world.insert_resource(UplinkCatalog {
        data,
        templates,
        board,
        op_boards,
    });
}

/// The window's entry for `item` (`/datum/asset/json/uplink`, `extra_purchasable`).
fn record(item: &UplinkItemDef, templates: &HashMap<String, Template>) -> UplinkItemRecord {
    UplinkItemRecord {
        id: item.path.clone(),
        name: item.name.clone(),
        desc: item.desc.clone(),
        cost: item.cost,
        category: item.category.clone(),
        icon: templates
            .get(&item.item)
            .and_then(|t| t.appearance.as_ref())
            .map(|a| ProductIcon {
                icon_state: a.icon_state,
                color: a.color,
            }),
        limited_stock: item.limited_stock,
        lock_other_purchases: item.lock_other_purchases,
        population_minimum: item.population_minimum,
    }
}

/// The window's catalog for `role`: tg's `Uplink` hides the items restricted to other roles.
fn board_for_role(
    catalog: &UplinkCatalog,
    board: &Arc<UplinkStatic>,
    role: &str,
) -> Arc<UplinkStatic> {
    let hidden = |r: &UplinkItemRecord| {
        catalog.item(&r.id).is_some_and(|i| {
            !i.restricted_roles.is_empty() && !i.restricted_roles.iter().any(|x| x == role)
        })
    };
    if !board.items.iter().any(&hidden) {
        return board.clone();
    }
    let mut out = (**board).clone();
    out.items.retain(|r| !hidden(r));
    out.categories = categories_of(&catalog.data, &out.items);
    Arc::new(out)
}

/// Categories that have `items`, by weight.
fn categories_of(data: &AntagData, items: &[UplinkItemRecord]) -> Vec<String> {
    data.uplink_categories
        .iter()
        .filter(|c| items.iter().any(|i| &i.category == *c))
        .cloned()
        .collect()
}

/// `get_discount`: `pick_weight(SMALL = 4, AVERAGE = 2, BIG = 1)` as 25%, 50%, 75% off.
fn roll_discount(rng: &mut Rng) -> f32 {
    match rng.range(1, 7) {
        1..=4 => 0.25,
        5..=6 => 0.5,
        _ => 0.75,
    }
}

/// `create_uplink_sales(num, discounts, -1, eligible)`: one item of each of up to `num` random
/// categories, as a discounted copy with the `?sale` id.
pub fn create_sales(
    num: usize,
    category: &str,
    eligible: &[UplinkItemDef],
    rng: &mut Rng,
) -> Vec<UplinkItemDef> {
    let mut per_category: Vec<(String, Vec<&UplinkItemDef>)> = Vec::new();
    for item in eligible {
        match per_category.iter_mut().find(|(c, _)| *c == item.category) {
            Some((_, items)) => items.push(item),
            None => per_category.push((item.category.clone(), vec![item])),
        }
    }
    let mut sales = Vec::new();
    for _ in 0..num.min(per_category.len()) {
        let (_, items) = per_category.remove(rng.range(0, per_category.len() as i64 - 1) as usize);
        let taken = items[rng.range(0, items.len() as i64 - 1) as usize];
        let mut sale = taken.clone();
        let mut discount = roll_discount(rng);
        if sale.limited_discount_stock > 0 {
            sale.limited_stock = sale.limited_discount_stock;
        }
        // "Tough love for nuke ops."
        if sale.cost >= 20 {
            discount *= 0.5;
        }
        let initial = taken.cost;
        // DM `round(x)` with one argument floors.
        sale.cost = ((initial as f32 * (1.0 - discount)).floor() as u32).max(1);
        let off = ((initial - sale.cost.min(initial)) as f32 / initial.max(1) as f32 * 100.0)
            .floor() as u32;
        sale.path = format!("{}{SALE_SUFFIX}", taken.path);
        sale.category = category.to_owned();
        sale.name = format!("{} ({off}% off!)", taken.name);
        let disclaimer = DISCLAIMERS[rng.range(0, DISCLAIMERS.len() as i64 - 1) as usize];
        sale.desc = format!(
            "{} Normally costs {initial} TC. All sales final. {disclaimer}",
            taken.desc
        );
        sales.push(sale);
    }
    sales
}

/// tg `/datum/antagonist/traitor/on_gain`'s sales: items of the uplink's flag that can be
/// discounted, cost at least 4 TC and are not barred to the uplink's role; `rand(4, 6)` of
/// them go on sale. The window's catalog gets them in the discount category.
pub fn add_sales(world: &mut World, device: Entity, rng: &mut Rng) {
    let Some(uplink) = world.get::<Uplink>(device) else {
        return;
    };
    let (flag, role) = (uplink.uplink_flag, uplink.assigned_role.clone());
    let catalog = world.resource::<UplinkCatalog>();
    let eligible: Vec<UplinkItemDef> = catalog
        .data
        .uplink_items
        .iter()
        .filter(|i| {
            catalog.templates.contains_key(&i.item)
                && i.path != FAILSAFE_ITEM
                && !i.cant_discount
                && i.purchasable_from & flag != 0
                && i.cost >= DISCOUNT_MIN_PRICE
                && (i.restricted_roles.is_empty() || i.restricted_roles.contains(&role))
        })
        .cloned()
        .collect();
    let category = match catalog.data.discount_category.as_str() {
        "" => DISCOUNT_CATEGORY_NAME.to_owned(),
        name => name.to_owned(),
    };
    let num = rng.range(SALES_MIN, SALES_MAX) as usize;
    let sales = create_sales(num, &category, &eligible, rng);
    let mut board = (*uplink.board).clone();
    board
        .items
        .extend(sales.iter().map(|s| record(s, &catalog.templates)));
    let mut data = catalog.data.clone();
    if !data.uplink_categories.contains(&category) {
        data.uplink_categories.push(category);
    }
    board.categories = categories_of(&data, &board.items);
    let mut uplink = world.get_mut::<Uplink>(device).expect("checked above");
    uplink.sales = sales;
    uplink.board = Arc::new(board);
}

/// tg `generate_code` for `device`, never the unlock code: a number and a word for a PDA,
/// three words for a radio.
fn generate_code(world: &World, device: Entity, phonetic: &[String], rng: &mut Rng) -> String {
    let unlock = world
        .get::<Uplink>(device)
        .map(|u| u.unlock_code.clone())
        .unwrap_or_default();
    let word = |rng: &mut Rng| {
        phonetic
            .get(rng.range(0, phonetic.len() as i64 - 1) as usize)
            .cloned()
            .unwrap_or_default()
    };
    for _ in 0..100 {
        let code = if world.get::<ModularComputer>(device).is_some() {
            let number = rng.range(100, 999);
            format!("{number} {}", word(rng))
        } else {
            format!("{} {} {}", word(rng), word(rng), word(rng))
        };
        if code != unlock {
            return code;
        }
    }
    String::new()
}

/// Where `item` is: its tile, or its carrier's or container's.
fn turf_of(world: &World, item: Entity) -> Option<TilePos> {
    let mut e = item;
    for _ in 0..8 {
        if let Some(&pos) = world.get::<TilePos>(e) {
            return Some(pos);
        }
        e = world
            .get::<CarriedBy>(e)
            .map(|c| c.0)
            .or_else(|| world.get::<crate::storage::StoredIn>(e).map(|s| s.0))?;
    }
    None
}

/// tg `failsafe`: `explosion(parent, 1, 2, 3)` where the device is, and the device is gone.
pub fn failsafe(world: &mut World, device: Entity, user: Option<Entity>) {
    let Some(pos) = turf_of(world, device) else {
        return;
    };
    let owner = world.get::<Uplink>(device).map(|u| u.owner);
    warn!(
        "uplink failsafe explosion at {pos:?} triggered by {user:?}; the uplink's owner was \
         {owner:?}"
    );
    crate::explosions::explosion(
        world,
        pos,
        crate::explosions::Explosion::new(1, 2, 3).cause("uplink failsafe"),
    );
    if world.get_entity(device).is_ok() {
        world.despawn(device);
    }
}

/// What [`give_uplink`] hid and where.
#[derive(Clone, Debug)]
pub struct UplinkGiven {
    pub device: Entity,
    pub code: String,
    /// `unlock_text`, for the info panel.
    pub unlock_text: String,
    /// `unlock_note`, for the notes (`antag_memory`).
    pub unlock_note: String,
}

/// The device's name as tg prints it (`uplink_loc.name`).
fn item_name(world: &World, item: Entity) -> String {
    world
        .get::<DmName>(item)
        .map(|n| n.name.replace("\\improper ", ""))
        .or_else(|| world.get::<Name>(item).map(|n| n.to_string()))
        .unwrap_or_default()
}

/// tg `/datum/mind/give_uplink(silent = TRUE)` for `mob`: an uplink in its PDA, else its radio.
pub fn give_uplink(
    world: &mut World,
    mob: Entity,
    phonetic: &[String],
    rng: &mut ss13_mapload::spawner::Rng,
) -> Option<UplinkGiven> {
    // `get_uplink_location(UPLINK_PDA)`: `my_pda || my_radio || my_pen`.
    let contents = super::objectives::all_contents(world, mob);
    let pda = contents
        .iter()
        .copied()
        .find(|&e| world.get::<ModularComputer>(e).is_some());
    let radio = pda.or_else(|| {
        contents
            .into_iter()
            .find(|&e| world.get::<Radio>(e).is_some())
    })?;
    let number = pda.map(|_| rng.range(100, 999));
    let mut word = || {
        phonetic
            .get(rng.range(0, phonetic.len() as i64 - 1) as usize)
            .cloned()
            .unwrap_or_default()
    };
    let name = item_name(world, radio);
    // `generate_code`: a number and a word for a computer, three words for a radio.
    let (code, unlock_text, unlock_note) = if let Some(number) = number {
        let code = format!("{number} {}", word());
        (
            code.clone(),
            format!(
                "Ваш аплинк хитро замаскирован под ваш {name}. Просто введите код \"{code}\" в \
                 настройках рингтона, чтобы открыть скрытые возможности."
            ),
            format!("Uplink Passcode: {code} ({name})."),
        )
    } else {
        let code = format!("{} {} {}", word(), word(), word());
        (
            code.clone(),
            format!(
                "Ваш аплинк хитро замаскирован под ваш {name}. Просто скажите \"{code}\" на \
                 частоте {RADIO_TOKEN_UPLINK}, чтобы открыть скрытые возможности."
            ),
            format!("Radio Passcode: {code} ({name}, {RADIO_TOKEN_UPLINK} channel)."),
        )
    };
    let role = world
        .get::<PlayerJob>(mob)
        .and_then(|j| {
            Some(
                world
                    .get_resource::<crate::jobs::Jobs>()?
                    .data
                    .jobs
                    .get(j.0)?
                    .title
                    .clone(),
            )
        })
        .unwrap_or_default();
    let board = {
        let catalog = world.resource::<UplinkCatalog>();
        board_for_role(catalog, &catalog.board, &role)
    };
    world.entity_mut(radio).insert((
        Uplink {
            owner: mob,
            unlock_code: code.clone(),
            locked: true,
            active: false,
            lockable: true,
            telecrystals: TELECRYSTALS_DEFAULT,
            uplink_flag: UPLINK_TRAITORS,
            item_stock: HashMap::new(),
            purchases: Vec::new(),
            shop_locked: false,
            assigned_role: role,
            primary_objectives: Vec::new(),
            failsafe_code: None,
            sales: Vec::new(),
            can_renegotiate: false,
            board,
        },
        // The window follows `inventory_state` ([`uplink_status`]), not the map distance.
        AlwaysState,
    ));
    if world.get::<PurchaseLog>(mob).is_none() {
        world.entity_mut(mob).insert(PurchaseLog::default());
    }
    Some(UplinkGiven {
        device: radio,
        code,
        unlock_text,
        unlock_note,
    })
}

/// `/obj/item/uplink/nuclear` (`uplink_flag = UPLINK_NUKE_OPS`, `owner` = the operative): an
/// open, unlockable uplink of `tc` telecrystals in `device`.
pub fn give_nuclear_uplink(world: &mut World, owner: Entity, device: Entity, tc: u32) {
    give_op_uplink(world, owner, device, tc, UPLINK_NUKE_OPS);
}

/// [`give_nuclear_uplink`] with the uplink type's flag: `/obj/item/uplink/loneop`
/// (`UPLINK_LONE_OP`), `/obj/item/uplink/clownop` (`UPLINK_CLOWN_OPS`).
pub fn give_op_uplink(world: &mut World, owner: Entity, device: Entity, tc: u32, flag: u32) {
    let board = world
        .get_resource::<UplinkCatalog>()
        .and_then(|c| c.op_boards.iter().find(|(f, _)| *f == flag))
        .map(|(_, b)| b.clone())
        .unwrap_or_default();
    world.entity_mut(device).insert((
        Uplink {
            owner,
            unlock_code: String::new(),
            locked: false,
            active: false,
            lockable: false,
            telecrystals: tc,
            uplink_flag: flag,
            item_stock: HashMap::new(),
            purchases: Vec::new(),
            shop_locked: false,
            assigned_role: super::nukeop::ROLE_OPERATIVE.into(),
            primary_objectives: Vec::new(),
            failsafe_code: None,
            sales: Vec::new(),
            can_renegotiate: false,
            board,
        },
        AlwaysState,
    ));
    if world.get::<PurchaseLog>(owner).is_none() {
        world.entity_mut(owner).insert(PurchaseLog::default());
    }
}

/// `interact`: an unlocked uplink opens its window for `user`.
pub fn open_uplink(world: &mut World, client: Entity, user: Entity, device: Entity) {
    if let Some(mut uplink) = world.get_mut::<Uplink>(device) {
        if uplink.locked {
            return;
        }
        uplink.active = true;
    }
    world.write_message(OpenUi::new::<UplinkUi>(client, user, device, UPLINK_NAME));
}

/// tg `new_ringtone` (`COMSIG_TABLET_CHANGE_ID`): the unlock code typed as the ringtone of
/// `device` unlocks its uplink and opens the window. `true` when it did (the ringtone stays).
pub fn unlock_by_ringtone(
    world: &mut World,
    client: Entity,
    user: Entity,
    device: Entity,
    ringtone: &str,
) -> bool {
    let Some(mut uplink) = world.get_mut::<Uplink>(device) else {
        return false;
    };
    let typed = ringtone.trim().to_lowercase();
    if typed != uplink.unlock_code.trim().to_lowercase() {
        // `COMPONENT_STOP_RINGTONE_CHANGE` after the failsafe.
        if uplink
            .failsafe_code
            .as_ref()
            .is_some_and(|c| !c.trim().is_empty() && c.trim().to_lowercase() == typed)
        {
            failsafe(world, device, Some(user));
            return true;
        }
        return false;
    }
    uplink.locked = false;
    open_uplink(world, client, user, device);
    world.write_message(SendChat {
        clients: vec![client],
        message: ChatMessage::simple(
            ChatChannel::System,
            Span::Plain,
            "The computer softly beeps.",
        ),
    });
    true
}

/// tg `new_message` on `RADIO_CHANNEL_UPLINK`: the code said into the headset unlocks it.
fn unlock_by_radio(
    mut spoken: MessageReader<Spoke>,
    mobs: Query<&Inventory>,
    mut uplinks: Query<&mut Uplink>,
    mut open: MessageWriter<OpenUi>,
    mut chat: MessageWriter<SendChat>,
    mut commands: Commands,
) {
    for spoke in spoken.read() {
        if spoke.radio != Some(RadioTarget::Channel(RADIO_CHANNEL_UPLINK)) {
            continue;
        }
        let Some(device) = mobs.get(spoke.speaker).ok().and_then(|inv| inv.ears) else {
            continue;
        };
        let Ok(mut uplink) = uplinks.get_mut(device) else {
            continue;
        };
        if !uplink.heard_code(&spoke.text) {
            if uplink.heard_failsafe(&spoke.text) {
                let speaker = spoke.speaker;
                commands.queue(move |world: &mut World| failsafe(world, device, Some(speaker)));
            }
            continue;
        }
        uplink.locked = false;
        uplink.active = true;
        open.write(OpenUi::new::<UplinkUi>(
            spoke.client,
            spoke.speaker,
            device,
            UPLINK_NAME,
        ));
        chat.write(SendChat {
            clients: vec![spoke.client],
            message: ChatMessage::simple(
                ChatChannel::System,
                Span::Plain,
                "As you whisper the code into your headset, a soft chime fills your ears.",
            ),
        });
    }
}

/// Whether a transmission of `text` from `device` on `channel` is swallowed by its uplink (tg
/// `COMPONENT_CANNOT_USE_RADIO` from `new_message`). `crate::radio` asks before transmitting.
pub fn swallows(uplink: Option<&Uplink>, channel: Option<RadioTarget>, text: &str) -> bool {
    channel == Some(RadioTarget::Channel(RADIO_CHANNEL_UPLINK))
        && uplink.is_some_and(|u| u.heard_code(text))
}

/// `ui_state` `inventory_state` + `ui_act`'s `if(!active) return`: the device must be active
/// and in the user's hands or slots; an unconscious user sees it disabled.
fn uplink_status(
    mut windows: Query<&mut UiWindow>,
    uplinks: Query<&Uplink>,
    users: Query<(&Inventory, Option<&Stat>)>,
) {
    for mut window in &mut windows {
        if window.interface != <UplinkUi as ss13_tgui::UiInterface>::NAME {
            continue;
        }
        let active = uplinks.get(window.source).is_ok_and(|u| u.active);
        let user = users.get(window.actor).ok();
        let held = user.is_some_and(|(inv, _)| inv.holds(window.source));
        if !active || !held {
            window.cap_status(UiStatus::Close);
        } else if user.is_some_and(|(_, stat)| stat.is_some_and(|s| *s != Stat::Conscious)) {
            window.cap_status(UiStatus::Disabled);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UplinkAct {
    Buy(String),
    BuyRawTc,
    Lock,
    RenegotiateObjectives,
}

impl ServerUi for UplinkUi {
    type Source = &'static Uplink;
    type UserParam = Query<'static, 'static, (), With<PlayerJob>>;
    type Act = UplinkAct;

    fn static_data(uplink: &Uplink) -> UplinkStatic {
        UplinkStatic {
            lockable: uplink.lockable,
            ..(*uplink.board).clone()
        }
    }

    fn data(
        uplink: &Uplink,
        user: Entity,
        crew: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> UplinkData {
        let mut current_stock: Vec<(String, i32)> = uplink
            .item_stock
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        current_stock.sort();
        UplinkData {
            telecrystals: uplink.telecrystals,
            joined_population: crew.iter().count() as u32,
            primary_objectives: uplink.primary_objectives.clone(),
            purchased_items: uplink.purchases.len() as u32,
            shop_locked: uplink.shop_locked,
            current_stock,
            // `user.mind == uplink_handler.owner && can_replace_objectives`.
            can_renegotiate: uplink.can_renegotiate && user == uplink.owner,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<UplinkAct, UiReject> {
        match action {
            BUY => params
                .get("path")
                .filter(|p| p.starts_with("/datum/uplink_item/"))
                .map(|p| UplinkAct::Buy(p.to_owned()))
                .ok_or_else(|| UiReject::BadParams("path: expected an uplink item type".into())),
            BUY_RAW_TC => Ok(UplinkAct::BuyRawTc),
            LOCK => Ok(UplinkAct::Lock),
            RENEGOTIATE_OBJECTIVES => Ok(UplinkAct::RenegotiateObjectives),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// Why a purchase was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuyReject {
    NoUplink,
    Inactive,
    NotHeld,
    UnknownItem,
    NotSold,
    ShopLocked,
    LocksOtherPurchases,
    Population,
    Telecrystals,
    OutOfStock,
    NoPrototype,
    /// The failsafe code on a device without an unlock code ("doesn't support code entry").
    NoCodeEntry,
}

/// `uplink_handler.purchase_item` with every check against the live world. Returns the item.
pub fn purchase(
    world: &mut World,
    user: Entity,
    device: Entity,
    path: &str,
) -> Result<Entity, BuyReject> {
    let uplink = world
        .get::<Uplink>(device)
        .cloned()
        .ok_or(BuyReject::NoUplink)?;
    if !uplink.active || uplink.locked {
        return Err(BuyReject::Inactive);
    }
    let inventory = world
        .get::<Inventory>(user)
        .cloned()
        .ok_or(BuyReject::NotHeld)?;
    if !inventory.holds(device) {
        return Err(BuyReject::NotHeld);
    }
    let catalog = world.resource::<UplinkCatalog>();
    let item = uplink
        .item_def(catalog, path)
        .ok_or(BuyReject::UnknownItem)?;
    // `can_purchase_item`.
    if uplink.shop_locked {
        return Err(BuyReject::ShopLocked);
    }
    if item.lock_other_purchases && !uplink.purchases.is_empty() {
        return Err(BuyReject::LocksOtherPurchases);
    }
    let restricted =
        !item.restricted_roles.is_empty() && !item.restricted_roles.contains(&uplink.assigned_role);
    if item.purchasable_from & uplink.uplink_flag == 0 || restricted {
        return Err(BuyReject::NotSold);
    }
    let joined = world
        .query_filtered::<(), With<PlayerJob>>()
        .iter(world)
        .count() as u32;
    if joined < item.population_minimum {
        return Err(BuyReject::Population);
    }
    if uplink.telecrystals < item.cost {
        return Err(BuyReject::Telecrystals);
    }
    let stock = uplink.item_stock.get(path).copied();
    if stock.is_some_and(|n| n <= 0) {
        return Err(BuyReject::OutOfStock);
    }
    if item.path == FAILSAFE_ITEM {
        return buy_failsafe(world, user, device, &item);
    }
    let spawned = world.resource_scope(|world, catalog: Mut<UplinkCatalog>| {
        catalog
            .templates
            .get(&item.item)
            .map(|t| t.spawn(world, None))
    });
    let spawned = spawned.ok_or(BuyReject::NoPrototype)?;
    let mut uplink = world.get_mut::<Uplink>(device).expect("checked above");
    if item.limited_stock != -1 {
        let left = uplink
            .item_stock
            .entry(path.to_owned())
            .or_insert(item.limited_stock);
        *left -= 1;
    }
    uplink.telecrystals -= item.cost;
    if !uplink.purchases.iter().any(|p| p == path) {
        uplink.purchases.push(path.to_owned());
    }
    if item.lock_other_purchases {
        uplink.shop_locked = true;
    }
    let owner = uplink.owner;
    let Some(in_hand) = hand_or_floor(world, user, spawned, &item.item) else {
        return Err(BuyReject::NotHeld);
    };
    if item.path != TELECRYSTAL_ITEM
        && let Some(mut log) = world.get_mut::<PurchaseLog>(owner)
    {
        log.log(&item.name, item.cost);
    }
    let name = capitalize(&item_name(world, spawned));
    let text = if in_hand {
        format!("{name} материализуется в вашей руке!")
    } else {
        format!("{name} материализуется на полу!")
    };
    if let Some(client) = world
        .get::<crate::server::PlayerMob>(user)
        .map(|p| p.client)
    {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage::simple(ChatChannel::System, Span::BoldNotice, text),
        });
    }
    info!(
        "uplink: {user} bought {} for {} TC ({} left)",
        item.name,
        item.cost,
        world.get::<Uplink>(device).map_or(0, |u| u.telecrystals)
    );
    Ok(spawned)
}

/// `spawn_item`: the fresh `spawned` (of type `item`) into `user`'s hands, the active hand
/// first; else (or not an item: a mech) onto their tile. `Some(true)` in a hand; `None` (it is
/// gone) when `user` is nowhere.
pub fn hand_or_floor(world: &mut World, user: Entity, spawned: Entity, item: &str) -> Option<bool> {
    crate::storage::finish_item(world, spawned, 0);
    let is_item = world.get::<DmItem>(spawned).is_some();
    let hand = world.get::<Inventory>(user).and_then(|inventory| {
        [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| is_item && inventory.hands[h].is_none())
    });
    let pos = crate::explosions::turf_of(world, user);
    match (hand, pos) {
        (Some(hand), _) => {
            world
                .entity_mut(spawned)
                .insert((Networked, CarriedBy(user)));
            world
                .get_mut::<Inventory>(user)
                .expect("checked above")
                .hands[hand] = Some(spawned);
            Some(true)
        }
        (None, Some(pos)) => {
            world.entity_mut(spawned).insert((Networked, pos));
            // tg `new spawn_path(get_turf(user))` runs the mech's `Initialize`.
            if !is_item
                && world
                    .get_resource::<crate::mecha::core::MechaDb>()
                    .is_some_and(|db| db.mech(item).is_some())
            {
                crate::mecha::core::init_mech(world, spawned, item, false);
            }
            Some(false)
        }
        (None, None) => {
            world.entity_mut(spawned).despawn();
            None
        }
    }
}

/// `spawn_item_for_generic_use(user)` of the catalog's uplink item `path`: its item in `user`'s
/// hands or at their feet ([`hand_or_floor`]). `None` without a catalog entry or prototype.
pub fn spawn_for_generic_use(
    world: &mut World,
    user: Entity,
    path: &str,
) -> Option<(Entity, bool)> {
    let spawned = world.resource_scope(|world, catalog: Mut<UplinkCatalog>| {
        let item = catalog.item(path)?;
        let template = catalog.templates.get(&item.item)?;
        Some((template.spawn(world, None), item.item.clone()))
    });
    let (spawned, item) = spawned?;
    hand_or_floor(world, user, spawned, &item).map(|in_hand| (spawned, in_hand))
}

/// Whether the catalog can spawn the uplink item `path` (it has a prototype).
pub fn can_spawn(catalog: &UplinkCatalog, path: &str) -> bool {
    catalog
        .item(path)
        .is_some_and(|i| catalog.templates.contains_key(&i.item))
}

/// `/datum/uplink_item/device_tools/failsafe/spawn_item`: a new failsafe code (never the
/// unlock code), noted in the traitor's memory and told to the buyer. Returns the device.
// tg spends the telecrystals before finding out that an implant cannot take codes; here a
// device without an unlock code refuses before charging.
fn buy_failsafe(
    world: &mut World,
    user: Entity,
    device: Entity,
    item: &UplinkItemDef,
) -> Result<Entity, BuyReject> {
    let uplink = world.get::<Uplink>(device).ok_or(BuyReject::NoUplink)?;
    if uplink.unlock_code.is_empty() {
        tell(
            world,
            user,
            Span::Warning,
            "This device doesn't support code entry!",
        );
        return Err(BuyReject::NoCodeEntry);
    }
    let phonetic = world
        .resource::<UplinkCatalog>()
        .data
        .phonetic_alphabet
        .clone();
    let mut rng = world
        .remove_resource::<super::dynamic::DynamicRng>()
        .map_or_else(|| Rng::new(device.to_bits()), |r| r.0);
    let code = generate_code(world, device, &phonetic, &mut rng);
    world.insert_resource(super::dynamic::DynamicRng(rng));
    let mut uplink = world.get_mut::<Uplink>(device).expect("checked above");
    uplink.telecrystals -= item.cost;
    if !uplink.purchases.iter().any(|p| p == &item.path) {
        uplink.purchases.push(item.path.clone());
    }
    uplink.failsafe_code = Some(code.clone());
    let owner = uplink.owner;
    if let Some(mut log) = world.get_mut::<PurchaseLog>(owner) {
        log.log(&item.name, item.cost);
    }
    let traitor = world.get_mut::<super::traitor::Traitor>(user).map(|mut t| {
        t.memory
            .push_str(&format!("\nUplink Failsafe Code: {code}"));
        t.failsafe_code = Some(code.clone());
    });
    let recall = if traitor.is_some() {
        " You may check your antagonist info to recall this."
    } else {
        ""
    };
    tell(
        world,
        user,
        Span::Warning,
        format!("The new failsafe code for this uplink is now: {code}.{recall}"),
    );
    Ok(device)
}

/// A line to `mob`'s player.
fn tell(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage::simple(ChatChannel::System, span, text),
        });
    }
}

/// On a `buy_raw_tc` prompt: the uplink it withdraws from.
#[derive(Component, Clone, Copy, Debug)]
struct RawTcPrompt {
    device: Entity,
}

/// `ui_act("buy_raw_tc")`: an active uplink with telecrystals asks how many.
fn ask_raw_tc(world: &mut World, client: Entity, user: Entity, device: Entity) {
    let Some(tc) = world
        .get::<Uplink>(device)
        .filter(|u| u.active)
        .map(|u| u.telecrystals)
    else {
        return;
    };
    if tc == 0 {
        return;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            user,
            "Buy Raw TC",
            "How many raw telecrystals to buy?",
            PromptKind::Number {
                default: f64::from(tc),
                min: 0.0,
                max: f64::from(tc),
                round: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(RawTcPrompt { device });
}

fn raw_tc_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, user, answer) in answers {
        let Some(RawTcPrompt { device }) = world.get::<RawTcPrompt>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Number(amount)) = answer else {
            continue;
        };
        // `!desired_amount || desired_amount < 1`.
        if !amount.is_finite() || amount < 1.0 {
            continue;
        }
        if let Err(reason) = purchase_raw_tc(world, user, device, amount as u32) {
            debug!("raw telecrystals for {user} refused: {reason:?}");
        }
    }
}

/// `uplink_handler.purchase_raw_tc`, re-checked after the prompt: the uplink is still active
/// and held, the shop is not locked and has the telecrystals. The stack goes into the hands.
pub fn purchase_raw_tc(
    world: &mut World,
    user: Entity,
    device: Entity,
    amount: u32,
) -> Result<Entity, BuyReject> {
    let uplink = world.get::<Uplink>(device).ok_or(BuyReject::NoUplink)?;
    if !uplink.active || uplink.locked {
        return Err(BuyReject::Inactive);
    }
    if uplink.shop_locked {
        return Err(BuyReject::ShopLocked);
    }
    if amount == 0 || uplink.telecrystals < amount {
        return Err(BuyReject::Telecrystals);
    }
    let inventory = world
        .get::<Inventory>(user)
        .cloned()
        .ok_or(BuyReject::NotHeld)?;
    if !inventory.holds(device) {
        return Err(BuyReject::NotHeld);
    }
    let pos = world
        .get::<TilePos>(user)
        .copied()
        .ok_or(BuyReject::NotHeld)?;
    let stack = crate::construction::spawn_stack(world, TELECRYSTAL, pos, amount)
        .ok_or(BuyReject::NoPrototype)?;
    world
        .get_mut::<Uplink>(device)
        .expect("checked above")
        .telecrystals -= amount;
    let inventory = world.get::<Inventory>(user).cloned().unwrap_or_default();
    crate::storage::put_in_hands(world, user, &inventory, stack);
    info!("uplink: {user} withdrew {amount} raw telecrystals");
    Ok(stack)
}

/// tg `OnAttackBy` -> `load_tc`: a telecrystal stack used on an active uplink charges it with
/// the whole stack. False for anything else (an inactive uplink lets the click through).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    _mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let is_tc = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, TELECRYSTAL));
    if !is_tc || !world.get::<Uplink>(target).is_some_and(|u| u.active) {
        return false;
    }
    let amount = crate::tools::stack_amount(world, held);
    let (tc_name, device_name) = (item_name(world, held), item_name(world, target));
    world
        .get_mut::<Uplink>(target)
        .expect("checked above")
        .telecrystals += amount;
    world.despawn(held);
    world.write_message(SendChat {
        clients: vec![client],
        message: ChatMessage::simple(
            ChatChannel::System,
            Span::Notice,
            format!("You slot {tc_name} into {device_name} and charge its internal uplink."),
        ),
    });
    info!("uplink: loaded {amount} telecrystals into {target}");
    true
}

use ss13_chat::capitalize;

/// `ui_act`: `buy`, `buy_raw_tc`, `lock` and `renegotiate_objectives`.
fn uplink_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<UplinkUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, UplinkAct)> = acts
        .read(world.resource::<Messages<UiActed<UplinkUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, user, device, act) in pending {
        if !world.get::<Uplink>(device).is_some_and(|u| u.active) {
            continue;
        }
        match act {
            UplinkAct::BuyRawTc => ask_raw_tc(world, client, user, device),
            UplinkAct::RenegotiateObjectives => {
                if world
                    .get::<Uplink>(device)
                    .is_some_and(|u| u.can_renegotiate && u.owner == user)
                {
                    super::traitor::ask_custom_objective(world, client, user);
                }
            }
            UplinkAct::Buy(path) => {
                if let Err(reason) = purchase(world, user, device, &path) {
                    debug!("uplink purchase of {path} by {user} refused: {reason:?}");
                }
            }
            // `lock_uplink`: the window closes through [`uplink_status`].
            UplinkAct::Lock => {
                if let Some(mut uplink) = world.get_mut::<Uplink>(device)
                    && uplink.lockable
                {
                    uplink.active = false;
                    uplink.locked = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_mapload::antag::UplinkItemDef;

    fn catalog(items: Vec<UplinkItemDef>) -> UplinkCatalog {
        UplinkCatalog {
            data: AntagData {
                uplink_items: items,
                ..default()
            },
            ..default()
        }
    }

    fn def(path: &str, cost: u32) -> UplinkItemDef {
        UplinkItemDef {
            path: path.into(),
            name: path.into(),
            item: "/obj/item/toy".into(),
            cost,
            purchasable_from: UPLINK_TRAITORS,
            limited_stock: -1,
            ..default()
        }
    }

    /// A user holding an unlocked uplink with `tc`; no item prototypes, so a purchase that
    /// passes every check stops at `NoPrototype`.
    fn world(tc: u32) -> (World, Entity, Entity) {
        let mut world = World::new();
        let mut cheap = def("/datum/uplink_item/a", 1);
        cheap.limited_stock = 1;
        let mut balloon = def("/datum/uplink_item/balloon", 2);
        balloon.lock_other_purchases = true;
        let mut nukie = def("/datum/uplink_item/nukie", 1);
        nukie.purchasable_from = 1 << 1;
        world.insert_resource(catalog(vec![
            cheap,
            balloon,
            nukie,
            def("/datum/uplink_item/big", 30),
        ]));
        let user = world.spawn(TilePos::new(1, 1, 1)).id();
        let device = world.spawn(CarriedBy(user)).id();
        world.entity_mut(user).insert(Inventory {
            ears: Some(device),
            ..default()
        });
        world.entity_mut(device).insert(Uplink {
            owner: user,
            unlock_code: "Альфа Браво Чарли".into(),
            locked: false,
            active: true,
            lockable: true,
            telecrystals: tc,
            uplink_flag: UPLINK_TRAITORS,
            item_stock: HashMap::new(),
            purchases: Vec::new(),
            shop_locked: false,
            assigned_role: String::new(),
            primary_objectives: Vec::new(),
            failsafe_code: None,
            sales: Vec::new(),
            can_renegotiate: false,
            board: default(),
        });
        (world, user, device)
    }

    /// Items restricted to other roles are not listed; the role's own are.
    #[test]
    fn the_window_hides_other_roles_items() {
        let mut hook = def("/datum/uplink_item/role_restricted/meathook", 11);
        hook.restricted_roles = vec!["Cook".into()];
        hook.category = "Role-Restricted".into();
        let mut toolbox = def("/datum/uplink_item/device_tools/toolbox", 1);
        toolbox.category = "Misc. Gadgets".into();
        let mut catalog = catalog(vec![hook, toolbox]);
        catalog.data.uplink_categories = vec!["Misc. Gadgets".into(), "Role-Restricted".into()];
        let items: Vec<UplinkItemRecord> = catalog
            .data
            .uplink_items
            .iter()
            .map(|i| record(i, &HashMap::new()))
            .collect();
        let board = Arc::new(UplinkStatic {
            categories: categories_of(&catalog.data, &items),
            items,
            lockable: true,
        });
        let ids = |b: &UplinkStatic| b.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>();
        let cook = board_for_role(&catalog, &board, "Cook");
        assert_eq!(ids(&cook).len(), 2);
        let clown = board_for_role(&catalog, &board, "Clown");
        assert_eq!(ids(&clown), ["/datum/uplink_item/device_tools/toolbox"]);
        assert_eq!(clown.categories, ["Misc. Gadgets"]);
    }

    #[test]
    fn purchases_are_checked_at_the_moment_of_effect() {
        let (mut world, user, device) = world(20);
        let buy = |world: &mut World, path: &str| purchase(world, user, device, path).err();
        assert_eq!(
            buy(&mut world, "/datum/uplink_item/a"),
            Some(BuyReject::NoPrototype)
        );
        assert_eq!(
            buy(&mut world, "/datum/uplink_item/x"),
            Some(BuyReject::UnknownItem)
        );
        assert_eq!(
            buy(&mut world, "/datum/uplink_item/nukie"),
            Some(BuyReject::NotSold)
        );
        assert_eq!(
            buy(&mut world, "/datum/uplink_item/big"),
            Some(BuyReject::Telecrystals)
        );
        // Out of stock, a shop locked by an earlier purchase, a lock-other item after one.
        world
            .get_mut::<Uplink>(device)
            .unwrap()
            .item_stock
            .insert("/datum/uplink_item/a".into(), 0);
        assert_eq!(
            buy(&mut world, "/datum/uplink_item/a"),
            Some(BuyReject::OutOfStock)
        );
        world.get_mut::<Uplink>(device).unwrap().purchases = vec!["x".into()];
        assert_eq!(
            buy(&mut world, "/datum/uplink_item/balloon"),
            Some(BuyReject::LocksOtherPurchases)
        );
        world.get_mut::<Uplink>(device).unwrap().shop_locked = true;
        assert_eq!(
            buy(&mut world, "/datum/uplink_item/big"),
            Some(BuyReject::ShopLocked)
        );
        // Not in the user's slots any more; locked.
        world.get_mut::<Inventory>(user).unwrap().ears = None;
        assert_eq!(
            buy(&mut world, "/datum/uplink_item/a"),
            Some(BuyReject::NotHeld)
        );
        world.get_mut::<Uplink>(device).unwrap().locked = true;
        assert_eq!(
            buy(&mut world, "/datum/uplink_item/a"),
            Some(BuyReject::Inactive)
        );
        // Nothing was spent by refused purchases.
        assert_eq!(world.get::<Uplink>(device).unwrap().telecrystals, 20);
    }

    #[test]
    fn sales_are_discounted_copies_one_per_category() {
        let item = |path: &str, cost, category: &str| UplinkItemDef {
            category: category.into(),
            ..def(path, cost)
        };
        let eligible = [
            item("/datum/uplink_item/a", 10, "A"),
            item("/datum/uplink_item/b", 8, "A"),
            item("/datum/uplink_item/c", 20, "B"),
            item("/datum/uplink_item/d", 4, "C"),
        ];
        for seed in 0..50 {
            let mut rng = Rng::new(seed);
            let sales = create_sales(6, "Discounted Gear", &eligible, &mut rng);
            assert_eq!(sales.len(), 3, "one per category");
            for sale in &sales {
                let base = sale.path.strip_suffix(SALE_SUFFIX).expect("a sale id");
                let original = eligible.iter().find(|i| i.path == base).unwrap();
                assert_eq!(sale.category, "Discounted Gear");
                assert!(sale.cost >= 1 && sale.cost < original.cost, "{sale:?}");
                assert!(sale.name.ends_with("% off!)"), "{}", sale.name);
                if original.cost >= 20 {
                    // Half the discount: 12.5%, 25% or 37.5% off 20.
                    assert!([17, 15, 12].contains(&sale.cost), "{}", sale.cost);
                }
            }
            let mut originals: Vec<&str> = sales
                .iter()
                .map(|s| {
                    eligible
                        .iter()
                        .find(|i| format!("{}{SALE_SUFFIX}", i.path) == s.path)
                        .unwrap()
                        .category
                        .as_str()
                })
                .collect();
            originals.sort();
            assert_eq!(originals, ["A", "B", "C"]);
        }
        let mut rng = Rng::new(1);
        assert_eq!(create_sales(2, "D", &eligible, &mut rng).len(), 2);
    }

    #[test]
    fn sale_copies_and_the_failsafe_are_bought_from_the_uplink() {
        let (mut world, user, device) = world(20);
        let mut sale = def("/datum/uplink_item/big?sale", 15);
        sale.category = "Discounted Gear".into();
        world.get_mut::<Uplink>(device).unwrap().sales = vec![sale];
        // The sale passes every check (no prototype here); a sale id the uplink lacks does not.
        assert_eq!(
            purchase(&mut world, user, device, "/datum/uplink_item/big?sale").err(),
            Some(BuyReject::NoPrototype)
        );
        assert_eq!(
            purchase(&mut world, user, device, "/datum/uplink_item/a?sale").err(),
            Some(BuyReject::UnknownItem)
        );
        // The failsafe: a second code, not the unlock code, heard on the uplink channel.
        world
            .resource_mut::<UplinkCatalog>()
            .data
            .uplink_items
            .push(def(FAILSAFE_ITEM, 1));
        world.resource_mut::<UplinkCatalog>().data.phonetic_alphabet =
            ["Альфа", "Браво", "Чарли", "Дельта"]
                .map(str::to_owned)
                .to_vec();
        assert_eq!(
            purchase(&mut world, user, device, FAILSAFE_ITEM),
            Ok(device)
        );
        let uplink = world.get::<Uplink>(device).unwrap();
        assert_eq!(uplink.telecrystals, 19);
        let code = uplink.failsafe_code.clone().expect("a failsafe code");
        assert_ne!(code, uplink.unlock_code);
        assert_eq!(code.split(' ').count(), 3, "a radio's code: {code}");
        assert!(uplink.heard_failsafe(&format!("ну {code}")));
        assert!(!uplink.heard_failsafe("Альфа"));
        // Without an unlock code (a nuclear uplink) nothing is charged.
        world.get_mut::<Uplink>(device).unwrap().unlock_code.clear();
        assert_eq!(
            purchase(&mut world, user, device, FAILSAFE_ITEM).err(),
            Some(BuyReject::NoCodeEntry)
        );
        assert_eq!(world.get::<Uplink>(device).unwrap().telecrystals, 19);
    }

    #[test]
    fn telecrystals_load_into_an_active_uplink_only() {
        let (mut world, user, device) = world(3);
        world.init_resource::<Messages<SendChat>>();
        let client = world.spawn_empty().id();
        let crystals = |world: &mut World| {
            world
                .spawn((
                    DmItem {
                        path: TELECRYSTAL.into(),
                        ..default()
                    },
                    crate::tools::Stack { amount: 5 },
                ))
                .id()
        };
        let tc = crystals(&mut world);
        assert!(item_interaction(&mut world, client, user, tc, device));
        assert_eq!(world.get::<Uplink>(device).unwrap().telecrystals, 8);
        assert!(world.get_entity(tc).is_err(), "the stack is used up");
        world.get_mut::<Uplink>(device).unwrap().active = false;
        let tc = crystals(&mut world);
        assert!(!item_interaction(&mut world, client, user, tc, device));
        assert_eq!(world.get::<Uplink>(device).unwrap().telecrystals, 8);
        // Raw telecrystals: refused on an inactive uplink, or past the balance.
        assert_eq!(
            purchase_raw_tc(&mut world, user, device, 2).err(),
            Some(BuyReject::Inactive)
        );
        world.get_mut::<Uplink>(device).unwrap().active = true;
        assert_eq!(
            purchase_raw_tc(&mut world, user, device, 9).err(),
            Some(BuyReject::Telecrystals)
        );
    }

    #[test]
    fn the_code_unlocks_only_on_the_uplink_channel() {
        let (world, _, device) = world(20);
        let uplink = world.get::<Uplink>(device);
        let channel = Some(RadioTarget::Channel(RADIO_CHANNEL_UPLINK));
        assert!(swallows(uplink, channel, "эээ альфа браво чарли!"));
        assert!(!swallows(uplink, channel, "альфа браво"));
        assert!(!swallows(
            uplink,
            Some(RadioTarget::Headset),
            "Альфа Браво Чарли"
        ));
        assert!(!swallows(None, channel, "Альфа Браво Чарли"));
        let mut log = PurchaseLog::default();
        log.log("Toolbox", 1);
        log.log("Toolbox", 1);
        log.log("Balloon", 20);
        assert_eq!(log.entries.len(), 2);
        assert_eq!((log.entries[0].amount, log.total_spent), (2, 22));
    }
}
