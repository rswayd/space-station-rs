//! Stealing the plutonium core (server), after tg `theft_tools.dm` (`/obj/item/nuke_core`,
//! `/obj/item/nuke_core_container`, `/obj/item/screwdriver/nuke`) and the deconstruction half
//! of `/obj/machinery/nuclearbomb` (`deconstruction_state`, `item_interaction`,
//! `screwdriver_act`, `crowbar_act`, `welder_act`).
//!
//! Every bomb holds a dormant core ([`NukeInterior`]). Only the ultra-thin screwdriver
//! unscrews the front panel (6 s; back in 8 s); a crowbar pries it off (3 s) or levers it back;
//! a lit welder cuts the inner plate (8 s); a crowbar pries the plate off (3 s, 15 iron sheets
//! drop) and the exposed core starts pulsing radiation (range 2, every 6 s). 20 iron sheets
//! repair the plate (10 s, the core goes dormant again). A core container takes the core out
//! (5 s) and seals 5 s later, which stops the radiation; a loaded container (15 s) or a bare
//! core (6 s) puts one back into an emptied bomb. A core and an empty container clicked on each
//! other load it the same way.
// shortcut: no right-click warning about the bare core (`attack_hand_secondary`), no flicked
// pulse sprite, no suicide; the supermatter sliver and its container live with the supermatter.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::bomb::{NuclearBomb, update_look};
use crate::click_handlers::{AddClickHandler, ClickCtx, ClickPhase};
use crate::construction::Job;
use crate::inventory::CarriedBy;
use crate::obj_damage::is_under;
use crate::radiation::{RAD_EXTREME_INSULATION, RadioactiveEmitter};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, ToolKind, tell, tell_mob, tool_of, tool_usable};

/// tg `/obj/item/nuke_core`.
pub const NUKE_CORE: &str = "/obj/item/nuke_core";
/// `/obj/item/nuke_core/supermatter_sliver`: not a plutonium core for the bomb or the box.
const SLIVER: &str = "/obj/item/nuke_core/supermatter_sliver";
/// tg `/obj/item/nuke_core_container`.
pub const CORE_CONTAINER: &str = "/obj/item/nuke_core_container";
/// `/obj/item/nuke_core_container/supermatter`: the sliver's own box.
const SM_CONTAINER: &str = "/obj/item/nuke_core_container/supermatter";
/// tg `/obj/item/screwdriver/nuke`: the only screwdriver the bomb's panel takes.
pub const NUKE_SCREWDRIVER: &str = "/obj/item/screwdriver/nuke";
const IRON: &str = "/obj/item/stack/sheet/iron";

/// `process`: `cooldown < world.time - 60`, `radiation_pulse(max_range = 2,
/// threshold = RAD_EXTREME_INSULATION)`.
const CORE_PULSE_SECS: f64 = 6.0;
const CORE_PULSE_RANGE: f32 = 2.0;
/// `addtimer(seal, 5 SECONDS)`.
const SEAL_SECS: f64 = 5.0;
/// Iron sheets the inner plate gives (`crowbar_act`) and takes back (`item_interaction`).
const PLATE_DROP: u32 = 15;
const PLATE_REPAIR: u32 = 20;

/// tg `NUKESTATE_*`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Decon {
    #[default]
    Intact,
    Unscrewed,
    PanelRemoved,
    Welded,
    CoreExposed,
    CoreRemoved,
}

impl Decon {
    /// `update_overlays`' `interior`.
    pub fn interior_overlay(self) -> Option<&'static str> {
        match self {
            Self::Intact => None,
            Self::Unscrewed => Some("panel-unscrewed"),
            Self::PanelRemoved => Some("panel-removed"),
            Self::Welded => Some("plate-welded"),
            Self::CoreExposed => Some("plate-removed"),
            Self::CoreRemoved => Some("core-removed"),
        }
    }

    /// `examine`'s deconstruction line.
    fn examine(self) -> (Span, &'static str) {
        match self {
            Self::Intact => (Span::Notice, "Передняя панель закреплена."),
            Self::Unscrewed => (
                Span::Notice,
                "Передняя панель откручена, её можно поддеть ломом.",
            ),
            Self::PanelRemoved => (
                Span::Notice,
                "Внутренняя пластина открыта, её можно разрезать сваркой.",
            ),
            Self::Welded => (
                Span::Notice,
                "Внутренняя пластина прорезана, её можно поддеть ломом.",
            ),
            Self::CoreExposed => (
                Span::Danger,
                "Внутренняя камера вскрыта, плутониевое ядро открыто! Повреждённую пластину можно заменить железом.",
            ),
            Self::CoreRemoved => (Span::Notice, "Внутренняя камера вскрыта, но пуста."),
        }
    }
}

/// The bomb's `deconstruction_state` and `core`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NukeInterior {
    pub state: Decon,
    pub core: Option<Entity>,
}

/// `/obj/item/nuke_core_container`'s `core` and pending `seal`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CoreContainer {
    pub core: Option<Entity>,
    pub sealed: bool,
    seal_at: Option<f64>,
}

/// A timed step on the bomb; each one applies only from the state it started in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreJob {
    Unscrew,
    Screw,
    PryPanel,
    LeverPanel,
    CutPlate,
    PryPlate,
    RepairPlate,
    /// A container takes the core.
    LoadCore,
    /// A loaded container's core goes back.
    PlaceFromBox,
    /// A bare core goes back.
    PlaceCore,
}

impl CoreJob {
    fn from(self) -> Decon {
        match self {
            Self::Unscrew => Decon::Intact,
            Self::Screw | Self::PryPanel => Decon::Unscrewed,
            Self::LeverPanel | Self::CutPlate => Decon::PanelRemoved,
            Self::PryPlate => Decon::Welded,
            Self::RepairPlate | Self::LoadCore => Decon::CoreExposed,
            Self::PlaceFromBox | Self::PlaceCore => Decon::CoreRemoved,
        }
    }
}

pub struct CoreTheftPlugin;

impl Plugin for CoreTheftPlugin {
    fn build(&self, app: &mut App) {
        app.add_click_handler(ClickPhase::HeldOnTarget, 261, held_on_target)
            .add_click_handler(ClickPhase::OtherHand, 471, other_hand)
            .add_systems(ss13_world::MapInit, loose_cores)
            .add_systems(
                Update,
                seal_containers.run_if(any_with_component::<CoreContainer>),
            );
    }
}

fn item_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

fn is_core(world: &World, e: Entity) -> bool {
    item_path(world, e).is_some_and(|p| is_under(p, NUKE_CORE) && !is_under(p, SLIVER))
}

fn is_container(world: &World, e: Entity) -> bool {
    item_path(world, e).is_some_and(|p| is_under(p, CORE_CONTAINER) && !is_under(p, SM_CONTAINER))
}

/// `nuclearbomb/Initialize`: `core = new(src)`, `STOP_PROCESSING(SSobj, core)`.
pub fn setup_bomb(world: &mut World, bomb: Entity) {
    let core = crate::construction::spawn_detached(world, NUKE_CORE);
    if let Some(core) = core {
        crate::storage::put_inside(world, core, bomb);
    }
    world.entity_mut(bomb).insert(NukeInterior {
        state: Decon::Intact,
        core,
    });
}

/// `nuke_core/Initialize`: `START_PROCESSING` for a core lying on the map.
fn loose_cores(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmItem), With<ss13_world::Mapped>>();
    let cores: Vec<Entity> = query
        .iter(world)
        .filter(|(_, i)| is_under(&i.path, NUKE_CORE))
        .map(|(e, _)| e)
        .collect();
    for core in cores {
        set_pulsing(world, core, true);
    }
}

/// `START_PROCESSING`/`STOP_PROCESSING(SSobj, core)`.
fn set_pulsing(world: &mut World, core: Entity, on: bool) {
    let Ok(mut entity) = world.get_entity_mut(core) else {
        return;
    };
    if on {
        entity.insert(RadioactiveEmitter::new(
            CORE_PULSE_SECS,
            CORE_PULSE_RANGE,
            RAD_EXTREME_INSULATION,
            None,
        ));
    } else {
        entity.remove::<RadioactiveEmitter>();
    }
}

fn set_state(world: &mut World, bomb: Entity, state: Decon) {
    if let Some(mut interior) = world.get_mut::<NukeInterior>(bomb) {
        interior.state = state;
    }
    update_look(world, bomb);
}

fn sound(world: &mut World, at: Entity, file: &str, vol: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, at) {
        playsound(world, PlaySound::new(pos, file, vol, true));
    }
}

fn tool_sound(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Crowbar => "sound/items/tools/crowbar.ogg",
        ToolKind::Welder => "sound/items/tools/welder.ogg",
        _ => "sound/items/tools/screwdriver.ogg",
    }
}

/// A world click: tools and cores on a bomb, a core and a container on each other.
fn held_on_target(world: &mut World, ctx: &ClickCtx) -> bool {
    let Some(held) = ctx.held else {
        return false;
    };
    let actor = Actor {
        client: ctx.client,
        mob: ctx.mob,
        item: Some(held),
    };
    if world.get::<NuclearBomb>(ctx.target).is_some() {
        return on_bomb(world, actor, ctx.target, held);
    }
    held_on_item(world, ctx.client, ctx.mob, held, ctx.target)
}

/// The other hand's container or core clicked with the matching item.
fn other_hand(world: &mut World, ctx: &ClickCtx) -> bool {
    let Some(held) = ctx.held else {
        return false;
    };
    held_on_item(world, ctx.client, ctx.mob, held, ctx.target)
}

/// `nuke_core/item_interaction` and `nuke_core_container/item_interaction`.
fn held_on_item(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if is_container(world, held) && is_core(world, target) {
        load(world, held, target, Some(mob));
        return true;
    }
    if is_core(world, held) && is_container(world, target) {
        if crate::inventory::no_drop(world, held) {
            tell(
                world,
                client,
                Span::Warning,
                "Ядро застревает на вашей руке!",
            );
            return true;
        }
        load(world, target, held, Some(mob));
        return true;
    }
    false
}

/// The bomb's `item_interaction` (cores and containers, iron) and tool acts.
fn on_bomb(world: &mut World, actor: Actor, bomb: Entity, held: Entity) -> bool {
    let Some(interior) = world.get::<NukeInterior>(bomb).copied() else {
        return false;
    };
    let say = |world: &mut World, text: &str| tell(world, actor.client, Span::Notice, text);
    match interior.state {
        Decon::CoreExposed if is_container(world, held) => {
            say(
                world,
                "Вы начинаете загружать плутониевое ядро в контейнер...",
            );
            crate::tools::do_after(
                world,
                actor,
                bomb,
                50.0,
                0,
                Job::NukeCore(CoreJob::LoadCore),
            );
            return true;
        }
        Decon::CoreExposed if item_path(world, held).is_some_and(|p| is_under(p, IRON)) => {
            if !tool_usable(world, held, PLATE_REPAIR, 0.0) {
                let text = crate::tools::unusable_message(world, held, PLATE_REPAIR);
                tell(world, actor.client, Span::Warning, text);
                return true;
            }
            say(
                world,
                "Вы начинаете чинить внутреннюю металлическую пластину...",
            );
            crate::tools::use_tool(
                world,
                actor,
                bomb,
                100.0,
                PLATE_REPAIR,
                Job::NukeCore(CoreJob::RepairPlate),
            );
            return true;
        }
        Decon::CoreRemoved
            if is_container(world, held)
                && world
                    .get::<CoreContainer>(held)
                    .is_some_and(|c| c.core.is_some()) =>
        {
            say(
                world,
                "Вы вскрываете контейнер и начинаете помещать ядро во внутреннюю камеру...",
            );
            crate::tools::do_after(
                world,
                actor,
                bomb,
                150.0,
                0,
                Job::NukeCore(CoreJob::PlaceFromBox),
            );
            return true;
        }
        Decon::CoreRemoved if is_core(world, held) => {
            say(world, "Вы начинаете помещать ядро во внутреннюю камеру...");
            crate::tools::do_after(
                world,
                actor,
                bomb,
                60.0,
                0,
                Job::NukeCore(CoreJob::PlaceCore),
            );
            return true;
        }
        _ => {}
    }
    let Some((kind, _)) = tool_of(world, held) else {
        return false;
    };
    let (job, delay, text) = match (kind, interior.state) {
        (ToolKind::Screwdriver, Decon::Intact | Decon::Unscrewed)
            if !item_path(world, held).is_some_and(|p| is_under(p, NUKE_SCREWDRIVER)) =>
        {
            return false;
        }
        (ToolKind::Screwdriver, Decon::Intact) => (
            CoreJob::Unscrew,
            60.0,
            "Вы начинаете выкручивать винты передней панели...",
        ),
        (ToolKind::Screwdriver, Decon::Unscrewed) => (
            CoreJob::Screw,
            80.0,
            "Вы начинаете прикручивать переднюю панель обратно...",
        ),
        (ToolKind::Crowbar, Decon::Unscrewed) => (
            CoreJob::PryPanel,
            30.0,
            "Вы начинаете снимать переднюю панель...",
        ),
        (ToolKind::Crowbar, Decon::Welded) => (
            CoreJob::PryPlate,
            30.0,
            "Вы начинаете поддевать внутреннюю пластину...",
        ),
        (ToolKind::Crowbar, Decon::PanelRemoved) => (
            CoreJob::LeverPanel,
            30.0,
            "Вы начинаете возвращать внутреннюю панель на место...",
        ),
        (ToolKind::Welder, Decon::PanelRemoved) => {
            // `tool_start_check(user, amount = 1)`.
            if !tool_usable(world, held, 1, 0.0) {
                let text = crate::tools::unusable_message(world, held, 1);
                tell(world, actor.client, Span::Warning, text);
                return true;
            }
            (
                CoreJob::CutPlate,
                80.0,
                "Вы начинаете резать внутреннюю пластину...",
            )
        }
        _ => return false,
    };
    say(world, text);
    if crate::tools::use_tool(world, actor, bomb, delay, 0, Job::NukeCore(job)) {
        sound(world, bomb, tool_sound(kind), 100.0);
    }
    true
}

/// A finished step, re-checked against the bomb's state now.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: CoreJob) {
    let bomb = action.target;
    let client = action.client;
    let Some(interior) = world.get::<NukeInterior>(bomb).copied() else {
        return;
    };
    if interior.state != job.from() {
        return;
    }
    let say = |world: &mut World, text: &str| tell(world, client, Span::Notice, text);
    match job {
        CoreJob::Unscrew => {
            set_state(world, bomb, Decon::Unscrewed);
            say(world, "Вы выкручиваете винты передней панели.");
        }
        CoreJob::Screw => {
            set_state(world, bomb, Decon::Intact);
            say(world, "Вы прикручиваете переднюю панель на место.");
        }
        CoreJob::PryPanel => {
            set_state(world, bomb, Decon::PanelRemoved);
            say(world, "Вы снимаете переднюю панель.");
        }
        CoreJob::LeverPanel => {
            set_state(world, bomb, Decon::Unscrewed);
            say(world, "Вы возвращаете внутреннюю панель на место.");
        }
        CoreJob::CutPlate => {
            set_state(world, bomb, Decon::Welded);
            say(world, "Вы прорезаете внутреннюю пластину.");
        }
        CoreJob::PryPlate => {
            if let Some(core) = interior.core {
                say(
                    world,
                    "Вы снимаете внутреннюю пластину. Видно зелёное свечение ядра!",
                );
                set_pulsing(world, core, true);
                set_state(world, bomb, Decon::CoreExposed);
            } else {
                say(
                    world,
                    "Вы снимаете внутреннюю пластину. Внутренняя камера пуста, если не считать пятен от пива.",
                );
                set_state(world, bomb, Decon::CoreRemoved);
            }
            if let Some(&pos) = world.get::<TilePos>(bomb) {
                crate::construction::spawn_stack(world, IRON, pos, PLATE_DROP);
            }
        }
        CoreJob::RepairPlate => {
            if !action
                .item
                .is_some_and(|i| crate::tools::consume(world, i, action.amount))
            {
                return;
            }
            say(
                world,
                "Вы чините внутреннюю металлическую пластину. Радиация сдержана.",
            );
            if let Some(core) = interior.core {
                set_pulsing(world, core, false);
            }
            set_state(world, bomb, Decon::PanelRemoved);
        }
        CoreJob::LoadCore => {
            let (Some(box_), Some(core)) = (action.item, interior.core) else {
                return;
            };
            if !is_container(world, box_) || !load(world, box_, core, Some(mob)) {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Не удаётся загрузить плутониевое ядро: контейнер уже использован!",
                );
                return;
            }
            say(world, "Вы загружаете плутониевое ядро в контейнер.");
            if let Some(mut i) = world.get_mut::<NukeInterior>(bomb) {
                i.core = None;
            }
            set_state(world, bomb, Decon::CoreRemoved);
        }
        CoreJob::PlaceFromBox => {
            let Some(box_) = action.item else {
                return;
            };
            let Some(core) = world.get::<CoreContainer>(box_).and_then(|c| c.core) else {
                return;
            };
            crate::storage::put_inside(world, core, bomb);
            world.entity_mut(box_).insert(CoreContainer::default());
            crate::obj_damage::set_icon_state(world, box_, "core_container_empty");
            say(world, "Вы помещаете ядро во внутреннюю камеру.");
            place_core(world, bomb, core);
        }
        CoreJob::PlaceCore => {
            let Some(core) = action.item.filter(|&c| is_core(world, c)) else {
                return;
            };
            crate::storage::put_inside(world, core, bomb);
            say(world, "Вы помещаете ядро во внутреннюю камеру.");
            place_core(world, bomb, core);
        }
    }
}

/// tg puts a core back without `START_PROCESSING`: it pulses only if it already did.
fn place_core(world: &mut World, bomb: Entity, core: Entity) {
    if let Some(mut i) = world.get_mut::<NukeInterior>(bomb) {
        i.core = Some(core);
    }
    set_state(world, bomb, Decon::CoreExposed);
}

/// `nuke_core_container/load`: an empty container takes `core` and seals in 5 s.
fn load(world: &mut World, container: Entity, core: Entity, user: Option<Entity>) -> bool {
    if world
        .get::<CoreContainer>(container)
        .is_some_and(|c| c.core.is_some())
        || !is_core(world, core)
    {
        return false;
    }
    let now = crate::tools::now(world);
    crate::storage::put_inside(world, core, container);
    world.entity_mut(container).insert(CoreContainer {
        core: Some(core),
        sealed: false,
        seal_at: Some(now + SEAL_SECS),
    });
    crate::obj_damage::set_icon_state(world, container, "core_container_loaded");
    if let Some(user) = user {
        tell_mob(world, user, Span::Warning, "Контейнер герметизируется...");
    }
    true
}

/// `nuke_core_container/seal`, 5 s after loading: the core inside stops pulsing.
fn seal_containers(world: &mut World) {
    let now = crate::tools::now(world);
    let mut query = world.query::<(Entity, &CoreContainer)>();
    let due: Vec<(Entity, CoreContainer)> = query
        .iter(world)
        .filter(|(_, c)| c.seal_at.is_some_and(|at| now >= at))
        .map(|(e, c)| (e, *c))
        .collect();
    for (container, mut state) in due {
        state.seal_at = None;
        let inside = state.core.filter(|&core| {
            world
                .get::<crate::storage::StoredIn>(core)
                .is_some_and(|s| s.0 == container)
        });
        if let Some(core) = inside {
            state.sealed = true;
            set_pulsing(world, core, false);
            crate::obj_damage::set_icon_state(world, container, "core_container_sealed");
            sound(world, container, "sound/items/deconstruct.ogg", 60.0);
            if let Some(&CarriedBy(holder)) = world.get::<CarriedBy>(container) {
                tell_mob(
                    world,
                    holder,
                    Span::Warning,
                    "Контейнер надёжно запечатывается, радиация плутониевого ядра сдерживается.",
                );
            }
        }
        world.entity_mut(container).insert(state);
    }
}

/// The bomb's deconstruction line.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(interior) = world.get::<NukeInterior>(target) {
        let (span, text) = interior.state.examine();
        lines.push((span, text.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_job_starts_from_its_state() {
        assert_eq!(CoreJob::Unscrew.from(), Decon::Intact);
        assert_eq!(CoreJob::PryPanel.from(), Decon::Unscrewed);
        assert_eq!(CoreJob::CutPlate.from(), Decon::PanelRemoved);
        assert_eq!(CoreJob::PryPlate.from(), Decon::Welded);
        assert_eq!(CoreJob::LoadCore.from(), Decon::CoreExposed);
        assert_eq!(CoreJob::PlaceCore.from(), Decon::CoreRemoved);
        assert_eq!(Decon::Intact.interior_overlay(), None);
        assert_eq!(Decon::CoreExposed.interior_overlay(), Some("plate-removed"));
    }
}
