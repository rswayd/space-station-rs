//! Wall frames (tg `/obj/item/wallframe`, `code/game/objects/items/wall_mounted.dm`) of the wall
//! mounts this module runs: a frame clicked on a closed turf next to the user (in line with it)
//! becomes its `result_path` on the user's tile, turned and shifted onto that wall; a
//! screwdriver on a frame mounts it on the wall the user faces; a wrench takes a frame apart
//! into its materials.
//!
//! Each `result_path` starts as tg's `Initialize(mapload = FALSE)` leaves it: an extinguisher
//! cabinet open and empty, a fire axe cabinet empty, a flasher without a bulb.
// shortcut: frames of machines with construction stages or their own module (fire alarms and
// their circuit, APCs, air alarms, intercoms, newscasters, status displays, cameras, lights,
// mirrors, ...) are not handled here; the button's panel starts closed (no screwdriver panels,
// it is built without a device) and the button frame is not put on tables. Add each with its
// machine's construction.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmAtom, DmItem, DmWallMount};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, TilePos};

use super::{LightSwitch, cabinets, controls, refresh_look};
use crate::inventory::Inventory;
use crate::sound::{PlaySound, playsound};
use crate::tools::{ToolKind, tell, tool_of};

/// `wallframe/interact_with_atom`: `playsound(loc, 'sound/machines/click.ogg', 75, TRUE)`.
const CLICK: &str = "sound/machines/click.ogg";
const IRON: &str = "/obj/item/stack/sheet/iron";
const GLASS: &str = "/obj/item/stack/sheet/glass";
const PLASTEEL: &str = "/obj/item/stack/sheet/plasteel";
const WOOD: &str = "/obj/item/stack/sheet/mineral/wood";

/// One frame type: tg `result_path`, `pixel_shift` and `custom_materials` (whole sheets, as
/// `atom_deconstruct` rounds them).
pub struct FrameKind {
    pub frame: &'static str,
    pub result: &'static str,
    pub pixel_shift: i16,
    pub materials: &'static [(&'static str, u32)],
}

/// The frames of `buttons.dm`, `lightswitch.dm`, `flasher.dm`, `extinguisher.dm`,
/// `fireaxe.dm`, the trophy fish mount (`crate::fishing::mount`), the photo frame
/// (`photos/frame.dm`, its photo moves along: [`crate::service::picture_frame_moved`]) and the
/// heretic's eldritch paintings (`eldritch_painting.dm`,
/// `crate::antag::heretic::server::EldritchPainting`). Subtypes come before their parents.
pub static FRAMES: [FrameKind; 14] = [
    FrameKind {
        frame: "/obj/item/wallframe/button",
        result: "/obj/machinery/button",
        pixel_shift: 24,
        materials: &[(IRON, 1)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/light_switch",
        result: "/obj/machinery/light_switch",
        pixel_shift: 26,
        materials: &[(IRON, 1)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/flasher",
        result: "/obj/machinery/flasher",
        pixel_shift: 28,
        materials: &[(IRON, 2)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/extinguisher_cabinet",
        result: "/obj/structure/extinguisher_cabinet",
        pixel_shift: 29,
        materials: &[(IRON, 2)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/fireaxecabinet/mechremoval",
        result: "/obj/structure/fireaxecabinet/mechremoval/empty",
        pixel_shift: 32,
        materials: &[(GLASS, 5), (PLASTEEL, 5)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/fireaxecabinet/jawsofrecovery",
        result: "/obj/structure/fireaxecabinet/jawsofrecovery/empty",
        pixel_shift: 32,
        materials: &[(GLASS, 5), (PLASTEEL, 5)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/fish",
        result: "/obj/structure/fish_mount",
        pixel_shift: 31,
        materials: &[(WOOD, 2)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/picture",
        result: "/obj/structure/sign/picture_frame",
        pixel_shift: 30,
        materials: &[(WOOD, 1)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/painting/eldritch/weeping",
        result: "/obj/structure/sign/painting/eldritch/weeping",
        pixel_shift: 30,
        materials: &[(WOOD, 1)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/painting/eldritch/desire",
        result: "/obj/structure/sign/painting/eldritch/desire",
        pixel_shift: 30,
        materials: &[(WOOD, 1)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/painting/eldritch/vines",
        result: "/obj/structure/sign/painting/eldritch/vines",
        pixel_shift: 30,
        materials: &[(WOOD, 1)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/painting/eldritch/beauty",
        result: "/obj/structure/sign/painting/eldritch/beauty",
        pixel_shift: 30,
        materials: &[(WOOD, 1)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/painting/eldritch/rust",
        result: "/obj/structure/sign/painting/eldritch/rust",
        pixel_shift: 30,
        materials: &[(WOOD, 1)],
    },
    FrameKind {
        frame: "/obj/item/wallframe/fireaxecabinet",
        result: "/obj/structure/fireaxecabinet/empty",
        pixel_shift: 32,
        materials: &[(GLASS, 5), (PLASTEEL, 5)],
    },
];

/// The frame and result types the game spawns (`ss13_mapload::GAME_RUNTIME_TYPES` lists them).
pub fn kind_of(world: &World, item: Entity) -> Option<&'static FrameKind> {
    let path = &world.get::<DmItem>(item)?.path;
    FRAMES.iter().find(|k| is_under(path, k.frame))
}

/// tg `wallframe/interact_with_atom` (the held frame on a closed turf), `screwdriver_act` and
/// `wrench_act` (a tool on a frame). True when the click was theirs.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if kind_of(world, held).is_some() {
        let closed = crate::construction::path_of(world, target)
            .is_some_and(|p| is_under(&p, "/turf/closed"));
        if !closed {
            return false;
        }
        let Some(&wall) = world.get::<TilePos>(target) else {
            return false;
        };
        mount(world, client, mob, held, wall);
        return true;
    }
    if kind_of(world, target).is_none() {
        return false;
    }
    match tool_of(world, held).map(|(k, _)| k) {
        Some(ToolKind::Screwdriver) => {
            let (Some(&at), Some(&Facing(dir))) =
                (world.get::<TilePos>(mob), world.get::<Facing>(mob))
            else {
                return true;
            };
            mount(world, client, mob, target, at.step(dir));
            true
        }
        Some(ToolKind::Wrench) => {
            dismantle(world, client, mob, target);
            true
        }
        _ => false,
    }
}

/// `try_build` then the mount: the frame's result on the user's tile facing `wall`.
fn mount(world: &mut World, client: Entity, mob: Entity, frame: Entity, wall: TilePos) {
    let Some(kind) = kind_of(world, frame) else {
        return;
    };
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let closed = crate::construction::turf_at(world, wall)
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/closed"));
    if !closed {
        return;
    }
    if at.chebyshev(wall).is_none_or(|d| d > 1) {
        tell(world, client, Span::Warning, "Слишком далеко!");
        return;
    }
    let Some(dir) = crate::pull::dir_between(at, wall).filter(|d| Dir::CARDINALS.contains(d))
    else {
        tell(
            world,
            client,
            Span::Warning,
            "Встаньте ровно напротив стены!",
        );
        return;
    };
    let floor = crate::construction::turf_at(world, at)
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/open/floor"));
    if !floor {
        tell(world, client, Span::Warning, "Здесь нельзя установить!");
        return;
    }
    let (dx, dy) = dir.offset();
    let offset = [dx as i16 * kind.pixel_shift, dy as i16 * kind.pixel_shift];
    if wall_item_on(world, at, dir) {
        tell(world, client, Span::Warning, "Здесь уже что-то висит!");
        return;
    }
    let Some(built) = crate::construction::spawn_obj(world, kind.result, at) else {
        return;
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(built) {
        appearance.pixel_offset = offset;
    }
    if let Some(mut atom) = world.get_mut::<DmAtom>(built) {
        atom.dir = dir as u8;
        atom.pixel_x = offset[0];
        atom.pixel_y = offset[1];
    }
    take_out(world, mob, frame);
    crate::service::picture_frame_moved(world, frame, Some(built), at);
    world.despawn(frame);
    init_built(world, built, at);
    playsound(world, PlaySound::new(at, CLICK, 75.0, true));
    let (user, name) = (
        crate::medical::name_of(world, mob),
        crate::medical::name_of(world, built),
    );
    crate::medical::visible(
        world,
        mob,
        &format!("Вы закрепляете {name} на стене."),
        &format!("{user} закрепляет {name} на стене."),
        Span::Notice,
    );
}

/// tg `check_wall_item` for interior wall items: a wall mount on `at` already hanging towards
/// `dir` (its pixel offset points at that wall).
fn wall_item_on(world: &World, at: TilePos, dir: Dir) -> bool {
    let (dx, dy) = dir.offset();
    world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(at)
        .iter()
        .filter(|&&e| world.get::<DmWallMount>(e).is_some())
        .filter_map(|&e| world.get::<Appearance>(e))
        .any(|a| {
            let [px, py] = a.pixel_offset;
            (dx != 0 && i32::from(px).signum() == dx) || (dy != 0 && i32::from(py).signum() == dy)
        })
}

/// The live state of a mount built from a frame: its type's `Initialize(mapload = FALSE)`.
fn init_built(world: &mut World, e: Entity, at: TilePos) {
    let Some(dm) = world.get::<DmWallMount>(e).cloned() else {
        return;
    };
    let path = dm.path.as_str();
    crate::power::register_machine(world, e);
    if is_under(path, ss13_mapload::wallmount::LIGHT_SWITCH_ROOT) {
        if let Some(area) = world.resource::<Areas>().area_at(at) {
            world.entity_mut(e).insert(LightSwitch { area });
        }
    } else if is_under(path, ss13_mapload::wallmount::BUTTON_ROOT) {
        controls::add_button(world, e, &dm);
    } else if is_under(path, ss13_mapload::wallmount::FLASHER_ROOT) {
        controls::add_flasher(world, e, &dm);
        // `if(mapload) bulb = new(src)`: a built flasher has no bulb and never flashes.
        if let Some(mut flasher) = world.get_mut::<controls::Flasher>(e) {
            flasher.burnt_out = true;
        }
    } else if is_under(path, ss13_mapload::wallmount::EXTINGUISHER_CABINET_ROOT) {
        // `if(!mapload) opened = TRUE`, no extinguisher.
        world.entity_mut(e).insert(cabinets::ExtinguisherCabinet {
            stored: None,
            opened: true,
        });
    } else if is_under(path, ss13_mapload::wallmount::FIREAXE_CABINET_ROOT) {
        cabinets::add_fireaxe_cabinet(world, e, &dm);
    }
    refresh_look(world, e);
}

/// tg `wrench_act` -> `deconstruct(TRUE)`: the frame's materials on its tile (or its holder's).
fn dismantle(world: &mut World, client: Entity, mob: Entity, frame: Entity) {
    let Some(kind) = kind_of(world, frame) else {
        return;
    };
    let Some(&pos) = world
        .get::<TilePos>(frame)
        .or_else(|| world.get::<TilePos>(mob))
    else {
        return;
    };
    tell(world, client, Span::Notice, "Вы разбираете рамку.");
    take_out(world, mob, frame);
    crate::service::picture_frame_moved(world, frame, None, pos);
    world.despawn(frame);
    for &(sheet, amount) in kind.materials {
        crate::construction::spawn_stack(world, sheet, pos, amount);
    }
    crate::construction::tile_changed(world, pos);
}

/// The frame leaves `mob`'s hands.
fn take_out(world: &mut World, mob: Entity, frame: Entity) {
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(frame) {
                *hand = None;
            }
        }
    }
}
