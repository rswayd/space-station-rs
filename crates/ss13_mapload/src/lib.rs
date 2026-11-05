//! Server-side station loading.
//!
//! Two steps:
//! 1. Offline, `ss13-mapload convert` turns `dm_types.json` (from `ss13_dm_extract`) into
//!    `ss13_proto` prototypes: one per DM type the maps use (plus ancestors), id = DM path,
//!    parent = DM parent, carrying [`DmName`] and [`DmAtom`] with only the vars the type declares.
//! 2. At startup, [`MapLoadPlugin`] reads `.dmm` files, creates one `ss13_world` z-level per map
//!    z, and spawns turfs and movables from those prototypes, with `.dmm` var edits applied as
//!    prototype overrides.
//!
//! Icons stay strings in prototypes and are resolved against the current asset manifest at load,
//! once per distinct (type, var edits): regenerating assets never leaves stale ids in prototypes.

pub mod antag;
pub mod atmos;
pub mod atmos_handbook;
pub mod atmos_machine;
pub mod audit;
pub mod barsign;
pub mod border;
pub mod bots;
pub mod cable;
pub mod cargo;
pub mod circuit;
pub mod clothing;
pub mod construct;
pub mod contents;
pub mod convert;
pub mod dispenser;
pub mod disposal;
pub mod dock;
pub mod door;
pub mod drinks;
pub mod fidelity;
pub mod food;
pub mod gags;
pub mod guns;
pub mod item;
pub mod jobs;
pub mod light;
pub mod load;
pub mod look;
pub mod mecha;
pub mod medical;
pub mod mining;
pub mod mobs;
pub mod modsuit;
pub mod plants;
pub mod power;
pub mod radio;
pub mod reagents;
pub mod research;
pub mod runtime;
pub mod security;
pub mod service;
pub mod silicon;
pub mod smartfridge;
pub mod smooth;
pub mod sound;
pub mod spawner;
pub mod turf_damage;
pub mod undertile;
pub mod value;
pub mod vending;
pub mod wallmount;

use std::path::PathBuf;

use bevy::prelude::*;
use ss13_proto::ProtoAppExt;

pub use atmos::{DmArea, DmAtmosPass, DmTurfAtmos};
pub use atmos_machine::DmAtmosDevice;
pub use barsign::DmBarsign;
pub use border::DmBorder;
pub use bots::{BOT_RUNTIME_TYPES, DmNavBeacon};
pub use cable::DmCable;
pub use circuit::{CircuitPart, DmCircuit, DmMachine, FRAME_RUNTIME_TYPES};
pub use clothing::{DmClothing, DmTank};
pub use construct::{
    CONSTRUCTION_TYPES, DmObj, DmRadInsulation, DmStack, DmTable, DmTool, DmTurfBuild,
};
pub use contents::{DmCloset, DmContents, DmLockbox, DmStorage, DmWeight, Fill, PickArm};
pub use dispenser::DmChemDispenser;
pub use disposal::{DISPOSAL_RUNTIME_TYPES, DmMailSort};
pub use dock::{DmDockingPort, ShuttleTemplates};
pub use door::{DmAirlockHelper, DmDoor, DoorKind};
pub use drinks::DmGlass;
pub use fidelity::{FidelityReport, IconIndex, MapSeed, tile_changed};
pub use food::{DmFood, Processing, Taste};
pub use guns::{
    CasingData, DmArmor, DmCasing, DmGun, DmMagazine, DmTurret, GunSounds, MagazineData,
    ProjectileData,
};
pub use item::DmItem;
pub use light::{DmLight, OpticsPlugin};
pub use load::{
    Areas, Category, DEFAULT_AREA, DEFAULT_TURF, IconResolution, MapLoadReport, Template,
    fold_color, load_maps, read_manifest,
};
pub use look::{DmLook, DmSprite};
pub use medical::{DmDefib, DmMedical, DmWounding, MEDICAL_RUNTIME_TYPES};
pub use mining::{DmMineral, DmOrderConsole, DmOre, DmOreRedemption, OrderItem};
pub use mobs::DmMob;
pub use power::DmPower;
pub use radio::DmRadio;
pub use reagents::DmReagents;
pub use security::{DmBaton, DmHandcuffs};
pub use service::DmService;
pub use silicon::{DmCamera, DmCameraConsole};
pub use smartfridge::DmSmartFridge;
pub use smooth::DmSmooth;
pub use spawner::{DmSpawner, LootEntry};
pub use undertile::{DmTurf, DmUndertile, HiddenUnderTile};
pub use vending::{DmPrice, DmVending, DmVendingRefill, VendCategory, VendProduct};
pub use wallmount::{DmWallMount, WALLMOUNT_RUNTIME_TYPES};

/// BYOND `name` and `desc`. Text macros (`\improper`, ...) are kept verbatim for the text layer.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmName {
    pub name: String,
    pub desc: String,
}

/// The static DM vars a spawned entity was built from (type defaults + map var edits).
/// Prototype data, not live state: the loader derives `Appearance`, `Facing`, `Density` and
/// [`Opaque`] from it once per template, and systems change those, never this.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
pub struct DmAtom {
    /// `.dmi` path relative to the source repo, or a rendered GAGS icon (`ss13_assets::gags`);
    /// empty = no icon.
    pub icon: String,
    /// Empty = BYOND's default (unnamed) state.
    pub icon_state: String,
    pub layer: f32,
    pub plane: i16,
    pub pixel_x: i16,
    pub pixel_y: i16,
    /// RGBA; `None` = untinted. Folded with `alpha` into `Appearance::color`.
    pub color: Option<[u8; 4]>,
    pub alpha: u8,
    /// BYOND direction bits.
    pub dir: u8,
    pub density: bool,
    pub opacity: bool,
    /// BYOND `invisibility`: above 0 hides the atom from mobs without matching `see_invisible`
    /// (landmarks and spawners use `INVISIBILITY_ABSTRACT`).
    pub invisibility: u8,
    /// BYOND `mouse_opacity` (0 transparent, 1 icon, 2 opaque).
    pub mouse_opacity: u8,
}

impl Default for DmAtom {
    /// BYOND `/atom` builtin defaults.
    fn default() -> Self {
        Self {
            icon: String::new(),
            icon_state: String::new(),
            layer: 0.0,
            plane: 0,
            pixel_x: 0,
            pixel_y: 0,
            color: None,
            alpha: 255,
            dir: 2,
            density: false,
            opacity: false,
            invisibility: 0,
            mouse_opacity: 1,
        }
    }
}

/// Types the game spawns by itself (the player mob and the assistant's kit in `ss13_game`), so
/// `ss13-mapload convert` emits them even when no converted map uses them.
pub const RUNTIME_TYPES: [&str; 4] = [
    "/mob/living/carbon/human",
    "/obj/item/card/id/advanced",
    "/obj/item/clothing/under/color/grey",
    "/obj/item/clothing/shoes/sneakers/black",
];

/// The assistant's backpack and the job box in it (`/datum/outfit/job`: `back`, `box`). Apart
/// from [`RUNTIME_TYPES`] so fixtures without containers still convert; players spawn without
/// them when their prototypes are missing.
pub const RUNTIME_CONTAINER_TYPES: [&str; 2] = [
    "/obj/item/storage/backpack",
    "/obj/item/storage/box/survival",
];

/// tg `/obj/effect/decal/cleanable`.
pub const CLEANABLE_ROOT: &str = "/obj/effect/decal/cleanable";

/// The type path of a cleanable decal (blood, dirt, vomit), which has no other component
/// naming its type; inserted by [`load::Template::spawn`].
#[derive(Component, Clone, Debug, PartialEq)]
pub struct DmCleanable {
    pub path: String,
}

/// Blocks vision (BYOND `opacity = TRUE`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Opaque;

/// The tile's turf. Exactly one entity per tile has it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Turf;

/// A BYOND area: one entity per area type, not per tile; tiles map to it through [`Areas`].
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Area;

/// Loads maps at startup (in `PreStartup`, so every `Startup` system sees the station).
///
/// Needs `ss13_world::WorldPlugin` and `ss13_proto::ProtoPlugin` loading
/// [`Self::prototypes_dir`] (the output of `ss13-mapload convert`).
#[derive(Resource, Clone, Debug)]
pub struct MapLoadPlugin {
    /// BYOND source checkout the map paths are relative to.
    pub source: PathBuf,
    /// Map names (`MetaStation` = `_maps/map_files/MetaStation/MetaStation.dmm`) or `.dmm` paths;
    /// loaded in order onto consecutive z-levels.
    pub maps: Vec<String>,
    /// Generated assets: `manifest.postcard` and `prototypes/`.
    pub assets: PathBuf,
}

impl MapLoadPlugin {
    /// `SS13_MAP` (comma-separated names or paths; unset = no map), `SS13_SOURCE`
    /// (default `../BandaStation`), `SS13_ASSETS` (default `assets/generated`).
    pub fn from_env() -> Option<Self> {
        let maps: Vec<String> = std::env::var("SS13_MAP")
            .ok()?
            .split(',')
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(str::to_owned)
            .collect();
        let var = |name: &str, default: &str| {
            PathBuf::from(std::env::var(name).unwrap_or_else(|_| default.to_owned()))
        };
        (!maps.is_empty()).then(|| Self {
            source: var("SS13_SOURCE", env!("SS13_SOURCE")),
            maps,
            assets: var("SS13_ASSETS", "assets/generated"),
        })
    }

    pub fn map_paths(&self) -> Vec<PathBuf> {
        self.maps
            .iter()
            .map(|m| {
                if m.ends_with(".dmm") {
                    self.source.join(m)
                } else {
                    self.source.join(format!("_maps/map_files/{m}/{m}.dmm"))
                }
            })
            .collect()
    }

    pub fn prototypes_dir(&self) -> PathBuf {
        self.assets.join("prototypes")
    }
}

/// Registers the prototype components the converter emits. [`MapLoadPlugin`] adds it; add it
/// alone to load the generated prototypes without loading a map (tests, tools).
pub struct DmProtoPlugin;

impl Plugin for DmProtoPlugin {
    fn build(&self, app: &mut App) {
        app.register_proto_component::<DmName>("dm_name")
            .register_proto_component::<DmAtom>("dm_atom")
            .register_proto_component::<DmLight>("dm_light")
            .register_proto_component::<DmDoor>("dm_door")
            .register_proto_component::<DmAirlockHelper>("dm_airlock_helper")
            .register_proto_component::<DmSmooth>("dm_smooth")
            .register_proto_component::<DmTurf>("dm_turf")
            .register_proto_component::<DmRadInsulation>(construct::RAD_COMPONENT)
            .register_proto_component::<DmUndertile>("dm_undertile")
            .register_proto_component::<DmSpawner>("dm_spawner")
            .register_proto_component::<DmItem>("dm_item")
            .register_proto_component::<DmVending>("dm_vending")
            .register_proto_component::<DmVendingRefill>("dm_vending_refill")
            .register_proto_component::<DmPrice>("dm_price")
            .register_proto_component::<DmBorder>("dm_border")
            .register_proto_component::<DmTurfAtmos>("dm_turf_atmos")
            .register_proto_component::<DmAtmosPass>("dm_atmos_pass")
            .register_proto_component::<DmArea>("dm_area")
            .register_proto_component::<turf_damage::DmTurfDamage>("dm_turf_damage")
            .register_proto_component::<DmAtmosDevice>("dm_atmos_device")
            .register_proto_component::<DmCloset>("dm_closet")
            .register_proto_component::<DmLockbox>("dm_lockbox")
            .register_proto_component::<DmContents>("dm_contents")
            .register_proto_component::<DmStorage>("dm_storage")
            .register_proto_component::<DmWeight>("dm_weight")
            .register_proto_component::<DmPower>("dm_power")
            .register_proto_component::<DmRadio>("dm_radio")
            .register_proto_component::<DmCamera>("dm_camera")
            .register_proto_component::<DmCameraConsole>("dm_camera_console")
            .register_proto_component::<DmLook>("dm_look")
            .register_proto_component::<DmCable>("dm_cable")
            .register_proto_component::<DmClothing>("dm_clothing")
            .register_proto_component::<DmTank>("dm_tank")
            .register_proto_component::<DmBaton>("dm_baton")
            .register_proto_component::<DmHandcuffs>("dm_handcuffs")
            .register_proto_component::<DmGun>("dm_gun")
            .register_proto_component::<DmMagazine>("dm_magazine")
            .register_proto_component::<DmCasing>("dm_casing")
            .register_proto_component::<DmArmor>("dm_armor")
            .register_proto_component::<DmTurret>("dm_turret")
            .register_proto_component::<DmDockingPort>("dm_docking_port")
            .register_proto_component::<DmMailSort>("dm_mail_sort")
            .register_proto_component::<DmNavBeacon>("dm_navbeacon")
            .register_proto_component::<DmTool>(construct::TOOL_COMPONENT)
            .register_proto_component::<DmStack>(construct::STACK_COMPONENT)
            .register_proto_component::<DmTurfBuild>(construct::TURF_COMPONENT)
            .register_proto_component::<DmObj>(construct::OBJ_COMPONENT)
            .register_proto_component::<DmTable>(construct::TABLE_COMPONENT)
            .register_proto_component::<DmMedical>(medical::MEDICAL_COMPONENT)
            .register_proto_component::<DmWounding>(medical::WOUNDING_COMPONENT)
            .register_proto_component::<DmDefib>(medical::DEFIB_COMPONENT)
            .register_proto_component::<DmReagents>(reagents::CONTAINER_COMPONENT)
            .register_proto_component::<DmFood>(food::FOOD_COMPONENT)
            .register_proto_component::<DmChemDispenser>(dispenser::DISPENSER_COMPONENT)
            .register_proto_component::<DmSmartFridge>(smartfridge::FRIDGE_COMPONENT)
            .register_proto_component::<DmGlass>(drinks::GLASS_COMPONENT)
            .register_proto_component::<DmBarsign>(barsign::BARSIGN_COMPONENT)
            .register_proto_component::<DmService>(service::SERVICE_COMPONENT)
            .register_proto_component::<DmMob>("dm_mob")
            .register_proto_component::<DmWallMount>("dm_wallmount")
            .register_proto_component::<DmCircuit>("dm_circuit")
            .register_proto_component::<DmMachine>("dm_machine")
            .register_proto_component::<DmMineral>(mining::MINERAL_COMPONENT)
            .register_proto_component::<DmOre>(mining::ORE_COMPONENT)
            .register_proto_component::<DmOreRedemption>(mining::ORM_COMPONENT)
            .register_proto_component::<DmOrderConsole>(mining::ORDER_COMPONENT);
    }
}

impl Plugin for MapLoadPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((DmProtoPlugin, OpticsPlugin))
            .init_resource::<Areas>()
            .insert_resource(self.clone())
            .add_systems(PreStartup, load_on_startup);
        if !app.world().contains_resource::<MapSeed>() {
            app.insert_resource(MapSeed::from_env());
        }
    }
}

fn load_on_startup(world: &mut World) {
    let config = world.resource::<MapLoadPlugin>().clone();
    let seed = world.resource::<MapSeed>().0;
    let mut paths = config.map_paths();
    if let Some(templates) = world.get_resource::<ShuttleTemplates>() {
        paths.extend(templates.0.iter().cloned());
    }
    let result = read_manifest(&config.assets).and_then(|manifest| {
        let report = load_maps(world, &paths, &manifest)?;
        Ok((report, fidelity::apply(world, &manifest, seed)))
    });
    // The server has nothing to simulate without its station: fail at startup, loudly.
    let (report, fidelity) = result.unwrap_or_else(|e| panic!("map load failed: {e:#}"));
    info!("{report}");
    info!("{fidelity}");
    world.insert_resource(atmos::overlay_icons(world.resource::<IconIndex>()));
    world.insert_resource(atmos::hotspot_icons(world.resource::<IconIndex>()));
    world.insert_resource(report);
    world.insert_resource(fidelity);
}
