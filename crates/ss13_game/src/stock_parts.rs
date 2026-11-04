//! Machine parts (server), after tg `code/modules/research/stock_parts.dm`,
//! `stock_parts/stock_part_datum.dm`, `code/game/machinery/_machinery.dm` (`component_parts`,
//! `RefreshParts`, `exchange_parts`, `display_parts`), `code/modules/research/part_replacer.dm`
//! and `code/datums/storage/subtypes/others/rped.dm`.
//!
//! - A machine's or frame's `component_parts` are item entities with [`PartOf`] (board included):
//!   off the map like stored items, despawned with their owner, and given back as the same items
//!   on deconstruction (a cell keeps its charge). A mapped machine holds its board's default parts
//!   as types until something needs the items ([`ensure_parts`]).
//! - Stock parts have a kind (capacitor, scanning module, servo, micro-laser, matter bin) and a
//!   tier (`rating`, 1-4; `<assets>/mecha.ron` `stock_parts`). [`PartRatings`] is what a
//!   machine's `RefreshParts` reads: [`refresh`] recomputes it from the parts and runs the
//!   machines' own hooks; machines read their numbers from it ([`ratings`]).
//! - RPED (`/obj/item/storage/part_replacer`): a storage that holds parts with a rating, machine
//!   and computer boards and up to 30 of glass, plasteel, cable or bluespace crystals. Clicked
//!   on a machine with an open panel it swaps each part for the best better one of the same kind
//!   inside it (`exchange_parts`), the old parts going into it; with the panel closed it lists the
//!   parts. On a wired frame it installs its board, on a frame with a board the parts and stacks
//!   still needed. The bluespace RPED also works on closed panels and from a distance in sight.
//!
//! Cost: parts change only on clicks; [`ratings`] of a machine without [`PartRatings`] reads its
//! board's few default parts.
// shortcut: no base `RefreshParts` power scaling (`idle/active_power_usage * (1 + energy
// ratings)`): mapped machines run on their DM power numbers; add it with per-machine power
// curves. Parts rated by `get_part_rating` are stock parts, cells and vending refills (no beakers
// or assemblies); the RPED splits no stacks (a stack over its 30 quota is refused), has no
// quick gather, no "dump the lowest tier" and no rigged-cell explosions; several boards in an
// RPED on a frame install the first (tg asks with a list).

use bevy::prelude::*;
use ss13_mapload::{DmCircuit, DmItem, DmMachine, DmStack};
use ss13_proto::Prototypes;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::inventory::CarriedBy;
use crate::mecha::core::{ItemCell, MechaDb};
use crate::obj_damage::is_under;
use crate::storage::{Stored, StoredIn};
use crate::tools::tell;

pub const STOCK_PARTS: &str = "/obj/item/stock_parts";
pub const CAPACITOR: &str = "/obj/item/stock_parts/capacitor";
pub const SCANNING_MODULE: &str = "/obj/item/stock_parts/scanning_module";
pub const SERVO: &str = "/obj/item/stock_parts/servo";
pub const MICRO_LASER: &str = "/obj/item/stock_parts/micro_laser";
pub const MATTER_BIN: &str = "/obj/item/stock_parts/matter_bin";
pub const CELL: &str = "/obj/item/stock_parts/power_store";
pub const RPED: &str = "/obj/item/storage/part_replacer";
pub const BLUESPACE_RPED: &str = "/obj/item/storage/part_replacer/bluespace";
const BOARD_ROOT: &str = ss13_mapload::circuit::BOARD_ROOT;
const STACK_ROOT: &str = "/obj/item/stack";
/// `MAX_STACK_PICKUP`.
pub const RPED_STACK_QUOTA: u32 = 30;
/// `allowed_material_types`, `allowed_bluespace_types` (counted together).
const RPED_STACKS: [&str; 3] = [
    "/obj/item/stack/sheet/glass",
    "/obj/item/stack/sheet/plasteel",
    "/obj/item/stack/cable_coil",
];
const RPED_BLUESPACE: [&str; 2] = [
    "/obj/item/stack/ore/bluespace_crystal",
    "/obj/item/stack/sheet/bluespace_crystal",
];

pub struct StockPartsPlugin;

impl Plugin for StockPartsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            ranged_rped
                .after(crate::click::ResolveClicks)
                .before(ss13_tgui::server::UiSystems::Send),
        );
    }
}

/// On a part inside a machine or frame (tg `component_parts`/`components` membership).
#[derive(Component, Debug)]
#[relationship(relationship_target = ComponentParts)]
pub struct PartOf(pub Entity);

/// A machine's or frame's parts, board included, in tg list order (exchanged parts go last).
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = PartOf, linked_spawn)]
pub struct ComponentParts(Vec<Entity>);

impl ComponentParts {
    pub fn parts(&self) -> &[Entity] {
        &self.0
    }
}

/// `/datum/stock_part` kinds (their `physical_object_base_type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    Capacitor,
    ScanningModule,
    Servo,
    MicroLaser,
    MatterBin,
}

const KINDS: [(&str, PartKind); 5] = [
    (CAPACITOR, PartKind::Capacitor),
    (SCANNING_MODULE, PartKind::ScanningModule),
    (SERVO, PartKind::Servo),
    (MICRO_LASER, PartKind::MicroLaser),
    (MATTER_BIN, PartKind::MatterBin),
];

/// The stock part kind of an item type.
pub fn kind(path: &str) -> Option<PartKind> {
    KINDS
        .iter()
        .find(|(root, _)| is_under(path, root))
        .map(|&(_, k)| k)
}

fn kind_root(kind: PartKind) -> &'static str {
    KINDS
        .iter()
        .find(|(_, k)| *k == kind)
        .map_or("", |(r, _)| r)
}

/// What `RefreshParts` reads: the stock parts' tiers in component order and the cells' charge
/// capacities.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PartRatings {
    pub parts: Vec<(PartKind, u32)>,
    /// `maxcharge` of each `/obj/item/stock_parts/power_store`, joules.
    pub cells: Vec<f64>,
}

impl PartRatings {
    pub fn tiers(&self, kind: PartKind) -> impl Iterator<Item = u32> + '_ {
        self.parts
            .iter()
            .filter(move |(k, _)| *k == kind)
            .map(|&(_, t)| t)
    }

    /// `for(var/datum/stock_part/x/part in component_parts) total += part.tier`.
    pub fn sum(&self, kind: PartKind) -> u32 {
        self.tiers(kind).sum()
    }

    /// `for(...) value = part.tier`: the last one's.
    pub fn last(&self, kind: PartKind) -> Option<u32> {
        self.tiers(kind).last()
    }

    /// `for(...) { value = part.tier; break }`.
    pub fn first(&self, kind: PartKind) -> Option<u32> {
        self.tiers(kind).next()
    }

    fn add(&mut self, world: &World, path: &str, cell: Option<f64>) {
        if let Some(k) = kind(path) {
            self.parts.push((k, tier(world, path)));
        } else if is_under(path, CELL) {
            let max = cell.or_else(|| {
                world
                    .get_resource::<MechaDb>()
                    .and_then(|db| db.cell(path))
                    .map(|c| c.maxcharge)
            });
            self.cells.extend(max);
        }
    }
}

/// A stock part type's `rating` (1 when unknown).
pub fn tier(world: &World, path: &str) -> u32 {
    world
        .get_resource::<MechaDb>()
        .map_or(1, |db| db.rating(path))
}

fn path_of(world: &World, item: Entity) -> Option<String> {
    world.get::<DmItem>(item).map(|i| i.path.clone())
}

/// tg `get_part_rating()`: a stock part's tier, a cell's `maxcharge * 10 + charge`, 0 for
/// anything else.
pub fn part_rating(world: &World, item: Entity) -> f64 {
    let Some(path) = path_of(world, item) else {
        return 0.0;
    };
    if kind(&path).is_some() {
        return f64::from(tier(world, &path));
    }
    if is_under(&path, CELL) {
        return world
            .get::<ItemCell>(item)
            .map_or(1.0, |c| c.0.maxcharge * 10.0 + c.0.charge);
    }
    if is_under(&path, crate::vending::refill::REFILL_ROOT) {
        return crate::vending::refill::rating(world, item);
    }
    0.0
}

fn board_def(world: &World, board: &str) -> Option<DmCircuit> {
    world
        .get_resource::<Prototypes>()?
        .get(board)?
        .get::<DmCircuit>()
        .cloned()
}

/// A machine's ratings: [`PartRatings`] if refreshed, else its parts, else its board's default
/// parts (a mapped machine).
pub fn ratings(world: &World, machine: Entity) -> PartRatings {
    if let Some(r) = world.get::<PartRatings>(machine) {
        return r.clone();
    }
    let mut out = PartRatings::default();
    if let Some(parts) = world.get::<ComponentParts>(machine) {
        for &p in parts.parts() {
            if let Some(path) = path_of(world, p) {
                let cell = world.get::<ItemCell>(p).map(|c| c.0.maxcharge);
                out.add(world, &path, cell);
            }
        }
        return out;
    }
    let def = world
        .get::<DmMachine>(machine)
        .and_then(|m| board_def(world, &m.circuit));
    for p in def.iter().flat_map(|d| &d.default_parts) {
        for _ in 0..p.amount {
            out.add(world, &p.path, None);
        }
    }
    out
}

/// tg `RefreshParts()`: the ratings from the machine's parts, then the machines that keep
/// numbers derived from them.
pub fn refresh(world: &mut World, machine: Entity) {
    world.entity_mut(machine).remove::<PartRatings>();
    let r = ratings(world, machine);
    world.entity_mut(machine).insert(r);
    crate::botany::refresh_parts(world, machine);
    crate::atmos_machinery::refresh_thermo_parts(world, machine);
    crate::science::autolathe::refresh_parts(world, machine);
    crate::science::lathe::refresh_parts(world, machine);
    crate::vending::refill::refresh_parts(world, machine);
    crate::chem_dispenser::refresh_parts(world, machine);
}

/// Into `owner`'s parts from wherever it is (a hand, a storage, the floor).
pub fn attach(world: &mut World, item: Entity, owner: Entity) {
    world
        .entity_mut(item)
        .remove::<(CarriedBy, TilePos, StoredIn, PartOf)>()
        .insert(PartOf(owner));
}

/// Out of its owner onto `pos`.
pub fn detach(world: &mut World, item: Entity, pos: TilePos) {
    world.entity_mut(item).remove::<PartOf>();
    crate::storage::take_out(world, item, pos);
}

/// A machine's parts as items: its board and default parts, made the first time a mapped
/// machine needs them (tg makes them in `Initialize`). False without a board.
pub fn ensure_parts(world: &mut World, machine: Entity) -> bool {
    if world
        .get::<ComponentParts>(machine)
        .is_some_and(|p| !p.parts().is_empty())
    {
        return true;
    }
    // A vendor's board carries its brand and its canister is its own type.
    if let Some(done) = crate::vending::refill::ensure_parts(world, machine) {
        return done;
    }
    let Some(board) = world.get::<DmMachine>(machine).map(|m| m.circuit.clone()) else {
        return false;
    };
    let (Some(def), Some(&pos)) = (board_def(world, &board), world.get::<TilePos>(machine)) else {
        return false;
    };
    let mut paths = vec![board];
    for p in &def.default_parts {
        paths.extend(std::iter::repeat_n(p.path.clone(), p.amount as usize));
    }
    for path in paths {
        if let Some(item) = crate::construction::spawn_obj(world, &path, pos) {
            attach(world, item, machine);
        }
    }
    true
}

/// The machine's parts other than its board.
fn non_board_parts(world: &World, owner: Entity) -> Vec<Entity> {
    world
        .get::<ComponentParts>(owner)
        .map(|p| {
            p.parts()
                .iter()
                .copied()
                .filter(|&e| path_of(world, e).is_some_and(|p| !is_under(&p, BOARD_ROOT)))
                .collect()
        })
        .unwrap_or_default()
}

fn board_of(world: &World, owner: Entity) -> Option<String> {
    world
        .get::<ComponentParts>(owner)?
        .parts()
        .iter()
        .find_map(|&e| {
            path_of(world, e).filter(|p| is_under(p, ss13_mapload::circuit::MACHINE_BOARD_ROOT))
        })
}

pub fn is_rped(world: &World, item: Entity) -> bool {
    path_of(world, item).is_some_and(|p| is_under(&p, RPED))
}

fn is_bluespace(world: &World, item: Entity) -> bool {
    path_of(world, item).is_some_and(|p| is_under(&p, BLUESPACE_RPED))
}

/// tg `/datum/storage/rped/can_insert` + the stack quota of `attempt_insert`: parts with a
/// rating, machine and computer boards, and the allowed stacks up to 30 of a kind.
pub fn rped_accepts(world: &World, rped: Entity, item: Entity) -> bool {
    let Some(path) = path_of(world, item) else {
        return false;
    };
    if part_rating(world, item) > 0.0 {
        return true;
    }
    if is_under(&path, ss13_mapload::circuit::MACHINE_BOARD_ROOT)
        || is_under(&path, ss13_mapload::circuit::COMPUTER_BOARD_ROOT)
    {
        return true;
    }
    if !is_under(&path, STACK_ROOT) {
        return false;
    }
    let group = |p: &str| -> Option<&'static str> {
        if RPED_BLUESPACE.iter().any(|r| is_under(p, r)) {
            return Some(RPED_BLUESPACE[0]);
        }
        RPED_STACKS.iter().copied().find(|r| is_under(p, r))
    };
    let Some(mine) = group(&path) else {
        return false;
    };
    let present: u32 = world
        .get::<Stored>(rped)
        .map(|s| {
            s.items()
                .iter()
                .filter(|&&e| path_of(world, e).as_deref().and_then(group) == Some(mine))
                .map(|&e| crate::tools::stack_amount(world, e))
                .sum()
        })
        .unwrap_or(0);
    // shortcut: tg splits the stack to the quota; here it goes in whole or not at all.
    present + crate::tools::stack_amount(world, item) <= RPED_STACK_QUOTA
}

/// tg `get_sorted_parts`: the RPED's contents best first (`cmp_rped_sort`, stable), without
/// boards (and stacks).
fn sorted_parts(world: &World, rped: Entity, ignore_stacks: bool) -> Vec<Entity> {
    let mut parts: Vec<(Entity, f64)> = world
        .get::<Stored>(rped)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| {
            let path = path_of(world, e)?;
            if is_under(&path, BOARD_ROOT) || (ignore_stacks && is_under(&path, STACK_ROOT)) {
                return None;
            }
            Some((e, part_rating(world, e)))
        })
        .collect();
    parts.sort_by(|a, b| b.1.total_cmp(&a.1));
    parts.into_iter().map(|(e, _)| e).collect()
}

fn name(world: &World, e: Entity) -> String {
    world
        .get::<ss13_mapload::DmName>(e)
        .map_or_else(String::new, |n| {
            crate::obj_damage::strip(&n.name).to_owned()
        })
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

/// tg `play_rped_effect`.
fn rped_sound(world: &mut World, rped: Entity, at: TilePos) {
    let sound = if is_bluespace(world, rped) {
        "sound/items/pshoom/pshoom.ogg"
    } else {
        "sound/items/tools/rped.ogg"
    };
    crate::sound::playsound(world, crate::sound::PlaySound::new(at, sound, 40.0, true));
}

/// tg `display_parts`: what is inside, stacks counted from the board.
pub fn display_parts(world: &World, machine: Entity) -> String {
    let mut counts: Vec<(String, u32)> = Vec::new();
    let mut bump = |name: String, n: u32| match counts.iter_mut().find(|(k, _)| *k == name) {
        Some((_, c)) => *c += n,
        None => counts.push((name, n)),
    };
    let board = match world.get::<ComponentParts>(machine) {
        Some(parts) => {
            for &p in parts.parts() {
                bump(name(world, p), 1);
            }
            board_of(world, machine)
        }
        None => {
            let board = world.get::<DmMachine>(machine).map(|m| m.circuit.clone());
            if let Some(b) = &board {
                bump(crate::machine_frame::type_name(world, b), 1);
                for p in board_def(world, b).iter().flat_map(|d| &d.default_parts) {
                    bump(crate::machine_frame::type_name(world, &p.path), p.amount);
                }
            }
            board
        }
    };
    for req in board
        .and_then(|b| board_def(world, &b))
        .iter()
        .flat_map(|d| d.req_components.clone())
        .filter(|r| is_under(&r.path, STACK_ROOT))
    {
        bump(
            crate::machine_frame::type_name(world, &req.path),
            req.amount,
        );
    }
    let mut text = String::from("Внутри имеются следующие компоненты:");
    for (n, c) in counts {
        text.push_str(&format!("\n{c} {n}."));
    }
    text
}

/// tg `exchange_parts`: swaps the machine's parts for better ones from the RPED. True when the
/// RPED was used on it (tg `ITEM_INTERACT_SUCCESS`).
pub fn exchange_parts(world: &mut World, client: Entity, rped: Entity, machine: Entity) -> bool {
    let bluespace = is_bluespace(world, rped);
    if !crate::machine_frame::panel_open(world, machine) && !bluespace {
        let text = display_parts(world, machine);
        tell(world, client, Span::Notice, text);
        return false;
    }
    if bluespace {
        let text = display_parts(world, machine);
        tell(world, client, Span::Notice, text);
    }
    if !ensure_parts(world, machine) {
        return false;
    }
    let Some(board) = board_of(world, machine).and_then(|b| board_def(world, &b)) else {
        return false;
    };
    let mut stock = sorted_parts(world, rped, true);
    if stock.is_empty() {
        return false;
    }
    let mut swapped = false;
    for primary in non_board_parts(world, machine) {
        if stock.is_empty() {
            break;
        }
        let Some(primary_path) = path_of(world, primary) else {
            continue;
        };
        let current = part_rating(world, primary);
        // A stock part takes its kind; anything else the `req_components` entry it fills.
        let required = match kind(&primary_path) {
            Some(k) => kind_root(k).to_owned(),
            None => match board
                .req_components
                .iter()
                .find(|r| is_under(&primary_path, &r.path))
            {
                Some(r) => r.path.clone(),
                None => continue,
            },
        };
        let better = stock.iter().position(|&s| {
            path_of(world, s).is_some_and(|p| is_under(&p, &required))
                && part_rating(world, s) > current
        });
        let Some(i) = better else {
            continue;
        };
        let secondary = stock.remove(i);
        let (old, new) = (name(world, primary), name(world, secondary));
        attach(world, secondary, machine);
        // `attempt_insert(override = TRUE, force = STORAGE_SOFT_LOCKED)`: past its limits.
        world.entity_mut(primary).remove::<PartOf>();
        crate::storage::put_inside(world, primary, rped);
        tell(
            world,
            client,
            Span::Notice,
            format!("{} заменяется на {new}.", capitalize(&old)),
        );
        swapped = true;
    }
    refresh(world, machine);
    if swapped && let Some(&pos) = world.get::<TilePos>(machine) {
        rped_sound(world, rped, pos);
    }
    true
}

/// tg `install_parts_from_part_replacer` of a machine frame: what `needed` still asks for,
/// best parts first. Returns the new `needed` and whether anything went in.
pub fn install_into_frame(
    world: &mut World,
    client: Entity,
    rped: Entity,
    frame: Entity,
    needed: &mut [ss13_mapload::CircuitPart],
) -> bool {
    if needed.iter().all(|n| n.amount == 0) {
        return false;
    }
    let mut stock = sorted_parts(world, rped, false);
    let mut added = false;
    for slot in needed.iter_mut() {
        while slot.amount > 0 {
            let found = stock.iter().position(|&s| {
                path_of(world, s).is_some_and(|p| {
                    is_under(&p, &slot.path)
                        || (is_under(&slot.path, RPED_BLUESPACE[0])
                            && is_under(&p, RPED_BLUESPACE[1]))
                })
            });
            let Some(i) = found else {
                break;
            };
            let part = stock.remove(i);
            if world.get::<crate::tools::Stack>(part).is_some() {
                let singular = world
                    .get::<DmStack>(part)
                    .map(|s| s.singular_name.clone())
                    .unwrap_or_default();
                let used = crate::tools::stack_amount(world, part).min(slot.amount);
                if used == 0 || !crate::tools::use_stack(world, part, used) {
                    continue;
                }
                slot.amount -= used;
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("Вы добавляете {used} {singular} в раму."),
                );
            } else {
                let n = name(world, part);
                attach(world, part, frame);
                slot.amount -= 1;
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("Вы добавляете {n} в раму."),
                );
            }
            added = true;
        }
    }
    if added && let Some(&pos) = world.get::<TilePos>(frame) {
        rped_sound(world, rped, pos);
    }
    true
}

/// The first board in the RPED whose type fits the frame (`install_circuit_from_part_replacer`).
pub fn board_in(world: &World, rped: Entity, computer: bool) -> Option<Entity> {
    let root = if computer {
        ss13_mapload::circuit::COMPUTER_BOARD_ROOT
    } else {
        ss13_mapload::circuit::MACHINE_BOARD_ROOT
    };
    world
        .get::<Stored>(rped)?
        .items()
        .iter()
        .copied()
        .find(|&e| path_of(world, e).is_some_and(|p| is_under(&p, root)))
}

/// The bluespace RPED's `ranged_interact_with_atom`: a machine in sight past reach.
pub(crate) fn ranged_rped(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::click::RangedClick>>,
) {
    let clicks: Vec<crate::click::RangedClick> = cursor
        .read(world.resource::<Messages<crate::click::RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(rped) = world
            .get::<crate::inventory::Inventory>(click.mob)
            .and_then(|inv| inv.active_item())
            .filter(|&i| is_bluespace(world, i))
        else {
            continue;
        };
        if world
            .get::<crate::combat::CombatMode>(click.mob)
            .is_some_and(|c| c.0)
        {
            continue;
        }
        // `ranged_interact_with_atom` is reached for atoms in view (`world.view` = 7).
        if !crate::mobs::ai::behaviors::can_see(world, click.mob_pos, click.target_pos, 7) {
            continue;
        }
        crate::machine_frame::rped_interaction(world, click.client, rped, click.target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratings_follow_tg_loops() {
        let r = PartRatings {
            parts: vec![
                (PartKind::MatterBin, 2),
                (PartKind::Servo, 1),
                (PartKind::MatterBin, 3),
            ],
            cells: vec![],
        };
        assert_eq!(r.sum(PartKind::MatterBin), 5);
        assert_eq!(r.first(PartKind::MatterBin), Some(2));
        assert_eq!(r.last(PartKind::MatterBin), Some(3));
        assert_eq!(r.last(PartKind::Capacitor), None);
        assert_eq!(
            kind("/obj/item/stock_parts/servo/femto"),
            Some(PartKind::Servo)
        );
        assert_eq!(kind("/obj/item/stock_parts/power_store/cell"), None);
    }

    /// The `RefreshParts` formulas of the machines that read [`PartRatings`] at use time.
    #[test]
    fn machine_formulas() {
        let r = |parts: Vec<(PartKind, u32)>, cells: Vec<f64>| PartRatings { parts, cells };
        // Recharge station: `5e-3 * tier` per capacitor times the cell's maxcharge.
        let stock = r(
            vec![(PartKind::Capacitor, 1), (PartKind::Capacitor, 1)],
            vec![100_000.0],
        );
        assert!((crate::silicon::cyborg::recharge_speed(&stock) - 1000.0).abs() < 1e-9);
        let best = r(
            vec![(PartKind::Capacitor, 4), (PartKind::Capacitor, 4)],
            vec![100_000.0],
        );
        assert!((crate::silicon::cyborg::recharge_speed(&best) - 4000.0).abs() < 1e-9);
        // Microwave: `max(12 - 2 * efficiency, 2)` ds.
        assert!((crate::kitchen::microwave_cycle_secs(1.0) - 1.0).abs() < 1e-9);
        assert!((crate::kitchen::microwave_cycle_secs(4.0) - 0.4).abs() < 1e-9);
        assert!((crate::kitchen::microwave_cycle_secs(6.0) - 0.2).abs() < 1e-9);
        // Thermomachine: tier 4 bins and lasers.
        let mut m = crate::atmos_machinery::ThermoMachine::tier1(140.0, 170.0, false);
        assert_eq!(m.heat_capacity, 5000.0);
        m.set_parts(8.0, 8.0, 140.0, 170.0);
        assert_eq!(m.heat_capacity, 5000.0 * 49.0);
        assert_eq!(m.min_temperature, ss13_atmos::gas::TCMB);
        assert!((m.max_temperature - (293.15 + 140.0 * 8.0)).abs() < 1e-9);
    }

    /// A constructable tray's `RefreshParts`: water and nutrient capacity, servo rating.
    #[test]
    fn tray_refresh() {
        use crate::botany::Tray;
        use crate::chem::Reagents;
        let mut world = World::new();
        let holder = ss13_reagents::Holder::new(
            20.0,
            0,
            ss13_reagents::Owner {
                path: String::new(),
                living: false,
            },
        );
        let tray = world
            .spawn((
                ss13_mapload::DmObj {
                    path: "/obj/machinery/hydroponics/constructable".into(),
                    ..default()
                },
                Tray {
                    rating: 1.0,
                    ..default()
                },
                Reagents(holder),
                PartRatings {
                    parts: vec![
                        (PartKind::MatterBin, 4),
                        (PartKind::MatterBin, 4),
                        (PartKind::Servo, 3),
                    ],
                    cells: vec![],
                },
            ))
            .id();
        crate::botany::refresh_parts(&mut world, tray);
        let t = world.get::<Tray>(tray).unwrap();
        assert_eq!((t.maxwater, t.maxnutri, t.rating), (400.0, 50.0, 3.0));
        assert!((t.nutridrain - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(world.get::<Reagents>(tray).unwrap().0.maximum_volume, 50.0);
    }
}
