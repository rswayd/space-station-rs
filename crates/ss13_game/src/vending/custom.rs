//! The custom vendor (server), after tg `code/modules/vending/custom.dm`: an owner links their
//! bank account with an ID swipe (again to unlink), loads priced items (`custom_price`, no
//! loose stacks, at most `max_loaded_items`) and takes them for free; everyone else pays the
//! item's price from their ID's account into the owner's. Without an owner the window does not
//! open. Items with the same name, price and type share one row (tg `ITEM_HASH`).
//!
//! Taking it apart moves the items into its canister, which restocks a custom vendor built
//! with it; while an account is linked the crowbar asks first, and "Yes" self-destructs it
//! (the canister is destroyed, the items fall out into a light explosion).
// shortcut: no custom slogan on rename, no hologram check (no holograms), item pictures are the
// item's icon state.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmPrice, DmVending};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::interfaces::vending::{ProductIcon, ProductRecord, sanitized_path};

use super::{RecordKind, VendRecord, VendReject, Vended, Vendor, name_of, speak};
use crate::cargo::economy::{BankAccountId, Economy, bank_card_talk};
use crate::inventory::{IdCard, Inventory};
use crate::obj_damage::is_under;
use crate::tools::{Actor, tell};

pub const CUSTOM_ROOT: &str = "/obj/machinery/vending/custom";
const STACK_ROOT: &str = "/obj/item/stack";
const STORAGE_BAG: &str = "/obj/item/storage/bag";

/// The custom vendor's owner (`linked_account`) and capacity.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CustomVendor {
    pub linked: Option<u32>,
    pub max_loaded: u32,
}

pub fn is_custom(world: &World, vendor: Entity) -> bool {
    crate::construction::path_of(world, vendor).is_some_and(|p| is_under(&p, CUSTOM_ROOT))
}

/// Map load or a frame: the owner slot and the one category tg shows.
pub fn setup(world: &mut World, vendor: Entity, dm: &DmVending) {
    world.entity_mut(vendor).insert(CustomVendor {
        linked: None,
        max_loaded: dm.max_loaded_items,
    });
    if let Some(mut machine) = world.get_mut::<Vendor>(vendor) {
        machine.ui.categories = vec![("Products".into(), "cart-shopping".into())];
    }
}

/// [`CustomVendor`] on every map-loaded custom vendor.
pub(super) fn init(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmVending), With<Vendor>>();
    let found: Vec<(Entity, DmVending)> = query.iter(world).map(|(e, d)| (e, d.clone())).collect();
    for (vendor, dm) in found {
        if is_custom(world, vendor) {
            setup(world, vendor, &dm);
        }
    }
}

/// The account of the ID tg `get_idcard(FALSE)` finds: the ID slot, then the hands.
fn owner_account(world: &World, user: Entity) -> Option<u32> {
    let inventory = world.get::<Inventory>(user)?;
    let card = [
        inventory.id,
        inventory.hands[inventory.active],
        inventory.hands[1 - inventory.active],
    ]
    .into_iter()
    .flatten()
    .find_map(|e| crate::inventory::get_id(world, e))?;
    world.get::<BankAccountId>(card).map(|a| a.0)
}

/// tg `compartmentLoadAccessCheck` of the custom vendor: the user's account is the linked one.
pub fn is_owner(world: &World, vendor: Entity, user: Entity) -> bool {
    let linked = world.get::<CustomVendor>(vendor).and_then(|c| c.linked);
    linked.is_some() && owner_account(world, user) == linked
}

/// `ui_interact`: no window without an owner.
pub(super) fn may_open(world: &mut World, client: Entity, vendor: Entity) -> bool {
    match world.get::<CustomVendor>(vendor) {
        Some(c) if c.linked.is_none() => {
            tell(
                world,
                client,
                Span::Warning,
                "нет зарегистрированного владельца!",
            );
            false
        }
        _ => true,
    }
}

/// Self-destructs waiting for the machine to be gone: `explosion(light_impact_range = 3)`.
#[derive(Resource, Default)]
pub(super) struct PendingBlasts(Vec<TilePos>);

pub(super) fn blasts(world: &mut World) {
    let Some(mut pending) = world.get_resource_mut::<PendingBlasts>() else {
        return;
    };
    let due = std::mem::take(&mut pending.0);
    for pos in due {
        // `devastation_range = -1, light_impact_range = 3`: flame defaults to the light range,
        // flash to the (negative, none) devastation range.
        crate::explosions::explosion(
            world,
            pos,
            crate::explosions::Explosion::new(-1, 0, 3).cause("custom vendor"),
        );
    }
}

/// On the self-destruct alert: the vendor and the crowbar that asked.
#[derive(Component, Clone, Copy, Debug)]
struct SelfDestructFor {
    vendor: Entity,
    crowbar: Option<Entity>,
}

/// tg `/obj/machinery/vending/custom/crowbar_act` while ID-locked: a warning and
/// `tgui_alert`; "Yes" takes it apart, which self-destructs. True when it asked.
pub fn asks_before_crowbar(world: &mut World, actor: Actor, vendor: Entity) -> bool {
    if !world
        .get::<CustomVendor>(vendor)
        .is_some_and(|c| c.linked.is_some())
    {
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(vendor) {
        crate::security::show(
            world,
            pos,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ss13_shared::chat::ChatChannel::Info,
            (Span::Warning, "Предупреждение безопасности".into()),
            &[(
                actor.mob,
                Span::Warning,
                "Несанкционированная разборка торгового автомата запрещена. Прочтите \
                 предупреждение."
                    .into(),
            )],
        );
    }
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        ss13_tgui::prompt::Ask::new(
            actor.client,
            actor.mob,
            "Vandalism protection protocol",
            "Автомат привязан к ID. Разборка приведёт к катастрофическому самоуничтожению. \
             Если вы владелец, сначала отвяжите счёт, проведя картой. Продолжить?",
            ss13_tgui::prompt::PromptKind::Alert {
                buttons: vec!["Yes".into(), "No".into()],
                autofocus: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(SelfDestructFor {
        vendor,
        crowbar: actor.item,
    });
    true
}

/// "Yes": `default_deconstruction_crowbar` at the moment of the answer (panel open, the crowbar
/// still in the active hand, the vendor in reach).
pub(super) fn self_destruct_answers(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<ss13_tgui::prompt::PromptAnswered>>,
) {
    let answers: Vec<(Entity, Entity, Option<ss13_tgui::prompt::PromptAnswer>)> = cursor
        .read(world.resource::<Messages<ss13_tgui::prompt::PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(&SelfDestructFor { vendor, crowbar }) = world.get::<SelfDestructFor>(prompt)
        else {
            continue;
        };
        if answer != Some(ss13_tgui::prompt::PromptAnswer::Button(0)) {
            continue;
        }
        let holding = world
            .get::<Inventory>(actor)
            .and_then(Inventory::active_item)
            .is_some_and(|i| Some(i) == crowbar);
        let open = world
            .get::<crate::construction::PanelOpen>(vendor)
            .is_some_and(|p| p.0);
        let conscious =
            world.get::<crate::health::Stat>(actor) == Some(&crate::health::Stat::Conscious);
        if world.get::<CustomVendor>(vendor).is_none()
            || !holding
            || !open
            || !conscious
            || !crate::storage::in_reach(world, actor, vendor)
        {
            continue;
        }
        crate::obj_damage::deconstruct(world, vendor, true);
    }
}

/// The ID swipe and loading branches of tg `item_interaction`. True when handled.
pub(super) fn item_interaction(
    world: &mut World,
    actor: Actor,
    held: Entity,
    vendor: Entity,
) -> bool {
    let Some(custom) = world.get::<CustomVendor>(vendor).copied() else {
        return false;
    };
    let client = actor.client;
    if world.get::<IdCard>(held).is_some() {
        let Some(account) = world.get::<BankAccountId>(held).map(|a| a.0) else {
            return true;
        };
        let card = name_of(world, held);
        match custom.linked {
            None => {
                world
                    .get_mut::<CustomVendor>(vendor)
                    .expect("checked")
                    .linked = Some(account);
                let name = name_of(world, vendor);
                speak(world, vendor, &format!("{name} привязан к {card}."));
            }
            Some(linked) if linked == account => {
                world
                    .get_mut::<CustomVendor>(vendor)
                    .expect("checked")
                    .linked = None;
                speak(world, vendor, "Счёт отвязан.");
            }
            Some(_) => tell(
                world,
                client,
                Span::Plain,
                "Проверка не пройдена. Отвязка отменена.",
            ),
        }
        return true;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(actor.mob)
        .is_some_and(|c| c.0);
    if combat || !is_owner(world, vendor, actor.mob) {
        return false;
    }
    if !super::operational(world, vendor) {
        let text = format!("{} не отвечает.", name_of(world, vendor));
        tell(world, client, Span::Warning, text);
        return true;
    }
    let path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let items = if is_under(&path, STORAGE_BAG) {
        world
            .get::<crate::storage::Stored>(held)
            .map(|s| s.items().to_vec())
            .unwrap_or_default()
    } else {
        if crate::inventory::no_drop(world, held) {
            let text = format!("{} прилип к руке!", name_of(world, held));
            tell(world, client, Span::Warning, text);
            return true;
        }
        vec![held]
    };
    let mut changed = false;
    for item in items {
        if !can_load(world, vendor, item, true) {
            continue;
        }
        if loaded(world, vendor) >= custom.max_loaded {
            speak(world, vendor, "В автомате слишком много товаров.");
            break;
        }
        changed |= add(world, vendor, item);
    }
    if changed {
        rebuild_ui(world, vendor);
    }
    true
}

/// tg `/obj/machinery/vending/custom/canLoadItem`.
fn can_load(world: &mut World, vendor: Entity, item: Entity, say: bool) -> bool {
    let path = world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if is_under(&path, STACK_ROOT) {
        if say {
            speak(
                world,
                vendor,
                "Россыпь может доставить проблем, попробуйте завернуть её в упаковочную бумагу.",
            );
        }
        return false;
    }
    if price_of(world, item) == 0 {
        if say {
            speak(world, vendor, "Предмету нужно назначить цену.");
        }
        return false;
    }
    true
}

fn price_of(world: &World, item: Entity) -> u32 {
    world.get::<DmPrice>(item).map_or(0, |p| p.custom_price)
}

/// `loaded_items()`.
fn loaded(world: &World, vendor: Entity) -> u32 {
    world
        .get::<Vendor>(vendor)
        .map_or(0, |v| v.records.iter().map(|r| r.amount).sum())
}

/// The row of `item` (`ITEM_HASH`: name, price, type), made if new. Returns true when a new row
/// appeared (the static data changes).
fn add(world: &mut World, vendor: Entity, item: Entity) -> bool {
    let name = name_of(world, item);
    let price = price_of(world, item);
    let path = world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let icon = world.get::<Appearance>(item).map(|a| ProductIcon {
        icon_state: a.icon_state,
        color: a.color,
    });
    crate::storage::put_inside(world, item, vendor);
    let mut machine = world
        .get_mut::<Vendor>(vendor)
        .expect("checked by the caller");
    let index = (0..machine.records.len()).find(|&i| {
        let r = &machine.records[i];
        r.path == path && r.price == price && row_name(&machine, i as u32) == name
    });
    let new_row = index.is_none_or(|i| machine.records[i].amount == 0);
    let index = index.unwrap_or_else(|| {
        let i = machine.records.len();
        machine.records.push(VendRecord {
            path: path.clone(),
            amount: 0,
            max_amount: 0,
            kind: RecordKind::Product,
            base_price: price,
            price,
            age_restricted: false,
            returned: Vec::new(),
        });
        machine.ui.product_records.push(ProductRecord {
            path: sanitized_path(&path),
            name: name.clone(),
            price,
            reference: i as u32,
            category: "Products".into(),
            colorable: false,
            premium: false,
            icon,
        });
        i
    });
    let record = &mut machine.records[index];
    record.amount += 1;
    record.max_amount = record.max_amount.max(record.amount);
    record.returned.push(item);
    new_row
}

/// A row's display name: its line in the static data (kept for rows sold out).
fn row_name(vendor: &Vendor, reference: u32) -> String {
    vendor
        .ui
        .product_records
        .iter()
        .chain(&vendor.ui.hidden_records)
        .find(|r| r.reference == reference)
        .map(|r| r.name.clone())
        .unwrap_or_default()
}

/// tg `collect_records_for_static_data` of the custom vendor: rows in stock only. Sold-out rows
/// keep their index in `hidden_records`, which the window never lists for a custom vendor.
pub(super) fn rebuild_ui(world: &mut World, vendor: Entity) {
    let Some(mut machine) = world.get_mut::<Vendor>(vendor) else {
        return;
    };
    let mut all: Vec<ProductRecord> = std::mem::take(&mut machine.ui.product_records);
    all.append(&mut machine.ui.hidden_records);
    all.sort_by_key(|r| r.reference);
    for row in all {
        let stocked = machine
            .records
            .get(row.reference as usize)
            .is_some_and(|r| r.amount > 0);
        if stocked {
            machine.ui.product_records.push(row);
        } else {
            machine.ui.hidden_records.push(row);
        }
    }
    super::refresh_static(world, &[vendor]);
}

/// tg `/obj/machinery/vending/custom/vend`: the owner takes for free; anyone else pays the price
/// from their ID's account into the owner's.
pub(super) fn vend(
    world: &mut World,
    actor: Entity,
    vendor: Entity,
    reference: u32,
) -> Result<Vended, VendReject> {
    let (vendor_pos, inventory) = super::vend_preconditions(world, actor, vendor)?;
    let machine = world.get::<Vendor>(vendor).expect("checked above");
    let record = machine
        .records
        .get(reference as usize)
        .cloned()
        .ok_or(VendReject::NotSold)?;
    if record.amount == 0 || record.returned.is_empty() {
        return Err(VendReject::SoldOut);
    }
    let product = row_name(machine, reference);
    let Some(card) = inventory.id_card(world) else {
        super::tell_mob(world, actor, Span::Warning, "карта не найдена!");
        return Err(VendReject::NoId);
    };
    let mut paid = 0;
    if !is_owner(world, vendor, actor) {
        if crate::cargo::economy::is_departmental_card(world, card) {
            crate::balloon::balloon_alert(world, actor, vendor, "неверная платёжная карта");
            super::tell_mob(
                world,
                actor,
                Span::Warning,
                "Эту карту отдела нельзя использовать для этого.",
            );
            return Err(VendReject::NoAccount);
        }
        let linked = world.get::<CustomVendor>(vendor).and_then(|c| c.linked);
        let payer = world.get::<BankAccountId>(card).map(|a| a.0);
        let (Some(linked), Some(payer)) = (linked, payer) else {
            super::tell_mob(world, actor, Span::Warning, "Аккаунт не найден.");
            return Err(VendReject::NoAccount);
        };
        let price = record.price;
        let mut economy = world.resource_mut::<Economy>();
        let holder = economy.account(payer).map(|a| a.holder.clone());
        let charged = economy
            .account_mut(payer)
            .is_some_and(|a| a.adjust_money(-i64::from(price)));
        if !charged {
            super::tell_mob(world, actor, Span::Warning, "недостаточно средств!");
            return Err(VendReject::NoFunds);
        }
        if let Some(owner) = economy.account_mut(linked) {
            owner.adjust_money(i64::from(price));
        }
        let holder = holder.unwrap_or_default();
        bank_card_talk(
            world,
            linked,
            &format!("{holder} совершает покупку на {price} ¢ в вашем автомате."),
        );
        paid = price;
        let now = world.resource::<Time>().elapsed_secs_f64();
        let machine = world.get::<Vendor>(vendor).expect("checked above");
        if !machine
            .last_shopper
            .is_some_and(|(who, until)| who == actor && now < until)
        {
            world
                .get_mut::<Vendor>(vendor)
                .expect("checked")
                .last_shopper = Some((actor, now + 5.0));
            let who = name_of(world, actor);
            speak(world, vendor, &format!("Спасибо за покупку, {who}!"));
        }
    }
    let item = super::dispense(world, vendor, reference as usize, vendor_pos, true, true)
        .ok_or(VendReject::SoldOut)?;
    // `try_put_in_hand`: a hand, else at the user's feet.
    let actor_pos = world.get::<TilePos>(actor).copied().unwrap_or(vendor_pos);
    let in_hand = super::give(world, actor, &inventory, item, &product, actor_pos);
    if world
        .get::<Vendor>(vendor)
        .is_some_and(|v| v.records[reference as usize].amount == 0)
    {
        rebuild_ui(world, vendor);
    }
    Ok(Vended {
        actor,
        vendor,
        item,
        path: record.path,
        in_hand,
        paid,
    })
}

/// tg `/obj/machinery/vending/custom/restock`: the canister's items in, up to the capacity.
pub fn restock(world: &mut World, vendor: Entity, canister: Entity) -> u32 {
    let max = world
        .get::<CustomVendor>(vendor)
        .map_or(0, |c| c.max_loaded);
    let items: Vec<Entity> = world
        .get::<crate::storage::Stored>(canister)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    let mut moved = 0;
    for item in items {
        if loaded(world, vendor) >= max {
            break;
        }
        add(world, vendor, item);
        moved += 1;
    }
    if moved > 0 {
        rebuild_ui(world, vendor);
    }
    moved
}

/// tg `/obj/machinery/vending/custom/on_deconstruction`: the items into the canister; while
/// ID-locked the canister is destroyed and it explodes (light impact 3).
pub(super) fn on_deconstruction(
    world: &mut World,
    vendor: Entity,
    pos: TilePos,
    _disassembled: bool,
) {
    if !crate::stock_parts::ensure_parts(world, vendor) {
        return;
    }
    let Some(canister) = super::refill::installed_canister(world, vendor) else {
        return;
    };
    if world
        .get::<CustomVendor>(vendor)
        .is_some_and(|c| c.linked.is_some())
    {
        // "we delete the canister so players don't resell our products as their own"; the
        // items fall out with the machine's contents. The blast comes after it is gone.
        world.despawn(canister);
        world.get_resource_or_init::<PendingBlasts>().0.push(pos);
        return;
    }
    let items: Vec<Entity> = world
        .get_mut::<Vendor>(vendor)
        .map(|mut v| {
            v.records
                .iter_mut()
                .flat_map(|r| {
                    r.amount = 0;
                    std::mem::take(&mut r.returned)
                })
                .collect()
        })
        .unwrap_or_default();
    for item in items {
        crate::storage::put_inside(world, item, canister);
    }
}
