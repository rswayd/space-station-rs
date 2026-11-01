//! Vending machines (server), after tg `code/modules/vending/vendor/`: a click with an empty
//! active hand on an adjacent vendor opens the `Vending` window (`ss13_tgui::interfaces::vending`
//! has the data shapes and the client view); `vend` hands out one item.
//!
//! Records come from the converter's [`DmVending`] (products, contraband, premium, prices) and
//! live in [`Vendor`]; the stock is per machine. `vend` is checked at the moment of effect: the
//! window is interactive this tick (tgui distance rules) and the actor is still within one tile,
//! the record exists in this vendor and is sellable (contraband only with `extended_inventory`),
//! it is in stock, the vendor has power, and, unless the vendor is free, the actor carries an ID.
//! tg has no vend
//! cooldown any more (`vend_ready` was removed); repeated acts are bounded by tgui's act rate
//! limit and the stock.
//!
//! Power ([`Unpowered`], tg `machine_stat & NOPOWER`): `can_interact` fails, so a click opens
//! nothing and an open window drops to update-only (`/atom/ui_status`); the sprite switches to
//! `<icon_state>-off` without the `light_mask` emissive (`update_icon_state`, `update_overlays`).

use std::collections::HashMap;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmName, DmVending, MapLoadPlugin, Template, read_manifest};
use ss13_net::server::Networked;
use ss13_shared::{Appearance, EMISSIVE_PLANE, IconStateId, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::UiStatus;
use ss13_tgui::interfaces::vending::{
    ProductIcon, ProductRecord, StockItem, UserData, VendingData, VendingStatic, VendingUi,
    sanitized_path,
};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, distance_status,
};
use ss13_world::{Mover, WorldSystems};

use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::{CarriedBy, Inventory};
use crate::power::Unpowered;

/// `DEPARTMENT_DISCOUNT`.
pub const DEPARTMENT_DISCOUNT: f32 = 0.2;
/// `MONEY_SYMBOL`; tg sends it with a leading space.
const MONEY_SYMBOL: &str = "¢";

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
    pub amount: u32,
    pub max_amount: u32,
    pub kind: RecordKind,
}

/// A vending machine's live state. Record index = the `ref` the window sends back.
#[derive(Component, Clone, Debug)]
pub struct Vendor {
    pub records: Vec<VendRecord>,
    pub extended_inventory: bool,
    pub all_products_free: bool,
    /// `ui_static_data` without the ad.
    pub ui: VendingStatic,
    pub ads: Vec<String>,
}

impl Vendor {
    /// The record `vend` may sell: in this vendor, contraband only when hacked.
    pub fn sellable(&self, reference: u32) -> Option<&VendRecord> {
        let record = self.records.get(reference as usize)?;
        (record.kind != RecordKind::Hidden || self.extended_inventory).then_some(record)
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
    /// [`Unpowered`] (tg: `can_interact` fails, the act never reaches `vend`).
    NoPower,
    /// The product type has no prototype (rerun the converter).
    NoPrototype,
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
    /// `vend` with `ref`; `premium` only steers tg's payment, which does not exist yet.
    Vend { reference: u32 },
}

/// A vendor's sprite with and without power: icon state and overlays.
#[derive(Component, Clone, Debug)]
struct PowerLooks {
    powered: (IconStateId, Vec<Overlay>),
    unpowered: (IconStateId, Vec<Overlay>),
}

/// Prototypes of everything the map's vendors sell, built once at startup.
#[derive(Resource, Default)]
pub struct ProductTemplates(pub HashMap<String, Template>);

type UserQueries = (
    Query<'static, 'static, &'static Name>,
    Query<'static, 'static, &'static Inventory>,
    crate::inventory::IdCards<'static, 'static>,
);

impl ServerUi for VendingWindow {
    type Source = &'static Vendor;
    type UserParam = UserQueries;
    type Act = VendingAct;

    fn static_data(vendor: &Vendor) -> VendingStatic {
        let mut data = vendor.ui.clone();
        // tg picks a random ad per `ui_static_data`.
        if !vendor.ads.is_empty() {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos() as usize);
            data.ad = vendor.ads[nanos % vendor.ads.len()].clone();
        }
        data
    }

    fn data(
        vendor: &Vendor,
        user: Entity,
        (names, inventories, cards): &SystemParamItem<'_, '_, UserQueries>,
    ) -> VendingData {
        // tg `get_idcard(TRUE)`: hands first, then the ID slot.
        let has_id = inventories
            .get(user)
            .is_ok_and(|inventory| inventory.access(cards).is_some());
        // shortcut: no bank accounts or jobs yet: the holder is the mob's name, every player is
        // an assistant (`paycheck_department` ACCOUNT_CIV), and there is no balance.
        let user = has_id.then(|| UserData {
            name: names.get(user).map(|n| n.to_string()).unwrap_or_default(),
            cash: None,
            job: "Assistant".into(),
            department: "CIV".into(),
            is_captain_card: false,
        });
        VendingData {
            user,
            stock: vendor
                .records
                .iter()
                .map(|r| StockItem {
                    amount: r.amount,
                    free: false,
                })
                .collect(),
            extended_inventory: vendor.extended_inventory,
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
                ),
            )
            .add_observer(power_lost)
            .add_observer(power_back);
    }
}

/// tg `/atom/ui_status`: a machine that `can_interact` refuses (no power) is update-only.
fn unpowered_windows(
    mut windows: Query<&mut ss13_tgui::server::UiWindow>,
    unpowered: Query<(), (With<Vendor>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == VendingWindow::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

fn set_look(appearance: &mut Appearance, (icon_state, overlays): &(IconStateId, Vec<Overlay>)) {
    appearance.icon_state = *icon_state;
    appearance.overlays.clone_from(overlays);
}

fn power_lost(add: On<Add<Unpowered>>, mut vendors: Query<(&PowerLooks, &mut Appearance)>) {
    if let Ok((looks, mut appearance)) = vendors.get_mut(add.entity) {
        set_look(&mut appearance, &looks.unpowered);
    }
}

fn power_back(remove: On<Remove<Unpowered>>, mut vendors: Query<(&PowerLooks, &mut Appearance)>) {
    if let Ok((looks, mut appearance)) = vendors.get_mut(remove.entity) {
        set_look(&mut appearance, &looks.powered);
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
        vendors.push((entity, vendor(&dm, &templates)));
        let looks = world.get::<Appearance>(entity).map(|a| {
            let off = world.get::<DmAtom>(entity).and_then(|atom| {
                let state = format!("{}-off", atom.icon_state);
                index.get(&(atom.icon.as_str(), state.as_str())).copied()
            });
            PowerLooks {
                powered: (a.icon_state, a.overlays.clone()),
                unpowered: (
                    off.unwrap_or(a.icon_state),
                    a.overlays
                        .iter()
                        .filter(|o| o.plane != Some(EMISSIVE_PLANE))
                        .cloned()
                        .collect(),
                ),
            }
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
    world.insert_resource(ProductTemplates(templates));
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
    Vendor {
        records,
        extended_inventory: false,
        all_products_free: ui.all_products_free,
        ui,
        ads: dm
            .product_ads
            .split(';')
            .filter(|a| !a.is_empty())
            .map(str::to_owned)
            .collect(),
    }
}

/// tg `attack_hand` -> `ui_interact`: an empty active hand opens the window (with an item, tg
/// calls `attackby` instead); without power `can_interact` fails and nothing opens.
fn open_on_click(
    mut clicks: MessageReader<ClickResolved>,
    // An open panel takes the hand (tg `INTERACT_MACHINE_WIRES_IF_OPEN`), an electrified
    // vendor shocks first: both in `crate::wires`.
    vendors: Query<
        (&DmName, Option<&crate::construction::PanelOpen>),
        (
            With<Vendor>,
            Without<Unpowered>,
            Without<crate::wires::Electrified>,
        ),
    >,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok((name, panel)) = vendors.get(click.target) else {
            continue;
        };
        if panel.is_some_and(|p| p.0) {
            continue;
        }
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none())
        {
            open.write(OpenUi::new::<VendingWindow>(
                click.client,
                click.mob,
                click.target,
                plain_name(&name.name),
            ));
        }
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
        match vend(world, actor, vendor, reference) {
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

/// tg `vend` + `dispense`: every check against the live world, then one item out.
pub fn vend(
    world: &mut World,
    actor: Entity,
    vendor: Entity,
    reference: u32,
) -> Result<Vended, VendReject> {
    let actor_ref = world.get_entity(actor).map_err(|_| VendReject::NoActor)?;
    let (Some(&actor_pos), Some(inventory), true) = (
        actor_ref.get::<TilePos>(),
        actor_ref.get::<Inventory>().cloned(),
        actor_ref.contains::<Mover>(),
    ) else {
        return Err(VendReject::NoActor);
    };
    let vendor_ref = world.get_entity(vendor).map_err(|_| VendReject::NoVendor)?;
    let (Some(&vendor_pos), Some(machine)) =
        (vendor_ref.get::<TilePos>(), vendor_ref.get::<Vendor>())
    else {
        return Err(VendReject::NoVendor);
    };
    if distance_status(actor_pos, vendor_pos) != UiStatus::Interactive {
        return Err(VendReject::TooFar);
    }
    if vendor_ref.contains::<Unpowered>() {
        return Err(VendReject::NoPower);
    }
    let record = machine.sellable(reference).ok_or(VendReject::NotSold)?;
    if record.amount == 0 {
        return Err(VendReject::SoldOut);
    }
    let path = record.path.clone();
    if !machine.all_products_free {
        let has_id = inventory.id_card(world).is_some();
        if !has_id {
            return Err(VendReject::NoId);
        }
        // shortcut: no economy yet, so `proceed_payment` charges nothing; charge the ID's
        // account once bank accounts exist.
    }
    let item = world.resource_scope(|world, templates: Mut<ProductTemplates>| {
        templates.0.get(&path).map(|t| t.spawn(world, None))
    });
    let item = item.ok_or(VendReject::NoPrototype)?;
    world
        .get_mut::<Vendor>(vendor)
        .expect("checked above")
        .records[reference as usize]
        .amount -= 1;
    crate::storage::finish_item(world, item, 0);
    // tg `put_in_hands`: the active hand, else the other one; else it stays on the floor.
    let hand = [inventory.active, 1 - inventory.active]
        .into_iter()
        .find(|&h| inventory.hands[h].is_none());
    match hand {
        Some(hand) => {
            world.entity_mut(item).insert((Networked, CarriedBy(actor)));
            world
                .get_mut::<Inventory>(actor)
                .expect("checked above")
                .hands[hand] = Some(item);
        }
        None => {
            world.entity_mut(item).insert((Networked, vendor_pos));
        }
    }
    Ok(Vended {
        actor,
        vendor,
        item,
        path,
        in_hand: hand.is_some(),
    })
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
            ..default()
        }
    }

    /// No prototypes: `vend` gets as far as spawning, which is where `NoPrototype` stops it.
    fn world() -> (World, Entity, Entity) {
        let mut world = World::new();
        world.init_resource::<ProductTemplates>();
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
        let vendor = world
            .spawn((TilePos::new(5, 5, 1), vendor(&dm(), &HashMap::new())))
            .id();
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
    }

    #[test]
    fn vend_checks_reach_record_stock_and_id() {
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
        // Without an ID a paid vendor sells nothing; a free one does.
        world
            .entity_mut(actor)
            .insert((TilePos::new(5, 6, 1), Inventory::default()));
        assert_eq!(
            vend(&mut world, actor, vendor, 1).err(),
            Some(VendReject::NoId)
        );
        world.get_mut::<Vendor>(vendor).unwrap().all_products_free = true;
        assert_eq!(
            vend(&mut world, actor, vendor, 1).err(),
            Some(VendReject::NoPrototype)
        );
    }

    #[test]
    fn vend_param_must_be_an_index() {
        let params = [("ref".to_owned(), "x".to_owned())];
        let parsed = VendingWindow::parse_act("vend", &ss13_tgui::server::UiParams::new(&params));
        assert!(matches!(parsed, Err(UiReject::BadParams(_))));
    }
}
