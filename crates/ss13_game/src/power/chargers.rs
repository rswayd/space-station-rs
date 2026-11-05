//! Cell chargers and rechargers (server), after tg `/obj/machinery/cell_charger`
//! (`code/game/machinery/cell_charger.dm`) and `/obj/machinery/recharger`
//! (`code/game/machinery/recharger.dm`).
//!
//! - Cell charger: a power cell clicked onto it goes in (it must be anchored, empty, and its
//!   area must have equipment power); an empty hand takes it out. Every machine tick it charges
//!   the cell by `charge_rate` (0.25 × `STANDARD_CELL_RATE` × capacitor tier: 2.5 kW at tier 1) from the
//!   grid's surplus only (`charge_cell(grid_only = TRUE)`) and pays 1% of that plus its active
//!   load as heat loss. Its overlays show the charge in quarters, the cell, the connector and
//!   the cell's charge light.
//! - Recharger: a stun baton or an energy gun clicked onto it goes in; an empty hand takes it out. Every machine
//!   tick its cell gets `chargerate × recharge_coeff` (the capacitor tier) from the grid, then
//!   the APC cell;
//!   when full the recharger says so; its overlay shows empty, charging or full.
//!
//! Cost: O(chargers) per machine tick (0.5 Hz); MetaStation maps ~20 of each.
// shortcut: no wrench (unanchoring), telekinesis or sounds; the recharger takes only stun
// batons (no energy guns, recharge magazines, modular computers or battle rifles exist yet).
// shortcut: the "charged" line uses the neuter ending (`genderize_ru` of the DM `gender`, which
// objects leave `NEUTER`); use the ru_names gender once names carry it.

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmBaton, DmName};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, EMISSIVE_PLANE, Overlay, TilePos};

use super::{
    AreaPower, Cell, Channel, MACHINE_TICK_SECS, MachineTicks, Unpowered, charge_cell,
    machine_use_energy,
};
use crate::inventory::Inventory;
use crate::mecha::core::ItemCell;
use crate::obj_damage::Anchored;
use crate::security::Baton;
use crate::tools::tell;

pub const CELL_CHARGER: &str = "/obj/machinery/cell_charger";
pub const RECHARGER: &str = "/obj/machinery/recharger";
const CELL_CHARGER_DMI: &str = "icons/obj/machines/cell_charger.dmi";
const RECHARGER_DMI: &str = "icons/obj/machines/sec.dmi";
/// `STANDARD_CELL_RATE`, watts.
pub const STANDARD_CELL_RATE: f64 = 10_000.0;
/// `cell_charger` `charge_rate` with a tier 1 capacitor.
pub const CELL_CHARGER_RATE: f64 = 0.25 * STANDARD_CELL_RATE;

/// tg `cell_charger/RefreshParts`: `0.25 * STANDARD_CELL_RATE` times each capacitor's tier.
pub fn cell_charger_rate(world: &World, charger: Entity) -> f64 {
    crate::stock_parts::ratings(world, charger)
        .tiers(crate::stock_parts::PartKind::Capacitor)
        .fold(CELL_CHARGER_RATE, |rate, tier| rate * f64::from(tier))
}

/// tg `recharger/RefreshParts`: `recharge_coeff` is the (last) capacitor's tier, else 1.
pub fn recharge_coeff(world: &World, recharger: Entity) -> f64 {
    crate::stock_parts::ratings(world, recharger)
        .last(crate::stock_parts::PartKind::Capacitor)
        .map_or(1.0, f64::from)
}
/// `power_store` `charge_light_type` and `connector_type` of every plain cell.
const CHARGE_LIGHT: &str = "standard";
const CONNECTOR: &str = "standard";

/// `/obj/machinery/cell_charger`: `charging`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CellCharger {
    pub charging: Option<Entity>,
}

/// `/obj/machinery/recharger`: `charging` and `using_power`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Recharger {
    pub charging: Option<Entity>,
    pub using_power: bool,
}

pub struct ChargersPlugin;

impl Plugin for ChargersPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` of cell chargers and rechargers: the cell or device out.
        app.add_click_handler(ClickPhase::EmptyHand, 1900, |w, c| {
            crate::power::chargers::interact(w, c.client, c.mob, c.target)
        });
        // tg `item_interaction` of cell chargers (cells) and rechargers (batons).
        app.add_click_handler(ClickPhase::HeldOnTarget, 2800, |w, c| {
            c.held.is_some_and(|h| {
                crate::power::chargers::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.add_systems(Startup, init_chargers.after(super::init_power))
            .add_systems(
                Update,
                (process_chargers, (cell_charger_looks, recharger_looks))
                    .chain()
                    .after(super::PowerSystems),
            );
    }
}

fn init_chargers(world: &mut World) {
    let mut query = world.query::<(Entity, &super::DmPower)>();
    let found: Vec<Entity> = query
        .iter(world)
        .filter(|(_, dm)| is_under(&dm.path, CELL_CHARGER) || is_under(&dm.path, RECHARGER))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// The live state of a cell charger or recharger (map load and `crate::machine_frame`).
pub(crate) fn setup(world: &mut World, e: Entity) {
    let Some(path) = world.get::<super::DmPower>(e).map(|d| d.path.clone()) else {
        return;
    };
    if is_under(&path, CELL_CHARGER) {
        world.entity_mut(e).insert(CellCharger::default());
    } else if is_under(&path, RECHARGER) {
        world.entity_mut(e).insert(Recharger::default());
    }
}

fn name_of(world: &World, e: Entity) -> String {
    world.get::<DmName>(e).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    })
}

fn anchored(world: &World, e: Entity) -> bool {
    world.get::<Anchored>(e).is_none_or(|a| a.0)
}

/// tg `area.power_equip` of the machine's area.
fn area_has_equipment_power(world: &World, machine: Entity) -> bool {
    super::machine_power(world, machine)
        .and_then(|pm| world.get::<AreaPower>(pm.area))
        .is_some_and(|a| a.powered(Channel::Equip))
}

/// A recharger takes `/obj/item/melee/baton/security` and `/obj/item/gun/energy` (tg
/// `allowed_devices`).
fn rechargeable(world: &World, item: Entity) -> bool {
    world.get::<DmBaton>(item).is_some_and(|b| b.uses_cell)
        || crate::guns::energy_cell(world, item).is_some()
}

/// tg `item_interaction` of the cell charger and the recharger. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if let Some(charger) = world.get::<CellCharger>(target).copied() {
        if world.get::<ItemCell>(held).is_none() {
            return false;
        }
        let name = name_of(world, target);
        if !anchored(world, target) {
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} isn't attached to the ground!"),
            );
        } else if charger.charging.is_some() {
            tell(
                world,
                client,
                Span::Warning,
                "There is already a cell in the charger!",
            );
        } else if !area_has_equipment_power(world, target) {
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} blinks red as you try to insert the cell!"),
            );
        } else {
            crate::storage::put_inside(world, held, target);
            world.entity_mut(target).insert(CellCharger {
                charging: Some(held),
            });
            tell(
                world,
                client,
                Span::Notice,
                format!("You insert a cell into {name}."),
            );
        }
        return true;
    }
    if let Some(recharger) = world.get::<Recharger>(target).copied() {
        if !rechargeable(world, held) {
            return false;
        }
        let name = name_of(world, target);
        if !anchored(world, target) {
            tell(
                world,
                client,
                Span::Notice,
                format!("{} не подключён ни к чему!", capitalize(&name)),
            );
        } else if recharger.charging.is_some() {
            // `ITEM_INTERACT_BLOCKING` without a message.
        } else if world
            .get::<ss13_mapload::DmGun>(held)
            .is_some_and(|g| g.energy && !g.can_charge)
        {
            tell(
                world,
                client,
                Span::Notice,
                "Ваше оружие не имеет внешнего разъёма питания.",
            );
        } else if !area_has_equipment_power(world, target) {
            let item = name_of(world, held);
            tell(
                world,
                client,
                Span::Notice,
                format!(
                    "{} мигает красным при попытке вставить {item}.",
                    capitalize(&name)
                ),
            );
        } else {
            crate::storage::put_inside(world, held, target);
            // `Entered`: `update_use_power(ACTIVE_POWER_USE)`.
            world.entity_mut(target).insert(Recharger {
                charging: Some(held),
                using_power: true,
            });
            super::update_use_power(world, target, true);
        }
        return true;
    }
    let _ = mob;
    false
}

/// tg `attack_hand`: the cell or the device comes out into the user's hands. True when the
/// click was theirs.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if let Some(charger) = world.get::<CellCharger>(target).copied() {
        let Some(cell) = charger.charging else {
            return true;
        };
        let (name, cell_name) = (name_of(world, target), name_of(world, cell));
        world.entity_mut(target).insert(CellCharger::default());
        give_back(world, mob, target, cell);
        tell(
            world,
            client,
            Span::Notice,
            format!("You remove {cell_name} from {name}."),
        );
        return true;
    }
    if let Some(recharger) = world.get::<Recharger>(target).copied() {
        if let Some(item) = recharger.charging {
            world.entity_mut(target).insert(Recharger::default());
            super::update_use_power(world, target, false);
            give_back(world, mob, target, item);
        }
        return true;
    }
    false
}

/// `user.put_in_hands(...)`, else onto the machine's tile (`drop_location`).
fn give_back(world: &mut World, mob: Entity, machine: Entity, item: Entity) {
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, item);
    } else if let Some(&pos) = world.get::<TilePos>(machine) {
        crate::storage::take_out(world, item, pos);
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

/// One machine tick of every charger, after the APCs (tg `SSmachines` processes machines in
/// one list; the order among them does not matter for these).
fn process_chargers(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    for _ in from..ticks {
        let mut q = world.query::<(Entity, &CellCharger, Has<Unpowered>)>();
        let chargers: Vec<(Entity, Entity)> = q
            .iter(world)
            .filter(|(_, _, unpowered)| !unpowered)
            .filter_map(|(e, c, _)| Some((e, c.charging?)))
            .collect();
        for (charger, cell) in chargers {
            process_cell_charger(world, charger, cell, MACHINE_TICK_SECS);
        }
        let mut q = world.query::<(Entity, &Recharger, Has<Unpowered>)>();
        let rechargers: Vec<(Entity, Entity)> = q
            .iter(world)
            .filter(|(_, _, unpowered)| !unpowered)
            .filter_map(|(e, r, _)| Some((e, r.charging?)))
            .collect();
        for (recharger, item) in rechargers {
            process_recharger(world, recharger, item, MACHINE_TICK_SECS);
        }
    }
}

/// tg `cell_charger/process`.
pub fn process_cell_charger(world: &mut World, charger: Entity, cell: Entity, dt: f64) {
    let Some(mut state) = world.get::<ItemCell>(cell).map(|c| c.0) else {
        return;
    };
    if state.percent() >= 100.0 || !anchored(world, charger) {
        return;
    }
    let rate = cell_charger_rate(world, charger);
    let given = charge_cell(world, charger, rate * dt, &mut state, true);
    if given > 0.0 {
        let active = world
            .get::<super::DmPower>(charger)
            .map_or(0.0, |d| d.active_power_usage);
        machine_use_energy(world, charger, (given + active) * 0.01, false);
    }
    world.entity_mut(cell).insert(ItemCell(state));
}

/// tg `recharger/process` for a device with a cell.
pub fn process_recharger(world: &mut World, recharger: Entity, item: Entity, dt: f64) {
    let device = match (world.get::<Baton>(item), world.get::<DmBaton>(item)) {
        (Some(baton), Some(dm)) => baton
            .charge
            .map(|c| (c, dm.cell_maxcharge, dm.cell_chargerate)),
        _ => crate::guns::energy_cell(world, item),
    };
    let Some((charge, maxcharge, rate)) = device else {
        world.entity_mut(recharger).insert(Recharger {
            charging: Some(item),
            using_power: false,
        });
        return;
    };
    let mut using_power = false;
    if charge < maxcharge {
        let mut cell = Cell {
            charge,
            maxcharge,
            chargerate: rate,
        };
        let coeff = recharge_coeff(world, recharger);
        charge_cell(world, recharger, rate * coeff * dt, &mut cell, false);
        if let Some(mut b) = world.get_mut::<Baton>(item) {
            b.charge = Some(cell.charge);
        } else if let Some(mut g) = world.get_mut::<crate::guns::Gun>(item) {
            g.charge = cell.charge;
        }
        if cell.charge >= cell.maxcharge {
            let name = capitalize(&name_of(world, item));
            crate::cargo::console::machine_say(world, recharger, &format!("{name} заряжено!"));
        } else {
            using_power = true;
        }
    }
    world.entity_mut(recharger).insert(Recharger {
        charging: Some(item),
        using_power,
    });
}

/// tg `cell_charger/update_overlays`, when the charger, its power or its cell's charge changed.
// Cost: one pass over the chargers per tick (MetaStation: ~20), work only for changed ones.
fn cell_charger_looks(
    mut chargers: Query<(Ref<CellCharger>, &mut Appearance, Option<Ref<Unpowered>>)>,
    mut removed: RemovedComponents<Unpowered>,
    cells: Query<(Ref<ItemCell>, &Appearance), Without<CellCharger>>,
    icons: Option<Res<IconIndex>>,
) {
    let Some(icons) = icons else {
        return;
    };
    let repowered: Vec<Entity> = removed.read().collect();
    for (charger, mut appearance, unpowered) in &mut chargers {
        let cell = charger.charging.and_then(|c| cells.get(c).ok());
        let dirty = charger.is_changed()
            || unpowered.as_ref().is_some_and(|u| u.is_added())
            || cell.as_ref().is_some_and(|(c, _)| c.is_changed())
            || (unpowered.is_none() && !repowered.is_empty());
        if !dirty {
            continue;
        }
        let mut new = Vec::new();
        if let Some((cell, look)) = cell {
            let get = |state: String| icons.get(CELL_CHARGER_DMI, &state).map(Overlay::new);
            let percent = cell.0.percent();
            if unpowered.is_none() {
                // DM `round(x)` with one argument floors.
                new.extend(get(format!(
                    "ccharger-o{}",
                    (percent * 4.0 / 100.0).floor() as i32
                )));
            }
            new.push(Overlay::new(look.icon_state));
            new.extend(get(format!("ccharger-{CONNECTOR}-on")));
            if cell.0.charge > 0.01 {
                let level = if percent >= 99.5 { 2 } else { 1 };
                new.extend(get(format!("cell-{CHARGE_LIGHT}-o{level}")));
            }
        }
        if appearance.overlays != new {
            appearance.overlays = new;
        }
    }
}

/// tg `recharger/update_overlays`: `recharger-empty`, `-charging` or `-full` (with its
/// emissive) while powered.
fn recharger_looks(
    mut rechargers: Query<(Ref<Recharger>, &mut Appearance, Option<Ref<Unpowered>>)>,
    mut removed: RemovedComponents<Unpowered>,
    icons: Option<Res<IconIndex>>,
) {
    let Some(icons) = icons else {
        return;
    };
    let repowered = removed.read().count() > 0;
    for (recharger, mut appearance, unpowered) in &mut rechargers {
        let dirty = recharger.is_changed()
            || unpowered.as_ref().is_some_and(|u| u.is_added())
            || (unpowered.is_none() && repowered);
        if !dirty {
            continue;
        }
        let state = match (
            unpowered.is_some(),
            recharger.charging,
            recharger.using_power,
        ) {
            (true, ..) => None,
            (false, None, _) => Some("recharger-empty"),
            (false, Some(_), true) => Some("recharger-charging"),
            (false, Some(_), false) => Some("recharger-full"),
        };
        let new = state
            .and_then(|s| icons.get(RECHARGER_DMI, s))
            .map_or_else(Vec::new, |id| {
                vec![
                    Overlay::new(id),
                    Overlay {
                        plane: Some(EMISSIVE_PLANE),
                        ..Overlay::new(id)
                    },
                ]
            });
        if appearance.overlays != new {
            appearance.overlays = new;
        }
    }
}
