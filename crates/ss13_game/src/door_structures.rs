//! Door structures (server): building firelocks and windoors, and the mineral doors.
//!
//! - Firelock frames (tg `/obj/structure/firelock_frame`, `firedoor.dm`): 3 iron sheets
//!   right-clicked on a floor with firelock circuitry in the other hand build a frame (5 s, the
//!   stack recipe's cost; the border frame takes 2). Without a circuit (`frame1`): the circuitry
//!   goes in (2 s), a lit welder (2 s) cuts it into 3 iron (2 for the border frame; plus 2
//!   plasteel if reinforced). With the circuit (`frame2`): 2 plasteel reinforce it (2 s; not a
//!   border frame), a crowbar (2 s) takes the circuit out, a wrench (2 s, no firelock on the
//!   tile) finishes it: a heavy firelock if reinforced, a border one facing the frame's way, else
//!   a plain one. A firelock comes apart again: welded shut (right click, `crate::doors`), its
//!   floor bolts unlocked with a screwdriver, a wrench (2 s) leaves its frame with the circuit in.
//! - Windoor assemblies (tg `/obj/structure/windoor_assembly`): 5 reinforced glass sheets
//!   right-clicked on a floor with airlock electronics in the other hand make one facing the
//!   builder's way (the stack recipe, no delay). Before it is wired: 2 plasteel (4 s) make it
//!   secure, a wrench (4 s) anchors or loosens it (not over a windoor facing the same way), a
//!   right-click wrench on a loose one flips the side it slides to, a lit welder (4 s) on a loose
//!   one takes it apart into 5 reinforced glass (and 4 rods if secure). Anchored, a cable (4 s,
//!   1 piece) wires it; wired, wirecutters (4 s) take the wire out, airlock electronics (4 s) go
//!   in, a screwdriver (4 s) takes them out; with electronics a crowbar (4 s) finishes it: a
//!   windoor (a brig windoor if secure) sliding its way, with the electronics' access, name and
//!   unrestricted sides (only the two along the door), autoclosing.
//! - Mineral doors (tg `/obj/structure/mineral_door`, `mineral_doors.dm`): a hand, a held item
//!   out of combat mode or a bump opens or closes an anchored one (not a handcuffed mob): 1 s
//!   of `<state>opening`/`<state>closing`, the door stops (or starts) blocking sight at once
//!   and movement and air after the second; a mob in the doorway keeps it open. A pickaxe (4 s)
//!   digs it into its sheets; a wrench (4 s) loosens or fastens it; a lit welder (6 s, 5 fuel)
//!   on a loose one welds it apart; wood and paper doors take a crowbar (6 s) instead, cannot
//!   be dug or welded, and burn (`crate::fire`). Transparent doors never block sight.
// shortcut: no stack recipe menus (the other-hand part stands in for the recipe choice), no RCD
// steps, no electroadaptive pseudocircuit, no renaming windoor assemblies with a pen, no examine
// lines; the frames' and assemblies' map var edits (`constructionStep`, `secure`, `facing`) are
// not read (they start fresh, their kind taken from the type); destroyed frames and assemblies
// leave nothing; a destroyed firelock leaves no frame (`on_deconstruction(FALSE)`); mineral doors
// ignore `close_delay` (no tg door sets it), the paper door's patching with paper and the
// uranium door's glow; mobs without a player open them too.

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmDoor, DmItem, DmName, DmObj, DmTool, DoorKind, Opaque};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, Layer, TilePos};
use ss13_world::{Border, Bumped, Density, Mover, SpatialIndex, WorldSystems};

use crate::airlock_construction::{AirlockElectronics, ELECTRONICS};
use crate::construction::{Job, spawn_obj, spawn_stack, tile_changed};
use crate::doors::Door;
use crate::inventory::Inventory;
use crate::obj_damage::{Anchored, set_anchored};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, ToolKind, do_after, tell, tool_of, use_tool};

pub const FIRELOCK_FRAME: &str = "/obj/structure/firelock_frame";
pub const FIRELOCK_FRAME_HEAVY: &str = "/obj/structure/firelock_frame/heavy";
pub const FIRELOCK_FRAME_BORDER: &str = "/obj/structure/firelock_frame/border_only";
pub const FIRELOCK_ELECTRONICS: &str = "/obj/item/electronics/firelock";
pub const FIREDOOR: &str = "/obj/machinery/door/firedoor";
pub const FIREDOOR_HEAVY: &str = "/obj/machinery/door/firedoor/heavy";
pub const FIREDOOR_BORDER: &str = "/obj/machinery/door/firedoor/border_only";
pub const WINDOOR_ASSEMBLY: &str = "/obj/structure/windoor_assembly";
pub const WINDOOR: &str = "/obj/machinery/door/window";
pub const BRIGDOOR: &str = "/obj/machinery/door/window/brigdoor";
pub const MINERAL_DOOR: &str = "/obj/structure/mineral_door";
const IRON: &str = "/obj/item/stack/sheet/iron";
const PLASTEEL: &str = "/obj/item/stack/sheet/plasteel";
const RGLASS: &str = "/obj/item/stack/sheet/rglass";
const RODS: &str = "/obj/item/stack/rods";
const CABLE: &str = "/obj/item/stack/cable_coil";
const DECONSTRUCT: &str = ss13_mapload::sound::files::DECONSTRUCT;
/// `CONSTRUCTION_NO_CIRCUIT`, `CONSTRUCTION_PANEL_OPEN`.
pub const NO_CIRCUIT: u8 = 1;
pub const PANEL_OPEN: u8 = 2;
/// Firelock frames: `DEFAULT_STEP_TIME`; the recipes' `5 SECONDS`.
const FRAME_STEP_DS: f32 = 20.0;
const FRAME_BUILD_DS: f32 = 50.0;
/// Windoor assembly steps: `4 SECONDS`.
const WINDOOR_STEP_DS: f32 = 40.0;
/// The `windoor frame` recipe: 5 reinforced glass.
pub const WINDOOR_SHEETS: u32 = 5;
/// Mineral doors: `sleep(1 SECONDS)` of `Open`/`Close`.
pub const MINERAL_SWITCH_SECS: f64 = 1.0;
const MINERAL_DIG_DS: f32 = 40.0;
const MINERAL_WRENCH_DS: f32 = 40.0;
const MINERAL_APART_DS: f32 = 60.0;
const STONE_SOUND: &str = "sound/effects/stonedoor_openclose.ogg";
pub const CREAKY_SOUND: &str = "sound/effects/doorcreaky.ogg";
const WOOD_DOOR: &str = "/obj/structure/mineral_door/wood";
const PAPER_DOOR: &str = "/obj/structure/mineral_door/paperframe";
const TRANSPARENT_DOOR: &str = "/obj/structure/mineral_door/transparent";
/// `CLOSED_DOOR_LAYER`, `OPEN_DOOR_LAYER`.
const CLOSED_LAYER: f32 = 3.1;
const OPEN_LAYER: f32 = 2.7;

/// A firelock frame's live state (tg `constructionStep`, `reinforced`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirelockFrame {
    pub step: u8,
    pub reinforced: bool,
}

/// On a firelock whose floor bolts a screwdriver unlocked (tg `boltslocked = FALSE`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FiredoorBoltsUnlocked;

/// A windoor assembly's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct WindoorAssembly {
    pub secure: bool,
    /// `facing == WINDOOR_RIGHT`.
    pub right: bool,
    pub cables_added: bool,
    pub electronics: Option<AirlockElectronics>,
}

/// tg `isSwitchingStates`: the mineral door finishes opening (`opening`) or closing at `at`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct MineralDoorMoving {
    pub at: f64,
    pub opening: bool,
}

/// The timed steps, with what they started from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorBuildJob {
    /// Iron sheets on a floor (`border`: the directional frame).
    FrameBuild {
        border: bool,
        dir: Dir,
    },
    FrameReinforce,
    FrameCircuit,
    FrameUncircuit,
    FrameFinish,
    FrameCut,
    /// A wrench on a welded firelock with unlocked bolts.
    FiredoorUnbolt,
    WindoorReinforce,
    WindoorAnchor {
        anchored: bool,
    },
    WindoorWire,
    WindoorUnwire,
    WindoorElectronics,
    WindoorUnelectronics,
    WindoorFinish,
    WindoorApart,
    MineralDig,
    MineralWrench {
        anchored: bool,
    },
    MineralApart,
}

pub struct DoorStructuresPlugin;

impl Plugin for DoorStructuresPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Frame and assembly steps, firelock bolts, tools and items on mineral doors.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2885, |w, c| {
            c.held.is_some_and(|h| {
                item_interaction(
                    w,
                    Actor {
                        client: c.client,
                        mob: c.mob,
                        item: Some(h),
                    },
                    c.target,
                    c.modifiers.right,
                )
            })
        });
        // A mineral door's `attack_hand`.
        app.add_click_handler(ClickPhase::EmptyHand, 2810, |w, c| {
            mineral_attack_hand(w, c.mob, c.target)
        });
        app.add_systems(
            Update,
            (
                mineral_bumps.after(WorldSystems::Movement),
                finish_mineral_doors.run_if(any_with_component::<MineralDoorMoving>),
            ),
        );
    }
}

fn path(world: &World, e: Entity) -> Option<&str> {
    world
        .get::<DmObj>(e)
        .map(|o| o.path.as_str())
        .or_else(|| world.get::<DmItem>(e).map(|i| i.path.as_str()))
}

fn is(world: &World, e: Entity, root: &str) -> bool {
    path(world, e).is_some_and(|p| is_under(p, root))
}

fn anchored(world: &World, e: Entity) -> bool {
    world.get::<Anchored>(e).is_some_and(|a| a.0)
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn sound(world: &mut World, e: Entity, file: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(e) {
        playsound(world, PlaySound::new(pos, file, vol, true));
    }
}

fn icon_state(world: &mut World, e: Entity, state: &str) {
    let Some(icon) = world.get::<DmAtom>(e).map(|a| a.icon.clone()) else {
        return;
    };
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&icon, state));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(e))
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
}

/// `setDir`: the facing, the prototype dir and a border object's blocked edge.
fn face(world: &mut World, e: Entity, dir: Dir) {
    world.entity_mut(e).insert(Facing(dir));
    if let Some(mut atom) = world.get_mut::<DmAtom>(e) {
        atom.dir = dir as u8;
    }
    let border = world
        .get::<ss13_mapload::DmBorder>(e)
        .and_then(|b| b.border(dir as u8))
        .or_else(|| world.get::<Border>(e).map(|_| Border::facing(dir, false)));
    if let Some(border) = border {
        world.entity_mut(e).insert(border);
    }
}

fn facing(world: &World, e: Entity) -> Dir {
    world.get::<Facing>(e).map_or(Dir::South, |f| f.0)
}

fn other_hand_has(world: &World, mob: Entity, held: Entity, root: &str) -> bool {
    world.get::<Inventory>(mob).is_some_and(|inv| {
        inv.hands
            .iter()
            .flatten()
            .any(|&e| e != held && is(world, e, root))
    })
}

// --- Firelock frames ---------------------------------------------------------------------

fn frame(world: &mut World, e: Entity) -> Option<FirelockFrame> {
    if let Some(&f) = world.get::<FirelockFrame>(e) {
        return Some(f);
    }
    if !is(world, e, FIRELOCK_FRAME) {
        return None;
    }
    let f = FirelockFrame {
        step: NO_CIRCUIT,
        reinforced: is(world, e, FIRELOCK_FRAME_HEAVY),
    };
    world.entity_mut(e).insert(f);
    Some(f)
}

fn set_frame(world: &mut World, e: Entity, f: FirelockFrame) {
    world.entity_mut(e).insert(f);
    icon_state(world, e, &format!("frame{}", f.step));
}

fn firelock_on(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world
                .get::<Door>(e)
                .is_some_and(|d| d.kind == DoorKind::Firedoor)
        })
}

/// Iron sheets right-clicked on a floor with firelock circuitry in the other hand (from
/// `crate::construction`): true when the circuitry is there and the build was tried.
pub fn try_build_frame(world: &mut World, actor: Actor, turf: Entity, pos: TilePos) -> bool {
    let Some(held) = actor.item else {
        return false;
    };
    if !other_hand_has(world, actor.mob, held, FIRELOCK_ELECTRONICS) {
        return false;
    }
    // shortcut: the directional frame is not offered (no recipe menu).
    let cost = 3;
    if crate::tools::stack_amount(world, held) < cost {
        tell(world, actor.client, Span::Warning, "Нужно 3 листа металла!");
        return true;
    }
    let w: &World = world;
    let frame_here = w
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| is(w, e, FIRELOCK_FRAME));
    if firelock_on(world, pos) || frame_here {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Здесь уже что-то стоит!",
        );
        return true;
    }
    let dir = facing(world, actor.mob);
    do_after(
        world,
        actor,
        turf,
        FRAME_BUILD_DS,
        cost,
        Job::DoorBuild(DoorBuildJob::FrameBuild { border: false, dir }),
    );
    true
}

fn frame_interaction(world: &mut World, actor: Actor, held: Entity, target: Entity) -> bool {
    let Some(f) = frame(world, target) else {
        return false;
    };
    let client = actor.client;
    let directional = is(world, target, FIRELOCK_FRAME_BORDER);
    let job = |j| Job::DoorBuild(j);
    if let Some((tool, _)) = tool_of(world, held) {
        let (started, text) = match tool {
            ToolKind::Crowbar if f.step == PANEL_OPEN => (
                use_tool(
                    world,
                    actor,
                    target,
                    FRAME_STEP_DS,
                    0,
                    job(DoorBuildJob::FrameUncircuit),
                ),
                "Вы начинаете извлекать плату из рамы...",
            ),
            ToolKind::Wrench if f.step == PANEL_OPEN => {
                let pos = world.get::<TilePos>(target).copied();
                if pos.is_some_and(|p| firelock_on(world, p)) {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "Здесь уже есть пожарный шлюз.",
                    );
                    return true;
                }
                (
                    use_tool(
                        world,
                        actor,
                        target,
                        FRAME_STEP_DS,
                        0,
                        job(DoorBuildJob::FrameFinish),
                    ),
                    "Вы начинаете прикручивать раму...",
                )
            }
            ToolKind::Welder if f.step == NO_CIRCUIT => {
                if !crate::tools::tool_usable(world, held, 1, 0.0) {
                    let text = crate::tools::unusable_message(world, held, 1);
                    tell(world, client, Span::Warning, text);
                    return true;
                }
                (
                    use_tool(
                        world,
                        actor,
                        target,
                        FRAME_STEP_DS,
                        1,
                        job(DoorBuildJob::FrameCut),
                    ),
                    "Вы начинаете разрезать раму...",
                )
            }
            _ => return false,
        };
        if started {
            tell(world, client, Span::Notice, text);
        }
        return true;
    }
    if f.step == PANEL_OPEN && is(world, held, PLASTEEL) {
        if directional {
            tell(world, client, Span::Warning, "Эту раму нельзя укрепить.");
        } else if f.reinforced {
            tell(world, client, Span::Warning, "Рама уже укреплена.");
        } else if crate::tools::stack_amount(world, held) < 2 {
            tell(world, client, Span::Warning, "Нужно больше пласстали.");
        } else if do_after(
            world,
            actor,
            target,
            FRAME_STEP_DS,
            2,
            job(DoorBuildJob::FrameReinforce),
        ) {
            sound(world, target, DECONSTRUCT, 50.0);
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете укреплять раму...",
            );
        }
        return true;
    }
    if f.step == NO_CIRCUIT && is(world, held, FIRELOCK_ELECTRONICS) {
        if do_after(
            world,
            actor,
            target,
            FRAME_STEP_DS,
            0,
            job(DoorBuildJob::FrameCircuit),
        ) {
            sound(world, target, DECONSTRUCT, 50.0);
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете вставлять плату в раму...",
            );
        }
        return true;
    }
    false
}

/// tg firedoor `screwdriver_act` (the bolts' screws) and `wrench_act` (taking it apart) on a
/// welded firelock.
fn firedoor_interaction(world: &mut World, actor: Actor, held: Entity, target: Entity) -> bool {
    let Some(door) = world.get::<Door>(target) else {
        return false;
    };
    if door.kind != DoorKind::Firedoor || door.is_operating() || !door.welded {
        return false;
    }
    let locked = world.get::<FiredoorBoltsUnlocked>(target).is_none();
    match tool_of(world, held).map(|t| t.0) {
        Some(ToolKind::Screwdriver) => {
            if locked {
                world.entity_mut(target).insert(FiredoorBoltsUnlocked);
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    "Вы откручиваете винты болтов пожарного шлюза.",
                );
            } else {
                world.entity_mut(target).remove::<FiredoorBoltsUnlocked>();
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    "Вы закручиваете винты болтов пожарного шлюза.",
                );
            }
            true
        }
        Some(ToolKind::Wrench) => {
            if locked {
                tell(world, actor.client, Span::Notice, "Болты держат винты!");
            } else if use_tool(
                world,
                actor,
                target,
                FRAME_STEP_DS,
                0,
                Job::DoorBuild(DoorBuildJob::FiredoorUnbolt),
            ) {
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    "Вы начинаете откручивать болты пожарного шлюза...",
                );
            }
            true
        }
        _ => false,
    }
}

/// tg firedoor `on_deconstruction(TRUE)`: its frame (`assemblytype`) with the circuit in, facing
/// its way; the firelock gone.
pub fn deconstruct_firedoor(world: &mut World, door: Entity) -> Option<Entity> {
    let pos = *world.get::<TilePos>(door)?;
    let frame_path = if is(world, door, FIREDOOR_BORDER) {
        FIRELOCK_FRAME_BORDER
    } else if is(world, door, FIREDOOR_HEAVY) {
        FIRELOCK_FRAME_HEAVY
    } else {
        FIRELOCK_FRAME
    };
    let dir = facing(world, door);
    crate::wallmount::firelock::teardown(world, door);
    world.despawn(door);
    tile_changed(world, pos);
    let frame = spawn_obj(world, frame_path, pos)?;
    face(world, frame, dir);
    let reinforced = frame_path == FIRELOCK_FRAME_HEAVY;
    set_frame(
        world,
        frame,
        FirelockFrame {
            step: PANEL_OPEN,
            reinforced,
        },
    );
    Some(frame)
}

/// The frame's `wrench_act`: the firelock in its place.
pub fn finish_firelock(world: &mut World, frame: Entity, f: FirelockFrame) -> Option<Entity> {
    let pos = *world.get::<TilePos>(frame)?;
    let directional = is(world, frame, FIRELOCK_FRAME_BORDER);
    let dir = facing(world, frame);
    let path = if f.reinforced {
        FIREDOOR_HEAVY
    } else if directional {
        FIREDOOR_BORDER
    } else {
        FIREDOOR
    };
    world.despawn(frame);
    let Some(door) = spawn_obj(world, path, pos) else {
        warn!("firelock frame at {pos:?}: no prototype for {path}");
        return None;
    };
    if directional {
        face(world, door, dir);
    }
    if !crate::doors::setup_built(world, door) {
        return Some(door);
    }
    crate::power::register_machine(world, door);
    crate::wallmount::firelock::setup_built(world, door);
    tile_changed(world, pos);
    Some(door)
}

// --- Windoor assemblies --------------------------------------------------------------------

fn windoor_assembly(world: &mut World, e: Entity) -> Option<WindoorAssembly> {
    if let Some(a) = world.get::<WindoorAssembly>(e) {
        return Some(a.clone());
    }
    if !is(world, e, WINDOOR_ASSEMBLY) {
        return None;
    }
    world.entity_mut(e).insert(WindoorAssembly::default());
    Some(WindoorAssembly::default())
}

/// `update_icon_state`: `<l|r>_[secure_]windoor_assembly<01|02>`.
fn set_windoor_assembly(world: &mut World, e: Entity, a: WindoorAssembly) {
    let state = format!(
        "{}_{}windoor_assembly{}",
        if a.right { "r" } else { "l" },
        if a.secure { "secure_" } else { "" },
        if a.cables_added { "02" } else { "01" }
    );
    world.entity_mut(e).insert(a);
    icon_state(world, e, &state);
}

/// A windoor on `pos` facing `dir` (the wrench's `competitor` check).
fn windoor_facing(world: &World, pos: TilePos, dir: Dir, except: Entity) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            e != except
                && world
                    .get::<Door>(e)
                    .is_some_and(|d| d.kind == DoorKind::Windoor)
                && world.get::<Facing>(e).map_or(Dir::South, |f| f.0) == dir
        })
}

/// Reinforced glass right-clicked on a floor with airlock electronics in the other hand (from
/// `crate::construction`): the `windoor frame` recipe. True when it was tried.
pub fn try_build_windoor_assembly(world: &mut World, actor: Actor, pos: TilePos) -> bool {
    let Some(held) = actor.item else {
        return false;
    };
    if !other_hand_has(world, actor.mob, held, ELECTRONICS) {
        return false;
    }
    let dir = facing(world, actor.mob);
    // `CRAFT_CHECK_DIRECTION`: nothing on the tile facing that way.
    let taken = world
        .resource::<SpatialIndex>()
        .border_on_tile(pos)
        .iter()
        .any(|&(_, edges)| edges & dir as u8 != 0);
    if taken {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Здесь уже что-то стоит!",
        );
        return true;
    }
    if !crate::tools::use_stack(world, held, WINDOOR_SHEETS) {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Нужно 5 листов укреплённого стекла!",
        );
        return true;
    }
    if let Some(e) = spawn_obj(world, WINDOOR_ASSEMBLY, pos) {
        face(world, e, dir);
        set_windoor_assembly(world, e, WindoorAssembly::default());
        tell(
            world,
            actor.client,
            Span::Notice,
            "Вы собираете каркас окно-двери.",
        );
    }
    true
}

fn windoor_interaction(
    world: &mut World,
    actor: Actor,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if right && !matches!(tool_of(world, held), Some((ToolKind::Wrench, _))) {
        return false;
    }
    let Some(a) = windoor_assembly(world, target) else {
        return false;
    };
    let client = actor.client;
    let is_anchored = anchored(world, target);
    let job = |j| Job::DoorBuild(j);
    if let Some((tool, _)) = tool_of(world, held) {
        let (started, text) = match tool {
            // `wrench_act_secondary`: flip the sliding side of a loose assembly.
            ToolKind::Wrench if right => {
                if is_anchored {
                    return false;
                }
                let mut a = a;
                a.right = !a.right;
                let text = if a.right {
                    "Окно-дверь теперь будет открываться вправо."
                } else {
                    "Окно-дверь теперь будет открываться влево."
                };
                set_windoor_assembly(world, target, a);
                tell(world, client, Span::Notice, text);
                return true;
            }
            ToolKind::Wrench if !a.cables_added => {
                let dir = facing(world, target);
                let pos = world.get::<TilePos>(target).copied();
                if !is_anchored && pos.is_some_and(|p| windoor_facing(world, p, dir, target)) {
                    tell(world, client, Span::Warning, "Здесь уже есть окно-дверь!");
                    return true;
                }
                (
                    use_tool(
                        world,
                        actor,
                        target,
                        WINDOOR_STEP_DS,
                        0,
                        job(DoorBuildJob::WindoorAnchor {
                            anchored: is_anchored,
                        }),
                    ),
                    if is_anchored {
                        "Вы начинаете откреплять каркас окно-двери от пола..."
                    } else {
                        "Вы начинаете крепить каркас окно-двери к полу..."
                    },
                )
            }
            // tg checks `anchored` here, yet its examine says the loose one is cut apart
            // (and nothing else takes a loose one apart): the loose one is welded.
            ToolKind::Welder if !a.cables_added && !is_anchored => {
                if !crate::tools::tool_usable(world, held, 1, 0.0) {
                    let text = crate::tools::unusable_message(world, held, 1);
                    tell(world, client, Span::Warning, text);
                    return true;
                }
                (
                    use_tool(
                        world,
                        actor,
                        target,
                        WINDOOR_STEP_DS,
                        1,
                        job(DoorBuildJob::WindoorApart),
                    ),
                    "Вы начинаете разбирать каркас окно-двери...",
                )
            }
            ToolKind::Wirecutters if a.cables_added => (
                use_tool(
                    world,
                    actor,
                    target,
                    WINDOOR_STEP_DS,
                    0,
                    job(DoorBuildJob::WindoorUnwire),
                ),
                "Вы начинаете вырезать провода из каркаса окно-двери...",
            ),
            ToolKind::Screwdriver if a.cables_added && a.electronics.is_some() => (
                use_tool(
                    world,
                    actor,
                    target,
                    WINDOOR_STEP_DS,
                    0,
                    job(DoorBuildJob::WindoorUnelectronics),
                ),
                "Вы начинаете извлекать электронику из каркаса окно-двери...",
            ),
            ToolKind::Crowbar if a.cables_added => {
                if a.electronics.is_none() {
                    tell(world, client, Span::Warning, "В каркасе нет электроники!");
                    return true;
                }
                (
                    use_tool(
                        world,
                        actor,
                        target,
                        WINDOOR_STEP_DS,
                        0,
                        job(DoorBuildJob::WindoorFinish),
                    ),
                    "Вы начинаете вставлять окно-дверь в раму...",
                )
            }
            _ => return false,
        };
        if started {
            tell(world, client, Span::Notice, text);
        }
        return true;
    }
    if !a.cables_added {
        if !a.secure && is(world, held, PLASTEEL) {
            if crate::tools::stack_amount(world, held) < 2 {
                tell(world, client, Span::Warning, "Нужно больше пласстали!");
            } else if do_after(
                world,
                actor,
                target,
                WINDOOR_STEP_DS,
                2,
                job(DoorBuildJob::WindoorReinforce),
            ) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете укреплять окно-дверь пласталью...",
                );
            }
            return true;
        }
        if is_anchored && is(world, held, CABLE) {
            if do_after(
                world,
                actor,
                target,
                WINDOOR_STEP_DS,
                1,
                job(DoorBuildJob::WindoorWire),
            ) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете прокладывать провода в каркасе окно-двери...",
                );
            }
            return true;
        }
        return false;
    }
    if is(world, held, ELECTRONICS) {
        if do_after(
            world,
            actor,
            target,
            WINDOOR_STEP_DS,
            0,
            job(DoorBuildJob::WindoorElectronics),
        ) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете устанавливать электронику в каркас окно-двери...",
            );
        }
        return true;
    }
    false
}

/// `finish_door`: the windoor (a brig windoor if secure) in the assembly's place.
pub fn finish_windoor(world: &mut World, e: Entity, a: &WindoorAssembly) -> Option<Entity> {
    let pos = *world.get::<TilePos>(e)?;
    let electronics = a.electronics.clone()?;
    let dir = facing(world, e);
    let path = if a.secure { BRIGDOOR } else { WINDOOR };
    world.despawn(e);
    let Some(door) = spawn_obj(world, path, pos) else {
        warn!("windoor assembly at {pos:?}: no prototype for {path}");
        return None;
    };
    face(world, door, dir);
    let base = match (a.right, a.secure) {
        (false, false) => "left",
        (true, false) => "right",
        (false, true) => "leftsecure",
        (true, true) => "rightsecure",
    };
    if let Some(mut dm) = world.get_mut::<DmDoor>(door) {
        dm.base_icon_state = base.to_owned();
    }
    if !crate::doors::setup_built(world, door) {
        return Some(door);
    }
    if !electronics.passed_name.is_empty()
        && let Some(mut n) = world.get_mut::<DmName>(door)
    {
        n.name = electronics.passed_name.clone();
    }
    if let Some(mut d) = world.get_mut::<Door>(door) {
        crate::airlock_construction::apply_electronics(&mut d, &electronics);
        // Only the sides along the door's way stay unrestricted.
        let along = if matches!(dir, Dir::North | Dir::South) {
            Dir::North as u8 | Dir::South as u8
        } else {
            Dir::East as u8 | Dir::West as u8
        };
        d.unres_sides &= along;
        d.unres_latch = d.unres_sides != 0;
    }
    world.entity_mut(door).insert(electronics);
    crate::power::register_machine(world, door);
    tile_changed(world, pos);
    Some(door)
}

// --- Mineral doors -------------------------------------------------------------------------

/// The door's `sheetType` and `sheetAmount`.
fn mineral_sheets(path: &str) -> (&'static str, u32) {
    const KINDS: [(&str, &str, u32); 9] = [
        ("/obj/structure/mineral_door/iron", IRON, 20),
        (
            "/obj/structure/mineral_door/silver",
            "/obj/item/stack/sheet/mineral/silver",
            10,
        ),
        (
            "/obj/structure/mineral_door/gold",
            "/obj/item/stack/sheet/mineral/gold",
            10,
        ),
        (
            "/obj/structure/mineral_door/uranium",
            "/obj/item/stack/sheet/mineral/uranium",
            10,
        ),
        (
            "/obj/structure/mineral_door/sandstone",
            "/obj/item/stack/sheet/mineral/sandstone",
            10,
        ),
        (
            "/obj/structure/mineral_door/transparent/plasma",
            "/obj/item/stack/sheet/mineral/plasma",
            10,
        ),
        (
            "/obj/structure/mineral_door/transparent/diamond",
            "/obj/item/stack/sheet/mineral/diamond",
            10,
        ),
        (WOOD_DOOR, "/obj/item/stack/sheet/mineral/wood", 10),
        (PAPER_DOOR, "/obj/item/stack/sheet/paperframes", 3),
    ];
    KINDS
        .iter()
        .find(|(root, ..)| is_under(path, root))
        .map_or((IRON, 10), |&(_, sheet, n)| (sheet, n))
}

/// Sheets and amounts a mineral door leaves (`atom_deconstruct`), for the runtime registry.
pub const MINERAL_SHEETS: [&str; 9] = [
    IRON,
    "/obj/item/stack/sheet/mineral/silver",
    "/obj/item/stack/sheet/mineral/gold",
    "/obj/item/stack/sheet/mineral/uranium",
    "/obj/item/stack/sheet/mineral/sandstone",
    "/obj/item/stack/sheet/mineral/plasma",
    "/obj/item/stack/sheet/mineral/diamond",
    "/obj/item/stack/sheet/mineral/wood",
    "/obj/item/stack/sheet/paperframes",
];

fn is_mineral_door(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, MINERAL_DOOR))
}

/// Wood and paper doors: crowbar instead of pickaxe and welder.
fn flammable_door(path: &str) -> bool {
    is_under(path, WOOD_DOOR) || is_under(path, PAPER_DOOR)
}

/// tg `TryToSwitchState`: an anchored door that is not moving, by a mob that is not
/// handcuffed. True when `door` is a mineral door.
pub fn try_switch(world: &mut World, door: Entity, user: Entity) -> bool {
    if !is_mineral_door(world, door) {
        return false;
    }
    if world.get::<MineralDoorMoving>(door).is_some()
        || !anchored(world, door)
        || world.get::<crate::restraints::Handcuffed>(user).is_some()
    {
        return true;
    }
    switch_state(world, door);
    true
}

/// tg `SwitchState`: `Open` or `Close` starts (a living mob in an open doorway stops `Close`).
pub fn switch_state(world: &mut World, door: Entity) {
    let Some(&pos) = world.get::<TilePos>(door) else {
        return;
    };
    let Some(door_path) = path(world, door).map(str::to_owned) else {
        return;
    };
    let opened = world.get::<Density>(door).is_none();
    if opened {
        let occupied = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .any(|&e| world.get::<crate::health::Health>(e).is_some());
        if occupied {
            return;
        }
    }
    let file = if flammable_door(&door_path) {
        CREAKY_SOUND
    } else {
        STONE_SOUND
    };
    playsound(world, PlaySound::new(pos, file, 100.0, true));
    let base = world
        .get::<DmAtom>(door)
        .map(|a| a.icon_state.clone())
        .unwrap_or_default();
    if !opened {
        // `set_opacity(FALSE)` before the second.
        world.entity_mut(door).remove::<Opaque>();
        tile_changed(world, pos);
    }
    icon_state(
        world,
        door,
        &format!("{base}{}", if opened { "closing" } else { "opening" }),
    );
    let at = now(world) + MINERAL_SWITCH_SECS;
    world.entity_mut(door).insert(MineralDoorMoving {
        at,
        opening: !opened,
    });
}

/// The end of `Open`/`Close`: density, opacity (not of transparent doors), layer and look.
pub fn finish_switch(world: &mut World, door: Entity) {
    let Some(MineralDoorMoving { opening, .. }) = world.get::<MineralDoorMoving>(door).copied()
    else {
        return;
    };
    world.entity_mut(door).remove::<MineralDoorMoving>();
    let Some(&pos) = world.get::<TilePos>(door) else {
        return;
    };
    let transparent = is(world, door, TRANSPARENT_DOOR);
    let base = world
        .get::<DmAtom>(door)
        .map(|a| a.icon_state.clone())
        .unwrap_or_default();
    if opening {
        world.entity_mut(door).remove::<Density>();
    } else {
        world.entity_mut(door).insert(Density);
        if !transparent && anchored(world, door) {
            world.entity_mut(door).insert(Opaque);
        }
    }
    if let Some(mut appearance) = world.get_mut::<Appearance>(door) {
        appearance.layer = Layer(if opening { OPEN_LAYER } else { CLOSED_LAYER });
    }
    icon_state(
        world,
        door,
        &format!("{base}{}", if opening { "open" } else { "" }),
    );
    tile_changed(world, pos);
}

fn finish_mineral_doors(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &MineralDoorMoving)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| t >= m.at)
        .map(|(e, _)| e)
        .collect();
    for door in due {
        finish_switch(world, door);
    }
}

/// tg `Bumped`: a closed door bumped by a mob (a mech by its pilot) tries to switch.
fn mineral_bumps(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<Bumped>>) {
    let bumps: Vec<(Entity, Entity)> = cursor
        .read(world.resource::<Messages<Bumped>>())
        .map(|b| (b.entity, b.obstacle))
        .collect();
    for (mover, door) in bumps {
        if world.get::<Mover>(mover).is_none()
            || world.get::<Density>(door).is_none()
            || !is_mineral_door(world, door)
        {
            continue;
        }
        try_switch(world, door, mover);
    }
}

fn mineral_attack_hand(world: &mut World, mob: Entity, target: Entity) -> bool {
    try_switch(world, target, mob)
}

fn mineral_interaction(world: &mut World, actor: Actor, held: Entity, target: Entity) -> bool {
    let Some(door_path) = world
        .get::<DmObj>(target)
        .filter(|o| is_under(&o.path, MINERAL_DOOR))
        .map(|o| o.path.clone())
    else {
        return false;
    };
    let client = actor.client;
    let flammable = flammable_door(&door_path);
    let is_anchored = anchored(world, target);
    let behaviour = world.get::<DmTool>(held).map(|t| t.behaviour.clone());
    let job = |j| Job::DoorBuild(j);
    // `pickaxe_door`.
    if behaviour.as_deref() == Some("mining") && !flammable {
        if use_tool(
            world,
            actor,
            target,
            MINERAL_DIG_DS,
            0,
            job(DoorBuildJob::MineralDig),
        ) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете раскапывать дверь...",
            );
        }
        return true;
    }
    match tool_of(world, held).map(|t| t.0) {
        Some(ToolKind::Wrench) => {
            if use_tool(
                world,
                actor,
                target,
                MINERAL_WRENCH_DS,
                0,
                job(DoorBuildJob::MineralWrench {
                    anchored: is_anchored,
                }),
            ) {
                let text = if is_anchored {
                    "Вы начинаете откручивать дверь от пола..."
                } else {
                    "Вы начинаете прикручивать дверь к полу..."
                };
                tell(world, client, Span::Notice, text);
            }
            return true;
        }
        Some(ToolKind::Welder) if !flammable => {
            if is_anchored {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Дверь всё ещё прочно закреплена!",
                );
            } else if use_tool(
                world,
                actor,
                target,
                MINERAL_APART_DS,
                5,
                job(DoorBuildJob::MineralApart),
            ) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете разваривать дверь...",
                );
            }
            return true;
        }
        Some(ToolKind::Crowbar) if flammable => {
            if is_anchored {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Дверь всё ещё прочно закреплена!",
                );
            } else if use_tool(
                world,
                actor,
                target,
                MINERAL_APART_DS,
                0,
                job(DoorBuildJob::MineralApart),
            ) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете разбирать дверь ломом...",
                );
            }
            return true;
        }
        _ => {}
    }
    // `if(!user.combat_mode) return attack_hand(user)`.
    let combat = world
        .get::<crate::combat::CombatMode>(actor.mob)
        .is_some_and(|c| c.0);
    if !combat {
        return try_switch(world, target, actor.mob);
    }
    false
}

/// tg `atom_deconstruct(TRUE)`: the door's sheets, the door gone.
pub fn deconstruct_mineral_door(world: &mut World, door: Entity) {
    let (Some(&pos), Some(door_path)) = (
        world.get::<TilePos>(door),
        path(world, door).map(str::to_owned),
    ) else {
        return;
    };
    let (sheet, amount) = mineral_sheets(&door_path);
    world.despawn(door);
    tile_changed(world, pos);
    spawn_stack(world, sheet, pos, amount);
}

// --- Click and job dispatch ----------------------------------------------------------------

/// The held item on a frame, an assembly, a welded firelock or a mineral door. True when the
/// click was theirs.
pub fn item_interaction(world: &mut World, actor: Actor, target: Entity, right: bool) -> bool {
    let Some(held) = actor.item else {
        return false;
    };
    if right {
        return windoor_interaction(world, actor, held, target, true);
    }
    frame_interaction(world, actor, held, target)
        || firedoor_interaction(world, actor, held, target)
        || windoor_interaction(world, actor, held, target, false)
        || mineral_interaction(world, actor, held, target)
}

/// The end of a [`DoorBuildJob`], re-checked at the moment of effect.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: DoorBuildJob) {
    let target = action.target;
    let client = action.client;
    let pos = action.target_pos;
    let consume = |world: &mut World| {
        action
            .item
            .is_none_or(|i| crate::tools::consume(world, i, action.amount))
    };
    let item_is = |world: &World, root: &str| action.item.is_some_and(|i| is(world, i, root));
    use DoorBuildJob as J;
    match job {
        J::FrameBuild { border, dir } => {
            if firelock_on(world, pos) || !consume(world) {
                return;
            }
            let path = if border {
                FIRELOCK_FRAME_BORDER
            } else {
                FIRELOCK_FRAME
            };
            if let Some(e) = spawn_obj(world, path, pos) {
                face(world, e, dir);
                set_frame(
                    world,
                    e,
                    FirelockFrame {
                        step: NO_CIRCUIT,
                        reinforced: false,
                    },
                );
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы собираете раму пожарного шлюза.",
            );
        }
        J::FrameReinforce => {
            let Some(mut f) = frame(world, target) else {
                return;
            };
            if f.step != PANEL_OPEN
                || f.reinforced
                || !item_is(&*world, PLASTEEL)
                || !consume(world)
            {
                return;
            }
            f.reinforced = true;
            set_frame(world, target, f);
            sound(world, target, DECONSTRUCT, 50.0);
            tell(world, client, Span::Notice, "Вы укрепляете раму.");
        }
        J::FrameCircuit => {
            let Some(mut f) = frame(world, target) else {
                return;
            };
            let Some(item) = action.item.filter(|&i| is(world, i, FIRELOCK_ELECTRONICS)) else {
                return;
            };
            if f.step != NO_CIRCUIT {
                return;
            }
            world.despawn(item);
            f.step = PANEL_OPEN;
            set_frame(world, target, f);
            sound(world, target, DECONSTRUCT, 50.0);
            tell(
                world,
                client,
                Span::Notice,
                "Вы вставляете и закрепляете плату.",
            );
        }
        J::FrameUncircuit => {
            let Some(mut f) = frame(world, target) else {
                return;
            };
            if f.step != PANEL_OPEN {
                return;
            }
            f.step = NO_CIRCUIT;
            set_frame(world, target, f);
            spawn_obj(world, FIRELOCK_ELECTRONICS, pos);
            sound(world, target, DECONSTRUCT, 50.0);
            tell(world, client, Span::Notice, "Вы извлекаете плату из рамы.");
        }
        J::FrameFinish => {
            let Some(f) = frame(world, target) else {
                return;
            };
            if f.step != PANEL_OPEN || firelock_on(world, pos) {
                return;
            }
            sound(world, target, DECONSTRUCT, 50.0);
            if finish_firelock(world, target, f).is_some() {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы заканчиваете пожарный шлюз.",
                );
            }
        }
        J::FrameCut => {
            let Some(f) = frame(world, target) else {
                return;
            };
            if f.step != NO_CIRCUIT || !consume(world) {
                return;
            }
            let directional = is(world, target, FIRELOCK_FRAME_BORDER);
            world.despawn(target);
            tile_changed(world, pos);
            spawn_stack(world, IRON, pos, if directional { 2 } else { 3 });
            if f.reinforced {
                spawn_stack(world, PLASTEEL, pos, 2);
            }
            tell(world, client, Span::Notice, "Вы разрезаете раму на металл.");
        }
        J::FiredoorUnbolt => {
            let ok = world
                .get::<Door>(target)
                .is_some_and(|d| d.kind == DoorKind::Firedoor && d.welded && !d.is_operating())
                && world.get::<FiredoorBoltsUnlocked>(target).is_some();
            if !ok {
                return;
            }
            sound(world, target, DECONSTRUCT, 50.0);
            if deconstruct_firedoor(world, target).is_some() {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы откручиваете болты пожарного шлюза.",
                );
            }
        }
        J::WindoorReinforce
        | J::WindoorAnchor { .. }
        | J::WindoorWire
        | J::WindoorUnwire
        | J::WindoorElectronics
        | J::WindoorUnelectronics
        | J::WindoorFinish
        | J::WindoorApart => complete_windoor(world, mob, action, job),
        J::MineralDig | J::MineralApart => {
            if !is_mineral_door(world, target) {
                return;
            }
            if job == J::MineralApart && anchored(world, target) {
                return;
            }
            if !consume(world) {
                return;
            }
            deconstruct_mineral_door(world, target);
            tell(world, client, Span::Notice, "Вы разбираете дверь.");
        }
        J::MineralWrench { anchored: was } => {
            if !is_mineral_door(world, target) || anchored(world, target) != was {
                return;
            }
            set_anchored(world, target, !was);
            // `set_anchored`: a loose door never blocks sight.
            let closed = world.get::<Density>(target).is_some();
            if was {
                world.entity_mut(target).remove::<Opaque>();
            } else if closed && !is(world, target, TRANSPARENT_DOOR) {
                world.entity_mut(target).insert(Opaque);
            }
            tile_changed(world, pos);
            let text = if was {
                "Вы откручиваете дверь от пола."
            } else {
                "Вы прикручиваете дверь к полу."
            };
            tell(world, client, Span::Notice, text);
        }
    }
}

fn complete_windoor(world: &mut World, mob: Entity, action: &DoAfter, job: DoorBuildJob) {
    let target = action.target;
    let client = action.client;
    let pos = action.target_pos;
    let Some(mut a) = windoor_assembly(world, target) else {
        return;
    };
    let consume = |world: &mut World| {
        action
            .item
            .is_none_or(|i| crate::tools::consume(world, i, action.amount))
    };
    let is_anchored = anchored(world, target);
    use DoorBuildJob as J;
    match job {
        J::WindoorReinforce => {
            let plasteel = action.item.is_some_and(|i| is(world, i, PLASTEEL));
            if a.secure || a.cables_added || !plasteel || !consume(world) {
                return;
            }
            a.secure = true;
            tell(world, client, Span::Notice, "Вы укрепляете окно-дверь.");
        }
        J::WindoorAnchor { anchored: was } => {
            let dir = facing(world, target);
            if a.cables_added
                || is_anchored != was
                || (!was && windoor_facing(world, pos, dir, target))
            {
                return;
            }
            set_anchored(world, target, !was);
            let text = if was {
                "Вы открепляете каркас окно-двери."
            } else {
                "Вы закрепляете каркас окно-двери."
            };
            tell(world, client, Span::Notice, text);
        }
        J::WindoorWire => {
            if !is_anchored || a.cables_added || !consume(world) {
                return;
            }
            a.cables_added = true;
            tell(
                world,
                client,
                Span::Notice,
                "Вы прокладываете провода в окно-двери.",
            );
        }
        J::WindoorUnwire => {
            if !a.cables_added {
                return;
            }
            a.cables_added = false;
            // `new/obj/item/stack/cable_coil(get_turf(user), 1)`.
            let at = world.get::<TilePos>(mob).copied().unwrap_or(pos);
            spawn_stack(world, CABLE, at, 1);
            tell(
                world,
                client,
                Span::Notice,
                "Вы вырезаете провода окно-двери.",
            );
        }
        J::WindoorElectronics => {
            let Some(item) = action.item.filter(|&i| is(world, i, ELECTRONICS)) else {
                return;
            };
            if !a.cables_added || a.electronics.is_some() {
                return;
            }
            a.electronics = Some(crate::airlock_construction::electronics_of(world, item));
            crate::airlock_construction::close_windows(world, item);
            world.despawn(item);
            tell(
                world,
                client,
                Span::Notice,
                "Вы устанавливаете электронику шлюза.",
            );
        }
        J::WindoorUnelectronics => {
            let Some(data) = a.electronics.take().filter(|_| a.cables_added) else {
                return;
            };
            if let Some(item) = spawn_obj(world, ELECTRONICS, pos) {
                world.entity_mut(item).insert(data);
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы извлекаете электронику шлюза.",
            );
        }
        J::WindoorFinish => {
            if !a.cables_added || a.electronics.is_none() {
                return;
            }
            if finish_windoor(world, target, &a).is_some() {
                tell(world, client, Span::Notice, "Вы заканчиваете окно-дверь.");
            }
            return;
        }
        J::WindoorApart => {
            if a.cables_added || is_anchored || !consume(world) {
                return;
            }
            world.despawn(target);
            tile_changed(world, pos);
            spawn_stack(world, RGLASS, pos, WINDOOR_SHEETS);
            if a.secure {
                spawn_stack(world, RODS, pos, 4);
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы разбираете каркас окно-двери.",
            );
            return;
        }
        _ => return,
    }
    set_windoor_assembly(world, target, a);
}
