//! A canister's internal cell and powered shielding (`portable/canister.dm`).
//!
//! - Mapped canisters start with a high-capacity cell inside (`Initialize(mapload)`).
//! - Shielding (the `shielding` button) blocks the gas damage over the limits and the outside
//!   heat (`take_atmos_damage`, `should_atmos_process`). Each machine tick it costs
//!   `round(log10(pressure over) + log10(temperature over)) * 250 * dt` joules: from the area's
//!   equipment channel when it has power, else a fortieth of that from the cell; without
//!   either it switches off.
//! - A screwdriver opens the hatch, a cell goes in (swapping the old one into the hand), a
//!   crowbar drops it.
//! - `relabel` takes the name, description and look of another canister type, keeping the gas.
// shortcut: no EMP (the cell's drain and the 75% chance to flip the shielding) and no `recolor`
// (the greyscale menu needs runtime GAGS rendering, see `ss13_mapload::gags`).

use bevy::prelude::*;
use ss13_chat::Span;
use ss13_mapload::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;

use super::portable::{Canister, PortableAtmos};
use crate::mecha::core::ItemCell;
use crate::power::{AreaPower, Channel, MACHINE_TICK_SECS, MachineTicks};
use crate::tools::tell;

pub use ss13_mapload::atmos_machine::CANISTER_LABELS as LABELS;
/// The canister a `relabel` prompt answers for.
#[derive(Component, Clone, Copy, Debug)]
struct RelabelFor(Entity);

const HIGH_CELL: &str = "/obj/item/stock_parts/power_store/cell/high";
const FUSION_TEST: &str = "/obj/machinery/portable_atmospherics/canister/fusion_test";

/// `Initialize(mapload)`: `internal_cell = new /obj/item/stock_parts/power_store/cell/high`.
pub fn init_canister_cells(world: &mut World) {
    let mut q = world.query::<(Entity, &Canister)>();
    let canisters: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.cell.is_none())
        .map(|(e, _)| e)
        .collect();
    for canister in canisters {
        let cell = crate::mecha::core::spawn_inside(world, HIGH_CELL, canister);
        if let Some(mut c) = world.get_mut::<Canister>(canister) {
            c.cell = cell;
        }
    }
}

/// The cell still inside (`Exited` forgets one that left some other way).
pub fn cell_of(world: &World, canister: Entity) -> Option<Entity> {
    let cell = world.get::<Canister>(canister)?.cell?;
    world
        .get::<crate::storage::StoredIn>(cell)
        .is_some_and(|s| s.0 == canister)
        .then_some(cell)
}

/// `internal_cell.percent()`, 0 without one.
pub fn cell_percent(world: &World, canister: Entity) -> f64 {
    cell_of(world, canister)
        .and_then(|c| world.get::<ItemCell>(c))
        .map_or(0.0, |c| c.0.percent())
}

/// `process`: the joules the shielding draws over `dt` seconds (DM one-argument `round` is
/// `floor`).
pub fn shield_energy(pressure: f64, temperature: f64, canister: &Canister, dt: f64) -> f64 {
    let factor = ((pressure - canister.pressure_limit).max(1.0).log10()
        + (temperature - canister.temp_limit).max(1.0).log10())
    .floor();
    factor * 250.0 * dt
}

/// tg `powered(AREA_USAGE_EQUIP, ignore_use_power = TRUE)`.
fn equipment_powered(world: &World, canister: Entity) -> bool {
    crate::power::machine_power(world, canister)
        .and_then(|pm| world.get::<AreaPower>(pm.area))
        .is_some_and(|a| a.powered(Channel::Equip))
}

/// `canister/process` once per machine tick.
pub fn process_canister_shielding(world: &mut World, mut last: Local<Option<u64>>) {
    let Some(ticks) = world.get_resource::<MachineTicks>().map(|t| t.0) else {
        return;
    };
    if last.replace(ticks) == Some(ticks) {
        return;
    }
    let mut q = world.query::<(Entity, &Canister, &PortableAtmos)>();
    let net = world.resource::<ss13_atmos::pipenet::Pipenet>();
    let draws: Vec<(Entity, f64)> = q
        .iter(world)
        .filter(|(_, c, _)| c.shielding)
        .filter_map(|(e, c, p)| {
            let air = p.air(net)?;
            let energy = shield_energy(air.pressure(), air.temperature(), c, MACHINE_TICK_SECS);
            (energy != 0.0).then_some((e, energy))
        })
        .collect();
    for (canister, energy) in draws {
        if equipment_powered(world, canister) {
            crate::power::machine_use_energy(world, canister, energy, false);
            continue;
        }
        let drawn = cell_of(world, canister)
            .and_then(|c| world.get_mut::<ItemCell>(c))
            .is_some_and(|mut c| c.0.take(energy * 0.025, false) > 0.0);
        if !drawn && let Some(mut c) = world.get_mut::<Canister>(canister) {
            c.shielding = false;
            info!("canister {canister:?} shielding turned off due to power loss");
        }
    }
}

/// The canister's cell and hatch tools. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    tool: Option<crate::tools::ToolKind>,
) -> bool {
    use crate::tools::ToolKind;
    let Some(canister) = world.get::<Canister>(target) else {
        return false;
    };
    let open = canister.panel_open;
    if world.get::<ItemCell>(held).is_some() {
        // `item_interaction` with a cell.
        if !open {
            tell(world, client, Span::Warning, "Сначала откройте люк!");
            return true;
        }
        let old = cell_of(world, target);
        crate::storage::put_inside(world, held, target);
        if let Some(old) = old {
            give_to_hands(world, mob, target, old);
        }
        if let Some(mut c) = world.get_mut::<Canister>(target) {
            c.cell = Some(held);
        }
        let text = if old.is_some() {
            "Вы заменяете элемент питания."
        } else {
            "Вы устанавливаете элемент питания."
        };
        tell(world, client, Span::Notice, text);
        return true;
    }
    match tool {
        // `default_deconstruction_screwdriver`.
        Some(ToolKind::Screwdriver) => {
            if let Some(mut c) = world.get_mut::<Canister>(target) {
                c.panel_open = !open;
            }
            let text = if open {
                "панель обслуживания закрыта"
            } else {
                "панель обслуживания открыта"
            };
            tell(world, client, Span::Notice, text);
            true
        }
        // `crowbar_act`: `ITEM_INTERACT_BLOCKING` whatever happens.
        Some(ToolKind::Crowbar) => {
            if open
                && let Some(cell) = cell_of(world, target)
                && let Some(&pos) = world.get::<TilePos>(target)
            {
                crate::storage::take_out(world, cell, pos);
                if let Some(mut c) = world.get_mut::<Canister>(target) {
                    c.cell = None;
                }
                tell(world, client, Span::Notice, "элемент питания извлечён");
            }
            true
        }
        _ => false,
    }
}

/// `user.put_in_hands(...)`, else onto the canister's tile.
fn give_to_hands(world: &mut World, mob: Entity, canister: Entity, item: Entity) {
    if let Some(inventory) = world.get::<crate::inventory::Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, item);
    } else if let Some(&pos) = world.get::<TilePos>(canister) {
        crate::storage::take_out(world, item, pos);
    }
}

/// `ui_act("relabel")`: `tgui_input_list(usr, "New canister label", "Canister", ...)`.
pub fn ask_relabel(world: &mut World, client: Entity, actor: Entity, canister: Entity) {
    let items = LABELS.iter().map(|(l, _)| (*l).to_owned()).collect();
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        ss13_tgui::prompt::Ask::new(
            client,
            actor,
            "Canister",
            "New canister label",
            ss13_tgui::prompt::PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(RelabelFor(canister));
}

/// The answers to [`ask_relabel`]. tg checks nothing after the input; the canister must still
/// be one and within reach, so a prompt left open cannot relabel it from across the station.
pub fn relabel_answers(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<ss13_tgui::prompt::PromptAnswered>>,
) {
    let answers: Vec<(Entity, Entity, Option<ss13_tgui::prompt::PromptAnswer>)> = cursor
        .read(world.resource::<bevy::ecs::message::Messages<ss13_tgui::prompt::PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(&RelabelFor(canister)) = world.get::<RelabelFor>(prompt) else {
            continue;
        };
        let Some(ss13_tgui::prompt::PromptAnswer::Item(i)) = answer else {
            continue;
        };
        if world.get::<Canister>(canister).is_none()
            || !crate::storage::in_reach(world, actor, canister)
        {
            continue;
        }
        if let Some((_, path)) = LABELS.get(i)
            && relabel(world, canister, path)
        {
            info!("canister {canister:?} was relabelled to {path} by {actor:?}");
        }
    }
}

/// `name`, `desc` and `icon_state` of `path` (`initial(replacement.*)`). False without its
/// prototype.
pub fn relabel(world: &mut World, canister: Entity, path: &str) -> bool {
    let Some(proto) = world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(path))
        .cloned()
    else {
        return false;
    };
    let name = proto
        .get::<ss13_mapload::DmName>()
        .cloned()
        .unwrap_or_default();
    let atom = proto
        .get::<ss13_mapload::DmAtom>()
        .cloned()
        .unwrap_or_default();
    // As the loader resolves it (`Template::new`): the exact state, else the icon's default.
    let state = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|index| {
            index
                .0
                .get(&(atom.icon.clone(), atom.icon_state.clone()))
                .or_else(|| index.0.get(&(atom.icon.clone(), String::new())))
                .copied()
        });
    if let Some(mut n) = world.get_mut::<ss13_mapload::DmName>(canister) {
        *n = name;
    }
    if let Some(state) = state
        && let Some(mut a) = world.get_mut::<ss13_shared::Appearance>(canister)
    {
        a.icon_state = state;
    }
    true
}

/// `canister/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(canister) = world.get::<Canister>(target) else {
        return;
    };
    let obj = world.get::<DmObj>(target);
    let damaged = world
        .get::<crate::obj_damage::Integrity>(target)
        .zip(obj)
        .is_some_and(|(i, o)| i.current < o.max_integrity);
    if damaged {
        lines.push((
            Span::Notice,
            "Целостность нарушена, корпус можно залатать сварочным аппаратом.".into(),
        ));
    }
    // `initial(pressure_limit)`: the sticker ignores the random quality.
    let sticker_limit = if obj.is_some_and(|o| is_under(&o.path, FUSION_TEST)) {
        1e14
    } else {
        super::damage::PRESSURE_LIMIT
    };
    lines.push((
        Span::Notice,
        format!(
            "Наклейка на боку гласит: <b>МАКС. БЕЗОПАСНОЕ ДАВЛЕНИЕ: {}; МАКС. БЕЗОПАСНАЯ \
             ТЕМПЕРАТУРА: {}</b>.",
            super::tank::siunit_pressure(sticker_limit),
            crate::power::apc_ui::siunit(canister.temp_limit, "K", 0)
        ),
    ));
    lines.push((
        Span::Notice,
        "Корпус <b>сварен</b>, его можно разрезать.".into(),
    ));
    if cell_of(world, target).is_some() {
        lines.push((
            Span::Notice,
            format!(
                "Заряд внутреннего элемента питания: {}%.",
                crate::chem::dm_num(cell_percent(world, target) as f32)
            ),
        ));
    } else {
        lines.push((
            Span::Notice,
            "Внимание, элемент питания не установлен: откройте люк отвёрткой и вставьте его."
                .into(),
        ));
    }
    if canister.panel_open {
        lines.push((
            Span::Notice,
            "Люк открыт, его можно закрыть отвёрткой.".into(),
        ));
    }
}
