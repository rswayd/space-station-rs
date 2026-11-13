//! Ridden vehicles (server), after tg `code/modules/vehicles` (`_vehicle.dm`, `ridden.dm`,
//! `vehicle_key.dm`, `scooter.dm`, `wheelchair.dm`, `motorized_wheelchair.dm`, `atv.dm`,
//! `bicycle.dm`, `secway.dm`, `speedbike.dm`, `pimpin_ride.dm`, `vehicle_actions.dm`) and the
//! riding component (`code/datums/components/riding/riding.dm`, `riding_vehicle.dm`).
//!
//! - Mounting is buckling (`crate::medical::surgery`: dragging a mob onto the vehicle, an empty
//!   hand or resisting to get off): the mob becomes its driver ([`Rider`], tg `add_occupant`
//!   with `VEHICLE_CONTROL_DRIVE`), sits upright facing the vehicle's way and is drawn
//!   `rider_up` pixels up for that facing (`get_rider_offsets_and_layers`, through
//!   [`crate::furniture::Seated`]); the vehicle takes its ridden layer and offset per facing
//!   (`get_parent_offsets_and_layers`). A skateboard is dense only while ridden.
//! - Driving: the driver's steps are the vehicle's (`client/Move` -> `buckled.relaymove` ->
//!   `driver_move`): the key must be inserted (`keycheck`), the driver not incapacitated, not
//!   floored when the vehicle needs legs, hands free when it needs arms; a vehicle with
//!   `UNBUCKLE_DISABLED_RIDER` throws a driver who cannot drive off (stunned 3 s). The step
//!   needs gravity unless the vehicle flies (`Process_Spacemove`), then the cooldown is
//!   BandaStation's `modified_move_delay(vehicle_move_delay)` (×√2 diagonally). A hand
//!   wheelchair's delay follows the driver's usable hands, a motorized one's its speed, and it
//!   draws from its cell; a skateboard is 0.6 slower at walk intent. The rider moves with the
//!   vehicle however it moves (pulled too); a rider moved away some other way gets off.
//! - Keys (`/obj/item/key/*`): used on the vehicle they go in, alt-click by the driver takes
//!   them back to the hand.
//! - Crashes: a skateboard hitting something dense at run intent costs `instability × 6`
//!   stamina and bounces back a tile; exhausted (or into a mob) the skater is thrown 3 tiles,
//!   paralyzed 8 s and, without a helmet, takes 5 brain damage; the mob hit is knocked down 4 s.
//! - Actions: a skateboard's driver gets Ollie (a hop forward, over tables) and Kickflip (off the
//!   board with it in hand); both cost stamina and drop an exhausted skater.
//! - A skateboard dragged onto its user is picked up as its board item, used in hand it is put
//!   down and ridden; a wheelchair dragged onto its user folds into its item, used in hand it
//!   unfolds.
//! - The janicart takes a trash bag (then anything else used on it goes into the bag), an empty
//!   hand takes the bag back.
//! - Lavaboats (`lavaboat.dm`): driven with an oar attached (the key), only onto lava or water
//!   or off it (`turf_check`); the rider does not burn (`crate::lava`: buckled to something lava
//!   proof). The dragon boat needs no oar, is faster, alt-click bottles it in 2 s
//!   (a ship in a bottle, used in hand it is a boat again).
//! - Examine: the integrity, the key hint, the skateboard's walk hint, the motorized
//!   wheelchair's screen.
//!
//! Cost: one relay per driver step; one pass per tick over vehicles and riders that moved.
// shortcut: no x pixel offsets of riders (only `pixel_z` through the elevation), no overlays
// drawn above the rider (wheelchair wheels, ATV cover, speedbike cover: overlays have no layer
// of their own here), no lavaboat or oar crafting recipes, no ship in a bottle shattering when
// thrown, no wheelys/roller skates/ski shoes or golf carts (sealed cars are `crate::cars`),
// no skateboard grinding on tables, no hoverboard falling or holy slowdown, no turf whitelist,
// no speed potion, no motorized wheelchair panel/parts/ramming/bomb, no wheelchair bells or
// TTVs, no janicart upgrades, no secway banana, no welder repairs, smoke or explosions, no
// projectiles hitting riders, no disarming riders off, no ollie/kickflip animation.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, Layer, Overlay, TilePos};
use ss13_tgui::server::{UiActed, UiSystems};
use ss13_world::{Bumped, Density, MoveRequest, Mover, WorldSystems};

use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::furniture::Seated;
use crate::health::{Health, Stat};
use crate::inventory::Inventory;
use crate::mecha::core::{ItemCell, modified_move_delay};
use crate::medical::surgery::Buckled;
use crate::medical::{name_of, visible};
use crate::server::PlayerMob;
use crate::sound::{PlaySound, playsound};
use crate::status_effects::Incap;
use crate::storage::{StoredIn, put_inside};
use crate::tools::{Actor, tell};

const RIDDEN: &str = "/obj/vehicle/ridden";
const SCOOTER: &str = "/obj/vehicle/ridden/scooter";
const SKATEBOARD: &str = "/obj/vehicle/ridden/scooter/skateboard";
const WHEELYS: &str = "/obj/vehicle/ridden/scooter/skateboard/wheelys";
const HOLO_SKATEBOARD: &str = "/obj/vehicle/ridden/scooter/skateboard/pro/holodeck";
const WHEELCHAIR: &str = "/obj/vehicle/ridden/wheelchair";
const MOTORIZED: &str = "/obj/vehicle/ridden/wheelchair/motorized";
const ITEM_WHEELCHAIR: &str = "/obj/item/wheelchair";
const ITEM_SKATEBOARD: &str = "/obj/item/melee/skateboard";
const TRASH_BAG: &str = "/obj/item/storage/bag/trash";
const BLUESPACE_BAG: &str = "/obj/item/storage/bag/trash/bluespace";
const CELL: &str = "/obj/item/stock_parts/power_store/cell";
const HELMET: &str = "/obj/item/clothing/head/helmet";
const HARDHAT: &str = "/obj/item/clothing/head/utility/hardhat";
const LAVABOAT: &str = "/obj/vehicle/ridden/lavaboat";
/// The dragon boat, which needs no oar and goes back into its bottle.
pub const DRAGON_BOAT: &str = "/obj/vehicle/ridden/lavaboat/dragon";
pub const SHIP_IN_A_BOTTLE: &str = "/obj/item/ship_in_a_bottle";
const OAR: &str = "/obj/item/oar";
/// The lavaboat riding component's `allowed_turfs`.
const BOAT_TURFS: [&str; 2] = ["/turf/open/lava", "/turf/open/water"];
const GLASS_BREAK: &str = "sound/effects/glass/glassbr1.ogg";
/// The dragon boat's `click_alt` `do_after`, deciseconds.
const BOTTLE_DS: f32 = 20.0;

/// `OBJ_LAYER`, `ABOVE_MOB_LAYER`.
const OBJ_LAYER: f32 = 3.0;
const ABOVE_MOB_LAYER: f32 = 4.1;

/// `RIDER_NEEDS_LEGS`, `RIDER_NEEDS_ARMS`, `UNBUCKLE_DISABLED_RIDER` (`ride_check_flags`).
pub const NEEDS_LEGS: u8 = 1;
pub const NEEDS_ARMS: u8 = 2;
pub const UNBUCKLE_DISABLED: u8 = 4;
const LEGS_ARMS_UNBUCKLE: u8 = NEEDS_LEGS | NEEDS_ARMS | UNBUCKLE_DISABLED;

/// The riding component's `message_cooldown` (5 s), the motorized wheelchair's `canmove` stall.
const MESSAGE_COOLDOWN_SECS: f64 = 5.0;
const STALL_SECS: f64 = 2.0;
/// `world.tick_lag`, deciseconds: the diagonal delay is floored to it.
const TICK_LAG_DS: f32 = 0.5;
/// `/datum/component/riding/vehicle/wheelchair/hand::delay_multiplier`,
/// `/obj/vehicle/ridden/wheelchair::delay_multiplier` (the motorized one's).
const HAND_DELAY_MULTIPLIER: f32 = 6.2;
const MOTOR_DELAY_MULTIPLIER: f32 = 6.7;
/// `refresh_parts` with the default two tier 1 servos: `1 + 1 + 1`; `power_efficiency` of the
/// tier 1 capacitor.
const MOTOR_SPEED: f32 = 3.0;
const MOTOR_EFFICIENCY: f64 = 1.0;
/// `energy_usage = 0.1 * STANDARD_CELL_CHARGE`; a step uses `energy_usage / efficiency * 0.05`.
const MOTOR_ENERGY_USAGE: f64 = 0.1 * 10_000.0;
/// The skateboard riding component's walk intent slowdown, deciseconds.
const WALK_SLOWDOWN: f32 = 0.6;
/// `next_crash = world.time + 10`.
const CRASH_COOLDOWN_SECS: f64 = 1.0;
const BANG: &str = "sound/effects/bang.ogg";
const OLLIE_SOUND: &str = "sound/vehicles/skateboard_ollie.ogg";
const ACTIONS_DMI: &str = "icons/mob/actions/actions_vehicle.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
/// Action button ids of this module.
const BUTTON: &str = "vehicle:";
const OLLIE: &str = "vehicle:ollie";
const KICKFLIP: &str = "vehicle:kickflip";

/// What a vehicle type does beyond its [`RidingDef`] numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Skateboard,
    /// A hand wheelchair (its delay follows the driver's hands).
    Wheelchair,
    Motorized,
    Janicart,
    /// A lavaboat: only on lava or water (`allowed_turf_typecache`).
    Lavaboat,
}

/// The riding component subtype of a vehicle type (`/datum/component/riding/vehicle/*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RidingDef {
    pub kind: Kind,
    /// `vehicle_move_delay`, deciseconds (wheelchairs compute theirs each step).
    pub move_delay: f32,
    /// `keytype`.
    pub key: Option<&'static str>,
    /// `ride_check_flags`.
    pub flags: u8,
    /// `override_allow_spacemove`.
    pub spacemove: bool,
    /// The rider's `pixel_z` facing north, south, east, west.
    pub rider_up: [i16; 4],
    /// The vehicle's pixel offset and layer per facing while ridden (layer `None`: kept).
    pub parent: [(i16, i16, Option<f32>); 4],
    /// Skateboards: `instability`.
    pub instability: f32,
    /// The board item a skateboard is picked up as, the item a wheelchair folds into.
    pub item: Option<&'static str>,
}

/// `/datum/component/riding/get_parent_offsets_and_layers`.
const DEFAULT_PARENT: [(i16, i16, Option<f32>); 4] = [
    (0, 0, Some(OBJ_LAYER)),
    (0, 0, Some(ABOVE_MOB_LAYER)),
    (0, 0, Some(ABOVE_MOB_LAYER)),
    (0, 0, Some(ABOVE_MOB_LAYER)),
];
const KEEP_PARENT: [(i16, i16, Option<f32>); 4] = [(0, 0, None); 4];

/// The riding component of `path`, `None` for what this module does not ride.
pub fn riding_def(path: &str) -> Option<RidingDef> {
    let under = |root: &str| is_under(path, root);
    if !under(RIDDEN) || under(WHEELYS) {
        return None;
    }
    let base = RidingDef {
        kind: Kind::Plain,
        move_delay: 2.0,
        key: None,
        flags: 0,
        spacemove: false,
        rider_up: [0; 4],
        parent: DEFAULT_PARENT,
        instability: 0.0,
        item: None,
    };
    let def = if under(SKATEBOARD) {
        let (move_delay, instability, item, spacemove) =
            if under("/obj/vehicle/ridden/scooter/skateboard/hoverboard/holyboarded") {
                (1.0, 3.0, "/obj/item/melee/skateboard/holyboard", true)
            } else if under("/obj/vehicle/ridden/scooter/skateboard/hoverboard/admin") {
                (
                    1.0,
                    0.0,
                    "/obj/item/melee/skateboard/hoverboard/admin",
                    true,
                )
            } else if under("/obj/vehicle/ridden/scooter/skateboard/hoverboard") {
                (1.0, 3.0, "/obj/item/melee/skateboard/hoverboard", true)
            } else if under("/obj/vehicle/ridden/scooter/skateboard/pro") {
                (1.0, 6.0, "/obj/item/melee/skateboard/pro", false)
            } else if under("/obj/vehicle/ridden/scooter/skateboard/improvised") {
                (1.5, 12.0, "/obj/item/melee/skateboard/improvised", false)
            } else {
                (1.5, 10.0, ITEM_SKATEBOARD, false)
            };
        RidingDef {
            kind: Kind::Skateboard,
            move_delay,
            flags: NEEDS_LEGS | UNBUCKLE_DISABLED,
            spacemove,
            rider_up: [5; 4],
            parent: [
                (0, 0, Some(ABOVE_MOB_LAYER)),
                (0, 0, Some(ABOVE_MOB_LAYER)),
                (0, 0, Some(OBJ_LAYER)),
                (0, 0, Some(OBJ_LAYER)),
            ],
            instability,
            // `skateboard/pro/holodeck/pick_up_board`: never picked up.
            item: (!under(HOLO_SKATEBOARD)).then_some(item),
            ..base
        }
    } else if under(SCOOTER) {
        RidingDef {
            flags: LEGS_ARMS_UNBUCKLE,
            rider_up: [2; 4],
            ..base
        }
    } else if under(MOTORIZED) {
        RidingDef {
            kind: Kind::Motorized,
            move_delay: 0.0,
            flags: NEEDS_ARMS,
            rider_up: [2; 4],
            parent: KEEP_PARENT,
            ..base
        }
    } else if under(WHEELCHAIR) {
        let item = if under("/obj/vehicle/ridden/wheelchair/gold") {
            "/obj/item/wheelchair/gold"
        } else {
            ITEM_WHEELCHAIR
        };
        RidingDef {
            kind: Kind::Wheelchair,
            move_delay: 0.0,
            flags: NEEDS_ARMS,
            rider_up: [2; 4],
            parent: KEEP_PARENT,
            item: Some(item),
            ..base
        }
    } else if under("/obj/vehicle/ridden/atv") {
        RidingDef {
            move_delay: 1.5,
            key: Some("/obj/item/key/atv"),
            flags: LEGS_ARMS_UNBUCKLE,
            rider_up: [4; 4],
            parent: [
                (0, 0, Some(OBJ_LAYER)),
                (0, 0, Some(ABOVE_MOB_LAYER)),
                (0, 0, Some(OBJ_LAYER)),
                (0, 0, Some(OBJ_LAYER)),
            ],
            ..base
        }
    } else if under("/obj/vehicle/ridden/bicycle") {
        RidingDef {
            move_delay: 0.0,
            flags: LEGS_ARMS_UNBUCKLE,
            rider_up: [4; 4],
            ..base
        }
    } else if under("/obj/vehicle/ridden/secway") {
        RidingDef {
            move_delay: 1.75,
            key: Some("/obj/item/key/security"),
            flags: LEGS_ARMS_UNBUCKLE,
            rider_up: [4; 4],
            ..base
        }
    } else if under("/obj/vehicle/ridden/speedbike") {
        RidingDef {
            move_delay: 0.0,
            flags: LEGS_ARMS_UNBUCKLE,
            spacemove: true,
            rider_up: [-8, 4, 5, 5],
            parent: [
                (-16, -16, None),
                (-16, -16, None),
                (-18, 0, None),
                (-18, 0, None),
            ],
            ..base
        }
    } else if under(LAVABOAT) {
        let dragon = under(DRAGON_BOAT);
        RidingDef {
            kind: Kind::Lavaboat,
            move_delay: if dragon { 1.0 } else { 2.0 },
            key: (!dragon).then_some(OAR),
            rider_up: if dragon { [2; 4] } else { [0; 4] },
            parent: [(0, 0, Some(ABOVE_MOB_LAYER)); 4],
            ..base
        }
    } else if under("/obj/vehicle/ridden/janicart") {
        RidingDef {
            kind: Kind::Janicart,
            key: Some("/obj/item/key/janitor"),
            rider_up: [4, 7, 7, 7],
            ..base
        }
    } else {
        return None;
    };
    Some(def)
}

/// A vehicle this module rides (`can_buckle` through the ridable element).
pub fn is_ridable(path: &str) -> bool {
    riding_def(path).is_some()
}

/// The live ridden vehicle (tg `/obj/vehicle/ridden` vars and its riding component's).
#[derive(Component, Clone, Debug)]
pub struct Vehicle {
    pub def: RidingDef,
    /// `inserted_key`, inside the vehicle.
    pub key: Option<Entity>,
    /// The driver (`occupants` with `VEHICLE_CONTROL_DRIVE`; `max_occupants = 1`).
    pub rider: Option<Entity>,
    /// The janicart's `trash_bag`, inside it.
    pub bag: Option<Entity>,
    /// The skateboard's `board_item`, inside it (spawned when it is picked up otherwise).
    pub board: Option<Entity>,
    /// The motorized wheelchair's `power_cell`, inside it; `cell_ready` once it got its default.
    pub cell: Option<Entity>,
    cell_ready: bool,
    /// `vehicle_move_cooldown`, `message_cooldown`, the skateboard's `next_crash`, the motorized
    /// wheelchair's `canmove` (game time each).
    next_move: f64,
    message_at: f64,
    next_crash: f64,
    stalled_until: f64,
}

impl Vehicle {
    pub fn new(def: RidingDef) -> Self {
        Self {
            def,
            key: None,
            rider: None,
            bag: None,
            board: None,
            cell: None,
            cell_ready: false,
            next_move: 0.0,
            message_at: 0.0,
            next_crash: 0.0,
            stalled_until: 0.0,
        }
    }

    /// Game time the next step is allowed (`vehicle_move_cooldown`).
    pub fn next_move(&self) -> f64 {
        self.next_move
    }
}

/// On a mob driving a [`Vehicle`] (buckled to it).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Rider {
    pub vehicle: Entity,
    /// The vehicle's step time in seconds (the driver's HUD `move_delay`, which the client
    /// predicts with).
    pub move_delay: f32,
}

/// A step of a driver (written by `crate::server` instead of moving it): tg `relaymove`.
#[derive(Message, Clone, Copy, Debug)]
pub struct VehicleRelayMove {
    pub rider: Entity,
    pub dir: Dir,
}

pub struct VehiclesPlugin;

impl Plugin for VehiclesPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A key, a trash bag or something for the bag used on a vehicle.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2950, |w, c| {
            c.held
                .is_some_and(|h| crate::vehicles::item_interaction(w, c.client, c.mob, h, c.target))
        });
        // The janicart's bag taken back.
        app.add_click_handler(ClickPhase::EmptyHand, 3450, |w, c| {
            crate::vehicles::attack_hand(w, c.mob, c.target, c.modifiers.right)
        });
        // A folded wheelchair or a skateboard put down.
        app.add_click_handler(ClickPhase::AttackSelf, 3150, |w, c| {
            crate::vehicles::attack_self(w, c.client, c.mob, c.target)
        });
        app.add_message::<VehicleRelayMove>()
            .add_observer(add_vehicle)
            .add_observer(rider_gone)
            .add_observer(vehicle_gone)
            .add_systems(
                Update,
                (
                    relay_moves
                        .after(crate::server::forward_moves)
                        .before(WorldSystems::Movement),
                    (carry_riders, crashes)
                        .chain()
                        .after(WorldSystems::Movement)
                        .before(crate::server::ack_moves),
                    unride_orphans.after(WorldSystems::Movement),
                    trigger_buttons.in_set(UiSystems::Handle),
                ),
            );
    }
}

fn add_vehicle(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Some(def) = objs.get(add.entity).ok().and_then(|o| riding_def(&o.path)) else {
        return;
    };
    // The cooldown is the vehicle's own; the mover only takes the step.
    commands
        .entity(add.entity)
        .insert((Vehicle::new(def), Mover::new(0.0)));
}

/// The driver got off however it happened (unbuckled, gone): the vehicle is free again
/// (`vehicle_mob_unbuckle` -> `restore_parent_layer_and_offsets`).
fn rider_gone(remove: On<Remove<Rider>>, riders: Query<&Rider>, mut commands: Commands) {
    let Ok(&Rider { vehicle, .. }) = riders.get(remove.entity) else {
        return;
    };
    let mob = remove.entity;
    commands.queue(move |world: &mut World| release_vehicle(world, vehicle, mob));
}

/// A vehicle despawned with its driver on: the driver is off.
fn vehicle_gone(remove: On<Remove<Vehicle>>, vehicles: Query<&Vehicle>, mut commands: Commands) {
    if let Some(rider) = vehicles.get(remove.entity).ok().and_then(|v| v.rider) {
        commands.queue(move |world: &mut World| dismount(world, rider));
    }
}

// --- Helpers ------------------------------------------------------------------------------

fn facing(world: &World, e: Entity) -> Dir {
    world.get::<Facing>(e).map_or(Dir::South, |f| f.0)
}

fn set_facing(world: &mut World, e: Entity, dir: Dir) {
    match world.get_mut::<Facing>(e) {
        Some(mut f) => {
            f.set_if_neq(Facing(dir));
        }
        None => {
            world.entity_mut(e).insert(Facing(dir));
        }
    }
}

/// The `"[dir]"` key of the riding offset lists: none for diagonals.
fn dir_index(dir: Dir) -> Option<usize> {
    match dir {
        Dir::North => Some(0),
        Dir::South => Some(1),
        Dir::East => Some(2),
        Dir::West => Some(3),
        _ => None,
    }
}

use crate::tools::client_of;

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = client_of(world, mob) {
        tell(world, client, span, text);
    }
}

fn sound(world: &mut World, at: Entity, file: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        playsound(world, PlaySound::new(pos, file, vol, true));
    }
}

fn path_of(world: &World, e: Entity) -> String {
    crate::construction::path_of(world, e).unwrap_or_default()
}

/// The item is still inside `container` (tg `Exited` clears the vehicle's references).
fn inside(world: &World, item: Option<Entity>, container: Entity) -> Option<Entity> {
    item.filter(|&i| world.get::<StoredIn>(i).is_some_and(|s| s.0 == container))
}

fn stamina(world: &World, mob: Entity) -> f32 {
    world
        .get::<crate::status_effects::StatusEffects>(mob)
        .map_or(0.0, |e| e.stamina)
}

fn put_in_hands(world: &mut World, mob: Entity, item: Entity) {
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, item);
    } else if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::storage::take_out(world, item, pos);
    }
}

/// `usable_hands`.
fn usable_hands(world: &World, mob: Entity) -> usize {
    (0..2)
        .filter(|&h| crate::medical::bodyparts::has_hand(world, mob, h))
        .count()
}

/// The vehicle's `vehicle_move_delay` for `driver`'s next step, deciseconds (`driver_move` of
/// the wheelchairs and the skateboard's walk intent toggle).
pub fn base_delay(def: &RidingDef, hands: usize, walking: bool) -> f32 {
    let run = crate::RUN_DELAY_SECS * 10.0;
    match def.kind {
        // DM `round(x)` with one argument floors.
        Kind::Wheelchair => (run * HAND_DELAY_MULTIPLIER).floor() / hands.clamp(1, 2) as f32,
        Kind::Motorized => (run * MOTOR_DELAY_MULTIPLIER).floor() / MOTOR_SPEED,
        Kind::Skateboard if walking => def.move_delay + WALK_SLOWDOWN,
        _ => def.move_delay,
    }
}

/// `handle_ride`'s cooldown in seconds: diagonal steps `FLOOR(delay * sqrt(2), tick_lag)`, then
/// BandaStation's `modified_move_delay`.
pub fn step_cooldown(delay: f32, diagonal: bool) -> f64 {
    let delay = if diagonal {
        (delay * std::f32::consts::SQRT_2 / TICK_LAG_DS).floor() * TICK_LAG_DS
    } else {
        delay
    };
    f64::from(modified_move_delay(delay)) / 10.0
}

fn driver_delay(world: &World, vehicle: Entity, driver: Entity) -> f32 {
    let Some(v) = world.get::<Vehicle>(vehicle) else {
        return 0.0;
    };
    let walking = world
        .get::<crate::janitor::Walking>(driver)
        .is_some_and(|w| w.0);
    base_delay(&v.def, usable_hands(world, driver), walking)
}

/// The driver's HUD pace follows the vehicle (the client predicts its steps with it).
fn refresh_pace(world: &mut World, vehicle: Entity, driver: Entity) {
    let secs = step_cooldown(driver_delay(world, vehicle, driver), false) as f32;
    if let Some(mut r) = world.get_mut::<Rider>(driver)
        && r.move_delay != secs
    {
        r.move_delay = secs;
    }
}

// --- Mounting ------------------------------------------------------------------------------

/// After `crate::medical::surgery::buckle` put `mob` on `vehicle` (`post_buckle_mob` ->
/// `add_occupant`, the riding component's `vehicle_mob_buckle`) and `buckle_feedback`.
pub(crate) fn post_buckle(world: &mut World, actor: Actor, mob: Entity, vehicle: Entity) {
    let Some(kind) = world.get::<Vehicle>(vehicle).map(|v| v.def.kind) else {
        return;
    };
    world.get_mut::<Vehicle>(vehicle).expect("checked").rider = Some(mob);
    world.entity_mut(mob).insert((
        Rider {
            vehicle,
            move_delay: 0.0,
        },
        Seated {
            chair: vehicle,
            up: 0,
        },
    ));
    // `post_buckle_mob`: two skaters collide.
    if kind == Kind::Skateboard {
        world.entity_mut(vehicle).insert(Density);
    }
    refresh_pace(world, vehicle, mob);
    refresh_look(world, vehicle);
    refresh_buttons(world, mob);
    let (user, target, what) = (
        name_of(world, actor.mob),
        name_of(world, mob),
        name_of(world, vehicle),
    );
    if mob == actor.mob {
        visible(
            world,
            mob,
            &format!("Вы пристегиваете себя к {what}."),
            &format!("{user} пристегивает себя к {what}."),
            Span::Notice,
        );
    } else {
        visible(
            world,
            mob,
            &format!("{user} пристегивает вас к {what}!"),
            &format!("{user} пристегивает {target} к {what}!"),
            Span::Warning,
        );
    }
}

/// After `crate::medical::surgery::unbuckle` freed `mob` from `vehicle`: `unbuckle_feedback`.
pub(crate) fn post_unbuckle(world: &mut World, actor: Actor, mob: Entity, vehicle: Entity) {
    unride(world, mob);
    let (user, target, what) = (
        name_of(world, actor.mob),
        name_of(world, mob),
        name_of(world, vehicle),
    );
    if mob == actor.mob {
        visible(
            world,
            mob,
            &format!("Вы отстегиваете себя от {what}."),
            &format!("{user} отстегивает себя от {what}."),
            Span::Notice,
        );
    } else {
        visible(
            world,
            mob,
            &format!("{user} отстёгивает вас от {what}."),
            &format!("{user} отстёгивает {target} от {what}."),
            Span::Notice,
        );
    }
}

/// `unbuckle_mob(rider, force)` without feedback: off the vehicle where it stands.
pub fn dismount(world: &mut World, mob: Entity) {
    if world.get_entity(mob).is_err() {
        return;
    }
    world.entity_mut(mob).remove::<Buckled>();
    unride(world, mob);
}

/// `remove_occupant` + `restore_rider_layer_and_offsets` on the mob, the vehicle freed at once
/// (what comes next may pick it up); [`rider_gone`] covers riders removed otherwise.
fn unride(world: &mut World, mob: Entity) {
    if world.get_entity(mob).is_err() {
        return;
    }
    let Some(vehicle) = world.get::<Rider>(mob).map(|r| r.vehicle) else {
        return;
    };
    release_vehicle(world, vehicle, mob);
    world.entity_mut(mob).remove::<(Rider, Seated)>();
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::tables::reelevate(world, pos);
    }
    refresh_buttons(world, mob);
}

/// `restore_parent_layer_and_offsets`; a skateboard is no longer dense.
fn release_vehicle(world: &mut World, vehicle: Entity, mob: Entity) {
    let Some(mut v) = world.get_mut::<Vehicle>(vehicle) else {
        return;
    };
    if v.rider != Some(mob) {
        return;
    }
    v.rider = None;
    let kind = v.def.kind;
    if kind == Kind::Skateboard {
        world.entity_mut(vehicle).remove::<Density>();
    }
    let base = world
        .get::<DmAtom>(vehicle)
        .map(|a| (a.layer, [a.pixel_x, a.pixel_y]));
    if let Some((layer, pixel)) = base
        && let Some(mut appearance) = world.get_mut::<Appearance>(vehicle)
    {
        if appearance.layer != Layer(layer) {
            appearance.layer = Layer(layer);
        }
        if appearance.pixel_offset != pixel {
            appearance.pixel_offset = pixel;
        }
    }
}

/// `update_parent_layer_and_offsets`: the rider faces the vehicle's way, drawn up for it; the
/// vehicle takes its ridden layer and offset.
fn refresh_look(world: &mut World, vehicle: Entity) {
    let Some((def, Some(rider))) = world.get::<Vehicle>(vehicle).map(|v| (v.def, v.rider)) else {
        return;
    };
    let dir = facing(world, vehicle);
    let index = dir_index(dir);
    set_facing(world, rider, dir);
    let up = index.map_or(0, |i| def.rider_up[i]);
    let lifted = match world.get_mut::<Seated>(rider) {
        Some(mut seat) if seat.up != up => {
            seat.up = up;
            true
        }
        _ => false,
    };
    if lifted && let Some(&pos) = world.get::<TilePos>(rider) {
        crate::tables::reelevate(world, pos);
    }
    let Some(i) = index else {
        return;
    };
    let (px, py, layer) = def.parent[i];
    let base = world
        .get::<DmAtom>(vehicle)
        .map(|a| [a.pixel_x + px, a.pixel_y + py]);
    if let Some(mut appearance) = world.get_mut::<Appearance>(vehicle) {
        if let Some(pixel) = base
            && appearance.pixel_offset != pixel
        {
            appearance.pixel_offset = pixel;
        }
        if let Some(layer) = layer
            && appearance.layer != Layer(layer)
        {
            appearance.layer = Layer(layer);
        }
    }
}

// --- Driving -------------------------------------------------------------------------------

/// tg `relaymove` -> `driver_move` for each driver step.
fn relay_moves(world: &mut World, mut relays: Local<MessageCursor<VehicleRelayMove>>) {
    let steps: Vec<VehicleRelayMove> = relays
        .read(world.resource::<Messages<VehicleRelayMove>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for step in steps {
        let Some(rider) = world.get::<Rider>(step.rider).copied() else {
            continue;
        };
        driver_move(world, rider.vehicle, step.rider, step.dir, now);
    }
}

/// A warning to the driver at most every [`MESSAGE_COOLDOWN_SECS`].
fn throttled(world: &mut World, vehicle: Entity, driver: Entity, now: f64, text: String) {
    let Some(mut v) = world.get_mut::<Vehicle>(vehicle) else {
        return;
    };
    if now < v.message_at {
        return;
    }
    v.message_at = now + MESSAGE_COOLDOWN_SECS;
    tell_mob(world, driver, Span::Warning, text);
}

/// The motorized wheelchair's default cell (`add_component_parts`), once.
fn ensure_cell(world: &mut World, vehicle: Entity) {
    let Some(v) = world.get::<Vehicle>(vehicle) else {
        return;
    };
    if v.def.kind != Kind::Motorized || v.cell_ready {
        return;
    }
    let cell = crate::mecha::core::spawn_inside(world, CELL, vehicle);
    if let Some(mut v) = world.get_mut::<Vehicle>(vehicle) {
        v.cell = cell;
        v.cell_ready = true;
    }
}

fn cell_charge(world: &World, vehicle: Entity) -> Option<f64> {
    let cell = inside(world, world.get::<Vehicle>(vehicle)?.cell, vehicle)?;
    Some(world.get::<ItemCell>(cell).map_or(0.0, |c| c.0.charge))
}

/// tg `/obj/vehicle/relaymove` (`is_driver`, `canmove`, the skateboard's crash cooldown, the
/// motorized wheelchair's power) and the riding component's `driver_move`. True when the
/// vehicle stepped.
pub fn driver_move(world: &mut World, vehicle: Entity, driver: Entity, dir: Dir, now: f64) -> bool {
    let Some(v) = world.get::<Vehicle>(vehicle).cloned() else {
        return false;
    };
    if v.rider != Some(driver) || now < v.stalled_until {
        return false;
    }
    let name = name_of(world, vehicle);
    if v.def.kind == Kind::Skateboard && now < v.next_crash {
        return false;
    }
    if v.def.kind == Kind::Motorized {
        ensure_cell(world, vehicle);
        let refusal = match cell_charge(world, vehicle) {
            None => Some(format!("Похоже, в {name} не установлена батарея.")),
            Some(charge) if charge < MOTOR_ENERGY_USAGE / MOTOR_EFFICIENCY.max(1.0) => {
                Some(format!("Дисплей {name} мигает: «Нет питания»."))
            }
            Some(_) => None,
        };
        if let Some(text) = refusal {
            tell_mob(world, driver, Span::Warning, text);
            world
                .get_mut::<Vehicle>(vehicle)
                .expect("checked")
                .stalled_until = now + STALL_SECS;
            return false;
        }
    }
    if now < v.next_move {
        return false;
    }
    // `keycheck`: a vehicle's key is the inserted one.
    if let Some(key) = v.def.key {
        let fits = inside(world, v.key, vehicle).is_some_and(|k| {
            world
                .get::<DmItem>(k)
                .is_some_and(|i| is_under(&i.path, key))
        });
        if !fits {
            throttled(
                world,
                vehicle,
                driver,
                now,
                format!("В {name} не вставлен ключ!"),
            );
            return false;
        }
    }
    let traits = crate::status_effects::incapacitation(world, driver);
    let stat = world.get::<Stat>(driver).copied().unwrap_or_default();
    let floored = traits.floored || world.get::<Health>(driver).is_some_and(|h| h.floored(stat));
    let incapacitated = traits.incapacitated || matches!(stat, Stat::HardCrit | Stat::Dead);
    let refusal = if incapacitated {
        Some((
            format!("Ваше тело обмякает, и вы соскальзываете с {name}!"),
            format!("Вы не можете сейчас управлять {name}!"),
        ))
    } else if v.def.flags & NEEDS_LEGS != 0 && floored {
        Some((
            format!("Вы пытаетесь управлять {name}, не стоя на ногах, и падаете!"),
            format!("Не стоя на ногах, вам не управиться с {name}..."),
        ))
    } else if v.def.flags & NEEDS_ARMS != 0 && traits.hands_blocked {
        Some((
            format!("Вы не можете держаться за {name} и падаете!"),
            format!("Вам не удержаться за {name}, чтобы ехать..."),
        ))
    } else {
        None
    };
    if let Some((fall, cannot)) = refusal {
        if v.def.flags & UNBUCKLE_DISABLED != 0 {
            dismount(world, driver);
            let who = name_of(world, driver);
            visible(
                world,
                driver,
                &fall,
                &format!("{who} падает с {name}."),
                Span::Danger,
            );
            crate::status_effects::apply(world, driver, Incap::Stun, 3.0);
        }
        throttled(world, vehicle, driver, now, cannot);
        return false;
    }
    handle_ride(world, vehicle, driver, dir, now)
}

/// tg `handle_ride`: the step, its cooldown, the motorized wheelchair's energy.
fn handle_ride(world: &mut World, vehicle: Entity, driver: Entity, dir: Dir, now: f64) -> bool {
    let Some(v) = world.get::<Vehicle>(vehicle).cloned() else {
        return false;
    };
    let Some(&from) = world.get::<TilePos>(vehicle) else {
        return false;
    };
    if v.def.kind == Kind::Lavaboat && !boat_turf_check(world, from, from.step(dir)) {
        let (name, turf) = (
            name_of(world, vehicle),
            crate::construction::turf_at(world, from.step(dir))
                .map(|t| name_of(world, t))
                .unwrap_or_default(),
        );
        tell_mob(
            world,
            driver,
            Span::Warning,
            format!("{name} не может заехать на {turf}!"),
        );
        return false;
    }
    // `Process_Spacemove`: `override_allow_spacemove || has_gravity()`.
    if !v.def.spacemove && !crate::multiz::has_gravity(world, from) {
        return false;
    }
    world.write_message(MoveRequest {
        entity: vehicle,
        dir,
    });
    let diagonal = !Dir::CARDINALS.contains(&dir);
    let cooldown = step_cooldown(driver_delay(world, vehicle, driver), diagonal);
    world
        .get_mut::<Vehicle>(vehicle)
        .expect("checked")
        .next_move = now + cooldown;
    refresh_pace(world, vehicle, driver);
    if v.def.kind == Kind::Motorized
        && let Some(cell) = inside(world, v.cell, vehicle)
        && let Some(mut c) = world.get_mut::<ItemCell>(cell)
    {
        // `power_cell.use(energy_usage / max(power_efficiency, 1) * 0.05)`.
        let _ =
            c.0.take(MOTOR_ENERGY_USAGE / MOTOR_EFFICIENCY.max(1.0) * 0.05, false);
    }
    true
}

/// tg `turf_check(next, current)` with the lavaboat's `allowed_turf_typecache`: onto lava or
/// water, or off it onto anything else (so a boat can be beached, once).
pub fn boat_turf_check(world: &World, current: TilePos, next: TilePos) -> bool {
    let allowed = |pos: TilePos| {
        crate::construction::turf_at(world, pos)
            .and_then(|t| crate::construction::path_of(world, t))
            .is_some_and(|p| BOAT_TURFS.iter().any(|root| is_under(&p, root)))
    };
    allowed(next) || allowed(current)
}

/// The rider moves with its vehicle (`Moved` of buckled mobs) and turns with it; a rider moved
/// away some other way (thrown, teleported) is off (`crate::medical::surgery` leaves riders to
/// this module, so a pulled vehicle keeps its rider).
// Cost: per tick, the vehicles and riders that moved or turned.
fn carry_riders(world: &mut World) {
    let moved: Vec<Entity> = world
        .query_filtered::<Entity, (With<Vehicle>, Or<(Changed<TilePos>, Changed<Facing>)>)>()
        .iter(world)
        .collect();
    for vehicle in moved {
        carry_rider(world, vehicle);
    }
    let strays: Vec<Entity> = world
        .query_filtered::<(Entity, &Rider, &TilePos), Changed<TilePos>>()
        .iter(world)
        .filter(|(_, r, pos)| world.get::<TilePos>(r.vehicle) != Some(*pos))
        .map(|(e, ..)| e)
        .collect();
    for mob in strays {
        dismount(world, mob);
    }
}

/// The rider onto the vehicle's tile, facing and offsets refreshed.
fn carry_rider(world: &mut World, vehicle: Entity) {
    let Some(rider) = world.get::<Vehicle>(vehicle).and_then(|v| v.rider) else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(vehicle) else {
        return;
    };
    if world.get_entity(rider).is_err() {
        return;
    }
    if world.get::<TilePos>(rider) != Some(&pos) {
        world.entity_mut(rider).insert(pos);
    }
    refresh_look(world, vehicle);
}

/// A rider whose buckle ended without this module (the vehicle gone, an unbuckle that skipped
/// `post_unbuckle`) is off.
fn unride_orphans(world: &mut World) {
    let orphans: Vec<Entity> = world
        .query_filtered::<Entity, (With<Rider>, Without<Buckled>)>()
        .iter(world)
        .collect();
    for mob in orphans {
        unride(world, mob);
    }
}

// --- Crashes -------------------------------------------------------------------------------

/// `Bump` of ridden vehicles: the skateboard's crash.
// Cost: per tick, per bump of a vehicle a few lookups.
fn crashes(world: &mut World, mut bumps: Local<MessageCursor<Bumped>>) {
    let bumped: Vec<Bumped> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .filter(|b| world.get::<Vehicle>(b.entity).is_some())
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for bump in bumped {
        let kind = world.get::<Vehicle>(bump.entity).map(|v| v.def.kind);
        if kind == Some(Kind::Skateboard) {
            skateboard_bump(world, bump.entity, bump.obstacle, now);
        }
    }
}

/// `/obj/vehicle/ridden/scooter/skateboard/Bump`.
fn skateboard_bump(world: &mut World, board: Entity, obstacle: Entity, now: f64) {
    let Some(v) = world.get::<Vehicle>(board).cloned() else {
        return;
    };
    let Some(rider) = v.rider else {
        return;
    };
    if now < v.next_crash {
        return;
    }
    // Going slow at walk intent prevents crashing (`can_slow_down`).
    if world
        .get::<crate::janitor::Walking>(rider)
        .is_some_and(|w| w.0)
    {
        return;
    }
    world.get_mut::<Vehicle>(board).expect("checked").next_crash = now + CRASH_COOLDOWN_SECS;
    crate::status_effects::adjust_stamina(world, rider, v.def.instability * 6.0);
    sound(world, board, BANG, 40.0);
    let carbon = |w: &World, e: Entity| w.get::<Inventory>(e).is_some();
    let hit_carbon = carbon(world, obstacle);
    if !carbon(world, rider) || stamina(world, rider) >= 100.0 || hit_carbon {
        dismount(world, rider);
        let dir = {
            let mut rng = world.resource_mut::<crate::combat::CombatRng>();
            Dir::CARDINALS[rng.range(0, 3) as usize]
        };
        crate::martial::fling(world, rider, dir, 3, 2.0);
        let helmet = world
            .get::<Inventory>(rider)
            .and_then(|i| i.head)
            .map(|h| path_of(world, h))
            .is_some_and(|p| is_under(&p, HELMET) || is_under(&p, HARDHAT));
        if !helmet {
            crate::medical::organs::adjust_organ_loss(
                world,
                rider,
                crate::medical::organs::Slot::Brain,
                5.0,
            );
        }
        let (what, thing, who) = (
            name_of(world, board),
            name_of(world, obstacle),
            name_of(world, rider),
        );
        let text = format!("{what} врезается в {thing}, отправляя {who} в полёт!");
        visible(world, board, "", &text, Span::Danger);
        tell_mob(world, rider, Span::Danger, text);
        crate::status_effects::apply(world, rider, Incap::Paralyze, 8.0);
        if hit_carbon {
            crate::status_effects::knockdown(world, obstacle, 4.0, 0.0);
        }
    } else {
        // `step(src, REVERSE_DIR(dir))`, `rider.spin(4, 1)`.
        let back = facing(world, board).reverse();
        if crate::pull::step_once(world, board, back, None).is_ok() {
            carry_rider(world, board);
        }
    }
}

// --- Keys, bags, boards ---------------------------------------------------------------------

/// `transferItemToLoc` of a held item into `container`; false when it is stuck to the hand.
fn hand_into(world: &mut World, client: Entity, item: Entity, container: Entity) -> bool {
    if crate::inventory::no_drop(world, item) {
        let what = name_of(world, item);
        tell(
            world,
            client,
            Span::Warning,
            format!("{what} прилип к вашей руке!"),
        );
        return false;
    }
    put_inside(world, item, container);
    true
}

/// `/obj/vehicle/ridden/item_interaction` (the key) and the janicart's (the trash bag, then
/// anything into it). True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    _mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(v) = world.get::<Vehicle>(target).cloned() else {
        return false;
    };
    let held_path = path_of(world, held);
    let key = inside(world, v.key, target);
    let key_fits = |w: &World, k: Option<Entity>| {
        v.def
            .key
            .zip(k)
            .is_some_and(|(kt, k)| is_under(&path_of(w, k), kt))
    };
    let (what, name) = (name_of(world, held), name_of(world, target));
    if let Some(key_type) = v.def.key
        && is_under(&held_path, key_type)
        && !key_fits(world, key)
    {
        if !hand_into(world, client, held, target) {
            return true;
        }
        let text = if v.def.kind == Kind::Lavaboat {
            format!("Вы прикрепляете {what} к {name}.")
        } else {
            format!("Вы вставляете {what} в {name}.")
        };
        tell(world, client, Span::Notice, text);
        // Just in case there's an invalid key.
        if let Some(old) = key
            && let Some(&pos) = world.get::<TilePos>(target)
        {
            crate::storage::take_out(world, old, pos);
        }
        world.get_mut::<Vehicle>(target).expect("checked").key = Some(held);
        return true;
    }
    if v.def.kind != Kind::Janicart {
        return false;
    }
    let bag = inside(world, v.bag, target);
    if is_under(&held_path, TRASH_BAG) {
        if bag.is_some() {
            tell(
                world,
                client,
                Span::Warning,
                format!("На {name} уже висит мешок для мусора!"),
            );
            return true;
        }
        if !hand_into(world, client, held, target) {
            return true;
        }
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы цепляете мешок для мусора к {name}."),
        );
        world.get_mut::<Vehicle>(target).expect("checked").bag = Some(held);
        refresh_bag_overlay(world, target);
        return true;
    }
    // Don't put a key in the trash when we need it.
    let needed_key = v.def.key.is_some_and(|k| is_under(&held_path, k)) && !key_fits(world, key);
    if let Some(bag) = bag
        && !needed_key
    {
        let _ = crate::storage::insert_from_hand(world, held, bag);
        return true;
    }
    false
}

/// `/obj/vehicle/ridden/janicart/attack_hand`: a right click takes the bag off while someone
/// rides, a left click when nobody does (else it unbuckles, in `crate::medical`). True when the
/// bag came off.
pub fn attack_hand(world: &mut World, mob: Entity, target: Entity, right: bool) -> bool {
    let Some(v) = world.get::<Vehicle>(target).cloned() else {
        return false;
    };
    if v.def.kind != Kind::Janicart || (!right && v.rider.is_some()) {
        return false;
    }
    let Some(bag) = inside(world, v.bag, target) else {
        return false;
    };
    world.get_mut::<Vehicle>(target).expect("checked").bag = None;
    put_in_hands(world, mob, bag);
    refresh_bag_overlay(world, target);
    true
}

/// `update_overlays` of the janicart: `cart_garbage` (bluespace: `cart_bluespace_garbage`).
fn refresh_bag_overlay(world: &mut World, cart: Entity) {
    let Some(icon) = world.get::<DmAtom>(cart).map(|a| a.icon.clone()) else {
        return;
    };
    let lookup = |state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&icon, state))
    };
    let (plain, bluespace) = (lookup("cart_garbage"), lookup("cart_bluespace_garbage"));
    let bag = world
        .get::<Vehicle>(cart)
        .and_then(|v| inside(world, v.bag, cart));
    let wanted = bag.and_then(|b| {
        if is_under(&path_of(world, b), BLUESPACE_BAG) {
            bluespace
        } else {
            plain
        }
    });
    let Some(mut appearance) = world.get_mut::<Appearance>(cart) else {
        return;
    };
    let ours = |id| Some(id) == plain || Some(id) == bluespace;
    let current = appearance
        .overlays
        .iter()
        .map(|o| o.icon_state)
        .find(|&id| ours(id));
    if current == wanted {
        return;
    }
    appearance.overlays.retain(|o| !ours(o.icon_state));
    if let Some(id) = wanted {
        appearance.overlays.push(Overlay::new(id));
    }
}

/// `/obj/vehicle/ridden/click_alt`: the driver takes the key back. True when it was a vehicle
/// with a key.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(v) = world.get::<Vehicle>(target).cloned() else {
        return false;
    };
    // `/obj/vehicle/ridden/lavaboat/dragon/click_alt`.
    if is_under(&path_of(world, target), DRAGON_BOAT) {
        crate::balloon::balloon_alert(world, mob, mob, "бутилируем лодку...");
        let actor = Actor {
            client,
            mob,
            item: None,
        };
        crate::tools::do_after(
            world,
            actor,
            target,
            BOTTLE_DS,
            0,
            crate::construction::Job::BottleBoat,
        );
        return true;
    }
    let Some(key) = inside(world, v.key, target) else {
        return v.def.key.is_some();
    };
    let (name, what) = (name_of(world, target), name_of(world, key));
    if v.rider != Some(mob) {
        tell(
            world,
            client,
            Span::Warning,
            format!("Нужно сидеть на {name}, чтобы вынуть {what}!"),
        );
        return true;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы вынимаете {what} из {name}."),
    );
    world.get_mut::<Vehicle>(target).expect("checked").key = None;
    put_in_hands(world, mob, key);
    true
}

/// The end of the dragon boat's `click_alt` `do_after`: a ship in a bottle in the hand, the
/// boat gone (its rider gets off first).
pub fn complete_bottle(world: &mut World, mob: Entity, boat: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    if world.get::<Vehicle>(boat).is_none() {
        return;
    }
    crate::balloon::balloon_alert(world, mob, mob, "лодка в бутылке!");
    if let Some(rider) = world.get::<Vehicle>(boat).and_then(|v| v.rider) {
        dismount(world, rider);
    }
    if let Some(&boat_pos) = world.get::<TilePos>(boat) {
        drop_contents(world, boat, boat_pos);
        world.despawn(boat);
        crate::construction::tile_changed(world, boat_pos);
    } else {
        world.despawn(boat);
    }
    if let Some(bottle) = crate::construction::spawn_obj(world, SHIP_IN_A_BOTTLE, pos) {
        put_in_hands(world, mob, bottle);
    }
}

/// A skateboard (`pick_up_board`) or a wheelchair (folding) dragged onto its user. True when
/// the drag was theirs.
pub(crate) fn mouse_drop_dragged(world: &mut World, actor: Actor, dragged: Entity) -> bool {
    let Some(v) = world.get::<Vehicle>(dragged).cloned() else {
        return false;
    };
    match v.def.kind {
        Kind::Skateboard => {
            pick_up_board(world, actor.mob, dragged, false);
            true
        }
        Kind::Wheelchair => {
            fold(world, actor.mob, dragged);
            true
        }
        _ => false,
    }
}

/// `/obj/vehicle/ridden/scooter/skateboard/pick_up_board`.
fn pick_up_board(world: &mut World, mob: Entity, board: Entity, forced: bool) {
    let Some(v) = world.get::<Vehicle>(board).cloned() else {
        return;
    };
    if v.def.item.is_none() {
        return;
    }
    if world.get::<Inventory>(mob).is_none() {
        return;
    }
    if !forced && crate::status_effects::incapacitation(world, mob).incapacitated {
        return;
    }
    if v.rider.is_some() {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "Нельзя поднять, пока на нём кто-то стоит.",
        );
        return;
    }
    let Some(&pos) = world.get::<TilePos>(board) else {
        return;
    };
    let item = match inside(world, v.board, board) {
        Some(item) => Some(item),
        None => v
            .def
            .item
            .and_then(|path| crate::construction::spawn_obj(world, path, pos)),
    };
    let Some(item) = item else {
        return;
    };
    put_in_hands(world, mob, item);
    drop_contents(world, board, pos);
    world.despawn(board);
    crate::construction::tile_changed(world, pos);
}

/// What is inside a vehicle about to go (the key, the bag) drops on its tile.
fn drop_contents(world: &mut World, vehicle: Entity, pos: TilePos) {
    let contents: Vec<Entity> = world
        .get::<crate::storage::Stored>(vehicle)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for item in contents {
        crate::storage::take_out(world, item, pos);
    }
}

/// `/obj/vehicle/ridden/wheelchair/mouse_drop_dragged` onto a human: folded into its item.
fn fold(world: &mut World, mob: Entity, chair: Entity) {
    let Some(v) = world.get::<Vehicle>(chair).cloned() else {
        return;
    };
    let Some(item_path) = v.def.item else {
        return;
    };
    if world.get::<Inventory>(mob).is_none() || v.rider.is_some() {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(chair) else {
        return;
    };
    let Some(item) = crate::construction::spawn_obj(world, item_path, pos) else {
        return;
    };
    let (user, what) = (name_of(world, mob), name_of(world, chair));
    visible(
        world,
        mob,
        &format!("Вы складываете {what}."),
        &format!("{user} складывает {what}."),
        Span::Notice,
    );
    drop_contents(world, chair, pos);
    world.despawn(chair);
    crate::construction::tile_changed(world, pos);
    put_in_hands(world, mob, item);
}

/// The vehicle a board or folded wheelchair item stands as.
pub fn unfolded_type(item_path: &str) -> Option<&'static str> {
    const PAIRS: [(&str, &str); 8] = [
        (
            "/obj/item/melee/skateboard/hoverboard/admin",
            "/obj/vehicle/ridden/scooter/skateboard/hoverboard/admin",
        ),
        (
            "/obj/item/melee/skateboard/hoverboard",
            "/obj/vehicle/ridden/scooter/skateboard/hoverboard",
        ),
        (
            "/obj/item/melee/skateboard/holyboard",
            "/obj/vehicle/ridden/scooter/skateboard/hoverboard/holyboarded",
        ),
        (
            "/obj/item/melee/skateboard/improvised",
            "/obj/vehicle/ridden/scooter/skateboard/improvised",
        ),
        (
            "/obj/item/melee/skateboard/pro",
            "/obj/vehicle/ridden/scooter/skateboard/pro",
        ),
        (ITEM_SKATEBOARD, SKATEBOARD),
        (
            "/obj/item/wheelchair/gold",
            "/obj/vehicle/ridden/wheelchair/gold",
        ),
        (ITEM_WHEELCHAIR, WHEELCHAIR),
    ];
    PAIRS
        .iter()
        .find(|(item, _)| is_under(item_path, item))
        .map(|&(_, vehicle)| vehicle)
}

/// `/obj/item/wheelchair/attack_self` (`deploy_wheelchair`) and
/// `/obj/item/melee/skateboard/attack_self` (the board is put down and ridden). True for those
/// items.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return false;
    };
    // `/obj/item/ship_in_a_bottle/attack_self` -> `create_boat`.
    if is_under(&path, SHIP_IN_A_BOTTLE) {
        let Some(&pos) = world.get::<TilePos>(mob) else {
            return true;
        };
        tell(
            world,
            client,
            Span::Notice,
            "Вы не знаете, как корабли попадают в такие бутылки, но, кажется, знаете, как его \
             оттуда достать.",
        );
        playsound(world, PlaySound::new(pos, GLASS_BREAK, 100.0, true));
        world.despawn(item);
        crate::construction::spawn_obj(world, DRAGON_BOAT, pos);
        return true;
    }
    let Some(vehicle_path) = unfolded_type(&path) else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return true;
    };
    if crate::inventory::no_drop(world, item) {
        return true;
    }
    let Some(vehicle) = crate::construction::spawn_obj(world, vehicle_path, pos) else {
        return true;
    };
    // The observers' commands: the new vehicle has its `Vehicle`.
    world.flush();
    let dir = facing(world, mob);
    set_facing(world, vehicle, dir);
    if is_under(&path, ITEM_WHEELCHAIR) {
        world.despawn(item);
        crate::construction::tile_changed(world, pos);
        return true;
    }
    put_inside(world, item, vehicle);
    if let Some(mut v) = world.get_mut::<Vehicle>(vehicle) {
        v.board = Some(item);
    }
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    crate::medical::surgery::buckle(world, actor, mob, vehicle);
    true
}

// --- Examine -------------------------------------------------------------------------------

/// `generate_integrity_message`.
pub fn integrity_message(percent: f32) -> Option<(Span, &'static str)> {
    if (50.0..=99.0).contains(&percent) {
        Some((Span::Notice, "Имеет незначительные повреждения."))
    } else if (25.0..=50.0).contains(&percent) {
        Some((Span::Notice, "Имеет значительные повреждения."))
    } else if (0.0..=25.0).contains(&percent) {
        Some((Span::Warning, "Разваливается на части!"))
    } else {
        None
    }
}

/// `examine()` of ridden vehicles.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(v) = world.get::<Vehicle>(target) else {
        return;
    };
    let max = world.get::<DmObj>(target).map_or(0.0, |o| o.max_integrity);
    if let Some(integrity) = world.get::<crate::obj_damage::Integrity>(target)
        && max > 0.0
        && let Some((span, text)) = integrity_message(integrity.current / max * 100.0)
    {
        lines.push((span, text.to_owned()));
    }
    let name = name_of(world, target);
    if v.def.key.is_some() {
        let key = inside(world, v.key, target);
        let line = match (key, v.def.kind) {
            (None, Kind::Lavaboat) => {
                format!("Можно прикрепить к {name} весло, кликнув по ней веслом.")
            }
            (Some(key), Kind::Lavaboat) => {
                format!("Alt-клик по {name}, чтобы снять {}.", name_of(world, key))
            }
            (None, _) => format!("Вставьте в {name} ключ, кликнув по нему ключом."),
            (Some(key), _) => format!("Alt-клик по {name}, чтобы вынуть {}.", name_of(world, key)),
        };
        lines.push((Span::Notice, line));
    } else if is_under(&path_of(world, target), DRAGON_BOAT) {
        lines.push((
            Span::Notice,
            format!("Можно вернуть {name} в бутылку, потерев нос дракона (Alt-клик)."),
        ));
    }
    match v.def.kind {
        Kind::Skateboard => lines.push((
            Span::Notice,
            "Если ехать медленно, шагом, можно не врезаться во что-нибудь.".to_owned(),
        )),
        Kind::Motorized => match cell_charge(world, target).zip(inside(world, v.cell, target)) {
            Some((charge, cell)) => {
                let max = world.get::<ItemCell>(cell).map_or(0.0, |c| c.0.maxcharge);
                lines.push((
                    Span::Notice,
                    "На нём небольшой экран, он показывает:".to_owned(),
                ));
                lines.push((Span::Notice, format!("Скорость: {MOTOR_SPEED}")));
                lines.push((
                    Span::Notice,
                    format!("Энергоэффективность: {MOTOR_EFFICIENCY}"),
                ));
                lines.push((
                    Span::Notice,
                    format!(
                        "Заряд: {} из {}",
                        crate::modsuit::display_energy(charge),
                        crate::modsuit::display_energy(max)
                    ),
                ));
            }
            // A mapped chair gets its cell with the first ride.
            None if !v.cell_ready => lines.push((
                Span::Notice,
                "На нём небольшой экран, он показывает:".to_owned(),
            )),
            None => lines.push((
                Span::Notice,
                "На нём небольшой экран, но он тёмный.".to_owned(),
            )),
        },
        _ => {}
    }
}

// --- Actions -------------------------------------------------------------------------------

/// The driver's vehicle buttons after the others it has (tg `grant_controller_actions` /
/// `remove_controller_actions`): a skateboard's Ollie and Kickflip.
fn refresh_buttons(world: &mut World, mob: Entity) {
    if world.get::<PlayerMob>(mob).is_none() {
        return;
    }
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(b, _)| !b.id.starts_with(BUTTON))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let skating = world
        .get::<Rider>(mob)
        .and_then(|r| world.get::<Vehicle>(r.vehicle))
        .is_some_and(|v| v.def.kind == Kind::Skateboard);
    if skating {
        let icon = |dmi: &str, state: &str| {
            world
                .get_resource::<IconIndex>()
                .and_then(|index| index.get(dmi, state))
        };
        for (id, name) in [(OLLIE, "Олли"), (KICKFLIP, "Кикфлип")] {
            buttons.push((
                ActionButton {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    icon: icon(ACTIONS_DMI, "skateboard_ollie"),
                    background: icon(BACKGROUNDS_DMI, "bg_default"),
                    border: None,
                    available: true,
                    cooldown: 0.0,
                    active: false,
                    maptext: None,
                },
                0.0,
            ));
        }
    }
    if buttons.is_empty() && world.get::<ActionButtons>(mob).is_none() {
        return;
    }
    set_buttons(world, mob, buttons);
}

/// A click on a vehicle button, checked at the moment of effect: the client controls the mob,
/// it still rides a skateboard, conscious (`AB_CHECK_CONSCIOUS`).
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || client_of(world, actor) != Some(client) {
            continue;
        }
        if world.get::<Stat>(actor) != Some(&Stat::Conscious) {
            continue;
        }
        let Some(board) = world.get::<Rider>(actor).map(|r| r.vehicle) else {
            continue;
        };
        if world.get::<Vehicle>(board).map(|v| v.def.kind) != Some(Kind::Skateboard) {
            continue;
        }
        match id.as_str() {
            OLLIE => ollie(world, actor, board),
            KICKFLIP => kickflip(world, actor, board),
            _ => {}
        }
    }
}

/// `/datum/action/vehicle/ridden/scooter/skateboard/ollie/Trigger`: a hop a tile forward, over
/// tables (`PASSTABLE`).
fn ollie(world: &mut World, rider: Entity, board: Entity) {
    let Some(instability) = world.get::<Vehicle>(board).map(|v| v.def.instability) else {
        return;
    };
    let dir = facing(world, board);
    crate::status_effects::adjust_stamina(world, rider, instability * 0.75);
    if stamina(world, rider) >= 100.0 {
        sound(world, board, BANG, 20.0);
        dismount(world, rider);
        crate::martial::fling(world, rider, dir, 2, 2.0);
        crate::status_effects::apply(world, rider, Incap::Paralyze, 4.0);
        let who = name_of(world, rider);
        visible(
            world,
            rider,
            "Вы не удерживаете приземление и падаете лицом вниз!",
            &format!("{who} не удерживает приземление и падает лицом вниз!"),
            Span::Danger,
        );
        return;
    }
    sound(world, board, OLLIE_SOUND, 50.0);
    if crate::pull::step_passing(world, board, dir, crate::tables::is_table).is_ok() {
        carry_rider(world, board);
    }
}

/// `/datum/action/vehicle/ridden/scooter/skateboard/kickflip/Trigger`.
fn kickflip(world: &mut World, rider: Entity, board: Entity) {
    let Some(instability) = world.get::<Vehicle>(board).map(|v| v.def.instability) else {
        return;
    };
    crate::status_effects::adjust_stamina(world, rider, instability);
    let who = name_of(world, rider);
    if stamina(world, rider) >= 100.0 {
        sound(world, board, BANG, 20.0);
        dismount(world, rider);
        crate::status_effects::apply(world, rider, Incap::Paralyze, 5.0);
        let hard = world
            .resource_mut::<crate::combat::CombatRng>()
            .chance(15.0);
        let own = if hard {
            crate::mecha::core::hurt_mob(world, rider, 10.0, crate::health::DamageType::Brute);
            "Вы сильно ударяетесь о доску."
        } else {
            "Вы падаете плашмя на доску!"
        };
        visible(
            world,
            rider,
            own,
            &format!("{who} не удерживает приземление и падает лицом вниз!"),
            Span::Danger,
        );
        return;
    }
    visible(
        world,
        rider,
        "Вы делаете крутой кикфлип и ловите доску в воздухе! Стильно.",
        &format!("{who} делает крутой кикфлип и ловит доску в воздухе."),
        Span::Notice,
    );
    sound(world, board, OLLIE_SOUND, 50.0);
    dismount(world, rider);
    crate::status_effects::apply(world, rider, Incap::Immobilize, 0.5);
    // shortcut: picked up at once, tg waits the 0.5 s immobilization out.
    pick_up_board(world, rider, board, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riding_components_by_type() {
        let secway = riding_def("/obj/vehicle/ridden/secway").unwrap();
        assert_eq!(secway.key, Some("/obj/item/key/security"));
        assert_eq!(secway.move_delay, 1.75);
        assert_eq!(secway.flags, LEGS_ARMS_UNBUCKLE);
        let board = riding_def("/obj/vehicle/ridden/scooter/skateboard/pro").unwrap();
        assert_eq!(board.kind, Kind::Skateboard);
        assert_eq!((board.move_delay, board.instability), (1.0, 6.0));
        assert_eq!(board.flags, NEEDS_LEGS | UNBUCKLE_DISABLED);
        let scooter = riding_def("/obj/vehicle/ridden/scooter").unwrap();
        assert_eq!(scooter.kind, Kind::Plain);
        assert_eq!(
            riding_def("/obj/vehicle/ridden/wheelchair/motorized")
                .unwrap()
                .kind,
            Kind::Motorized
        );
        assert_eq!(
            riding_def("/obj/vehicle/ridden/wheelchair/gold")
                .unwrap()
                .item,
            Some("/obj/item/wheelchair/gold")
        );
        let cart = riding_def("/obj/vehicle/ridden/janicart/upgraded").unwrap();
        assert_eq!((cart.kind, cart.flags), (Kind::Janicart, 0));
        assert_eq!(cart.rider_up, [4, 7, 7, 7]);
        assert!(
            riding_def("/obj/vehicle/ridden/speedbike/red")
                .unwrap()
                .spacemove
        );
        assert!(
            riding_def("/obj/vehicle/ridden/scooter/skateboard/wheelys/rollerskates").is_none()
        );
        let boat = riding_def("/obj/vehicle/ridden/lavaboat/plasma").unwrap();
        assert_eq!(
            (boat.kind, boat.key, boat.move_delay),
            (Kind::Lavaboat, Some(OAR), 2.0)
        );
        let dragon = riding_def(DRAGON_BOAT).unwrap();
        assert_eq!((dragon.key, dragon.move_delay), (None, 1.0));
        assert!(riding_def("/obj/vehicle/sealed/mecha/ripley").is_none());
        assert!(riding_def("/obj/vehicle/ridden_x").is_none());
    }

    #[test]
    fn delays_follow_hands_speed_and_walk() {
        let hand = riding_def(WHEELCHAIR).unwrap();
        // `round(2.2 * 6.2) / hands` = 13 / 2.
        assert_eq!(base_delay(&hand, 2, false), 6.5);
        assert_eq!(base_delay(&hand, 1, false), 13.0);
        assert_eq!(base_delay(&hand, 0, false), 13.0);
        let motor = riding_def(MOTORIZED).unwrap();
        assert_eq!(base_delay(&motor, 0, false), 14.0 / 3.0);
        let board = riding_def(SKATEBOARD).unwrap();
        assert_eq!(base_delay(&board, 2, false), 1.5);
        assert!((base_delay(&board, 2, true) - 2.1).abs() < 1e-6);
        let cart = riding_def("/obj/vehicle/ridden/janicart").unwrap();
        assert_eq!(base_delay(&cart, 2, true), 2.0);
    }

    #[test]
    fn cooldowns_are_bandastation_scaled() {
        // `modified_move_delay(1.5)` = 2.2 / 1.5 * 1.5 = 2.2 ds.
        assert!((step_cooldown(1.5, false) - 0.22).abs() < 1e-6);
        // Diagonal: FLOOR(1.5 * 1.414, 0.5) = 2.0 -> 2.2 / 1.5 * 2 = 2.93 ds.
        assert!((step_cooldown(1.5, true) - 0.293).abs() < 1e-6);
        // Zero delay: `(RP_SPEED - TG_SPEED) * TG_SPEED` = 1.05 ds.
        assert!((step_cooldown(0.0, false) - 0.105).abs() < 1e-6);
    }

    #[test]
    fn items_unfold_into_their_vehicles() {
        assert_eq!(unfolded_type(ITEM_WHEELCHAIR), Some(WHEELCHAIR));
        assert_eq!(
            unfolded_type("/obj/item/wheelchair/gold"),
            Some("/obj/vehicle/ridden/wheelchair/gold")
        );
        assert_eq!(
            unfolded_type("/obj/item/melee/skateboard/holyboard"),
            Some("/obj/vehicle/ridden/scooter/skateboard/hoverboard/holyboarded")
        );
        assert_eq!(unfolded_type(ITEM_SKATEBOARD), Some(SKATEBOARD));
        assert_eq!(unfolded_type("/obj/item/melee/baton"), None);
        // Every board and folded chair a vehicle turns back into unfolds into it again.
        for path in [
            SKATEBOARD,
            "/obj/vehicle/ridden/scooter/skateboard/pro",
            "/obj/vehicle/ridden/scooter/skateboard/hoverboard",
            "/obj/vehicle/ridden/scooter/skateboard/hoverboard/admin",
            "/obj/vehicle/ridden/scooter/skateboard/hoverboard/holyboarded",
            "/obj/vehicle/ridden/scooter/skateboard/improvised",
            WHEELCHAIR,
            "/obj/vehicle/ridden/wheelchair/gold",
        ] {
            let item = riding_def(path).unwrap().item.unwrap();
            assert_eq!(unfolded_type(item), Some(path), "{path}");
        }
    }

    #[test]
    fn integrity_messages() {
        assert_eq!(integrity_message(100.0), None);
        assert_eq!(
            integrity_message(75.0).unwrap().1,
            "Имеет незначительные повреждения."
        );
        assert_eq!(
            integrity_message(50.0).unwrap().1,
            "Имеет незначительные повреждения."
        );
        assert_eq!(
            integrity_message(30.0).unwrap().1,
            "Имеет значительные повреждения."
        );
        assert_eq!(integrity_message(10.0).unwrap().0, Span::Warning);
    }
}
