//! Vending machines (server), after tg `code/modules/vending/vendor/`: a click with an empty
//! active hand on an adjacent vendor opens the `Vending` window (`ss13_tgui::interfaces::vending`
//! has the data shapes and the client view); `vend` hands out one item.
//!
//! Records come from the converter's [`DmVending`] (products, contraband, premium, prices) and
//! live in [`Vendor`]; the stock is per machine. `vend` is checked at the moment of effect: the
//! window is interactive this tick (tgui distance rules) and the actor is still within one tile,
//! the record exists in this vendor and is sellable (contraband only with `extended_inventory`),
//! it is in stock, the vendor works (power, not broken), and, unless the vendor is free, the
//! actor carries an ID: old enough for an age-restricted product, with a bank account that pays
//! the price (`proceed_payment`: 20% for the vendor's own department and the captain, premium
//! and contraband at full price, into the vendor's department budget; returned items are free).
//! tg has no vend cooldown any more (`vend_ready` was removed); repeated acts are bounded by
//! tgui's act rate limit and the stock, and each one charges at its own moment.
//!
//! Power ([`Unpowered`], tg `machine_stat & NOPOWER`) and breaking (`BROKEN`): `can_interact`
//! fails, so a click opens nothing and an open window drops to update-only (`/atom/ui_status`);
//! the sprite switches to `<icon_state>-off` (`-broken`) without the `light_mask` emissive.
//!
//! - Speech ([`speak`]): sold out, no ID, underage, no funds, the vend reply; slogans every
//!   10+ minutes (`process`), all silenced by a cut speaker wire.
//! - Items of the vendor's types go back in ([`item_interaction`], tg `loadingAttempt`) and are
//!   sold first and free.
//! - Refill canisters, deconstruction and building: [`refill`]. Tilting: [`tilt`]. The custom
//!   vendor: [`custom`].
// shortcut: no greyscale (GAGS) colour menu, no cash or holochips (credits a vendor collects
// stay in it until a restock pays them to the restocker's account), no AAS age announcement
// (the announcement system is not ported), no speaker language.

pub mod custom;
pub mod refill;
pub mod tilt;

use std::collections::HashMap;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmName, DmVending, IconIndex, MapLoadPlugin, Template, read_manifest};
use ss13_net::server::Networked;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, EMISSIVE_PLANE, IconStateId, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::UiStatus;
use ss13_tgui::interfaces::vending::{
    ProductIcon, ProductRecord, StockItem, UserData, VendingData, VendingStatic, VendingUi,
    sanitized_path,
};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow, distance_status,
};
use ss13_world::{Mover, WorldSystems};

use crate::cargo::economy::{ACCOUNT_CIV, BankAccountId, Economy, bank_card_talk};
use crate::click::{ClickResolved, ResolveClicks};
use crate::combat::CombatRng;
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::{Integrity, is_under};
use crate::power::Unpowered;
use crate::tools::{Actor, tell};

pub const VENDING_ROOT: &str = ss13_mapload::vending::VENDING_ROOT;
/// `DEPARTMENT_DISCOUNT`: the share of the price the vendor's department pays.
pub const DEPARTMENT_DISCOUNT: f32 = 0.2;
/// `VENDING_CREDITS_COLLECTION_AMOUNT`.
const CREDITS_COLLECTION: f32 = 0.2;
/// `AGE_MINOR`.
pub const AGE_MINOR: u32 = 20;
/// `MONEY_SYMBOL`; tg sends it with a leading space.
const MONEY_SYMBOL: &str = "¢";
/// `slogan_delay` (10 minutes), seconds.
pub const SLOGAN_DELAY: f64 = 600.0;
/// `SSmachines` wait: `process` runs this often, seconds.
const MACHINE_TICK: f64 = 2.0;
/// `SPT_PROB(2.5, seconds_per_tick)` of a slogan once it is due.
const SLOGAN_PROB: f32 = 2.5;
/// `purchase_message_cooldown`: 5 seconds between vend replies to one shopper.
const REPLY_COOLDOWN: f64 = 5.0;
/// `big_boss_trim`: `/datum/id_trim/job/captain`.
const CAPTAIN: &str = "Captain";
const VEND_SOUND: &str = "sound/machines/machine_vend.ogg";
const CASH_SOUND: &str = "sound/effects/cashregister.ogg";
/// `/obj/item/card/emag`.
const EMAG: &str = "/obj/item/card/emag";
const STORAGE_BAG: &str = "/obj/item/storage/bag";

/// The server side of the `Vending` interface (same name and data as [`VendingUi`]).
pub struct VendingWindow;

impl UiInterface for VendingWindow {
    const NAME: &'static str = VendingUi::NAME;
    type Data = VendingData;
    type StaticData = VendingStatic;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordKind {
    /// `product_records`.
    Product,
    /// `coin_records` (premium).
    Coin,
    /// `hidden_records` (contraband).
    Hidden,
}

/// `datum/data/vending_product`.
#[derive(Clone, Debug, PartialEq)]
pub struct VendRecord {
    pub path: String,
    /// Stock, the returned items included.
    pub amount: u32,
    pub max_amount: u32,
    pub kind: RecordKind,
    /// The price before inflation (the converter's `build_inventory` price).
    pub base_price: u32,
    pub price: u32,
    pub age_restricted: bool,
    /// `returned_products`: items put back, inside the vendor ([`crate::storage::StoredIn`]),
    /// sold first (last in, first out) and free.
    pub returned: Vec<Entity>,
}

/// A vending machine's live state. Record index = the `ref` the window sends back; records are
/// never removed, so an index stays the same record.
#[derive(Component, Clone, Debug)]
pub struct Vendor {
    pub records: Vec<VendRecord>,
    pub extended_inventory: bool,
    pub all_products_free: bool,
    /// `ui_static_data` without the ad.
    pub ui: VendingStatic,
    pub ads: Vec<String>,
    pub slogans: Vec<String>,
    pub vend_reply: String,
    pub payment_department: String,
    pub onstation: bool,
    pub age_restrictions: bool,
    pub req_access: Vec<String>,
    /// `refill_canister` type; empty: no circuit.
    pub refill_canister: String,
    pub tiltable: bool,
    pub squish_damage: f32,
    pub crit_chance: f32,
    /// `credits_contained`: 20% of every sale, paid out on a restock.
    pub credits_contained: i64,
    /// `obj_flags & EMAGGED`.
    pub emagged: bool,
    /// `last_slogan`, game seconds.
    pub last_slogan: f64,
    /// `last_shopper` and `purchase_message_cooldown`.
    pub last_shopper: Option<(Entity, f64)>,
}

impl Vendor {
    /// The record `vend` may sell: in this vendor, contraband only when hacked.
    pub fn sellable(&self, reference: u32) -> Option<&VendRecord> {
        let record = self.records.get(reference as usize)?;
        (record.kind != RecordKind::Hidden || self.extended_inventory).then_some(record)
    }

    /// `total_stock()`: loaded and maximum over every record.
    pub fn total_stock(&self) -> (u32, u32) {
        self.records
            .iter()
            .fold((0, 0), |(n, m), r| (n + r.amount, m + r.max_amount))
    }
}

/// Why a `vend` did nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendReject {
    /// The actor is gone, not living, or not on the map.
    NoActor,
    NoVendor,
    TooFar,
    /// Not a record of this vendor, or contraband of an unhacked one (tg: "exploit attempted").
    NotSold,
    SoldOut,
    /// tg `"You do not possess an ID to purchase ..."`.
    NoId,
    /// [`Unpowered`] or broken (tg: `can_interact` fails, the act never reaches `vend`).
    NoPower,
    /// The product type has no prototype (rerun the converter).
    NoPrototype,
    /// An age-restricted product and an ID under `AGE_MINOR`.
    Underage,
    /// The ID has no bank account (tg `handle_card`: "No account found.").
    NoAccount,
    /// The account cannot pay the price.
    NoFunds,
}

/// One accepted `vend`, for logs and tests.
#[derive(Message, Clone, Debug)]
pub struct Vended {
    pub actor: Entity,
    pub vendor: Entity,
    pub item: Entity,
    pub path: String,
    /// The item went into a hand (else it is on the vendor's tile).
    pub in_hand: bool,
    /// What the account paid.
    pub paid: u32,
}

/// A dropped `vend`.
#[derive(Message, Clone, Debug)]
pub struct VendRejected {
    pub actor: Entity,
    pub vendor: Entity,
    pub reason: VendReject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendingAct {
    /// `vend` with `ref`. tg's `premium` param is not trusted: the record's kind decides.
    Vend { reference: u32 },
}

/// A timed vendor step (`do_after`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendJob {
    /// Righting a tilted vendor (`interact`, 5 s).
    Right,
}

/// A vendor's sprite with and without power and broken: icon state and overlays.
#[derive(Component, Clone, Debug)]
struct PowerLooks {
    powered: (IconStateId, Vec<Overlay>),
    unpowered: (IconStateId, Vec<Overlay>),
    broken: (IconStateId, Vec<Overlay>),
}

/// Prototypes of everything vendors sell, built at startup for the map and on demand for built
/// vendors.
#[derive(Resource, Default)]
pub struct ProductTemplates(pub HashMap<String, Template>);

/// `SSeconomy.inflation_value` while a market crash is on (1: prices as built).
#[derive(Resource, Clone, Copy, Debug)]
pub struct VendingInflation(pub f32);

impl Default for VendingInflation {
    fn default() -> Self {
        Self(1.0)
    }
}

type UserQueries = (
    Query<'static, 'static, &'static Inventory>,
    crate::inventory::IdCards<'static, 'static>,
    Query<'static, 'static, &'static BankAccountId>,
    Query<'static, 'static, &'static crate::id_access::CardAccess>,
    Option<Res<'static, Economy>>,
    Query<'static, 'static, &'static custom::CustomVendor>,
);

impl ServerUi for VendingWindow {
    type Source = (&'static Vendor, Entity);
    type UserParam = UserQueries;
    type Act = VendingAct;

    fn static_data((vendor, _): (&Vendor, Entity)) -> VendingStatic {
        let mut data = vendor.ui.clone();
        // tg picks a random ad per `ui_static_data`.
        if !vendor.ads.is_empty() {
            let nanos = web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos() as usize);
            data.ad = vendor.ads[nanos % vendor.ads.len()].clone();
        }
        data
    }

    fn data(
        (vendor, source): (&Vendor, Entity),
        user: Entity,
        (inventories, cards, accounts, access, economy, customs): &SystemParamItem<
            '_,
            '_,
            UserQueries,
        >,
    ) -> VendingData {
        // tg `get_idcard(TRUE)`: hands first, then the ID slot; `user` only with an account.
        let card = inventories
            .get(user)
            .ok()
            .and_then(|i| i.id_search().find_map(|e| cards.id_of(e)));
        let account = card.and_then(|c| accounts.get(c).ok()).map(|a| a.0);
        let user_data = account.and_then(|id| {
            let account = economy.as_ref()?.account(id)?;
            let captain = card
                .and_then(|c| access.get(c).ok())
                .and_then(|a| a.trim.as_ref())
                .is_some_and(|t| t.job == CAPTAIN);
            let has_job = !account.paycheck_department.is_empty();
            Some(UserData {
                name: account.holder.clone(),
                cash: Some(account.balance.clamp(0, i64::from(u32::MAX)) as u32),
                job: if has_job {
                    account.job.clone()
                } else {
                    "No Job".into()
                },
                department: if has_job {
                    account.paycheck_department.clone()
                } else {
                    "No Department".into()
                },
                is_captain_card: captain,
            })
        });
        // The custom vendor's owner takes for free (`free = is_owner`, `get_idcard(FALSE)`:
        // the ID slot first).
        let owner = customs.get(source).is_ok_and(|c| {
            let owner_card = inventories.get(user).ok().and_then(|i| {
                [i.id, i.hands[i.active], i.hands[1 - i.active]]
                    .into_iter()
                    .flatten()
                    .find_map(|e| cards.id_of(e))
            });
            let owner_account = owner_card.and_then(|c| accounts.get(c).ok()).map(|a| a.0);
            owner_account.is_some() && c.linked == owner_account
        });
        VendingData {
            user: user_data,
            stock: vendor
                .records
                .iter()
                .map(|r| StockItem {
                    amount: r.amount,
                    free: owner || !r.returned.is_empty(),
                })
                .collect(),
            // A custom vendor's sold-out rows wait in `hidden_records`.
            extended_inventory: vendor.extended_inventory && !customs.contains(source),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<VendingAct, UiReject> {
        match action {
            "vend" => {
                let reference = params
                    .get("ref")
                    .and_then(|r| r.trim().parse::<u32>().ok())
                    .ok_or_else(|| UiReject::BadParams("ref: expected a record index".into()))?;
                Ok(VendingAct::Vend { reference })
            }
            // `select_colors` opens tg's greyscale menu, which is not ported; no record is
            // colorable here, so the window never sends it.
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub struct VendingServerPlugin;

impl Plugin for VendingServerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<VendingWindow>()
            .init_resource::<ProductTemplates>()
            .init_resource::<VendingInflation>()
            .add_message::<Vended>()
            .add_message::<VendRejected>()
            .add_systems(Startup, init_vendors)
            .add_systems(
                Update,
                (
                    open_on_click
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    apply_vends.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                    process.run_if(bevy::time::common_conditions::on_timer(
                        std::time::Duration::from_secs_f64(MACHINE_TICK),
                    )),
                    tilt::hold_pinned,
                    (
                        refill::brand_answers,
                        custom::self_destruct_answers,
                        custom::blasts,
                    )
                        .chain()
                        .in_set(UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                ),
            )
            .add_observer(power_lost)
            .add_observer(power_back)
            .add_observer(refill::name_canister);
    }
}

/// tg `/atom/ui_status`: a machine that `can_interact` refuses (no power, broken) is
/// update-only.
fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    vendors: Query<(Has<Unpowered>, Option<&Integrity>), With<Vendor>>,
) {
    for mut window in &mut windows {
        if window.interface != VendingWindow::NAME {
            continue;
        }
        if let Ok((unpowered, integrity)) = vendors.get(window.source)
            && (unpowered || integrity.is_some_and(|i| i.broken))
        {
            window.cap_status(UiStatus::Update);
        }
    }
}

/// tg `is_operational`: powered and not broken.
pub fn operational(world: &World, vendor: Entity) -> bool {
    world.get::<Unpowered>(vendor).is_none()
        && world.get::<Integrity>(vendor).is_none_or(|i| !i.broken)
}

fn set_look(appearance: &mut Appearance, (icon_state, overlays): &(IconStateId, Vec<Overlay>)) {
    appearance.icon_state = *icon_state;
    appearance.overlays.clone_from(overlays);
}

/// `update_icon_state` + `update_overlays`: broken, then unpowered, else lit.
pub fn refresh_look(world: &mut World, vendor: Entity) {
    let Some(looks) = world.get::<PowerLooks>(vendor).cloned() else {
        return;
    };
    let broken = world.get::<Integrity>(vendor).is_some_and(|i| i.broken);
    let look = if broken {
        &looks.broken
    } else if world.get::<Unpowered>(vendor).is_some() {
        &looks.unpowered
    } else {
        &looks.powered
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(vendor) {
        set_look(&mut appearance, look);
    }
}

fn power_lost(
    add: On<Add<Unpowered>>,
    vendors: Query<(), With<PowerLooks>>,
    mut commands: Commands,
) {
    if vendors.contains(add.entity) {
        let vendor = add.entity;
        commands.queue(move |world: &mut World| refresh_look(world, vendor));
    }
}

fn power_back(
    remove: On<Remove<Unpowered>>,
    vendors: Query<(), With<PowerLooks>>,
    mut commands: Commands,
) {
    if vendors.contains(remove.entity) {
        let vendor = remove.entity;
        // `Remove` observers run before the component is gone.
        commands.queue(move |world: &mut World| refresh_look(world, vendor));
    }
}

/// BYOND text macros at the start of a name.
pub(crate) fn plain_name(name: &str) -> &str {
    name.trim_start_matches("\\improper ")
        .trim_start_matches("\\proper ")
}

/// tg `capitalize`: the first letter upper-cased.
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

pub(crate) fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| plain_name(&n.name).to_owned())
        .or_else(|| world.get::<Name>(e).map(|n| n.as_str().to_owned()))
        .unwrap_or_else(|| "что-то".to_owned())
}

/// Templates for `paths` missing from [`ProductTemplates`] (built vendors, custom vendors).
pub(crate) fn add_templates<'a>(world: &mut World, paths: impl IntoIterator<Item = &'a str>) {
    let missing: Vec<String> = {
        let templates = world.get_resource_or_init::<ProductTemplates>();
        paths
            .into_iter()
            .filter(|p| !templates.0.contains_key(*p))
            .map(str::to_owned)
            .collect()
    };
    if missing.is_empty() {
        return;
    }
    let built: Vec<(String, Template)> = {
        let index: HashMap<(&str, &str), IconStateId> = world
            .get_resource::<IconIndex>()
            .map(|i| {
                i.0.iter()
                    .map(|((dmi, state), id)| ((dmi.as_str(), state.as_str()), *id))
                    .collect()
            })
            .unwrap_or_default();
        let Some(protos) = world.get_resource::<ss13_proto::Prototypes>() else {
            return;
        };
        missing
            .into_iter()
            .filter_map(|p| {
                let proto = protos.get(&p)?.clone();
                Some((p, Template::new(proto, &index)))
            })
            .collect()
    };
    world.resource_mut::<ProductTemplates>().0.extend(built);
}

/// [`Vendor`] for every map-loaded vendor, and the product templates they need.
fn init_vendors(world: &mut World) {
    let mut query = world.query::<(Entity, &DmVending)>();
    let found: Vec<(Entity, DmVending)> = query.iter(world).map(|(e, v)| (e, v.clone())).collect();
    if found.is_empty() {
        return;
    }
    let Some(assets) = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.clone())
    else {
        return;
    };
    let manifest = match read_manifest(&assets) {
        Ok(manifest) => manifest,
        Err(e) => {
            warn!("vendors stay empty: {e:#}");
            return;
        }
    };
    let index = manifest.index();
    let mut templates = HashMap::new();
    let mut missing = 0usize;
    let mut unresolved = 0usize;
    let mut vendors = Vec::new();
    let station = crate::events::controller::station_levels(world);
    let now = world.resource::<Time>().elapsed_secs_f64();
    for (entity, dm) in found {
        if !dm.unresolved.is_empty() {
            unresolved += 1;
        }
        for product in dm.products.iter().chain(&dm.contraband).chain(&dm.premium) {
            if templates.contains_key(&product.path) {
                continue;
            }
            match world
                .resource::<ss13_proto::Prototypes>()
                .get(&product.path)
                .cloned()
            {
                Some(proto) => {
                    templates.insert(product.path.clone(), Template::new(proto, &index));
                }
                None => missing += 1,
            }
        }
        let mut machine = vendor(&dm, &templates);
        // `Initialize(mapload)`: off the station levels a vendor is not on station and, unless
        // the type says otherwise, free.
        let on_station_level = world
            .get::<TilePos>(entity)
            .is_some_and(|p| station.contains(&p.z));
        if !on_station_level {
            machine.onstation = false;
            machine.all_products_free = dm.all_products_free.unwrap_or(true);
            machine.ui.all_products_free = machine.all_products_free;
        }
        // "So not all machines speak at the exact same time."
        let delay = world
            .get_resource_mut::<CombatRng>()
            .map_or(0.0, |mut rng| f64::from(rng.unit()) * SLOGAN_DELAY);
        machine.last_slogan = now + delay;
        vendors.push((entity, machine));
        let looks = world.get::<Appearance>(entity).map(|a| {
            let state = |suffix: &str| {
                world.get::<DmAtom>(entity).and_then(|atom| {
                    let state = format!("{}-{suffix}", atom.icon_state);
                    index.get(&(atom.icon.as_str(), state.as_str())).copied()
                })
            };
            looks_of(a, state("off"), state("broken"))
        });
        if let Some(looks) = looks {
            world.entity_mut(entity).insert(looks);
        }
    }
    info!(
        "{} vendors ({unresolved} without static product lists), {} product types, {missing} without a prototype",
        vendors.len(),
        templates.len()
    );
    world.insert_batch(vendors);
    world.resource_mut::<ProductTemplates>().0.extend(templates);
    custom::init(world);
}

fn looks_of(a: &Appearance, off: Option<IconStateId>, broken: Option<IconStateId>) -> PowerLooks {
    let dark: Vec<Overlay> = a
        .overlays
        .iter()
        .filter(|o| o.plane != Some(EMISSIVE_PLANE))
        .cloned()
        .collect();
    PowerLooks {
        powered: (a.icon_state, a.overlays.clone()),
        unpowered: (off.unwrap_or(a.icon_state), dark.clone()),
        broken: (broken.or(off).unwrap_or(a.icon_state), dark),
    }
}

/// [`PowerLooks`] of a vendor spawned after map load.
pub(crate) fn add_looks(world: &mut World, vendor: Entity) {
    let Some(appearance) = world.get::<Appearance>(vendor).cloned() else {
        return;
    };
    let state = |suffix: &str| {
        let atom = world.get::<DmAtom>(vendor)?;
        world
            .get_resource::<IconIndex>()?
            .get(&atom.icon, &format!("{}-{suffix}", atom.icon_state))
    };
    let looks = looks_of(&appearance, state("off"), state("broken"));
    world.entity_mut(vendor).insert(looks);
}

/// tg `build_inventories` + `ui_static_data` (minus the ad).
pub fn vendor(dm: &DmVending, templates: &HashMap<String, Template>) -> Vendor {
    let mut records = Vec::new();
    let mut ui = VendingStatic {
        all_products_free: dm.all_products_free.unwrap_or(false),
        department: dm.payment_department.clone(),
        job_discount: DEPARTMENT_DISCOUNT,
        displayed_currency_icon: if dm.displayed_currency_icon.is_empty() {
            "coins".into()
        } else {
            dm.displayed_currency_icon.clone()
        },
        displayed_currency_name: format!(
            " {}",
            if dm.displayed_currency_name.is_empty() {
                MONEY_SYMBOL
            } else {
                &dm.displayed_currency_name
            }
        ),
        ..default()
    };
    for (list, kind) in [
        (&dm.products, RecordKind::Product),
        (&dm.premium, RecordKind::Coin),
        (&dm.contraband, RecordKind::Hidden),
    ] {
        for product in list {
            let reference = records.len() as u32;
            records.push(VendRecord {
                path: product.path.clone(),
                amount: product.amount,
                max_amount: product.amount,
                kind,
                base_price: product.price,
                price: product.price,
                age_restricted: product.age_restricted,
                returned: Vec::new(),
            });
            let template = templates.get(&product.path);
            let name = template
                .and_then(|t| t.proto.get::<DmName>())
                .map(|n| plain_name(&n.name).to_owned())
                .unwrap_or_else(|| ss13_mapload::convert::default_name(&product.path));
            let category = dm
                .categories
                .get(usize::from(product.category))
                .map(|c| (c.name.clone(), c.icon.clone()))
                .unwrap_or_else(|| ("Products".into(), "cart-shopping".into()));
            // The JS object keeps insertion order: product categories, then premium, then
            // contraband, each the first time a record names it.
            if !ui.categories.iter().any(|(n, _)| *n == category.0) {
                ui.categories.push(category.clone());
            }
            let record = ProductRecord {
                path: sanitized_path(&product.path),
                name: capitalize(&name),
                price: product.price,
                reference,
                category: category.0,
                colorable: false,
                premium: kind != RecordKind::Product,
                icon: template
                    .and_then(|t| t.appearance.as_ref())
                    .map(|a| ProductIcon {
                        icon_state: a.icon_state,
                        color: a.color,
                    }),
            };
            match kind {
                RecordKind::Product => ui.product_records.push(record),
                RecordKind::Coin => ui.coin_records.push(record),
                RecordKind::Hidden => ui.hidden_records.push(record),
            }
        }
    }
    let split = |s: &str| -> Vec<String> {
        s.split(';')
            .filter(|a| !a.is_empty())
            .map(str::to_owned)
            .collect()
    };
    Vendor {
        records,
        extended_inventory: false,
        all_products_free: ui.all_products_free,
        ui,
        ads: split(&dm.product_ads),
        slogans: split(&dm.product_slogans),
        vend_reply: dm.vend_reply.clone(),
        payment_department: dm.payment_department.clone(),
        onstation: dm.onstation,
        age_restrictions: dm.age_restrictions,
        req_access: dm.req_access.clone(),
        refill_canister: dm.refill_canister.clone(),
        tiltable: dm.tiltable,
        squish_damage: dm.squish_damage,
        crit_chance: dm.crit_chance,
        credits_contained: 0,
        emagged: false,
        last_slogan: 0.0,
        last_shopper: None,
    }
}

/// The record's line in the static data (its price).
fn ui_record_mut(vendor: &mut Vendor, reference: u32) -> Option<&mut ProductRecord> {
    vendor
        .ui
        .product_records
        .iter_mut()
        .chain(vendor.ui.coin_records.iter_mut())
        .chain(vendor.ui.hidden_records.iter_mut())
        .find(|r| r.reference == reference)
}

/// tg `reset_prices` with `inflation_value`: every station vendor's prices (custom vendors and
/// vendors off the station keep theirs), the open windows' static data again.
// shortcut: the inflated price is `round(price * inflation)`, where tg inflates `custom_price`
// and `extra_price` separately before adding them (at most 1 credit apart).
pub fn update_prices(world: &mut World) {
    let inflation = world
        .get_resource::<VendingInflation>()
        .map_or(1.0, |i| i.0);
    let mut query = world.query_filtered::<(Entity, &mut Vendor), Without<custom::CustomVendor>>();
    let mut changed = Vec::new();
    for (entity, mut vendor) in query.iter_mut(world) {
        if !vendor.onstation {
            continue;
        }
        for i in 0..vendor.records.len() {
            let price = (vendor.records[i].base_price as f32 * inflation).round() as u32;
            vendor.records[i].price = price;
            if let Some(r) = ui_record_mut(&mut vendor, i as u32) {
                r.price = price;
            }
        }
        changed.push(entity);
    }
    refresh_static(world, &changed);
}

/// tg `update_static_data_for_all_viewers`.
pub(crate) fn refresh_static(world: &mut World, vendors: &[Entity]) {
    let mut windows = world.query::<&mut UiWindow>();
    for mut window in windows.iter_mut(world) {
        if window.interface == VendingWindow::NAME && vendors.contains(&window.source) {
            window.refresh_static();
        }
    }
}

/// tg `speak`/`say`: only a working vendor with its speaker on talks.
pub fn speak(world: &mut World, vendor: Entity, text: &str) {
    let silenced = world
        .get::<crate::wires::VendorHack>(vendor)
        .is_some_and(|h| h.shut_up);
    if text.is_empty() || silenced || !operational(world, vendor) {
        return;
    }
    crate::atmos_machinery::machine_say(world, vendor, text);
}

/// tg `process`: a slogan once `slogan_delay` passed, at `SPT_PROB(2.5)` per machine tick.
fn process(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &Vendor)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, v)| !v.slogans.is_empty() && v.last_slogan + SLOGAN_DELAY <= now)
        .map(|(e, _)| e)
        .collect();
    for vendor in due {
        let silenced = world
            .get::<crate::wires::VendorHack>(vendor)
            .is_some_and(|h| h.shut_up);
        if silenced || !operational(world, vendor) {
            continue;
        }
        let chance = 100.0 * (1.0 - (1.0 - SLOGAN_PROB / 100.0).powf(MACHINE_TICK as f32));
        if !world.resource_mut::<CombatRng>().chance(chance) {
            continue;
        }
        let count = world.get::<Vendor>(vendor).map_or(0, |v| v.slogans.len());
        let pick = world.resource_mut::<CombatRng>().range(0, count as u32 - 1) as usize;
        let slogan = world.get::<Vendor>(vendor).expect("checked").slogans[pick].clone();
        speak(world, vendor, &slogan);
        world
            .get_mut::<Vendor>(vendor)
            .expect("checked")
            .last_slogan = now;
    }
}

/// tg `attack_hand` -> `interact` -> `ui_interact`: an empty active hand rights a tilted vendor
/// or opens the window (with an item, tg calls `attackby` instead); without power or broken
/// `can_interact` fails and nothing opens.
fn open_on_click(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let pending: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .cloned()
        .collect();
    for click in pending {
        let target = click.target;
        if world.get::<Vendor>(target).is_none() {
            continue;
        }
        let empty_hand = world
            .get::<Inventory>(click.mob)
            .is_some_and(|i| i.active_item().is_none());
        // An electrified vendor shocks first, an open panel takes the hand
        // (`INTERACT_MACHINE_WIRES_IF_OPEN`): both in `crate::wires`.
        let panel = world
            .get::<crate::construction::PanelOpen>(target)
            .is_some_and(|p| p.0);
        if !empty_hand || panel || world.get::<crate::wires::Electrified>(target).is_some() {
            continue;
        }
        if tilt::is_tilted(world, target) {
            tilt::start_righting(world, click.client, click.mob, target);
            continue;
        }
        if !operational(world, target) {
            continue;
        }
        if !custom::may_open(world, click.client, target) {
            continue;
        }
        let title = name_of(world, target);
        world.write_message(OpenUi::new::<VendingWindow>(
            click.client,
            click.mob,
            target,
            &title,
        ));
    }
}

/// `ui_act("vend")`, one exclusive pass so each act sees the previous one's stock and hands.
fn apply_vends(world: &mut World, mut acts: Local<MessageCursor<UiActed<VendingWindow>>>) {
    let pending: Vec<(Entity, Entity, u32)> = acts
        .read(world.resource::<Messages<UiActed<VendingWindow>>>())
        .map(|a| {
            let VendingAct::Vend { reference } = a.act;
            (a.actor, a.source, reference)
        })
        .collect();
    for (actor, vendor, reference) in pending {
        let result = if world.get::<custom::CustomVendor>(vendor).is_some() {
            custom::vend(world, actor, vendor, reference)
        } else {
            vend(world, actor, vendor, reference)
        };
        match result {
            Ok(vended) => {
                world.write_message(vended);
            }
            Err(reason) => {
                debug!("vend {reference} on {vendor} by {actor} rejected: {reason:?}");
                world.write_message(VendRejected {
                    actor,
                    vendor,
                    reason,
                });
            }
        }
    }
}

/// The client of a player's mob.
pub(crate) fn client_of(world: &World, mob: Entity) -> Option<Entity> {
    world.get::<crate::server::PlayerMob>(mob).map(|p| p.client)
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = client_of(world, mob) {
        tell(world, client, span, text);
    }
}

/// Checks shared by `vend` and the custom vendor's: actor, vendor, reach, power.
pub(crate) fn vend_preconditions(
    world: &World,
    actor: Entity,
    vendor: Entity,
) -> Result<(TilePos, Inventory), VendReject> {
    let actor_ref = world.get_entity(actor).map_err(|_| VendReject::NoActor)?;
    let (Some(&actor_pos), Some(inventory), true) = (
        actor_ref.get::<TilePos>(),
        actor_ref.get::<Inventory>().cloned(),
        actor_ref.contains::<Mover>(),
    ) else {
        return Err(VendReject::NoActor);
    };
    let vendor_ref = world.get_entity(vendor).map_err(|_| VendReject::NoVendor)?;
    let (Some(&vendor_pos), true) = (vendor_ref.get::<TilePos>(), vendor_ref.contains::<Vendor>())
    else {
        return Err(VendReject::NoVendor);
    };
    if distance_status(actor_pos, vendor_pos) != UiStatus::Interactive {
        return Err(VendReject::TooFar);
    }
    if !operational(world, vendor) {
        return Err(VendReject::NoPower);
    }
    Ok((vendor_pos, inventory))
}

/// tg `discount_check`: not for premium or contraband; the captain everywhere, everyone else at
/// their own department's vendors.
pub fn discounted(
    premium: bool,
    captain: bool,
    account_department: &str,
    vendor_department: &str,
) -> bool {
    !premium && (captain || account_department == vendor_department)
}

/// `max(round(price * DEPARTMENT_DISCOUNT), 1)`.
pub fn discount_price(price: u32) -> u32 {
    ((price as f32 * DEPARTMENT_DISCOUNT).round() as u32).max(1)
}

/// tg `vend` + `proceed_payment` + `dispense`: every check against the live world, then the
/// payment, then one item out.
pub fn vend(
    world: &mut World,
    actor: Entity,
    vendor: Entity,
    reference: u32,
) -> Result<Vended, VendReject> {
    let (vendor_pos, inventory) = vend_preconditions(world, actor, vendor)?;
    let machine = world.get::<Vendor>(vendor).expect("checked above");
    let record = machine
        .sellable(reference)
        .ok_or(VendReject::NotSold)?
        .clone();
    let product = capitalize(&ui_name(machine, reference, &record.path));
    if record.amount == 0 {
        speak(world, vendor, &format!("{product} закончился."));
        return Err(VendReject::SoldOut);
    }
    let free = machine.all_products_free;
    let returned = !record.returned.is_empty();
    let mut paid = 0;
    if !free {
        let Some(card) = inventory.id_card(world) else {
            speak(
                world,
                vendor,
                &format!("У вас нет ID-карты, чтобы купить {product}."),
            );
            return Err(VendReject::NoId);
        };
        let age = world
            .get::<crate::id_access::CardAccess>(card)
            .map_or(30, |c| c.registered_age);
        let machine = world.get::<Vendor>(vendor).expect("checked above");
        if machine.age_restrictions && record.age_restricted && age < AGE_MINOR {
            speak(
                world,
                vendor,
                &format!("Вы недостаточно взрослы, чтобы купить {product}."),
            );
            return Err(VendReject::Underage);
        }
        // `proceed_payment`: returned items are free.
        if !returned {
            paid = pay(world, actor, vendor, card, &record, &product)?;
        }
    }
    reply(world, actor, vendor);
    let item = dispense(
        world,
        vendor,
        reference as usize,
        vendor_pos,
        false,
        returned,
    )
    .ok_or(VendReject::NoPrototype)?;
    let in_hand = give(world, actor, &inventory, item, &product, vendor_pos);
    Ok(Vended {
        actor,
        vendor,
        item,
        path: record.path,
        in_hand,
        paid,
    })
}

/// The record's display name (its static data line).
fn ui_name(vendor: &Vendor, reference: u32, path: &str) -> String {
    vendor
        .ui
        .product_records
        .iter()
        .chain(&vendor.ui.coin_records)
        .chain(&vendor.ui.hidden_records)
        .find(|r| r.reference == reference)
        .map_or_else(
            || ss13_mapload::convert::default_name(path),
            |r| r.name.clone(),
        )
}

/// `proceed_payment` + the payment component's `handle_card`: the price, after the department
/// discount, from the card's account into the vendor's department budget.
fn pay(
    world: &mut World,
    actor: Entity,
    vendor: Entity,
    card: Entity,
    record: &VendRecord,
    product: &str,
) -> Result<u32, VendReject> {
    let machine = world.get::<Vendor>(vendor).expect("checked by the caller");
    let department = machine.payment_department.clone();
    let mut price = record.price;
    let account_id = world.get::<BankAccountId>(card).map(|a| a.0);
    let captain = world
        .get::<crate::id_access::CardAccess>(card)
        .and_then(|c| c.trim.as_ref())
        .is_some_and(|t| t.job == CAPTAIN);
    let account_department = account_id
        .and_then(|id| world.get_resource::<Economy>()?.account(id))
        .map(|a| a.paycheck_department.clone());
    if let Some(dep) = &account_department
        && discounted(
            record.kind != RecordKind::Product,
            captain,
            dep,
            &department,
        )
    {
        price = discount_price(price);
    }
    // `attempt_charge`: nothing to charge for a free product.
    if price == 0 {
        return Ok(0);
    }
    let Some(account_id) = account_id.filter(|_| account_department.is_some()) else {
        tell_mob(world, actor, Span::Warning, "Аккаунт не найден.");
        return Err(VendReject::NoAccount);
    };
    let charged = world
        .resource_mut::<Economy>()
        .account_mut(account_id)
        .is_some_and(|a| a.adjust_money(-i64::from(price)));
    if !charged {
        tell_mob(
            world,
            actor,
            Span::Warning,
            "У вас недостаточно средств для этой покупки.",
        );
        speak(
            world,
            vendor,
            &format!("У вас недостаточно средств, чтобы купить {product}."),
        );
        return Err(VendReject::NoFunds);
    }
    let mut economy = world.resource_mut::<Economy>();
    // `get_dep_account(payment_department)`, CIV for an unknown one.
    let budget = if economy.department(&department).is_some() {
        department
    } else {
        ACCOUNT_CIV.to_owned()
    };
    if let Some(budget) = economy.department_mut(&budget) {
        budget.adjust_money(i64::from(price));
    }
    bank_card_talk(
        world,
        account_id,
        &format!("С вашего счёта списано {price}{MONEY_SYMBOL}."),
    );
    if let Some(&pos) = world.get::<TilePos>(vendor) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, CASH_SOUND, 20.0, true),
        );
    }
    if let Some(mut machine) = world.get_mut::<Vendor>(vendor) {
        machine.credits_contained += (price as f32 * CREDITS_COLLECTION).round() as i64;
    }
    Ok(price)
}

/// The vend reply, once per shopper per 5 seconds.
fn reply(world: &mut World, actor: Entity, vendor: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let machine = world.get::<Vendor>(vendor).expect("checked by the caller");
    if machine
        .last_shopper
        .is_some_and(|(who, until)| who == actor && now < until)
    {
        return;
    }
    let text = if machine.vend_reply.is_empty() {
        format!("Благодарим за использование {}!", name_of(world, vendor))
    } else {
        machine.vend_reply.clone()
    };
    world
        .get_mut::<Vendor>(vendor)
        .expect("checked")
        .last_shopper = Some((actor, now + REPLY_COOLDOWN));
    speak(world, vendor, &text);
}

/// tg `dispense`: the last returned item with `returned` (`dispense_returned`), else a new one;
/// one out of the stock. The item is off the map (no [`TilePos`]) for the caller to place.
pub(crate) fn dispense(
    world: &mut World,
    vendor: Entity,
    record: usize,
    vendor_pos: TilePos,
    silent: bool,
    returned: bool,
) -> Option<Entity> {
    prune_returned(world, vendor, record);
    let machine = world.get::<Vendor>(vendor)?;
    let r = machine.records.get(record)?;
    if r.amount == 0 {
        return None;
    }
    let new_stock = r.amount as usize > r.returned.len();
    let item = if let Some(&item) = r.returned.last().filter(|_| returned || !new_stock) {
        world.entity_mut(item).remove::<crate::storage::StoredIn>();
        world.get_mut::<Vendor>(vendor)?.records[record]
            .returned
            .pop();
        item
    } else {
        let path = r.path.clone();
        add_templates(world, [path.as_str()]);
        let item = world.resource_scope(|world, templates: Mut<ProductTemplates>| {
            templates.0.get(&path).map(|t| t.spawn(world, None))
        })?;
        crate::storage::finish_item(world, item, 0);
        item
    };
    world.get_mut::<Vendor>(vendor)?.records[record].amount -= 1;
    if !silent {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(vendor_pos, VEND_SOUND, 50.0, true).extrarange(-3.0),
        );
    }
    Some(item)
}

/// tg `Exited`: a returned item that is no longer inside (gone some other way) leaves the
/// record and its stock.
fn prune_returned(world: &mut World, vendor: Entity, record: usize) {
    let Some(r) = world
        .get::<Vendor>(vendor)
        .and_then(|v| v.records.get(record))
    else {
        return;
    };
    let inside = |e: Entity| {
        world
            .get::<crate::storage::StoredIn>(e)
            .is_some_and(|s| s.0 == vendor)
    };
    if r.returned.iter().all(|&e| inside(e)) {
        return;
    }
    let kept: Vec<Entity> = r.returned.iter().copied().filter(|&e| inside(e)).collect();
    let lost = (r.returned.len() - kept.len()) as u32;
    let mut machine = world.get_mut::<Vendor>(vendor).expect("checked above");
    let r = &mut machine.records[record];
    r.returned = kept;
    r.amount = r.amount.saturating_sub(lost);
}

/// tg `put_in_hands` of a vended item: the active hand, else the other one; else it falls on
/// the vendor's tile. True when it went into a hand.
fn give(
    world: &mut World,
    actor: Entity,
    inventory: &Inventory,
    item: Entity,
    product: &str,
    vendor_pos: TilePos,
) -> bool {
    let hand = [inventory.active, 1 - inventory.active]
        .into_iter()
        .find(|&h| inventory.hands[h].is_none());
    let Some(hand) = hand else {
        world.entity_mut(item).insert((Networked, vendor_pos));
        tell_mob(
            world,
            actor,
            Span::Warning,
            format!("{product} падает на пол!"),
        );
        return false;
    };
    world
        .entity_mut(item)
        .remove::<TilePos>()
        .insert((Networked, CarriedBy(actor)));
    world
        .get_mut::<Inventory>(actor)
        .expect("checked by the caller")
        .hands[hand] = Some(item);
    tell_mob(
        world,
        actor,
        Span::Notice,
        format!("Вы забираете {product} из лотка."),
    );
    true
}

/// tg `compartmentLoadAccessCheck`: no `req_access`, the access, an emag, or a cut ID scan wire.
pub fn may_load(world: &World, vendor: Entity, user: Entity) -> bool {
    let Some(machine) = world.get::<Vendor>(vendor) else {
        return false;
    };
    if world.get::<custom::CustomVendor>(vendor).is_some() {
        return custom::is_owner(world, vendor, user);
    }
    if machine.req_access.is_empty() || machine.emagged {
        return true;
    }
    if world
        .get::<crate::wires::VendorHack>(vendor)
        .is_some_and(|h| !h.scan_id)
    {
        return true;
    }
    let access = world
        .get::<Inventory>(user)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<crate::inventory::IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default();
    crate::inventory::check_access(&machine.req_access, &[], &access)
}

/// tg `canLoadItem`: an empty item of one of the vendor's types.
fn can_load(world: &World, vendor: &Vendor, item: Entity) -> bool {
    let Some(path) = world.get::<ss13_mapload::DmItem>(item).map(|i| &i.path) else {
        return false;
    };
    let empty = world
        .get::<crate::storage::Stored>(item)
        .is_none_or(|s| s.items().is_empty());
    empty && vendor.records.iter().any(|r| &r.path == path)
}

/// Why an item did not go back in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadRefusal {
    NotAccepted,
    Full,
}

/// tg `loadingAttempt`: `item` into the first record of its type with room, as a returned item.
pub fn load_item(world: &mut World, vendor: Entity, item: Entity) -> Result<(), LoadRefusal> {
    let machine = world
        .get::<Vendor>(vendor)
        .ok_or(LoadRefusal::NotAccepted)?;
    if !can_load(world, machine, item) {
        return Err(LoadRefusal::NotAccepted);
    }
    let path = world
        .get::<ss13_mapload::DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let index = machine
        .records
        .iter()
        .position(|r| r.path == path)
        .expect("can_load found it");
    if machine.records[index].amount >= machine.records[index].max_amount {
        return Err(LoadRefusal::Full);
    }
    crate::storage::put_inside(world, item, vendor);
    let mut machine = world.get_mut::<Vendor>(vendor).expect("checked above");
    let record = &mut machine.records[index];
    record.amount += 1;
    record.returned.push(item);
    Ok(())
}

/// tg `/obj/machinery/vending/item_interaction` (after the tool acts and the wires): a refill
/// canister restocks, an emag unlocks, a custom vendor takes IDs; with access and out of
/// combat mode, items of its types go back in (a storage bag empties its fitting items).
/// True when the click was handled (tg `ITEM_INTERACT_SUCCESS`/`FAILURE`); false lets it
/// become an attack.
pub fn item_interaction(world: &mut World, actor: Actor, held: Entity, vendor: Entity) -> bool {
    if world.get::<Vendor>(vendor).is_none() {
        return false;
    }
    let client = actor.client;
    let path = world
        .get::<ss13_mapload::DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if is_under(&path, EMAG) {
        emag_act(world, client, vendor);
        return true;
    }
    if custom::item_interaction(world, actor, held, vendor) {
        return true;
    }
    if refill::canister_interaction(world, actor, held, vendor) {
        return true;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(actor.mob)
        .is_some_and(|c| c.0);
    if combat || !may_load(world, vendor, actor.mob) {
        return false;
    }
    if !operational(world, vendor) {
        tell(
            world,
            client,
            Span::Warning,
            format!("{} не отвечает.", name_of(world, vendor)),
        );
        return true;
    }
    let name = name_of(world, vendor);
    if is_under(&path, STORAGE_BAG) {
        let items: Vec<Entity> = world
            .get::<crate::storage::Stored>(held)
            .map(|s| s.items().to_vec())
            .unwrap_or_default();
        let (mut loaded, mut denied) = (0, 0);
        for item in items {
            match load_item(world, vendor, item) {
                Ok(()) => loaded += 1,
                Err(_) => denied += 1,
            }
        }
        if denied > 0 {
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} отказывается принимать некоторые предметы!"),
            );
        }
        if loaded > 0 {
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы загружаете {loaded} предметов в отсек {name}."),
            );
        }
        return true;
    }
    if crate::inventory::no_drop(world, held) {
        tell(
            world,
            client,
            Span::Warning,
            format!("{} прилип к руке!", name_of(world, held)),
        );
        return true;
    }
    let item = name_of(world, held);
    let text = match load_item(world, vendor, held) {
        Ok(()) => (
            Span::Notice,
            format!("Вы кладёте {item} в приёмный отсек {name}."),
        ),
        Err(LoadRefusal::Full) => (
            Span::Warning,
            format!("{name} больше не может принять {item}!"),
        ),
        Err(LoadRefusal::NotAccepted) => (Span::Warning, format!("{name} не принимает {item}!")),
    };
    tell(world, client, text.0, text.1);
    true
}

/// tg `emag_act`: the product lock is off (anyone may load items back).
fn emag_act(world: &mut World, client: Entity, vendor: Entity) {
    let Some(mut machine) = world.get_mut::<Vendor>(vendor) else {
        return;
    };
    if machine.emagged {
        return;
    }
    machine.emagged = true;
    tell(world, client, Span::Notice, "блокировка товаров отключена");
}

/// The end of a [`VendJob`].
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: VendJob) {
    match job {
        VendJob::Right => tilt::finish_righting(world, mob, action),
    }
}

/// tg `atom_break` of a vendor: dark and broken, up to 16 items spill out (returned ones first;
/// most of the rest is destroyed).
pub fn atom_break(world: &mut World, vendor: Entity) {
    let Some(&pos) = world.get::<TilePos>(vendor) else {
        return;
    };
    if let Some(mut integrity) = world.get_mut::<Integrity>(vendor) {
        integrity.broken = true;
    }
    refresh_look(world, vendor);
    let Some(machine) = world.get::<Vendor>(vendor) else {
        return;
    };
    let count = machine.records.len();
    let mut dumped = 0;
    let mut found_anything = true;
    while found_anything {
        found_anything = false;
        let mut order: Vec<usize> = (0..count)
            .filter(|&i| {
                world.get::<Vendor>(vendor).expect("checked").records[i].kind == RecordKind::Product
            })
            .collect();
        shuffle(world, &mut order);
        for i in order {
            // Returned items first, "in case they contain the nuke disk or something".
            while world.get::<Vendor>(vendor).expect("checked").records[i]
                .returned
                .last()
                .is_some()
            {
                if let Some(item) = dispense(world, vendor, i, pos, true, true) {
                    scatter(world, item, pos);
                }
            }
            if world.get::<Vendor>(vendor).expect("checked").records[i].amount == 0 {
                continue;
            }
            if found_anything && world.resource_mut::<CombatRng>().chance(80.0) {
                world.get_mut::<Vendor>(vendor).expect("checked").records[i].amount -= 1;
                continue;
            }
            if let Some(item) = dispense(world, vendor, i, pos, true, false) {
                scatter(world, item, pos);
            }
            found_anything = true;
            dumped += 1;
            if dumped >= 16 {
                return;
            }
        }
    }
}

/// `step(item, pick(GLOB.alldirs))`: onto the tile, then one step if the way is free.
fn scatter(world: &mut World, item: Entity, pos: TilePos) {
    world.entity_mut(item).insert((Networked, pos));
    let dirs = ss13_shared::Dir::DMI_ORDER;
    let dir = dirs[world
        .resource_mut::<CombatRng>()
        .range(0, dirs.len() as u32 - 1) as usize];
    let to = pos.step(dir);
    if !crate::construction::tile_blocked(world, to) {
        world.entity_mut(item).insert(to);
    }
}

pub(crate) fn shuffle(world: &mut World, list: &mut [usize]) {
    let mut rng = world.resource_mut::<CombatRng>();
    for i in (1..list.len()).rev() {
        let j = rng.range(0, i as u32) as usize;
        list.swap(i, j);
    }
}

/// tg `/obj/machinery/vending/examine` (vendors with a canister type): the panel, the stock,
/// the collected credits; a custom vendor's lock.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(machine) = world.get::<Vendor>(target) else {
        return;
    };
    if world
        .get::<custom::CustomVendor>(target)
        .is_some_and(|c| c.linked.is_some())
    {
        lines.push((
            Span::Warning,
            "Автомат привязан к ID. Отвяжите счёт перед разборкой.".into(),
        ));
    }
    if machine.refill_canister.is_empty() {
        return;
    }
    let open = world
        .get::<crate::construction::PanelOpen>(target)
        .is_some_and(|p| p.0);
    let panel = if open {
        "Панель обслуживания можно закрутить отвёрткой; автомат можно разобрать ломом."
    } else {
        "Панель обслуживания можно открутить отвёрткой."
    };
    lines.push((Span::Notice, panel.into()));
    let (loaded, max) = machine.total_stock();
    if max > 0 {
        let text = if loaded < max {
            let canister = world
                .get_resource::<ss13_proto::Prototypes>()
                .and_then(|p| p.get(&machine.refill_canister))
                .and_then(|p| p.get::<ss13_mapload::DmVendingRefill>())
                .map_or_else(String::new, |r| plain_name(&r.machine_name).to_owned());
            format!("Его можно пополнить канистрой «{canister}» при открытой панели.")
        } else {
            "Автомат полностью заполнен.".to_owned()
        };
        lines.push((Span::Notice, text));
    }
    // `CREDITS_DUMP_THRESHOLD` and `PAYCHECK_CREW`, both 50.
    let credits = machine.credits_contained;
    if credits > 0 && credits < 50 {
        lines.push((
            Span::Notice,
            "Внутри должна быть горстка кредитов за проданные товары.".into(),
        ));
    } else if credits > 50 {
        lines.push((
            Span::Notice,
            "Внутри должно быть не меньше целой зарплаты!".into(),
        ));
    }
}

/// Before a vendor is gone (`deconstruct`): its returned items onto the floor, its stock into
/// its refill canister ([`refill::on_deconstruction`]).
pub fn on_deconstruction(world: &mut World, vendor: Entity, pos: TilePos, disassembled: bool) {
    if world.get::<Vendor>(vendor).is_none() {
        return;
    }
    refill::on_deconstruction(world, vendor, pos, disassembled);
    let returned: Vec<Entity> = world
        .get::<Vendor>(vendor)
        .map(|v| v.records.iter().flat_map(|r| r.returned.clone()).collect())
        .unwrap_or_default();
    for item in returned {
        crate::storage::take_out(world, item, pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_mapload::{VendCategory, VendProduct};

    fn dm() -> DmVending {
        let product = |path: &str, amount, price, category| VendProduct {
            path: path.into(),
            amount,
            price,
            category,
            age_restricted: false,
        };
        DmVending {
            products: vec![product("/obj/item/a", 2, 30, 0)],
            contraband: vec![product("/obj/item/c", 1, 50, 1)],
            premium: vec![product("/obj/item/p", 1, 70, 2)],
            categories: vec![
                VendCategory {
                    name: "Products".into(),
                    icon: "cart-shopping".into(),
                },
                VendCategory {
                    name: "Contraband".into(),
                    icon: "mask".into(),
                },
                VendCategory {
                    name: "Premium".into(),
                    icon: "coins".into(),
                },
            ],
            product_ads: "a;b".into(),
            product_slogans: "s1;s2".into(),
            age_restrictions: true,
            onstation: true,
            ..default()
        }
    }

    /// No prototypes: `vend` gets as far as spawning, which is where `NoPrototype` stops it.
    fn world() -> (World, Entity, Entity) {
        let mut world = World::new();
        world.init_resource::<ProductTemplates>();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(CombatRng(1));
        let card = world
            .spawn(crate::inventory::IdCard { access: vec![] })
            .id();
        let actor = world
            .spawn((
                TilePos::new(5, 6, 1),
                Mover::new(0.2),
                Inventory {
                    id: Some(card),
                    ..default()
                },
            ))
            .id();
        let mut machine = vendor(&dm(), &HashMap::new());
        machine.all_products_free = true;
        let vendor = world.spawn((TilePos::new(5, 5, 1), machine)).id();
        (world, actor, vendor)
    }

    #[test]
    fn records_and_categories_follow_ui_static_data() {
        let v = vendor(&dm(), &HashMap::new());
        let refs: Vec<(u32, RecordKind)> = v
            .records
            .iter()
            .enumerate()
            .map(|(i, r)| (i as u32, r.kind))
            .collect();
        assert_eq!(
            refs,
            [
                (0, RecordKind::Product),
                (1, RecordKind::Coin),
                (2, RecordKind::Hidden)
            ]
        );
        let names: Vec<&str> = v.ui.categories.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Products", "Premium", "Contraband"]);
        assert_eq!(v.ui.product_records[0].path, "a");
        assert_eq!(v.ui.product_records[0].name, "A");
        assert!(v.ui.coin_records[0].premium);
        assert_eq!(v.ui.displayed_currency_name, " ¢");
        assert_eq!(v.ads, ["a", "b"]);
        assert_eq!(v.slogans, ["s1", "s2"]);
        assert_eq!(v.total_stock(), (4, 4));
    }

    #[test]
    fn vend_checks_reach_record_and_stock() {
        let (mut world, actor, vendor) = world();
        // In reach and in stock: only the missing prototype stops it.
        assert_eq!(
            vend(&mut world, actor, vendor, 0).err(),
            Some(VendReject::NoPrototype)
        );
        // Not a record of this vendor; contraband of an unhacked vendor.
        assert_eq!(
            vend(&mut world, actor, vendor, 7).err(),
            Some(VendReject::NotSold)
        );
        assert_eq!(
            vend(&mut world, actor, vendor, 2).err(),
            Some(VendReject::NotSold)
        );
        world.get_mut::<Vendor>(vendor).unwrap().extended_inventory = true;
        assert_eq!(
            vend(&mut world, actor, vendor, 2).err(),
            Some(VendReject::NoPrototype)
        );
        // Out of stock.
        world.get_mut::<Vendor>(vendor).unwrap().records[0].amount = 0;
        assert_eq!(
            vend(&mut world, actor, vendor, 0).err(),
            Some(VendReject::SoldOut)
        );
        // Two tiles away: not interactive any more.
        world.entity_mut(actor).insert(TilePos::new(5, 7, 1));
        assert_eq!(
            vend(&mut world, actor, vendor, 1).err(),
            Some(VendReject::TooFar)
        );
        // Without an ID a paid vendor sells nothing.
        world
            .entity_mut(actor)
            .insert((TilePos::new(5, 6, 1), Inventory::default()));
        world.get_mut::<Vendor>(vendor).unwrap().all_products_free = false;
        assert_eq!(
            vend(&mut world, actor, vendor, 1).err(),
            Some(VendReject::NoId)
        );
    }

    /// tg `discount_check`: own department or the captain, never premium; at least 1 credit.
    #[test]
    fn discounts_follow_discount_check() {
        assert!(discounted(false, false, "SRV", "SRV"));
        assert!(!discounted(false, false, "CIV", "SRV"));
        assert!(discounted(false, true, "CIV", "SRV"));
        assert!(!discounted(true, true, "SRV", "SRV"));
        assert_eq!(discount_price(30), 6);
        assert_eq!(discount_price(2), 1);
        assert_eq!(discount_price(13), 3);
    }

    #[test]
    fn vend_param_must_be_an_index() {
        let params = [("ref".to_owned(), "x".to_owned())];
        let parsed = VendingWindow::parse_act("vend", &ss13_tgui::server::UiParams::new(&params));
        assert!(matches!(parsed, Err(UiReject::BadParams(_))));
    }
}
