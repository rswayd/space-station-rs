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
//! the buyer's hands, else at their feet.
// shortcut: the uplink location preference is always tg's default (PDA); pen twisting and the
// implant fallback are not modelled (no PDA and no radio: no uplink). No failsafe code, raw
// telecrystals, discounts or refunds yet.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_chat::radio::RADIO_CHANNEL_UPLINK;
use ss13_chat::server::{SaySystems, Spoke};
use ss13_chat::speech::RadioTarget;
use ss13_mapload::antag::{ANTAG_FILE, AntagData, UPLINK_NUKE_OPS, UPLINK_TRAITORS, UplinkItemDef};
use ss13_mapload::{DmName, MapLoadPlugin, Template, read_manifest};
use ss13_net::chat::server::SendChat;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_tgui::UiStatus;
use ss13_tgui::interfaces::vending::ProductIcon;
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::{BUY, LOCK, UplinkData, UplinkItemRecord, UplinkStatic, UplinkUi};
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

/// The catalog and the item templates, loaded at startup from `<assets>/antag.ron`.
#[derive(Resource, Default)]
pub struct UplinkCatalog {
    pub data: AntagData,
    templates: HashMap<String, Template>,
    board: Arc<UplinkStatic>,
    /// The catalog of `UPLINK_NUKE_OPS` uplinks.
    nuke_board: Arc<UplinkStatic>,
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
        app.register_ui::<UplinkUi>()
            .init_resource::<UplinkCatalog>()
            .add_systems(Startup, load_catalog)
            .add_systems(
                Update,
                (
                    unlock_by_radio.after(SaySystems).before(UiSystems::Open),
                    uplink_status.in_set(UiSystems::StatusHook),
                    uplink_acts.in_set(UiSystems::Handle),
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
            .map(|i| UplinkItemRecord {
                id: i.path.clone(),
                name: i.name.clone(),
                desc: i.desc.clone(),
                cost: i.cost,
                category: i.category.clone(),
                icon: templates[&i.item].appearance.as_ref().map(|a| ProductIcon {
                    icon_state: a.icon_state,
                    color: a.color,
                }),
                limited_stock: i.limited_stock,
                lock_other_purchases: i.lock_other_purchases,
                population_minimum: i.population_minimum,
            })
            .collect();
        let categories = data
            .uplink_categories
            .iter()
            .filter(|c| items.iter().any(|i| &i.category == *c))
            .cloned()
            .collect();
        UplinkStatic {
            categories,
            items,
            // `/obj/item/uplink` is not lockable; a traitor's hidden uplink is.
            lockable: flag == UPLINK_TRAITORS,
        }
    };
    let board = Arc::new(board_of(UPLINK_TRAITORS));
    let nuke_board = Arc::new(board_of(UPLINK_NUKE_OPS));
    info!(
        "uplink: {} items for traitors, {} for nuclear operatives, {} steal targets",
        board.items.len(),
        nuke_board.items.len(),
        data.steal_items.len()
    );
    world.insert_resource(UplinkCatalog {
        data,
        templates,
        board,
        nuke_board,
    });
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
    let board = world.resource::<UplinkCatalog>().board.clone();
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
    let board = world
        .get_resource::<UplinkCatalog>()
        .map(|c| c.nuke_board.clone())
        .unwrap_or_default();
    world.entity_mut(device).insert((
        Uplink {
            owner,
            unlock_code: String::new(),
            locked: false,
            active: false,
            lockable: false,
            telecrystals: tc,
            uplink_flag: UPLINK_NUKE_OPS,
            item_stock: HashMap::new(),
            purchases: Vec::new(),
            shop_locked: false,
            assigned_role: super::nukeop::ROLE_OPERATIVE.into(),
            primary_objectives: Vec::new(),
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
    if ringtone.trim().to_lowercase() != uplink.unlock_code.trim().to_lowercase() {
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
    Lock,
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
        _: Entity,
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
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<UplinkAct, UiReject> {
        match action {
            BUY => params
                .get("path")
                .filter(|p| p.starts_with("/datum/uplink_item/"))
                .map(|p| UplinkAct::Buy(p.to_owned()))
                .ok_or_else(|| UiReject::BadParams("path: expected an uplink item type".into())),
            LOCK => Ok(UplinkAct::Lock),
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
    let item = catalog.item(path).cloned().ok_or(BuyReject::UnknownItem)?;
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
    crate::storage::finish_item(world, spawned, 0);
    // `spawn_item`: `put_in_hands`, the active hand first; else it stays on the floor.
    let hand = [inventory.active, 1 - inventory.active]
        .into_iter()
        .find(|&h| inventory.hands[h].is_none());
    let pos = world.get::<TilePos>(user).copied();
    match (hand, pos) {
        (Some(hand), _) => {
            world
                .entity_mut(spawned)
                .insert((Networked, CarriedBy(user)));
            world
                .get_mut::<Inventory>(user)
                .expect("checked above")
                .hands[hand] = Some(spawned);
        }
        (None, Some(pos)) => {
            world.entity_mut(spawned).insert((Networked, pos));
        }
        (None, None) => {
            world.entity_mut(spawned).despawn();
            return Err(BuyReject::NotHeld);
        }
    }
    if let Some(mut log) = world.get_mut::<PurchaseLog>(owner) {
        log.log(&item.name, item.cost);
    }
    let name = capitalize(&item_name(world, spawned));
    let text = if hand.is_some() {
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

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// `ui_act`: `buy` and `lock`.
fn uplink_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<UplinkUi>>>) {
    let pending: Vec<(Entity, Entity, UplinkAct)> = acts
        .read(world.resource::<Messages<UiActed<UplinkUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (user, device, act) in pending {
        match act {
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
            board: default(),
        });
        (world, user, device)
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
