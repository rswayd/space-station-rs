//! Building airlocks (server): tg `/obj/structure/door_assembly` and its types
//! (`code/game/objects/structures/door_assembly.dm`, `door_assembly_types.dm`), airlock
//! electronics (`/obj/item/electronics/airlock`, `airlock_electronics.dm`) and the airlock
//! painter (`/obj/item/airlock_painter`, airlock `change_paintjob`).
//!
//! - An assembly: 4 iron sheets right-clicked on a floor with airlock electronics in the other
//!   hand (5 s, one per tile; [`build_assembly`]). A wrench (4 s) anchors it (not on a tile with
//!   another door) or loosens it; anchored, a cable (4 s, 1 piece) wires it, wirecutters (4 s)
//!   take the wire out again; wired, airlock electronics (4 s) go in, a crowbar (4 s) takes them
//!   out; a screwdriver (4 s) finishes the airlock (`finish_door`): the assembly's
//!   `airlock_type` (`glass_type` with glass in), facing its way, with the electronics' access
//!   (`req_one_access` when "one" is chosen, else `req_access`), unrestricted sides (latched),
//!   name (the electronics' `passed_name`, else the assembly's `base_name`), autoclosing.
//! - Glass or reinforced glass (4 s, 1 sheet) goes into an assembly with a glass version; a lit
//!   welder (4 s) takes it out again, or takes a loose glassless assembly apart into its
//!   `material_type` × `material_amt` (and its glass).
//! - The electronics' window (`AirlockElectronics`, in hand): the access list by station region,
//!   all/one access, the unrestricted sides, the airlock's name.
//! - The airlock painter on an airlock: a list of paint jobs (`available_paint_jobs`); the pick,
//!   re-checked (in reach, painter in hand, ink left), takes that airlock type's `icon`,
//!   `overlays_file` and `assemblytype` (a glass airlock refuses a paint job without a glass
//!   version) and uses one of the cartridge's 5 charges.
// shortcut: no stack recipe menu: the right click with electronics in the other hand builds the
// standard assembly (other kinds come from the map or the painter); no mineral plating
// (`construction_path_type` sheets, the welder taking it off), no `heat_proof` airlocks (the
// reinforced glass flag is kept on the assembly only), no assembly names or examine lines, no
// renaming with a pen, no RCD; destroyed assemblies leave nothing; finished airlocks are not
// taken back apart into assemblies (their electronics are kept on them for that); no
// cycle id of the electronics; the painter's toner cartridge
// cannot be swapped or refilled, and painting an assembly is not ported (tg has no code for it
// either).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::construct::PAINT_JOBS;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmDoor, DmDoorAssembly, DmItem, DmName, DmObj, DoorKind};
use ss13_proto::Prototypes;
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Facing, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::airlock_electronics::{
    AccessRegion, AirlockElectronicsData, AirlockElectronicsStatic, AirlockElectronicsUi,
    RegionAccess,
};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::{Mover, SpatialIndex};

use crate::construction::{Job, spawn_obj, spawn_stack, tile_changed};
use crate::doors::Door;
use crate::id_access::{STATION_REGIONS, access_desc, all_station_access, region_access_list};
use crate::inventory::Inventory;
use crate::obj_damage::set_anchored;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, ToolKind, do_after, tell, tool_of, use_tool};

pub const ASSEMBLY: &str = ss13_mapload::construct::DOOR_ASSEMBLY;
pub const ELECTRONICS: &str = "/obj/item/electronics/airlock";
pub const PAINTER: &str = "/obj/item/airlock_painter";
const IRON: &str = "/obj/item/stack/sheet/iron";
const GLASS: &str = "/obj/item/stack/sheet/glass";
const RGLASS: &str = "/obj/item/stack/sheet/rglass";
const CABLE: &str = "/obj/item/stack/cable_coil";
const AIRLOCK: &str = "/obj/machinery/door/airlock";
/// `AIRLOCK_ASSEMBLY_NEEDS_*`.
pub const NEEDS_WIRES: u8 = 0;
pub const NEEDS_ELECTRONICS: u8 = 1;
pub const NEEDS_SCREWDRIVER: u8 = 2;
/// The `standard airlock assembly` recipe: 4 sheets, `5 SECONDS`.
pub const ASSEMBLY_SHEETS: u32 = 4;
const ASSEMBLY_DS: f32 = 50.0;
/// Every assembly step's `use_tool`/`do_after`: 40 deciseconds.
const STEP_DS: f32 = 40.0;
/// `/obj/item/toner` `charges`.
pub const TONER_CHARGES: u32 = 5;
/// `trim(passed_name, 30)`.
const NAME_MAX: usize = 30;
const SPRAY: &str = "sound/effects/spray2.ogg";

/// `/obj/item/electronics/airlock` vars, on the item, kept in an assembly and on the airlock it
/// built.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct AirlockElectronics {
    pub accesses: Vec<String>,
    pub one_access: bool,
    /// BYOND dir bits.
    pub unres_sides: u8,
    pub passed_name: String,
    /// `shell`: the airlock it builds takes an integrated circuit.
    pub shell: bool,
}

/// A door assembly's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct DoorAssembly {
    /// `AIRLOCK_ASSEMBLY_NEEDS_*`.
    pub state: u8,
    pub glass: bool,
    /// `heat_proof_finished`: reinforced glass went in.
    pub heat_proof: bool,
    /// The installed electronics.
    pub electronics: Option<AirlockElectronics>,
}

/// An airlock painter's toner charges (absent: a full cartridge).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PainterInk(pub u32);

/// The timed assembly steps, with the state each started from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssemblyJob {
    /// Iron sheets on a floor with electronics in the other hand.
    Build,
    Anchor {
        anchored: bool,
    },
    Wire,
    Unwire,
    InstallElectronics,
    RemoveElectronics,
    Finish,
    /// Glass (`reinforced`: rglass) into the assembly.
    Glass {
        reinforced: bool,
    },
    /// Welder: the glass out.
    Unglass,
    /// Welder on a loose assembly without glass.
    Disassemble,
}

/// A paint job list open for an airlock (the `tgui_input_list` of `change_paintjob`).
#[derive(Component, Clone, Copy, Debug)]
struct PaintFor {
    airlock: Entity,
    painter: Entity,
}

pub struct AirlockConstructionPlugin;

impl Plugin for AirlockConstructionPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Assembly tool acts and `item_interaction`, the painter on an airlock.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2880, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        // The electronics' window.
        app.add_click_handler(ClickPhase::AttackSelf, 3160, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        app.register_ui::<ElectronicsWindow>().add_systems(
            Update,
            (
                electronics_acts.in_set(UiSystems::Handle),
                window_status.in_set(UiSystems::StatusHook),
                paint_answers
                    .in_set(UiSystems::Handle)
                    .after(ss13_tgui::prompt::PromptSystems::Answer),
            ),
        );
    }
}

fn item_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

fn is_item(world: &World, e: Entity, root: &str) -> bool {
    item_path(world, e).is_some_and(|p| is_under(p, root))
}

fn assembly_def(world: &World, e: Entity) -> Option<DmDoorAssembly> {
    world.get::<DmDoorAssembly>(e).cloned()
}

use crate::obj_damage::anchored;

/// The assembly's live state, made from its type on first use (a mapped or spawned assembly).
fn assembly(world: &mut World, e: Entity) -> Option<DoorAssembly> {
    if let Some(a) = world.get::<DoorAssembly>(e) {
        return Some(a.clone());
    }
    let def = assembly_def(world, e)?;
    let a = DoorAssembly {
        glass: def.glass,
        ..default()
    };
    world.entity_mut(e).insert(a.clone());
    Some(a)
}

/// The electronics on an item, made on first use.
pub(crate) fn electronics_of(world: &mut World, item: Entity) -> AirlockElectronics {
    if let Some(e) = world.get::<AirlockElectronics>(item) {
        return e.clone();
    }
    world.entity_mut(item).insert(AirlockElectronics::default());
    AirlockElectronics::default()
}

/// Airlock electronics in `mob`'s other hand than `held`: iron sheets build an assembly.
pub fn electronics_in_other_hand(world: &World, mob: Entity, held: Entity) -> bool {
    world.get::<Inventory>(mob).is_some_and(|inv| {
        inv.hands
            .iter()
            .flatten()
            .any(|&e| e != held && is_item(world, e, ELECTRONICS))
    })
}

/// A door or an assembly on `pos` (`one_per_turf`, the wrench's `door_check`).
fn door_on(world: &World, pos: TilePos, except: Entity) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            e != except
                && (world.get::<Door>(e).is_some() || world.get::<DmDoorAssembly>(e).is_some())
        })
}

/// Dense objects on `pos` (`CRAFT_CHECK_DENSITY`).
fn dense_on(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&e| world.get::<Mover>(e).is_none() && world.get::<ss13_mapload::Turf>(e).is_none())
}

/// The iron sheets' right click on a floor with electronics in the other hand (from
/// `crate::construction`): checks the tile and starts the 5 s build.
pub fn build_assembly(world: &mut World, actor: Actor, turf: Entity, pos: TilePos) {
    let Some(held) = actor.item else {
        return;
    };
    if door_on(world, pos, turf) || dense_on(world, pos) {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Здесь уже что-то стоит!",
        );
        return;
    }
    if crate::tools::stack_amount(world, held) < ASSEMBLY_SHEETS {
        tell(world, actor.client, Span::Warning, "Нужно 4 листа металла!");
        return;
    }
    do_after(
        world,
        actor,
        turf,
        ASSEMBLY_DS,
        ASSEMBLY_SHEETS,
        Job::Airlock(AssemblyJob::Build),
    );
}

/// The held item on an assembly or (a painter) on an airlock. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if is_item(world, held, PAINTER) {
        let airlock = world
            .get::<Door>(target)
            .is_some_and(|d| d.kind == DoorKind::Airlock);
        if airlock {
            ask_paintjob(world, actor, held, target);
        }
        return airlock;
    }
    let Some(def) = assembly_def(world, target) else {
        return false;
    };
    let Some(a) = assembly(world, target) else {
        return false;
    };
    let job = |j| Job::Airlock(j);
    let is_anchored = anchored(world, target);
    if let Some((tool, _)) = tool_of(world, held) {
        let started = match tool {
            ToolKind::Wrench => {
                let pos = world.get::<TilePos>(target).copied();
                if !is_anchored && pos.is_some_and(|p| door_on(world, p, target)) {
                    tell(world, client, Span::Warning, "Здесь уже есть другая дверь!");
                    return true;
                }
                let anchored = is_anchored;
                (
                    use_tool(
                        world,
                        actor,
                        target,
                        STEP_DS,
                        0,
                        job(AssemblyJob::Anchor { anchored }),
                    ),
                    if anchored {
                        "Вы начинаете откреплять каркас шлюза от пола..."
                    } else {
                        "Вы начинаете крепить каркас шлюза к полу..."
                    },
                )
            }
            ToolKind::Wirecutters if a.state == NEEDS_ELECTRONICS => (
                use_tool(world, actor, target, STEP_DS, 0, job(AssemblyJob::Unwire)),
                "Вы начинаете вырезать провода из каркаса шлюза...",
            ),
            ToolKind::Crowbar if a.state == NEEDS_SCREWDRIVER => (
                use_tool(
                    world,
                    actor,
                    target,
                    STEP_DS,
                    0,
                    job(AssemblyJob::RemoveElectronics),
                ),
                "Вы начинаете извлекать электронику из каркаса шлюза...",
            ),
            ToolKind::Screwdriver if a.state == NEEDS_SCREWDRIVER => (
                use_tool(world, actor, target, STEP_DS, 0, job(AssemblyJob::Finish)),
                "Вы начинаете заканчивать шлюз...",
            ),
            ToolKind::Welder => {
                // `if(!mineral && !glass && anchored) return ITEM_INTERACT_SKIP_TO_ATTACK`.
                if !a.glass && is_anchored {
                    return false;
                }
                let j = if a.glass {
                    AssemblyJob::Unglass
                } else {
                    AssemblyJob::Disassemble
                };
                let text = if a.glass {
                    "Вы начинаете вываривать стекло из каркаса шлюза..."
                } else {
                    "Вы начинаете разбирать каркас шлюза..."
                };
                (use_tool(world, actor, target, STEP_DS, 1, job(j)), text)
            }
            // `ITEM_INTERACT_SKIP_TO_ATTACK` of the other tool acts.
            _ => return false,
        };
        if started.0 {
            tell(world, client, Span::Notice, started.1);
        }
        return true;
    }
    if is_anchored && a.state == NEEDS_WIRES && is_item(world, held, CABLE) {
        if use_tool(world, actor, target, STEP_DS, 1, job(AssemblyJob::Wire)) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете прокладывать провода в каркасе шлюза...",
            );
        }
        return true;
    }
    if a.state == NEEDS_ELECTRONICS && is_item(world, held, ELECTRONICS) {
        if do_after(
            world,
            actor,
            target,
            STEP_DS,
            0,
            job(AssemblyJob::InstallElectronics),
        ) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете устанавливать электронику в каркас шлюза...",
            );
        }
        return true;
    }
    let glass = is_item(world, held, GLASS);
    let reinforced = is_item(world, held, RGLASS);
    if !a.glass && (glass || reinforced) {
        if def.noglass {
            tell(world, client, Span::Warning, "Сюда нельзя вставить стекло!");
        } else if do_after(
            world,
            actor,
            target,
            STEP_DS,
            1,
            job(AssemblyJob::Glass { reinforced }),
        ) {
            if let Some(&pos) = world.get::<TilePos>(target) {
                playsound(
                    world,
                    PlaySound::new(pos, "sound/items/tools/crowbar.ogg", 100.0, true),
                );
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете вставлять стекло в каркас шлюза...",
            );
        }
        return true;
    }
    false
}

/// The end of an [`AssemblyJob`]: its effect if the state it started from still holds.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: AssemblyJob) {
    let target = action.target;
    let client = action.client;
    let pos = action.target_pos;
    let consume = |world: &mut World| {
        action
            .item
            .is_none_or(|i| crate::tools::consume(world, i, action.amount))
    };
    if job == AssemblyJob::Build {
        if door_on(world, pos, target) || dense_on(world, pos) || !consume(world) {
            return;
        }
        if let Some(e) = spawn_obj(world, ASSEMBLY, pos) {
            assembly(world, e);
            refresh_look(world, e);
        }
        tell(world, client, Span::Notice, "Вы собираете каркас шлюза.");
        return;
    }
    let (Some(def), Some(mut a)) = (assembly_def(world, target), assembly(world, target)) else {
        return;
    };
    let is_anchored = anchored(world, target);
    match job {
        AssemblyJob::Build => {}
        AssemblyJob::Anchor { anchored: was } => {
            if is_anchored != was || (!was && door_on(world, pos, target)) {
                return;
            }
            set_anchored(world, target, !was);
            let text = if was {
                "Вы открепляете каркас шлюза."
            } else {
                "Вы закрепляете каркас шлюза."
            };
            tell(world, client, Span::Notice, text);
        }
        AssemblyJob::Wire => {
            if a.state != NEEDS_WIRES || !is_anchored || !consume(world) {
                return;
            }
            a.state = NEEDS_ELECTRONICS;
            tell(
                world,
                client,
                Span::Notice,
                "Вы прокладываете провода в каркасе шлюза.",
            );
        }
        AssemblyJob::Unwire => {
            if a.state != NEEDS_ELECTRONICS {
                return;
            }
            a.state = NEEDS_WIRES;
            // `new/obj/item/stack/cable_coil(get_turf(user), 1)`.
            let at = world.get::<TilePos>(mob).copied().unwrap_or(pos);
            spawn_stack(world, CABLE, at, 1);
            tell(
                world,
                client,
                Span::Notice,
                "Вы вырезаете провода из каркаса шлюза.",
            );
        }
        AssemblyJob::InstallElectronics => {
            let Some(item) = action.item.filter(|&i| is_item(world, i, ELECTRONICS)) else {
                return;
            };
            if a.state != NEEDS_ELECTRONICS {
                return;
            }
            a.electronics = Some(electronics_of(world, item));
            close_windows(world, item);
            world.despawn(item);
            a.state = NEEDS_SCREWDRIVER;
            tell(
                world,
                client,
                Span::Notice,
                "Вы устанавливаете электронику шлюза.",
            );
        }
        AssemblyJob::RemoveElectronics => {
            if a.state != NEEDS_SCREWDRIVER {
                return;
            }
            a.state = NEEDS_ELECTRONICS;
            let data = a.electronics.take().unwrap_or_default();
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
        AssemblyJob::Finish => {
            if a.state != NEEDS_SCREWDRIVER {
                return;
            }
            if finish_door(world, target, &def, &a).is_some() {
                tell(world, client, Span::Notice, "Вы заканчиваете шлюз.");
            }
            return;
        }
        AssemblyJob::Glass { reinforced } => {
            if a.glass || def.noglass || !consume(world) {
                return;
            }
            a.glass = true;
            a.heat_proof = reinforced;
            tell(
                world,
                client,
                Span::Notice,
                "Вы вставляете стекло в каркас шлюза.",
            );
        }
        AssemblyJob::Unglass => {
            if !a.glass || !consume(world) {
                return;
            }
            let sheet = if a.heat_proof { RGLASS } else { GLASS };
            spawn_stack(world, sheet, pos, 1);
            a.glass = false;
            a.heat_proof = false;
            tell(world, client, Span::Notice, "Вы вывариваете стекло.");
        }
        AssemblyJob::Disassemble => {
            if a.glass || is_anchored || !consume(world) {
                return;
            }
            disassemble(world, target, &def, &a, pos);
            tell(world, client, Span::Notice, "Вы разбираете каркас шлюза.");
            return;
        }
    }
    world.entity_mut(target).insert(a);
    refresh_look(world, target);
}

/// `atom_deconstruct(TRUE)`: the material, the glass, the electronics; the assembly gone.
fn disassemble(world: &mut World, e: Entity, def: &DmDoorAssembly, a: &DoorAssembly, pos: TilePos) {
    let material = if def.material_type.is_empty() {
        IRON
    } else {
        def.material_type.as_str()
    };
    spawn_stack(world, material, pos, def.material_amt.max(1));
    if a.glass {
        spawn_stack(world, if a.heat_proof { RGLASS } else { GLASS }, pos, 1);
    }
    world.despawn(e);
    tile_changed(world, pos);
}

/// `finish_door`: the airlock in the assembly's place, the assembly gone.
pub fn finish_door(
    world: &mut World,
    e: Entity,
    def: &DmDoorAssembly,
    a: &DoorAssembly,
) -> Option<Entity> {
    let pos = *world.get::<TilePos>(e)?;
    let path = if a.glass && !def.glass_type.is_empty() {
        def.glass_type.clone()
    } else if def.airlock_type.is_empty() {
        AIRLOCK.to_owned()
    } else {
        def.airlock_type.clone()
    };
    let facing = world.get::<Facing>(e).copied();
    let electronics = a.electronics.clone().unwrap_or_default();
    world.despawn(e);
    let Some(door) = spawn_obj(world, &path, pos) else {
        warn!("door assembly at {pos:?}: no prototype for {path}");
        return None;
    };
    if let Some(facing) = facing {
        world.entity_mut(door).insert(facing);
        if let Some(mut atom) = world.get_mut::<DmAtom>(door) {
            atom.dir = facing.0 as u8;
        }
    }
    if !crate::doors::setup_built(world, door) {
        return Some(door);
    }
    let name = if !electronics.passed_name.is_empty() {
        electronics.passed_name.clone()
    } else {
        def.base_name.clone()
    };
    if !name.is_empty()
        && let Some(mut n) = world.get_mut::<DmName>(door)
    {
        n.name = name;
    }
    if let Some(mut d) = world.get_mut::<Door>(door) {
        apply_electronics(&mut d, &electronics);
    }
    if electronics.shell {
        world
            .entity_mut(door)
            .insert(crate::wiremod::server::Shell {
                kind: crate::wiremod::server::ShellKind::Airlock,
                circuit: None,
            });
    }
    world.entity_mut(door).insert(electronics);
    // What map load gives an airlock: its wires and its area's power.
    add_wires(world, door, pos);
    crate::power::register_machine(world, door);
    tile_changed(world, pos);
    Some(door)
}

/// The electronics' part of `finish_door`: access, unrestricted sides (latched), autoclose.
pub fn apply_electronics(door: &mut Door, electronics: &AirlockElectronics) {
    door.unres_sides = electronics.unres_sides;
    door.unres_latch = electronics.unres_sides != 0;
    if electronics.one_access {
        door.req_one_access = electronics.accesses.clone();
        door.req_access.clear();
    } else {
        door.req_access = electronics.accesses.clone();
        door.req_one_access.clear();
    }
    door.autoclose = true;
}

fn add_wires(world: &mut World, door: Entity, pos: TilePos) {
    let area_path = world
        .get_resource::<ss13_mapload::Areas>()
        .and_then(|a| a.area_at(pos))
        .and_then(|a| world.get::<ss13_mapload::DmPower>(a))
        .map(|p| p.path.clone())
        .unwrap_or_default();
    let Some(wires) = world
        .get_resource_mut::<crate::wires::WireColors>()
        .map(|mut colors| {
            colors.wires_for(
                crate::wires::WireHolder::Airlock,
                &crate::wires::airlock_set(&area_path),
            )
        })
    else {
        return;
    };
    world.entity_mut(door).insert(wires);
}

/// `update_overlays`: `construction` with the solid fill (`fill_construction` of its icon) or
/// the glass one (`glass_construction` of its overlays), and the panel of its state
/// (`panel_c<state + 1>`).
fn refresh_look(world: &mut World, e: Entity) {
    let (Some(def), Some(a), Some(atom)) = (
        assembly_def(world, e),
        world.get::<DoorAssembly>(e).cloned(),
        world.get::<DmAtom>(e).cloned(),
    ) else {
        return;
    };
    let Some(index) = world.get_resource::<IconIndex>() else {
        return;
    };
    let fill = if a.glass {
        index.get(&def.overlays_file, "glass_construction")
    } else {
        index.get(&atom.icon, "fill_construction")
    };
    let panel = index.get(&def.overlays_file, &format!("panel_c{}", a.state + 1));
    let overlays: Vec<Overlay> = [fill, panel]
        .into_iter()
        .flatten()
        .map(Overlay::new)
        .collect();
    if let Some(mut appearance) = world.get_mut::<Appearance>(e)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
}

/// `change_paintjob`: the paint job list (`tgui_input_list`, sorted).
fn ask_paintjob(world: &mut World, actor: Actor, painter: Entity, airlock: Entity) {
    if !can_paint(world, actor.client, painter) {
        return;
    }
    let mut items: Vec<String> = PAINT_JOBS.iter().map(|(n, _)| (*n).to_owned()).collect();
    items.sort();
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        ss13_tgui::prompt::Ask::new(
            actor.client,
            actor.mob,
            "Customize",
            "Paintjob for this airlock",
            ss13_tgui::prompt::PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PaintFor { airlock, painter });
}

/// `can_use`: ink left. Tells why not.
fn can_paint(world: &mut World, client: Entity, painter: Entity) -> bool {
    let charges = world
        .get::<PainterInk>(painter)
        .map_or(TONER_CHARGES, |i| i.0);
    if charges < 1 {
        tell(world, client, Span::Warning, "краска закончилась!");
        return false;
    }
    true
}

/// The paint job picks, re-checked at the moment of effect: the airlock in reach, the painter
/// in the user's hands with ink.
fn paint_answers(
    world: &mut World,
    mut cursor: Local<MessageCursor<ss13_tgui::prompt::PromptAnswered>>,
) {
    let answers: Vec<(
        Entity,
        Entity,
        Entity,
        Option<ss13_tgui::prompt::PromptAnswer>,
    )> = cursor
        .read(world.resource::<Messages<ss13_tgui::prompt::PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(&PaintFor { airlock, painter }) = world.get::<PaintFor>(prompt) else {
            continue;
        };
        let Some(ss13_tgui::prompt::PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let mut names: Vec<&str> = PAINT_JOBS.iter().map(|(n, _)| *n).collect();
        names.sort();
        let Some(path) = names
            .get(i)
            .and_then(|n| PAINT_JOBS.iter().find(|(m, _)| m == n))
            .map(|(_, p)| *p)
        else {
            continue;
        };
        let held = world
            .get::<Inventory>(actor)
            .is_some_and(|inv| inv.hands.contains(&Some(painter)));
        if !held
            || world.get::<Door>(airlock).is_none()
            || !crate::storage::in_reach(world, actor, airlock)
            || !can_paint(world, client, painter)
        {
            continue;
        }
        paint(world, client, painter, airlock, path);
    }
}

/// The paint job `path` on `airlock` (`change_paintjob` after the pick).
pub fn paint(
    world: &mut World,
    client: Entity,
    painter: Entity,
    airlock: Entity,
    path: &str,
) -> bool {
    let Some((icon, overlays, assembly_type)) = world.get_resource::<Prototypes>().and_then(|p| {
        let proto = p.get(path)?;
        Some((
            proto.get::<DmAtom>()?.icon.clone(),
            proto.get::<DmDoor>()?.overlays_file.clone(),
            proto
                .get::<DmObj>()
                .map(|o| o.assembly_type.clone())
                .unwrap_or_default(),
        ))
    }) else {
        return false;
    };
    let noglass = world
        .get_resource::<Prototypes>()
        .and_then(|p| p.get(&assembly_type))
        .and_then(|p| p.get::<DmDoorAssembly>())
        .is_some_and(|a| a.noglass);
    let glass = world.get::<DmDoor>(airlock).is_some_and(|d| {
        d.airlock_material == "glass" || (d.airlock_material.is_empty() && d.glass)
    });
    if glass && noglass {
        tell(
            world,
            client,
            Span::Warning,
            "Эту покраску можно нанести только на шлюз без стекла.",
        );
        return false;
    }
    // `use_paint`.
    let charges = world
        .get::<PainterInk>(painter)
        .map_or(TONER_CHARGES, |i| i.0);
    world
        .entity_mut(painter)
        .insert(PainterInk(charges.saturating_sub(1)));
    if let Some(&pos) = world.get::<TilePos>(airlock) {
        playsound(world, PlaySound::new(pos, SPRAY, 50.0, true));
    }
    crate::doors::repaint(world, airlock, &icon, &overlays);
    if let Some(mut obj) = world.get_mut::<DmObj>(airlock) {
        obj.assembly_type = assembly_type;
    }
    true
}

/// The electronics in hand: their window.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !is_item(world, item, ELECTRONICS) {
        return false;
    }
    electronics_of(world, item);
    // Not on the map while held: `hands_state` is [`window_status`], not the distance check.
    world
        .entity_mut(item)
        .insert(ss13_tgui::server::AlwaysState);
    let title = world
        .get::<DmName>(item)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<ElectronicsWindow>(client, mob, item, title));
    true
}

pub(crate) fn close_windows(world: &mut World, item: Entity) {
    let mut q = world.query::<&mut UiWindow>();
    for mut w in q.iter_mut(world) {
        if w.source == item {
            w.cap_status(ss13_tgui::UiStatus::Close);
        }
    }
}

/// `hands_state`: the window closes once the electronics leave the user's hands.
fn window_status(mut windows: Query<&mut UiWindow>, inventories: Query<&Inventory>) {
    for mut window in &mut windows {
        if window.interface == ElectronicsWindow::NAME
            && !inventories
                .get(window.actor)
                .is_ok_and(|inv| inv.hands.contains(&Some(window.source)))
        {
            window.cap_status(ss13_tgui::UiStatus::Close);
        }
    }
}

pub struct ElectronicsWindow;

impl UiInterface for ElectronicsWindow {
    const NAME: &'static str = AirlockElectronicsUi::NAME;
    type Data = AirlockElectronicsData;
    type StaticData = AirlockElectronicsStatic;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElectronicsAct {
    ClearAll,
    GrantAll,
    OneAccess,
    Set(String),
    DirecSet(u8),
    GrantRegion(String),
    DenyRegion(String),
    PassedName(String),
    SetShell(bool),
}

/// `ui_static_data`: the station regions (`SSid_access.station_regions`) with the accesses
/// that have a description.
pub fn station_regions() -> Vec<AccessRegion> {
    STATION_REGIONS
        .iter()
        .map(|(name, accesses)| AccessRegion {
            name: (*name).to_owned(),
            accesses: accesses
                .iter()
                .filter_map(|a| {
                    Some(RegionAccess {
                        desc: access_desc(a)?.to_owned(),
                        access: (*a).to_owned(),
                    })
                })
                .collect(),
        })
        .collect()
}

impl ServerUi for ElectronicsWindow {
    type Source = &'static AirlockElectronics;
    type UserParam = ();
    type Act = ElectronicsAct;

    fn static_data(_: &AirlockElectronics) -> AirlockElectronicsStatic {
        AirlockElectronicsStatic {
            regions: station_regions(),
        }
    }

    fn data(
        e: &AirlockElectronics,
        _: Entity,
        _: &SystemParamItem<'_, '_, ()>,
    ) -> AirlockElectronicsData {
        AirlockElectronicsData {
            accesses: e.accesses.clone(),
            one_access: e.one_access,
            unres_direction: e.unres_sides,
            passed_name: e.passed_name.clone(),
            shell: e.shell,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ElectronicsAct, UiReject> {
        let text = |key: &str| {
            params
                .get(key)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams(format!("{key}: missing")))
        };
        match action {
            "clear_all" => Ok(ElectronicsAct::ClearAll),
            "grant_all" => Ok(ElectronicsAct::GrantAll),
            "one_access" => Ok(ElectronicsAct::OneAccess),
            "set" => text("access").map(ElectronicsAct::Set),
            "direc_set" => match params.number("unres_direction")? {
                d @ (1.0 | 2.0 | 4.0 | 8.0) => Ok(ElectronicsAct::DirecSet(d as u8)),
                _ => Err(UiReject::BadParams("unres_direction: a cardinal".into())),
            },
            "grant_region" => text("region").map(ElectronicsAct::GrantRegion),
            "deny_region" => text("region").map(ElectronicsAct::DenyRegion),
            "passedName" => text("passedName").map(ElectronicsAct::PassedName),
            // `!!params["on"]`.
            "set_shell" => Ok(ElectronicsAct::SetShell(
                params
                    .get("on")
                    .is_some_and(|v| !matches!(v, "" | "0" | "false")),
            )),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `do_action`. Accesses are kept to the station's (the client cannot add others).
pub fn apply_act(e: &mut AirlockElectronics, act: ElectronicsAct) {
    let station = all_station_access();
    match act {
        ElectronicsAct::ClearAll => {
            e.accesses.clear();
            e.one_access = false;
        }
        ElectronicsAct::GrantAll => {
            e.accesses = station.iter().map(|a| (*a).to_owned()).collect();
        }
        ElectronicsAct::OneAccess => e.one_access = !e.one_access,
        ElectronicsAct::Set(access) => {
            if let Some(i) = e.accesses.iter().position(|a| *a == access) {
                e.accesses.remove(i);
            } else if station.contains(&access.as_str()) {
                e.accesses.push(access);
            }
        }
        ElectronicsAct::DirecSet(bit) => e.unres_sides ^= bit,
        ElectronicsAct::GrantRegion(region) => {
            for a in region_access_list(&[region.as_str()]) {
                if !e.accesses.contains(&a) {
                    e.accesses.push(a);
                }
            }
        }
        ElectronicsAct::DenyRegion(region) => {
            let gone = region_access_list(&[region.as_str()]);
            e.accesses.retain(|a| !gone.contains(a));
        }
        ElectronicsAct::SetShell(on) => e.shell = on,
        ElectronicsAct::PassedName(name) => {
            // BandaStation `sanitize(passed_name)`, tg `trim(..., 30)`.
            e.passed_name = crate::records::console::sanitize(&name, NAME_MAX);
        }
    }
}

/// `ui_act`, against the electronics now (the window's status checked the hands).
fn electronics_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ElectronicsWindow>>>) {
    let pending: Vec<(Entity, ElectronicsAct)> = acts
        .read(world.resource::<Messages<UiActed<ElectronicsWindow>>>())
        .map(|a| (a.source, a.act.clone()))
        .collect();
    for (source, act) in pending {
        if let Some(mut e) = world.get_mut::<AirlockElectronics>(source) {
            apply_act(&mut e, act);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electronics_acts_follow_do_action() {
        let mut e = AirlockElectronics::default();
        apply_act(&mut e, ElectronicsAct::Set("engineering".into()));
        apply_act(&mut e, ElectronicsAct::Set("not_an_access".into()));
        assert_eq!(e.accesses, vec!["engineering".to_owned()]);
        apply_act(&mut e, ElectronicsAct::Set("engineering".into()));
        assert!(e.accesses.is_empty(), "set toggles");
        apply_act(&mut e, ElectronicsAct::GrantRegion("Engineering".into()));
        assert!(e.accesses.contains(&"maint_tunnels".to_owned()));
        apply_act(&mut e, ElectronicsAct::DenyRegion("Engineering".into()));
        assert!(e.accesses.is_empty());
        apply_act(&mut e, ElectronicsAct::DirecSet(1));
        apply_act(&mut e, ElectronicsAct::DirecSet(4));
        apply_act(&mut e, ElectronicsAct::DirecSet(1));
        assert_eq!(e.unres_sides, 4);
        apply_act(&mut e, ElectronicsAct::OneAccess);
        assert!(e.one_access);
        apply_act(&mut e, ElectronicsAct::GrantAll);
        assert!(e.accesses.contains(&"captain".to_owned()));
        apply_act(&mut e, ElectronicsAct::ClearAll);
        assert!(e.accesses.is_empty() && !e.one_access);
        apply_act(
            &mut e,
            ElectronicsAct::PassedName("<b>Engine</b> Room and a very long tail".into()),
        );
        assert!(e.passed_name.chars().count() <= NAME_MAX);
        assert!(!e.passed_name.contains('<'));
    }

    #[test]
    fn parse_act_rejects_diagonals() {
        let params = vec![("unres_direction".to_owned(), "5".to_owned())];
        assert!(ElectronicsWindow::parse_act("direc_set", &UiParams::new(&params)).is_err());
        let params = vec![("unres_direction".to_owned(), "8".to_owned())];
        assert_eq!(
            ElectronicsWindow::parse_act("direc_set", &UiParams::new(&params)),
            Ok(ElectronicsAct::DirecSet(8))
        );
    }
}
