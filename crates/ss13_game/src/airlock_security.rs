//! Airlock reinforcement (server): tg airlock `security_level` (`AIRLOCK_SECURITY_*`) with
//! `try_reinforce`, `welder_act`, `crowbar_act`, `wirecutter_act` and
//! `attempt_wire_interaction` (`code/game/machinery/doors/airlock.dm`).
//!
//! - With the panel open and no plating, 2 iron sheets (iron cover) or 2 plasteel sheets (full
//!   plasteel, grille on top) reinforce the airlock in 2 s.
//! - Taking it off, panel open: the iron cover is welded off (4 s, 2 iron back); plasteel goes
//!   grille (wirecutters 1 s, shocks a powered airlock at 60%) -> outer layer (welder 4 s, then
//!   crowbar 4 s, 1 plasteel) -> inner layer (welder 4 s, then crowbar 4 s, 1 plasteel).
//! - While any plating is on, the wires refuse every hand and tool ("Провода защищены!"), and a
//!   crowbar or welder on an open panel works the plating instead of prying or repairing.
// shortcut: no integrity or damage deflection bonus (`AIRLOCK_INTEGRITY_MULTIPLIER`,
// `AIRLOCK_DAMAGE_DEFLECTION_R`): doors have no integrity yet.

use bevy::prelude::*;
use ss13_mapload::DoorKind;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::doors::{Door, DoorJob, with_door};
use crate::power::Unpowered;
use crate::tools::{Actor, ToolKind, do_after, tell, use_tool};
use crate::wires::Electrified;

/// `AIRLOCK_SECURITY_*`.
pub const NONE: u8 = 0;
pub const IRON: u8 = 1;
pub const PLASTEEL_I_S: u8 = 2;
pub const PLASTEEL_I: u8 = 3;
pub const PLASTEEL_O_S: u8 = 4;
pub const PLASTEEL_O: u8 = 5;
pub const PLASTEEL: u8 = 6;

const IRON_SHEET: &str = "/obj/item/stack/sheet/iron";
const PLASTEEL_SHEET: &str = "/obj/item/stack/sheet/plasteel";
/// `try_reinforce`: `do_after(2 SECONDS)`, 2 sheets.
const REINFORCE_DS: f32 = 20.0;
const REINFORCE_SHEETS: u32 = 2;

/// One step of taking the plating off: the tool, its delay (deciseconds) and the level after.
fn removal(level: u8) -> Option<(ToolKind, f32, u8)> {
    Some(match level {
        IRON => (ToolKind::Welder, 40.0, NONE),
        PLASTEEL_O => (ToolKind::Welder, 40.0, PLASTEEL_O_S),
        PLASTEEL_I => (ToolKind::Welder, 40.0, PLASTEEL_I_S),
        PLASTEEL_O_S => (ToolKind::Crowbar, 40.0, PLASTEEL_I),
        PLASTEEL_I_S => (ToolKind::Crowbar, 40.0, NONE),
        PLASTEEL => (ToolKind::Wirecutters, 10.0, PLASTEEL_O),
        _ => return None,
    })
}

/// The level a sheet type reinforces a bare airlock to.
fn reinforcement(path: &str) -> Option<u8> {
    if ss13_mapload::fidelity::is_under(path, IRON_SHEET) {
        Some(IRON)
    } else if ss13_mapload::fidelity::is_under(path, PLASTEEL_SHEET) {
        Some(PLASTEEL)
    } else {
        None
    }
}

fn airlock(world: &World, target: Entity) -> Option<&Door> {
    world
        .get::<Door>(target)
        .filter(|d| d.kind == DoorKind::Airlock)
}

/// A sheet that would reinforce `door` (`item_interaction`: open panel, no plating). The door
/// click (`crate::doors`) leaves such a click to [`item_interaction`].
pub fn reinforces(door: &Door, sheet_path: &str) -> bool {
    door.kind == DoorKind::Airlock
        && door.panel_open
        && door.security_level == NONE
        && reinforcement(sheet_path).is_some()
}

/// tg `attempt_wire_interaction`: plating over the wires refuses them. True when refused.
pub fn wires_protected(world: &mut World, client: Entity, holder: Entity) -> bool {
    if airlock(world, holder).is_none_or(|d| d.security_level == NONE) {
        return false;
    }
    tell(world, client, Span::Warning, "Провода защищены!");
    true
}

/// `airlock/item_interaction` with iron or plasteel sheets -> `try_reinforce`. True when
/// handled.
pub fn item_interaction(world: &mut World, actor: Actor, held: Entity, target: Entity) -> bool {
    let Some(path) = world
        .get::<ss13_mapload::DmItem>(held)
        .map(|i| i.path.clone())
    else {
        return false;
    };
    let Some(level) = reinforcement(&path) else {
        return false;
    };
    if !airlock(world, target).is_some_and(|d| reinforces(d, &path)) {
        return false;
    }
    if world.get::<Electrified>(target).is_some()
        && crate::wires::shock(world, target, actor.mob, 75.0)
    {
        return true;
    }
    if crate::tools::stack_amount(world, held) < REINFORCE_SHEETS {
        let name = crate::security::name_of(world, held);
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("Нужно как минимум {REINFORCE_SHEETS} шт. ({name}), чтобы укрепить шлюз."),
        );
        return true;
    }
    if do_after(
        world,
        actor,
        target,
        REINFORCE_DS,
        0,
        crate::construction::Job::Door(DoorJob::Reinforce { sheet: held, level }),
    ) {
        tell(
            world,
            actor.client,
            Span::Notice,
            "Вы начинаете укреплять шлюз.",
        );
    }
    true
}

/// `welder_act`, `crowbar_act`, `wirecutter_act` on an open panel with plating. True when the
/// plating took the click (a tool that is not this step's does nothing, as in tg).
pub fn tool_act(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> bool {
    let Some(door) = airlock(world, target) else {
        return false;
    };
    let (panel_open, level) = (door.panel_open, door.security_level);
    // Wirecutters only work the grille; elsewhere they reach the wires (refused there).
    let plating_tool = matches!(kind, ToolKind::Welder | ToolKind::Crowbar)
        || (kind == ToolKind::Wirecutters && level == PLASTEEL);
    if !panel_open || level == NONE || !plating_tool {
        return false;
    }
    let Some((tool, delay, _)) = removal(level).filter(|(tool, ..)| *tool == kind) else {
        return true;
    };
    let job = crate::construction::Job::Door(DoorJob::Shield { from: level });
    match tool {
        ToolKind::Wirecutters => {
            let powered = world.get::<Unpowered>(target).is_none();
            let has_power = airlock(world, target).is_some_and(|d| d.has_power(powered));
            if has_power && crate::wires::shock(world, target, actor.mob, 60.0) {
                return true;
            }
            tell(
                world,
                actor.client,
                Span::Notice,
                "Вы начинаете перекусывать внешнюю решётку.",
            );
            use_tool(world, actor, target, delay, 0, job);
        }
        ToolKind::Welder => {
            // `tool_start_check(user, amount = 1)`.
            if let Some(item) = actor.item
                && !crate::tools::tool_usable(world, item, 1, 0.0)
            {
                let text = crate::tools::unusable_message(world, item, 1);
                tell(world, actor.client, Span::Warning, text);
                return true;
            }
            if use_tool(world, actor, target, delay, 0, job) {
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    format!("Вы начинаете срезать {}...", layer_name(level)),
                );
            }
        }
        _ => {
            if use_tool(world, actor, target, delay, 0, job) {
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    format!("Вы начинаете снимать {} шлюза.", layer_name(level)),
                );
            }
        }
    }
    true
}

/// tg `layer_flavor`.
fn layer_name(level: u8) -> &'static str {
    match level {
        IRON => "защиту панели",
        PLASTEEL_O | PLASTEEL_O_S => "внешний слой защиты",
        _ => "внутренний слой защиты",
    }
}

/// The end of a [`DoorJob::Reinforce`] or [`DoorJob::Shield`], re-checked: the panel is still
/// open and the plating as the job found it; sheets are used up only now (`material.use`).
pub fn complete(world: &mut World, mob: Entity, target: Entity, job: DoorJob) {
    let Some(&user_pos) = world.get::<TilePos>(mob) else {
        return;
    };
    match job {
        DoorJob::Reinforce { sheet, level } => {
            if !airlock(world, target).is_some_and(|d| d.panel_open && d.security_level == NONE)
                || world.get::<crate::inventory::CarriedBy>(sheet).map(|c| c.0) != Some(mob)
                || !crate::tools::use_stack(world, sheet, REINFORCE_SHEETS)
            {
                return;
            }
            set_level(world, target, level);
            let user = crate::security::name_of(world, mob);
            crate::medical::visible(
                world,
                mob,
                "Вы укрепляете шлюз.",
                &format!("{user} укрепляет шлюз."),
                Span::Notice,
            );
        }
        DoorJob::Shield { from } => {
            if !airlock(world, target).is_some_and(|d| d.panel_open && d.security_level == from) {
                return;
            }
            let Some((tool, _, next)) = removal(from) else {
                return;
            };
            set_level(world, target, next);
            let user = crate::security::name_of(world, mob);
            let (own, others) = match tool {
                ToolKind::Wirecutters => (
                    "Вы перекусываете внешнюю решётку шлюза.".to_owned(),
                    format!("{user} перекусывает внешнюю решётку шлюза."),
                ),
                ToolKind::Welder => (
                    format!("Вы срезаете {} шлюза.", layer_name(from)),
                    format!("{user} срезает защиту шлюза."),
                ),
                _ => (
                    format!("Вы снимаете {} шлюза.", layer_name(from)),
                    format!("{user} снимает защиту шлюза."),
                ),
            };
            crate::medical::visible(world, mob, &own, &others, Span::Notice);
            match (tool, from) {
                (ToolKind::Crowbar, _) => {
                    crate::construction::spawn_stack(world, PLASTEEL_SHEET, user_pos, 1);
                }
                (ToolKind::Welder, IRON) => {
                    crate::construction::spawn_stack(world, IRON_SHEET, user_pos, 2);
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn set_level(world: &mut World, target: Entity, level: u8) {
    with_door(world, target, |door, appearance, _| {
        door.security_level = level;
        door.apply_look(appearance);
        ((), false)
    });
}

/// The airlock's plating line (`airlock/examine`).
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(door) = airlock(world, target) else {
        return;
    };
    let text = if door.panel_open {
        match door.security_level {
            NONE => "Его провода оголены!",
            IRON => "Его провода скрыты за приваренной железной пластиной.",
            PLASTEEL_I_S | PLASTEEL_O_S => "Внутри видны обрывки пластали.",
            PLASTEEL_I => "Его провода закрыты внутренним слоем пластали.",
            PLASTEEL_O => "Его провода закрыты приваренной пласталевой пластиной.",
            _ => "Его панель закрыта защитной решёткой.",
        }
    } else {
        match door.security_level {
            NONE => return,
            IRON => "Он выглядит чуть прочнее.",
            _ => "Он выглядит очень прочным.",
        }
    };
    lines.push((Span::Plain, text.to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plasteel steps lead back to bare wires; the iron cover in one weld.
    #[test]
    fn removal_chain_ends_bare() {
        let mut level = PLASTEEL;
        let mut tools = Vec::new();
        while let Some((tool, _, next)) = removal(level) {
            tools.push(tool);
            level = next;
        }
        assert_eq!(level, NONE);
        assert_eq!(
            tools,
            [
                ToolKind::Wirecutters,
                ToolKind::Welder,
                ToolKind::Crowbar,
                ToolKind::Welder,
                ToolKind::Crowbar
            ]
        );
        assert_eq!(removal(IRON), Some((ToolKind::Welder, 40.0, NONE)));
        assert_eq!(removal(NONE), None);
    }

    #[test]
    fn sheets_reinforce_only_open_bare_airlocks() {
        let mut door = crate::doors::test_airlock(&[]);
        assert!(!reinforces(&door, IRON_SHEET), "panel closed");
        door.panel_open = true;
        assert!(reinforces(&door, IRON_SHEET));
        assert!(reinforces(&door, "/obj/item/stack/sheet/plasteel/fifty"));
        assert!(!reinforces(&door, "/obj/item/stack/sheet/glass"));
        door.security_level = IRON;
        assert!(!reinforces(&door, PLASTEEL_SHEET), "already plated");
    }
}
