//! The stacking machine and its console (server), after tg
//! `/obj/machinery/mineral/stacking_machine` and `stacking_unit_console`
//! (`code/modules/mining/machine_stacking.dm`) with tgui's `StackingConsole`.
//!
//! - Sheets and bluespace crystals that land on the machine's input tile (`input_dir`, east)
//!   are taken: into the ore silo of its level when their materials are silo materials (what
//!   does not fit waits on the tile), else stacked inside by `merge_type`; every `stack_amt`
//!   (50) sheets of a kind go out onto the output tile (`output_dir`, west).
//! - The console links at round start to the first unlinked stacking machine in its area; a
//!   multitool saves the console and links a machine to it. Its window shows the stored sheets
//!   (`release` ejects all of a kind) and turns the input or output a quarter
//!   counterclockwise (`rotate`; never onto each other).
// shortcut: the stacking machine is not taken apart or built (no machine family), its silo
// link is only the round-start one, and it does not pause on silo holds.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{Areas, DmItem, DmName, DmObj, DmStack, DmStacker};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use super::orm::{dir_text, silo_on};
use super::{
    STACK_INPUT, STACK_RELEASE, STACK_ROTATE, STACK_TYPE, StackingConsoleUi, StackingData,
    StackingSheet,
};
use crate::obj_damage::is_under;
use crate::science::lathe::{LatheSetup, MaterialStore};
use crate::science::techweb::ResearchDb;
use crate::tools::{ToolKind, tell, tool_of};

const CONSOLE: &str = "/obj/machinery/mineral/stacking_unit_console";
/// `accepted_types`.
const ACCEPTED: [&str; 2] = [
    "/obj/item/stack/sheet",
    "/obj/item/stack/ore/bluespace_crystal",
];

/// Sheets of one `merge_type` inside the machine (`stack_list[key]`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoredSheets {
    /// The `merge_type` (the window's `type`).
    pub key: String,
    /// The type released (`inp.type`: the first one stored).
    pub path: String,
    pub name: String,
    pub amount: u32,
}

/// A stacking machine's live state.
#[derive(Component, Clone, Debug, Default)]
pub struct StackingMachine {
    pub input: Dir,
    pub output: Dir,
    pub stack_amt: u32,
    /// The ore silo it feeds (`materials.silo`).
    pub silo: Option<Entity>,
    pub stored: Vec<StoredSheets>,
}

/// A stacking console's linked machine.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct StackingConsole {
    pub machine: Option<Entity>,
}

pub(super) struct StackingPlugin;

impl Plugin for StackingPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<StackingConsoleUi>()
            .add_systems(Startup, setup_stackers.after(LatheSetup))
            .add_systems(
                Update,
                (
                    stack_input.run_if(any_with_component::<StackingMachine>),
                    console_acts.in_set(UiSystems::Handle),
                ),
            );
    }
}

fn setup_stackers(world: &mut World) {
    let mut query = world.query::<(Entity, &DmStacker, Option<&TilePos>)>();
    let machines: Vec<(Entity, DmStacker, Option<TilePos>)> = query
        .iter(world)
        .map(|(e, dm, p)| (e, dm.clone(), p.copied()))
        .collect();
    for (e, dm, pos) in &machines {
        let silo = silo_on(world, pos.map(|p| p.z));
        let input = Dir::from_bits(dm.input_dir).unwrap_or(Dir::East);
        let output = Dir::from_bits(dm.output_dir).unwrap_or(Dir::West);
        world.entity_mut(*e).insert(StackingMachine {
            input,
            output,
            stack_amt: dm.stack_amt.max(1),
            silo,
            stored: Vec::new(),
        });
    }
    // `stacking_unit_console/Initialize`: the first unlinked machine in its area.
    let mut consoles = world.query::<(Entity, &DmObj, &TilePos)>();
    let consoles: Vec<(Entity, TilePos)> = consoles
        .iter(world)
        .filter(|(_, obj, _)| is_under(&obj.path, CONSOLE))
        .map(|(e, _, p)| (e, *p))
        .collect();
    let mut linked: Vec<Entity> = Vec::new();
    for (console, pos) in consoles {
        let area = world.get_resource::<Areas>().and_then(|a| a.area_at(pos));
        let machine = machines.iter().find_map(|(m, _, mpos)| {
            let mpos = (*mpos)?;
            let same = mpos.z == pos.z
                && area.is_some()
                && world.get_resource::<Areas>().and_then(|a| a.area_at(mpos)) == area;
            (same && !linked.contains(m)).then_some(*m)
        });
        if let Some(m) = machine {
            linked.push(m);
        }
        world
            .entity_mut(console)
            .insert(StackingConsole { machine });
    }
}

fn accepted(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| ACCEPTED.iter().any(|root| is_under(&i.path, root)))
}

/// `HasProximity`: what lies on the input tile is stacked.
// Cost: one tile lookup per stacking machine per tick (one or two on a station).
fn stack_input(world: &mut World) {
    let mut query = world.query::<(Entity, &StackingMachine, &TilePos)>();
    let machines: Vec<(Entity, TilePos)> = query
        .iter(world)
        .map(|(e, m, p)| (e, p.step(m.input)))
        .collect();
    for (machine, tile) in machines {
        let items: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(tile)
            .iter()
            .copied()
            .filter(|&e| accepted(world, e))
            .collect();
        for item in items {
            process_stack(world, machine, item);
        }
    }
}

/// `process_stack`.
pub fn process_stack(world: &mut World, machine: Entity, item: Entity) {
    let Some(state) = world.get::<StackingMachine>(machine).cloned() else {
        return;
    };
    let Some(item_path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return;
    };
    let amount = crate::tools::stack_amount(world, item);
    // `materials.insert_item(input)` when the silo takes its materials.
    if let Some(silo) = state
        .silo
        .filter(|&s| world.get::<MaterialStore>(s).is_some())
    {
        let per_unit = world
            .get_resource::<ResearchDb>()
            .and_then(|db| db.stacks.get(&item_path).cloned())
            .unwrap_or_default();
        if !per_unit.is_empty() {
            let taken = world
                .get_mut::<MaterialStore>(silo)
                .map_or(0, |mut s| s.insert_stack(&per_unit, amount));
            if taken > 0 {
                crate::tools::use_stack(world, item, taken);
            }
            return;
        }
    }
    let key = world
        .get::<DmStack>(item)
        .map(|s| s.merge_type.clone())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| item_path.clone());
    let name = world
        .get::<DmName>(item)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.despawn(item);
    let Some(mut m) = world.get_mut::<StackingMachine>(machine) else {
        return;
    };
    let index = match m.stored.iter().position(|s| s.key == key) {
        Some(i) => i,
        None => {
            m.stored.push(StoredSheets {
                key,
                path: item_path.clone(),
                name,
                amount: 0,
            });
            m.stored.len() - 1
        }
    };
    m.stored[index].amount += amount;
    let mut out = 0;
    while m.stored[index].amount >= m.stack_amt {
        m.stored[index].amount -= m.stack_amt;
        out += 1;
    }
    let stack_amt = m.stack_amt;
    for _ in 0..out {
        unload(world, machine, &item_path, stack_amt);
    }
}

/// `unload_mineral` of a new stack onto the output tile.
fn unload(world: &mut World, machine: Entity, path: &str, amount: u32) {
    let (Some(&pos), Some(output)) = (
        world.get::<TilePos>(machine),
        world.get::<StackingMachine>(machine).map(|m| m.output),
    ) else {
        return;
    };
    let max = world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(path))
        .and_then(|p| p.get::<DmStack>())
        .map_or(50, |s| s.max_amount.max(1));
    let mut left = amount;
    while left > 0 {
        let n = left.min(max);
        crate::construction::spawn_stack(world, path, pos.step(output), n);
        left -= n;
    }
}

/// tg `turn(dir, 90)`: a quarter counterclockwise.
fn counterclockwise(dir: Dir) -> Dir {
    match dir {
        Dir::North => Dir::West,
        Dir::West => Dir::South,
        Dir::South => Dir::East,
        Dir::East => Dir::North,
        other => other,
    }
}

/// `stacking_machine/rotate(input)`.
pub fn rotate(m: &mut StackingMachine, input: bool) {
    for _ in 0..4 {
        if input {
            m.input = counterclockwise(m.input);
        } else {
            m.output = counterclockwise(m.output);
        }
        if m.input != m.output {
            return;
        }
    }
}

/// An empty hand on the console opens its window. True when `target` is a stacking console.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<StackingConsole>(target).is_none() {
        return false;
    }
    let name = world
        .get::<DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<StackingConsoleUi>(client, mob, target, name));
    true
}

/// The multitool's `set_buffer` on a console and `multitool_act` on a machine. True when
/// handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if tool_of(world, held).map(|t| t.0) != Some(ToolKind::Multitool) {
        return false;
    }
    if world.get::<StackingConsole>(target).is_some() {
        crate::tools::set_multitool_buffer(world, held, target);
        tell(world, client, Span::Notice, "saved to multitool buffer");
        return true;
    }
    if world.get::<StackingMachine>(target).is_none() {
        return false;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    if combat {
        return false;
    }
    let Some(console) = crate::tools::multitool_buffer(world, held)
        .filter(|&c| world.get::<StackingConsole>(c).is_some())
    else {
        return true;
    };
    // The console's old machine stays unlinked; one machine per console.
    if let Some(mut c) = world.get_mut::<StackingConsole>(console) {
        c.machine = Some(target);
    }
    let name = world
        .get::<DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    tell(
        world,
        client,
        Span::Notice,
        format!("You link {name} to the console in the multitool's buffer."),
    );
    true
}

type StackingParams = Query<'static, 'static, &'static StackingMachine>;

impl ServerUi for StackingConsoleUi {
    type Source = &'static StackingConsole;
    type UserParam = StackingParams;
    type Act = StackingAct;

    fn static_data(_: &StackingConsole) {}

    fn data(
        console: &StackingConsole,
        _: Entity,
        machines: &SystemParamItem<'_, '_, StackingParams>,
    ) -> StackingData {
        let Some(m) = console.machine.and_then(|m| machines.get(m).ok()) else {
            return StackingData::default();
        };
        StackingData {
            machine: true,
            stacking_amount: Some(m.stack_amt),
            input_direction: dir_text(m.input).to_owned(),
            output_direction: dir_text(m.output).to_owned(),
            contents: m
                .stored
                .iter()
                .filter(|s| s.amount > 0)
                .map(|s| StackingSheet {
                    r#type: s.key.clone(),
                    name: crate::cooking::capitalize(&s.name),
                    amount: s.amount,
                })
                .collect(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<StackingAct, UiReject> {
        match action {
            STACK_RELEASE => params
                .get(STACK_TYPE)
                .map(|t| StackingAct::Release(t.to_owned()))
                .ok_or_else(|| UiReject::BadParams("type: expected a sheet type".into())),
            STACK_ROTATE => Ok(StackingAct::Rotate(params.number(STACK_INPUT)? != 0.0)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum StackingAct {
    /// A stored `merge_type`.
    Release(String),
    /// `true`: the input.
    Rotate(bool),
}

fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<StackingConsoleUi>>>) {
    let pending: Vec<(Entity, StackingAct)> = acts
        .read(world.resource::<Messages<UiActed<StackingConsoleUi>>>())
        .map(|a| (a.source, a.act.clone()))
        .collect();
    for (console, act) in pending {
        let Some(machine) = world
            .get::<StackingConsole>(console)
            .and_then(|c| c.machine)
            .filter(|&m| world.get::<StackingMachine>(m).is_some())
        else {
            continue;
        };
        apply(world, machine, act);
    }
}

/// A console act on its machine. `Release` takes only a kind it stores.
pub fn apply(world: &mut World, machine: Entity, act: StackingAct) {
    match act {
        StackingAct::Release(key) => {
            let Some(mut m) = world.get_mut::<StackingMachine>(machine) else {
                return;
            };
            let Some(entry) = m.stored.iter_mut().find(|s| s.key == key) else {
                return;
            };
            let (path, amount) = (entry.path.clone(), entry.amount);
            entry.amount = 0;
            if amount > 0 {
                unload(world, machine, &path, amount);
            }
        }
        StackingAct::Rotate(input) => {
            if let Some(mut m) = world.get_mut::<StackingMachine>(machine) {
                rotate(&mut m, input);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_never_meets() {
        let mut m = StackingMachine {
            input: Dir::East,
            output: Dir::West,
            stack_amt: 50,
            ..default()
        };
        rotate(&mut m, true);
        assert_eq!(m.input, Dir::North);
        rotate(&mut m, true);
        // West is the output: it skips on to south.
        assert_eq!(m.input, Dir::South);
        rotate(&mut m, false);
        // South is the input now: it skips on to east.
        assert_eq!(m.output, Dir::East);
    }
}
