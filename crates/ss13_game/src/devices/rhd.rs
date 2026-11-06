//! The RCD's family (server): tg `RLD.dm`, `RTD.dm`, `RDD.dm`, `RPLD.dm` (on the
//! `/obj/item/construction` matter of [`super::rcd::Matter`]), `RWD.dm` and `RSF.dm` (their own
//! stores).
//!
//! - RLD (`/obj/item/construction/rld`, 200; mini 100): in hand a list of the modes (light
//!   fixture, glow stick launcher, deconstruct) and the colour picker. A wall gets a tube light
//!   (10 matter, 1 s) on its open side closest to the RLD, facing the wall; a floor a floor
//!   light (15, 1 s, one per tile); both take the chosen colour. Deconstruct takes a light down
//!   (10, 1.5 s). A glow stick (5) of the colour is thrown at the clicked atom.
//! - RTD (`/obj/item/construction/rtd`, 350): in hand the list of tg's `floor_designs` and the
//!   tile directions. A plating click lays the design's tile (`tile_cost`, `cost × 0.15 s`);
//!   a click on a supported floor copies its design and direction; a right click takes a
//!   supported floor off (`cost × 0.7`, `cost × 0.25 s`, its decals with it; over the level's
//!   bottom it leaves plating).
//! - RDD (`/obj/item/construction/rdd`, 60): the list of tg's `rdd_designs`; a click puts the
//!   decoration on the tile (1, 0.5 s; not on a blocked tile or a fourth one), a right click
//!   takes one away (0.5 s).
//! - RPLD (`/obj/item/construction/plumbing`, 200; the service one): the list of designs and
//!   the five ducting layers; a click on an open turf builds the machine (its cost, as many
//!   deciseconds; a dense one needs a free tile, one machine per layer and tile), a click on an
//!   unanchored machine of a design takes it back (2 s, its cost refunded up to the room left).
//! - RWD (`/obj/item/rwd`, 210 cable): used in hand, with the other hand free, it is wielded
//!   or let go, or its layer changes; wielded, each step onto plating without a cable of its
//!   layer lays one (joined into the power grid) and stray cable coils on the tile are taken
//!   in. A cable coil clicked on it (or it on one) loads it.
//! - RSF (`/obj/item/rsf`, 30; the cookie synthesizer 10, 10 s cooldown): matter cartridges
//!   give 10; in hand the list of what it makes; a click on a floor or a table makes it there
//!   (1 unit; tg uses one unit per item outside a cyborg).
// shortcut: lists instead of tg's radial menus and the RTD/RDD/RPLD tgui windows (no icons);
// the RLD tints its radial icons and the glow stick fluid by hue: glow sticks are not ported
// (→ ../tools: no light), it is a thrown coloured item; the RTD keeps no floor overlays
// (decals) when it copies a design; the RWD has no custom amount dispensing (right click in
// hand); the RPLD has no duct colours (ducts are not ported: right click does nothing, the
// machines themselves → ../chemistry/plumbing.md), no mouse wheel layer change; no cyborg
// (cell) versions; the RCL (pipe cleaners) is not ported.
// Cost: one pass over the moved mobs' hands per step for the RWD; the rest runs on clicks.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmCable, DmItem, DmName, DmStack, IconIndex, Turf};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::UiSystems;
use ss13_world::{Moved, SpatialIndex};

use super::rcd::Matter;
use crate::construction::{Job, path_of, spawn_obj, tile_changed};
use crate::health::Stat;
use crate::inventory::Inventory;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, Stack, do_after, tell, use_stack};

pub const RLD: &str = "/obj/item/construction/rld";
pub const RTD: &str = "/obj/item/construction/rtd";
pub const RDD: &str = "/obj/item/construction/rdd";
pub const RPLD: &str = "/obj/item/construction/plumbing";
const RPLD_SERVICE: &str = "/obj/item/construction/plumbing/service";
pub const RWD: &str = "/obj/item/rwd";
pub const RSF: &str = "/obj/item/rsf";
const COOKIESYNTH: &str = "/obj/item/rsf/cookiesynth";
const AMMO: &str = "/obj/item/rcd_ammo";
const CABLE_COIL: &str = "/obj/item/stack/cable_coil";
const LIGHT: &str = "/obj/machinery/light";
const FLOOR_LIGHT: &str = "/obj/machinery/light/floor";
const GLOWSTICK: &str = "/obj/item/flashlight/glowstick";
const DECORATION: &str = "/obj/structure/decoration";
const FLOOR: &str = "/turf/open/floor";
const PLATING: &str = "/turf/open/floor/plating";
const OPEN: &str = "/turf/open";
const SPACE: &str = "/turf/open/space";
const WALL: &str = "/turf/closed/wall";
const TABLE: &str = "/obj/structure/table";
const DECAL: &str = "/obj/effect/decal";
const BASETURF_BOTTOM: &str = "/turf/baseturf_bottom";
const CLICK: &str = "sound/machines/click.ogg";
const FLICKER: &str = "sound/effects/light_flicker.ogg";
const DECONSTRUCT_SOUND: &str = "sound/items/deconstruct.ogg";
const POP: &str = "sound/effects/pop.ogg";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";

// ---------------------------------------------------------------------------------------------
// Per-device state

/// RLD `mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RldMode {
    Light,
    Glow,
    Remove,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Rld {
    pub mode: RldMode,
    /// `color_choice`.
    pub color: [u8; 3],
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Rtd {
    /// Index into [`TILES`].
    pub design: usize,
    pub dir: Dir,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Rdd {
    /// Index into [`decorations`].
    pub design: usize,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Rpld {
    pub service: bool,
    /// Index into the device's design table.
    pub design: usize,
    /// `GLOB.plumbing_layers` value 1..=5 (3: the default layer).
    pub layer: u8,
}

/// The ducting layer of a machine an RPLD built.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlumbingLayer(pub u8);

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Rwd {
    pub amount: u32,
    pub max: u32,
    /// `cable_layer` bits (1, 2, 4).
    pub layer: u8,
    /// Wielded (tg `two_handed` `on_wield`).
    pub active: bool,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Rsf {
    pub matter: u32,
    pub max: u32,
    /// Index into the device's item table.
    pub design: usize,
    /// `cooldown`: elapsed seconds it is usable again at.
    pub ready_at: f64,
}

// ---------------------------------------------------------------------------------------------
// Tables

/// `GLOB.floor_designs`: category, name, tile type, `tile_cost`.
pub const TILES: &[(&str, &str, &str, u32)] = &[
    ("Standard", "Base", "/obj/item/stack/tile/iron", 4),
    ("Standard", "Small", "/obj/item/stack/tile/iron/small", 3),
    ("Standard", "Large", "/obj/item/stack/tile/iron/large", 7),
    (
        "Standard",
        "Diagonal",
        "/obj/item/stack/tile/iron/diagonal",
        5,
    ),
    ("Standard", "Edge", "/obj/item/stack/tile/iron/edge", 5),
    ("Standard", "Half", "/obj/item/stack/tile/iron/half", 5),
    ("Standard", "Corner", "/obj/item/stack/tile/iron/corner", 5),
    (
        "Standard",
        "Textured",
        "/obj/item/stack/tile/iron/textured",
        5,
    ),
    (
        "Standard",
        "Textured Edge",
        "/obj/item/stack/tile/iron/textured_edge",
        6,
    ),
    (
        "Standard",
        "Textured Half",
        "/obj/item/stack/tile/iron/textured_half",
        6,
    ),
    (
        "Standard",
        "Textured Corner",
        "/obj/item/stack/tile/iron/textured_corner",
        6,
    ),
    (
        "Standard",
        "Textured Large",
        "/obj/item/stack/tile/iron/textured_large",
        6,
    ),
    (
        "Translucent",
        "Smooth",
        "/obj/item/stack/tile/iron/smooth",
        4,
    ),
    (
        "Translucent",
        "Smooth Edge",
        "/obj/item/stack/tile/iron/smooth_edge",
        4,
    ),
    (
        "Translucent",
        "Smooth Half",
        "/obj/item/stack/tile/iron/smooth_half",
        4,
    ),
    (
        "Translucent",
        "Smooth Corner",
        "/obj/item/stack/tile/iron/smooth_corner",
        4,
    ),
    (
        "Translucent",
        "Smooth Large",
        "/obj/item/stack/tile/iron/smooth_large",
        7,
    ),
    (
        "Translucent",
        "Freezer",
        "/obj/item/stack/tile/iron/freezer",
        5,
    ),
    (
        "Translucent",
        "Showroom",
        "/obj/item/stack/tile/iron/showroomfloor",
        5,
    ),
    ("Translucent", "Glass", "/obj/item/stack/tile/glass", 5),
    (
        "Translucent",
        "Reinforced Glass",
        "/obj/item/stack/tile/rglass",
        10,
    ),
    (
        "Circuit",
        "Recharge",
        "/obj/item/stack/tile/iron/recharge_floor",
        5,
    ),
    (
        "Circuit",
        "Solar Panel",
        "/obj/item/stack/tile/iron/solarpanel",
        5,
    ),
    ("Circuit", "Blue Circuit", "/obj/item/stack/tile/circuit", 5),
    (
        "Circuit",
        "Green Circuit",
        "/obj/item/stack/tile/circuit/green",
        5,
    ),
    (
        "Circuit",
        "Green Circuit Anim",
        "/obj/item/stack/tile/circuit/green/anim",
        5,
    ),
    (
        "Circuit",
        "Red Circuit",
        "/obj/item/stack/tile/circuit/red",
        5,
    ),
    (
        "Circuit",
        "Red Circuit Anim",
        "/obj/item/stack/tile/circuit/red/anim",
        5,
    ),
    ("Dark Colored", "Base", "/obj/item/stack/tile/iron/dark", 4),
    (
        "Dark Colored",
        "Smooth Edge",
        "/obj/item/stack/tile/iron/dark/smooth_edge",
        4,
    ),
    (
        "Dark Colored",
        "Smooth Half",
        "/obj/item/stack/tile/iron/dark/smooth_half",
        4,
    ),
    (
        "Dark Colored",
        "Smooth Corner",
        "/obj/item/stack/tile/iron/dark/smooth_corner",
        4,
    ),
    (
        "Dark Colored",
        "Smooth Large",
        "/obj/item/stack/tile/iron/dark/smooth_large",
        7,
    ),
    (
        "Dark Colored",
        "Small",
        "/obj/item/stack/tile/iron/dark/small",
        4,
    ),
    (
        "Dark Colored",
        "Diagonal",
        "/obj/item/stack/tile/iron/dark/diagonal",
        4,
    ),
    (
        "Dark Colored",
        "Herringbone",
        "/obj/item/stack/tile/iron/dark/herringbone",
        4,
    ),
    (
        "Dark Colored",
        "Half Dark",
        "/obj/item/stack/tile/iron/dark_side",
        4,
    ),
    (
        "Dark Colored",
        "Dark Corner",
        "/obj/item/stack/tile/iron/dark_corner",
        4,
    ),
    (
        "White Colored",
        "Base",
        "/obj/item/stack/tile/iron/white",
        5,
    ),
    (
        "White Colored",
        "Smooth Edge",
        "/obj/item/stack/tile/iron/white/smooth_edge",
        5,
    ),
    (
        "White Colored",
        "Smooth Half",
        "/obj/item/stack/tile/iron/white/smooth_half",
        5,
    ),
    (
        "White Colored",
        "Smooth Corner",
        "/obj/item/stack/tile/iron/white/smooth_corner",
        5,
    ),
    (
        "White Colored",
        "Smooth Large",
        "/obj/item/stack/tile/iron/white/smooth_large",
        7,
    ),
    (
        "White Colored",
        "Small",
        "/obj/item/stack/tile/iron/white/small",
        5,
    ),
    (
        "White Colored",
        "Diagonal",
        "/obj/item/stack/tile/iron/white/diagonal",
        5,
    ),
    (
        "White Colored",
        "Herringbone",
        "/obj/item/stack/tile/iron/white/herringbone",
        5,
    ),
    (
        "White Colored",
        "Half White",
        "/obj/item/stack/tile/iron/white_side",
        5,
    ),
    (
        "White Colored",
        "White Corner",
        "/obj/item/stack/tile/iron/white_corner",
        5,
    ),
    (
        "Textured",
        "Textured White",
        "/obj/item/stack/tile/iron/white/textured",
        5,
    ),
    (
        "Textured",
        "Textured White Edge",
        "/obj/item/stack/tile/iron/white/textured_edge",
        5,
    ),
    (
        "Textured",
        "Textured White Half",
        "/obj/item/stack/tile/iron/white/textured_half",
        5,
    ),
    (
        "Textured",
        "Textured White Corner",
        "/obj/item/stack/tile/iron/white/textured_corner",
        5,
    ),
    (
        "Textured",
        "Textured White Large",
        "/obj/item/stack/tile/iron/white/textured_large",
        7,
    ),
    (
        "Textured",
        "Textured Dark",
        "/obj/item/stack/tile/iron/dark/textured",
        5,
    ),
    (
        "Textured",
        "Textured Dark Edge",
        "/obj/item/stack/tile/iron/dark/textured_edge",
        5,
    ),
    (
        "Textured",
        "Textured Dark Half",
        "/obj/item/stack/tile/iron/dark/textured_half",
        5,
    ),
    (
        "Textured",
        "Textured Dark Corner",
        "/obj/item/stack/tile/iron/dark/textured_corner",
        5,
    ),
    (
        "Textured",
        "Textured Dark Large",
        "/obj/item/stack/tile/iron/dark/textured_large",
        7,
    ),
    ("Room", "Kitchen", "/obj/item/stack/tile/iron/kitchen", 4),
    (
        "Room",
        "Kitchen Small",
        "/obj/item/stack/tile/iron/kitchen/small",
        4,
    ),
    (
        "Room",
        "Diagonal Kitchen",
        "/obj/item/stack/tile/iron/kitchen/diagonal",
        4,
    ),
    ("Room", "Chapel", "/obj/item/stack/tile/iron/chapel", 4),
    (
        "Room",
        "Cafeteria",
        "/obj/item/stack/tile/iron/cafeteria",
        4,
    ),
    ("Room", "Grimy", "/obj/item/stack/tile/iron/grimy", 5),
    ("Room", "Sepia", "/obj/item/stack/tile/iron/sepia", 5),
    (
        "Room",
        "Herringbone",
        "/obj/item/stack/tile/iron/kitchen/herringbone",
        5,
    ),
    (
        "Pattern",
        "Terracotta",
        "/obj/item/stack/tile/iron/terracotta",
        5,
    ),
    (
        "Pattern",
        "Small",
        "/obj/item/stack/tile/iron/terracotta/small",
        5,
    ),
    (
        "Pattern",
        "Diagonal",
        "/obj/item/stack/tile/iron/terracotta/diagonal",
        5,
    ),
    (
        "Pattern",
        "Herrigone",
        "/obj/item/stack/tile/iron/terracotta/herringbone",
        5,
    ),
    (
        "Pattern",
        "Checkered",
        "/obj/item/stack/tile/iron/checker",
        5,
    ),
    (
        "Pattern",
        "Herringbone",
        "/obj/item/stack/tile/iron/herringbone",
        5,
    ),
];

/// `GLOB.rdd_designs` compressed: category, base type, numbered variants (`/first` …); each
/// base also has `/style_random`.
const DECORATION_BASES: &[(&str, &str, usize)] = &[
    ("Grasses", "grass", 3),
    ("Grasses", "grass/brown", 3),
    ("Grasses", "jungle_grass", 5),
    ("Grasses", "jungle_grass/b", 5),
    ("Bushes", "bush", 4),
    ("Bushes", "bush/reed", 4),
    ("Bushes", "bush/leafy", 3),
    ("Bushes", "bush/pale", 4),
    ("Bushes", "bush/stalky", 3),
    ("Bushes", "bush/grassy", 4),
    ("Bushes", "bush/sparsegrass", 3),
    ("Bushes", "bush/fullgrass", 3),
    ("Bushes", "bush/ferny", 3),
    ("Bushes", "bush/sunny", 3),
    ("Bushes", "bush/generic", 4),
    ("Bushes", "bush/pointy", 4),
    ("Bushes", "bush/lavendergrass", 4),
    ("Flowers", "bush/flowers_yw", 3),
    ("Flowers", "bush/flowers_br", 3),
    ("Flowers", "bush/flowers_pp", 3),
    ("Snow", "bush/snow", 6),
    ("Jungle", "bush/jungle", 3),
    ("Jungle", "bush/jungle/b", 3),
    ("Jungle", "bush/jungle/c", 3),
    ("Jungle", "bush/large", 3),
    ("Rocks", "rock", 4),
    ("Rocks", "rock/pile", 3),
    ("Rocks", "rock/pile/jungle", 5),
    ("Rocks", "rock/pile/jungle/large", 3),
    ("Rocks", "rock/volcano", 5),
    ("Misc", "misc/glowgrowth", 3),
];

const VARIANT_NAMES: [&str; 6] = ["first", "second", "third", "fourth", "fifth", "sixth"];

/// The decoration designs in tg's order: category, type, label suffix.
pub fn decorations() -> Vec<(&'static str, String, String)> {
    let mut out = Vec::new();
    for &(category, base, count) in DECORATION_BASES {
        for (i, variant) in VARIANT_NAMES.iter().take(count).enumerate() {
            let suffix = match i {
                0 => String::new(),
                1 => " (Alt)".to_owned(),
                n => format!(" (Alt {n})"),
            };
            out.push((category, format!("{DECORATION}/{base}/{variant}"), suffix));
        }
        out.push((
            category,
            format!("{DECORATION}/{base}/style_random"),
            " (Random)".to_owned(),
        ));
    }
    out
}

/// `general_design_types`: category, machine, cost.
pub const PLUMBING: &[(&str, &str, u32)] = &[
    ("Synthesizers", "/obj/machinery/plumbing/synthesizer", 15),
    (
        "Synthesizers",
        "/obj/machinery/plumbing/reaction_chamber/chem",
        15,
    ),
    (
        "Synthesizers",
        "/obj/machinery/plumbing/grinder_chemical",
        30,
    ),
    ("Synthesizers", "/obj/machinery/plumbing/fermenter", 30),
    ("Synthesizers", "/obj/machinery/plumbing/liquid_pump", 35),
    ("Synthesizers", "/obj/machinery/plumbing/disposer", 10),
    ("Distributors", "/obj/machinery/duct", 1),
    ("Distributors", "/obj/machinery/plumbing/layer_manifold", 5),
    ("Distributors", "/obj/machinery/plumbing/input", 5),
    ("Distributors", "/obj/machinery/plumbing/filter", 5),
    ("Distributors", "/obj/machinery/plumbing/splitter", 5),
    ("Distributors", "/obj/machinery/plumbing/sender", 20),
    ("Distributors", "/obj/machinery/plumbing/output", 5),
    ("Storage", "/obj/machinery/plumbing/tank", 20),
    ("Storage", "/obj/machinery/plumbing/acclimator", 10),
    ("Storage", "/obj/machinery/plumbing/buffer", 10),
    ("Storage", "/obj/machinery/plumbing/bottler", 50),
    ("Storage", "/obj/machinery/plumbing/pill_press", 20),
    ("Storage", "/obj/machinery/iv_drip/plumbing", 20),
];

/// `service_design_types`.
pub const PLUMBING_SERVICE: &[(&str, &str, u32)] = &[
    (
        "Synthesizers",
        "/obj/machinery/plumbing/synthesizer/soda",
        15,
    ),
    (
        "Synthesizers",
        "/obj/machinery/plumbing/synthesizer/beer",
        15,
    ),
    (
        "Synthesizers",
        "/obj/machinery/plumbing/reaction_chamber",
        15,
    ),
    ("Synthesizers", "/obj/machinery/plumbing/buffer", 10),
    ("Synthesizers", "/obj/machinery/plumbing/fermenter", 30),
    (
        "Synthesizers",
        "/obj/machinery/plumbing/grinder_chemical",
        30,
    ),
    ("Synthesizers", "/obj/machinery/plumbing/disposer", 10),
    ("Distributors", "/obj/machinery/duct", 1),
    ("Distributors", "/obj/machinery/plumbing/layer_manifold", 5),
    ("Distributors", "/obj/machinery/plumbing/input", 5),
    ("Distributors", "/obj/machinery/plumbing/filter", 5),
    ("Distributors", "/obj/machinery/plumbing/splitter", 5),
    ("Distributors", "/obj/machinery/plumbing/output/tap", 5),
    ("Distributors", "/obj/machinery/plumbing/sender", 20),
];

fn plumbing_table(service: bool) -> &'static [(&'static str, &'static str, u32)] {
    if service { PLUMBING_SERVICE } else { PLUMBING }
}

/// `cost_by_item` of the RSF.
pub const RSF_ITEMS: &[&str] = &[
    "/obj/item/reagent_containers/cup/glass/drinkingglass",
    "/obj/item/reagent_containers/cup/glass/sillycup",
    "/obj/item/plate",
    "/obj/item/reagent_containers/cup/bowl",
    "/obj/item/kitchen/fork/plastic",
    "/obj/item/knife/plastic",
    "/obj/item/kitchen/spoon/plastic",
    "/obj/item/food/seaweedsheet",
    "/obj/item/storage/dice",
    "/obj/item/toy/cards/deck",
    "/obj/item/paper",
    "/obj/item/pen",
    "/obj/item/cigarette",
];
const COOKIE_ITEMS: &[&str] = &["/obj/item/food/cookie"];

fn rsf_table(cookies: bool) -> &'static [&'static str] {
    if cookies { COOKIE_ITEMS } else { RSF_ITEMS }
}

/// RLD costs (`LIGHT_TUBE_COST`, `FLOOR_LIGHT_COST`, `GLOW_STICK_COST`, `DECONSTRUCT_COST`) and
/// delays in deciseconds (`BUILD_DELAY`, `REMOVE_DELAY`).
const TUBE_COST: u32 = 10;
const FLOOR_LIGHT_COST: u32 = 15;
const GLOW_COST: u32 = 5;
const RLD_REMOVE_COST: u32 = 10;
const RLD_BUILD_DELAY: f32 = 10.0;
const RLD_REMOVE_DELAY: f32 = 15.0;
const RDD_DELAY: f32 = 5.0;
const RPLD_TAKE_DELAY: f32 = 20.0;
const RWD_MAX: u32 = 210;
const COOKIE_COOLDOWN: f64 = 10.0;

/// `CONSTRUCTION_TIME(cost)` and `DECONSTRUCTION_TIME(cost)`, deciseconds.
pub fn tile_delays(cost: u32) -> (f32, f32) {
    (cost as f32 * 1.5, cost as f32 * 2.5)
}

/// `cost * 0.7` of an RTD deconstruction, rounded up (matter is whole units here).
pub fn tile_removal_cost(cost: u32) -> u32 {
    (cost * 7).div_ceil(10)
}

/// The cable type of an RWD layer (`modify_cable`).
pub fn cable_type(layer: u8) -> &'static str {
    match layer {
        1 => "/obj/structure/cable/layer1",
        2 => "/obj/structure/cable",
        _ => "/obj/structure/cable/layer3",
    }
}

/// tg's wall light spot: the candidate closest (`CHEAP_HYPOTENUSE`) to `start`, the first of
/// equals.
pub fn closest(start: TilePos, candidates: &[TilePos]) -> Option<TilePos> {
    let dist = |p: &TilePos| {
        let (dx, dy) = ((p.x - start.x) as f32, (p.y - start.y) as f32);
        (dx * dx + dy * dy).sqrt()
    };
    let mut best: Option<(TilePos, f32)> = None;
    for c in candidates {
        let d = dist(c);
        if best.is_none_or(|(_, b)| d < b) {
            best = Some((*c, d));
        }
    }
    best.map(|(p, _)| p)
}

// ---------------------------------------------------------------------------------------------
// Plugin

/// The timed work of these devices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RhdJob {
    RldLight {
        cost: u32,
    },
    RldRemove,
    RtdLay {
        design: usize,
        cost: u32,
        effect: Option<Entity>,
    },
    RtdRemove {
        cost: u32,
        effect: Option<Entity>,
    },
    RddPlace {
        design: usize,
    },
    RddRemove,
    RpldBuild {
        design: usize,
        cost: u32,
    },
    RpldTake {
        cost: u32,
    },
}

/// The list open for a device: what each entry does.
#[derive(Component, Clone, Debug)]
struct RhdPrompt {
    item: Entity,
    options: Vec<Choice>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Choice {
    RldMode(RldMode),
    RldColor,
    Tile(usize),
    TileDir(Dir),
    Decoration(usize),
    Plumbing(usize),
    PlumbingLayer(u8),
    RwdWield,
    RwdLayer(u8),
    Rsf(usize),
}

/// The RLD's colour picker.
#[derive(Component, Clone, Copy, Debug)]
struct RldColorPrompt {
    item: Entity,
}

pub fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::AttackSelf, 1737, |w, c| {
        attack_self(w, c.client, c.mob, c.target)
    });
    // A cartridge onto the RSF or cable onto the RWD in the other hand.
    app.add_click_handler(ClickPhase::OtherHand, 457, |w, c| {
        c.held.is_some_and(|h| load(w, c.client, c.target, h))
    });
    app.add_click_handler(ClickPhase::HeldOnTarget, 1577, |w, c| {
        c.held.is_some_and(|h| {
            let actor = Actor {
                client: c.client,
                mob: c.mob,
                item: Some(h),
            };
            item_interaction(w, actor, h, c.target, c.modifiers.right)
        })
    });
    app.add_observer(attach).add_systems(
        Update,
        (
            answers
                .in_set(UiSystems::Handle)
                .after(ss13_tgui::prompt::PromptSystems::Answer),
            rwd_on_moved,
        ),
    );
}

/// `Initialize`: the devices' state however they appear.
fn attach(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let path = item.path.as_str();
    let mut e = commands.entity(add.entity);
    if is_under(path, RLD) {
        e.insert_if_new(Rld {
            mode: RldMode::Light,
            color: [255, 255, 255],
        });
    } else if is_under(path, RTD) {
        e.insert_if_new(Rtd {
            design: 0,
            dir: Dir::South,
        });
    } else if is_under(path, RDD) {
        e.insert_if_new(Rdd { design: 0 });
    } else if is_under(path, RPLD) {
        e.insert_if_new(Rpld {
            service: is_under(path, RPLD_SERVICE),
            design: 0,
            layer: 3,
        });
    } else if is_under(path, RWD) {
        let (amount, max) = if is_under(path, "/obj/item/rwd/admin") {
            (u32::MAX, u32::MAX)
        } else if is_under(path, "/obj/item/rwd/loaded") {
            (RWD_MAX, RWD_MAX)
        } else {
            (0, RWD_MAX)
        };
        e.insert_if_new(Rwd {
            amount,
            max,
            layer: 2,
            active: false,
        });
    } else if is_under(path, RSF) {
        let cookies = is_under(path, COOKIESYNTH);
        let max = if cookies { 10 } else { 30 };
        let matter = if is_under(path, "/obj/item/rsf/cyborg") {
            30
        } else {
            0
        };
        e.insert_if_new(Rsf {
            matter,
            max,
            design: 0,
            ready_at: 0.0,
        });
    }
}

fn item_path(world: &World, e: Entity) -> Option<String> {
    world.get::<DmItem>(e).map(|i| i.path.clone())
}

fn proto_name(world: &World, path: &str) -> String {
    world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(path))
        .and_then(|p| p.get::<DmName>())
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or("").to_owned())
}

/// The turf a tile type lays (`initial(tile_type.turf_type)`).
fn tile_turf(world: &World, tile: &str) -> Option<String> {
    world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(tile))
        .and_then(|p| p.get::<DmStack>())
        .map(|s| s.turf_type.clone())
        .filter(|t| !t.is_empty())
}

fn sound_at(world: &mut World, e: Entity, path: &'static str, volume: f32, vary: bool) {
    if let Some(pos) = crate::modpc::pda::turf_of(world, e) {
        playsound(world, PlaySound::new(pos, path, volume, vary));
    }
}

fn matter_of(world: &World, item: Entity) -> Option<Matter> {
    world.get::<Matter>(item).copied()
}

/// `useResource(amount)`: false (and the warning) when short.
fn take_matter(world: &mut World, client: Entity, item: Entity, cost: u32, dry: bool) -> bool {
    let Some(m) = matter_of(world, item) else {
        return false;
    };
    if !m.has(cost) {
        tell(world, client, Span::Warning, "Недостаточно материи!");
        return false;
    }
    if !dry && let Some(mut m) = world.get_mut::<Matter>(item) {
        m.take(cost);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// In hand

fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(path) = item_path(world, item) else {
        return false;
    };
    let mut items = Vec::new();
    let mut options = Vec::new();
    let mut add = |label: String, choice: Choice, selected: bool| {
        let mark = if selected { "► " } else { "" };
        items.push(format!("{mark}{label}"));
        options.push(choice);
    };
    let title;
    let mut message = String::new();
    if let Some(r) = world.get::<Rld>(item).copied() {
        title = "RLD";
        for (label, mode) in [
            ("Светильник", RldMode::Light),
            ("Светящаяся палочка", RldMode::Glow),
            ("Разобрать", RldMode::Remove),
        ] {
            add(label.to_owned(), Choice::RldMode(mode), r.mode == mode);
        }
        add("Выбрать цвет".to_owned(), Choice::RldColor, false);
    } else if let Some(r) = world.get::<Rtd>(item).copied() {
        title = "RTD";
        for (i, &(category, name, ..)) in TILES.iter().enumerate() {
            add(
                format!("{category}: {name}"),
                Choice::Tile(i),
                r.design == i,
            );
        }
        for (label, dir) in [
            ("Направление: север", Dir::North),
            ("Направление: юг", Dir::South),
            ("Направление: восток", Dir::East),
            ("Направление: запад", Dir::West),
        ] {
            add(label.to_owned(), Choice::TileDir(dir), r.dir == dir);
        }
    } else if let Some(r) = world.get::<Rdd>(item).copied() {
        title = "RDD";
        for (i, (category, design, suffix)) in decorations().into_iter().enumerate() {
            let name = proto_name(world, &design);
            add(
                format!("{category}: {name}{suffix}"),
                Choice::Decoration(i),
                r.design == i,
            );
        }
    } else if let Some(r) = world.get::<Rpld>(item).copied() {
        title = "RPLD";
        for (i, &(category, machine, cost)) in plumbing_table(r.service).iter().enumerate() {
            let name = proto_name(world, machine);
            add(
                format!("{category}: {name} ({cost})"),
                Choice::Plumbing(i),
                r.design == i,
            );
        }
        for layer in 1..=5 {
            add(
                format!("Слой {layer}"),
                Choice::PlumbingLayer(layer),
                r.layer == layer,
            );
        }
    } else if let Some(r) = world.get::<Rwd>(item).copied() {
        title = "RWD";
        let wield = if r.active {
            "Отпустить"
        } else {
            "Взять обеими руками"
        };
        add(wield.to_owned(), Choice::RwdWield, false);
        for (i, layer) in [1u8, 2, 4].into_iter().enumerate() {
            add(
                format!("Слой {}", i + 1),
                Choice::RwdLayer(layer),
                r.layer == layer,
            );
        }
        message = format!("Кабеля: {}", amount_text(r.amount, r.max));
    } else if let Some(r) = world.get::<Rsf>(item).copied() {
        title = "RSF";
        let cookies = is_under(&path, COOKIESYNTH);
        for (i, made) in rsf_table(cookies).iter().enumerate() {
            add(proto_name(world, made), Choice::Rsf(i), r.design == i);
        }
        sound_at(world, item, POP, 50.0, false);
    } else {
        return false;
    }
    if let Some(m) = matter_of(world, item) {
        message = format!("Материя: {}", amount_text(m.matter, m.max));
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            message,
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(RhdPrompt { item, options });
    true
}

fn amount_text(amount: u32, max: u32) -> String {
    if max == u32::MAX {
        "∞".to_owned()
    } else {
        format!("{amount}/{max}")
    }
}

/// `check_menu`: the device still in a hand of a conscious, able user.
fn can_use(world: &World, mob: Entity, item: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(item)))
        && world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

/// `item` in one of `mob`'s hands with the other hand empty (wielding).
fn other_hand_free(world: &World, mob: Entity, item: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|inv| inv.hands.contains(&Some(item)) && inv.hands.contains(&None))
}

fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        if let Some(RldColorPrompt { item }) = world.get::<RldColorPrompt>(prompt).copied() {
            if let Some(PromptAnswer::Color(hex)) = answer
                && can_use(world, mob, item)
                && let Some(color) = parse_hex(&hex)
                && let Some(mut rld) = world.get_mut::<Rld>(item)
            {
                rld.color = color;
            }
            continue;
        }
        let Some(RhdPrompt { item, options }) = world.get::<RhdPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        let Some(&choice) = options.get(index) else {
            continue;
        };
        if !can_use(world, mob, item) {
            continue;
        }
        choose(world, client, mob, item, choice);
    }
}

fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// `handle_ui_act` / the radial choice, re-checked against the device's own tables.
fn choose(world: &mut World, client: Entity, mob: Entity, item: Entity, choice: Choice) {
    sound_at(world, item, RATCHET, 20.0, true);
    match choice {
        Choice::RldMode(mode) => {
            if let Some(mut r) = world.get_mut::<Rld>(item) {
                r.mode = mode;
            }
        }
        Choice::RldColor => {
            let Some(r) = world.get::<Rld>(item).copied() else {
                return;
            };
            let [red, green, blue] = r.color;
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    mob,
                    "Выбор цвета",
                    "",
                    PromptKind::Color {
                        default: format!("#{red:02x}{green:02x}{blue:02x}"),
                        autofocus: true,
                    },
                ),
            );
            world.entity_mut(prompt).insert(RldColorPrompt { item });
            return;
        }
        Choice::Tile(i) if i < TILES.len() => {
            if let Some(mut r) = world.get_mut::<Rtd>(item) {
                r.design = i;
                r.dir = Dir::South;
            }
        }
        Choice::TileDir(dir) => {
            if let Some(mut r) = world.get_mut::<Rtd>(item) {
                r.dir = dir;
            }
        }
        Choice::Decoration(i) if i < decorations().len() => {
            if let Some(mut r) = world.get_mut::<Rdd>(item) {
                r.design = i;
            }
        }
        Choice::Plumbing(i) => {
            if let Some(mut r) = world.get_mut::<Rpld>(item)
                && i < plumbing_table(r.service).len()
            {
                r.design = i;
            }
        }
        Choice::PlumbingLayer(layer) if (1..=5).contains(&layer) => {
            if let Some(mut r) = world.get_mut::<Rpld>(item) {
                r.layer = layer;
            }
        }
        Choice::RwdWield => {
            let Some(r) = world.get::<Rwd>(item).copied() else {
                return;
            };
            if !r.active && !other_hand_free(world, mob, item) {
                tell(world, client, Span::Warning, "Нужна свободная вторая рука!");
                return;
            }
            world.get_mut::<Rwd>(item).expect("checked").active = !r.active;
        }
        Choice::RwdLayer(layer) if matches!(layer, 1 | 2 | 4) => {
            if let Some(mut r) = world.get_mut::<Rwd>(item) {
                r.layer = layer;
            }
        }
        Choice::Rsf(i) => {
            let cookies = item_path(world, item).is_some_and(|p| is_under(&p, COOKIESYNTH));
            if i < rsf_table(cookies).len()
                && let Some(mut r) = world.get_mut::<Rsf>(item)
            {
                r.design = i;
            }
        }
        _ => return,
    }
    tell(world, client, Span::Notice, "Режим изменён.");
}

// ---------------------------------------------------------------------------------------------
// Loading

/// A cartridge into an RSF, a cable coil into an RWD. True when handled.
fn load(world: &mut World, client: Entity, device: Entity, used: Entity) -> bool {
    let Some(path) = item_path(world, used) else {
        return false;
    };
    if let Some(r) = world.get::<Rsf>(device).copied() {
        if !is_under(&path, AMMO) {
            return false;
        }
        // tg reads `matter_by_item[tool.type]`: the large cartridge would give nothing (a tg
        // bug); every cartridge gives 10 here.
        if r.matter + 10 > r.max {
            tell(world, client, Span::Warning, "Больше не помещается!");
            return true;
        }
        world.despawn(used);
        let matter = r.matter + 10;
        world.get_mut::<Rsf>(device).expect("checked").matter = matter;
        sound_at(world, device, CLICK, 10.0, true);
        tell(
            world,
            client,
            Span::Notice,
            format!("В нём теперь {matter}/{} единиц.", r.max),
        );
        return true;
    }
    if world.get::<Rwd>(device).is_some() && is_under(&path, CABLE_COIL) {
        add_cable(world, Some(client), device, used);
        return true;
    }
    false
}

/// `add_cable`: as much of the coil as fits.
fn add_cable(world: &mut World, client: Option<Entity>, rwd: Entity, coil: Entity) {
    let Some(r) = world.get::<Rwd>(rwd).copied() else {
        return;
    };
    if r.amount >= r.max {
        if let Some(client) = client {
            tell(world, client, Span::Warning, "Устройство заполнено!");
        }
        return;
    }
    let have = world.get::<Stack>(coil).map_or(1, |s| s.amount);
    let insert = have.min(r.max - r.amount);
    if !use_stack(world, coil, insert) {
        return;
    }
    if let Some(mut r) = world.get_mut::<Rwd>(rwd) {
        r.amount = r.amount.saturating_add(insert).min(r.max);
    }
    if let Some(client) = client {
        tell(
            world,
            client,
            Span::Notice,
            format!("Вставлено кабеля: {insert}."),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Clicks

fn item_interaction(
    world: &mut World,
    actor: Actor,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    // A cartridge or coil onto a device lying around.
    if (world.get::<Rsf>(target).is_some() || world.get::<Rwd>(target).is_some())
        && world.get::<Rsf>(held).is_none()
        && world.get::<Rwd>(held).is_none()
    {
        return load(world, actor.client, target, held);
    }
    if world.get::<Rwd>(held).is_some() {
        if item_path(world, target).is_some_and(|p| is_under(&p, CABLE_COIL)) {
            add_cable(world, Some(actor.client), held, target);
            return true;
        }
        return false;
    }
    if world.get::<Rld>(held).is_some() {
        return rld_click(world, actor, held, target);
    }
    if world.get::<Rtd>(held).is_some() {
        return rtd_click(world, actor, held, target, right);
    }
    if world.get::<Rdd>(held).is_some() {
        return rdd_click(world, actor, held, target, right);
    }
    if world.get::<Rpld>(held).is_some() {
        return !right && rpld_click(world, actor, held, target);
    }
    if world.get::<Rsf>(held).is_some() {
        return !right && rsf_click(world, actor, held, target);
    }
    false
}

fn turf_of_target(world: &World, target: Entity) -> Option<(Entity, TilePos)> {
    let pos = *world.get::<TilePos>(target)?;
    let turf = crate::construction::turf_at(world, pos)?;
    Some((turf, pos))
}

fn has_on_tile(world: &World, pos: TilePos, root: &str) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| path_of(world, e).is_some_and(|p| is_under(&p, root)))
}

// --- RLD

fn rld_click(world: &mut World, actor: Actor, rld: Entity, target: Entity) -> bool {
    let Some(r) = world.get::<Rld>(rld).copied() else {
        return false;
    };
    let path = path_of(world, target).unwrap_or_default();
    let is_turf = world.get::<Turf>(target).is_some();
    match r.mode {
        RldMode::Remove => {
            if !is_under(&path, LIGHT) || world.get::<crate::power::LightFixture>(target).is_none()
            {
                return false;
            }
            if !take_matter(world, actor.client, rld, RLD_REMOVE_COST, true) {
                return true;
            }
            sound_at(world, rld, CLICK, 50.0, true);
            do_after(
                world,
                actor,
                target,
                RLD_REMOVE_DELAY,
                0,
                Job::Rhd(RhdJob::RldRemove),
            );
            true
        }
        RldMode::Light => {
            let wall = is_turf && is_under(&path, WALL);
            if !wall && !(is_turf && is_under(&path, FLOOR)) {
                return false;
            }
            let cost = if wall { TUBE_COST } else { FLOOR_LIGHT_COST };
            if !take_matter(world, actor.client, rld, cost, true) {
                return true;
            }
            sound_at(world, rld, CLICK, 50.0, true);
            sound_at(world, rld, FLICKER, 50.0, false);
            do_after(
                world,
                actor,
                target,
                RLD_BUILD_DELAY,
                0,
                Job::Rhd(RhdJob::RldLight { cost }),
            );
            true
        }
        RldMode::Glow => {
            let Some(&to) = world.get::<TilePos>(target) else {
                return false;
            };
            let Some(from) = crate::modpc::pda::turf_of(world, rld) else {
                return true;
            };
            if !take_matter(world, actor.client, rld, GLOW_COST, false) {
                return true;
            }
            if let Some(stick) = spawn_obj(world, GLOWSTICK, from) {
                let [red, green, blue] = r.color;
                if let Some(mut atom) = world.get_mut::<DmAtom>(stick) {
                    atom.color = Some([red, green, blue, 255]);
                }
                crate::throw::throw_at(world, stick, to, 9, 3.0);
            }
            true
        }
    }
}

/// The open side of `wall` for a tube light: no light there yet, space or an open turf
/// sharing air, the closest to `start`.
fn wall_light_spot(world: &World, wall: TilePos, start: TilePos) -> Option<(TilePos, Dir)> {
    let candidates: Vec<TilePos> = Dir::CARDINALS
        .iter()
        .map(|&d| wall.step(d))
        .filter(|&p| has_on_tile(world, p, LIGHT).is_none())
        .filter(|&p| {
            crate::construction::turf_at(world, p)
                .and_then(|t| path_of(world, t))
                .is_some_and(|t| is_under(&t, SPACE) || is_under(&t, OPEN))
        })
        .collect();
    let spot = closest(start, &candidates)?;
    // `get_dir(winner, interacting_with)`.
    let dir = Dir::CARDINALS.into_iter().find(|&d| spot.step(d) == wall)?;
    Some((spot, dir))
}

fn tint_light(world: &mut World, light: Entity, color: [u8; 3]) {
    if let Some(mut atom) = world.get_mut::<DmAtom>(light) {
        atom.color = Some([color[0], color[1], color[2], 255]);
    }
    if let Some(mut fixture) = world.get_mut::<crate::power::LightFixture>(light) {
        if let Some(l) = fixture.on_light.as_mut() {
            l.color = color;
        }
        if let Some(l) = fixture.template.as_mut() {
            l.color = color;
        }
    }
    crate::power::lighting::update(world, light);
}

fn rld_complete(world: &mut World, action: &DoAfter, rld: Entity, job: RhdJob) {
    let client = action.client;
    let target = action.target;
    match job {
        RhdJob::RldRemove => {
            if !take_matter(world, client, rld, RLD_REMOVE_COST, true) {
                return;
            }
            let Some(area) = world
                .get::<crate::power::LightFixture>(target)
                .map(|f| f.area)
            else {
                return;
            };
            if let Some(mut members) = world.get_mut::<crate::power::AreaMembers>(area) {
                members.lights.retain(|&l| l != target);
            }
            let pos = world.get::<TilePos>(target).copied();
            world.despawn(target);
            if let Some(pos) = pos {
                tile_changed(world, pos);
            }
            take_matter(world, client, rld, RLD_REMOVE_COST, false);
        }
        RhdJob::RldLight { cost } => {
            if !take_matter(world, client, rld, cost, true) {
                return;
            }
            let Some(r) = world.get::<Rld>(rld).copied() else {
                return;
            };
            let pos = action.target_pos;
            let path = path_of(world, target).unwrap_or_default();
            let light = if is_under(&path, WALL) {
                let start = crate::modpc::pda::turf_of(world, rld).unwrap_or(action.user_pos);
                let Some((spot, dir)) = wall_light_spot(world, pos, start) else {
                    tell(world, client, Span::Warning, "Некуда поставить!");
                    return;
                };
                crate::power::lighting::build_fixture(world, LIGHT, spot, Some(dir))
            } else if is_under(&path, FLOOR) {
                if has_on_tile(world, pos, FLOOR_LIGHT).is_some() {
                    return;
                }
                crate::power::lighting::build_fixture(world, FLOOR_LIGHT, pos, None)
            } else {
                None
            };
            let Some(light) = light else {
                return;
            };
            tint_light(world, light, r.color);
            take_matter(world, client, rld, cost, false);
        }
        _ => {}
    }
}

// --- RTD

/// The [`TILES`] design whose tile lays `turf`.
fn design_of_floor(world: &World, turf: &str) -> Option<usize> {
    TILES
        .iter()
        .position(|&(_, _, tile, _)| tile_turf(world, tile).as_deref() == Some(turf))
}

fn rtd_click(world: &mut World, actor: Actor, rtd: Entity, target: Entity, right: bool) -> bool {
    let Some(r) = world.get::<Rtd>(rtd).copied() else {
        return false;
    };
    if world.get::<Turf>(target).is_none() {
        return false;
    }
    let path = path_of(world, target).unwrap_or_default();
    if !is_under(&path, FLOOR) {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    let plating = is_under(&path, PLATING);
    if right {
        if plating {
            tell(world, actor.client, Span::Warning, "Нечего разбирать!");
            return true;
        }
        let Some(design) = design_of_floor(world, &path) else {
            tell(world, actor.client, Span::Warning, "Этот пол не разобрать!");
            return true;
        };
        let cost = tile_removal_cost(TILES[design].3);
        if !take_matter(world, actor.client, rtd, cost, true) {
            return true;
        }
        let delay = tile_delays(TILES[design].3).1;
        let effect = super::rcd::spawn_effect(world, pos, delay, true);
        sound_at(world, rtd, FLICKER, 50.0, false);
        if !do_after(
            world,
            actor,
            target,
            delay,
            0,
            Job::Rhd(RhdJob::RtdRemove { cost, effect }),
        ) && let Some(e) = effect
        {
            world.despawn(e);
        }
        return true;
    }
    if !plating {
        // Copy the floor's design.
        let Some(design) = design_of_floor(world, &path) else {
            tell(
                world,
                actor.client,
                Span::Warning,
                "Этот пол не поддерживается!",
            );
            return true;
        };
        let dir = world
            .get::<DmAtom>(target)
            .and_then(|a| Dir::from_bits(a.dir))
            .unwrap_or(Dir::South);
        if let Some(mut r) = world.get_mut::<Rtd>(rtd) {
            r.design = design;
            r.dir = dir;
        }
        let (category, name, ..) = TILES[design];
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("Плитка: {category}: {name}."),
        );
        return true;
    }
    let Some(&(_, _, _, cost)) = TILES.get(r.design) else {
        return true;
    };
    if !take_matter(world, actor.client, rtd, cost, true) {
        return true;
    }
    let delay = tile_delays(cost).0;
    let effect = super::rcd::spawn_effect(world, pos, delay, false);
    sound_at(world, rtd, FLICKER, 50.0, false);
    let job = RhdJob::RtdLay {
        design: r.design,
        cost,
        effect,
    };
    if !do_after(world, actor, target, delay, 0, Job::Rhd(job))
        && let Some(e) = effect
    {
        world.despawn(e);
    }
    true
}

fn rtd_complete(world: &mut World, action: &DoAfter, rtd: Entity, job: RhdJob) {
    let client = action.client;
    let pos = action.target_pos;
    let (effect, cost) = match job {
        RhdJob::RtdLay { effect, cost, .. } | RhdJob::RtdRemove { effect, cost } => (effect, cost),
        _ => return,
    };
    if let Some(e) = effect
        && world.get_entity(e).is_ok()
    {
        world.despawn(e);
    }
    let Some(turf) = crate::construction::turf_at(world, pos) else {
        return;
    };
    let path = path_of(world, turf).unwrap_or_default();
    if !take_matter(world, client, rtd, cost, true) {
        return;
    }
    match job {
        RhdJob::RtdLay { design, .. } => {
            // `blueprint_changed`, the tile still plating.
            let r = world.get::<Rtd>(rtd).copied();
            if r.is_none_or(|r| r.design != design) || !is_under(&path, PLATING) {
                return;
            }
            let Some(new_turf) = tile_turf(world, TILES[design].2) else {
                return;
            };
            let Some(new) = crate::construction::place_on_top(world, pos, &new_turf, true) else {
                return;
            };
            let dir = r.map_or(Dir::South, |r| r.dir);
            if let Some(mut atom) = world.get_mut::<DmAtom>(new) {
                atom.dir = dir as u8;
            }
            world.entity_mut(new).insert(Facing(dir));
            tile_changed(world, pos);
        }
        RhdJob::RtdRemove { .. } => {
            if is_under(&path, PLATING) || design_of_floor(world, &path).is_none() {
                return;
            }
            let decals: Vec<Entity> = world
                .resource::<SpatialIndex>()
                .on_tile(pos)
                .iter()
                .copied()
                .filter(|&e| path_of(world, e).is_some_and(|p| is_under(&p, DECAL)))
                .collect();
            for decal in decals {
                world.despawn(decal);
            }
            let stack = crate::construction::baseturfs_of(world, turf);
            let over_bottom = stack.last().is_none_or(|b| b == BASETURF_BOTTOM);
            let changed = if over_bottom {
                crate::construction::change_turf(world, pos, PLATING, stack, true)
            } else {
                crate::construction::scrape_away(world, pos, true)
            };
            if changed.is_none() {
                return;
            }
        }
        _ => return,
    }
    take_matter(world, client, rtd, cost, false);
    playsound(world, PlaySound::new(pos, DECONSTRUCT_SOUND, 50.0, true));
}

// --- RDD

fn rdd_click(world: &mut World, actor: Actor, rdd: Entity, target: Entity, right: bool) -> bool {
    let Some((turf, pos)) = turf_of_target(world, target) else {
        return false;
    };
    if right {
        if has_on_tile(world, pos, DECORATION).is_none() {
            return false;
        }
        sound_at(world, turf, CLICK, 50.0, true);
        do_after(
            world,
            actor,
            turf,
            RDD_DELAY,
            0,
            Job::Rhd(RhdJob::RddRemove),
        );
        return true;
    }
    if !rdd_room(world, actor.client, pos) {
        return true;
    }
    if !take_matter(world, actor.client, rdd, 1, true) {
        return true;
    }
    let Some(r) = world.get::<Rdd>(rdd).copied() else {
        return true;
    };
    sound_at(world, rdd, CLICK, 50.0, true);
    do_after(
        world,
        actor,
        turf,
        RDD_DELAY,
        0,
        Job::Rhd(RhdJob::RddPlace { design: r.design }),
    );
    true
}

/// Not blocked (but by mobs), fewer than three decorations.
fn rdd_room(world: &mut World, client: Entity, pos: TilePos) -> bool {
    if super::rcd::blocked(world, pos, true, &[]) {
        tell(world, client, Span::Warning, "Клетка занята!");
        return false;
    }
    let count = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter(|&&e| path_of(world, e).is_some_and(|p| is_under(&p, DECORATION)))
        .count();
    if count >= 3 {
        tell(
            world,
            client,
            Span::Warning,
            "Здесь слишком много украшений!",
        );
        return false;
    }
    true
}

fn rdd_complete(world: &mut World, action: &DoAfter, rdd: Entity, job: RhdJob) {
    let client = action.client;
    let pos = action.target_pos;
    match job {
        RhdJob::RddPlace { design } => {
            if world.get::<Rdd>(rdd).is_none_or(|r| r.design != design)
                || !rdd_room(world, client, pos)
                || !take_matter(world, client, rdd, 1, true)
            {
                return;
            }
            let Some((_, path, _)) = decorations().into_iter().nth(design) else {
                return;
            };
            if spawn_obj(world, &path, pos).is_none() {
                return;
            }
            take_matter(world, client, rdd, 1, false);
        }
        RhdJob::RddRemove => {
            let Some(found) = has_on_tile(world, pos, DECORATION) else {
                return;
            };
            world.despawn(found);
            tile_changed(world, pos);
        }
        _ => return,
    }
    playsound(world, PlaySound::new(pos, DECONSTRUCT_SOUND, 50.0, true));
}

// --- RPLD

fn rpld_click(world: &mut World, actor: Actor, rpld: Entity, target: Entity) -> bool {
    let Some(r) = world.get::<Rpld>(rpld).copied() else {
        return false;
    };
    let table = plumbing_table(r.service);
    let path = path_of(world, target).unwrap_or_default();
    // A machine of a design: taken back.
    if let Some(&(_, _, cost)) = table.iter().find(|(_, m, _)| *m == path) {
        if world
            .get::<crate::obj_damage::Anchored>(target)
            .is_some_and(|a| a.0)
        {
            tell(world, actor.client, Span::Warning, "Нужно открутить!");
            return true;
        }
        do_after(
            world,
            actor,
            target,
            RPLD_TAKE_DELAY,
            0,
            Job::Rhd(RhdJob::RpldTake { cost }),
        );
        return true;
    }
    if world.get::<Turf>(target).is_none() || !is_under(&path, OPEN) {
        return false;
    }
    let Some(&(_, _, cost)) = table.get(r.design) else {
        return true;
    };
    if !take_matter(world, actor.client, rpld, cost, true) {
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    if !rpld_can_place(world, pos, table[r.design].1, r.layer) {
        tell(world, actor.client, Span::Warning, "Клетка занята!");
        return true;
    }
    do_after(
        world,
        actor,
        target,
        cost as f32,
        0,
        Job::Rhd(RhdJob::RpldBuild {
            design: r.design,
            cost,
        }),
    );
    true
}

/// `canPlace`: a dense machine needs a free tile; one machine per ducting layer and tile
/// (`ducting_layer_check`).
fn rpld_can_place(world: &World, pos: TilePos, machine: &str, layer: u8) -> bool {
    let dense = world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(machine))
        .and_then(|p| p.get::<DmAtom>())
        .is_some_and(|a| a.density);
    if dense && super::rcd::blocked(world, pos, false, &[]) {
        return false;
    }
    !world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| world.get::<PlumbingLayer>(e).is_some_and(|l| l.0 == layer))
}

fn rpld_complete(world: &mut World, action: &DoAfter, rpld: Entity, job: RhdJob) {
    let client = action.client;
    let pos = action.target_pos;
    let Some(r) = world.get::<Rpld>(rpld).copied() else {
        return;
    };
    match job {
        RhdJob::RpldBuild { design, cost } => {
            let table = plumbing_table(r.service);
            if r.design != design || !take_matter(world, client, rpld, cost, true) {
                return;
            }
            let machine = table[design].1;
            if !rpld_can_place(world, pos, machine, r.layer) {
                tell(world, client, Span::Warning, "Клетка занята!");
                return;
            }
            let Some(new) = spawn_obj(world, machine, pos) else {
                return;
            };
            world.entity_mut(new).insert(PlumbingLayer(r.layer));
            crate::obj_damage::set_anchored(world, new, false);
            take_matter(world, client, rpld, cost, false);
        }
        RhdJob::RpldTake { cost } => {
            let target = action.target;
            if world
                .get::<crate::obj_damage::Anchored>(target)
                .is_some_and(|a| a.0)
            {
                return;
            }
            let Some(m) = matter_of(world, rpld) else {
                return;
            };
            let room = m.max.saturating_sub(m.matter);
            let back = cost.min(room);
            if back < cost {
                tell(world, client, Span::Warning, "Хранилище заполнено!");
            }
            if let Some(mut m) = world.get_mut::<Matter>(rpld)
                && m.max != u32::MAX
            {
                m.matter += back;
            }
            world.despawn(target);
            tile_changed(world, pos);
        }
        _ => return,
    }
    playsound(world, PlaySound::new(pos, CLICK, 50.0, true));
}

// --- RSF

fn rsf_click(world: &mut World, actor: Actor, rsf: Entity, target: Entity) -> bool {
    let Some(r) = world.get::<Rsf>(rsf).copied() else {
        return false;
    };
    let path = path_of(world, target).unwrap_or_default();
    let allowed =
        (world.get::<Turf>(target).is_some() && is_under(&path, FLOOR)) || is_under(&path, TABLE);
    if !allowed {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < r.ready_at {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    if r.matter == 0 {
        tell(world, actor.client, Span::Warning, "Не хватает материи.");
        return true;
    }
    let cookies = item_path(world, rsf).is_some_and(|p| is_under(&p, COOKIESYNTH));
    let Some(&made) = rsf_table(cookies).get(r.design) else {
        return true;
    };
    let Some(new) = spawn_obj(world, made, pos) else {
        return true;
    };
    let matter = r.matter - 1;
    if let Some(mut r) = world.get_mut::<Rsf>(rsf) {
        r.matter = matter;
        if cookies {
            r.ready_at = now + COOKIE_COOLDOWN;
        }
    }
    sound_at(world, rsf, CLICK, 10.0, true);
    let name = world
        .get::<DmName>(new)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_else(|| proto_name(world, made));
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("Создаётся {name}..."),
    );
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("В нём теперь {matter}/{} единиц.", r.max),
    );
    true
}

// --- RWD

/// `on_move`: a wielded RWD lays cable on the mob's new tile and takes stray coils in.
fn rwd_on_moved(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        let Some(inv) = world.get::<Inventory>(m.entity) else {
            continue;
        };
        let Some(rwd) = inv
            .hands
            .iter()
            .flatten()
            .copied()
            .find(|&h| world.get::<Rwd>(h).is_some())
        else {
            continue;
        };
        rwd_step(world, m.entity, rwd, m.to);
    }
}

pub fn rwd_step(world: &mut World, mob: Entity, rwd: Entity, pos: TilePos) {
    let Some(r) = world.get::<Rwd>(rwd).copied() else {
        return;
    };
    // Unwielded when the other hand took something.
    let wielded = r.active && other_hand_free(world, mob, rwd);
    if r.active && !wielded {
        world.get_mut::<Rwd>(rwd).expect("checked").active = false;
    }
    if wielded
        && r.amount > 0
        && can_lay_cable(world, pos)
        && cable_allowed_here(world, pos, r.layer)
    {
        lay_cable(world, pos, r.layer);
        if let Some(mut r) = world.get_mut::<Rwd>(rwd)
            && r.max != u32::MAX
        {
            r.amount -= 1;
        }
    }
    let coils: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            world
                .get::<DmItem>(e)
                .is_some_and(|i| is_under(&i.path, CABLE_COIL))
        })
        .collect();
    for coil in coils {
        add_cable(world, None, rwd, coil);
    }
}

/// `can_have_cabling() && can_lay_cable()`: plating (no floor tile over it).
fn can_lay_cable(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .is_some_and(|p| is_under(&p, PLATING))
}

/// `cable_allowed_here`: no cable of this layer on the tile yet.
fn cable_allowed_here(world: &World, pos: TilePos, layer: u8) -> bool {
    !world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world
                .get::<DmCable>(e)
                .is_some_and(|c| c.layer & layer != 0)
        })
}

/// `place_turf`: a cable of `layer` on `pos`, joined into the grid, it and its neighbours'
/// sprites relinked.
fn lay_cable(world: &mut World, pos: TilePos, layer: u8) -> Option<Entity> {
    let cable = spawn_obj(world, cable_type(layer), pos)?;
    if let Some(mut grid) = world.get_resource_mut::<crate::power::grid::Grid>() {
        grid.add_cable(cable, pos, layer);
    }
    let mut tiles = vec![pos];
    tiles.extend(Dir::CARDINALS.iter().map(|&d| pos.step(d)));
    for tile in tiles {
        relink_sprites(world, tile);
    }
    Some(cable)
}

fn cables_on(world: &World, pos: TilePos) -> Vec<(Entity, u8, u8)> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter_map(|&e| {
            world
                .get::<DmCable>(e)
                .filter(|c| c.layer != 0)
                .map(|c| (e, c.layer, c.banned_links))
        })
        .collect()
}

/// The cable sprites of a tile (`ss13_mapload::cable::connect_all` for one tile).
fn relink_sprites(world: &mut World, pos: TilePos) {
    let mut updates = Vec::new();
    let node_here = cables_on(world, pos).iter().any(|&(_, _, b)| b != 0);
    for (e, layer, banned) in cables_on(world, pos) {
        let mut links = 0u8;
        for d in Dir::CARDINALS {
            let bit = d as u8;
            if banned & bit != 0 {
                continue;
            }
            let inverse = d.reverse() as u8;
            let linked = cables_on(world, pos.step(d))
                .iter()
                .any(|&(_, l, b)| l & layer != 0 && b & inverse == 0);
            if linked {
                links |= bit;
            }
        }
        updates.push((
            e,
            ss13_mapload::cable::state(layer, links, banned != 0 || node_here),
        ));
    }
    for (e, state) in updates {
        let id = world.get::<DmAtom>(e).and_then(|a| {
            world
                .get_resource::<IconIndex>()
                .and_then(|i| i.get(&a.icon, &state))
        });
        if let Some(id) = id
            && let Some(mut appearance) = world.get_mut::<Appearance>(e)
            && appearance.icon_state != id
        {
            appearance.icon_state = id;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Completion and examine

/// The do_after's end.
pub fn complete(world: &mut World, action: &DoAfter, job: RhdJob) {
    let Some(item) = action.item else { return };
    match job {
        RhdJob::RldLight { .. } | RhdJob::RldRemove => rld_complete(world, action, item, job),
        RhdJob::RtdLay { .. } | RhdJob::RtdRemove { .. } => rtd_complete(world, action, item, job),
        RhdJob::RddPlace { .. } | RhdJob::RddRemove => rdd_complete(world, action, item, job),
        RhdJob::RpldBuild { .. } | RhdJob::RpldTake { .. } => {
            rpld_complete(world, action, item, job)
        }
    }
}

pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(r) = world.get::<Rwd>(target) {
        lines.push((
            Span::Plain,
            format!("В нём {} кусков кабеля.", amount_text(r.amount, r.max)),
        ));
    }
    if let Some(r) = world.get::<Rsf>(target) {
        lines.push((
            Span::Notice,
            format!("В нём {}/{} единиц.", r.matter, r.max),
        ));
    }
    if let Some(r) = world.get::<Rdd>(target)
        && let Some((_, path, _)) = decorations().into_iter().nth(r.design)
    {
        lines.push((
            Span::Info,
            format!("Сейчас делает: {}.", proto_name(world, &path)),
        ));
    }
    if let Some(r) = world.get::<Rtd>(target)
        && let Some(&(category, name, ..)) = TILES.get(r.design)
    {
        lines.push((Span::Notice, format!("Плитка: {category}: {name}.")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtd_times_and_removal_cost_follow_tg() {
        assert_eq!(tile_delays(4), (6.0, 10.0));
        assert_eq!(tile_removal_cost(4), 3, "2.8 rounded up");
        assert_eq!(tile_removal_cost(10), 7);
        assert_eq!(
            TILES[0],
            ("Standard", "Base", "/obj/item/stack/tile/iron", 4)
        );
        assert_eq!(TILES.len(), 72);
    }

    #[test]
    fn rdd_designs_expand_in_tg_order() {
        let designs = decorations();
        assert_eq!(designs[0].1, "/obj/structure/decoration/grass/first");
        assert_eq!(designs[1].2, " (Alt)");
        assert_eq!(designs[3].1, "/obj/structure/decoration/grass/style_random");
        assert!(
            designs
                .iter()
                .any(|d| d.1 == "/obj/structure/decoration/bush/snow/sixth")
        );
        assert_eq!(designs.len(), 143);
    }

    #[test]
    fn wall_lights_go_to_the_closest_side() {
        let wall = TilePos::new(5, 5, 1);
        let start = TilePos::new(5, 2, 1);
        let sides = [wall.step(Dir::North), wall.step(Dir::South)];
        assert_eq!(closest(start, &sides), Some(wall.step(Dir::South)));
        assert_eq!(closest(start, &[]), None);
    }

    #[test]
    fn rwd_layers_pick_their_cable_and_colour_parses() {
        assert_eq!(cable_type(1), "/obj/structure/cable/layer1");
        assert_eq!(cable_type(2), "/obj/structure/cable");
        assert_eq!(cable_type(4), "/obj/structure/cable/layer3");
        assert_eq!(parse_hex("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex("ff8000"), None);
    }

    #[test]
    fn plumbing_and_service_tables_follow_tg() {
        assert_eq!(
            PLUMBING[0],
            ("Synthesizers", "/obj/machinery/plumbing/synthesizer", 15)
        );
        assert!(
            PLUMBING
                .iter()
                .any(|&(_, m, c)| m == "/obj/machinery/duct" && c == 1)
        );
        assert!(
            PLUMBING_SERVICE
                .iter()
                .any(|&(_, m, _)| m == "/obj/machinery/plumbing/synthesizer/beer")
        );
        assert_eq!(
            RSF_ITEMS[0],
            "/obj/item/reagent_containers/cup/glass/drinkingglass"
        );
    }
}
