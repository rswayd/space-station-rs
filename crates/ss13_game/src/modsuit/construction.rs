//! MOD construction (server), after tg `code/modules/mod/mod_construction.dm`:
//!
//! - The MOD shell (`/obj/item/mod/construction/shell`) is built step by step, on the floor or
//!   in the other hand: a MOD core in, screwdriver (core secured), helmet, chestplate,
//!   gauntlets, boots, wrench (assembly secured), screwdriver (assembly screwed), external
//!   plating. A crowbar takes the last part (or the loose core) back out, the screwdriver and
//!   wrench steps undo themselves. The plating makes a control unit of the plating's theme with
//!   the shell's core (tg `new /obj/item/mod/control(loc, theme, null, core)`) in the user's
//!   hands; the shell, its parts and the plating are gone. The shell shows
//!   `mod-construction_<step>`.
//! - A broken MOD core becomes a standard core after a 5 s screwdriver; a plasma flower with two
//!   cables (5 s) becomes a lavaland plasma core.
//!
//! The data (the platings' themes) is `ss13-mapload convert`'s `platings` in `modsuits.ron`.
// shortcut: the plating keeps its prototype name (tg renames it "MOD <theme> external plating"
// with the theme's description at `Initialize`); messages are chat lines (tg balloon alerts).

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::ModJob;
use super::server::{InSuit, ModDb, ModSuit, ThemeOverride, init_core, init_suit};
use crate::construction::Job;
use crate::inventory::CarriedBy;
use crate::sound::{PlaySound, playsound};
use crate::storage::StoredIn;
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of};

pub const SHELL: &str = "/obj/item/mod/construction/shell";
const HELMET: &str = "/obj/item/mod/construction/helmet";
const CHESTPLATE: &str = "/obj/item/mod/construction/chestplate";
const GAUNTLETS: &str = "/obj/item/mod/construction/gauntlets";
const BOOTS: &str = "/obj/item/mod/construction/boots";
const BROKEN_CORE: &str = "/obj/item/mod/construction/broken_core";
const PLASMA_FLOWER: &str = "/obj/item/mod/construction/lavalandcore";
const CABLE_COIL: &str = "/obj/item/stack/cable_coil";
const STANDARD_CORE: &str = "/obj/item/mod/core/standard";
const LAVALAND_CORE: &str = "/obj/item/mod/core/plasma/lavaland";
const CLICK: &str = "sound/machines/click.ogg";
/// `use_tool(src, user, 5 SECONDS)` of the core repairs, deciseconds.
const CORE_REPAIR_DS: f32 = 50.0;
/// `tool_start_check(user, amount = 2)` of the plasma flower.
const FLOWER_CABLES: u32 = 2;

/// The shell's `step` (`START_STEP` .. `SCREWED_ASSEMBLY_STEP`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShellStep {
    #[default]
    Start,
    Core,
    ScrewedCore,
    Helmet,
    Chestplate,
    Gauntlets,
    Boots,
    WrenchedAssembly,
    ScrewedAssembly,
}

impl ShellStep {
    /// The `#define`'s text, the end of the shell's icon state.
    pub fn id(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Core => "core",
            Self::ScrewedCore => "screwed_core",
            Self::Helmet => "helmet",
            Self::Chestplate => "chestplate",
            Self::Gauntlets => "gauntlets",
            Self::Boots => "boots",
            Self::WrenchedAssembly => "wrenched_assembly",
            Self::ScrewedAssembly => "screwed_assembly",
        }
    }

    /// The part a step takes, the step it leads to and what the user is told.
    fn part(self) -> Option<(&'static str, Self, &'static str)> {
        match self {
            Self::ScrewedCore => Some((HELMET, Self::Helmet, "шлем установлен")),
            Self::Helmet => Some((CHESTPLATE, Self::Chestplate, "нагрудник установлен")),
            Self::Chestplate => Some((GAUNTLETS, Self::Gauntlets, "перчатки установлены")),
            Self::Gauntlets => Some((BOOTS, Self::Boots, "ботинки установлены")),
            _ => None,
        }
    }

    /// `crowbar_act`: the step a removed part goes back to, and the message.
    fn back(self) -> Option<(Self, &'static str)> {
        match self {
            Self::Core => Some((Self::Start, "ядро извлечено")),
            Self::Helmet => Some((Self::ScrewedCore, "шлем снят")),
            Self::Chestplate => Some((Self::Helmet, "нагрудник снят")),
            Self::Gauntlets => Some((Self::Chestplate, "перчатки сняты")),
            Self::Boots => Some((Self::Gauntlets, "ботинки сняты")),
            _ => None,
        }
    }

    /// `screwdriver_act` and `wrench_act`: the toggled step and the message.
    fn toggle(self, tool: ToolKind) -> Option<(Self, &'static str)> {
        match (tool, self) {
            (ToolKind::Screwdriver, Self::Core) => Some((Self::ScrewedCore, "ядро закреплено")),
            (ToolKind::Screwdriver, Self::ScrewedCore) => Some((Self::Core, "ядро откреплено")),
            (ToolKind::Screwdriver, Self::WrenchedAssembly) => {
                Some((Self::ScrewedAssembly, "сборка закручена"))
            }
            (ToolKind::Screwdriver, Self::ScrewedAssembly) => {
                Some((Self::WrenchedAssembly, "сборка откручена"))
            }
            (ToolKind::Wrench, Self::Boots) => Some((Self::WrenchedAssembly, "сборка закреплена")),
            (ToolKind::Wrench, Self::WrenchedAssembly) => Some((Self::Boots, "сборка откреплена")),
            _ => None,
        }
    }
}

/// The live `/obj/item/mod/construction/shell`: its step, the core and the parts inside it
/// (`StoredIn` the shell; the parts in the order they went in).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ModShell {
    pub step: ShellStep,
    pub core: Option<Entity>,
    pub parts: Vec<Entity>,
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    world.get::<DmItem>(e).map(|i| i.path.clone())
}

fn sound(world: &mut World, e: Entity, sound: &str) {
    if let Some(pos) = crate::chem::map_pos(world, e) {
        playsound(world, PlaySound::new(pos, sound, 30.0, true));
    }
}

/// The tool's `usesound` (`use_tool(volume = 30)`).
fn tool_sound(tool: ToolKind) -> &'static str {
    match tool {
        ToolKind::Wrench => "sound/items/tools/ratchet.ogg",
        ToolKind::Crowbar => "sound/items/tools/crowbar.ogg",
        _ => "sound/items/tools/screwdriver.ogg",
    }
}

/// The `do_after` target: the thing on the floor, the user when the thing is held.
pub(super) fn job_target(world: &World, mob: Entity, thing: Entity) -> Entity {
    if world.get::<TilePos>(thing).is_some() {
        thing
    } else {
        mob
    }
}

/// Still in the user's hands or within reach (tg `do_after(target = src)`).
pub(super) fn reaches(world: &World, mob: Entity, thing: Entity) -> bool {
    world.get::<CarriedBy>(thing).is_some_and(|c| c.0 == mob)
        || crate::storage::in_reach(world, mob, thing)
}

/// `held` used on a MOD construction item (on the floor or in the other hand). True when the
/// click was its.
pub fn item_interaction(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(held) = actor.item else {
        return false;
    };
    let Some(path) = path_of(world, target) else {
        return false;
    };
    if is_under(&path, SHELL) {
        return shell_interaction(world, actor, held, target);
    }
    let tool = tool_of(world, held).map(|(k, _)| k);
    if is_under(&path, BROKEN_CORE) && tool == Some(ToolKind::Screwdriver) {
        tell(world, actor.client, Span::Notice, "ремонтируем...");
        let on = job_target(world, actor.mob, target);
        crate::tools::use_tool(
            world,
            actor,
            on,
            CORE_REPAIR_DS,
            0,
            Job::ModSuit(ModJob::RepairCore(target)),
        );
        return true;
    }
    let cable = path_of(world, held).is_some_and(|p| is_under(&p, CABLE_COIL));
    if is_under(&path, PLASMA_FLOWER) && cable {
        if !crate::tools::tool_usable(world, held, FLOWER_CABLES, 0.0) {
            let text = crate::tools::unusable_message(world, held, FLOWER_CABLES);
            tell(world, actor.client, Span::Warning, text);
            return true;
        }
        tell(world, actor.client, Span::Notice, "подключаем провода...");
        let on = job_target(world, actor.mob, target);
        crate::tools::use_tool(
            world,
            actor,
            on,
            CORE_REPAIR_DS,
            FLOWER_CABLES,
            Job::ModSuit(ModJob::WireFlower(target)),
        );
        return true;
    }
    false
}

/// The end of the repair and wiring `do_after`s: the core replaces the thing where it was.
pub(super) fn complete(world: &mut World, mob: Entity, action: &DoAfter, item: Entity) {
    if world.get_entity(item).is_err() || !reaches(world, mob, item) {
        return;
    }
    let Some(path) = path_of(world, item) else {
        return;
    };
    let result = if is_under(&path, BROKEN_CORE) {
        STANDARD_CORE
    } else if is_under(&path, PLASMA_FLOWER) {
        if !action
            .item
            .is_some_and(|c| crate::tools::consume(world, c, FLOWER_CABLES))
        {
            return;
        }
        LAVALAND_CORE
    } else {
        return;
    };
    let Some(pos) = crate::chem::map_pos(world, item) else {
        return;
    };
    world.despawn(item);
    if let Some(core) = crate::construction::spawn_obj(world, result, pos) {
        init_core(world, core);
    }
}

fn shell_interaction(world: &mut World, actor: Actor, held: Entity, shell: Entity) -> bool {
    if world.get::<ModShell>(shell).is_none() {
        world.entity_mut(shell).insert(ModShell::default());
    }
    let state = world.get::<ModShell>(shell).cloned().unwrap_or_default();
    let held_path = path_of(world, held).unwrap_or_default();
    let tell_user = |world: &mut World, text: &str| tell(world, actor.client, Span::Notice, text);
    // `item_interaction`: parts and the core go in.
    if state.step == ShellStep::Start && is_under(&held_path, ss13_mapload::modsuit::CORE_ROOT) {
        init_core(world, held);
        crate::storage::put_inside(world, held, shell);
        sound(world, shell, CLICK);
        tell_user(world, "ядро установлено");
        set_state(world, shell, ShellStep::Core, |s| s.core = Some(held));
        return true;
    }
    if let Some((part, next, text)) = state.step.part()
        && is_under(&held_path, part)
    {
        crate::storage::put_inside(world, held, shell);
        sound(world, shell, CLICK);
        tell_user(world, text);
        set_state(world, shell, next, |s| s.parts.push(held));
        return true;
    }
    if state.step == ShellStep::ScrewedAssembly
        && is_under(&held_path, ss13_mapload::modsuit::PLATING_ROOT)
    {
        finish(world, actor, held, shell);
        return true;
    }
    let Some(tool) = tool_of(world, held).map(|(k, _)| k) else {
        return false;
    };
    if let Some((next, text)) = state.step.toggle(tool) {
        sound(world, shell, tool_sound(tool));
        tell_user(world, text);
        set_state(world, shell, next, |_| {});
        return true;
    }
    if tool == ToolKind::Crowbar
        && let Some((back, text)) = state.step.back()
    {
        let Some(pos) = crate::chem::map_pos(world, shell) else {
            return true;
        };
        let out = if state.step == ShellStep::Core {
            state.core
        } else {
            state.parts.last().copied()
        };
        if let Some(out) = out.filter(|&e| world.get::<StoredIn>(e).is_some_and(|s| s.0 == shell)) {
            crate::storage::take_out(world, out, pos);
            crate::construction::tile_changed(world, pos);
        }
        sound(world, shell, tool_sound(tool));
        tell_user(world, text);
        // Not tg: tg's crowbar keeps a stale `core` reference after taking the core out.
        set_state(world, shell, back, |s| {
            if back == ShellStep::Start {
                s.core = None;
            } else {
                s.parts.pop();
            }
        });
        return true;
    }
    false
}

/// The shell's new step and look (`update_icon_state`: `mod-construction_<step>`).
fn set_state(world: &mut World, shell: Entity, step: ShellStep, edit: impl FnOnce(&mut ModShell)) {
    if let Some(mut s) = world.get_mut::<ModShell>(shell) {
        s.step = step;
        edit(&mut *s);
    }
    let state = format!("mod-construction_{}", step.id());
    crate::obj_damage::set_icon_state(world, shell, &state);
}

/// The plating finishes the shell: a control unit of its theme with the shell's core, in the
/// user's hands.
fn finish(world: &mut World, actor: Actor, plating: Entity, shell: Entity) {
    let plating_path = path_of(world, plating).unwrap_or_default();
    let theme = world
        .get_resource::<ModDb>()
        .and_then(|db| {
            db.data
                .platings
                .iter()
                .find(|(p, _)| *p == plating_path)
                .map(|(_, t)| t.clone())
        })
        .unwrap_or_else(|| format!("{}/standard", ss13_mapload::modsuit::THEME_ROOT));
    let Some(pos) = crate::chem::map_pos(world, shell) else {
        return;
    };
    let Some(control) =
        crate::construction::spawn_detached(world, ss13_mapload::modsuit::CONTROL_ROOT)
    else {
        warn!("no prototype for {}", ss13_mapload::modsuit::CONTROL_ROOT);
        return;
    };
    world.entity_mut(control).insert(ThemeOverride(theme));
    init_suit(world, control);
    sound(world, shell, CLICK);
    let core = world
        .get::<ModShell>(shell)
        .and_then(|s| s.core)
        .filter(|&c| world.get::<StoredIn>(c).is_some_and(|s| s.0 == shell));
    if let Some(core) = core
        && world
            .get::<ModSuit>(control)
            .is_some_and(|s| s.core.is_none())
    {
        world
            .entity_mut(core)
            .remove::<StoredIn>()
            .insert(InSuit(control));
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.core = Some(core);
        }
    }
    world.despawn(plating);
    world.despawn(shell);
    crate::storage::take_out(world, control, pos);
    if let Some(inventory) = world.get::<crate::inventory::Inventory>(actor.mob).cloned() {
        crate::storage::put_in_hands(world, actor.mob, &inventory, control);
    }
    crate::construction::tile_changed(world, pos);
    tell(world, actor.client, Span::Notice, "блок собран");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_steps_follow_tg_order() {
        let mut step = ShellStep::ScrewedCore;
        let mut parts = Vec::new();
        while let Some((part, next, _)) = step.part() {
            parts.push(part);
            step = next;
        }
        assert_eq!(parts, [HELMET, CHESTPLATE, GAUNTLETS, BOOTS]);
        assert_eq!(step, ShellStep::Boots);
        assert_eq!(
            step.toggle(ToolKind::Wrench).map(|t| t.0),
            Some(ShellStep::WrenchedAssembly)
        );
        assert_eq!(
            ShellStep::WrenchedAssembly
                .toggle(ToolKind::Screwdriver)
                .map(|t| t.0),
            Some(ShellStep::ScrewedAssembly)
        );
        assert!(ShellStep::Start.toggle(ToolKind::Screwdriver).is_none());
        assert_eq!(ShellStep::Core.back().map(|b| b.0), Some(ShellStep::Start));
        assert!(ShellStep::ScrewedCore.back().is_none());
    }
}
