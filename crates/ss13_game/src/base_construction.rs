//! Base construction consoles (server), after tg `code/game/objects/structures/
//! construction_console/` (`/obj/machinery/computer/camera_advanced/base_construction`, its
//! `aux` and `centcom` subtypes, `construction_actions.dm`).
//!
//! The console is an advanced camera console (`crate::silicon::camera_advanced`) whose eye is a
//! construction holo-drone: the aux console's starts on the aux base controller and only moves
//! inside `/area/shuttle/auxiliary_base`; the CentCom one starts on the console and goes
//! anywhere. Its buttons instead of the camera ones:
//! - "Log out";
//! - "Configure RCD": the design list of the console's internal RCD (600 matter, half delays);
//! - "Build": the internal RCD builds its design at the eye (on the last object there the RCD
//!   works on, else the turf), with the RCD's own costs and delays, while the user stays at the
//!   console;
//! - aux only: "Place Tiny Fan" and "Install Plasma Anti-Wildlife Turret", four of each, on a
//!   free floor tile.
//!
//! Building is refused outside the allowed area and, for the aux base, once it is off the
//! station level. Sheets and matter cartridges used on the console load its RCD. A console from
//! the map starts restocked.
// shortcut: the drone is not shown to others while in use, no turret linking to the aux base
// controller (the turret works on its own; the controller is `cargo` "Вспомогательная база"),
// no move cooldown on "Build" (`CLICK_CD_RANGE`); add them with the aux base.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::silicon::camera_advanced::{AdvancedConsole, RemoteEye, RemoteEyeUser};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, tell};

pub const BASE_CONSOLE: &str = "/obj/machinery/computer/camera_advanced/base_construction";
const AUX_CONSOLE: &str = "/obj/machinery/computer/camera_advanced/base_construction/aux";
const INTERNAL_RCD: &str = "/obj/item/construction/rcd/internal";
/// `allowed_area` of the aux console.
pub const AUX_AREA: &str = "/area/shuttle/auxiliary_base";
const AUX_CONTROLLER: &str = "/obj/machinery/computer/auxiliary_base";
const TINY_FAN: &str = "/obj/structure/fans/tiny";
const AUX_TURRET: &str = "/obj/machinery/porta_turret/aux_base";
/// `restock_materials` of the aux console.
const AUX_STOCK: u32 = 4;

/// Button ids (after the advanced console's `advcam:` prefix, which routes them here).
pub const LOG_OUT: &str = "advcam:base:off";
pub const CONFIGURE: &str = "advcam:base:configure";
pub const BUILD: &str = "advcam:base:build";
pub const FAN: &str = "advcam:base:fan";
pub const TURRET: &str = "advcam:base:turret";

/// A base construction console: its internal RCD, `allowed_area` and `structures`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BaseConsole {
    pub rcd: Option<Entity>,
    pub aux: bool,
    pub fans: u32,
    pub turrets: u32,
}

pub fn is_base_console(path: &str) -> bool {
    is_under(path, BASE_CONSOLE)
}

pub struct BaseConstructionPlugin;

impl Plugin for BaseConstructionPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::HeldOnTarget, 2941, |w, c| {
            let Some(held) = c.held else {
                return false;
            };
            refill(w, c.client, c.target, held)
        })
        .add_systems(ss13_world::MapInit, init_consoles);
    }
}

fn init_consoles(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let consoles: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_base_console(&o.path))
        .map(|(e, _)| e)
        .collect();
    for console in consoles {
        setup(world, console, true);
    }
}

/// `Initialize`: the internal RCD (aux and CentCom), restocked on the map (`restock_materials`).
pub fn setup(world: &mut World, console: Entity, mapload: bool) {
    let Some(path) = world.get::<DmObj>(console).map(|o| o.path.clone()) else {
        return;
    };
    let aux = is_under(&path, AUX_CONSOLE);
    let rcd = crate::construction::spawn_detached(world, INTERNAL_RCD);
    if let Some(rcd) = rcd {
        crate::storage::put_inside(world, rcd, console);
        // The matter comes from the RCD's `attach` observer; top it up after it ran.
        world.flush();
        if mapload && let Some(mut m) = world.get_mut::<crate::devices::rcd::Matter>(rcd) {
            m.matter = m.max;
        }
    }
    let stock = if mapload && aux { AUX_STOCK } else { 0 };
    world.entity_mut(console).insert(BaseConsole {
        rcd,
        aux,
        fans: stock,
        turrets: stock,
    });
}

/// `find_spawn_spot`: the aux base controller inside the base, else the console's turf; `None`
/// when the aux console finds no controller ("ERROR: Unable to locate ...").
pub fn spawn_spot(world: &mut World, console: Entity) -> Option<TilePos> {
    let state = world.get::<BaseConsole>(console).copied()?;
    if !state.aux {
        return world.get::<TilePos>(console).copied();
    }
    let mut query = world.query::<(&DmObj, &TilePos)>();
    let spots: Vec<TilePos> = query
        .iter(world)
        .filter(|(o, _)| is_under(&o.path, AUX_CONTROLLER))
        .map(|(_, &p)| p)
        .collect();
    let found = spots.into_iter().find(|&p| in_aux_area(world, p));
    if found.is_none() {
        crate::atmos_machinery::machine_say(
            world,
            console,
            "ERROR: Unable to locate auxiliary base controller!",
        );
    }
    found
}

fn area_path(world: &World, pos: TilePos) -> Option<String> {
    let area = world.get_resource::<ss13_mapload::Areas>()?.area_at(pos)?;
    world
        .get::<ss13_mapload::DmPower>(area)
        .map(|p| p.path.clone())
}

fn in_aux_area(world: &World, pos: TilePos) -> bool {
    area_path(world, pos).is_some_and(|p| is_under(&p, AUX_AREA))
}

/// `base_construction/setLoc`: the drone moves only inside the allowed area.
pub fn may_move(world: &World, console: Entity, to: TilePos) -> bool {
    match world.get::<BaseConsole>(console) {
        Some(state) if state.aux => in_aux_area(world, to),
        _ => true,
    }
}

/// Whether `mob` looks through the eye of the console that owns `rcd`.
pub fn controls_rcd(world: &World, mob: Entity, rcd: Entity) -> bool {
    world.get::<RemoteEyeUser>(mob).is_some_and(|u| {
        world
            .get::<BaseConsole>(u.console)
            .is_some_and(|s| s.rcd == Some(rcd))
            && world
                .get::<AdvancedConsole>(u.console)
                .is_some_and(|c| c.user == Some(mob))
    })
}

/// The console's buttons (`populate_actions_list`): `(id, name, dmi, state)`.
pub fn buttons(
    world: &World,
    console: Entity,
) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    let Some(state) = world.get::<BaseConsole>(console) else {
        return Vec::new();
    };
    const ICONS: &str = "icons/mob/actions/actions_construction.dmi";
    let mut out = vec![
        (
            LOG_OUT,
            "Log out",
            "icons/mob/actions/actions_silicon.dmi",
            "camera_off",
        ),
        (CONFIGURE, "Configure RCD", "icons/obj/tools.dmi", "rcd"),
        (BUILD, "Build", ICONS, "build"),
    ];
    if state.aux {
        out.push((FAN, "Place Tiny Fan", ICONS, "build_fan"));
        out.push((
            TURRET,
            "Install Plasma Anti-Wildlife Turret",
            ICONS,
            "build_turret",
        ));
    }
    out
}

/// A pressed button (the user's eye checked by the caller); false for another id.
pub fn trigger(
    world: &mut World,
    client: Entity,
    mob: Entity,
    user: RemoteEyeUser,
    id: &str,
) -> bool {
    let console = user.console;
    let Some(state) = world.get::<BaseConsole>(console).copied() else {
        return false;
    };
    let Some(eye_pos) = world.get::<RemoteEye>(user.eye).map(|e| e.pos) else {
        return true;
    };
    match id {
        LOG_OUT => crate::silicon::camera_advanced::remove_control(world, console),
        CONFIGURE => {
            if let Some(rcd) = state.rcd {
                crate::devices::rcd::open_designs(world, client, mob, rcd, true);
            }
        }
        BUILD => {
            let Some(rcd) = state.rcd else {
                return true;
            };
            if !check_spot(world, client, &state, eye_pos) {
                return true;
            }
            let Some(target) = rcd_target(world, rcd, eye_pos) else {
                return true;
            };
            let actor = Actor {
                client,
                mob,
                item: Some(rcd),
            };
            crate::devices::rcd::remote_create(world, actor, rcd, target);
            playsound(
                world,
                PlaySound::new(eye_pos, "sound/items/deconstruct.ogg", 60.0, true),
            );
        }
        FAN | TURRET if state.aux => place_structure(world, client, console, id == FAN, eye_pos),
        _ => return false,
    }
    true
}

/// `check_spot`: inside the allowed area, and the aux base still on the station level.
fn check_spot(world: &mut World, client: Entity, state: &BaseConsole, pos: TilePos) -> bool {
    if !state.aux {
        return true;
    }
    if !in_aux_area(world, pos) {
        tell(
            world,
            client,
            Span::Warning,
            "Строить можно только внутри вспомогательной базы!",
        );
        return false;
    }
    if !crate::comms_server::on_station_level(world, pos) {
        tell(
            world,
            client,
            Span::Warning,
            "Вспомогательная база уже запущена, её больше нельзя менять.",
        );
        return false;
    }
    true
}

/// `build/Activate`: the last object on the tile the RCD works on, else the turf.
fn rcd_target(world: &World, rcd: Entity, pos: TilePos) -> Option<Entity> {
    let rcd_on =
        |e: Entity| world.get::<DmObj>(e).is_some() && crate::devices::rcd::works_on(world, rcd, e);
    let on_tile = world.resource::<ss13_world::SpatialIndex>().on_tile(pos);
    on_tile
        .iter()
        .copied()
        .rfind(|&e| rcd_on(e))
        .or_else(|| crate::construction::turf_at(world, pos))
}

/// `place_structure/Activate` and `after_place`.
fn place_structure(world: &mut World, client: Entity, console: Entity, fan: bool, pos: TilePos) {
    let Some(state) = world.get::<BaseConsole>(console).copied() else {
        return;
    };
    let (left, name, path, sound) = if fan {
        (
            state.fans,
            "вентиляторы",
            TINY_FAN,
            "sound/machines/click.ogg",
        )
    } else {
        (
            state.turrets,
            "турели",
            AUX_TURRET,
            "sound/items/tools/drill_use.ogg",
        )
    };
    if left == 0 {
        tell(
            world,
            client,
            Span::Warning,
            format!("В консоли закончились {name}!"),
        );
        return;
    }
    if !check_spot(world, client, &state, pos) {
        return;
    }
    let closed = crate::construction::turf_at(world, pos)
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/closed"));
    if closed {
        tell(
            world,
            client,
            Span::Warning,
            "Это можно поставить только на пол.",
        );
        return;
    }
    // A tiny fan is not dense; a turret needs a free tile.
    if !fan && crate::construction::tile_blocked(world, pos) {
        tell(
            world,
            client,
            Span::Warning,
            "Место чем-то занято. Освободите его и попробуйте снова.",
        );
        return;
    }
    let Some(placed) = crate::construction::spawn_obj(world, path, pos) else {
        return;
    };
    let left = left - 1;
    if let Some(mut s) = world.get_mut::<BaseConsole>(console) {
        if fan {
            s.fans = left;
        } else {
            s.turrets = left;
        }
    }
    playsound(world, PlaySound::new(pos, sound, 50.0, true));
    if fan {
        tell(
            world,
            client,
            Span::Notice,
            format!("Вентилятор установлен. Осталось: {left}."),
        );
    } else {
        crate::defenses::server::setup_turret(world, placed, 0.0);
        tell(
            world,
            client,
            Span::Notice,
            format!("Турель построена. Осталось: {left}."),
        );
    }
}

/// `item_interaction`: sheets and matter cartridges go into the internal RCD.
fn refill(world: &mut World, client: Entity, console: Entity, held: Entity) -> bool {
    let Some(rcd) = world.get::<BaseConsole>(console).and_then(|s| s.rcd) else {
        return false;
    };
    let fuel = world.get::<DmItem>(held).is_some_and(|i| {
        is_under(&i.path, "/obj/item/rcd_ammo") || is_under(&i.path, "/obj/item/stack/sheet")
    });
    if !fuel {
        return false;
    }
    crate::devices::rcd::load(world, client, rcd, held);
    true
}
