//! The holodeck (server), after tg `code/modules/holodeck`.
//!
//! - The control console (`/obj/machinery/computer/holodeck`) links the nearest
//!   `/area/station/holodeck/rec_center` and loads programs into it: the `Holodeck` window lists
//!   the programs ([`PROGRAMS`]), the restricted ones only once emagged; a load waits out a 2 s
//!   cooldown (7 s while damaged), clears the previous projection and places the program's map
//!   template (`_maps/templates/holodeck_*.dmm`) at the area's bottom-left corner, on holodeck
//!   compatible turfs only (holo floors, engine floors). Everything the template brings is a
//!   [`Hologram`]; with the safeties on, its items deal stamina damage.
//! - Every machine tick a running program checks its floor: a turf that is neither a holo floor
//!   nor a wall shuts it down ("The holodeck overloads!") and damages the holodeck; holograms
//!   that left the area fade away (not when emagged). Losing power loads the offline program,
//!   power back reloads the last one 2.5 s later.
//! - Holo floors ([`HOLOFLOOR`]) ignore every item used on them (no prying, no tiling) and pick
//!   their random looks at map init (grass, asteroid, basalt, hyperspace, the chapel corners).
//! - Holo effects (`/obj/effect/holodeck_effect`) activate after a load: a holographic card
//!   deck, sparks with a hotspot, a random manual, holographic animals; they and what they made
//!   go with the next load.
//! - Items: the dodgeball (10 stamina damage on a human it hits, 5%: knocked off their feet),
//!   the ready button (when every button of its area is ready the holo windows of the area go
//!   and its mobs read "FIGHT!"), the holodeck conveyor (an item used on it is put onto it), the
//!   holographic skateboard (never picked up, `crate::vehicles`).
//! - Emag: the restricted programs are offered and the holograms keep their real damage; a
//!   silicon resets the safeties from the window.
// shortcut: every load leaves its emptied template level behind (~9x10 tiles, a few KiB); load
// straight onto the station level if the map loader learns to paste onto an existing one.
// shortcut: no power draw per hologram, no overload explosion (`SSexplosions.lowturf`) or
// hotspot, no EMP/explosion/blob shutdown, no 1218 AD shuttle unlock, no admin message for
// restricted programs (server log only).
// shortcut: a non-holographic item carried by a holographic mob goes with it.
// shortcut: holographic mobs keep their death drops (`TRAIT_PERMANENTLY_MORTAL`, no blood,
// no hunger are not modelled for spawned mobs).

use std::path::PathBuf;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmItem, DmName, DmPower, DmTurfBuild, MapLoadPlugin, Turf};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::holodeck::{HoloProgram, HolodeckData, HolodeckUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use crate::combat::CombatRng;
use crate::construction::{path_of, turf_at};
use crate::health::Health;
use crate::power::Unpowered;
use crate::sound::{PlaySound, playsound};
use crate::storage::Stored;
use crate::tools::tell;

pub const COMPUTER: &str = "/obj/machinery/computer/holodeck";
/// `mapped_start_area`.
pub const REC_CENTER: &str = "/area/station/holodeck/rec_center";
pub const HOLOFLOOR: &str = "/turf/open/floor/holofloor";
const ENGINE_FLOOR: &str = "/turf/open/floor/engine";
const EFFECT: &str = "/obj/effect/holodeck_effect";
pub const READY_BUTTON: &str = "/obj/machinery/readybutton";
pub const HOLO_CONVEYOR: &str = "/obj/machinery/conveyor/holodeck";
pub const DODGEBALL: &str = "/obj/item/toy/dodgeball";
const HOLO_WINDOW: &str = "/obj/structure/window";
/// `offline_program`.
pub const OFFLINE: &str = "holodeck_offline";
/// `HOLODECK_CD`, `HOLODECK_DMG_CD`.
pub const COOLDOWN: f64 = 2.0;
pub const DAMAGED_COOLDOWN: f64 = 5.0;
/// `SSmachines` wait.
const TICK_SECS: f64 = 2.0;
/// `toggle_power`: `addtimer(load_program, 2.5 SECONDS)`.
const POWER_BACK_SECS: f64 = 2.5;
const DODGEBALL_SOUND: &str = "sound/items/dodgeball.ogg";
const SPARKS_SOUND: &str = "sparks";

/// The `/obj/effect/holodeck_effect/cards` deck.
pub const HOLO_DECK: &str = "/obj/item/toy/cards/deck/syndicate/holographic";
pub const HOLO_CARP: &str = "/mob/living/basic/carp/holographic";
pub const HOLO_BEE: &str = "/mob/living/basic/bee/toxin";
pub const HOLO_MONKEY: &str = "/mob/living/carbon/human/species/monkey";
pub const HOLO_PENGUIN: &str = "/mob/living/basic/pet/penguin/emperor/neuter";
pub const HOLO_PENGUIN_SHAMEBRERO: &str = "/mob/living/basic/pet/penguin/emperor/shamebrero/neuter";
pub const HOLO_PENGUIN_BABY: &str = "/mob/living/basic/pet/penguin/baby/permanent";
pub const HOLO_CRAB_JON: &str = "/mob/living/basic/crab/jon";
/// `mobspawner/pet`: always one of these...
pub const PET_ALWAYS: &[&str] = &[
    "/mob/living/basic/butterfly",
    "/mob/living/basic/chick/permanent",
    "/mob/living/basic/pet/fox/docile",
    "/mob/living/basic/rabbit",
];
/// ...plus one dog...
pub const PET_DOGS: &[&str] = &[
    "/mob/living/basic/pet/dog/corgi",
    "/mob/living/basic/pet/dog/corgi/puppy",
    "/mob/living/basic/pet/dog/pug",
];
/// ...and one cat, then one of the six is picked.
pub const PET_CATS: &[&str] = &[
    "/mob/living/basic/pet/cat",
    "/mob/living/basic/pet/cat/kitten",
];
/// `get_random_manual`: `valid_subtypesof(/obj/item/book/manual)` without `random` and `nuclear`.
pub const MANUALS: &[&str] = &[
    "/obj/item/book/manual/chef_recipes",
    "/obj/item/book/manual/hydroponics_pod_people",
    "/obj/item/book/manual/ripley_build_and_repair",
    "/obj/item/book/manual/wiki/atmospherics",
    "/obj/item/book/manual/wiki/barman_recipes",
    "/obj/item/book/manual/wiki/chemistry",
    "/obj/item/book/manual/wiki/detective",
    "/obj/item/book/manual/wiki/engineering_construction",
    "/obj/item/book/manual/wiki/engineering_guide",
    "/obj/item/book/manual/wiki/engineering_hacking",
    "/obj/item/book/manual/wiki/medicine",
    "/obj/item/book/manual/wiki/research_and_development",
    "/obj/item/book/manual/wiki/security_space_law",
    "/obj/item/book/manual/wiki/surgery",
];

/// A `/datum/map_template/holodeck`: `mappath` is `_maps/templates/[id].dmm`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Program {
    pub id: &'static str,
    pub name: &'static str,
    pub restricted: bool,
}

const fn program(id: &'static str, name: &'static str, restricted: bool) -> Program {
    Program {
        id,
        name,
        restricted,
    }
}

/// `subtypesof(/datum/map_template/holodeck)` in declaration order.
pub const PROGRAMS: &[Program] = &[
    program(OFFLINE, "Holodeck - Offline", false),
    program("holodeck_emptycourt", "Holodeck - Empty Court", false),
    program("holodeck_dodgeball", "Holodeck - Dodgeball Court", false),
    program("holodeck_basketball", "Holodeck - Basketball Court", false),
    program(
        "holodeck_thunderdome",
        "Holodeck - Thunderdome Arena",
        false,
    ),
    program("holodeck_beach", "Holodeck - Beach", false),
    program("holodeck_lounge", "Holodeck - Lounge", false),
    program("holodeck_petpark", "Holodeck - Pet Park", false),
    program("holodeck_firingrange", "Holodeck - Firing Range", false),
    program("holodeck_animeschool", "Holodeck - Anime School", false),
    program("holodeck_chapelcourt", "Holodeck - Chapel Courtroom", false),
    program("holodeck_spacechess", "Holodeck - Space Chess", false),
    program("holodeck_spacecheckers", "Holodeck - Space Checkers", false),
    program("holodeck_kobayashi", "Holodeck - Kobayashi Maru", false),
    program(
        "holodeck_winterwonderland",
        "Holodeck - Winter Wonderland",
        false,
    ),
    program("holodeck_photobooth", "Holodeck - Photobooth", false),
    program("holodeck_skatepark", "Holodeck - Skatepark", false),
    program("holodeck_microwave", "Holodeck - Microwave Paradise", false),
    program("holodeck_baseball", "Holodeck - Baseball Field", false),
    program("holodeck_card_battle", "Holodeck - TGC Battle Arena", false),
    program("holodeck_fitness", "Holodeck - Gymnasium", false),
    program("holodeck_medicalsim", "Holodeck - Emergency Medical", true),
    program("holodeck_thunderdome1218", "Holodeck - 1218 AD", true),
    program(
        "holodeck_burntest",
        "Holodeck - Atmospheric Burn Test",
        true,
    ),
    program(
        "holodeck_wildlifesim",
        "Holodeck - Wildlife Simulation",
        true,
    ),
    program("holodeck_holdoutbunker", "Holodeck - Holdout Bunker", true),
    program("holodeck_anthophillia", "Holodeck - Anthophillia", true),
    program(
        "holodeck_refuelingstation",
        "Holodeck - Refueling Station",
        true,
    ),
    program("holodeck_prison", "Holodeck - Prison Block", true),
];

pub fn find_program(id: &str) -> Option<&'static Program> {
    PROGRAMS.iter().find(|p| p.id == id)
}

/// Where the program templates come from; insert before the plugin builds to override (tests).
/// `None`: the BYOND checkout of [`MapLoadPlugin`].
#[derive(Resource, Clone, Debug, Default)]
pub struct HolodeckSource(pub Option<PathBuf>);

/// tg `HOLOGRAM_1`: made by a holodeck.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Hologram;

/// The `damtype` a hologram item had before the safeties set it to stamina.
#[derive(Component, Clone, Debug)]
pub struct RealDamtype(pub String);

/// A holodeck control console.
#[derive(Component, Clone, Debug)]
pub struct Holodeck {
    /// `linked`.
    pub area: Entity,
    /// `bottom_left`.
    pub bottom_left: TilePos,
    /// The linked area's tiles on the console's level.
    pub tiles: Vec<TilePos>,
    pub program: String,
    pub last_program: Option<String>,
    pub spawned: Vec<Entity>,
    pub effects: Vec<Entity>,
    pub cooldown_until: f64,
    pub damaged: bool,
    pub emagged: bool,
    pub powered: bool,
    pub reload_at: Option<f64>,
    pub next_tick: f64,
}

/// A ready declaration device.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ReadyButton {
    pub ready: bool,
    pub event_started: bool,
}

pub struct HolodeckPlugin;

impl Plugin for HolodeckPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<HolodeckSource>() {
            let source = app
                .world()
                .get_resource::<MapLoadPlugin>()
                .map(|m| m.source.clone());
            app.insert_resource(HolodeckSource(source));
        }
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The console's window, the ready button.
        app.add_click_handler(ClickPhase::EmptyHand, 3493, |w, c| {
            crate::holodeck::interact(w, c.client, c.mob, c.target)
        });
        // Holo floors, the ready button and the holodeck conveyor take no item interactions.
        app.add_click_handler(ClickPhase::HeldOnTarget, 53, |w, c| {
            c.held
                .is_some_and(|h| crate::holodeck::item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.register_ui::<HolodeckWindow>()
            .add_systems(ss13_world::MapInit, (init_consoles, init_mapped))
            .add_systems(
                Update,
                (
                    window_acts.in_set(UiSystems::Handle),
                    process.run_if(any_with_component::<Holodeck>),
                ),
            );
    }
}

fn now(world: &World) -> f64 {
    world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64())
}

fn turf_path(world: &World, pos: TilePos) -> Option<String> {
    turf_at(world, pos).and_then(|t| world.get::<DmTurfBuild>(t).map(|b| b.path.clone()))
}

/// `holodeck_compatible`: holo floors and engine floors take programs.
pub fn holodeck_compatible(path: &str) -> bool {
    is_under(path, HOLOFLOOR) || is_under(path, ENGINE_FLOOR)
}

/// `typecache_holodeck_linked_floorcheck_ok`.
pub fn floor_ok(path: &str) -> bool {
    is_under(path, HOLOFLOOR) || is_under(path, "/turf/closed")
}

/// The random look of a holo floor at `(x, y)` (`Initialize`); `roll` picks among the variants.
pub fn holofloor_state(
    path: &str,
    x: i32,
    y: i32,
    roll: &mut dyn FnMut(u32) -> u32,
) -> Option<String> {
    if is_under(path, "/turf/open/floor/holofloor/grass") {
        return Some(format!("grass{}", roll(4)));
    }
    if is_under(path, "/turf/open/floor/holofloor/asteroid") {
        return Some(format!("asteroid{}", roll(13)));
    }
    if is_under(path, "/turf/open/floor/holofloor/basalt") {
        // `prob(15)`.
        return (roll(100) < 15).then(|| format!("basalt{}", roll(13)));
    }
    if is_under(path, "/turf/open/floor/holofloor/hyperspace") {
        return Some(format!(
            "speedspace_ns_{}",
            (x + 5 * y + (y.rem_euclid(2) + 1) * 7).rem_euclid(15) + 1
        ));
    }
    None
}

/// The chapel holo floor corners' `direction`.
fn chapel_dir(path: &str) -> Option<ss13_shared::Dir> {
    use ss13_shared::Dir;
    if path.ends_with("/chapel/bottom_left") {
        Some(Dir::West)
    } else if path.ends_with("/chapel/top_right") {
        Some(Dir::East)
    } else if path.ends_with("/chapel/top_left") {
        Some(Dir::North)
    } else {
        None
    }
}

fn init_mapped(world: &mut World) {
    let mut q = world
        .query_filtered::<(Entity, &DmTurfBuild, &TilePos), (With<Turf>, With<ss13_world::Mapped>)>(
        );
    let floors: Vec<(Entity, String, TilePos)> = q
        .iter(world)
        .filter(|(_, b, _)| is_under(&b.path, HOLOFLOOR))
        .map(|(e, b, &p)| (e, b.path.clone(), p))
        .collect();
    for (e, path, pos) in floors {
        let state = holofloor_state(&path, pos.x, pos.y, &mut |n| {
            world.resource_mut::<CombatRng>().range(0, n - 1)
        });
        if let Some(state) = state {
            crate::obj_damage::set_icon_state(world, e, &state);
        }
        if let Some(dir) = chapel_dir(&path) {
            world.entity_mut(e).insert(ss13_shared::Facing(dir));
        }
    }
    let mut q = world.query_filtered::<(Entity, &DmPower), With<ss13_world::Mapped>>();
    let buttons: Vec<Entity> = q
        .iter(world)
        .filter(|(_, p)| is_under(&p.path, READY_BUTTON))
        .map(|(e, _)| e)
        .collect();
    for e in buttons {
        world.entity_mut(e).insert(ReadyButton::default());
    }
}

fn init_consoles(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmPower, &TilePos), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, TilePos)> = q
        .iter(world)
        .filter(|(_, p, _)| is_under(&p.path, COMPUTER))
        .map(|(e, _, &pos)| (e, pos))
        .collect();
    for (e, pos) in found {
        setup_console(world, e, pos, REC_CENTER);
    }
}

/// `post_machine_initialize`: links the console to the `area_path` area on its level. False (the
/// console stays inert) without one, or from inside a holodeck.
pub fn setup_console(world: &mut World, e: Entity, pos: TilePos, area_path: &str) -> bool {
    let areas = world.resource::<Areas>();
    let area = areas
        .entities()
        .iter()
        .copied()
        .find(|&a| world.get::<DmPower>(a).is_some_and(|p| p.path == area_path));
    let Some(area) = area else {
        warn!("holodeck console at {pos:?} has no matching holodeck area");
        return false;
    };
    let own = areas
        .area_at(pos)
        .and_then(|a| world.get::<DmPower>(a))
        .is_some_and(|p| is_under(&p.path, "/area/station/holodeck"));
    if own {
        warn!("holodeck console at {pos:?} is inside a holodeck");
        return false;
    }
    // One pass over the console's level at map init.
    let Some((w, h)) = world.resource::<SpatialIndex>().level_size(pos.z) else {
        return false;
    };
    let mut tiles = Vec::new();
    for y in 1..=h as i32 {
        for x in 1..=w as i32 {
            let tile = TilePos::new(x, y, pos.z);
            if areas.area_at(tile) == Some(area) {
                tiles.push(tile);
            }
        }
    }
    let Some(min_x) = tiles.iter().map(|t| t.x).min() else {
        warn!("holodeck console at {pos:?} has an invalid holodeck area");
        return false;
    };
    let min_y = tiles.iter().map(|t| t.y).min().unwrap_or(1);
    // The mapped room is the offline program already: no load at round start.
    let t = now(world);
    world.entity_mut(e).insert(Holodeck {
        area,
        bottom_left: TilePos::new(min_x, min_y, pos.z),
        tiles,
        program: OFFLINE.to_owned(),
        last_program: None,
        spawned: Vec::new(),
        effects: Vec::new(),
        cooldown_until: t + COOLDOWN,
        damaged: false,
        emagged: false,
        powered: true,
        reload_at: None,
        next_tick: t + TICK_SECS,
    });
    true
}

fn say(world: &mut World, console: Entity, text: &str) {
    crate::atmos_machinery::machine_say(world, console, text);
}

/// `load_program(map_id, force, add_delay)`. True when the program was placed.
pub fn load_program(world: &mut World, console: Entity, id: &str, force: bool) -> bool {
    let Some(holo) = world.get::<Holodeck>(console).cloned() else {
        return false;
    };
    if holo.program == id {
        return false;
    }
    let (id, force) = if world.get::<Unpowered>(console).is_some() {
        (OFFLINE, true)
    } else {
        (id, force)
    };
    let Some(program) = find_program(id) else {
        return false;
    };
    let t = now(world);
    if !force && t < holo.cooldown_until {
        say(world, console, "ERROR. Recalibrating projection apparatus.");
        return false;
    }
    let floor_fine = holo.damaged && floorcheck(world, &holo.tiles);
    if let Some(mut h) = world.get_mut::<Holodeck>(console) {
        h.cooldown_until = t + if h.damaged {
            COOLDOWN + DAMAGED_COOLDOWN
        } else {
            COOLDOWN
        };
        if floor_fine {
            h.damaged = false;
        }
        h.program = id.to_owned();
    }
    clear_projection(world, console);
    let spawned = place_template(world, console, program);
    if program.restricted {
        info!("loaded a restricted Holodeck program: {}", program.id);
    }
    let emagged = world.get::<Holodeck>(console).is_some_and(|h| h.emagged);
    if let Some(mut h) = world.get_mut::<Holodeck>(console) {
        h.spawned = spawned;
    }
    finish_spawn(world, console);
    nerf(world, console, !emagged);
    true
}

/// `template.load(bottom_left)` with the holodeck `update_blacklist`: what the template brings
/// (not its turfs), closets' contents included.
fn place_template(world: &mut World, console: Entity, program: &Program) -> Vec<Entity> {
    let Some(origin) = world.get::<Holodeck>(console).map(|h| h.bottom_left) else {
        return Vec::new();
    };
    let Some(path) = world
        .resource::<HolodeckSource>()
        .0
        .as_ref()
        .map(|s| s.join(format!("_maps/templates/{}.dmm", program.id)))
    else {
        warn!("holodeck template {}: no source", program.id);
        return Vec::new();
    };
    let scratch = match ss13_mapload::load_template(world, &path) {
        Ok(z) => z,
        Err(e) => {
            warn!("holodeck template {}: {e:#}", path.display());
            ss13_world::run_map_init(world);
            return Vec::new();
        }
    };
    let (w, h) = world
        .resource::<SpatialIndex>()
        .level_size(scratch)
        .unwrap_or((0, 0));
    let mut spawned = Vec::new();
    let mut changed = Vec::new();
    for dy in 0..h as i32 {
        for dx in 0..w as i32 {
            let from = TilePos::new(1 + dx, 1 + dy, scratch);
            let to = TilePos::new(origin.x + dx, origin.y + dy, origin.z);
            let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(from).to_vec();
            let compatible = turf_path(world, to).is_some_and(|p| holodeck_compatible(&p));
            if !compatible {
                for e in here {
                    world.despawn(e);
                }
                continue;
            }
            for e in here {
                if world.get::<Turf>(e).is_some() {
                    // `should_place_on_top = FALSE`: the template's turf replaces the old one.
                    if let Some(old) = turf_at(world, to) {
                        world.despawn(old);
                    }
                    world.entity_mut(e).insert(to);
                    let air = world.get::<ss13_atmos::TurfAir>(e).cloned();
                    if let (Some(air), Some(mut atmos)) =
                        (air, world.get_resource_mut::<ss13_atmos::Atmos>())
                    {
                        atmos.change_turf(to, &air);
                    }
                    continue;
                }
                let visual = world.get::<ss13_atmos::GasOverlay>(e).is_some()
                    || world.get::<ss13_atmos::HotspotEffect>(e).is_some();
                if visual {
                    continue;
                }
                world.entity_mut(e).insert(to);
                spawned.push(e);
            }
            crate::construction::tile_changed(world, to);
            changed.push(to);
        }
    }
    ss13_world::run_map_init(world);
    // `register_contents`: what a closet holds after its `PopulateContents`.
    let mut i = 0;
    while i < spawned.len() {
        if let Some(stored) = world.get::<Stored>(spawned[i]) {
            spawned.extend(stored.items().iter().copied());
        }
        i += 1;
    }
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        for &pos in &changed {
            atmos.blocker_changed(pos, false);
        }
    }
    if !changed.is_empty() {
        let (min, max) = (
            (origin.x, origin.y),
            (origin.x + w as i32 - 1, origin.y + h as i32 - 1),
        );
        ss13_mapload::light::relight_rect(world, origin.z, min, max);
    }
    spawned
}

/// `finish_spawn` / `finalize_spawned`: holograms marked, effects activated.
fn finish_spawn(world: &mut World, console: Entity) {
    let Some(spawned) = world.get::<Holodeck>(console).map(|h| h.spawned.clone()) else {
        return;
    };
    let mut kept = Vec::new();
    let mut effects = Vec::new();
    for e in spawned {
        if world.get_entity(e).is_err() {
            continue;
        }
        world.entity_mut(e).insert(Hologram);
        if path_of(world, e).is_some_and(|p| is_under(&p, EFFECT)) {
            effects.push(e);
            kept.extend(activate_effect(world, e));
        } else {
            kept.push(e);
        }
    }
    for &e in &kept {
        if let Ok(mut entity) = world.get_entity_mut(e) {
            entity.insert(Hologram);
        }
    }
    if let Some(mut h) = world.get_mut::<Holodeck>(console) {
        h.spawned = kept;
        h.effects = effects;
    }
}

fn pick<'a>(world: &mut World, list: &[&'a str]) -> &'a str {
    let i = world
        .resource_mut::<CombatRng>()
        .range(0, list.len() as u32 - 1) as usize;
    list[i]
}

/// `/obj/effect/holodeck_effect/activate`: what the effect made.
fn activate_effect(world: &mut World, effect: Entity) -> Vec<Entity> {
    let (Some(path), Some(&pos)) = (path_of(world, effect), world.get::<TilePos>(effect)) else {
        return Vec::new();
    };
    let kind = &path[EFFECT.len()..];
    let made = match kind {
        "/cards" => crate::construction::spawn_obj(world, HOLO_DECK, pos),
        "/sparks" => {
            crate::effects::particles::do_sparks(world, 3, true, pos);
            if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
                atmos.hotspot_expose(pos, 50_000.0, 50_000.0, true);
            }
            None
        }
        "/random_book" => {
            let book = pick(world, MANUALS);
            crate::construction::spawn_obj(world, book, pos)
        }
        _ if kind.starts_with("/mobspawner") => {
            let mob = mob_type(world, kind);
            crate::mobs::spawn::spawn_mob(world, mob, pos)
        }
        _ => None,
    };
    let Some(made) = made else {
        return Vec::new();
    };
    world.entity_mut(made).insert(Hologram);
    let mut out = vec![made];
    if let Some(stored) = world.get::<Stored>(made) {
        out.extend(stored.items().iter().copied());
    }
    out
}

/// `mobspawner/mobtype` of a `/mobspawner...` effect path suffix.
fn mob_type(world: &mut World, kind: &str) -> &'static str {
    match kind {
        "/mobspawner/pet" => {
            let dog = pick(world, PET_DOGS);
            let cat = pick(world, PET_CATS);
            let mut all: Vec<&'static str> = PET_ALWAYS.to_vec();
            all.push(dog);
            all.push(cat);
            pick(world, &all)
        }
        "/mobspawner/bee" => HOLO_BEE,
        "/mobspawner/monkey" => HOLO_MONKEY,
        "/mobspawner/penguin" => {
            if world.resource_mut::<CombatRng>().chance(1.0) {
                HOLO_PENGUIN_SHAMEBRERO
            } else {
                HOLO_PENGUIN
            }
        }
        "/mobspawner/penguin_baby" => HOLO_PENGUIN_BABY,
        "/mobspawner/crab/jon" => HOLO_CRAB_JON,
        _ => HOLO_CARP,
    }
}

/// `clear_projection`: every hologram fades, every effect deactivates.
fn clear_projection(world: &mut World, console: Entity) {
    let Some((spawned, effects)) = world.get_mut::<Holodeck>(console).map(|mut h| {
        (
            std::mem::take(&mut h.spawned),
            std::mem::take(&mut h.effects),
        )
    }) else {
        return;
    };
    for e in spawned {
        derez(world, e);
    }
    for e in effects {
        if world.get_entity(e).is_ok() {
            world.despawn(e);
        }
    }
}

/// `derez`: what is not a hologram inside it drops out first.
pub fn derez(world: &mut World, e: Entity) {
    if world.get_entity(e).is_err() {
        return;
    }
    let pos = crate::explosions::turf_of(world, e);
    let inside: Vec<Entity> = world
        .get::<Stored>(e)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    if let Some(pos) = pos {
        for item in inside {
            if world.get::<Hologram>(item).is_none() {
                crate::storage::take_out(world, item, pos);
            }
        }
    }
    world.despawn(e);
    if let Some(pos) = pos {
        crate::construction::tile_changed(world, pos);
    }
}

/// `floorcheck`: every tile of the room a holo floor or a wall.
pub fn floorcheck(world: &World, tiles: &[TilePos]) -> bool {
    tiles.iter().all(|&pos| match turf_at(world, pos) {
        None => false,
        Some(t) => world
            .get::<DmTurfBuild>(t)
            .is_none_or(|b| floor_ok(&b.path)),
    })
}

/// `nerf(nerf_this)`: hologram items deal stamina damage, or their own again.
pub fn nerf(world: &mut World, console: Entity, nerf_this: bool) {
    let Some(spawned) = world.get::<Holodeck>(console).map(|h| h.spawned.clone()) else {
        return;
    };
    for e in spawned {
        let Some(current) = world.get::<DmItem>(e).map(|i| i.damtype.clone()) else {
            continue;
        };
        if nerf_this {
            if world.get::<RealDamtype>(e).is_none() {
                world.entity_mut(e).insert(RealDamtype(current));
            }
            world.get_mut::<DmItem>(e).unwrap().damtype = "stamina".to_owned();
        } else if let Some(real) = world.get::<RealDamtype>(e).cloned() {
            world.get_mut::<DmItem>(e).unwrap().damtype = real.0;
        }
    }
}

/// `emergency_shutdown`.
pub fn emergency_shutdown(world: &mut World, console: Entity) {
    {
        let Some(mut h) = world.get_mut::<Holodeck>(console) else {
            return;
        };
        h.last_program = Some(h.program.clone());
    }
    load_program(world, console, OFFLINE, true);
}

fn process(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &Holodeck)>();
    let consoles: Vec<Entity> = q.iter(world).map(|(e, _)| e).collect();
    for console in consoles {
        power_change(world, console, t);
        let Some(h) = world.get::<Holodeck>(console).cloned() else {
            continue;
        };
        if t < h.next_tick {
            continue;
        }
        world.get_mut::<Holodeck>(console).unwrap().next_tick = t + TICK_SECS;
        machine_tick(world, console, h);
    }
}

/// `power_change` -> `toggle_power`.
fn power_change(world: &mut World, console: Entity, t: f64) {
    let unpowered = world.get::<Unpowered>(console).is_some();
    let Some(h) = world.get::<Holodeck>(console).cloned() else {
        return;
    };
    if h.powered == unpowered {
        world.get_mut::<Holodeck>(console).unwrap().powered = !unpowered;
        if unpowered {
            world.get_mut::<Holodeck>(console).unwrap().last_program = Some(h.program.clone());
            load_program(world, console, OFFLINE, true);
        } else if h.last_program.as_deref().is_some_and(|p| p != OFFLINE) {
            world.get_mut::<Holodeck>(console).unwrap().reload_at = Some(t + POWER_BACK_SECS);
        }
    }
    if h.reload_at.is_some_and(|at| at <= t) {
        world.get_mut::<Holodeck>(console).unwrap().reload_at = None;
        if let Some(last) = h.last_program {
            load_program(world, console, &last, true);
        }
    }
}

/// `process`: sparks while damaged, the floor check, holograms that left the room.
fn machine_tick(world: &mut World, console: Entity, h: Holodeck) {
    // `SPT_PROB(5)`, then `SPT_PROB(2.5)` per tile until one sparks.
    if h.damaged
        && world
            .resource_mut::<CombatRng>()
            .chance(5.0 * TICK_SECS as f32)
    {
        for &pos in &h.tiles {
            if world
                .resource_mut::<CombatRng>()
                .chance(2.5 * TICK_SECS as f32)
            {
                crate::effects::particles::do_sparks(world, 2, true, pos);
                break;
            }
        }
    }
    if world.get::<Unpowered>(console).is_some() || h.program == OFFLINE {
        return;
    }
    if !floorcheck(world, &h.tiles) {
        emergency_shutdown(world, console);
        world.get_mut::<Holodeck>(console).unwrap().damaged = true;
        if let Some(&pos) = world.get::<TilePos>(console) {
            visible(world, pos, Span::Warning, "The holodeck overloads!");
        }
        for &pos in &h.tiles {
            if world.resource_mut::<CombatRng>().chance(30.0) {
                crate::effects::particles::do_sparks(world, 2, true, pos);
            }
        }
        return;
    }
    if !h.emagged {
        let areas = world.resource::<Areas>();
        let gone: Vec<Entity> = h
            .spawned
            .iter()
            .copied()
            .filter(|&e| {
                crate::explosions::turf_of(world, e)
                    .is_none_or(|p| areas.area_at(p) != Some(h.area))
            })
            .collect();
        if !gone.is_empty() {
            for &e in &gone {
                derez(world, e);
            }
            world
                .get_mut::<Holodeck>(console)
                .unwrap()
                .spawned
                .retain(|e| !gone.contains(e));
        }
    }
}

fn visible(world: &mut World, pos: TilePos, span: Span, text: &str) {
    for message in
        crate::pull::visible_message(world, pos, ChatChannel::Local, (span, text.to_owned()), &[])
    {
        world.write_message(message);
    }
}

/// `emag_act`: the safeties are gone, the holograms deal their real damage.
pub fn emag_act(world: &mut World, client: Entity, console: Entity) -> bool {
    let Some(h) = world.get::<Holodeck>(console) else {
        return false;
    };
    if h.emagged {
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(console) {
        playsound(world, PlaySound::new(pos, SPARKS_SOUND, 75.0, true));
    }
    world.get_mut::<Holodeck>(console).unwrap().emagged = true;
    tell(
        world,
        client,
        Span::Warning,
        "You override the safety and security protocols.",
    );
    info!("the Holodeck Control Console was emagged");
    say(
        world,
        console,
        "Warning. Automatic shutoff and derezzing protocols have been corrupted. Please call \
         Nanotrasen maintenance and do not use the simulator.",
    );
    nerf(world, console, false);
    true
}

/// A click on the console (its window) or on a ready button.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<ReadyButton>(target).is_some() {
        ready_button(world, client, target);
        return true;
    }
    if world.get::<Holodeck>(target).is_none() {
        return false;
    }
    if world.get::<Unpowered>(target).is_none() {
        let title = world.get::<DmName>(target).map_or_else(String::new, |n| {
            crate::vending::plain_name(&n.name).to_owned()
        });
        world.write_message(OpenUi::new::<HolodeckWindow>(client, mob, target, title));
    }
    true
}

/// Item clicks that holo floors, ready buttons and the holodeck conveyor take for themselves.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    _mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Turf>(target).is_some() {
        // `/turf/open/floor/holofloor/item_interaction`: `ITEM_INTERACT_BLOCKING`.
        return world
            .get::<DmTurfBuild>(target)
            .is_some_and(|b| is_under(&b.path, HOLOFLOOR));
    }
    if world.get::<ReadyButton>(target).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "The device is a solid button, there's nothing you can do with it!",
        );
        return true;
    }
    if path_of(world, target).is_some_and(|p| is_under(&p, HOLO_CONVEYOR)) {
        // `transferItemToLoc(tool, drop_location())`.
        let Some(&pos) = world.get::<TilePos>(target) else {
            return false;
        };
        if world.get::<crate::inventory::NoDrop>(held).is_some() {
            return false;
        }
        crate::inventory::put_down(world, held, pos);
        crate::construction::tile_changed(world, pos);
        return true;
    }
    false
}

/// `/obj/machinery/readybutton/attack_hand`.
fn ready_button(world: &mut World, client: Entity, button: Entity) {
    if world.get::<Unpowered>(button).is_some() {
        tell(world, client, Span::Warning, "This device is not powered!");
        return;
    }
    let Some(pos) = world.get::<TilePos>(button).copied() else {
        return;
    };
    let Some(area) = world.resource::<Areas>().area_at(pos) else {
        return;
    };
    if world
        .get::<ReadyButton>(button)
        .is_some_and(|b| b.event_started)
    {
        tell(world, client, Span::Warning, "The event has already begun!");
        return;
    }
    let ready = {
        let mut b = world.get_mut::<ReadyButton>(button).unwrap();
        b.ready = !b.ready;
        b.ready
    };
    crate::obj_damage::set_icon_state(world, button, if ready { "auth_on" } else { "auth_off" });
    let mut q = world.query::<(Entity, &ReadyButton, &TilePos)>();
    let areas = world.resource::<Areas>();
    let in_area: Vec<(Entity, bool)> = q
        .iter(world)
        .filter(|(_, _, p)| areas.area_at(**p) == Some(area))
        .map(|(e, b, _)| (e, b.ready))
        .collect();
    if in_area.iter().all(|&(_, r)| r) {
        let buttons: Vec<Entity> = in_area.iter().map(|&(e, _)| e).collect();
        begin_event(world, area, &buttons);
    }
}

/// `begin_event`: the holo windows of the area go, its mobs read "FIGHT!".
fn begin_event(world: &mut World, area: Entity, buttons: &[Entity]) {
    for &b in buttons {
        if let Some(mut b) = world.get_mut::<ReadyButton>(b) {
            b.event_started = true;
        }
    }
    let mut q = world.query_filtered::<(Entity, &TilePos), With<Hologram>>();
    let areas = world.resource::<Areas>();
    let windows: Vec<Entity> = q
        .iter(world)
        .filter(|(_, p)| areas.area_at(**p) == Some(area))
        .map(|(e, _)| e)
        .filter(|&e| path_of(world, e).is_some_and(|p| is_under(&p, HOLO_WINDOW)))
        .collect();
    for e in windows {
        derez(world, e);
    }
    let mut q = world.query_filtered::<(&crate::server::PlayerMob, &TilePos), With<Health>>();
    let areas = world.resource::<Areas>();
    let clients: Vec<Entity> = q
        .iter(world)
        .filter(|(_, p)| areas.area_at(**p) == Some(area))
        .map(|(m, _)| m.client)
        .collect();
    for client in clients {
        tell(world, client, Span::Danger, "FIGHT!");
    }
}

/// `/obj/item/toy/dodgeball/throw_impact` on what it hit.
pub fn dodgeball_hit(world: &mut World, ball: Entity, hit: Entity) {
    let is_ball = world
        .get::<DmItem>(ball)
        .is_some_and(|i| is_under(&i.path, DODGEBALL));
    if !is_ball || world.get::<crate::human::HumanLook>(hit).is_none() {
        return;
    }
    if let Some(&pos) = world.get::<TilePos>(hit) {
        playsound(world, PlaySound::new(pos, DODGEBALL_SOUND, 50.0, true));
    }
    crate::status_effects::adjust_stamina(world, hit, 10.0);
    if world.resource_mut::<CombatRng>().chance(5.0) {
        crate::status_effects::apply(world, hit, crate::status_effects::Incap::Paralyze, 6.0);
        if let Some(&pos) = world.get::<TilePos>(hit) {
            let name = crate::examine::name_of(world, hit);
            visible(
                world,
                pos,
                Span::Danger,
                &format!("{name} is knocked right off their feet!"),
            );
        }
    }
}

pub struct HolodeckWindow;

impl UiInterface for HolodeckWindow {
    const NAME: &'static str = HolodeckUi::NAME;
    type Data = HolodeckData;
    type StaticData = ();
}

fn entries(restricted: bool) -> Vec<HoloProgram> {
    PROGRAMS
        .iter()
        .filter(|p| p.restricted == restricted)
        .map(|p| HoloProgram {
            id: p.id.to_owned(),
            name: p.name.to_owned(),
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HolodeckAct {
    Load(String),
    Safety,
}

impl ServerUi for HolodeckWindow {
    type Source = &'static Holodeck;
    type UserParam = crate::silicon::interact::SiliconUsers<'static, 'static>;
    type Act = HolodeckAct;

    fn static_data(_: &Holodeck) {}

    fn data(
        h: &Holodeck,
        user: Entity,
        silicons: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> HolodeckData {
        HolodeckData {
            default_programs: entries(false),
            emagged: h.emagged,
            emag_programs: if h.emagged { entries(true) } else { Vec::new() },
            program: h.program.clone(),
            can_toggle_safety: silicons.contains(user),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<HolodeckAct, UiReject> {
        match action {
            "load_program" => params.text("id").map(HolodeckAct::Load),
            "safety" => Ok(HolodeckAct::Safety),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn window_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<HolodeckWindow>>>) {
    let acts: Vec<(Entity, Entity, Entity, HolodeckAct)> = cursor
        .read(world.resource::<Messages<UiActed<HolodeckWindow>>>())
        .map(|a| (a.source, a.client, a.actor, a.act.clone()))
        .collect();
    for (console, client, actor, act) in acts {
        ui_act(world, console, client, actor, act);
    }
}

/// `ui_act`: a program the console offers right now; the safeties for silicons or once emagged.
pub fn ui_act(
    world: &mut World,
    console: Entity,
    _client: Entity,
    actor: Entity,
    act: HolodeckAct,
) {
    let Some(h) = world.get::<Holodeck>(console).cloned() else {
        return;
    };
    match act {
        HolodeckAct::Load(id) => {
            let offered = find_program(&id).is_some_and(|p| !p.restricted || h.emagged);
            if offered {
                load_program(world, console, &id, false);
            }
        }
        HolodeckAct::Safety => {
            let silicon = crate::silicon::interact::has_silicon_access(world, actor);
            if !h.emagged && !silicon {
                return;
            }
            if h.emagged {
                emergency_shutdown(world, console);
            }
            nerf(world, console, h.emagged);
            world.get_mut::<Holodeck>(console).unwrap().emagged = !h.emagged;
            say(world, console, "Safeties reset. Restarting...");
            info!("Holodeck safeties toggled");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_split_into_default_and_restricted() {
        assert_eq!(PROGRAMS.len(), 29);
        assert_eq!(entries(true).len(), 8);
        assert_eq!(entries(false)[0].id, OFFLINE);
        assert!(find_program("holodeck_burntest").unwrap().restricted);
        assert!(!find_program("holodeck_beach").unwrap().restricted);
    }

    #[test]
    fn holo_floors_take_programs_and_pass_the_floor_check() {
        assert!(holodeck_compatible("/turf/open/floor/holofloor/grass"));
        assert!(holodeck_compatible("/turf/open/floor/engine"));
        assert!(!holodeck_compatible("/turf/open/floor/iron"));
        assert!(floor_ok("/turf/closed/wall"));
        assert!(floor_ok("/turf/open/floor/holofloor/plating"));
        assert!(!floor_ok("/turf/open/floor/engine"));
        assert!(!floor_ok("/turf/open/floor/plating"));
    }

    #[test]
    fn holo_floor_looks() {
        let mut max = |n: u32| n - 1;
        assert_eq!(
            holofloor_state("/turf/open/floor/holofloor/grass", 1, 1, &mut max).as_deref(),
            Some("grass3")
        );
        assert_eq!(
            holofloor_state("/turf/open/floor/holofloor/asteroid", 1, 1, &mut max).as_deref(),
            Some("asteroid12")
        );
        // `prob(15)` failed: the plain basalt.
        assert_eq!(
            holofloor_state("/turf/open/floor/holofloor/basalt", 1, 1, &mut max),
            None
        );
        // (2 + 5 * 3 + (1 + 1) * 7) % 15 + 1 = 31 % 15 + 1.
        assert_eq!(
            holofloor_state("/turf/open/floor/holofloor/hyperspace", 2, 3, &mut max).as_deref(),
            Some("speedspace_ns_2")
        );
        assert_eq!(holofloor_state(HOLOFLOOR, 1, 1, &mut max), None);
        assert_eq!(
            chapel_dir("/turf/open/floor/holofloor/chapel/top_left"),
            Some(ss13_shared::Dir::North)
        );
    }

    #[test]
    fn effect_spawns_are_runtime_types() {
        let types = ss13_mapload::runtime::runtime_types();
        let mut all = vec![
            HOLO_DECK,
            HOLO_CARP,
            HOLO_BEE,
            HOLO_MONKEY,
            HOLO_PENGUIN,
            HOLO_PENGUIN_SHAMEBRERO,
            HOLO_PENGUIN_BABY,
            HOLO_CRAB_JON,
        ];
        all.extend(PET_ALWAYS);
        all.extend(PET_DOGS);
        all.extend(PET_CATS);
        all.extend(MANUALS);
        for path in all {
            assert!(types.contains(path), "{path}");
        }
    }

    #[test]
    fn window_acts_parse() {
        let p = [("id".to_owned(), "holodeck_beach".to_owned())];
        assert_eq!(
            HolodeckWindow::parse_act("load_program", &UiParams::new(&p)).unwrap(),
            HolodeckAct::Load("holodeck_beach".into())
        );
        assert!(HolodeckWindow::parse_act("load_program", &UiParams::new(&[])).is_err());
        assert_eq!(
            HolodeckWindow::parse_act("safety", &UiParams::new(&[])).unwrap(),
            HolodeckAct::Safety
        );
    }
}
