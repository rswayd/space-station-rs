//! Construction (server): what an item in the active hand does to the atom it is clicked on
//! before it is used as a weapon, after tg `base_item_interaction` (`code/game/atom/
//! atom_tool_acts.dm`): `tool_act` (only out of combat mode), the target's `item_interaction`,
//! the item's `interact_with_atom`; then `attackby` ([`crate::obj_damage::attackby`]). A right
//! click (`ClickModifiers::right`) runs the `_secondary` variants.
//!
//! Turfs change in place of their tile (tg `ChangeTurf`, [`change_turf`]): the turf entity is
//! replaced by one of the new type with the tile's baseturf stack ([`Baseturfs`]), contents stay,
//! atmos gets the new tile kind (`ss13_atmos::Atmos::change_turf`, unless the air is inherited
//! between open turfs), lighting keeps the tile's area light, smoothing and undertile hiding
//! are redone around it.
//!
//! The chains, with tg's numbers (delays × `toolspeed`):
//! - floor: crowbar pries the tile off (instant, drops `floor_tile`, `ScrapeAway` to plating);
//!   a floor tile on plating places it (`place_on_top`);
//! - iron sheets, right click on an open turf: 4 s, 2 sheets -> displaced girder (with a circuit
//!   board in the other hand: 2.5 s, 5 sheets -> a frame, [`crate::machine_frame`]; with
//!   airlock electronics: 5 s, 4 sheets -> an airlock assembly,
//!   [`crate::airlock_construction`]); wrench 4 s
//!   secures it (girder) or loosens it again; screwdriver on a displaced girder 4 s -> 2 sheets;
//!   a plasteel sheet on a secured girder 5 s -> reinforced girder; screwdriver 4 s toggles its
//!   struts, wirecutters on loose struts 4 s -> the plasteel back and a plain girder;
//! - wall recipes on a girder on a floor ([`GIRDER_RECIPES`]): 2 iron on a girder 4 s -> wall
//!   (`place_on_top`), 1 plasteel on a reinforced girder 5 s -> reinforced wall, 2 iron or 2
//!   plasteel on a displaced girder 2 s -> a false wall ([`crate::falsewall`]); a lit welder
//!   with 2 fuel on a wall: 10 s -> 2 sheets + girder, the tile scraped to plating; reinforced
//!   walls and windows come apart step by step ([`crate::reinforced`]);
//! - grilles: right click with wirecutters cuts one into its rods; screwdriver toggles its
//!   anchoring; rods rebuild a broken one (1 s); 2 glass sheets on an anchored grille (2 s)
//!   place an unanchored full-tile window;
//! - windows: screwdriver (3 s) frame <-> unscrewed, crowbar out of the frame (10 s) or back
//!   (5 s), screwdriver (3 s) unanchors, wrench (3 s) on an unanchored window -> its glass;
//!   a lit welder repairs it (4 s);
//! - vending machines: screwdriver toggles the panel, wrench with the panel open (6 s)
//!   unanchors or anchors (`default_unfasten_wrench`).
// shortcut: mineral and material wall recipes are not ported (only iron and plasteel); a tile on
// a tiled floor needs a crowbar in the other hand in tg (`try_replace_tile`) and does nothing
// here.

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_atmos::{Atmos, TileKind, TurfAir};
use ss13_mapload::{
    DmAtom, DmItem, DmObj, DmStack, DmTurf, DmTurfBuild, HiddenUnderTile, IconIndex, Template, Turf,
};
use ss13_net::server::Networked;
use ss13_proto::Prototypes;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Optics, TilePos};
use ss13_world::{Density, Mover, SpatialIndex};

use crate::combat::{CombatMode, NextMove};
use crate::explosions::ex_act::{CATWALK, LATTICE};
use crate::health::Health;
use crate::obj_damage::{
    GIRDER, GIRDER_DISPLACED, GIRDER_NORMAL, GIRDER_REINF, GIRDER_REINF_STRUTS, GRILLE, Integrity,
    ObjState, WINDOW, WINDOW_IN_FRAME, WINDOW_OUT_OF_FRAME, WINDOW_SCREWED_TO_FRAME, is_under,
};
use crate::tools::{Actor, DoAfter, Stack, ToolKind, do_after, tell, tool_of, use_tool};

/// `/turf/baseturf_bottom`: the bottom of every baseturf stack, the level's baseturf.
const BASETURF_BOTTOM: &str = "/turf/baseturf_bottom";
/// The level's `ZTRAIT_BASETURF`: space, lava on the mining level (`ZTRAITS_LAVALAND`).
const LEVEL_BASETURF: &str = "/turf/open/space/basic";
const LAVALAND_BASETURF: &str = "/turf/open/lava/smooth/lava_land_surface";
const FLOOR: &str = "/turf/open/floor";
const WALL: &str = "/turf/closed/wall";
const OPEN: &str = "/turf/open";
const IRON: &str = "/obj/item/stack/sheet/iron";
const PLASTEEL: &str = "/obj/item/stack/sheet/plasteel";
const GLASS: &str = "/obj/item/stack/sheet/glass";
const RODS: &str = "/obj/item/stack/rods";
const TILE: &str = "/obj/item/stack/tile";
const DISPLACED_GIRDER: &str = "/obj/structure/girder/displaced";
const REINFORCED_GIRDER: &str = "/obj/structure/girder/reinforced";
const FULLTILE_WINDOW: &str = "/obj/structure/window/fulltile";
const VENDING: &str = "/obj/machinery/vending";
/// `/obj/structure/fans`: airtight fans; the shuttle bay shield cannot be wrenched apart.
const FANS: &str = "/obj/structure/fans";
const TINY_FAN: &str = "/obj/structure/fans/tiny";
const FAN_SHIELD: &str = "/obj/structure/fans/tiny/shield";
const SPACE: &str = "/turf/open/space";
const OPENSPACE: &str = "/turf/open/openspace";
const PLATING: &str = "/turf/open/floor/plating";
/// `ismetaltile`.
const IRON_TILE: &str = "/obj/item/stack/tile/iron";

/// A `GLOB.main_girder_wall_recipes` entry (`create_girder_wall_recipes`): `wall` (a turf or
/// a false wall) from `amount` of `stack` on a `girder` (or subtype) in `state`.
#[derive(Clone, Copy, Debug)]
pub struct GirderRecipe {
    pub wall: &'static str,
    pub stack: &'static str,
    pub amount: u32,
    pub girder: &'static str,
    pub state: i32,
    /// `make_delay`, deciseconds.
    pub delay: f32,
}

/// The iron and plasteel recipes, in tg's order (the first match wins).
pub const GIRDER_RECIPES: [GirderRecipe; 4] = [
    GirderRecipe {
        wall: WALL,
        stack: IRON,
        amount: 2,
        girder: GIRDER,
        state: GIRDER_NORMAL,
        delay: 40.0,
    },
    GirderRecipe {
        wall: crate::reinforced::R_WALL,
        stack: PLASTEEL,
        amount: 1,
        girder: REINFORCED_GIRDER,
        state: GIRDER_REINF,
        delay: 50.0,
    },
    GirderRecipe {
        wall: crate::falsewall::FALSEWALL,
        stack: IRON,
        amount: 2,
        girder: DISPLACED_GIRDER,
        state: GIRDER_DISPLACED,
        delay: 20.0,
    },
    GirderRecipe {
        wall: crate::falsewall::R_FALSEWALL,
        stack: PLASTEEL,
        amount: 2,
        girder: DISPLACED_GIRDER,
        state: GIRDER_DISPLACED,
        delay: 20.0,
    },
];

/// A timed construction step (see the module docs), with the state it started from: the
/// effect applies only if that state still holds (tg `extra_checks`).
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    /// Wrench on a girder: `GIRDER_NORMAL` <-> `GIRDER_DISPLACED`.
    GirderBolts {
        from: i32,
    },
    /// Screwdriver on a displaced girder.
    GirderDisassemble,
    /// Sheets on a girder: the index of its [`GIRDER_RECIPES`] entry.
    Wall {
        recipe: usize,
    },
    /// Plasteel on a secured girder.
    GirderReinforce,
    /// Screwdriver on a reinforced girder: its struts from `from`.
    GirderStruts {
        from: i32,
    },
    /// Wirecutters on a reinforced girder with loose struts.
    GirderGrille,
    /// Reinforced wall and window steps ([`crate::reinforced`]).
    Reinforced(crate::reinforced::ReinforcedJob),
    /// A holosign projector's `creation_time` ([`crate::holosign`]).
    Holosign,
    /// Wrench on fans.
    FanDisassemble,
    /// Welder on a wall.
    SliceWall,
    /// Iron sheets on an open turf (right click).
    BuildGirder,
    /// Screwdriver on a window.
    WindowScrew {
        state: i32,
        anchored: bool,
    },
    /// Crowbar on an anchored window.
    WindowPry {
        state: i32,
    },
    /// Wrench on an unanchored window.
    WindowDisassemble {
        state: i32,
    },
    /// Welder on a damaged window.
    WindowRepair,
    /// Glass sheets on a grille.
    GlassOnGrille,
    /// Rods on a broken grille.
    GrilleRepair,
    /// Rods on an open turf (right click).
    BuildGrille,
    /// Wrench on a vending machine with an open panel.
    Unfasten {
        anchored: bool,
    },
    /// Cuffing, taking cuffs off, breaking out (`crate::restraints`).
    Restraints(crate::restraints::RestraintJob),
    /// A welder on a turret frame (`crate::defenses::turret_build`).
    TurretBuild(crate::defenses::turret_build::TurretBuildJob),
    /// Barricade welds, prying and plank nailing (`crate::defenses::barricades`).
    Barricade(crate::defenses::barricades::BarricadeJob),
    /// Pulling taser electrodes out of yourself (`crate::taser`).
    RemoveElectrodes,
    /// A gun in the mouth or to a head (`crate::guns`, tg `handle_suicide`).
    GunSuicide,
    /// Taking aim at a mob (`crate::gunpoint`).
    Gunpoint,
    /// A medical stack on a living mob ([`crate::medical`]).
    Heal(crate::medical::HealJob),
    /// tg `do_cpr` ([`crate::medical`]).
    Cpr {
        panicking: bool,
    },
    /// An implanter on a mob ([`crate::implants`]).
    Implant,
    /// Feeding, injecting or forcing a pill on someone ([`crate::chem`]).
    Chem(crate::chem::ChemJob),
    /// Eating, feeding or processing food ([`crate::food`]).
    Food(crate::food::FoodJob),
    /// Striking the battery lighter ([`crate::smoking`]).
    Smoking(crate::smoking::SmokingJob),
    /// Digging a plant out of a tray ([`crate::botany`]).
    Botany(crate::botany::BotanyJob),
    /// Welding a vent or scrubber ([`crate::atmos_machinery`]).
    Atmos(crate::atmos_machinery::windows::AtmosJob),
    /// Climbing a ladder ([`crate::multiz`]).
    Climb(crate::multiz::Climb),
    /// Flying a level up or down (PgUp/PgDn, [`crate::multiz`]).
    Fly(crate::multiz::Climb),
    /// Dumping a storage, stuffing a mob into a closet ([`crate::drag`]).
    Interact(crate::drag::DragJob),
    /// Flipping, climbing, taking apart tables and racks ([`crate::tables`]).
    Table(crate::tables::TableJob),
    /// Taking an item off someone or putting one on them ([`crate::strip`]).
    Strip(crate::strip::StripJob),
    /// Wrenching or welding an emitter ([`crate::power::emitter`]).
    Emitter(crate::power::emitter::EmitterJob),
    /// Blood samples, IV drips, surgery, buckling, defibrillators, wound treatments
    /// ([`crate::medical::MedJob`]).
    Medical(crate::medical::MedJob),
    /// Entering and leaving mechs, their equipment ([`crate::mecha`]).
    Mecha(crate::mecha::core::MechJob),
    /// Stuffing a mob into a disposal bin, unwrapping a parcel ([`crate::disposals`]).
    Disposal(crate::disposals::DisposalJob),
    /// Belts, disposal constructs, plastic flaps ([`crate::recycling`]).
    Recycling(crate::recycling::RecyclingJob),
    /// Mopping or scrubbing ([`crate::janitor`]).
    Clean(crate::janitor::CleanJob),
    /// A vampire's bite or lair ritual ([`crate::antag::vampire::server`]).
    Vampire(crate::antag::vampire::server::VampireJob),
    /// A changeling's absorption stages ([`crate::antag::changeling::server`]).
    Changeling(crate::antag::changeling::server::ChangelingJob),
    /// A cultist's rune, its erasure, blood magic carving, shadow shackles
    /// ([`crate::antag::cult::server`]).
    Cult(crate::antag::cult::server::CultJob),
    /// A heretic's transmutation rune, influence drain, Volcano Blast channel
    /// ([`crate::antag::heretic::server`]).
    Heretic(crate::antag::heretic::server::HereticJob),
    /// A DNA injector on someone else ([`crate::genetics::console`]).
    Genetics(crate::genetics::console::InjectJob),
    /// Unlocking a fire axe cabinet ([`crate::wallmount`]).
    WallMount(crate::wallmount::WallJob),
    /// Frames and taking computers apart ([`crate::machine_frame`]).
    Frame(crate::machine_frame::FrameJob),
    /// Airlock assembly steps ([`crate::airlock_construction`]).
    Airlock(crate::airlock_construction::AssemblyJob),
    /// Righting a tilted vendor ([`crate::vending`]).
    Vending(crate::vending::VendJob),
    /// Taping a knife to a cleanbot, wiring and finishing an ED-209 ([`crate::bots`]).
    Bot(crate::bots::BotUserJob),
    /// Stuffing a pulled mob into a gibber ([`crate::gibber`]).
    Gibber(crate::gibber::StuffJob),
    /// Planting a C-4 charge ([`crate::explosions::sources`]).
    Explosive(crate::explosions::sources::ExplosiveJob),
    /// An experi-scanner's scan ([`crate::science::experi_scanner`]).
    Experiment,
    /// Taking a dead slime's cores out ([`crate::science::xenobio`]).
    SlimeCores,
    /// Swabbing an atom for a sample ([`crate::science::cell_lines`]).
    Swab,
    /// Reading a martial arts manual or scroll ([`crate::martial`]).
    Martial(crate::martial::ReadJob),
    /// Welding a camera frame or a camera off the wall ([`crate::defenses::cameras`]).
    Camera(crate::defenses::cameras::CameraJob),
    /// A rite's step at the altar, unpacking a bible into an altar ([`crate::service`]).
    Religion(crate::service::ReligionJob),
    /// The Hippocrates bust's oath, carving a statue ([`crate::service`]).
    Art(crate::service::ArtJob),
    /// Washing at a sink, a shower's valve, a toilet's cistern, a urinal's cap
    /// ([`crate::plumbing`]).
    Sink(crate::plumbing::SinkJob),
    /// Curtains, mirrors, signs, plaques, statues, posters and showcases ([`crate::decor`]).
    Decor(crate::decor::DecorJob),
    /// Razors, lipstick, wiping lips, the dye spray ([`crate::service::barber`]).
    Barber(crate::service::BarberJob),
    /// Taking a picture frame off the wall ([`crate::service`] photos).
    Photo(crate::service::PhotoJob),
    /// Harvesting, uprooting and replanting flora ([`crate::flora`]).
    Flora(crate::flora::FloraJob),
    /// Tripwires ([`crate::traps`]).
    Trap(crate::traps::TrapJob),
    /// Packing and unfolding deployable turrets ([`crate::deployable_turrets`]).
    DeployableTurret(crate::deployable_turrets::TurretJob),
    /// Loading and winding mounted guns ([`crate::cannons`]).
    Cannon(crate::cannons::CannonJob),
    /// The punching bag and the loom ([`crate::gym`]).
    Gym(crate::gym::GymJob),
    /// Aiming a bat at a leg ([`crate::weapons::kneecap`]).
    Kneecap(crate::zone::BodyZone),
    /// Safes ([`crate::safes`], [`crate::secure_safes`]).
    Safe(crate::safes::SafeJob),
    SecureSafe(crate::secure_safes::SecureSafeJob),
    /// Taking an unlit bonfire apart ([`crate::fireplace`]).
    Bonfire,
    /// Screwing, welding or repairing a law rack's module ([`crate::silicon::law_rack`]).
    LawRack(crate::silicon::law_rack::LawRackJob),
    /// AI core construction steps ([`crate::silicon::ai_core`]).
    AiCore(crate::silicon::ai_core::CoreJob),
    /// Taking a shunted AI out of an APC into an intelliCard ([`crate::silicon::aicard`]).
    AiCard(crate::silicon::aicard::CardJob),
    /// A pAI's door jack hack ([`crate::silicon::pai`]).
    Pai(crate::silicon::pai::PaiJob),
    /// MOD construction, AI transfers into and out of MODsuits ([`crate::modsuit`]).
    ModSuit(crate::modsuit::ModJob),
    /// Digging rock, prying an ore box apart ([`crate::mining`]).
    Mining(crate::mining::MiningJob),
    /// Switching a satellite on or off ([`crate::station_goals`]).
    Satellite,
    /// A stage of the emergency shuttle console's hijack ([`crate::shuttle_console_server`]).
    Hijack,
    /// Welding an airlock shut ([`crate::doors`]).
    Door(crate::doors::DoorJob),
    /// Firelock frames, windoor assemblies, mineral doors ([`crate::door_structures`]).
    DoorBuild(crate::door_structures::DoorBuildJob),
    /// Wrenching or welding a field generator ([`crate::power::field_generator`]).
    FieldGenerator(crate::power::field_generator::FieldGenJob),
    /// Laying or cutting a SMES terminal ([`crate::power::smes`]).
    Smes(crate::power::smes::SmesJob),
    /// Repairing an emergency shield projector ([`crate::power::shields`]).
    Shield(crate::power::shields::ShieldJob),
    /// Finishing a circuit shell assembly ([`crate::wiremod::server`]).
    Wiremod(Entity),
    /// Abductor console mashing, science tool marks, baton cuffs
    /// ([`crate::antag::abductor::server`]).
    Abductor(crate::antag::abductor::server::AbductorJob),
    /// One second of an inducer charging this atom's cell ([`crate::inducer`]).
    Inducer(Entity),
    /// Mending shredded clothing ([`crate::clothing::damage`]).
    Clothing(crate::clothing::ClothingJob),
    /// A sledgehammer tearing at a wall ([`crate::weapons::rip_and_tear`]).
    TearWall,
    /// Tipping or righting a cow, a turtle drinking ([`crate::mobs::farm`]).
    Fauna(crate::mobs::farm::FarmJob),
    /// An alien's timed steps ([`crate::antag::xeno`]).
    Xeno(crate::antag::xeno::abilities::XenoJob),
    /// A morph eating a corpse or an item ([`crate::antag::morph`]).
    Morph {
        corpse: bool,
    },
    /// A space dragon's rift, meal, wall tearing or airlock prying ([`crate::antag::space_dragon`]).
    SpaceDragon(crate::antag::space_dragon::DragonJob),
    /// The Grand Ritual and the Scepter of Vendormancy.
    Wizard(crate::antag::wizard::server::WizardJob),
    /// Fixing a Radiation Leak's machine ([`crate::events::radiation_leak`]).
    RadiationLeak,
    /// An anomaly releaser on a core ([`crate::events::anomalies`]).
    AnomalyRelease,
    /// A screwdriver taking the core out of an anomaly-locked MOD module
    /// ([`crate::modsuit::anomalock`]).
    AnomalockRemove {
        module: Entity,
    },
    /// Netpods, hololadders, the quantum console ([`crate::science::bitrunning`]).
    Bitrunning(crate::science::bitrunning::server::BitrunJob),
    /// Pressing a license plate ([`crate::brig::labor`]).
    PlatePress,
    /// An RCD build or deconstruction ([`crate::devices::rcd`]).
    Rcd(crate::devices::rcd::RcdJob),
    /// The RCD family's work ([`crate::devices::rhd`]).
    Rhd(crate::devices::rhd::RhdJob),
    /// A screwdriver respooling an unwound tape ([`crate::devices::taperecorder`]).
    TapeRespool(Entity),
    /// Fixing, prying, building display cases ([`crate::displaycase`]).
    DisplayCase(crate::displaycase::CaseJob),
    /// Mounting, taking down and unscrewing trophy fish mounts ([`crate::fishing::mount`]).
    FishMount(crate::fishing::mount::MountJob),
    /// Linking or unlinking a quantum keycard ([`crate::science::quantum_pad`]).
    QuantumPad(crate::science::quantum_pad::PadJob),
    /// Door seals, table clock repairs, hand gestures ([`crate::devices`]).
    Device(crate::devices::DeviceJob),
    /// Deploying a flatpack ([`crate::eng_machines`]).
    EngMachine(crate::eng_machines::EngJob),
    /// Wrenching a light fixture frame apart ([`crate::power::lighting`]).
    Light(crate::power::lighting::LightJob),
    /// Drawing, outlining or eating with a crayon ([`crate::crayons`]).
    Crayon(crate::crayons::CrayonJob),
    /// The Heimlich, washing slime off ([`crate::afflictions`]).
    Affliction(crate::afflictions::AfflictionJob),
    /// Shaking ants off, taking a strand off ([`crate::curses`]).
    Curse(crate::curses::CurseJob),
    /// Climbing into or out of a vent ([`crate::ventcrawl`]).
    Ventcrawl(crate::ventcrawl::VentJob),
    /// A stack crafting menu's recipe ([`crate::stack_recipes`]).
    StackRecipe(crate::stack_recipes::StackJob),
    /// Cutting the hair off a hide ([`crate::sheets`]).
    Sheet(crate::sheets::SheetJob),
}

/// A turf's baseturf stack, bottom first (tg `baseturfs`). Absent: assembled from the type's
/// `baseturfs` chain (`assemble_baseturfs`).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Baseturfs(pub Vec<String>);

/// tg `panel_open` of a machine.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelOpen(pub bool);

/// Whether the maintenance panel of `e` is open.
pub fn panel_open(world: &World, e: Entity) -> bool {
    world.get::<PanelOpen>(e).is_some_and(|p| p.0)
}

/// Templates of the types construction spawns, built on first use.
#[derive(Resource, Default)]
pub(crate) struct BuildTemplates(HashMap<String, Option<Template>>);

pub struct ConstructionServerPlugin;

impl Plugin for ConstructionServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_plugins((
            crate::stack_recipes::StackRecipesPlugin,
            crate::sheets::SheetsPlugin,
        ));
        // tg `/obj/item/stack/attack_hand` from the other hand: one unit into the empty active
        // hand.
        app.add_click_handler(ClickPhase::OtherHand, 1700, |w, c| {
            c.held.is_none()
                && crate::construction::split_into_hand(w, c.mob, c.target, c.active_hand)
        });
        // tg `attack_hand` of a false wall: open or close it.
        app.add_click_handler(ClickPhase::EmptyHand, 2800, |w, c| {
            crate::falsewall::attack_hand(w, c.target)
        });
        // Frames, and screwdrivers and crowbars on machines that come apart
        // (`crate::machine_frame`): tool acts come before the machines' own handlers.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1600, |w, c| {
            c.held.is_some_and(|h| {
                crate::machine_frame::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // tg `base_item_interaction` and `attackby` come before storage and dropping.
        app.add_click_handler(ClickPhase::HeldOnTarget, 4500, |w, c| {
            c.held.is_some_and(|h| {
                crate::construction::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        app.init_resource::<BuildTemplates>()
            .add_plugins(crate::holosign::HolosignServerPlugin)
            .add_systems(
                Update,
                (
                    crate::falsewall::finish_moving
                        .run_if(any_with_component::<crate::falsewall::FalseWallMoving>),
                    crate::reinforced::cool_bolts
                        .run_if(any_with_component::<crate::reinforced::BoltsCooling>),
                ),
            );
    }
}

/// What an interaction step did (tg `ITEM_INTERACT_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Interact {
    /// Not handled: the next step runs.
    None,
    /// Handled (succeeded, started or refused): the chain stops.
    Done,
}

/// tg `melee_attack_chain` of `held` on `target` (not a mob) up to the attack. True when it
/// was handled; false lets the click do what it does otherwise (put the item down).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    if world.get::<Health>(target).is_some() {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<NextMove>(mob).is_some_and(|n| now < n.0) {
        // tg `ClickOn`: nothing during the click cooldown.
        return true;
    }
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let mut result = Interact::None;
    if !combat && let Some((kind, _)) = tool_of(world, held) {
        result = if modifiers.right {
            tool_act_secondary(world, actor, kind, target)
        } else {
            tool_act(world, actor, kind, target)
        };
    }
    if result == Interact::None {
        result = if modifiers.right {
            interact_secondary(world, actor, target)
        } else {
            target_interaction(world, actor, target)
        };
    }
    match result {
        Interact::Done => true,
        Interact::None => crate::obj_damage::attackby(world, actor, target),
    }
}

pub(crate) fn path_of(world: &World, e: Entity) -> Option<String> {
    let entity = world.get_entity(e).ok()?;
    entity
        .get::<DmObj>()
        .map(|o| o.path.clone())
        .or_else(|| entity.get::<DmTurfBuild>().map(|t| t.path.clone()))
        .or_else(|| entity.get::<DmItem>().map(|i| i.path.clone()))
}

fn obj_state(world: &World, e: Entity) -> i32 {
    world.get::<ObjState>(e).map_or(0, |s| s.0)
}

use crate::obj_damage::anchored;

/// The item is a stack of `root` (or a subtype).
fn stack_of(world: &World, item: Entity, root: &str) -> bool {
    world.get::<Stack>(item).is_some()
        && world
            .get::<DmItem>(item)
            .is_some_and(|i| is_under(&i.path, root))
}

/// tg `tool_act` (left click).
fn tool_act(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> Interact {
    let Some(path) = path_of(world, target) else {
        return Interact::None;
    };
    // The vendor board's brand dial.
    if kind == ToolKind::Screwdriver
        && crate::vending::refill::screwdriver_board(world, actor, target)
    {
        return Interact::Done;
    }
    let client = actor.client;
    if world.get::<Turf>(target).is_some() {
        // `/turf/open/floor/crowbar_act`: pry the tile.
        let overfloor = world
            .get::<DmTurfBuild>(target)
            .is_some_and(|t| t.overfloor_placed);
        if kind == ToolKind::Crowbar && is_under(&path, FLOOR) && overfloor {
            pry_tile(world, actor, target);
            return Interact::Done;
        }
        return Interact::None;
    }
    if is_under(&path, GIRDER) {
        return girder_tool(world, actor, kind, target);
    }
    if is_under(&path, WINDOW) {
        return window_tool(world, actor, kind, target);
    }
    if is_under(&path, LATTICE) && kind == ToolKind::Wirecutters {
        // `/obj/structure/lattice/wirecutter_act`.
        let indestructible = world
            .get::<DmObj>(target)
            .is_some_and(|o| o.resistance_flags & crate::obj_damage::INDESTRUCTIBLE != 0);
        if indestructible {
            return Interact::None;
        }
        tell(
            world,
            client,
            Span::Notice,
            "Вы разрезаете соединения решётки...",
        );
        crate::obj_damage::deconstruct(world, target, true);
        return Interact::Done;
    }
    if is_under(&path, GRILLE) && kind == ToolKind::Screwdriver {
        // `/obj/structure/grille/screwdriver_act`: `use_tool` without delay.
        let now = !anchored(world, target);
        crate::obj_damage::set_anchored(world, target, now);
        let text = if now {
            "Вы прикручиваете решётку к полу."
        } else {
            "Вы откручиваете решётку от пола."
        };
        tell(world, client, Span::Notice, text);
        return Interact::Done;
    }
    if is_under(&path, VENDING) {
        return vending_tool(world, actor, kind, target);
    }
    if crate::falsewall::tool_act(world, actor, kind, target) {
        return Interact::Done;
    }
    if kind == ToolKind::Wrench && is_under(&path, FANS) && !is_under(&path, FAN_SHIELD) {
        // `/obj/structure/fans/wrench_act`.
        if use_tool(world, actor, target, 20.0, 0, Job::FanDisassemble) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете разбирать вентилятор...",
            );
        }
        return Interact::Done;
    }
    if crate::power::emitter::tool_act(world, actor, kind, target)
        || crate::power::field_generator::tool_act(world, actor, kind, target)
    {
        return Interact::Done;
    }
    // Airlock and APC `screwdriver_act`: the wire panel.
    if crate::wires::screwdriver_act(world, actor, kind, target) {
        return Interact::Done;
    }
    // Airlock `welder_act`, `crowbar_act`, `wirecutter_act` on its plating.
    if crate::airlock_security::tool_act(world, actor, kind, target) {
        return Interact::Done;
    }
    if kind == ToolKind::Crowbar
        && (crate::wallmount::firelock::crowbar_act(world, actor, target, false)
            || crate::doors::force_open_act(world, actor, target)
            || crate::doors::crowbar_act(world, client, target))
    {
        return Interact::Done;
    }
    Interact::None
}

/// tg `*_act_secondary` (right click).
fn tool_act_secondary(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> Interact {
    let Some(path) = path_of(world, target) else {
        return Interact::None;
    };
    if is_under(&path, GRILLE) && kind == ToolKind::Wirecutters {
        // `/obj/structure/grille/wirecutter_act_secondary`.
        // shortcut: no shocks from powered grilles yet.
        crate::obj_damage::deconstruct(world, target, true);
        tell(
            world,
            actor.client,
            Span::Notice,
            "Вы перекусываете решётку.",
        );
        return Interact::Done;
    }
    if kind == ToolKind::Crowbar
        && crate::wallmount::firelock::crowbar_act(world, actor, target, true)
    {
        return Interact::Done;
    }
    // Airlock `welder_act_secondary`: welding it shut.
    if kind == ToolKind::Welder && crate::doors::welder_act_secondary(world, actor, target) {
        return Interact::Done;
    }
    if crate::reinforced::window_secondary(world, actor, kind, target) {
        return Interact::Done;
    }
    // `wrench_act_secondary` of chairs and beds.
    if kind == ToolKind::Wrench && crate::furniture::wrench_secondary(world, target) {
        return Interact::Done;
    }
    Interact::None
}

fn pry_tile(world: &mut World, actor: Actor, turf: Entity) {
    let Some(&pos) = world.get::<TilePos>(turf) else {
        return;
    };
    let tile = world
        .get::<DmTurfBuild>(turf)
        .map(|t| t.floor_tile.clone())
        .unwrap_or_default();
    // `remove_tile` -> `make_plating` (`ScrapeAway`, air inherited).
    if scrape_away(world, pos, true).is_none() {
        return;
    }
    tell(world, actor.client, Span::Notice, "Вы снимаете плитку.");
    if !tile.is_empty() {
        spawn_stack(world, &tile, pos, 1);
    }
}

fn girder_tool(world: &mut World, actor: Actor, kind: ToolKind, girder: Entity) -> Interact {
    let state = obj_state(world, girder);
    let client = actor.client;
    let can_displace = world.get::<DmObj>(girder).is_some_and(|o| o.can_displace);
    match kind {
        ToolKind::Wrench if !can_displace => {
            tell(world, client, Span::Warning, "Здесь нет болтов!");
        }
        ToolKind::Wrench if matches!(state, GIRDER_NORMAL | GIRDER_DISPLACED) => {
            let text = if state == GIRDER_NORMAL {
                "Вы начинаете откручивать каркас..."
            } else {
                "Вы начинаете закреплять каркас..."
            };
            if use_tool(
                world,
                actor,
                girder,
                40.0,
                0,
                Job::GirderBolts { from: state },
            ) {
                tell(world, client, Span::Notice, text);
            }
        }
        ToolKind::Screwdriver if state == GIRDER_DISPLACED => {
            if use_tool(world, actor, girder, 40.0, 0, Job::GirderDisassemble) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете разбирать каркас...",
                );
            }
        }
        ToolKind::Screwdriver if matches!(state, GIRDER_REINF | GIRDER_REINF_STRUTS) => {
            let text = if state == GIRDER_REINF {
                "Вы начинаете откручивать опорные стойки..."
            } else {
                "Вы начинаете закреплять опорные стойки..."
            };
            if use_tool(
                world,
                actor,
                girder,
                40.0,
                0,
                Job::GirderStruts { from: state },
            ) {
                tell(world, client, Span::Notice, text);
            }
        }
        ToolKind::Wirecutters if state == GIRDER_REINF_STRUTS => {
            if use_tool(world, actor, girder, 40.0, 0, Job::GirderGrille) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете вынимать внутреннюю решётку...",
                );
            }
        }
        ToolKind::Welder => {
            tell(world, client, Span::Warning, "Это не разварить!");
        }
        ToolKind::Wrench | ToolKind::Screwdriver | ToolKind::Wirecutters => {}
        _ => return Interact::None,
    }
    // Every girder tool act is `ITEM_INTERACT_BLOCKING` at least.
    Interact::Done
}

fn window_tool(world: &mut World, actor: Actor, kind: ToolKind, window: Entity) -> Interact {
    let Some(obj) = world.get::<DmObj>(window).cloned() else {
        return Interact::None;
    };
    let client = actor.client;
    // shortcut: `can_be_reached` (a directional window behind another blocker) is not checked.
    let state = obj_state(world, window);
    let anchored = anchored(world, window);
    if obj.reinf {
        match kind {
            // `/obj/structure/window/reinforced/crowbar_act` replaces the plain one.
            ToolKind::Crowbar => {
                return if crate::reinforced::window_crowbar(world, actor, window) {
                    Interact::Done
                } else {
                    Interact::None
                };
            }
            // The plain `screwdriver_act` has no case for the `RWINDOW_*` states.
            ToolKind::Screwdriver if state != WINDOW_OUT_OF_FRAME => return Interact::Done,
            ToolKind::Wrench if state >= crate::reinforced::RWINDOW_FRAME_BOLTED => {
                return Interact::None;
            }
            _ => {}
        }
    }
    let speed = obj.decon_speed;
    match kind {
        ToolKind::Welder => {
            let full = world
                .get::<Integrity>(window)
                .is_none_or(|i| i.current >= obj.max_integrity);
            if full {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Окно и так в хорошем состоянии!",
                );
            } else if use_tool(world, actor, window, 40.0, 0, Job::WindowRepair) {
                tell(world, client, Span::Notice, "Вы начинаете чинить окно...");
            }
            Interact::Done
        }
        ToolKind::Screwdriver => {
            let text = match (state, anchored) {
                (WINDOW_SCREWED_TO_FRAME, _) => "Вы начинаете откручивать окно от рамы...",
                (WINDOW_IN_FRAME, _) => "Вы начинаете прикручивать окно к раме...",
                (_, true) => "Вы начинаете откручивать раму от пола...",
                (_, false) => "Вы начинаете прикручивать раму к полу...",
            };
            if use_tool(
                world,
                actor,
                window,
                speed,
                0,
                Job::WindowScrew { state, anchored },
            ) {
                tell(world, client, Span::Notice, text);
            }
            Interact::Done
        }
        ToolKind::Wrench if !anchored => {
            if use_tool(
                world,
                actor,
                window,
                speed,
                0,
                Job::WindowDisassemble { state },
            ) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете разбирать окно...",
                );
            }
            Interact::Done
        }
        ToolKind::Crowbar if anchored && matches!(state, WINDOW_IN_FRAME | WINDOW_OUT_OF_FRAME) => {
            let (delay, text) = if state == WINDOW_IN_FRAME {
                (100.0, "Вы начинаете выдвигать окно из рамы...")
            } else {
                (50.0, "Вы начинаете вдвигать окно обратно в раму...")
            };
            if use_tool(world, actor, window, delay, 0, Job::WindowPry { state }) {
                tell(world, client, Span::Notice, text);
            }
            Interact::Done
        }
        _ => Interact::None,
    }
}

fn vending_tool(world: &mut World, actor: Actor, kind: ToolKind, machine: Entity) -> Interact {
    let client = actor.client;
    let anchored = anchored(world, machine);
    let open = world.get::<PanelOpen>(machine).is_some_and(|p| p.0);
    match kind {
        // `default_deconstruction_screwdriver`, only while anchored.
        ToolKind::Screwdriver if anchored => {
            world.entity_mut(machine).insert(PanelOpen(!open));
            let text = if open {
                "панель обслуживания закрыта"
            } else {
                "панель обслуживания открыта"
            };
            tell(world, client, Span::Notice, text);
        }
        ToolKind::Screwdriver => {
            tell(world, client, Span::Warning, "Сначала закрепите автомат.");
        }
        ToolKind::Wrench if !open => {}
        ToolKind::Wrench => {
            if !anchored && blocked_for(world, machine) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вам не удалось прикрутить автомат.",
                );
            } else if use_tool(world, actor, machine, 60.0, 0, Job::Unfasten { anchored }) {
                let text = if anchored {
                    "Вы начинаете откреплять автомат..."
                } else {
                    "Вы начинаете прикреплять автомат..."
                };
                tell(world, client, Span::Notice, text);
            }
        }
        _ => return Interact::None,
    }
    Interact::Done
}

/// tg `is_blocked_turf(exclude_mobs = TRUE, source_atom = obj)` of the obj's own tile.
pub(crate) fn blocked_for(world: &World, obj: Entity) -> bool {
    let Some(&pos) = world.get::<TilePos>(obj) else {
        return true;
    };
    world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&e| e != obj && world.get::<Mover>(e).is_none() && world.get::<Turf>(e).is_none())
}

/// The target's `item_interaction` and the item's `interact_with_atom` (left click).
fn target_interaction(world: &mut World, actor: Actor, target: Entity) -> Interact {
    let Some(held) = actor.item else {
        return Interact::None;
    };
    // `is_wire_tool(tool) && panel_open`: the wires window.
    if crate::wires::wire_tool_interaction(world, actor, held, target) {
        return Interact::Done;
    }
    // Airlock `item_interaction`: sheets reinforce an open, bare panel.
    if crate::airlock_security::item_interaction(world, actor, held, target) {
        return Interact::Done;
    }
    if crate::holosign::interact_with_atom(world, actor, held, target) {
        return Interact::Done;
    }
    let Some(path) = path_of(world, target) else {
        return Interact::None;
    };
    // `/obj/machinery/vending/item_interaction`: canisters, IDs, items back in.
    if is_under(&path, VENDING) {
        return if crate::vending::item_interaction(world, actor, held, target) {
            Interact::Done
        } else {
            Interact::None
        };
    }
    let client = actor.client;
    // `/obj/structure/lattice/item_interaction`: handed to the turf under it.
    if is_under(&path, LATTICE)
        && let Some(turf) = world
            .get::<TilePos>(target)
            .and_then(|&p| turf_at(world, p))
        && build_over_space(world, actor, held, turf)
    {
        return Interact::Done;
    }
    if world.get::<Turf>(target).is_some() {
        if build_over_space(world, actor, held, target) {
            return Interact::Done;
        }
        if is_under(&path, crate::reinforced::R_WALL) && tool_of(world, held).is_some() {
            crate::reinforced::wall_tool(world, actor, target);
            return Interact::Done;
        }
        if is_under(&path, WALL) && tool_of(world, held).map(|t| t.0) == Some(ToolKind::Welder) {
            return slice_wall(world, actor, target);
        }
        if is_under(&path, FLOOR) && stack_of(world, held, TILE) {
            let overfloor = world
                .get::<DmTurfBuild>(target)
                .is_some_and(|t| t.overfloor_placed);
            if !overfloor {
                place_tile(world, held, target);
            }
            return Interact::Done;
        }
        return Interact::None;
    }
    if is_under(&path, GIRDER) && world.get::<Stack>(held).is_some() {
        return girder_stack(world, actor, held, target);
    }
    if is_under(&path, GRILLE) {
        let broken = world.get::<Integrity>(target).is_some_and(|i| i.broken);
        if broken && stack_of(world, held, RODS) {
            do_after(world, actor, target, 10.0, 1, Job::GrilleRepair);
            return Interact::Done;
        }
        if !broken && stack_of(world, held, GLASS) {
            if crate::tools::stack_amount(world, held) < 2 {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Нужно как минимум два листа стекла!",
                );
            } else if !anchored(world, target) {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Сначала прикрутите решётку к полу!",
                );
            } else if window_on(world, target) {
                tell(world, client, Span::Warning, "Здесь уже есть окно!");
            } else if do_after(world, actor, target, 20.0, 2, Job::GlassOnGrille) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете устанавливать окно...",
                );
            }
            return Interact::Done;
        }
        return Interact::None;
    }
    // `/obj/item/stack/item_interaction`: the clicked stack merges into the held one.
    if world.get::<Stack>(target).is_some() && can_merge(world, target, held) {
        merge_into(world, target, held);
        let total = crate::tools::stack_amount(world, held);
        tell(
            world,
            client,
            Span::Notice,
            format!("В стопке теперь {total} шт."),
        );
        return Interact::Done;
    }
    Interact::None
}

/// The item's `interact_with_atom_secondary` (right click).
fn interact_secondary(world: &mut World, actor: Actor, target: Entity) -> Interact {
    let Some(held) = actor.item else {
        return Interact::None;
    };
    let (Some(path), Some(&pos)) = (path_of(world, target), world.get::<TilePos>(target)) else {
        return Interact::None;
    };
    if world.get::<Turf>(target).is_none() || !is_under(&path, OPEN) {
        return Interact::None;
    }
    let client = actor.client;
    let groundless = world.get::<DmTurf>(target).is_some_and(|t| t.groundless);
    if stack_of(world, held, IRON) {
        // `/obj/item/stack/sheet/iron/interact_with_atom_secondary`.
        if groundless {
            tell(world, client, Span::Warning, "Сюда это не поставить!");
        } else if tile_blocked(world, pos) {
            tell(world, client, Span::Warning, "Что-то загораживает клетку!");
        } else {
            // A board in the other hand makes it a machine or computer frame instead.
            // Airlock electronics in the other hand: an airlock assembly.
            if crate::airlock_construction::electronics_in_other_hand(world, actor.mob, held) {
                crate::airlock_construction::build_assembly(world, actor, target, pos);
                return Interact::Done;
            }
            // Firelock circuitry in the other hand: a firelock frame.
            if crate::door_structures::try_build_frame(world, actor, target, pos) {
                return Interact::Done;
            }
            let frame = crate::machine_frame::held_board_frame(world, actor.mob, held);
            let (cost, delay, job) = match frame {
                Some(computer) => (
                    crate::machine_frame::FRAME_SHEETS,
                    crate::machine_frame::BUILD_DS,
                    Job::Frame(crate::machine_frame::FrameJob::Build { computer }),
                ),
                None => (2, 40.0, Job::BuildGirder),
            };
            if crate::tools::stack_amount(world, held) < cost {
                let sheets = if cost < 5 {
                    "листа"
                } else {
                    "листов"
                };
                let text = format!("Нужно {cost} {sheets} металла!");
                tell(world, client, Span::Warning, text);
            } else {
                do_after(world, actor, target, delay, cost, job);
            }
        }
        return Interact::Done;
    }
    // Reinforced glass with airlock electronics in the other hand: a windoor assembly.
    if stack_of(world, held, "/obj/item/stack/sheet/rglass")
        && !groundless
        && crate::door_structures::try_build_windoor_assembly(world, actor, pos)
    {
        return Interact::Done;
    }
    if stack_of(world, held, RODS) {
        // shortcut: tg builds grilles from the rods' crafting menu (`rod_recipes`: 2 rods, 1 s,
        // one per turf, on solid ground); a right click on the floor stands in for it.
        if groundless || grille_on(world, pos) {
            tell(
                world,
                client,
                Span::Warning,
                "Здесь нельзя построить решётку!",
            );
        } else if crate::tools::stack_amount(world, held) < 2 {
            tell(world, client, Span::Warning, "Нужно 2 стержня!");
        } else {
            do_after(world, actor, target, 10.0, 2, Job::BuildGrille);
        }
        return Interact::Done;
    }
    Interact::None
}

/// `/turf/open/space/item_interaction` and the openspace one: rods build a lattice, or a
/// catwalk over a lattice (`build_with_rods`, one rod each); an iron tile over a lattice or
/// next to a floor builds plating (`build_with_floor_tiles`). True when handled.
// shortcut: `CanBuildHere` (shuttle areas) and openspace's `CanCoverUp` are not checked.
fn build_over_space(world: &mut World, actor: Actor, held: Entity, turf: Entity) -> bool {
    let (Some(path), Some(&pos)) = (path_of(world, turf), world.get::<TilePos>(turf)) else {
        return false;
    };
    if !is_under(&path, SPACE) && !is_under(&path, OPENSPACE) {
        return false;
    }
    let client = actor.client;
    let lattice = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter_map(|e| Some((e, world.get::<DmObj>(e)?.path.clone())))
        .find(|(_, p)| is_under(p, LATTICE));
    if stack_of(world, held, RODS) {
        match lattice {
            Some((_, p)) if is_under(&p, CATWALK) => {
                tell(world, client, Span::Warning, "Здесь уже есть мостик!");
            }
            Some((old, _)) => {
                if has_template(world, CATWALK) && crate::tools::use_stack(world, held, 1) {
                    world.despawn(old);
                    spawn_obj(world, CATWALK, pos);
                    tell(world, client, Span::Notice, "Вы собираете мостик.");
                }
            }
            None => {
                if has_template(world, LATTICE) && crate::tools::use_stack(world, held, 1) {
                    spawn_obj(world, LATTICE, pos);
                    tell(world, client, Span::Notice, "Вы собираете решётку.");
                }
            }
        }
        return true;
    }
    if stack_of(world, held, IRON_TILE) {
        // `has_valid_support`: a floor turf on a cardinal side.
        let supported = ss13_shared::Dir::CARDINALS.iter().any(|&d| {
            turf_at(world, pos.step(d))
                .and_then(|t| path_of(world, t))
                .is_some_and(|p| is_under(&p, FLOOR))
        });
        if lattice.is_none() && !supported {
            tell(
                world,
                client,
                Span::Warning,
                "Нужна опора, проложите прутья!",
            );
        } else if has_template(world, PLATING) && crate::tools::use_stack(world, held, 1) {
            if let Some((old, _)) = lattice {
                world.despawn(old);
            }
            place_on_top(world, pos, PLATING, true);
        }
        return true;
    }
    false
}

/// `/turf/closed/wall/try_decon`.
fn slice_wall(world: &mut World, actor: Actor, wall: Entity) -> Interact {
    let client = actor.client;
    let item = actor.item.expect("checked by the caller");
    let slicing = world
        .get::<DmTurfBuild>(wall)
        .map_or(100.0, |t| t.slicing_duration);
    // `tool_start_check(amount = round(slicing_duration / 50), heat_required)`.
    let fuel = (slicing / 50.0).round() as u32;
    if !crate::tools::tool_usable(world, item, fuel, crate::tools::HIGH_TEMPERATURE_REQUIRED) {
        let text = crate::tools::unusable_message(world, item, fuel);
        tell(world, client, Span::Warning, text);
        return Interact::None;
    }
    if use_tool(world, actor, wall, slicing, 0, Job::SliceWall) {
        tell(
            world,
            client,
            Span::Notice,
            "Вы начинаете вгрызаться во внешнюю обшивку...",
        );
    }
    Interact::Done
}

/// `/obj/item/stack/tile/place_tile` on plating.
fn place_tile(world: &mut World, tile: Entity, plating: Entity) {
    let Some(&pos) = world.get::<TilePos>(plating) else {
        return;
    };
    let Some(turf) = world
        .get::<DmStack>(tile)
        .map(|s| s.turf_type.clone())
        .filter(|t| !t.is_empty())
    else {
        return;
    };
    if !has_template(world, &turf) || !crate::tools::use_stack(world, tile, 1) {
        return;
    }
    place_on_top(world, pos, &turf, true);
}

/// `uses_girder_wall_recipes` for a stack on a girder, then the girder's own
/// `item_interaction` (plasteel reinforces a secured girder).
fn girder_stack(world: &mut World, actor: Actor, stack: Entity, girder: Entity) -> Interact {
    let client = actor.client;
    let Some(girder_path) = path_of(world, girder) else {
        return Interact::None;
    };
    let state = obj_state(world, girder);
    let stack_path = world
        .get::<DmItem>(stack)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if let Some(index) = girder_recipe(&stack_path, &girder_path, state) {
        let recipe = GIRDER_RECIPES[index];
        if !wall_recipe_ok(world, actor, girder, &recipe) {
            return Interact::Done;
        }
        let job = Job::Wall { recipe: index };
        if use_tool(world, actor, girder, recipe.delay, recipe.amount, job) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете строить стену на каркасе.",
            );
        }
        return Interact::Done;
    }
    if stack_of(world, stack, PLASTEEL) {
        // `/obj/structure/girder/item_interaction`: not in combat mode, a secured girder only.
        let combat = world.get::<CombatMode>(actor.mob).is_some_and(|c| c.0);
        if combat {
            return Interact::None;
        }
        if state == GIRDER_NORMAL && use_tool(world, actor, girder, 50.0, 1, Job::GirderReinforce) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете укреплять каркас...",
            );
        }
        return Interact::Done;
    }
    if stack_of(world, stack, IRON) {
        tell(world, client, Span::Warning, "Нужен другой каркас!");
        return Interact::Done;
    }
    // shortcut: material and mineral recipes are not ported.
    tell(world, client, Span::Warning, "Неподходящий материал!");
    Interact::Done
}

/// `get_main_recipe`: the first recipe for this stack on this girder in its state.
fn girder_recipe(stack: &str, girder: &str, state: i32) -> Option<usize> {
    GIRDER_RECIPES
        .iter()
        .position(|r| is_under(stack, r.stack) && is_under(girder, r.girder) && r.state == state)
}

/// `check_recipe`: no wall (or false wall) yet, a floor under the girder, the girder still in
/// the recipe's state.
fn wall_recipe_ok(world: &mut World, actor: Actor, girder: Entity, recipe: &GirderRecipe) -> bool {
    let Some(&pos) = world.get::<TilePos>(girder) else {
        return false;
    };
    let turf = turf_at(world, pos).and_then(|t| path_of(world, t));
    let falsewall = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world
                .get::<DmObj>(e)
                .is_some_and(|o| is_under(&o.path, crate::falsewall::FALSEWALL))
        });
    let text = match turf {
        Some(t) if is_under(&t, WALL) => "Здесь уже есть стена!",
        Some(_) if falsewall => "Здесь уже есть стена!",
        Some(t) if !is_under(&t, FLOOR) => "Нужен пол!",
        None => "Нужен пол!",
        Some(_) if obj_state(world, girder) != recipe.state => return false,
        Some(_) => return true,
    };
    tell(world, actor.client, Span::Warning, text);
    false
}

/// tg `is_blocked_turf()`: something dense on the tile (the turf itself included).
pub(crate) fn tile_blocked(world: &World, pos: TilePos) -> bool {
    !world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .is_empty()
}

fn window_on(world: &World, grille: Entity) -> bool {
    let Some(&pos) = world.get::<TilePos>(grille) else {
        return false;
    };
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world
                .get::<DmObj>(e)
                .is_some_and(|o| is_under(&o.path, WINDOW))
        })
}

fn grille_on(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world
                .get::<DmObj>(e)
                .is_some_and(|o| is_under(&o.path, GRILLE))
        })
}

/// tg `can_merge`: same `merge_type` on both sides.
pub fn can_merge(world: &World, a: Entity, b: Entity) -> bool {
    let merge_type = |e: Entity| {
        let stack = world.get::<DmStack>(e)?;
        let path = &world.get::<DmItem>(e)?.path;
        Some(if stack.merge_type.is_empty() {
            path.clone()
        } else {
            stack.merge_type.clone()
        })
    };
    a != b && merge_type(a).is_some() && merge_type(a) == merge_type(b)
}

/// tg `/obj/item/stack/sheet` `manufactured`: sheets a material container made (the ORM, a
/// lathe's ejected sheets); only these earn labor camp points.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Manufactured;

/// tg `merge`: as much of `from` as fits moves into `into`; an emptied `from` is deleted.
pub fn merge_into(world: &mut World, from: Entity, into: Entity) {
    let max = world.get::<DmStack>(into).map_or(50, |s| s.max_amount);
    let have = crate::tools::stack_amount(world, into);
    let moved = crate::tools::stack_amount(world, from).min(max.saturating_sub(have));
    if moved == 0 {
        return;
    }
    // `sheet/merge`: the stack stays manufactured only when both were.
    if world.get::<Manufactured>(from).is_none() {
        world.entity_mut(into).remove::<Manufactured>();
    }
    crate::tools::use_stack(world, from, moved);
    if let Some(mut stack) = world.get_mut::<Stack>(into) {
        stack.amount += moved;
    }
}

/// The job's effect, if its state still holds (tg `do_after` returned true, `extra_checks`
/// passed, `use(amount)` succeeded).
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter) {
    let target = action.target;
    if world.get_entity(target).is_err() {
        return;
    }
    let client = action.client;
    let item = action.item;
    let pos = action.target_pos;
    let path = path_of(world, target).unwrap_or_default();
    let consume =
        |world: &mut World| item.is_none_or(|i| crate::tools::consume(world, i, action.amount));
    match &action.job {
        &Job::Restraints(job) => crate::restraints::complete(world, mob, action, job),
        &Job::TurretBuild(job) => crate::defenses::turret_build::complete(world, action, job),
        &Job::Barricade(job) => crate::defenses::barricades::complete(world, action, job),
        Job::RemoveElectrodes => crate::taser::complete_removal(world, mob),
        Job::GunSuicide => crate::guns::complete_suicide(world, mob, action),
        Job::Gunpoint => crate::gunpoint::complete_aim(world, mob, action),
        Job::Heal(job) => crate::medical::complete_heal(world, mob, action, *job),
        Job::Cpr { panicking } => crate::medical::complete_cpr(world, mob, action, *panicking),
        Job::Chem(job) => crate::chem::complete(world, mob, action, *job),
        Job::Implant => crate::implants::complete_implant(world, mob, action),
        Job::Food(job) => crate::food::complete(world, mob, action, *job),
        Job::Smoking(job) => crate::smoking::complete(world, mob, action, *job),
        &Job::Clean(job) => crate::janitor::complete(world, mob, action, job),
        Job::Botany(job) => crate::botany::complete(world, mob, action, *job),
        Job::Atmos(job) => crate::atmos_machinery::windows::complete(world, mob, action, *job),
        &Job::Climb(climb) => crate::multiz::complete_climb(world, mob, target, climb),
        &Job::Fly(climb) => crate::multiz::complete_flight(world, client, mob, climb),
        Job::Interact(job) => crate::drag::complete(world, mob, action, *job),
        &Job::Bitrunning(job) => {
            crate::science::bitrunning::server::complete(world, mob, action, job)
        }
        Job::PlatePress => crate::brig::labor::complete_press(world, mob, target),
        &Job::Table(job) => crate::tables::complete(world, mob, action, job),
        Job::Strip(job) => crate::strip::complete(world, mob, action, *job),
        Job::Medical(job) => crate::medical::complete_med(world, mob, action, *job),
        &Job::Mecha(job) => crate::mecha::core::complete(world, mob, action, job),
        Job::Disposal(job) => crate::disposals::complete(world, mob, action, *job),
        Job::Recycling(job) => crate::recycling::complete(world, mob, action, *job),
        &Job::Vampire(job) => crate::antag::vampire::server::complete(world, mob, action, job),
        &Job::Cult(job) => crate::antag::cult::server::complete(world, mob, action, job),
        &Job::Abductor(job) => {
            crate::antag::abductor::server::complete_abductor_job(world, mob, action, job)
        }
        &Job::Heretic(job) => crate::antag::heretic::server::complete(world, mob, action, job),
        &Job::Changeling(job) => {
            crate::antag::changeling::server::complete(world, mob, action, job)
        }
        Job::Genetics(_) => crate::genetics::console::complete(world, mob, action),
        &Job::WallMount(job) => crate::wallmount::complete(world, client, mob, target, job),
        &Job::Frame(job) => crate::machine_frame::complete(world, mob, action, job),
        &Job::Airlock(job) => crate::airlock_construction::complete(world, mob, action, job),
        &Job::Vending(job) => crate::vending::complete(world, mob, action, job),
        &Job::Bot(job) => crate::bots::complete_job(world, mob, action, job),
        &Job::Gibber(job) => crate::gibber::complete(world, mob, action, job),
        &Job::Explosive(job) => crate::explosions::sources::complete(world, mob, action, job),
        Job::Experiment => crate::science::experi_scanner::complete(world, mob, action),
        Job::SlimeCores => crate::science::xenobio::complete_core_removal(world, action),
        Job::Swab => crate::science::cell_lines::complete_swab(world, action),
        &Job::Martial(job) => crate::martial::complete_reading(world, mob, action, job),
        &Job::Religion(job) => crate::service::religion_complete(world, mob, action, job),
        &Job::Art(job) => crate::service::art_complete(world, mob, action, job),
        &Job::Barber(job) => crate::service::barber_complete(world, mob, action, job),
        &Job::Photo(job) => crate::service::photo_complete(world, mob, action, job),
        &Job::Camera(job) => crate::defenses::cameras::complete(world, mob, action, job),
        &Job::Sink(job) => crate::plumbing::complete(world, mob, action, job),
        &Job::Decor(job) => crate::decor::complete(world, mob, action, job),
        &Job::DisplayCase(job) => crate::displaycase::complete(world, mob, action, job),
        &Job::FishMount(job) => crate::fishing::mount::complete(world, mob, action, job),
        &Job::Light(job) => crate::power::lighting::complete(world, action, job),
        Job::Crayon(job) => crate::crayons::complete(world, mob, action, job.clone()),
        &Job::Affliction(job) => crate::afflictions::complete(world, mob, action, job),
        &Job::Curse(job) => crate::curses::complete(world, mob, action, job),
        &Job::Ventcrawl(job) => crate::ventcrawl::complete(world, mob, action, job),
        &Job::StackRecipe(job) => crate::stack_recipes::complete(world, client, mob, job),
        &Job::Sheet(job) => crate::sheets::complete(world, client, mob, job),
        &Job::QuantumPad(job) => crate::science::quantum_pad::complete(world, client, target, job),
        &Job::Device(job) => crate::devices::complete_job(world, client, mob, target, job),
        &Job::EngMachine(job) => crate::eng_machines::complete(world, client, target, job),
        &Job::Flora(job) => crate::flora::complete(world, mob, action, job),
        &Job::Trap(job) => crate::traps::complete(world, mob, action, job),
        &Job::DeployableTurret(job) => crate::deployable_turrets::complete(world, mob, action, job),
        &Job::Cannon(job) => crate::cannons::complete(world, mob, action, job),
        &Job::Gym(job) => crate::gym::complete(world, mob, action, job),
        &Job::Kneecap(zone) => crate::weapons::kneecap::complete(world, mob, action, zone),
        &Job::Safe(job) => crate::safes::complete(world, action, job),
        &Job::SecureSafe(job) => crate::secure_safes::complete(world, action, job),
        Job::Bonfire => crate::fireplace::complete_dismantle(world, action),
        &Job::LawRack(job) => crate::silicon::law_rack::complete(world, mob, action, job),
        &Job::AiCore(job) => crate::silicon::ai_core::complete(world, mob, action, job),
        &Job::AiCard(job) => crate::silicon::aicard::complete(world, mob, action, job),
        &Job::Pai(job) => crate::silicon::pai::complete(world, mob, action, job),
        &Job::ModSuit(job) => crate::modsuit::complete(world, mob, action, job),
        &Job::Mining(job) => crate::mining::complete(world, mob, action, job),
        Job::Hijack => crate::shuttle_console_server::complete_hijack(world, mob, action),
        &Job::Door(job) => crate::doors::complete(world, client, mob, target, job),
        &Job::DoorBuild(job) => crate::door_structures::complete(world, mob, action, job),
        &Job::Wiremod(shell) => crate::wiremod::server::complete(world, mob, action, shell),
        &Job::Inducer(thing) => crate::inducer::complete(world, mob, action, thing),
        &Job::Clothing(job) => crate::clothing::damage::complete(world, mob, action, job),
        &Job::Fauna(job) => crate::mobs::farm::complete(world, mob, action, job),
        &Job::Xeno(job) => crate::antag::xeno::abilities::complete(world, mob, action, job),
        &Job::Morph { corpse } => crate::antag::morph::complete_eat(world, mob, action, corpse),
        &Job::SpaceDragon(job) => crate::antag::space_dragon::complete(world, mob, action, job),
        &Job::Wizard(job) => crate::antag::wizard::server::complete(world, mob, action, job),
        &Job::Emitter(job) => crate::power::emitter::complete(world, client, target, job, consume),
        &Job::FieldGenerator(job) => {
            crate::power::field_generator::complete(world, client, target, job, consume)
        }
        &Job::Smes(job) => crate::power::smes::complete(world, mob, action, job, consume),
        &Job::Shield(job) => crate::power::shields::complete(world, action, job, consume),
        Job::RadiationLeak => crate::events::radiation_leak::complete(world, action, consume),
        Job::AnomalyRelease => crate::events::anomalies::complete_release(world, action),
        &Job::AnomalockRemove { module } => {
            crate::modsuit::anomalock::complete_removal(world, mob, action.client, module);
        }
        &Job::Rcd(job) => crate::devices::rcd::complete(world, mob, action, job),
        &Job::Rhd(job) => crate::devices::rhd::complete(world, action, job),
        &Job::TapeRespool(tape) => {
            crate::devices::taperecorder::respool_complete(world, mob, action.client, tape)
        }
        Job::GirderBolts { from } => {
            if !is_under(&path, GIRDER) || obj_state(world, target) != *from || !consume(world) {
                return;
            }
            let (to, text) = if *from == GIRDER_NORMAL {
                (DISPLACED_GIRDER, "Вы откручиваете каркас.")
            } else {
                (GIRDER, "Вы закрепляете каркас.")
            };
            replace_girder(world, target, to);
            tell(world, client, Span::Notice, text);
        }
        Job::GirderDisassemble => {
            if obj_state(world, target) != GIRDER_DISPLACED || !consume(world) {
                return;
            }
            crate::obj_damage::deconstruct(world, target, true);
            tell(world, client, Span::Notice, "Вы разбираете каркас.");
        }
        &Job::Wall { recipe } => {
            let Some(recipe) = GIRDER_RECIPES.get(recipe).copied() else {
                return;
            };
            let actor = Actor { client, mob, item };
            if !is_under(&path, recipe.girder)
                || !wall_recipe_ok(world, actor, target, &recipe)
                || !has_template(world, recipe.wall)
            {
                return;
            }
            if !consume(world) {
                return;
            }
            world.despawn(target);
            if is_under(recipe.wall, "/turf") {
                place_on_top(world, pos, recipe.wall, false);
            } else {
                spawn_obj(world, recipe.wall, pos);
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы заканчиваете строить стену на каркасе.",
            );
        }
        Job::GirderReinforce => {
            let normal = obj_state(world, target) == GIRDER_NORMAL;
            if !is_under(&path, GIRDER) || !normal || !has_template(world, REINFORCED_GIRDER) {
                return;
            }
            if !consume(world) {
                return;
            }
            replace_girder(world, target, REINFORCED_GIRDER);
            tell(world, client, Span::Notice, "Вы укрепляете каркас.");
        }
        &Job::GirderStruts { from } => {
            if !is_under(&path, GIRDER) || obj_state(world, target) != from {
                return;
            }
            let (to, text) = if from == GIRDER_REINF {
                (GIRDER_REINF_STRUTS, "Вы откручиваете опорные стойки.")
            } else {
                (GIRDER_REINF, "Вы закрепляете опорные стойки.")
            };
            world.entity_mut(target).insert(ObjState(to));
            tell(world, client, Span::Notice, text);
        }
        Job::GirderGrille => {
            let struts = obj_state(world, target) == GIRDER_REINF_STRUTS;
            if !is_under(&path, GIRDER) || !struts || !has_template(world, GIRDER) {
                return;
            }
            spawn_stack(world, PLASTEEL, pos, 1);
            replace_girder(world, target, GIRDER);
            tell(
                world,
                client,
                Span::Notice,
                "Вы вынимаете внутреннюю решётку.",
            );
        }
        &Job::Reinforced(job) => crate::reinforced::complete(world, client, target, job),
        Job::Holosign => crate::holosign::complete(world, item, pos),
        Job::Satellite => crate::station_goals::complete_toggle(world, client, target),
        Job::TearWall => crate::weapons::rip_and_tear::complete(world, mob, action),
        Job::FanDisassemble => {
            if !is_under(&path, FANS) {
                return;
            }
            // `buildstacktype`, `buildstackamount`.
            let amount = if is_under(&path, TINY_FAN) { 2 } else { 5 };
            world.despawn(target);
            spawn_stack(world, IRON, pos, amount);
            tile_changed(world, pos);
            tell(world, client, Span::Notice, "Вы разбираете вентилятор.");
        }
        Job::SliceWall => {
            if !is_under(&path, WALL) || !consume(world) {
                return;
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы снимаете внешнюю обшивку с каркаса.",
            );
            dismantle_wall(world, target, pos);
        }
        Job::BuildGirder => {
            if tile_blocked(world, pos) || !has_template(world, DISPLACED_GIRDER) {
                tell(world, client, Span::Warning, "Что-то загораживает клетку!");
                return;
            }
            if !consume(world) {
                return;
            }
            spawn_obj(world, DISPLACED_GIRDER, pos);
            tell(world, client, Span::Notice, "каркас создан");
        }
        Job::WindowScrew {
            state,
            anchored: was,
        } => {
            if obj_state(world, target) != *state || anchored(world, target) != *was {
                return;
            }
            let text = match *state {
                WINDOW_SCREWED_TO_FRAME => {
                    world.entity_mut(target).insert(ObjState(WINDOW_IN_FRAME));
                    "Вы открепляете окно от рамы."
                }
                WINDOW_IN_FRAME => {
                    world
                        .entity_mut(target)
                        .insert(ObjState(WINDOW_SCREWED_TO_FRAME));
                    "Вы прикрепляете окно к раме."
                }
                _ => {
                    crate::obj_damage::set_anchored(world, target, !was);
                    if *was {
                        "Вы открепляете раму от пола."
                    } else {
                        "Вы прикрепляете раму к полу."
                    }
                }
            };
            tell(world, client, Span::Notice, text);
        }
        Job::WindowPry { state } => {
            if obj_state(world, target) != *state || !anchored(world, target) {
                return;
            }
            let (to, text) = if *state == WINDOW_IN_FRAME {
                (WINDOW_OUT_OF_FRAME, "Вы вынимаете окно из рамы.")
            } else {
                (
                    WINDOW_SCREWED_TO_FRAME,
                    "Вы вставляете окно обратно в раму.",
                )
            };
            world.entity_mut(target).insert(ObjState(to));
            tell(world, client, Span::Notice, text);
        }
        Job::WindowDisassemble { state } => {
            if obj_state(world, target) != *state || anchored(world, target) {
                return;
            }
            let Some(obj) = world.get::<DmObj>(target).cloned() else {
                return;
            };
            // The glass lands under the user (`new glass_type(user.loc, glass_amount)`).
            let at = world.get::<TilePos>(mob).copied().unwrap_or(pos);
            spawn_stack(world, &obj.glass_type, at, obj.glass_amount);
            world.despawn(target);
            tile_changed(world, pos);
            tell(world, client, Span::Notice, "Вы разбираете окно.");
        }
        Job::WindowRepair => {
            let Some(max) = world.get::<DmObj>(target).map(|o| o.max_integrity) else {
                return;
            };
            if !consume(world) {
                return;
            }
            if let Some(mut integrity) = world.get_mut::<Integrity>(target) {
                integrity.current = max;
            }
            tell(world, client, Span::Notice, "Вы чините окно.");
        }
        Job::GlassOnGrille => {
            if !anchored(world, target) || window_on(world, target) {
                return;
            }
            if !has_template(world, FULLTILE_WINDOW) || !consume(world) {
                return;
            }
            if let Some(window) = spawn_obj(world, FULLTILE_WINDOW, pos) {
                // `set_anchored(FALSE)`, `state = 0`.
                crate::obj_damage::set_anchored(world, window, false);
                world
                    .entity_mut(window)
                    .insert(ObjState(WINDOW_OUT_OF_FRAME));
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы устанавливаете стекло на решётку.",
            );
        }
        Job::GrilleRepair => {
            let broken = world.get::<Integrity>(target).is_some_and(|i| i.broken);
            let Some(max) = world.get::<DmObj>(target).map(|o| o.max_integrity) else {
                return;
            };
            if !broken || !consume(world) {
                return;
            }
            world.entity_mut(target).insert((
                Integrity {
                    current: max,
                    broken: false,
                },
                Density,
            ));
            crate::obj_damage::set_icon_state(world, target, "grille");
            tell(world, client, Span::Notice, "Вы чините решётку.");
        }
        Job::BuildGrille => {
            if grille_on(world, pos) || !has_template(world, GRILLE) || !consume(world) {
                return;
            }
            spawn_obj(world, GRILLE, pos);
            tell(world, client, Span::Notice, "Вы собираете решётку.");
        }
        Job::Unfasten { anchored: was } => {
            if anchored(world, target) != *was || (!was && blocked_for(world, target)) {
                return;
            }
            crate::obj_damage::set_anchored(world, target, !was);
            // Also the fork's shredder and teapot (`default_unfasten_wrench`).
            let name = if path_of(world, target).is_some_and(|p| is_under(&p, VENDING)) {
                "автомат".to_owned()
            } else {
                crate::chem::name_of(world, target)
            };
            let text = if *was {
                format!("Вы откручиваете {name}.")
            } else {
                format!("Вы прикручиваете {name}.")
            };
            tell(world, client, Span::Notice, text);
        }
    }
}

/// `replace_girder`: a girder of another type in its place, with the same integrity ratio.
fn replace_girder(world: &mut World, girder: Entity, to: &str) {
    let Some(&pos) = world.get::<TilePos>(girder) else {
        return;
    };
    let ratio = match (world.get::<Integrity>(girder), world.get::<DmObj>(girder)) {
        (Some(i), Some(o)) if o.max_integrity > 0.0 => i.current / o.max_integrity,
        _ => 1.0,
    };
    if !has_template(world, to) {
        return;
    }
    world.despawn(girder);
    if let Some(new) = spawn_obj(world, to, pos) {
        let max = world.get::<DmObj>(new).map_or(0.0, |o| o.max_integrity);
        world.entity_mut(new).insert(Integrity {
            current: max * ratio,
            broken: false,
        });
    }
    tile_changed(world, pos);
}

/// `dismantle_wall` -> `break_wall`: its sheets and girder on the tile, then `ScrapeAway`.
pub(crate) fn dismantle_wall(world: &mut World, wall: Entity, pos: TilePos) {
    let Some(build) = world.get::<DmTurfBuild>(wall).cloned() else {
        return;
    };
    if scrape_away(world, pos, false).is_none() {
        return;
    }
    if !build.sheet_type.is_empty() {
        spawn_stack(world, &build.sheet_type, pos, build.sheet_amount);
    }
    if !build.girder_type.is_empty() {
        spawn_obj(world, &build.girder_type, pos);
    }
    tile_changed(world, pos);
}

/// tg `dismantle_wall(devastated = TRUE)` -> `devastate_wall`: the wall's sheets and, with a
/// girder type, one iron sheet instead of the girder.
pub fn devastate_wall(world: &mut World, wall: Entity, pos: TilePos) {
    let Some(build) = world.get::<DmTurfBuild>(wall).cloned() else {
        return;
    };
    if scrape_away(world, pos, false).is_none() {
        return;
    }
    if !build.sheet_type.is_empty() {
        spawn_stack(world, &build.sheet_type, pos, build.sheet_amount);
    }
    if !build.girder_type.is_empty() {
        spawn_stack(world, IRON, pos, 1);
    }
    tile_changed(world, pos);
}

/// The turf entity of `pos`.
pub fn turf_at(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<Turf>(e).is_some())
}

/// tg `isspaceturf`: the turf on `pos` is a `/turf/open/space` type (by type, not by its air:
/// test fixtures and odd maps can leave a space turf's air blocked).
pub fn is_space(world: &World, pos: TilePos) -> bool {
    turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .is_some_and(|p| crate::obj_damage::is_under(&p, "/turf/open/space"))
}

/// tg `assemble_baseturfs` of a type: its `baseturfs` chain down to the bottom, bottom first.
fn assemble_baseturfs(world: &World, path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let protos = world.get_resource::<Prototypes>();
    let mut current = protos
        .and_then(|p| p.get(path))
        .and_then(|p| p.get::<DmTurfBuild>())
        .map(|b| b.baseturf.clone())
        .unwrap_or_default();
    while !current.is_empty() && out.len() < 16 && !out.contains(&current) {
        out.push(current.clone());
        if current == BASETURF_BOTTOM {
            break;
        }
        current = protos
            .and_then(|p| p.get(&current))
            .and_then(|p| p.get::<DmTurfBuild>())
            .map(|b| b.baseturf.clone())
            .unwrap_or_default();
    }
    out.reverse();
    out
}

/// The baseturf stack of a turf entity (its [`Baseturfs`], else its type's chain).
pub fn baseturfs_of(world: &World, turf: Entity) -> Vec<String> {
    if let Some(stack) = world.get::<Baseturfs>(turf) {
        return stack.0.clone();
    }
    path_of(world, turf).map_or_else(Vec::new, |p| assemble_baseturfs(world, &p))
}

/// tg `place_on_top`: `path` over the turf of `pos`; an open turf's own type joins the stack.
pub fn place_on_top(
    world: &mut World,
    pos: TilePos,
    path: &str,
    inherit_air: bool,
) -> Option<Entity> {
    let old = turf_at(world, pos)?;
    let mut stack = baseturfs_of(world, old);
    let old_path = path_of(world, old)?;
    if is_under(&old_path, OPEN) {
        stack.push(old_path);
    }
    change_turf(world, pos, path, stack, inherit_air)
}

/// tg `ScrapeAway()`: the top of the stack comes off; the bottom never goes away.
pub fn scrape_away(world: &mut World, pos: TilePos, inherit_air: bool) -> Option<Entity> {
    let old = turf_at(world, pos)?;
    let mut stack = baseturfs_of(world, old);
    let next = if stack.len() > 1 {
        let mut next = stack.pop().expect("non-empty");
        // tg `ScrapeAway` skips `/turf/baseturf_skipover` types (a shuttle's boundary).
        while is_under(&next, ss13_mapload::dock::SKIPOVER_ROOT) && stack.len() > 1 {
            next = stack.pop().expect("non-empty");
        }
        next
    } else {
        stack
            .first()
            .cloned()
            .unwrap_or_else(|| BASETURF_BOTTOM.to_owned())
    };
    change_turf(world, pos, &next, stack, inherit_air)
}

/// tg `ChangeTurf(path, new_baseturfs, flags)`: the turf of `pos` is replaced by a `path` turf.
/// `None` (nothing changes) without a prototype for `path`.
pub fn change_turf(
    world: &mut World,
    pos: TilePos,
    path: &str,
    baseturfs: Vec<String>,
    inherit_air: bool,
) -> Option<Entity> {
    let path = if path == BASETURF_BOTTOM {
        if ss13_mapload::mining_z(world) == Some(pos.z) {
            LAVALAND_BASETURF
        } else {
            LEVEL_BASETURF
        }
    } else {
        path
    };
    if !has_template(world, path) {
        warn!("ChangeTurf to {path}: no prototype (rerun `ss13-mapload convert`)");
        return None;
    }
    let old = turf_at(world, pos);
    let was_lava = old
        .and_then(|o| path_of(world, o))
        .is_some_and(|p| crate::obj_damage::is_under(&p, crate::lava::LAVA));
    let (old_kind, area_light) = old.map_or((None, None), |o| {
        (
            world.get::<TurfAir>(o).map(|a| a.kind),
            world.get::<Optics>(o).and_then(|op| op.tile),
        )
    });
    if let Some(old) = old {
        world.despawn(old);
    }
    let turf = spawn_template(world, path, pos)?;
    world.entity_mut(turf).insert(Baseturfs(baseturfs));
    if area_light.is_some() {
        // The area's lighting of the tile stays (tg keeps the turf's lighting object).
        let mut entity = world.entity_mut(turf);
        match entity.get_mut::<Optics>() {
            Some(mut optics) => optics.tile = area_light,
            None => {
                entity.insert(Optics {
                    tile: area_light,
                    ..default()
                });
            }
        }
    }
    let air = world.get::<TurfAir>(turf).cloned();
    if let (Some(air), Some(mut atmos)) = (air, world.get_resource_mut::<Atmos>()) {
        let both_open = old_kind == Some(TileKind::Open) && air.kind == TileKind::Open;
        if inherit_air && both_open {
            atmos.replace_turf_keeping_air(pos, &air);
        } else {
            atmos.change_turf(pos, &air);
        }
    }
    tile_changed(world, pos);
    // tg `refresh_light`: the glow of this tile and which of the lava around it is on the shore.
    if was_lava || crate::obj_damage::is_under(path, crate::lava::LAVA) {
        ss13_mapload::light::relight_rect(world, pos.z, (pos.x, pos.y), (pos.x, pos.y));
    }
    Some(turf)
}

/// [`ss13_mapload::tile_changed`] once the map's icons are known.
pub fn tile_changed(world: &mut World, pos: TilePos) {
    if world.contains_resource::<IconIndex>() {
        ss13_mapload::tile_changed(world, pos);
    }
    crate::multiz::refresh_transparency(world, pos);
}

fn has_template(world: &mut World, path: &str) -> bool {
    world.resource_scope(|world, mut cache: Mut<BuildTemplates>| template(world, &mut cache, path))
}

fn template(world: &World, cache: &mut BuildTemplates, path: &str) -> bool {
    if let Some(t) = cache.0.get(path) {
        return t.is_some();
    }
    // While the map loader holds `Prototypes` (map objects initialize inside its resource scope)
    // the resource is absent: a miss then is not "no such prototype", so it is not cached.
    if !world.contains_resource::<Prototypes>() {
        return false;
    }
    let built = world
        .get_resource::<Prototypes>()
        .and_then(|p| p.get(path).cloned())
        .map(|proto| {
            let index: HashMap<(&str, &str), ss13_shared::IconStateId> = world
                .get_resource::<IconIndex>()
                .map(|i| {
                    i.0.iter()
                        .map(|((dmi, state), id)| ((dmi.as_str(), state.as_str()), *id))
                        .collect()
                })
                .unwrap_or_default();
            Template::new(proto, &index)
        });
    let found = built.is_some();
    cache.0.insert(path.to_owned(), built);
    found
}

/// Spawns a `path` atom on `pos` as the map would, replicated if drawn. `None` without its
/// prototype.
fn spawn_template(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    let entity = world.resource_scope(|world, mut cache: Mut<BuildTemplates>| {
        if !template(world, &mut cache, path) {
            return None;
        }
        let t = cache.0.get(path)?.as_ref()?;
        Some(t.spawn(world, Some(pos)))
    })?;
    let visible = world
        .get::<DmAtom>(entity)
        .is_none_or(|a| a.invisibility == 0);
    let drawn = world.get::<ss13_shared::Appearance>(entity).is_some();
    if visible && drawn && world.get::<HiddenUnderTile>(entity).is_none() {
        world.entity_mut(entity).insert(Networked);
    }
    Some(entity)
}

/// A new `path` object on `pos` (tg `new path(loc)`), initialized like map objects.
pub fn spawn_obj(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    let entity = spawn_template(world, path, pos)?;
    crate::storage::finish_item(world, entity, 0);
    tile_changed(world, pos);
    Some(entity)
}

/// A new `path` object nowhere (tg `new path()` in nullspace): not on the map, not replicated
/// until placed. `None` without its prototype.
pub fn spawn_detached(world: &mut World, path: &str) -> Option<Entity> {
    let entity = world
        .try_resource_scope(|world, mut cache: Mut<BuildTemplates>| {
            if !template(world, &mut cache, path) {
                return None;
            }
            Some(cache.0.get(path)?.as_ref()?.spawn(world, None))
        })
        .flatten()?;
    crate::storage::finish_item(world, entity, 0);
    Some(entity)
}

/// tg `new stack_type(loc, amount)`: merges into stacks of its kind on the tile first
/// (`/obj/item/stack/Initialize(merge = TRUE)`), the rest is a new stack (split at
/// `max_amount`).
pub fn spawn_stack(world: &mut World, path: &str, pos: TilePos, amount: u32) -> Option<Entity> {
    spawn_stack_as(world, path, pos, amount, false)
}

/// [`spawn_stack`] of [`Manufactured`] sheets (tg `retrieve_stack`: `manufactured = TRUE`).
pub fn spawn_manufactured(
    world: &mut World,
    path: &str,
    pos: TilePos,
    amount: u32,
) -> Option<Entity> {
    spawn_stack_as(world, path, pos, amount, true)
}

fn spawn_stack_as(
    world: &mut World,
    path: &str,
    pos: TilePos,
    amount: u32,
    manufactured: bool,
) -> Option<Entity> {
    if path.is_empty() || amount == 0 {
        return None;
    }
    let mut left = amount;
    let mut last = None;
    while left > 0 {
        let stack = spawn_template(world, path, pos)?;
        crate::storage::finish_item(world, stack, 0);
        let max = world
            .get::<DmStack>(stack)
            .map_or(50, |s| s.max_amount.max(1));
        let here = left.min(max);
        world.entity_mut(stack).insert(Stack { amount: here });
        if manufactured {
            world.entity_mut(stack).insert(Manufactured);
        }
        left -= here;
        let others: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| e != stack && world.get::<Stack>(e).is_some())
            .collect();
        for other in others {
            if world.get_entity(stack).is_ok() && can_merge(world, stack, other) {
                merge_into(world, stack, other);
                last = Some(other);
            }
        }
        if world.get_entity(stack).is_ok() {
            last = Some(stack);
        }
    }
    last
}

/// Clients of player mobs within `range` of `pos`, except `exclude`.
pub fn clients_near(world: &World, pos: TilePos, range: u32, exclude: &[Entity]) -> Vec<Entity> {
    crate::obj_damage::clients_in_range(world, pos, range, exclude)
}

/// tg `attack_hand` on a stack held in the other hand: one unit of it into the active hand
/// (`split_n_take(user, 1)`).
pub fn split_into_hand(world: &mut World, mob: Entity, stack: Entity, hand: usize) -> bool {
    if crate::tools::stack_amount(world, stack) < 2 {
        return false;
    }
    let Some(path) = world.get::<DmItem>(stack).map(|i| i.path.clone()) else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    let Some(new) = spawn_template(world, &path, pos) else {
        return false;
    };
    crate::storage::finish_item(world, new, 0);
    // `copy_evidences`: the split keeps the seal.
    if world.get::<Manufactured>(stack).is_some() {
        world.entity_mut(new).insert(Manufactured);
    }
    crate::tools::use_stack(world, stack, 1);
    world.entity_mut(new).insert(Stack { amount: 1 });
    crate::inventory::pick_up(world, mob, new, hand);
    true
}

#[cfg(test)]
mod tests {
    use crate::obj_damage::Anchored;
    use ss13_atmos::{AtmosBlocker, AtmosPass};
    use ss13_net::chat::server::SendChat;
    use ss13_world::WorldPlugin;

    use super::*;

    fn world() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, WorldPlugin))
            .add_message::<SendChat>()
            .init_resource::<BuildTemplates>();
        app.world_mut()
            .resource_mut::<SpatialIndex>()
            .create_level(1, 5, 5)
            .unwrap();
        app
    }

    fn job(world: &mut World, target: Entity, job: Job) {
        let mob = world.spawn(TilePos::new(1, 1, 1)).id();
        let action = DoAfter {
            client: mob,
            target,
            target_pos: TilePos::new(2, 2, 1),
            user_pos: TilePos::new(1, 1, 1),
            item: None,
            amount: 0,
            ends_at: 0.0,
            job,
        };
        complete(world, mob, &action);
    }

    /// The plain window chain of `/obj/structure/window`: screwdriver, crowbar, screwdriver,
    /// wrench; a step whose starting state changed meanwhile does nothing.
    #[test]
    fn window_chain_and_stale_steps() {
        let mut app = world();
        let world = app.world_mut();
        let window = world
            .spawn((
                DmObj {
                    path: FULLTILE_WINDOW.into(),
                    fulltile: true,
                    max_integrity: 100.0,
                    ..default()
                },
                ObjState(WINDOW_SCREWED_TO_FRAME),
                Anchored(true),
                AtmosBlocker(AtmosPass::Proc),
                Density,
                TilePos::new(2, 2, 1),
            ))
            .id();
        let state = |world: &World| obj_state(world, window);
        let unscrew = Job::WindowScrew {
            state: WINDOW_SCREWED_TO_FRAME,
            anchored: true,
        };
        job(world, window, unscrew.clone());
        assert_eq!(state(world), WINDOW_IN_FRAME);
        job(world, window, unscrew);
        assert_eq!(
            state(world),
            WINDOW_IN_FRAME,
            "stale: started from another state"
        );
        job(
            world,
            window,
            Job::WindowPry {
                state: WINDOW_IN_FRAME,
            },
        );
        assert_eq!(state(world), WINDOW_OUT_OF_FRAME);
        job(
            world,
            window,
            Job::WindowDisassemble {
                state: WINDOW_OUT_OF_FRAME,
            },
        );
        assert!(world.get_entity(window).is_ok(), "still anchored");
        job(
            world,
            window,
            Job::WindowScrew {
                state: WINDOW_OUT_OF_FRAME,
                anchored: true,
            },
        );
        assert!(!anchored(world, window));
        assert!(
            world.get::<AtmosBlocker>(window).is_none(),
            "an unanchored window lets air through"
        );
        job(
            world,
            window,
            Job::WindowDisassemble {
                state: WINDOW_OUT_OF_FRAME,
            },
        );
        assert!(world.get_entity(window).is_err());
    }

    #[test]
    fn girder_steps_need_their_state() {
        let mut app = world();
        let world = app.world_mut();
        let girder = world
            .spawn((
                DmObj {
                    path: GIRDER.into(),
                    max_integrity: 200.0,
                    ..default()
                },
                ObjState(GIRDER_NORMAL),
                TilePos::new(2, 2, 1),
            ))
            .id();
        // A screwdriver only takes a displaced girder apart.
        job(world, girder, Job::GirderDisassemble);
        assert!(world.get_entity(girder).is_ok());
        // A wrench started on a displaced girder does nothing to a secured one.
        job(
            world,
            girder,
            Job::GirderBolts {
                from: GIRDER_DISPLACED,
            },
        );
        assert!(world.get_entity(girder).is_ok());
        assert_eq!(obj_state(world, girder), GIRDER_NORMAL);
    }

    /// `main_girder_wall_recipes`: iron walls a secured girder, plasteel a reinforced one, both
    /// make false walls on a displaced girder; plasteel on a secured girder is no recipe (it
    /// reinforces the girder instead).
    #[test]
    fn girder_recipes_follow_stack_girder_and_state() {
        let wall = |stack, girder, state| {
            girder_recipe(stack, girder, state).map(|i| GIRDER_RECIPES[i].wall)
        };
        assert_eq!(wall(IRON, GIRDER, GIRDER_NORMAL), Some(WALL));
        assert_eq!(
            wall(PLASTEEL, REINFORCED_GIRDER, GIRDER_REINF),
            Some(crate::reinforced::R_WALL)
        );
        assert_eq!(wall(PLASTEEL, REINFORCED_GIRDER, GIRDER_REINF_STRUTS), None);
        assert_eq!(
            wall(IRON, DISPLACED_GIRDER, GIRDER_DISPLACED),
            Some(crate::falsewall::FALSEWALL)
        );
        assert_eq!(
            wall(PLASTEEL, DISPLACED_GIRDER, GIRDER_DISPLACED),
            Some(crate::falsewall::R_FALSEWALL)
        );
        assert_eq!(wall(PLASTEEL, GIRDER, GIRDER_NORMAL), None);
        assert_eq!(wall(IRON, REINFORCED_GIRDER, GIRDER_REINF), None);
    }

    #[test]
    fn reinforced_girder_struts_toggle_from_their_state() {
        let mut app = world();
        let world = app.world_mut();
        let girder = world
            .spawn((
                DmObj {
                    path: REINFORCED_GIRDER.into(),
                    max_integrity: 350.0,
                    ..default()
                },
                ObjState(GIRDER_REINF),
                TilePos::new(2, 2, 1),
            ))
            .id();
        let unscrew = Job::GirderStruts { from: GIRDER_REINF };
        job(world, girder, unscrew.clone());
        assert_eq!(obj_state(world, girder), GIRDER_REINF_STRUTS);
        job(world, girder, unscrew);
        assert_eq!(obj_state(world, girder), GIRDER_REINF_STRUTS, "stale");
        job(
            world,
            girder,
            Job::GirderStruts {
                from: GIRDER_REINF_STRUTS,
            },
        );
        assert_eq!(obj_state(world, girder), GIRDER_REINF);
    }

    #[test]
    fn fans_come_apart_only_while_fans() {
        let mut app = world();
        let world = app.world_mut();
        let fan = world
            .spawn((
                DmObj {
                    path: TINY_FAN.into(),
                    ..default()
                },
                TilePos::new(2, 2, 1),
            ))
            .id();
        let girder = world
            .spawn((
                DmObj {
                    path: GIRDER.into(),
                    ..default()
                },
                TilePos::new(2, 2, 1),
            ))
            .id();
        job(world, girder, Job::FanDisassemble);
        assert!(world.get_entity(girder).is_ok(), "not a fan");
        job(world, fan, Job::FanDisassemble);
        assert!(world.get_entity(fan).is_err());
    }

    #[test]
    fn stacks_merge_up_to_their_max() {
        let mut app = world();
        let world = app.world_mut();
        let stack = |world: &mut World, amount| {
            world
                .spawn((
                    DmItem {
                        path: IRON.into(),
                        ..default()
                    },
                    DmStack {
                        amount: 1,
                        max_amount: 50,
                        merge_type: IRON.into(),
                        ..default()
                    },
                    Stack { amount },
                ))
                .id()
        };
        let (a, b) = (stack(world, 30), stack(world, 30));
        assert!(can_merge(world, a, b) && !can_merge(world, a, a));
        merge_into(world, a, b);
        assert_eq!(world.get::<Stack>(b).unwrap().amount, 50);
        assert_eq!(world.get::<Stack>(a).unwrap().amount, 10);
        let c = stack(world, 10);
        merge_into(world, a, c);
        assert!(world.get_entity(a).is_err(), "an emptied stack is deleted");
        assert_eq!(world.get::<Stack>(c).unwrap().amount, 20);
        let rods = world
            .spawn((
                DmItem {
                    path: RODS.into(),
                    ..default()
                },
                DmStack {
                    merge_type: RODS.into(),
                    ..default()
                },
                Stack { amount: 5 },
            ))
            .id();
        assert!(!can_merge(world, rods, c));
    }
}
