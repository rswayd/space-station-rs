//! The AI core structure (server), after tg `/obj/structure/ai_core`
//! (`code/game/machinery/ai_core/_core.dm`, `core_construction.dm`) and the AI's
//! `create_core_link`, `resolve_core_link`, `break_core_link` (`ai.dm`).
//!
//! - Construction, one state after the other: the AI core board into the empty frame (crowbar
//!   pries it out), screwdriver secures it (and unsecures it), 5 cable (2 s; wirecutters take
//!   them back), a brain processor (MMI or positronic brain; crowbar pries it out), 2 reinforced
//!   glass (2 s; crowbar takes it back), then a screwdriver on the anchored frame: a processor
//!   with a present mind becomes the AI (tg `ai_structure_to_mob`: the processor and the frame
//!   are gone, the AI starts like the AI job's, without its position), else the core is finished
//!   and empty (wirecutters back to glassed); a screwdriver on a finished core whose processor got
//!   its mind back connects it after 10 s. A welder (2 s) melts an empty frame into 4 plasteel; a
//!   wrench (2 s) bolts or unbolts every state but the finished one.
//! - An empty finished core is what a carded AI is uploaded into (`super::aicard`) and what a
//!   shunted malfunctioning AI returns to (`super::malf`): carding and shunting leave one behind
//!   (tg `new /obj/structure/ai_core(loc, CORE_STATE_FINISHED, make_mmi())`); a shunted AI's core
//!   tells it about tampering, and its destruction cuts the link.
// shortcut: the core left behind holds no MMI copy (tg `make_mmi`), only its board; the board
// keeps no backup battery; no suicide check, AI job ban or `allow_ai` config check; an inactive
// processor goes in without tg's "make an inactive AI?" question; a mind that ghosted out of its
// processor is not connected (only a present client); no multitool buffer link to a rack, no
// latejoin core transmitters; messages are chat lines (tg balloon alerts); deconstruction by
// damage drops nothing.

use bevy::prelude::*;
use ss13_mapload::{DmItem, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};

use super::ai::LinkedCore;
use super::chassis::tell_ai;
use crate::combat::CombatMode;
use crate::construction::Job;
use crate::medical::brainmob::{self, Mind, Occupant};
use crate::obj_damage::{Anchored, is_under};
use crate::sound::{PlaySound, playsound};
use crate::storage::{put_inside, take_out};
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of, tool_usable, unusable_message};

pub const AI_CORE: &str = "/obj/structure/ai_core";
const LATEJOIN_CORE: &str = "/obj/structure/ai_core/latejoin_inactive";
pub const AI_CORE_BOARD: &str = "/obj/item/circuitboard/aicore";
pub const RGLASS: &str = "/obj/item/stack/sheet/rglass";
pub const PLASTEEL: &str = "/obj/item/stack/sheet/plasteel";
const CABLE: &str = "/obj/item/stack/cable_coil";
const AI_DMI: &str = "icons/mob/silicon/ai.dmi";

/// tg `CORE_STATE_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CoreState {
    Empty = 0,
    Circuit = 1,
    Screwed = 2,
    Cabled = 3,
    Glassed = 4,
    Finished = 5,
}

/// The frame's state and contents (tg `state`, `circuit`, `core_mmi`); made on first use.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct CoreFrame {
    pub state: CoreState,
    pub board: Option<Entity>,
    pub mmi: Option<Entity>,
    /// The shunted AI this emptied core belongs to (tg the AI's `linked_core`).
    pub linked_ai: Option<Entity>,
}

/// A timed construction step and the state it started from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreJob {
    /// Welder on an empty frame.
    Melt,
    /// Wrench: anchored before.
    Unfasten { anchored: bool },
    /// 5 cable on a screwed board.
    Cable,
    /// 2 reinforced glass on a cabled frame with a processor.
    Glass,
    /// Screwdriver on a finished core with an active processor.
    Connect,
}

pub struct AiCorePlugin;

impl Plugin for AiCorePlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Tools, boards, cable, processors and glass on an AI core frame.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1460, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_observer(frame_removed);
    }
}

fn path_of(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

/// Whether `e` is an AI core frame.
pub fn is_core(world: &World, e: Entity) -> bool {
    world.get::<CoreFrame>(e).is_some() || is_under(&path_of(world, e), AI_CORE)
}

/// The frame's state, made from its type on first use (`latejoin_inactive` is finished and
/// anchored, the base type empty and loose).
pub fn frame(world: &mut World, e: Entity) -> Option<CoreFrame> {
    if let Some(f) = world.get::<CoreFrame>(e) {
        return Some(f.clone());
    }
    let path = path_of(world, e);
    if !is_under(&path, AI_CORE) {
        return None;
    }
    let finished = is_under(&path, LATEJOIN_CORE);
    let state = CoreFrame {
        state: if finished {
            CoreState::Finished
        } else {
            CoreState::Empty
        },
        board: None,
        mmi: None,
        linked_ai: None,
    };
    if world.get::<Anchored>(e).is_none() {
        world.entity_mut(e).insert(Anchored(finished));
    }
    world.entity_mut(e).insert(state.clone());
    Some(state)
}

use crate::obj_damage::anchored;

fn set_state(world: &mut World, core: Entity, state: CoreState) {
    if let Some(mut f) = world.get_mut::<CoreFrame>(core) {
        f.state = state;
    }
    refresh_look(world, core);
}

/// tg `update_icon_state`: `build_<state>` (`build_3b` with a processor), finished `ai-core`
/// with the `ai-empty` screen.
fn refresh_look(world: &mut World, core: Entity) {
    let Some(f) = world.get::<CoreFrame>(core).cloned() else {
        return;
    };
    let (state, screen) = match f.state {
        CoreState::Finished => ("ai-core".to_owned(), true),
        CoreState::Cabled if f.mmi.is_some() => ("build_3b".to_owned(), false),
        s => (format!("build_{}", s as u8), false),
    };
    let Some(icons) = world.get_resource::<IconIndex>() else {
        return;
    };
    let base = icons.get(AI_DMI, &state);
    let empty = icons.get(AI_DMI, "ai-empty");
    let Some(mut look) = world.get_mut::<Appearance>(core) else {
        return;
    };
    if let Some(base) = base
        && look.icon_state != base
    {
        look.icon_state = base;
    }
    let mut overlays = look.overlays.clone();
    overlays.retain(|o| Some(o.icon_state) != empty);
    if screen {
        overlays.extend(empty.map(Overlay::new));
    }
    if look.overlays != overlays {
        look.overlays = overlays;
    }
}

/// A finished, empty, anchored core on `pos` with a board (tg `new /obj/structure/ai_core(loc,
/// CORE_STATE_FINISHED, ...)`). `None` without its prototype.
pub fn spawn_empty_core(world: &mut World, pos: TilePos) -> Option<Entity> {
    let core = crate::construction::spawn_obj(world, AI_CORE, pos)?;
    let board = crate::construction::spawn_detached(world, AI_CORE_BOARD);
    if let Some(board) = board {
        put_inside(world, board, core);
    }
    world.entity_mut(core).insert((
        CoreFrame {
            state: CoreState::Finished,
            board,
            mmi: None,
            linked_ai: None,
        },
        Anchored(true),
    ));
    refresh_look(world, core);
    Some(core)
}

/// tg `create_core_link`: the shunted `ai` belongs to `core`.
pub fn link(world: &mut World, ai: Entity, core: Entity) {
    if let Some(mut f) = world.get_mut::<CoreFrame>(core) {
        f.linked_ai = Some(ai);
    }
    world.entity_mut(ai).insert(LinkedCore(core));
}

/// tg `resolve_core_link`: the AI mob comes out of its container onto its linked core's tile and
/// the core is gone. False without a linked core.
pub fn resolve_link(world: &mut World, ai: Entity) -> bool {
    let Some(LinkedCore(core)) = world.get::<LinkedCore>(ai).copied() else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(core) else {
        world.entity_mut(ai).remove::<LinkedCore>();
        return false;
    };
    world.entity_mut(ai).remove::<LinkedCore>();
    if let Some(mut f) = world.get_mut::<CoreFrame>(core) {
        f.linked_ai = None;
    }
    despawn_core(world, core);
    take_out(world, ai, pos);
    crate::construction::tile_changed(world, pos);
    true
}

/// The core and what is inside it (tg `qdel(src)`, `QDEL_NULL(circuit)`, `QDEL_NULL(core_mmi)`).
pub fn despawn_core(world: &mut World, core: Entity) {
    let Some(f) = world.get::<CoreFrame>(core).cloned() else {
        world.try_despawn(core).ok();
        return;
    };
    if let Some(mmi) = f.mmi {
        let brain = world
            .get::<super::mmi::BrainProcessor>(mmi)
            .and_then(|p| p.brain);
        super::mmi::set_brainmob(world, mmi, None);
        if let Some(brain) = brain {
            world.try_despawn(brain).ok();
        }
        world.try_despawn(mmi).ok();
    }
    if let Some(board) = f.board {
        world.try_despawn(board).ok();
    }
    let pos = world.get::<TilePos>(core).copied();
    world.try_despawn(core).ok();
    if let Some(pos) = pos {
        crate::construction::tile_changed(world, pos);
    }
}

/// tg `on_core_destroyed` / `break_core_link`: a destroyed linked core tells its shunted AI.
fn frame_removed(remove: On<Remove<CoreFrame>>, frames: Query<&CoreFrame>, mut commands: Commands) {
    let Some(ai) = frames.get(remove.entity).ok().and_then(|f| f.linked_ai) else {
        return;
    };
    let core = remove.entity;
    commands.queue(move |world: &mut World| {
        if world.get::<LinkedCore>(ai) != Some(&LinkedCore(core)) {
            return;
        }
        world.entity_mut(ai).remove::<LinkedCore>();
        tell_ai(world, ai, Span::Danger, "Your core has been destroyed!");
    });
}

/// The held `tool` on an AI core frame. False for anything that is not one.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tool: Entity,
    target: Entity,
) -> bool {
    if !is_core(world, target) {
        return false;
    }
    if world.get::<DoAfter>(mob).is_some() {
        return true;
    }
    let Some(f) = frame(world, target) else {
        return false;
    };
    // tg `on_core_item_interaction` of a shunted AI's core.
    if let Some(ai) = f.linked_ai
        && !world.get::<CombatMode>(mob).is_some_and(|c| c.0)
    {
        tell_ai(world, ai, Span::Danger, "CORE TAMPERING DETECTED!");
    }
    let actor = Actor {
        client,
        mob,
        item: Some(tool),
    };
    if let Some((kind, _)) = tool_of(world, tool) {
        return tool_act(world, actor, kind, target, &f);
    }
    if f.state == CoreState::Finished {
        return false;
    }
    let path = path_of(world, tool);
    let say = |world: &mut World, span: Span, text: &str| tell(world, client, span, text);
    if is_under(&path, AI_CORE_BOARD) {
        if f.state != CoreState::Empty {
            return false;
        }
        put_inside(world, tool, target);
        if let Some(mut fr) = world.get_mut::<CoreFrame>(target) {
            fr.board = Some(tool);
        }
        set_state(world, target, CoreState::Circuit);
        sound(world, target, "sound/items/deconstruct.ogg");
        return true;
    }
    if is_under(&path, CABLE) {
        if f.state != CoreState::Screwed {
            return false;
        }
        if !tool_usable(world, tool, 5, 0.0) {
            say(world, Span::Warning, "not enough cable!");
            return true;
        }
        say(world, Span::Notice, "adding cable...");
        crate::tools::use_tool(world, actor, target, 20.0, 5, Job::AiCore(CoreJob::Cable));
        return true;
    }
    if is_under(&path, super::mmi::PROCESSOR) {
        if f.state != CoreState::Cabled || f.mmi.is_some() {
            return false;
        }
        install_processor(world, tool, target);
        return true;
    }
    if is_under(&path, RGLASS) {
        if f.state != CoreState::Cabled {
            return false;
        }
        if f.mmi.is_none() {
            say(world, Span::Warning, "needs a processor!");
            return true;
        }
        if !tool_usable(world, tool, 2, 0.0) {
            say(world, Span::Warning, "not enough reinforced glass!");
            return true;
        }
        crate::tools::use_tool(world, actor, target, 20.0, 2, Job::AiCore(CoreJob::Glass));
        return true;
    }
    false
}

fn sound(world: &mut World, core: Entity, file: &str) {
    if let Some(&pos) = world.get::<TilePos>(core) {
        playsound(world, PlaySound::new(pos, file, 50.0, true));
    }
}

/// tg `install_mmi`.
fn install_processor(world: &mut World, processor: Entity, core: Entity) {
    if world.get::<super::mmi::BrainProcessor>(processor).is_none()
        && super::mmi::processor(world, processor).is_none()
    {
        return;
    }
    put_inside(world, processor, core);
    if let Some(mut f) = world.get_mut::<CoreFrame>(core) {
        f.mmi = Some(processor);
    }
    refresh_look(world, core);
}

/// The processor's brainmob with a present client (tg `core_mmi.brainmob.mind`).
fn active_mind(world: &mut World, f: &CoreFrame) -> Option<(Entity, Entity)> {
    let Occupant(mob) = world.get::<Occupant>(f.mmi?).copied()?;
    match brainmob::mind_of(world, mob)? {
        Mind::Client(client) => Some((mob, client)),
        Mind::Ghost(_) => None,
    }
}

/// Whether the core's processor holds a mind (tg `core_mmi?.brainmob?.mind`).
pub fn has_mind(world: &mut World, core: Entity) -> bool {
    let Some(f) = world.get::<CoreFrame>(core).cloned() else {
        return false;
    };
    f.mmi
        .and_then(|m| world.get::<Occupant>(m).copied())
        .is_some_and(|Occupant(mob)| brainmob::mind_of(world, mob).is_some())
}

fn tool_act(world: &mut World, actor: Actor, kind: ToolKind, core: Entity, f: &CoreFrame) -> bool {
    let client = actor.client;
    let tool = actor.item.expect("a tool");
    let say = |world: &mut World, span: Span, text: &str| tell(world, client, span, text);
    let drop_at = world.get::<TilePos>(core).copied();
    match kind {
        ToolKind::Welder => {
            if f.state != CoreState::Empty {
                say(world, Span::Warning, "frame has to be empty!");
            } else if !tool_usable(world, tool, 1, 0.0) {
                let text = unusable_message(world, tool, 1);
                say(world, Span::Warning, &text);
            } else {
                crate::tools::use_tool(world, actor, core, 20.0, 1, Job::AiCore(CoreJob::Melt));
            }
        }
        ToolKind::Wrench => {
            if f.state == CoreState::Finished {
                say(
                    world,
                    Span::Warning,
                    "can't be unanchored while operational!",
                );
            } else {
                let anchored = anchored(world, core);
                crate::tools::use_tool(
                    world,
                    actor,
                    core,
                    20.0,
                    0,
                    Job::AiCore(CoreJob::Unfasten { anchored }),
                );
            }
        }
        ToolKind::Screwdriver => match f.state {
            CoreState::Empty => say(world, Span::Warning, "nothing to screw in there!"),
            CoreState::Circuit => {
                set_state(world, core, CoreState::Screwed);
                say(world, Span::Notice, "board secured");
            }
            CoreState::Screwed => {
                set_state(world, core, CoreState::Circuit);
                say(world, Span::Notice, "board unsecured");
            }
            CoreState::Cabled => say(world, Span::Warning, "can't reach the board!"),
            CoreState::Glassed => {
                if !anchored(world, core) {
                    say(world, Span::Warning, "isn't anchored!");
                } else if active_mind(world, f).is_some() {
                    if structure_to_mob(world, core).is_some() {
                        say(world, Span::Notice, "connected monitor and neural network");
                    }
                } else {
                    set_state(world, core, CoreState::Finished);
                    say(world, Span::Notice, "connected monitor");
                }
            }
            CoreState::Finished => {
                if active_mind(world, f).is_none() {
                    say(world, Span::Warning, "processor is inactive!");
                } else if !anchored(world, core) {
                    say(world, Span::Warning, "anchor it first!");
                } else {
                    say(world, Span::Notice, "connecting neural network...");
                    crate::tools::use_tool(
                        world,
                        actor,
                        core,
                        100.0,
                        0,
                        Job::AiCore(CoreJob::Connect),
                    );
                }
            }
        },
        ToolKind::Crowbar => match f.state {
            CoreState::Empty => say(world, Span::Warning, "nothing to pry out!"),
            CoreState::Circuit => {
                if let (Some(board), Some(at)) = (f.board, drop_at) {
                    take_out(world, board, at);
                }
                if let Some(mut fr) = world.get_mut::<CoreFrame>(core) {
                    fr.board = None;
                }
                set_state(world, core, CoreState::Empty);
            }
            CoreState::Screwed => say(world, Span::Warning, "won't budge!"),
            CoreState::Cabled => match (f.mmi, drop_at) {
                (Some(mmi), Some(at)) => {
                    take_out(world, mmi, at);
                    if let Some(mut fr) = world.get_mut::<CoreFrame>(core) {
                        fr.mmi = None;
                    }
                    refresh_look(world, core);
                }
                _ => say(world, Span::Warning, "nothing to pry out!"),
            },
            CoreState::Glassed => {
                if let Some(at) = drop_at {
                    crate::construction::spawn_stack(world, RGLASS, at, 2);
                }
                set_state(world, core, CoreState::Cabled);
            }
            CoreState::Finished => say(world, Span::Warning, "display is on!"),
        },
        ToolKind::Wirecutters => match f.state {
            CoreState::Empty | CoreState::Circuit => say(world, Span::Warning, "nothing to cut!"),
            CoreState::Screwed => return false,
            CoreState::Cabled => {
                if f.mmi.is_some() {
                    say(world, Span::Warning, "processor in the way!");
                } else {
                    if let Some(at) = drop_at {
                        crate::construction::spawn_stack(world, CABLE, at, 5);
                    }
                    set_state(world, core, CoreState::Screwed);
                }
            }
            CoreState::Glassed => say(world, Span::Warning, "nothing left to cut!"),
            CoreState::Finished => set_state(world, core, CoreState::Glassed),
        },
        ToolKind::Multitool => return false,
    }
    true
}

/// The timed step's effect, if the frame is still in the state it started from (tg
/// `CHECK_STATE_CALLBACK`).
pub fn complete(world: &mut World, _mob: Entity, action: &DoAfter, job: CoreJob) {
    let core = action.target;
    let Some(f) = world.get::<CoreFrame>(core).cloned() else {
        return;
    };
    let consume = |world: &mut World| {
        action
            .item
            .is_none_or(|i| crate::tools::consume(world, i, action.amount))
    };
    match job {
        CoreJob::Melt => {
            if f.state != CoreState::Empty || !consume(world) {
                return;
            }
            // tg `atom_deconstruct`: 4 plasteel (an empty frame holds nothing else).
            if let Some(&at) = world.get::<TilePos>(core) {
                crate::construction::spawn_stack(world, PLASTEEL, at, 4);
            }
            despawn_core(world, core);
        }
        CoreJob::Unfasten { anchored: was } => {
            if f.state == CoreState::Finished || anchored(world, core) != was {
                return;
            }
            world.entity_mut(core).insert(Anchored(!was));
            sound(world, core, "sound/items/tools/ratchet.ogg");
        }
        CoreJob::Cable => {
            if f.state == CoreState::Screwed && consume(world) {
                set_state(world, core, CoreState::Cabled);
            }
        }
        CoreJob::Glass => {
            if f.state == CoreState::Cabled && f.mmi.is_some() && consume(world) {
                set_state(world, core, CoreState::Glassed);
            }
        }
        CoreJob::Connect => {
            if f.state != CoreState::Finished {
                return;
            }
            if structure_to_mob(world, core).is_some() {
                tell(
                    world,
                    action.client,
                    Span::Notice,
                    "connected neural network",
                );
            } else {
                tell(
                    world,
                    action.client,
                    Span::Warning,
                    "processor is inactive!",
                );
            }
        }
    }
}

/// tg `ai_structure_to_mob`: the processor's mind becomes an AI on the core's tile; the core and
/// the processor are gone. `None` without a present mind or the AI's prototype.
pub fn structure_to_mob(world: &mut World, core: Entity) -> Option<Entity> {
    let f = world.get::<CoreFrame>(core).cloned()?;
    let (mob, client) = active_mind(world, &f)?;
    let pos = *world.get::<TilePos>(core)?;
    let name = world
        .get::<Name>(mob)
        .map_or_else(|| "ИИ".to_owned(), |n| n.as_str().to_owned());
    let mmi = f.mmi?;
    // The brainmob loses its radio first; the new AI's `attach_client` despawns it.
    super::mmi::set_brainmob(world, mmi, None);
    let ai = super::ai::spawn_ai(world, client, pos, &name, None)?;
    despawn_core(world, core);
    info!("{name} ({client}) brought online as AI {ai} at {pos:?}");
    Some(ai)
}

/// Whether an AI mob may be uploaded into `core`: finished and without an active mind (tg
/// `/obj/structure/ai_core/transfer_ai`). The refusal, else.
pub fn upload_refusal(world: &mut World, core: Entity) -> Option<&'static str> {
    let Some(f) = frame(world, core) else {
        return Some("");
    };
    if f.state != CoreState::Finished {
        return Some("");
    }
    if has_mind(world, core) {
        return Some("already contains an active mind!");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StoredIn;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world
    }

    fn spawn_frame(world: &mut World, state: CoreState) -> Entity {
        world
            .spawn((
                DmItem {
                    path: AI_CORE.into(),
                    ..default()
                },
                TilePos::new(2, 2, 1),
                CoreFrame {
                    state,
                    board: None,
                    mmi: None,
                    linked_ai: None,
                },
                Anchored(true),
            ))
            .id()
    }

    #[test]
    fn a_frame_is_made_from_its_type() {
        let mut world = world();
        let plain = world
            .spawn(DmItem {
                path: AI_CORE.into(),
                ..default()
            })
            .id();
        let latejoin = world
            .spawn(DmItem {
                path: LATEJOIN_CORE.into(),
                ..default()
            })
            .id();
        assert_eq!(frame(&mut world, plain).unwrap().state, CoreState::Empty);
        assert_eq!(world.get::<Anchored>(plain), Some(&Anchored(false)));
        assert_eq!(
            frame(&mut world, latejoin).unwrap().state,
            CoreState::Finished
        );
        assert_eq!(world.get::<Anchored>(latejoin), Some(&Anchored(true)));
    }

    #[test]
    fn an_empty_finished_core_takes_an_upload_and_a_glassed_one_does_not() {
        let mut world = world();
        let finished = spawn_frame(&mut world, CoreState::Finished);
        let glassed = spawn_frame(&mut world, CoreState::Glassed);
        assert_eq!(upload_refusal(&mut world, finished), None);
        assert!(upload_refusal(&mut world, glassed).is_some());
    }

    #[test]
    fn a_linked_core_returns_its_ai_to_its_tile() {
        let mut world = world();
        let frame = spawn_frame(&mut world, CoreState::Finished);
        let apc = world.spawn(TilePos::new(5, 5, 1)).id();
        let ai = world.spawn(StoredIn(apc)).id();
        link(&mut world, ai, frame);
        assert_eq!(world.get::<CoreFrame>(frame).unwrap().linked_ai, Some(ai));
        assert!(resolve_link(&mut world, ai));
        assert_eq!(world.get::<TilePos>(ai), Some(&TilePos::new(2, 2, 1)));
        assert!(world.get_entity(frame).is_err());
        assert!(world.get::<LinkedCore>(ai).is_none());
        assert!(!resolve_link(&mut world, ai));
    }
}
