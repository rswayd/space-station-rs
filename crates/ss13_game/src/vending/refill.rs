//! Refill canisters, the vendor board and taking vendors apart (server), after tg
//! `code/game/objects/items/vending_items.dm` (`/obj/item/vending_refill`),
//! `vendor/inventory.dm` (`RefreshParts`, `restock`), `vendor/interaction.dm` (`item_interaction`,
//! `post_restock`, `exchange_parts`), `vendor/_vending.dm` (`on_deconstruction`) and
//! `machine_circuitboards.dm` (`/obj/item/circuitboard/machine/vendor`).
//!
//! - A vendor with a `refill_canister` type has a circuit: its parts are the vendor board (its
//!   brand: the vendor type and its canister) and one canister. Taken apart with a crowbar
//!   (panel open, `crate::machine_frame`), the canister keeps the stock that was in the machine
//!   (returned items fall out); a frame built with that board and canister is that vendor again,
//!   stocked from the canister. A vendor without a canister type cannot be taken apart.
//! - A canister ([`RefillStock`]) of the vendor's type, used on it with the panel open, moves
//!   what the records miss into them (`restock`); one never filled (cargo, map) fills the
//!   machine completely. An RPED holding canisters does the same (from a distance for the
//!   bluespace one). The restocker gets the credits the machine collected, cargo half as much
//!   again.
// - The vendor board's dial (a screwdriver on it, `tgui_input_list`) picks any brand with
//   `allow_custom` and a canister type; a board from an off-station vendor has none.
// shortcut: the brands offered are the converted vendor types (every mapped one); the credits go
// to the restocker's bank account (tg hands out a holochip; no cash here).

use bevy::prelude::*;
use ss13_mapload::{CircuitPart, DmItem, DmName, DmVending, DmVendingRefill};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use super::{RecordKind, VENDING_ROOT, Vendor, name_of, operational};
use crate::cargo::economy::{ACCOUNT_CAR, BankAccountId, Economy};
use crate::construction::PanelOpen;
use crate::obj_damage::is_under;
use crate::stock_parts::{ComponentParts, attach};
use crate::tools::{Actor, tell};

/// `/obj/item/circuitboard/machine/vendor`.
pub const VENDOR_BOARD: &str = "/obj/item/circuitboard/machine/vendor";
pub const REFILL_ROOT: &str = ss13_mapload::vending::REFILL_ROOT;
const CUSTOM_REFILL: &str = "/obj/item/vending_refill/custom";

/// The record lists of a canister in tg order: `products`, `contraband`, `premium`.
const KINDS: [RecordKind; 3] = [RecordKind::Product, RecordKind::Hidden, RecordKind::Coin];

/// A refill canister's stock by type, per list (`products`, `contraband`, `premium`). `None`:
/// never filled (tg `products == null`, `get_part_rating` INFINITY): the first restock takes the
/// machine's full lists.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct RefillStock(pub Option<[Vec<(String, u32)>; 3]>);

impl RefillStock {
    /// tg `get_part_rating`: what it can restock.
    pub fn rating(&self) -> f64 {
        match &self.0 {
            None => f64::INFINITY,
            Some(lists) => lists.iter().flatten().map(|(_, n)| f64::from(*n)).sum(),
        }
    }
}

/// The vendor board's state (`set_type`, `all_products_free`): what a frame builds with it.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct VendorBoard {
    pub build_path: String,
    pub refill: String,
    pub free: bool,
}

/// `/obj/item/vending_refill/Initialize`: `"\improper [machine_name] restocking unit"`.
pub(super) fn name_canister(
    add: On<Add<DmVendingRefill>>,
    refills: Query<&DmVendingRefill>,
    mut commands: Commands,
) {
    let Ok(refill) = refills.get(add.entity) else {
        return;
    };
    let name = format!(
        "\\improper {} restocking unit",
        super::plain_name(&refill.machine_name)
    );
    let entity = add.entity;
    commands.queue(move |world: &mut World| {
        if let Some(mut dm) = world.get_mut::<DmName>(entity) {
            dm.name = name;
        }
        if world.get::<RefillStock>(entity).is_none() {
            world.entity_mut(entity).insert(RefillStock::default());
        }
    });
}

fn item_path(world: &World, item: Entity) -> Option<String> {
    world.get::<DmItem>(item).map(|i| i.path.clone())
}

/// A vendor's circuit (tg: `circuit = null` without a `refill_canister`).
pub fn has_circuit(world: &World, vendor: Entity) -> bool {
    world
        .get::<Vendor>(vendor)
        .map(|v| !v.refill_canister.is_empty())
        .or_else(|| {
            world
                .get::<DmVending>(vendor)
                .map(|v| !v.refill_canister.is_empty())
        })
        .unwrap_or(false)
}

/// The canister among a vendor's parts.
pub fn installed_canister(world: &World, vendor: Entity) -> Option<Entity> {
    world
        .get::<ComponentParts>(vendor)?
        .parts()
        .iter()
        .copied()
        .find(|&p| item_path(world, p).is_some_and(|path| is_under(&path, REFILL_ROOT)))
}

/// `/obj/item/circuitboard/machine/vendor` of `vendor`'s type (`apply_default_parts` ->
/// `set_type`).
fn board_for(vendor: &Vendor, path: &str) -> VendorBoard {
    VendorBoard {
        build_path: path.to_owned(),
        refill: vendor.refill_canister.clone(),
        free: vendor.all_products_free,
    }
}

/// `set_type`: `"[typepath::name] Vendor"`.
fn name_board(world: &mut World, board: Entity, vendor_name: &str) {
    if let Some(mut dm) = world.get_mut::<DmName>(board) {
        dm.name = format!("{vendor_name} Vendor");
    }
}

/// [`crate::stock_parts::ensure_parts`] for vendors: the board with this vendor's brand and an
/// emptied canister (a mapped vendor took its stock from it in `RefreshParts`). `None`: not a
/// vendor.
pub fn ensure_parts(world: &mut World, machine: Entity) -> Option<bool> {
    let vendor = world.get::<Vendor>(machine)?.clone();
    if vendor.refill_canister.is_empty() {
        return Some(false);
    }
    let path = crate::construction::path_of(world, machine)?;
    let pos = *world.get::<TilePos>(machine)?;
    let board_state = board_for(&vendor, &path);
    let vendor_name = name_of(world, machine);
    let Some(board) = crate::construction::spawn_obj(world, VENDOR_BOARD, pos) else {
        return Some(false);
    };
    world.entity_mut(board).insert(board_state);
    name_board(world, board, &vendor_name);
    attach(world, board, machine);
    if let Some(canister) = crate::construction::spawn_obj(world, &vendor.refill_canister, pos) {
        world
            .entity_mut(canister)
            .insert(RefillStock(Some(Default::default())));
        attach(world, canister, machine);
    }
    Some(true)
}

/// The board's build path and required parts when it carries a brand ([`VendorBoard`]).
pub fn board_override(world: &World, board: Entity) -> Option<(String, Vec<CircuitPart>)> {
    let state = world.get::<VendorBoard>(board)?;
    Some((
        state.build_path.clone(),
        vec![CircuitPart {
            path: state.refill.clone(),
            amount: 1,
        }],
    ))
}

/// A printed vendor board: the custom vendor (its `build_path`), free when it says so.
fn board_state(world: &World, parts: &[Entity]) -> Option<VendorBoard> {
    parts
        .iter()
        .find_map(|&p| world.get::<VendorBoard>(p).cloned())
}

/// The vendor a frame finished: its records empty (`build_inventories(start_empty = TRUE)`),
/// pricing from the board, wires, then [`refresh_parts`] stocks it from the canister.
pub fn setup_built(world: &mut World, machine: Entity) {
    let Some(dm) = world.get::<DmVending>(machine).cloned() else {
        return;
    };
    let paths: Vec<String> = dm
        .products
        .iter()
        .chain(&dm.contraband)
        .chain(&dm.premium)
        .map(|p| p.path.clone())
        .collect();
    super::add_templates(world, paths.iter().map(String::as_str));
    let mut vendor = world.resource_scope(|_, templates: Mut<super::ProductTemplates>| {
        super::vendor(&dm, &templates.0)
    });
    for record in &mut vendor.records {
        record.amount = 0;
    }
    let parts: Vec<Entity> = world
        .get::<ComponentParts>(machine)
        .map(|p| p.parts().to_vec())
        .unwrap_or_default();
    // `all_products_free = circuit.all_products_free`: a reconstructed vendor keeps its pricing.
    let free = board_state(world, &parts).is_some_and(|b| b.free);
    vendor.all_products_free = free;
    vendor.ui.all_products_free = free;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let delay = world
        .get_resource_mut::<crate::combat::CombatRng>()
        .map_or(0.0, |mut rng| f64::from(rng.unit()) * super::SLOGAN_DELAY);
    vendor.last_slogan = now + delay;
    let custom = super::custom::is_custom(world, machine);
    world.entity_mut(machine).insert(vendor);
    if custom {
        super::custom::setup(world, machine, &dm);
    }
    super::add_looks(world, machine);
    crate::wires::add_vending_wires(world, machine);
    super::refresh_look(world, machine);
}

/// `RefreshParts`: a vendor restocks from the canister among its parts.
pub fn refresh_parts(world: &mut World, machine: Entity) {
    if world.get::<Vendor>(machine).is_none() {
        return;
    }
    if let Some(canister) = installed_canister(world, machine) {
        restock(world, machine, canister);
    }
}

/// tg `restock`: each record takes what it misses from the canister's list of its kind; a
/// canister never filled first takes the machine's full lists. Returns the items moved.
pub fn restock(world: &mut World, vendor: Entity, canister: Entity) -> u32 {
    if super::custom::is_custom(world, vendor) {
        return super::custom::restock(world, vendor, canister);
    }
    let Some(dm) = world.get::<DmVending>(vendor).cloned() else {
        return 0;
    };
    let mut stock = world
        .get::<RefillStock>(canister)
        .cloned()
        .unwrap_or_default();
    let lists = stock.0.get_or_insert_with(|| {
        let list = |l: &[ss13_mapload::VendProduct]| -> Vec<(String, u32)> {
            l.iter().map(|p| (p.path.clone(), p.amount)).collect()
        };
        [list(&dm.products), list(&dm.contraband), list(&dm.premium)]
    });
    let Some(mut machine) = world.get_mut::<Vendor>(vendor) else {
        return 0;
    };
    let mut moved = 0;
    for (kind, list) in KINDS.iter().zip(lists.iter_mut()) {
        for record in machine.records.iter_mut().filter(|r| r.kind == *kind) {
            let Some(i) = list.iter().position(|(p, _)| *p == record.path) else {
                continue;
            };
            let diff = record
                .max_amount
                .saturating_sub(record.amount)
                .min(list[i].1);
            if diff == 0 {
                continue;
            }
            list[i].1 -= diff;
            if list[i].1 == 0 {
                list.remove(i);
            }
            record.amount += diff;
            moved += diff;
        }
    }
    world.entity_mut(canister).insert(stock);
    moved
}

/// `istype(item, refill_canister)`.
fn fits(world: &World, vendor: Entity, canister: Entity) -> bool {
    let Some(refill) = world
        .get::<Vendor>(vendor)
        .map(|v| v.refill_canister.clone())
    else {
        return false;
    };
    !refill.is_empty() && item_path(world, canister).is_some_and(|p| is_under(&p, &refill))
}

/// A canister's `get_part_rating` (custom canisters: the items inside).
pub fn rating(world: &World, canister: Entity) -> f64 {
    if item_path(world, canister).is_some_and(|p| is_under(&p, CUSTOM_REFILL)) {
        return world
            .get::<crate::storage::Stored>(canister)
            .map_or(0.0, |s| s.items().len() as f64);
    }
    world
        .get::<RefillStock>(canister)
        .map_or(f64::INFINITY, RefillStock::rating)
}

/// The canister branch of tg `item_interaction`. True when `held` is this vendor's canister.
pub fn canister_interaction(world: &mut World, actor: Actor, held: Entity, vendor: Entity) -> bool {
    if !fits(world, vendor, held) {
        return false;
    }
    let client = actor.client;
    let open = world.get::<PanelOpen>(vendor).is_some_and(|p| p.0);
    if !open {
        tell(
            world,
            client,
            Span::Warning,
            "Сначала стоит открутить панель обслуживания!",
        );
    } else if !operational(world, vendor) {
        let text = format!("{} не отвечает.", name_of(world, vendor));
        tell(world, client, Span::Warning, text);
    } else if rating(world, held) == 0.0 {
        let text = format!("{} пуст!", name_of(world, held));
        tell(world, client, Span::Warning, text);
    } else {
        let restocked = restock(world, vendor, held);
        post_restock(world, actor, vendor, restocked);
    }
    true
}

/// tg `post_restock`: the count, and the credits the machine collected (cargo gets half as much
/// again).
fn post_restock(world: &mut World, actor: Actor, vendor: Entity, restocked: u32) {
    let client = actor.client;
    if restocked == 0 {
        tell(world, client, Span::Warning, "Нечего пополнять!");
        return;
    }
    let name = name_of(world, vendor);
    if super::custom::is_custom(world, vendor) {
        let text = format!("Вы загрузили {restocked} предметов в {name}.");
        tell(world, client, Span::Notice, text);
        return;
    }
    let credits = world
        .get::<Vendor>(vendor)
        .map_or(0, |v| v.credits_contained);
    let text = if credits > 0 {
        format!("Вы загрузили {restocked} предметов в {name} и получаете {credits}¢.")
    } else {
        format!("Вы загрузили {restocked} предметов в {name}.")
    };
    tell(world, client, Span::Notice, text);
    let account = world
        .get::<crate::inventory::Inventory>(actor.mob)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<BankAccountId>(c))
        .map(|a| a.0);
    if let Some(mut economy) = world.get_resource_mut::<Economy>() {
        if let Some(cargo) = economy.department_mut(ACCOUNT_CAR) {
            cargo.adjust_money((credits as f64 * 0.5).round() as i64);
        }
        if credits >= 1
            && let Some(account) = account.and_then(|a| economy.account_mut(a))
        {
            account.adjust_money(credits);
        }
    }
    if let Some(mut machine) = world.get_mut::<Vendor>(vendor) {
        machine.credits_contained = 0;
    }
}

/// tg `exchange_parts` of a vendor: the RPED's canisters of its type restock it (panel open, or
/// a bluespace RPED); with the panel closed it only lists the parts. True when handled.
pub fn rped_restock(world: &mut World, client: Entity, rped: Entity, vendor: Entity) -> bool {
    if !has_circuit(world, vendor) {
        return false;
    }
    let bluespace =
        item_path(world, rped).is_some_and(|p| is_under(&p, crate::stock_parts::BLUESPACE_RPED));
    let open = world.get::<PanelOpen>(vendor).is_some_and(|p| p.0);
    if !open || bluespace {
        crate::stock_parts::ensure_parts(world, vendor);
        let text = crate::stock_parts::display_parts(world, vendor);
        tell(world, client, Span::Notice, text);
    }
    if !open && !bluespace {
        return false;
    }
    let canisters: Vec<Entity> = world
        .get::<crate::storage::Stored>(rped)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&c| fits(world, vendor, c))
        .collect();
    let restocked: u32 = canisters
        .into_iter()
        .map(|c| restock(world, vendor, c))
        .sum();
    let Some(mob) = world.resource::<crate::server::Players>().mob(client) else {
        return true;
    };
    post_restock(
        world,
        Actor {
            client,
            mob,
            item: Some(rped),
        },
        vendor,
        restocked,
    );
    true
}

/// tg `on_deconstruction`: the canister among the parts keeps the machine's stock (returned
/// items are not stock: they fall out). The custom vendor's own: [`super::custom`].
pub fn on_deconstruction(world: &mut World, vendor: Entity, pos: TilePos, disassembled: bool) {
    if super::custom::is_custom(world, vendor) {
        super::custom::on_deconstruction(world, vendor, pos, disassembled);
        return;
    }
    if !crate::stock_parts::ensure_parts(world, vendor) {
        return;
    }
    let (Some(canister), Some(machine)) = (
        installed_canister(world, vendor),
        world.get::<Vendor>(vendor),
    ) else {
        return;
    };
    let lists = KINDS.map(|kind| {
        machine
            .records
            .iter()
            .filter(|r| r.kind == kind)
            .filter_map(|r| {
                let stock = r.amount.saturating_sub(r.returned.len() as u32);
                (stock > 0).then(|| (r.path.clone(), stock))
            })
            .collect()
    });
    world.entity_mut(canister).insert(RefillStock(Some(lists)));
}

/// On a brand dial prompt: the board it turns.
#[derive(Component, Clone, Copy, Debug)]
struct BrandFor(Entity);

/// `valid_vendor_names_paths`: (name, type) of every brand, by name (a later type with the same
/// name wins, as in tg's assoc list).
fn brands(world: &World) -> Vec<(String, String)> {
    let Some(protos) = world.get_resource::<ss13_proto::Prototypes>() else {
        return Vec::new();
    };
    let mut ids: Vec<&str> = protos
        .ids()
        .filter(|id| is_under(id, VENDING_ROOT) && *id != VENDING_ROOT)
        .collect();
    ids.sort_unstable();
    let mut out: Vec<(String, String)> = Vec::new();
    for id in ids {
        let Some(proto) = protos.get(id) else {
            continue;
        };
        let Some(dm) = proto.get::<DmVending>() else {
            continue;
        };
        if !dm.allow_custom || dm.refill_canister.is_empty() {
            continue;
        }
        let name = proto
            .get::<DmName>()
            .map(|n| super::plain_name(&n.name).to_owned())
            .unwrap_or_else(|| ss13_mapload::convert::default_name(id));
        out.retain(|(n, _)| *n != name);
        out.push((name, id.to_owned()));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// tg `/obj/item/circuitboard/machine/vendor/screwdriver_act`: "Choose a new brand". True when
/// `board` is a vendor board.
pub fn screwdriver_board(world: &mut World, actor: Actor, board: Entity) -> bool {
    if !item_path(world, board).is_some_and(|p| is_under(&p, VENDOR_BOARD)) {
        return false;
    }
    // "This board's card reader component has been cut out, along with its brand selection
    // dial."
    if world.get::<VendorBoard>(board).is_some_and(|b| b.free) {
        return true;
    }
    let items: Vec<String> = brands(world).into_iter().map(|(n, _)| n).collect();
    if items.is_empty() {
        return true;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            actor.client,
            actor.mob,
            "Select an Item",
            "Choose a new brand",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(BrandFor(board));
    true
}

/// The brand dial's answers: `set_type` if the board is still within the actor's reach
/// (`can_perform_action`).
pub(super) fn brand_answers(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<PromptAnswered>>,
) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(&BrandFor(board)) = world.get::<BrandFor>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let conscious =
            world.get::<crate::health::Stat>(actor) == Some(&crate::health::Stat::Conscious);
        let reach = world.get_entity(board).is_ok()
            && (crate::storage::in_reach(world, actor, board)
                || world
                    .get::<crate::inventory::CarriedBy>(board)
                    .is_some_and(|c| c.0 == actor));
        if !conscious || !reach || world.get::<VendorBoard>(board).is_some_and(|b| b.free) {
            continue;
        }
        let Some((name, path)) = brands(world).into_iter().nth(i) else {
            continue;
        };
        let refill = world
            .get_resource::<ss13_proto::Prototypes>()
            .and_then(|p| p.get(&path))
            .and_then(|p| p.get::<DmVending>())
            .map(|d| d.refill_canister.clone())
            .unwrap_or_default();
        world.entity_mut(board).insert(VendorBoard {
            build_path: path,
            refill,
            free: false,
        });
        name_board(world, board, &name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_counts_every_list_and_a_fresh_canister_is_full() {
        assert_eq!(RefillStock(None).rating(), f64::INFINITY);
        let stock = RefillStock(Some([
            vec![("/a".into(), 2)],
            vec![],
            vec![("/b".into(), 3)],
        ]));
        assert_eq!(stock.rating(), 5.0);
    }
}
