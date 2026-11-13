//! The rapid construction device (server): tg `/obj/item/construction` (`game/objects/items/
//! rcd/RHD.dm`) and `/obj/item/construction/rcd` (`RCD.dm`), with the `rcd_vals`/`rcd_act` of
//! the atoms it works on.
//!
//! - Matter ([`Matter`]): `matter` of `max_matter`, loaded from matter cartridges
//!   (`/obj/item/rcd_ammo`, 40 or 160) or sheets by their `matter_amount` (iron and glass 4,
//!   reinforced glass 6, plasma reinforced glass 8, plasteel 12, rods 2, iron tiles 1), whole
//!   sheets only; either the RCD clicked with them or them clicked with the RCD. Examine shows
//!   "N/M matter-units" and the installed upgrade disks.
//! - Upgrade disks (`/obj/item/rcd_upgrade/*`) go in the same way, once each: frames (machine and
//!   computer frames), furnishing (furniture), anti disruption, enhanced cooling, simple
//!   circuits and silo link (installed, but see the shortcut).
//! - In hand: a list of the designs (tg's `RapidConstructionDevice` categories in order;
//!   machines and furniture only with their disk) and the airlock access entry, which opens the
//!   airlock electronics window bound to the RCD (`airlock_electronics`).
//! - A left click builds the design on the clicked atom, a right click deconstructs it. Costs
//!   and delays are tg's `rcd_vals` (× `delay_mod`; reinforced walls ×2 and only with
//!   `canRturf`); matter and the tile are checked before and after the delay, the matter is
//!   used at the end; a changed design cancels a build (`blueprint_changed`). A construction
//!   effect (`/obj/effect/constructing_effect`) shows on the tile meanwhile.
//! - Built: plating on space (1 with lattice, 3 without) or on a lattice, a catwalk (2/4), a
//!   wall on a floor (16) or on a girder (8), grilles (4) and windows on a floor or a grille
//!   (directional ones facing the user, not over a window of that direction), girders, frames,
//!   furniture facing the user, airlocks (16, the plain glass one 20) through a door assembly
//!   that finishes itself with the RCD's electronics. Deconstructed: walls, floors (scraped one
//!   layer), airlocks, grilles, windows, girders, tables, catwalks.
// shortcut: the design menu is a list prompt instead of tg's icon window; one build at a time
// (the do_after is exclusive here), so the ×3 frequent use delay and the destruction scan
// memory never apply; the effect cannot be hit to interrupt a build (as with the anti
// disruption disk); no silo link (the ore silo's remote materials), no ranged ARCD beam, no
// cyborg (cell) or exosuit RCD, no windoors, reflectors, floodlight frames or wall frames
// (APC, alarms: the simple circuits disk does nothing yet), no openspace clicks, no emag/EMP
// detonation; misc, lava and chasm turfs take nothing.
// BandaStation `skillchip_rcd`: without the engineer skillchip ([`knows_advanced`]) a right
// click does not deconstruct ("нет скиллчипа!"), the list has no airlock access and every
// build takes 1.5 times as long; silicons, drones, repair bots, the ARCD and the admin RCD
// need no chip.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, DmName, DmObj, Turf};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::{AlwaysState, OpenUi, UiSystems};
use ss13_world::SpatialIndex;

use crate::airlock_construction::{AirlockElectronics, DoorAssembly, ElectronicsWindow};
use crate::construction::{Job, path_of, place_on_top, scrape_away, spawn_obj, tile_changed};
use crate::doors::Door;
use crate::health::{Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::{is_under, set_anchored, set_icon_state};
use crate::silicon::cyborg::{Cyborg, STANDARD_CELL_CHARGE};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, Stack, do_after, tell, use_stack};

pub const RCD: &str = "/obj/item/construction/rcd";
/// BandaStation `RCD_NO_SKILLCHIP_DELAY_MULTIPLIER`.
const NO_SKILLCHIP_DELAY_MULTIPLIER: f32 = 1.5;
/// The RCDs whose `check_engineer_skillchip` always passes (BandaStation).
const NO_SKILLCHIP_RCDS: [&str; 2] = [
    "/obj/item/construction/rcd/combat/admin",
    "/obj/item/construction/rcd/arcd",
];
/// `/obj/item/construction`: the RCD and its family ([`super::rhd`]) share [`Matter`].
const CONSTRUCTION: &str = "/obj/item/construction";
const AMMO: &str = "/obj/item/rcd_ammo";
const AMMO_LARGE: &str = "/obj/item/rcd_ammo/large";
const UPGRADE: &str = "/obj/item/rcd_upgrade";
const EFFECT: &str = "/obj/effect/constructing_effect";
const PLATING_DESIGN: &str = "/turf/open/floor/plating/rcd";
const PLATING: &str = "/turf/open/floor/plating";
const SPACE: &str = "/turf/open/space";
const OPENSPACE: &str = "/turf/open/openspace";
const FLOOR: &str = "/turf/open/floor";
const WALL: &str = "/turf/closed/wall";
const LATTICE: &str = crate::explosions::ex_act::LATTICE;
const CATWALK: &str = crate::explosions::ex_act::CATWALK;
const GIRDER: &str = "/obj/structure/girder";
const GRILLE: &str = "/obj/structure/grille";
const WINDOW: &str = "/obj/structure/window";
const R_WINDOW: &str = "/obj/structure/window/reinforced";
const F_WINDOW: &str = "/obj/structure/window/fulltile";
const RF_WINDOW: &str = "/obj/structure/window/reinforced/fulltile";
const WINDOOR: &str = "/obj/machinery/door/window";
const AIRLOCK: &str = "/obj/machinery/door/airlock";
const GLASS_AIRLOCK: &str = "/obj/machinery/door/airlock/glass";
const TABLE: &str = "/obj/structure/table";
const CLICK: &str = "sound/machines/click.ogg";
const DECONSTRUCT_SOUND: &str = "sound/items/deconstruct.ogg";
const POP: &str = "sound/effects/pop.ogg";
/// `RCD_RWALL_DELAY_MULT`.
const RWALL_DELAY_MULT: f32 = 2.0;
/// `max_matter = INFINITY`.
const INFINITE: u32 = u32::MAX;

/// `RCD_UPGRADE_*`.
pub const UPGRADE_FRAMES: u8 = 1 << 0;
pub const UPGRADE_SIMPLE_CIRCUITS: u8 = 1 << 1;
pub const UPGRADE_SILO_LINK: u8 = 1 << 2;
pub const UPGRADE_FURNISHING: u8 = 1 << 3;
pub const UPGRADE_ANTI_INTERRUPT: u8 = 1 << 4;
pub const UPGRADE_COOLING: u8 = 1 << 5;
const ALL_UPGRADES: u8 = 0b11_1111;

/// Disk path, upgrade bit, examine name; subtypes before nothing (all siblings).
const DISKS: [(&str, u8, &str); 6] = [
    ("/obj/item/rcd_upgrade/frames", UPGRADE_FRAMES, "рамы"),
    (
        "/obj/item/rcd_upgrade/simple_circuits",
        UPGRADE_SIMPLE_CIRCUITS,
        "простые платы",
    ),
    (
        "/obj/item/rcd_upgrade/silo_link",
        UPGRADE_SILO_LINK,
        "связь с силосом",
    ),
    (
        "/obj/item/rcd_upgrade/furnishing",
        UPGRADE_FURNISHING,
        "мебель",
    ),
    (
        "/obj/item/rcd_upgrade/anti_interrupt",
        UPGRADE_ANTI_INTERRUPT,
        "защита от прерывания",
    ),
    (
        "/obj/item/rcd_upgrade/cooling",
        UPGRADE_COOLING,
        "улучшенное охлаждение",
    ),
];

/// Stack `matter_amount`, subtypes first.
const SHEET_MATTER: [(&str, u32); 7] = [
    ("/obj/item/stack/sheet/plasmarglass", 8),
    ("/obj/item/stack/sheet/rglass", 6),
    ("/obj/item/stack/sheet/glass", 4),
    ("/obj/item/stack/sheet/plasteel", 12),
    ("/obj/item/stack/sheet/iron", 4),
    ("/obj/item/stack/rods", 2),
    ("/obj/item/stack/tile/iron", 1),
];

/// RCD types (and the family's, [`super::rhd`]): path, `max_matter`, starting `matter`,
/// `construction_upgrades`, `canRturf`, `delay_mod`; subtypes first.
const VARIANTS: [(&str, u32, u32, u8, bool, f32); 16] = [
    // A base construction console's (`crate::base_construction`); restocked on the map.
    ("/obj/item/construction/rcd/internal", 600, 0, 0, false, 0.5),
    (
        "/obj/item/construction/rcd/borg/syndicate",
        160,
        0,
        0,
        true,
        1.0,
    ),
    ("/obj/item/construction/rld/mini", 100, 100, 0, false, 1.0),
    ("/obj/item/construction/rld", 200, 200, 0, false, 1.0),
    (
        "/obj/item/construction/rtd/admin",
        INFINITE,
        INFINITE,
        0,
        false,
        1.0,
    ),
    ("/obj/item/construction/rtd/loaded", 350, 350, 0, false, 1.0),
    ("/obj/item/construction/rtd", 350, 0, 0, false, 1.0),
    ("/obj/item/construction/rdd/loaded", 60, 60, 0, false, 1.0),
    ("/obj/item/construction/rdd", 60, 0, 0, false, 1.0),
    ("/obj/item/construction/plumbing", 200, 200, 0, false, 1.0),
    (
        "/obj/item/construction/rcd/combat/admin",
        INFINITE,
        INFINITE,
        ALL_UPGRADES & !UPGRADE_SILO_LINK,
        true,
        0.1,
    ),
    (
        "/obj/item/construction/rcd/combat",
        500,
        500,
        ALL_UPGRADES,
        true,
        1.0,
    ),
    (
        "/obj/item/construction/rcd/arcd",
        INFINITE,
        INFINITE,
        ALL_UPGRADES & !UPGRADE_SILO_LINK,
        true,
        0.6,
    ),
    (
        "/obj/item/construction/rcd/ce",
        160,
        160,
        UPGRADE_ANTI_INTERRUPT | UPGRADE_COOLING,
        false,
        1.0,
    ),
    (
        "/obj/item/construction/rcd/loaded/upgraded",
        160,
        160,
        ALL_UPGRADES,
        false,
        1.0,
    ),
    ("/obj/item/construction/rcd/loaded", 160, 160, 0, false, 1.0),
];

/// `/obj/item/construction/{rcd,rtd,rdd}/borg`: `energyfactor`, joules of the cyborg's cell per
/// matter unit. Subtypes first.
const CELL_POWERED: [(&str, f64); 4] = [
    (
        "/obj/item/construction/rcd/borg/syndicate",
        0.066 * STANDARD_CELL_CHARGE,
    ),
    (
        "/obj/item/construction/rcd/borg",
        0.072 * STANDARD_CELL_CHARGE,
    ),
    (
        "/obj/item/construction/rtd/borg",
        0.03 * STANDARD_CELL_CHARGE,
    ),
    (
        "/obj/item/construction/rdd/borg",
        0.1 * STANDARD_CELL_CHARGE,
    ),
];

/// A cyborg's RHD (tg's `/borg` types): its joules per matter unit come out of the holder's cell.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CellPowered(pub f64);

fn cell_powered(path: &str) -> Option<CellPowered> {
    CELL_POWERED
        .iter()
        .find(|(p, _)| is_under(path, p))
        .map(|&(_, factor)| CellPowered(factor))
}

/// `get_matter`: a cyborg's RHD shows the holder's cell (`0` in any other hands), others their
/// own matter.
pub(crate) fn shown_matter(world: &World, item: Entity, m: &Matter) -> Matter {
    let mut shown = *m;
    if world.get::<CellPowered>(item).is_some() {
        let cyborg = world
            .get::<CarriedBy>(item)
            .and_then(|c| world.get::<Cyborg>(c.0));
        shown.matter = cyborg.map_or(0, |c| c.charge as u32);
        if let Some(cyborg) = cyborg {
            shown.max = cyborg.max_charge as u32;
        }
    }
    shown
}

/// tg `useResource(amount, user, dry_run)`: a cell-powered RHD draws on the cyborg holding it,
/// the others on their own matter. False (with the warning) when short.
pub(crate) fn use_resource(
    world: &mut World,
    client: Entity,
    item: Entity,
    cost: u32,
    dry: bool,
) -> bool {
    let Some(&CellPowered(per_unit)) = world.get::<CellPowered>(item) else {
        let Some(m) = world.get::<Matter>(item).copied() else {
            return false;
        };
        if !m.has(cost) {
            tell(world, client, Span::Warning, "Недостаточно материи!");
            return false;
        }
        if !dry && let Some(mut m) = world.get_mut::<Matter>(item) {
            m.take(cost);
        }
        return true;
    };
    let Some(borg) = world.get::<CarriedBy>(item).map(|c| c.0) else {
        return false;
    };
    if world
        .get::<crate::silicon::chassis::Chassis>(borg)
        .is_some_and(|c| c.cell.is_none())
    {
        tell(world, client, Span::Warning, "Ячейка не найдена!");
        return false;
    }
    let need = f64::from(cost) * per_unit;
    let Some(mut cyborg) = world.get_mut::<Cyborg>(borg) else {
        return false;
    };
    if cyborg.charge < need {
        tell(world, client, Span::Warning, "Недостаточно заряда!");
        return false;
    }
    if !dry {
        cyborg.charge -= need;
    }
    true
}

/// `RCD_TURF`, `RCD_WINDOWGRILLE`, `RCD_STRUCTURE`, `RCD_AIRLOCK`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Turf,
    WindowGrille,
    Structure,
    Airlock,
}

/// A `GLOB.rcd_designs` entry: the disk it needs, mode, path, label (empty: the type's name).
pub struct Design {
    pub needs: u8,
    pub mode: Mode,
    pub path: &'static str,
    pub label: &'static str,
}

const fn d(needs: u8, mode: Mode, path: &'static str, label: &'static str) -> Design {
    Design {
        needs,
        mode,
        path,
        label,
    }
}

const M: u8 = UPGRADE_FRAMES;
const F: u8 = UPGRADE_FURNISHING;

/// tg's designs in menu order (see the shortcut for the missing ones).
pub const DESIGNS: &[Design] = &[
    d(0, Mode::Turf, PLATING_DESIGN, "Пол и стены"),
    d(0, Mode::WindowGrille, WINDOW, "Окно (направленное)"),
    d(
        0,
        Mode::WindowGrille,
        R_WINDOW,
        "Укреплённое окно (направленное)",
    ),
    d(0, Mode::WindowGrille, F_WINDOW, "Окно на решётке"),
    d(
        0,
        Mode::WindowGrille,
        RF_WINDOW,
        "Укреплённое окно на решётке",
    ),
    d(0, Mode::Turf, CATWALK, "Мостик"),
    d(0, Mode::Structure, GIRDER, "Каркас стены"),
    d(
        M,
        Mode::Structure,
        "/obj/structure/frame/machine/secured",
        "Рама машины",
    ),
    d(
        M,
        Mode::Structure,
        "/obj/structure/frame/computer/rcd/north",
        "Рама компьютера (север)",
    ),
    d(
        M,
        Mode::Structure,
        "/obj/structure/frame/computer/rcd/south",
        "Рама компьютера (юг)",
    ),
    d(
        M,
        Mode::Structure,
        "/obj/structure/frame/computer/rcd/east",
        "Рама компьютера (восток)",
    ),
    d(
        M,
        Mode::Structure,
        "/obj/structure/frame/computer/rcd/west",
        "Рама компьютера (запад)",
    ),
    d(F, Mode::Structure, "/obj/structure/chair", "Стул"),
    d(F, Mode::Structure, "/obj/structure/chair/stool", "Табурет"),
    d(
        F,
        Mode::Structure,
        "/obj/structure/chair/stool/bar",
        "Барный табурет",
    ),
    d(F, Mode::Structure, TABLE, "Стол"),
    d(
        F,
        Mode::Structure,
        "/obj/structure/table/glass",
        "Стеклянный стол",
    ),
    d(F, Mode::Structure, "/obj/structure/rack", "Стеллаж"),
    d(F, Mode::Structure, "/obj/structure/bed", "Кровать"),
    d(0, Mode::Airlock, GLASS_AIRLOCK, ""),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/public/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/engineering/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/atmos/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/security/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/command/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/medical/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/research/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/hydroponics/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/virology/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/mining/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/maintenance/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/external/glass",
        "",
    ),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/maintenance/external/glass",
        "",
    ),
    d(0, Mode::Airlock, AIRLOCK, ""),
    d(0, Mode::Airlock, "/obj/machinery/door/airlock/public", ""),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/engineering",
        "",
    ),
    d(0, Mode::Airlock, "/obj/machinery/door/airlock/atmos", ""),
    d(0, Mode::Airlock, "/obj/machinery/door/airlock/security", ""),
    d(0, Mode::Airlock, "/obj/machinery/door/airlock/command", ""),
    d(0, Mode::Airlock, "/obj/machinery/door/airlock/medical", ""),
    d(0, Mode::Airlock, "/obj/machinery/door/airlock/research", ""),
    d(0, Mode::Airlock, "/obj/machinery/door/airlock/freezer", ""),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/hydroponics",
        "",
    ),
    d(0, Mode::Airlock, "/obj/machinery/door/airlock/virology", ""),
    d(0, Mode::Airlock, "/obj/machinery/door/airlock/mining", ""),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/maintenance",
        "",
    ),
    d(0, Mode::Airlock, "/obj/machinery/door/airlock/external", ""),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/maintenance/external",
        "",
    ),
    d(0, Mode::Airlock, "/obj/machinery/door/airlock/hatch", ""),
    d(
        0,
        Mode::Airlock,
        "/obj/machinery/door/airlock/maintenance_hatch",
        "",
    ),
];

/// `/obj/item/construction` matter and upgrades, `/obj/item/construction/rcd` settings.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Matter {
    pub matter: u32,
    pub max: u32,
    pub upgrades: u8,
    /// `canRturf`.
    pub rturf: bool,
    pub delay_mod: f32,
    /// Index into [`DESIGNS`].
    pub design: usize,
}

impl Matter {
    pub fn for_path(path: &str) -> Self {
        let (max, matter, upgrades, rturf, delay_mod) = VARIANTS
            .iter()
            .find(|v| is_under(path, v.0))
            .map_or((160, 0, 0, false, 1.0), |v| (v.1, v.2, v.3, v.4, v.5));
        Self {
            matter,
            max,
            upgrades,
            rturf,
            delay_mod,
            design: 0,
        }
    }

    /// `useResource(dry_run = TRUE)`.
    pub fn has(&self, cost: u32) -> bool {
        self.max == INFINITE || self.matter >= cost
    }

    pub fn take(&mut self, cost: u32) {
        if self.max != INFINITE {
            self.matter -= cost.min(self.matter);
        }
    }
}

/// The timed build or deconstruction (`_rcd_create_effect`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RcdJob {
    /// `None`: deconstruct; else the [`DESIGNS`] index chosen at the click.
    pub design: Option<usize>,
    pub cost: u32,
    pub effect: Option<Entity>,
    /// Worked through a base construction console's eye: the RCD is the console's, not held.
    pub remote: bool,
}

/// A construction effect, gone at `until` if the build never finished.
#[derive(Component, Clone, Copy, Debug)]
struct RcdEffect {
    until: f64,
}

/// The design list open for an RCD: its entries (`None`: airlock access).
#[derive(Component, Clone, Debug)]
struct RcdPrompt {
    item: Entity,
    options: Vec<Option<usize>>,
    /// Opened through a base construction console (the RCD is the console's).
    remote: bool,
}

pub fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::AttackSelf, 1730, |w, c| {
        attack_self(w, c.client, c.mob, c.target)
    });
    // Matter or a disk onto the RCD in the other hand.
    app.add_click_handler(ClickPhase::OtherHand, 460, |w, c| {
        c.held.is_some_and(|h| load(w, c.client, c.target, h))
    });
    app.add_click_handler(ClickPhase::HeldOnTarget, 1560, |w, c| {
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
            expire_effects.run_if(any_with_component::<RcdEffect>),
            design_answers
                .in_set(UiSystems::Handle)
                .after(ss13_tgui::prompt::PromptSystems::Answer),
        ),
    );
}

/// `Initialize`: RCDs get their matter however they appear.
fn attach(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if is_under(&item.path, RCD) {
        commands.entity(add.entity).insert_if_new((
            Matter::for_path(&item.path),
            AirlockElectronics::default(),
            AlwaysState,
        ));
    } else if is_under(&item.path, CONSTRUCTION) {
        commands
            .entity(add.entity)
            .insert_if_new(Matter::for_path(&item.path));
    } else {
        return;
    }
    if let Some(powered) = cell_powered(&item.path) {
        commands.entity(add.entity).insert_if_new(powered);
    }
}

fn item_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

/// An RCD proper, not one of the family sharing [`Matter`].
fn is_rcd(world: &World, e: Entity) -> bool {
    item_path(world, e).is_some_and(|p| is_under(p, RCD))
}

/// `ammoamt` of a matter cartridge.
fn ammo_amount(world: &World, e: Entity) -> Option<u32> {
    let path = item_path(world, e)?;
    if !is_under(path, AMMO) {
        return None;
    }
    if let Some(left) = world.get::<AmmoLeft>(e) {
        return Some(left.0);
    }
    Some(if is_under(path, AMMO_LARGE) { 160 } else { 40 })
}

/// What is left in a partly used cartridge.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmmoLeft(pub u32);

pub fn sheet_matter(path: &str) -> Option<u32> {
    SHEET_MATTER
        .iter()
        .find(|(p, _)| is_under(path, p))
        .map(|&(_, m)| m)
}

/// `install_upgrade` and `insert_matter`: `used` into the RCD `rcd`. True when `used` is a disk,
/// a cartridge or a stack with matter (handled, loaded or refused).
pub fn load(world: &mut World, client: Entity, rcd: Entity, used: Entity) -> bool {
    let Some(m) = world.get::<Matter>(rcd).copied() else {
        return false;
    };
    let Some(path) = item_path(world, used).map(str::to_owned) else {
        return false;
    };
    let pos = crate::modpc::pda::turf_of(world, rcd);
    if is_under(&path, UPGRADE) {
        let Some(&(_, bit, _)) = DISKS.iter().find(|(p, ..)| is_under(&path, p)) else {
            return false;
        };
        if m.upgrades & bit != 0 {
            tell(world, client, Span::Warning, "Уже установлено!");
            return true;
        }
        // `banned_upgrades = RCD_ALL_UPGRADES & ~RCD_UPGRADE_SILO_LINK` of the family.
        if bit != UPGRADE_SILO_LINK && !is_rcd(world, rcd) {
            tell(
                world,
                client,
                Span::Warning,
                "Это улучшение сюда не подходит!",
            );
            return true;
        }
        // `/borg` `banned_upgrades = RCD_UPGRADE_SILO_LINK`.
        if bit == UPGRADE_SILO_LINK && world.get::<CellPowered>(rcd).is_some() {
            tell(world, client, Span::Warning, "Нельзя установить улучшение!");
            return true;
        }
        world.get_mut::<Matter>(rcd).expect("checked").upgrades |= bit;
        world.despawn(used);
        if let Some(pos) = pos {
            playsound(world, PlaySound::new(pos, CLICK, 50.0, true));
        }
        return true;
    }
    // `insert_matter`: `if(iscyborg(user)) return FALSE`; only cyborgs hold a `/borg` RHD.
    if world.get::<CellPowered>(rcd).is_some() {
        return false;
    }
    let room = m.max.saturating_sub(m.matter);
    let loaded = if let Some(ammo) = ammo_amount(world, used) {
        let load = ammo.min(room);
        if load == 0 {
            tell(world, client, Span::Warning, "Хранилище заполнено!");
            return true;
        }
        if ammo - load == 0 {
            world.despawn(used);
        } else {
            world.entity_mut(used).insert(AmmoLeft(ammo - load));
        }
        load
    } else if let Some(per) = sheet_matter(&path) {
        let sheets = (room / per).min(world.get::<Stack>(used).map_or(1, |s| s.amount));
        if sheets == 0 {
            tell(world, client, Span::Warning, "Хранилище заполнено!");
            return true;
        }
        if !use_stack(world, used, sheets) {
            return true;
        }
        sheets * per
    } else {
        return false;
    };
    if let Some(mut m) = world.get_mut::<Matter>(rcd)
        && m.max != INFINITE
    {
        m.matter += loaded;
    }
    if let Some(pos) = pos {
        playsound(world, PlaySound::new(pos, CLICK, 50.0, true));
    }
    true
}

/// `attack_self`: the design list.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    open_designs(world, client, mob, item, false)
}

/// The design list of `item`; `remote`: a base construction console's RCD (its "Configure RCD").
pub fn open_designs(
    world: &mut World,
    client: Entity,
    mob: Entity,
    item: Entity,
    remote: bool,
) -> bool {
    let Some(m) = world.get::<Matter>(item).copied() else {
        return false;
    };
    if !is_rcd(world, item) {
        return false;
    }
    if let Some(pos) = crate::modpc::pda::turf_of(world, item) {
        playsound(world, PlaySound::new(pos, POP, 50.0, false));
    }
    let shown = shown_matter(world, item, &m);
    let mut options = Vec::new();
    let mut items = Vec::new();
    for (i, design) in DESIGNS.iter().enumerate() {
        if design.needs != 0 && m.upgrades & design.needs == 0 {
            continue;
        }
        let mark = if i == m.design { "► " } else { "" };
        items.push(format!("{mark}{}", design_name(world, design)));
        options.push(Some(i));
    }
    // BandaStation: "Airlock Access" needs the engineer skillchip.
    if knows_advanced(world, mob, item) {
        items.push("Доступ шлюзов".to_owned());
        options.push(None);
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "RCD",
            format!("Материя: {}", matter_text(&shown)),
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(RcdPrompt {
        item,
        options,
        remote,
    });
    true
}

fn matter_text(m: &Matter) -> String {
    if m.max == INFINITE {
        "∞".to_owned()
    } else {
        format!("{}/{}", m.matter, m.max)
    }
}

pub fn design_name(world: &World, design: &Design) -> String {
    if !design.label.is_empty() {
        return design.label.to_owned();
    }
    world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(design.path))
        .and_then(|p| p.get::<DmName>())
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| design.path.rsplit('/').next().unwrap_or("").to_owned())
}

/// The chosen design (`handle_ui_act` "design"), re-checked: the RCD still in a hand of a
/// conscious user, the disk still in for its category.
fn design_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(RcdPrompt {
            item,
            options,
            remote,
        }) = world.get::<RcdPrompt>(prompt).cloned()
        else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        let Some(&choice) = options.get(index) else {
            continue;
        };
        let holding = if remote {
            crate::base_construction::controls_rcd(world, mob, item)
        } else {
            world
                .get::<Inventory>(mob)
                .is_some_and(|i| i.hands.contains(&Some(item)))
        };
        let able = world.get::<Stat>(mob) == Some(&Stat::Conscious)
            && !crate::status_effects::incapacitation(world, mob).incapacitated;
        if !holding || !able {
            continue;
        }
        let Some(m) = world.get::<Matter>(item).copied() else {
            continue;
        };
        if let Some(pos) = crate::modpc::pda::turf_of(world, item) {
            playsound(
                world,
                PlaySound::new(pos, "sound/items/tools/ratchet.ogg", 20.0, true),
            );
        }
        match choice {
            None => {
                if !knows_advanced(world, mob, item) {
                    continue;
                }
                world.write_message(OpenUi::new::<ElectronicsWindow>(
                    client,
                    mob,
                    item,
                    "Access Control".to_owned(),
                ));
            }
            Some(i) => {
                let Some(design) = DESIGNS.get(i) else {
                    continue;
                };
                if design.needs != 0 && m.upgrades & design.needs == 0 {
                    continue;
                }
                world.get_mut::<Matter>(item).expect("checked").design = i;
                let name = design_name(world, design);
                tell(world, client, Span::Notice, format!("Выбрано: {name}."));
            }
        }
    }
}

fn user_dir(world: &World, mob: Entity) -> Dir {
    world.get::<Facing>(mob).map_or(Dir::South, |f| f.0)
}

fn dir_bits(world: &World, e: Entity) -> u8 {
    world
        .get::<Facing>(e)
        .map(|f| f.0 as u8)
        .or_else(|| world.get::<DmAtom>(e).map(|a| a.dir))
        .unwrap_or(2)
}

fn on_tile_under(world: &World, pos: TilePos, root: &str) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| path_of(world, e).is_some_and(|p| is_under(&p, root)))
}

/// `is_blocked_turf`: something dense on the tile but the turf, `ignore` types and (with
/// `ignore_mobs`) mobs.
pub(crate) fn blocked(world: &World, pos: TilePos, ignore_mobs: bool, ignore: &[&str]) -> bool {
    world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&e| {
            if world.get::<Turf>(e).is_some() {
                return false;
            }
            if world.get::<Health>(e).is_some() {
                return !ignore_mobs;
            }
            let path = path_of(world, e).unwrap_or_default();
            !ignore.iter().any(|r| is_under(&path, r))
        })
}

/// `valid_build_direction` for a directional window or windoor facing `dir`.
fn direction_free(world: &World, pos: TilePos, dir: u8) -> bool {
    !world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            path_of(world, e).is_some_and(|p| is_under(&p, WINDOW) || is_under(&p, WINDOOR))
                && (world.get::<DmObj>(e).is_some_and(|o| o.fulltile) || dir_bits(world, e) == dir)
        })
}

fn window_cost(path: &str, on_grille: bool) -> Option<(f32, u32)> {
    Some(match path {
        WINDOW => (20.0, 4),
        R_WINDOW => (25.0, 6),
        F_WINDOW if on_grille => (30.0, 8),
        RF_WINDOW if on_grille => (40.0, 12),
        // A floor: the grille for a full-tile window first.
        F_WINDOW | RF_WINDOW => (10.0, 4),
        _ => return None,
    })
}

/// `structure_costs` of the floor's `rcd_vals`.
fn structure_cost(path: &str) -> Option<(f32, u32)> {
    const COSTS: [(&str, f32, u32); 10] = [
        ("/obj/structure/reflector", 20.0, 20),
        (GIRDER, 13.0, 8),
        ("/obj/structure/frame/machine/secured", 20.0, 20),
        ("/obj/structure/frame/computer/rcd", 20.0, 20),
        ("/obj/structure/floodlight_frame", 30.0, 20),
        ("/obj/structure/chair/stool/bar", 5.0, 4),
        ("/obj/structure/chair", 10.0, 4),
        (TABLE, 20.0, 8),
        ("/obj/structure/bed", 25.0, 8),
        ("/obj/structure/rack", 25.0, 4),
    ];
    COSTS
        .iter()
        .find(|(p, ..)| is_under(path, p))
        .map(|&(_, delay, cost)| (delay, cost))
}

/// `rcd_vals` of `target` for `design` (`None`: deconstruct): delay (deciseconds, before
/// `delay_mod`) and cost. `None`: the target takes nothing.
pub fn rcd_vals(
    world: &World,
    m: &Matter,
    target: Entity,
    design: Option<&Design>,
) -> Option<(f32, u32)> {
    let path = path_of(world, target)?;
    let pos = *world.get::<TilePos>(target)?;
    let is_turf = world.get::<Turf>(target).is_some();
    if is_turf && (is_under(&path, SPACE) || is_under(&path, OPENSPACE)) {
        let d = design?;
        let lattice = on_tile_under(world, pos, LATTICE).is_some();
        return match (d.mode, d.path) {
            (Mode::Turf, PLATING_DESIGN) => Some((0.0, if lattice { 1 } else { 3 })),
            (Mode::Turf, CATWALK) => Some((0.0, if lattice { 2 } else { 4 })),
            _ => None,
        };
    }
    if is_turf && is_under(&path, FLOOR) {
        let Some(d) = design else {
            return Some((50.0, 33));
        };
        return match d.mode {
            Mode::Turf => {
                // tg builds a wall here whatever the turf design; a catwalk design takes nothing.
                if d.path != PLATING_DESIGN {
                    return None;
                }
                if on_tile_under(world, pos, GIRDER).is_some() {
                    return Some((20.0, 8));
                }
                Some((20.0, 16))
            }
            Mode::WindowGrille => window_cost(d.path, on_tile_under(world, pos, GRILLE).is_some()),
            Mode::Airlock => Some((50.0, if d.path == GLASS_AIRLOCK { 20 } else { 16 })),
            Mode::Structure => structure_cost(d.path),
        };
    }
    if is_turf && is_under(&path, WALL) {
        design.is_none().then_some(())?;
        if is_under(&path, crate::reinforced::R_WALL) {
            return m.rturf.then_some((40.0 * RWALL_DELAY_MULT, 26));
        }
        return Some((40.0, 26));
    }
    if is_turf {
        return None;
    }
    // Objects: a click on one acts on it, or for builds on its tile where tg's turf does.
    if is_under(&path, CATWALK) {
        return design.is_none().then_some((10.0, 5));
    }
    if is_under(&path, LATTICE) {
        let d = design?;
        return match (d.mode, d.path) {
            (Mode::Turf, PLATING_DESIGN) => Some((0.0, 1)),
            (Mode::Turf, CATWALK) => Some((0.0, 2)),
            _ => None,
        };
    }
    if is_under(&path, GIRDER) {
        return match design {
            None => Some((20.0, 13)),
            Some(d) if d.mode == Mode::Turf && d.path == PLATING_DESIGN => Some((20.0, 8)),
            _ => None,
        };
    }
    if is_under(&path, GRILLE) {
        return match design {
            None => Some((20.0, 5)),
            Some(d) if d.mode == Mode::WindowGrille => window_cost(d.path, true),
            _ => None,
        };
    }
    if is_under(&path, WINDOW) {
        design.is_none().then_some(())?;
        let reinforced = is_under(&path, R_WINDOW);
        let fulltile = world.get::<DmObj>(target).is_some_and(|o| o.fulltile);
        return Some(match (reinforced, fulltile) {
            (true, true) => (40.0, 20),
            (true, false) => (30.0, 15),
            (false, true) => (25.0, 10),
            (false, false) => (20.0, 5),
        });
    }
    if is_under(&path, AIRLOCK) && world.get::<Door>(target).is_some() {
        return design.is_none().then_some((50.0, 32));
    }
    if is_under(&path, TABLE) {
        return design.is_none().then_some((24.0, 16));
    }
    None
}

/// `can_place`: the tile has room for what `design` builds there.
fn can_place(world: &World, target: Entity, design: Option<&Design>) -> bool {
    let Some(d) = design else {
        return true;
    };
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    let path = path_of(world, target).unwrap_or_default();
    match d.mode {
        Mode::WindowGrille => {
            let fulltile = matches!(d.path, F_WINDOW | RF_WINDOW);
            let ignore: &[&str] = if is_under(&path, GRILLE) {
                if fulltile {
                    &[GRILLE]
                } else {
                    &[GRILLE, WINDOW]
                }
            } else {
                &[WINDOW]
            };
            !blocked(world, pos, !fulltile, ignore)
        }
        Mode::Turf if d.path == PLATING_DESIGN => {
            if world.get::<Turf>(target).is_some() && !is_under(&path, FLOOR) {
                return true;
            }
            !blocked(world, pos, false, &[GIRDER])
        }
        Mode::Turf => true,
        Mode::Airlock => !blocked(world, pos, false, &[]),
        Mode::Structure => {
            const SMALL: [&str; 5] = [
                "/obj/structure/frame/computer/rcd",
                "/obj/structure/chair",
                TABLE,
                "/obj/structure/rack",
                "/obj/structure/bed",
            ];
            let small = SMALL.iter().any(|s| is_under(d.path, s))
                || is_under(d.path, "/obj/structure/frame/machine/secured");
            let ignore: &[&str] = if small { &[WINDOW] } else { &[] };
            !blocked(world, pos, false, ignore)
        }
    }
}

/// The RCD used on `target` (`interact_with_atom`, `_secondary`): loading, then
/// `rcd_create`. True when `held` is an RCD (or a cartridge, disk or sheets on an RCD).
pub fn item_interaction(
    world: &mut World,
    actor: Actor,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    start(world, actor, held, target, right, false)
}

/// `LAZYLEN(atom.rcd_vals(user, rcd))`: the RCD's current design (a build) works on `target`.
pub fn works_on(world: &World, rcd: Entity, target: Entity) -> bool {
    world
        .get::<Matter>(rcd)
        .is_some_and(|m| rcd_vals(world, m, target, DESIGNS.get(m.design)).is_some())
}

/// `rcd_create` of a base construction console's RCD on `target` at its eye: `actor.item` is
/// the console's RCD, the user's hands are not involved.
pub fn remote_create(world: &mut World, actor: Actor, rcd: Entity, target: Entity) -> bool {
    start(world, actor, rcd, target, false, true)
}

fn start(
    world: &mut World,
    actor: Actor,
    held: Entity,
    target: Entity,
    right: bool,
    remote: bool,
) -> bool {
    // Matter or a disk clicked onto an RCD lying around (`item_interaction`).
    if world.get::<Matter>(target).is_some() && world.get::<Matter>(held).is_none() {
        return load(world, actor.client, target, held);
    }
    let Some(m) = world.get::<Matter>(held).copied() else {
        return false;
    };
    if !right && load(world, actor.client, held, target) {
        return true;
    }
    if !is_rcd(world, held) {
        // The family's own clicks ([`super::rhd`]).
        return false;
    }
    let design = if right { None } else { DESIGNS.get(m.design) };
    // Atoms without `rcd_vals` take nothing (`NOBLUDGEON`: no hit either).
    let Some((delay, cost)) = rcd_vals(world, &m, target, design) else {
        return true;
    };
    // BandaStation `can_place`: deconstruction needs the engineer skillchip.
    let knows = knows_advanced(world, actor.mob, held);
    if design.is_none() && !knows {
        crate::balloon::balloon_alert(world, actor.mob, held, "нет скиллчипа!");
        return true;
    }
    if !use_resource(world, actor.client, held, cost, true) {
        return true;
    }
    if !can_place(world, target, design) {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Что-то мешает на клетке!",
        );
        return true;
    }
    let mut delay = delay * m.delay_mod;
    if !knows {
        delay *= NO_SKILLCHIP_DELAY_MULTIPLIER;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    let effect = spawn_effect(world, pos, delay, design.is_none());
    let job = RcdJob {
        design: design.map(|_| m.design),
        cost,
        effect,
        remote,
    };
    if !do_after(world, actor, target, delay, 0, Job::Rcd(job))
        && let Some(e) = effect
    {
        world.despawn(e);
    }
    true
}

/// `/obj/effect/constructing_effect`: its animation by delay.
pub(crate) fn spawn_effect(
    world: &mut World,
    pos: TilePos,
    delay: f32,
    deconstruct: bool,
) -> Option<Entity> {
    let e = spawn_obj(world, EFFECT, pos)?;
    let shown = if deconstruct { delay - 11.0 } else { delay };
    let state = if shown < 10.0 {
        "rcd_shortest"
    } else if shown < 20.0 {
        "rcd_shorter"
    } else if shown < 37.0 {
        "rcd_short"
    } else if deconstruct {
        "rcd_reverse"
    } else {
        "rcd"
    };
    set_icon_state(world, e, state);
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(e).insert(RcdEffect {
        until: now + f64::from(delay) / 10.0 + 0.5,
    });
    Some(e)
}

fn expire_effects(time: Res<Time>, effects: Query<(Entity, &RcdEffect)>, mut commands: Commands) {
    let now = time.elapsed_secs_f64();
    for (e, effect) in &effects {
        if now >= effect.until {
            commands.entity(e).despawn();
        }
    }
}

/// The do_after's end: matter, design and tile checked again, then `rcd_act` and the matter.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: RcdJob) {
    if let Some(e) = job.effect
        && world.get_entity(e).is_ok()
    {
        world.despawn(e);
    }
    let Some(item) = action.item else { return };
    let Some(m) = world.get::<Matter>(item).copied() else {
        return;
    };
    let client = action.client;
    let target = action.target;
    // `blueprint_changed`: a new design cancels a build.
    if job.design.is_some() && job.design != Some(m.design) {
        return;
    }
    let design = job.design.and_then(|i| DESIGNS.get(i));
    let Some((_, cost)) = rcd_vals(world, &m, target, design) else {
        return;
    };
    if !use_resource(world, client, item, cost, true) {
        return;
    }
    if !can_place(world, target, design) {
        return;
    }
    let dir = user_dir(world, mob);
    if !rcd_act(world, client, target, design, dir, item) {
        return;
    }
    use_resource(world, client, item, cost, false);
    let pos = action.target_pos;
    playsound(world, PlaySound::new(pos, DECONSTRUCT_SOUND, 50.0, true));
    playsound(world, PlaySound::new(pos, CLICK, 50.0, true));
}

fn face(world: &mut World, e: Entity, dir: Dir) {
    world.entity_mut(e).insert(Facing(dir));
    if let Some(mut atom) = world.get_mut::<DmAtom>(e) {
        atom.dir = dir as u8;
    }
}

fn remove(world: &mut World, e: Entity) {
    let pos = world.get::<TilePos>(e).copied();
    world.despawn(e);
    if let Some(pos) = pos {
        tile_changed(world, pos);
    }
}

/// A window of `path` on `pos` (anchored; a directional one facing `dir`).
fn place_window(world: &mut World, client: Entity, pos: TilePos, path: &str, dir: Dir) -> bool {
    let fulltile = matches!(path, F_WINDOW | RF_WINDOW);
    if !fulltile && !direction_free(world, pos, dir as u8) {
        tell(world, client, Span::Warning, "Здесь уже есть окно!");
        return false;
    }
    let Some(w) = spawn_obj(world, path, pos) else {
        return false;
    };
    if !fulltile {
        face(world, w, dir);
    }
    set_anchored(world, w, true);
    true
}

/// `rcd_act` of `target`.
fn rcd_act(
    world: &mut World,
    client: Entity,
    target: Entity,
    design: Option<&Design>,
    dir: Dir,
    rcd: Entity,
) -> bool {
    let Some(path) = path_of(world, target) else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    let is_turf = world.get::<Turf>(target).is_some();
    let Some(d) = design else {
        // Deconstruction.
        if is_turf {
            return scrape_away(world, pos, is_under(&path, FLOOR)).is_some();
        }
        remove(world, target);
        return true;
    };
    match d.mode {
        Mode::Turf if d.path == PLATING_DESIGN => {
            if is_turf && is_under(&path, FLOOR) {
                if let Some(girder) = on_tile_under(world, pos, GIRDER) {
                    world.despawn(girder);
                }
                return place_on_top(world, pos, WALL, false).is_some();
            }
            if is_turf {
                // Space or openspace.
                if let Some(lattice) = on_tile_under(world, pos, LATTICE) {
                    world.despawn(lattice);
                }
                return place_on_top(world, pos, PLATING, true).is_some();
            }
            if is_under(&path, GIRDER) {
                world.despawn(target);
                return place_on_top(world, pos, WALL, false).is_some();
            }
            // A lattice over a groundless turf.
            let groundless = crate::construction::turf_at(world, pos)
                .and_then(|t| path_of(world, t))
                .is_some_and(|p| is_under(&p, SPACE) || is_under(&p, OPENSPACE));
            if !groundless {
                return false;
            }
            world.despawn(target);
            place_on_top(world, pos, PLATING, true).is_some()
        }
        Mode::Turf => {
            // A catwalk replaces the lattice.
            if let Some(lattice) = on_tile_under(world, pos, LATTICE) {
                world.despawn(lattice);
            }
            spawn_obj(world, CATWALK, pos).is_some()
        }
        Mode::WindowGrille => {
            let fulltile = matches!(d.path, F_WINDOW | RF_WINDOW);
            let grille = if is_under(&path, GRILLE) {
                Some(target)
            } else {
                on_tile_under(world, pos, GRILLE)
            };
            if grille.is_some() || !fulltile {
                return place_window(world, client, pos, d.path, dir);
            }
            // The grille for a full-tile window.
            let Some(g) = spawn_obj(world, GRILLE, pos) else {
                return false;
            };
            set_anchored(world, g, true);
            true
        }
        Mode::Airlock => {
            let door_here = world
                .resource::<SpatialIndex>()
                .on_tile(pos)
                .iter()
                .any(|&e| world.get::<Door>(e).is_some());
            if door_here {
                tell(world, client, Span::Warning, "Здесь уже есть дверь!");
                return false;
            }
            let electronics = world
                .get::<AirlockElectronics>(rcd)
                .cloned()
                .unwrap_or_default();
            let def = ss13_mapload::DmDoorAssembly {
                airlock_type: d.path.to_owned(),
                ..Default::default()
            };
            let assembly = DoorAssembly {
                electronics: Some(electronics),
                ..Default::default()
            };
            let e = world.spawn(pos).id();
            crate::airlock_construction::finish_door(world, e, &def, &assembly).is_some()
        }
        Mode::Structure => {
            // One of a kind per tile.
            let base = if is_under(d.path, "/obj/structure/frame/machine") {
                "/obj/structure/frame/machine"
            } else if is_under(d.path, "/obj/structure/frame/computer") {
                "/obj/structure/frame/computer"
            } else {
                d.path
            };
            if on_tile_under(world, pos, base).is_some() {
                return false;
            }
            let Some(new) = spawn_obj(world, d.path, pos) else {
                return false;
            };
            const DIR_TYPES: [&str; 4] = [
                "/obj/structure/chair",
                TABLE,
                "/obj/structure/rack",
                "/obj/structure/bed",
            ];
            if DIR_TYPES.iter().any(|t| is_under(d.path, t)) {
                face(world, new, dir);
            }
            true
        }
    }
}

/// Examine: matter and upgrades.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(m) = world.get::<Matter>(target) else {
        return;
    };
    lines.push((
        Span::Plain,
        format!(
            "В нём {} единиц материи.",
            matter_text(&shown_matter(world, target, m))
        ),
    ));
    let names: Vec<&str> = DISKS
        .iter()
        .filter(|(_, bit, _)| m.upgrades & bit != 0)
        .map(|&(_, _, n)| n)
        .collect();
    if !names.is_empty() {
        lines.push((
            Span::Plain,
            format!("Установлены улучшения: {}.", names.join(", ")),
        ));
    }
    if let Some(design) = DESIGNS.get(m.design).filter(|_| is_rcd(world, target)) {
        lines.push((
            Span::Notice,
            format!("Выбрано: {}.", design_name(world, design)),
        ));
    }
    if item_path(world, target).is_some_and(|p| is_under(p, RCD)) {
        lines.push((
            Span::Plain,
            "Для полноценного использования требуется скиллчип инженера.".to_owned(),
        ));
    }
}

/// BandaStation `check_engineer_skillchip`: `user` has `TRAIT_ADVANCED_RCD_KNOWLEDGE` (the
/// engineer skillchip) or uses an RCD inherently (`can_inherently_use_rcd`: silicons, drones,
/// repair bots), or `rcd` needs no chip.
pub fn knows_advanced(world: &World, user: Entity, rcd: Entity) -> bool {
    if item_path(world, rcd).is_some_and(|p| NO_SKILLCHIP_RCDS.iter().any(|r| is_under(p, r))) {
        return true;
    }
    if world.get::<crate::silicon::Silicon>(user).is_some()
        || crate::skillchips::has_trait(world, user, crate::skillchips::ADVANCED_RCD_KNOWLEDGE)
    {
        return true;
    }
    world.get::<crate::mobs::BasicMob>(user).is_some_and(|b| {
        is_under(&b.def.path, "/mob/living/basic/drone")
            || is_under(&b.def.path, "/mob/living/basic/bot/repairbot")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyborg_rcds_are_cell_powered() {
        assert_eq!(
            cell_powered("/obj/item/construction/rcd/borg"),
            Some(CellPowered(720.0))
        );
        assert_eq!(
            cell_powered("/obj/item/construction/rcd/borg/syndicate"),
            Some(CellPowered(660.0))
        );
        assert_eq!(cell_powered(RCD), None);
        assert!(Matter::for_path("/obj/item/construction/rcd/borg/syndicate").rturf);
    }

    #[test]
    fn a_borg_rcd_draws_on_the_holders_cell() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let borg = world
            .spawn(Cyborg {
                model: None,
                model_item: None,
                charge: 1000.0,
                max_charge: 1000.0,
                low_power: false,
                connected_ai: None,
                lawupdate: false,
            })
            .id();
        let item = world
            .spawn((
                Matter::for_path("/obj/item/construction/rcd/borg"),
                CellPowered(720.0),
                CarriedBy(borg),
            ))
            .id();
        let client = world.spawn_empty().id();
        let cyborg = |w: &World| w.get::<Cyborg>(borg).unwrap().charge;
        // A dry run checks, a real one pays `amount * energyfactor`.
        assert!(use_resource(&mut world, client, item, 1, true));
        assert_eq!(cyborg(&world), 1000.0);
        assert!(use_resource(&mut world, client, item, 1, false));
        assert_eq!(cyborg(&world), 280.0);
        // Not enough charge: refused, nothing taken.
        assert!(!use_resource(&mut world, client, item, 1, false));
        assert_eq!(cyborg(&world), 280.0);
        // Out of a cyborg's hands it has no power at all (tg: `!iscyborg(user)`).
        world.entity_mut(item).remove::<CarriedBy>();
        assert!(!use_resource(&mut world, client, item, 1, true));
    }

    #[test]
    fn variants_follow_tg() {
        let plain = Matter::for_path(RCD);
        assert_eq!((plain.matter, plain.max, plain.upgrades), (0, 160, 0));
        let loaded = Matter::for_path("/obj/item/construction/rcd/loaded");
        assert_eq!(loaded.matter, 160);
        let upgraded = Matter::for_path("/obj/item/construction/rcd/loaded/upgraded");
        assert_eq!(upgraded.upgrades, ALL_UPGRADES);
        let combat = Matter::for_path("/obj/item/construction/rcd/combat");
        assert!(combat.rturf && combat.max == 500);
        let admin = Matter::for_path("/obj/item/construction/rcd/combat/admin");
        assert!(admin.has(u32::MAX - 1) && (admin.delay_mod - 0.1).abs() < 1e-6);
        assert_eq!(admin.upgrades & UPGRADE_SILO_LINK, 0);
        let ce = Matter::for_path("/obj/item/construction/rcd/ce");
        assert_eq!(ce.upgrades, UPGRADE_ANTI_INTERRUPT | UPGRADE_COOLING);
    }

    #[test]
    fn matter_is_used_but_never_below_zero_or_from_infinity() {
        let mut m = Matter::for_path("/obj/item/construction/rcd/loaded");
        assert!(m.has(160) && !m.has(161));
        m.take(16);
        assert_eq!(m.matter, 144);
        let mut admin = Matter::for_path("/obj/item/construction/rcd/combat/admin");
        admin.take(1000);
        assert_eq!(admin.matter, INFINITE);
    }

    #[test]
    fn sheet_matter_follows_matter_amount() {
        assert_eq!(sheet_matter("/obj/item/stack/sheet/iron"), Some(4));
        assert_eq!(sheet_matter("/obj/item/stack/sheet/iron/fifty"), Some(4));
        assert_eq!(sheet_matter("/obj/item/stack/sheet/rglass"), Some(6));
        assert_eq!(sheet_matter("/obj/item/stack/sheet/plasmarglass"), Some(8));
        assert_eq!(sheet_matter("/obj/item/stack/sheet/plasteel"), Some(12));
        assert_eq!(sheet_matter("/obj/item/stack/rods"), Some(2));
        assert_eq!(sheet_matter("/obj/item/stack/tile/iron"), Some(1));
        assert_eq!(sheet_matter("/obj/item/stack/sheet/mineral/gold"), None);
    }

    #[test]
    fn window_and_structure_costs_follow_rcd_vals() {
        assert_eq!(window_cost(WINDOW, false), Some((20.0, 4)));
        assert_eq!(window_cost(RF_WINDOW, false), Some((10.0, 4)), "the grille");
        assert_eq!(window_cost(RF_WINDOW, true), Some((40.0, 12)));
        assert_eq!(window_cost(F_WINDOW, true), Some((30.0, 8)));
        assert_eq!(
            structure_cost("/obj/structure/chair/stool/bar"),
            Some((5.0, 4))
        );
        assert_eq!(
            structure_cost("/obj/structure/chair/stool"),
            Some((10.0, 4))
        );
        assert_eq!(
            structure_cost("/obj/structure/table/glass"),
            Some((20.0, 8))
        );
        assert_eq!(
            structure_cost("/obj/structure/frame/computer/rcd/west"),
            Some((20.0, 20))
        );
    }

    #[test]
    fn upgrade_gated_designs_name_their_disk() {
        for design in DESIGNS {
            if design.path.starts_with("/obj/structure/frame") {
                assert_eq!(design.needs, UPGRADE_FRAMES);
            }
            if design.mode == Mode::Airlock {
                assert_eq!(design.needs, 0);
                assert!(is_under(design.path, AIRLOCK));
            }
        }
        assert_eq!(DESIGNS[0].path, PLATING_DESIGN);
    }
}
