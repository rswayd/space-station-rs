//! Building an exosuit from its chassis (server), after tg `/datum/component/construction`
//! (`datums/components/construction.dm`), `/datum/component/construction/unordered/
//! mecha_chassis` and `/datum/component/construction/mecha` (`mecha_construction_paths.dm`).
//!
//! A chassis item (`/obj/item/mecha_parts/chassis/*`) first takes its parts in any order (each
//! is used up and drawn as a `<part icon_state>+o` overlay); then it becomes the frame
//! (`mech_construction.dmi`, dense) and walks tg's ordered steps (`get_steps`): wrench and
//! screwdriver the hydraulics, 5 cable, wirecutters, the central and peripheral control boards
//! (each screwed in), the scanning module, capacitor, servo and power cell (each screwed in),
//! the inner plating (wrench, weld) and the outer plating (wrench, weld; the Ripley's cockpit:
//! 10 rods, weld). A step's `back_key` undoes the previous step and gives back what it used.
//! Each step shows `<base_icon><n>`. After the last one the mech is built with the frame's
//! contents (the parts and the cell; `spawn_result`).
//!
//! Tool steps are tg's `use_tool(parent, user, 0)`: instant, a welder must be lit; stacks need
//! the step's amount.
// shortcut: only chassis whose construction keeps tg's default steps (or the Ripley's cockpit
// override) are built; the Clarke, Gygax, Durand, Phazon, H.O.N.K. and Savannah-Ivanov frames
// (their own step procs, BandaStation's cores) take their parts but go no further. The step
// description is not added to the chassis' examine text.

use bevy::prelude::*;
use ss13_mapload::mecha::ConstructionDef;
use ss13_mapload::{DmAtom, DmItem, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_world::Density;

use super::core::{MechaDb, init_mech, name_of, visible};
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::storage::{Stored, put_inside, take_out};
use crate::tools::{Actor, ToolKind, tell, tool_of};

const FRAME_DMI: &str = "icons/mob/rideables/mech_construction.dmi";
const CABLE: &str = "/obj/item/stack/cable_coil";
const SCANNING_MODULE: &str = "/obj/item/stock_parts/scanning_module";
const CAPACITOR: &str = "/obj/item/stock_parts/capacitor";
const SERVO: &str = "/obj/item/stock_parts/servo";
const CELL: &str = "/obj/item/stock_parts/power_store/cell";
const SHEET: &str = "/obj/item/stack/sheet";
const STACK: &str = "/obj/item/stack";

/// What a step takes: a tool behaviour or an item type (and its subtypes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Tool(ToolKind),
    Item(String),
}

/// What a forward step does with the item (tg `action`; stacks use their `amount`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepAction {
    /// A tool.
    Use,
    /// `ITEM_DELETE`.
    Delete,
    /// `ITEM_MOVE_INSIDE`.
    MoveInside,
    /// A stack of this many.
    Stack(u32),
}

/// One `steps` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub key: Key,
    pub back_key: Option<Key>,
    pub action: StepAction,
    pub desc: String,
    pub forward: String,
    pub backward: String,
    pub icon_state: String,
}

fn step(
    key: Key,
    back: Option<Key>,
    action: StepAction,
    desc: &str,
    forward: &str,
    backward: &str,
) -> Step {
    Step {
        key,
        back_key: back,
        action,
        desc: desc.to_owned(),
        forward: forward.to_owned(),
        backward: backward.to_owned(),
        icon_state: String::new(),
    }
}

use Key::Tool as T;
use ToolKind::{Crowbar, Screwdriver, Welder, Wirecutters, Wrench};

fn item(path: &str) -> Key {
    Key::Item(path.to_owned())
}

/// The plating step's action: a sheet stack by amount, else the item itself.
fn plating_action(path: &str, amount: u32) -> StepAction {
    if is_under(path, SHEET) {
        StepAction::Stack(amount)
    } else {
        StepAction::Delete
    }
}

/// tg `get_steps` of a construction, `None` when its type has step procs not ported here.
pub fn steps(def: &ConstructionDef) -> Option<Vec<Step>> {
    let ripley = def.path.ends_with("/ripley");
    let ported = def.procs.is_empty() || (ripley && def.procs == ["get_outer_plating_steps"]);
    if !ported {
        return None;
    }
    let mut all = vec![
        step(
            T(Wrench),
            None,
            StepAction::Use,
            "The hydraulic systems can be connected with a <b>wrench</b>.",
            "connected the hydraulic systems",
            "",
        ),
        step(
            T(Screwdriver),
            Some(T(Wrench)),
            StepAction::Use,
            "The hydraulic systems are connected, and can be activated with a <b>screwdriver</b>.",
            "activated the hydraulic systems",
            "disconnected the hydraulic systems",
        ),
        step(
            item(CABLE),
            Some(T(Screwdriver)),
            StepAction::Stack(5),
            "The hydraulic systems are active, and the frame can be <b>wired</b>.",
            "added wiring",
            "deactivated the hydraulic systems",
        ),
        step(
            T(Wirecutters),
            Some(T(Screwdriver)),
            StepAction::Use,
            "The wiring is added, and can be adjusted with <b>wirecutters</b>.",
            "adjusted wiring",
            "removed wiring",
        ),
        // `get_circuit_steps`.
        step(
            item(&def.circuit_control),
            Some(T(Screwdriver)),
            StepAction::Delete,
            "The wiring is adjusted, and the <b>central control module</b> slot has opened.",
            "added central control module",
            "disconnected wiring",
        ),
        step(
            T(Screwdriver),
            Some(T(Crowbar)),
            StepAction::Use,
            "Central control module is installed, and can be <b>screwed</b> into place.",
            "secured central control module",
            "removed central control module",
        ),
        step(
            item(&def.circuit_periph),
            Some(T(Screwdriver)),
            StepAction::Delete,
            "Central control module is secured, and the <b>peripheral control module</b> slot has opened.",
            "added peripheral control module",
            "unsecured central control module",
        ),
        step(
            T(Screwdriver),
            Some(T(Crowbar)),
            StepAction::Use,
            "Peripheral control module is installed, and can be <b>screwed</b> into place.",
            "secured peripheral control module",
            "removed peripheral control module",
        ),
    ];
    if let Some(weapon) = &def.circuit_weapon {
        all.push(step(item(weapon), Some(T(Screwdriver)), StepAction::Delete, "Peripherals control module is secured, and the <b>weapon control module</b> slot has opened.", "added weapon control module", "unsecured peripheral control module"));
        all.push(step(
            T(Screwdriver),
            Some(T(Crowbar)),
            StepAction::Use,
            "Weapon control module is installed, and can be <b>screwed</b> into place.",
            "secured weapon control module",
            "removed weapon control module",
        ));
    }
    // `get_stockpart_steps`.
    let (prev, back) = if def.circuit_weapon.is_some() {
        (
            "Weapon control module is secured",
            "unsecured weapon control module",
        )
    } else {
        (
            "Peripherals control module is secured",
            "unsecured peripheral module",
        )
    };
    let first = format!("{prev}, and the <b>scanning module</b> can be added.");
    all.extend([
        step(
            item(SCANNING_MODULE),
            Some(T(Screwdriver)),
            StepAction::MoveInside,
            &first,
            "added scanning module",
            back,
        ),
        step(
            T(Screwdriver),
            Some(T(Crowbar)),
            StepAction::Use,
            "Scanning module is installed, and can be <b>screwed</b> into place.",
            "secured scanning module",
            "removed scanning module",
        ),
        step(
            item(CAPACITOR),
            Some(T(Screwdriver)),
            StepAction::MoveInside,
            "Scanning module is secured, the <b>capacitor</b> can be added.",
            "added capacitor",
            "unscecured scanning module",
        ),
        step(
            T(Screwdriver),
            Some(T(Crowbar)),
            StepAction::Use,
            "Capacitor is installed, and can be <b>screwed</b> into place.",
            "secured capacitor",
            "removed capacitor",
        ),
        step(
            item(SERVO),
            Some(T(Screwdriver)),
            StepAction::MoveInside,
            "Scanning module is secured, the <b>servo</b> can be added.",
            "added servo",
            "unsecured capacitor",
        ),
        step(
            T(Screwdriver),
            Some(T(Crowbar)),
            StepAction::Use,
            "Servo is installed, and can be <b>screwed</b> into place.",
            "secured servo",
            "removed servo",
        ),
        step(
            item(CELL),
            Some(T(Screwdriver)),
            StepAction::MoveInside,
            "Servo is secured, and the <b>power cell</b> can be added.",
            "added power cell",
            "unsecured servo",
        ),
        step(
            T(Screwdriver),
            Some(T(Crowbar)),
            StepAction::Use,
            "The power cell is installed, and can be <b>screwed</b> into place.",
            "secured power cell",
            "removed power cell",
        ),
    ]);
    // `get_inner_plating_steps`.
    let inner_desc = if is_under(&def.inner_plating, SHEET) {
        format!(
            "The power cell is secured, {} sheets of {} can be used as inner plating.",
            def.inner_plating_amount, def.inner_plating_name
        )
    } else {
        format!(
            "The power cell is secured, {} can be used as inner plating.",
            def.inner_plating_name
        )
    };
    all.extend([
        step(
            item(&def.inner_plating),
            Some(T(Screwdriver)),
            plating_action(&def.inner_plating, def.inner_plating_amount),
            &inner_desc,
            "installed internal armor layer",
            "unsecured power cell",
        ),
        step(
            T(Wrench),
            Some(T(Crowbar)),
            StepAction::Use,
            "Inner plating is installed, and can be <b>wrenched</b> into place.",
            "secured internal armor layer",
            "pried off internal armor layer",
        ),
        step(
            T(Welder),
            Some(T(Wrench)),
            StepAction::Use,
            "Inner plating is wrenched, and can be <b>welded</b>.",
            "welded internal armor layer",
            "unfastened internal armor layer",
        ),
    ]);
    // `get_outer_plating_steps`.
    let outer_action = plating_action(&def.outer_plating, def.outer_plating_amount);
    if ripley {
        all.extend([
            step(
                item(&def.outer_plating),
                Some(T(Welder)),
                outer_action,
                "Plating is welded, and 10 <b>rods</b> can be used to install the cockpit.",
                "installed cockpit",
                "cut off plating",
            ),
            step(
                T(Welder),
                Some(T(Wirecutters)),
                StepAction::Use,
                "Cockpit wire screen is installed, and can be <b>welded</b>.",
                "welded cockpit",
                "cut off cockpit",
            ),
        ]);
    } else {
        let outer_desc = if is_under(&def.outer_plating, SHEET) {
            format!(
                "Inner plating is welded, {} sheets of {} can be used as external armor.",
                def.outer_plating_amount, def.outer_plating_name
            )
        } else {
            format!(
                "Inner plating is welded, {} can be used as external armor.",
                def.outer_plating_name
            )
        };
        all.extend([
            step(
                item(&def.outer_plating),
                Some(T(Welder)),
                outer_action,
                &outer_desc,
                "installed external armor layer",
                "cut off internal armor layer",
            ),
            step(
                T(Wrench),
                Some(T(Crowbar)),
                StepAction::Use,
                "External armor is installed, and can be <b>wrenched</b> into place.",
                "secured external armor layer",
                "pried off external armor layer",
            ),
            step(
                T(Welder),
                Some(T(Wrench)),
                StepAction::Use,
                "External armor is wrenched, and can be <b>welded</b>.",
                "welded external armor layer",
                "unfastened external armor layer",
            ),
        ]);
    }
    for (i, s) in all.iter_mut().enumerate() {
        s.icon_state = format!("{}{i}", def.base_icon);
    }
    Some(all)
}

/// A chassis being built: the parts still missing, then the step index (tg `index`, from 0).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    Parts {
        construction: usize,
        missing: Vec<String>,
    },
    Steps {
        construction: usize,
        index: usize,
    },
}

/// tg `is_right_key` + `check_used_item`: forward, backward or neither.
pub fn direction(step: &Step, item_path: &str, tool: Option<ToolKind>) -> Option<bool> {
    let matches = |key: &Key| match key {
        Key::Tool(kind) => tool == Some(*kind),
        Key::Item(root) => !root.is_empty() && is_under(item_path, root),
    };
    if matches(&step.key) {
        Some(true)
    } else if step.back_key.as_ref().is_some_and(matches) {
        Some(false)
    } else {
        None
    }
}

/// A held item used on a chassis (tg `COMSIG_ATOM_ATTACKBY` -> `action` -> `check_step`). True
/// when the chassis took the click.
pub fn attackby(world: &mut World, actor: Actor, chassis: Entity) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    let Some(frame) = frame_of(world, chassis) else {
        return false;
    };
    let Some(item_path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return false;
    };
    match frame {
        Frame::Parts {
            construction,
            missing,
        } => {
            let Some(i) = missing.iter().position(|p| is_under(&item_path, p)) else {
                return false;
            };
            add_part(world, actor, chassis, item, construction, missing, i);
            true
        }
        Frame::Steps {
            construction,
            index,
        } => {
            let def = world.resource::<MechaDb>().data.constructions[construction].clone();
            let Some(steps) = steps(&def) else {
                return false;
            };
            let Some(current) = steps.get(index) else {
                return false;
            };
            let tool = tool_of(world, item).map(|t| t.0);
            let Some(forward) = direction(current, &item_path, tool) else {
                return false;
            };
            if forward && current_action_ok(world, actor, item, current) {
                do_step(world, actor, chassis, item, &def, &steps, index, true);
            } else if !forward && index > 0 && tool_ok(world, actor, item) {
                do_step(world, actor, chassis, item, &def, &steps, index, false);
            }
            true
        }
    }
}

/// The chassis' frame state; a fresh chassis starts with its parts missing.
fn frame_of(world: &mut World, chassis: Entity) -> Option<Frame> {
    if let Some(f) = world.get::<Frame>(chassis) {
        return Some(f.clone());
    }
    let path = world.get::<DmItem>(chassis)?.path.clone();
    let db = world.resource::<MechaDb>();
    let construction = db.construction(&path)?;
    let frame = Frame::Parts {
        construction,
        missing: db.data.constructions[construction].parts.clone(),
    };
    world.entity_mut(chassis).insert(frame.clone());
    Some(frame)
}

/// tg `use_tool(parent, user, 0)`'s start check for tools; other items pass.
fn tool_ok(world: &mut World, actor: Actor, item: Entity) -> bool {
    if tool_of(world, item).is_none() || crate::tools::tool_usable(world, item, 0, 0.0) {
        return true;
    }
    let text = crate::tools::unusable_message(world, item, 0);
    tell(world, actor.client, Span::Warning, text);
    false
}

/// A forward step's item check: tools usable, stacks with the amount.
fn current_action_ok(world: &mut World, actor: Actor, item: Entity, step: &Step) -> bool {
    match step.action {
        StepAction::Use => tool_ok(world, actor, item),
        StepAction::Stack(amount) => {
            if crate::tools::stack_amount(world, item) >= amount {
                return true;
            }
            tell(
                world,
                actor.client,
                Span::Warning,
                format!("Нужно как минимум {amount} шт.!"),
            );
            false
        }
        StepAction::Delete | StepAction::MoveInside => true,
    }
}

/// `user.transferItemToLoc(item, parent)`: out of the user's hands.
fn from_hands(world: &mut World, mob: Entity, item: Entity) {
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        for hand in &mut inventory.hands {
            if *hand == Some(item) {
                *hand = None;
            }
        }
    }
    world.entity_mut(item).remove::<CarriedBy>();
}

/// `/datum/component/construction/unordered/mecha_chassis/custom_action`.
fn add_part(
    world: &mut World,
    actor: Actor,
    chassis: Entity,
    item: Entity,
    construction: usize,
    mut missing: Vec<String>,
    i: usize,
) {
    let part = name_of(world, item);
    let overlay_state = world
        .get::<DmAtom>(item)
        .map(|a| (a.icon.clone(), format!("{}+o", a.icon_state)));
    from_hands(world, actor.mob, item);
    world.despawn(item);
    missing.remove(i);
    if let Some((icon, state)) = overlay_state
        && let Some(id) = world
            .get_resource::<IconIndex>()
            .and_then(|x| x.get(&icon, &state))
        && let Some(mut appearance) = world.get_mut::<Appearance>(chassis)
    {
        appearance.overlays.push(Overlay {
            icon_state: id,
            dir: None,
            color: None,
            pixel_offset: [0, 0],
            plane: None,
            only_dirs: 0,
        });
    }
    let pos = world.get::<TilePos>(chassis).copied();
    if let Some(pos) = pos {
        let user = name_of(world, actor.mob);
        visible(
            world,
            pos,
            Span::Notice,
            format!("{user}: connected {part}"),
            &[],
        );
    }
    if !missing.is_empty() {
        world.entity_mut(chassis).insert(Frame::Parts {
            construction,
            missing,
        });
        return;
    }
    // `spawn_result`: the frame sprite, dense, then the ordered steps.
    world.entity_mut(chassis).insert((
        Density,
        Frame::Steps {
            construction,
            index: 0,
        },
    ));
    if let Some(mut appearance) = world.get_mut::<Appearance>(chassis) {
        appearance.overlays.clear();
    }
    let def = world.resource::<MechaDb>().data.constructions[construction].clone();
    if let Some(steps) = steps(&def) {
        show_step(world, chassis, &steps[0]);
    }
    if let Some(pos) = pos {
        crate::construction::tile_changed(world, pos);
    }
}

/// tg `update_parent`: the step's sprite.
fn show_step(world: &mut World, chassis: Entity, step: &Step) {
    if let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(FRAME_DMI, &step.icon_state))
        && let Some(mut appearance) = world.get_mut::<Appearance>(chassis)
    {
        appearance.icon_state = id;
    }
}

/// tg `custom_action` + `update_index` of one step forward or back.
#[allow(clippy::too_many_arguments)]
fn do_step(
    world: &mut World,
    actor: Actor,
    chassis: Entity,
    item: Entity,
    def: &ConstructionDef,
    steps: &[Step],
    index: usize,
    forward: bool,
) {
    let Some(&pos) = world.get::<TilePos>(chassis) else {
        return;
    };
    let current = &steps[index];
    if forward {
        match current.action {
            StepAction::Use => {}
            StepAction::Delete => {
                from_hands(world, actor.mob, item);
                world.despawn(item);
            }
            StepAction::MoveInside => {
                from_hands(world, actor.mob, item);
                put_inside(world, item, chassis);
            }
            StepAction::Stack(amount) => {
                crate::tools::use_stack(world, item, amount);
            }
        }
    } else {
        // Undo the previous step: give back what it used (tg `no_refund` is never set here).
        let target = &steps[index - 1];
        if let Key::Item(path) = &target.key {
            match target.action {
                StepAction::Delete => {
                    crate::construction::spawn_obj(world, path, pos);
                }
                StepAction::MoveInside => {
                    let inside = world.get::<Stored>(chassis).and_then(|s| {
                        s.items().iter().copied().find(|&e| {
                            world
                                .get::<DmItem>(e)
                                .is_some_and(|i| is_under(&i.path, path))
                        })
                    });
                    if let Some(part) = inside {
                        take_out(world, part, pos);
                    }
                }
                StepAction::Stack(amount) if is_under(path, STACK) => {
                    crate::construction::spawn_stack(world, path, pos, amount);
                }
                _ => {}
            }
        }
    }
    let user = name_of(world, actor.mob);
    let (message, next_desc) = if forward {
        (
            &current.forward,
            steps.get(index + 1).map(|s| s.desc.clone()),
        )
    } else {
        (
            &current.backward,
            index.checked_sub(1).map(|i| steps[i].desc.clone()),
        )
    };
    if !message.is_empty() {
        visible(world, pos, Span::Notice, format!("{user}: {message}"), &[]);
    }
    if let Some(desc) = next_desc {
        tell(
            world,
            actor.client,
            Span::Notice,
            desc.replace("<b>", "").replace("</b>", ""),
        );
    }
    let next = if forward { index + 1 } else { index - 1 };
    if next >= steps.len() {
        spawn_result(world, chassis, def, pos);
        return;
    }
    let construction = world
        .resource::<MechaDb>()
        .construction(&def.chassis)
        .unwrap_or(0);
    world.entity_mut(chassis).insert(Frame::Steps {
        construction,
        index: next,
    });
    show_step(world, chassis, &steps[next]);
}

/// `/datum/component/construction/mecha/spawn_result`: the mech with the frame's contents.
fn spawn_result(world: &mut World, chassis: Entity, def: &ConstructionDef, pos: TilePos) {
    let Some(mech) = crate::construction::spawn_obj(world, &def.result, pos) else {
        warn!("mecha: no prototype for {}", def.result);
        return;
    };
    let contents: Vec<Entity> = world
        .get::<Stored>(chassis)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for part in contents {
        put_inside(world, part, mech);
    }
    init_mech(world, mech, &def.result, true);
    world.despawn(chassis);
    crate::construction::tile_changed(world, pos);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ripley() -> ConstructionDef {
        ConstructionDef {
            chassis: "/obj/item/mecha_parts/chassis/ripley".into(),
            path: "/datum/component/construction/mecha/ripley".into(),
            result: "/obj/vehicle/sealed/mecha/ripley".into(),
            base_icon: "ripley".into(),
            circuit_control: "/obj/item/circuitboard/mecha/ripley/main".into(),
            circuit_periph: "/obj/item/circuitboard/mecha/ripley/peripherals".into(),
            inner_plating: "/obj/item/stack/sheet/iron".into(),
            inner_plating_amount: 5,
            inner_plating_name: "iron".into(),
            outer_plating: "/obj/item/stack/rods".into(),
            outer_plating_amount: 10,
            outer_plating_name: "iron rod".into(),
            procs: vec!["get_outer_plating_steps".into()],
            ..default()
        }
    }

    #[test]
    fn ripley_steps_follow_tg() {
        let steps = steps(&ripley()).unwrap();
        // 4 frame + 4 circuit + 8 stock part + 3 inner plating + 2 cockpit.
        assert_eq!(steps.len(), 21);
        assert_eq!(steps[0].key, Key::Tool(Wrench));
        assert_eq!(steps[0].icon_state, "ripley0");
        assert_eq!(steps[2].action, StepAction::Stack(5));
        assert_eq!(steps[4].action, StepAction::Delete);
        assert_eq!(
            steps[14].key,
            item("/obj/item/stock_parts/power_store/cell")
        );
        assert_eq!(steps[14].action, StepAction::MoveInside);
        assert_eq!(steps[16].action, StepAction::Stack(5));
        assert_eq!(steps[19].key, item("/obj/item/stack/rods"));
        // Rods are not sheets: tg's `ITEM_DELETE` takes the whole stack.
        assert_eq!(steps[19].action, StepAction::Delete);
        assert_eq!(steps[20].back_key, Some(Key::Tool(Wirecutters)));
        assert_eq!(steps[20].icon_state, "ripley20");
    }

    #[test]
    fn keys_go_forward_or_back() {
        let steps = steps(&ripley()).unwrap();
        let cell = "/obj/item/stock_parts/power_store/cell/high";
        assert_eq!(
            direction(&steps[14], cell, None),
            Some(true),
            "subtypes fit"
        );
        assert_eq!(
            direction(&steps[14], "/obj/item/wrench", Some(Screwdriver)),
            Some(false)
        );
        assert_eq!(
            direction(&steps[14], "/obj/item/wrench", Some(Wrench)),
            None
        );
        assert_eq!(
            direction(&steps[0], "/obj/item/wrench", Some(Wrench)),
            Some(true)
        );
        assert_eq!(steps[0].back_key, None, "the first step has no way back");
    }

    #[test]
    fn custom_step_procs_are_not_built() {
        let mut gygax = ripley();
        gygax.path = "/datum/component/construction/mecha/gygax".into();
        gygax.procs = vec!["action".into(), "get_outer_plating_steps".into()];
        assert!(steps(&gygax).is_none());
        let mut odysseus = ripley();
        odysseus.path = "/datum/component/construction/mecha/odysseus".into();
        odysseus.procs.clear();
        odysseus.outer_plating = "/obj/item/stack/sheet/plasteel".into();
        let steps = steps(&odysseus).unwrap();
        assert_eq!(
            steps.len(),
            22,
            "default outer plating: sheets, wrench, weld"
        );
        assert_eq!(steps[19].action, StepAction::Stack(10));
    }
}
