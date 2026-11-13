//! Sealed cars (server), after tg `code/modules/vehicles/sealed.dm`, `cars/car.dm`,
//! `cars/clowncar.dm`, `cars/speedwagon.dm` and the sealed actions of `vehicle_actions.dm`.
//!
//! - Occupants sit inside the car ([`crate::closets::InCloset`] with the car as container:
//!   hidden, unclickable, on the car's tile) with tg's control flags ([`CarOccupant`]): the
//!   first suitable one drives (`auto_assign_occupant_flags`; the clown car takes only a
//!   human Clown as its driver, everyone else is kidnapped).
//! - Getting in: dragging yourself onto the car (`mouse_drop_receive` -> `mob_try_enter`,
//!   `enter_delay`); the clown car also takes another living mob dragged onto it after the
//!   same delay while it stands still (`mob_try_forced_enter`). Getting out: the "Climb Out"
//!   button; a kidnapped occupant pushes against the trunk for `escape_time` first. An empty
//!   hand on the clown car opens the trunk in 3 s and dumps the kidnapped.
//! - Driving: the driver's steps are the car's (`relaymove` -> `vehicle_move`) with the key
//!   inserted (the clown car's bike horn) and the cannon folded; the cooldown is BandaStation's
//!   `modified_move_delay(vehicle_move_delay)`; the engine revs every 2 s. Bumped doors open
//!   with the access providers' access (`crate::doors`).
//! - Clown car: walls crash it (everyone falls out), living mobs it rams are sucked in
//!   (paralyzed 4 s; a deer is killed and hurts the passengers instead), lying carbons it runs
//!   over are flattened (paralyzed 2 s). A banana repairs it by its potency, hits spew lube foam
//!   (33%), destruction farts and explodes. Buttons: horn, headlights, dump the kidnapped,
//!   the kidnapped's thanks; emagged, the colourful buttons (six random effects, 15 s) and the
//!   siege cannon that fires a random kidnapped occupant at the clicked spot. An Irish Car Bomb
//!   (30 u or more) in a mob forced in blows the car up 5 s later.
//! - Speedwagon: occupied, it rams every standing human it bumps or passes within 2 tiles
//!   (paralyzed 10 s, 30 stamina, 20-35 brute, thrown 4 tiles).
//!
//! Cost: one relay per driver step; one pass per tick over the cars for their timers.
// shortcut: no achievements, squish element, waddling, flick animations, supermatter dusting,
// cannon mouse pointer, headlight cone (the light shines all around), speedwagon cover overlay
// or `crash_all`, and the deer crash blurs eyes without the head wound; add with those systems.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::light::{DmLight, source};
use ss13_mapload::{DmObj, DmTurfBuild, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, LightSource, Optics, TilePos};
use ss13_tgui::server::{UiActed, UiSystems};
use ss13_world::{Bumped, Density, MoveRequest, Moved, Mover, SpatialIndex, WorldSystems};

use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::{ClickIntercept, InterceptedClick};
use crate::closets::InCloset;
use crate::combat::CombatRng;
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::mecha::core::{modified_move_delay, visible};
use crate::medical::name_of;
use crate::obj_damage::{Integrity, is_under};
use crate::sound::{PlaySound, playsound};
use crate::status_effects::Incap;
use crate::storage::StoredIn;
use crate::tools::{Actor, DoAfter, client_of, do_after, tell};

pub const CAR: &str = "/obj/vehicle/sealed/car";
pub const CLOWNCAR: &str = "/obj/vehicle/sealed/car/clowncar";
pub const SPEEDWAGON: &str = "/obj/vehicle/sealed/car/speedwagon";
const BIKEHORN: &str = "/obj/item/bikehorn";
const BANANA: &str = "/obj/item/food/grown/banana";
const SPECIAL_PEEL: &str = "/obj/item/grown/bananapeel/specialpeel";
const SLIPPERY_OIL: &str = "/obj/effect/decal/cleanable/blood/oil/slippery";
const DEER: &str = "/mob/living/basic/deer";
const MEGAFAUNA: &str = "/mob/living/simple_animal/hostile/megafauna";
const CLOSED_TURF: &str = "/turf/closed";
const LUBE: &str = "/datum/reagent/lube";
const SUPERLAUGHTER: &str = "/datum/reagent/consumable/superlaughter";
const IRISH_CAR_BOMB: &str = "/datum/reagent/consumable/ethanol/irishcarbomb";
const SINGULARITY_DMI: &str = "icons/obj/machines/engine/singularity.dmi";
const ACTIONS_DMI: &str = "icons/mob/actions/actions_vehicle.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";

const ENGINE_SOUND: &str = "sound/vehicles/carrev.ogg";
const CLOWN_DOOR_CLOSE: &str = "sound/vehicles/clown_car/door_close.ogg";
const CLOWN_DOOR_OPEN: &str = "sound/vehicles/clown_car/door_open.ogg";
const CLOWN_CAR_LOAD: &str = "clown_car_load";
const CRASHES: [&str; 2] = [
    "sound/vehicles/clown_car/clowncar_crash1.ogg",
    "sound/vehicles/clown_car/clowncar_crash2.ogg",
];
const CRASH_PINS: &str = "sound/vehicles/clown_car/clowncar_crashpins.ogg";
const RAMS: [&str; 3] = [
    "sound/vehicles/clown_car/clowncar_ram1.ogg",
    "sound/vehicles/clown_car/clowncar_ram2.ogg",
    "sound/vehicles/clown_car/clowncar_ram3.ogg",
];
const CAR_CRASH: &str = "sound/vehicles/car_crash.ogg";
const FLASH_RING: &str = "sound/items/weapons/flash_ring.ogg";
const SPLAT: &str = "sound/effects/cartoon_sfx/cartoon_splat.ogg";
const FART: &str = "sound/vehicles/clown_car/clowncar_fart.ogg";
const BUTTON_PRESS: &str = "sound/vehicles/clown_car/button_press.ogg";
const CANNON_OPEN: &str = "sound/vehicles/clown_car/clowncar_cannonmode1.ogg";
const CANNON_CLOSE: &str = "sound/vehicles/clown_car/clowncar_cannonmode2.ogg";
const CANNON_SHOTS: [&str; 3] = [
    "sound/vehicles/carcannon1.ogg",
    "sound/vehicles/carcannon2.ogg",
    "sound/vehicles/carcannon3.ogg",
];
const CAR_HORN: &str = "sound/items/carhorn.ogg";
const BIKEHORN_SOUND: &str = "sound/items/bikehorn.ogg";
const LIGHTS_ON: &str = "sound/items/weapons/magin.ogg";
const LIGHTS_OFF: &str = "sound/items/weapons/magout.ogg";
const BANG: &str = "sound/effects/bang.ogg";

/// `VEHICLE_CONTROL_DRIVE`, `VEHICLE_CONTROL_KIDNAPPED`.
pub const DRIVE: u8 = 1 << 0;
pub const KIDNAPPED: u8 = 1 << 1;
/// `MEDIUM_RANGE_SOUND_EXTRARANGE`.
const MEDIUM_RANGE: f32 = -3.0;
/// `engine_sound_length`, `COOLDOWN_CAR_HONK`, the thanks' cooldown, `dice_cooldown_time`.
const ENGINE_SOUND_SECS: f64 = 2.0;
const HONK_SECS: f64 = 2.0;
const THANK_SECS: f64 = 6.0;
const DICE_SECS: f64 = 15.0;
/// The cannon's unfolding and folding, the singularity disguise, the oil trail, the Irish Car
/// Bomb's fuse, the emagged headlights' colour change (`SSobj` wait).
const CANNON_SECS: f64 = 2.0;
const SINGULO_SECS: f64 = 10.0;
const OIL_SECS: f64 = 3.0;
const BOMB_SECS: f64 = 5.0;
const LIGHT_CYCLE_SECS: f64 = 2.0;
/// `do_after(user, 30)` of the trunk.
const TRUNK_DS: f32 = 30.0;
/// `headlight_colors`.
const HEADLIGHT_COLORS: [[u8; 3]; 7] = [
    [0xff, 0x00, 0x00],
    [0xff, 0x99, 0x00],
    [0xff, 0xff, 0x00],
    [0x32, 0xcd, 0x32],
    [0x2c, 0xb2, 0xe8],
    [0x00, 0xff, 0xff],
    [0x80, 0x00, 0x80],
];

const BUTTON: &str = "car:";
const CLIMB_OUT: &str = "car:climb_out";
const REMOVE_KEY: &str = "car:remove_key";
const DUMP: &str = "car:dump";
const HORN: &str = "car:horn";
const HEADLIGHTS: &str = "car:headlights";
const THANK: &str = "car:thank";
const DICE: &str = "car:dice";
const CANNON: &str = "car:cannon";

/// The ported cars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarKind {
    Clown,
    Speedwagon,
}

impl CarKind {
    pub fn of(path: &str) -> Option<Self> {
        if is_under(path, CLOWNCAR) {
            Some(Self::Clown)
        } else if is_under(path, SPEEDWAGON) {
            Some(Self::Speedwagon)
        } else {
            None
        }
    }

    /// `max_occupants`.
    pub fn max_occupants(self) -> usize {
        match self {
            Self::Clown => 50,
            Self::Speedwagon => 4,
        }
    }

    /// `enter_delay`, `escape_time`, deciseconds.
    fn enter_delay_ds(self) -> f32 {
        match self {
            Self::Clown => 20.0,
            Self::Speedwagon => 0.0,
        }
    }

    fn escape_ds(self) -> f32 {
        match self {
            Self::Clown => 60.0,
            Self::Speedwagon => 0.0,
        }
    }

    /// `vehicle_move_delay`, deciseconds.
    fn move_delay(self) -> f32 {
        match self {
            Self::Clown => 1.0,
            Self::Speedwagon => 0.0,
        }
    }

    /// `key_type`.
    fn key(self) -> Option<&'static str> {
        match self {
            Self::Clown => Some(BIKEHORN),
            Self::Speedwagon => None,
        }
    }

    /// `car_traits & CAN_KIDNAP`.
    fn can_kidnap(self) -> bool {
        self == Self::Clown
    }

    /// `access_provider_flags`.
    pub fn access_flags(self) -> u8 {
        match self {
            Self::Clown => DRIVE | KIDNAPPED,
            Self::Speedwagon => DRIVE,
        }
    }

    fn enter_sound(self) -> Option<&'static str> {
        (self == Self::Clown).then_some(CLOWN_DOOR_CLOSE)
    }

    fn exit_sound(self) -> Option<&'static str> {
        (self == Self::Clown).then_some(CLOWN_DOOR_OPEN)
    }
}

/// `cannonmode`: `CLOWN_CANNON_BUSY` split by direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cannon {
    #[default]
    Inactive,
    Opening,
    Ready,
    Closing,
}

/// The live car (tg `/obj/vehicle/sealed/car` vars).
#[derive(Component, Clone, Debug)]
pub struct Car {
    pub kind: CarKind,
    /// `occupants`, in the order they got in; their flags are on their [`CarOccupant`].
    pub occupants: Vec<Entity>,
    /// `inserted_key`, inside the car.
    pub key: Option<Entity>,
    pub emagged: bool,
    pub headlights: bool,
    pub cannon: Cannon,
    pub thanks: u32,
    /// `cooldown_vehicle_move`, `enginesound_cooldown`, `COOLDOWN_CLOWNCAR_RANDOMNESS`, game
    /// time each.
    next_move: f64,
    next_engine: f64,
    next_dice: f64,
    /// When the cannon finishes unfolding or folding.
    cannon_at: f64,
    singulo_until: Option<f64>,
    oil_until: f64,
    bomb_at: Option<f64>,
    next_light_color: f64,
}

impl Car {
    pub fn new(kind: CarKind) -> Self {
        Self {
            kind,
            occupants: Vec::new(),
            key: None,
            emagged: false,
            headlights: false,
            cannon: Cannon::Inactive,
            thanks: 0,
            next_move: 0.0,
            next_engine: 0.0,
            next_dice: 0.0,
            cannon_at: 0.0,
            singulo_until: None,
            oil_until: 0.0,
            bomb_at: None,
            next_light_color: 0.0,
        }
    }

    /// Game time the next step is allowed (`cooldown_vehicle_move`).
    pub fn next_move(&self) -> f64 {
        self.next_move
    }

    /// `canmove`: the cannon folded.
    pub fn can_move(&self) -> bool {
        self.cannon == Cannon::Inactive
    }
}

/// On a mob inside a [`Car`]: its `occupants` control flags.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CarOccupant {
    pub car: Entity,
    pub flags: u8,
    /// The car's step time in seconds (a driver's HUD `move_delay`, which the client predicts
    /// with).
    pub move_delay: f32,
    /// The horn's and the thanks' action cooldowns.
    next_honk: f64,
    next_thank: f64,
}

impl CarOccupant {
    pub fn driver(&self) -> bool {
        self.flags & DRIVE != 0
    }
}

/// On a clown car driver whose clicks fire the cannon (it got [`ClickIntercept`] from us).
#[derive(Component, Clone, Copy, Debug)]
struct CannonAim;

/// A step of a mob inside a car (written by `crate::server` instead of moving it): tg
/// `relaymove`.
#[derive(Message, Clone, Copy, Debug)]
pub struct CarRelayMove {
    pub mob: Entity,
    pub dir: Dir,
}

/// The timed steps around cars (`crate::construction::Job::Car`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarJob {
    /// The user climbs into the target car (`mob_try_enter`).
    Enter,
    /// The target mob is forced into `car`, which must still stand at `at`.
    Force { car: Entity, at: TilePos },
    /// A kidnapped occupant pushes out of the target car's trunk.
    Escape,
    /// The user opens the target car's trunk (`attack_hand`).
    OpenTrunk,
}

pub struct CarsPlugin;

impl Plugin for CarsPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // An emag on a clown car (before the emag's own families).
        app.add_click_handler(ClickPhase::HeldOnTarget, 299, |w, c| {
            c.held
                .is_some_and(|h| crate::cars::emag(w, c.client, c.mob, h, c.target))
        });
        // A key or a banana on a car.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2949, |w, c| {
            c.held
                .is_some_and(|h| crate::cars::item_interaction(w, c.client, c.mob, h, c.target))
        });
        // The clown car's trunk opened.
        app.add_click_handler(ClickPhase::EmptyHand, 3449, |w, c| {
            crate::cars::attack_hand(w, c.client, c.mob, c.target)
        });
        app.add_message::<CarRelayMove>()
            .add_observer(add_car)
            .add_observer(car_gone)
            .add_observer(occupant_gone)
            .add_observer(left_car)
            .add_systems(
                Update,
                (
                    relay_moves
                        .after(crate::server::forward_moves)
                        .before(WorldSystems::Movement),
                    (follow_cars, after_moves, car_bumps)
                        .chain()
                        .after(WorldSystems::Movement)
                        .before(crate::server::ack_moves),
                    timers,
                    cannon_clicks.after(crate::click::ResolveClicks),
                    trigger_buttons.in_set(UiSystems::Handle),
                ),
            );
    }
}

fn add_car(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Some((kind, max)) = objs
        .get(add.entity)
        .ok()
        .and_then(|o| Some((CarKind::of(&o.path)?, o.max_integrity)))
    else {
        return;
    };
    // The cooldown is the car's own; the mover only takes the step.
    commands
        .entity(add.entity)
        .insert((Car::new(kind), Mover::new(0.0)))
        .insert_if_new(Integrity {
            current: max,
            broken: false,
        });
}

/// A car despawned with mobs inside lets them out where it stood.
fn car_gone(remove: On<Remove<Car>>, cars: Query<&Car>, mut commands: Commands) {
    let Ok(car) = cars.get(remove.entity) else {
        return;
    };
    let mobs = car.occupants.clone();
    commands.queue(move |world: &mut World| {
        for mob in mobs {
            remove_occupant(world, mob);
        }
    });
}

/// An occupant lost its flags some other way (despawned): off the car's list.
fn occupant_gone(
    remove: On<Remove<CarOccupant>>,
    occs: Query<&CarOccupant>,
    mut commands: Commands,
) {
    let Ok(&CarOccupant { car, .. }) = occs.get(remove.entity) else {
        return;
    };
    let mob = remove.entity;
    commands.queue(move |world: &mut World| {
        if let Some(mut c) = world.get_mut::<Car>(car) {
            c.occupants.retain(|&m| m != mob);
        }
    });
}

/// An occupant taken out of the car by something else (`Exited` -> `remove_occupant`).
fn left_car(remove: On<Remove<InCloset>>, occs: Query<&CarOccupant>, mut commands: Commands) {
    if occs.get(remove.entity).is_err() {
        return;
    }
    let mob = remove.entity;
    commands.queue(move |world: &mut World| {
        if world.get::<InCloset>(mob).is_none() {
            remove_occupant(world, mob);
        }
    });
}

// --- Helpers ------------------------------------------------------------------------------

fn sound(world: &mut World, at: Entity, file: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        playsound(world, PlaySound::new(pos, file, vol, true));
    }
}

fn pick<'a>(world: &mut World, list: &[&'a str]) -> &'a str {
    let i = world
        .resource_mut::<CombatRng>()
        .range(0, list.len() as u32 - 1);
    list[i as usize]
}

fn path_of(world: &World, e: Entity) -> String {
    crate::construction::path_of(world, e).unwrap_or_default()
}

fn kind_of(world: &World, car: Entity) -> Option<CarKind> {
    world.get::<Car>(car).map(|c| c.kind)
}

fn flags_of(world: &World, mob: Entity, car: Entity) -> u8 {
    world
        .get::<CarOccupant>(mob)
        .filter(|o| o.car == car)
        .map_or(0, |o| o.flags)
}

/// `return_controllers_with_flag(flag)`.
pub fn controllers(world: &World, car: Entity, flag: u8) -> Vec<Entity> {
    world.get::<Car>(car).map_or_else(Vec::new, |c| {
        c.occupants
            .iter()
            .copied()
            .filter(|&m| flags_of(world, m, car) & flag != 0)
            .collect()
    })
}

fn occupant_count(world: &World, car: Entity) -> usize {
    world.get::<Car>(car).map_or(0, |c| c.occupants.len())
}

/// The key is still inside the car (tg `Exited` clears `inserted_key`).
fn inserted_key(world: &World, car: Entity) -> Option<Entity> {
    world
        .get::<Car>(car)?
        .key
        .filter(|&k| world.get::<StoredIn>(k).is_some_and(|s| s.0 == car))
}

fn key_fits(world: &World, car: Entity) -> bool {
    let Some(key_type) = kind_of(world, car).and_then(CarKind::key) else {
        return true;
    };
    inserted_key(world, car).is_some_and(|k| is_under(&path_of(world, k), key_type))
}

fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<crate::human::HumanLook>(mob).is_some()
}

/// The mob is inside something already (a closet, a mech, a car).
fn contained(world: &World, mob: Entity) -> bool {
    world.get::<InCloset>(mob).is_some() || world.get::<crate::mecha::Pilot>(mob).is_some()
}

fn active_item(world: &World, mob: Entity) -> Option<Entity> {
    world.get::<Inventory>(mob).and_then(Inventory::active_item)
}

/// The tile `range` steps from `pos` in `dir` (`get_edge_target_turf` for a throw of that range).
fn far(pos: TilePos, dir: Dir, range: u32) -> TilePos {
    (0..range).fold(pos, |p, _| p.step(dir))
}

// --- Occupants ------------------------------------------------------------------------------

/// tg `add_occupant` + `after_add_occupant`: the mob goes in with `flags`, then the car's own
/// assignment. False when it already sits in a car.
fn add_occupant(world: &mut World, mob: Entity, car: Entity, flags: u8, forced: bool) -> bool {
    if world.get::<CarOccupant>(mob).is_some() {
        return false;
    }
    let (Some(kind), Some(&pos)) = (kind_of(world, car), world.get::<TilePos>(car)) else {
        return false;
    };
    if !forced && let Some(s) = kind.enter_sound() {
        playsound(
            world,
            PlaySound::new(pos, s, 70.0, true).extrarange(MEDIUM_RANGE),
        );
    }
    // `forceMove(src)`: off a vehicle or furniture first.
    if world.get::<crate::vehicles::Rider>(mob).is_some() {
        crate::vehicles::dismount(world, mob);
    } else if world.get::<crate::medical::surgery::Buckled>(mob).is_some() {
        let actor = Actor {
            client: client_of(world, mob).unwrap_or(mob),
            mob,
            item: None,
        };
        crate::medical::surgery::unbuckle(world, actor, mob);
    }
    crate::closets::put_mob_inside(world, mob, car);
    let move_delay = modified_move_delay(kind.move_delay()) / 10.0;
    world.entity_mut(mob).remove::<Density>().insert((
        CarOccupant {
            car,
            flags,
            move_delay,
            next_honk: 0.0,
            next_thank: 0.0,
        },
        pos,
    ));
    if let Some(mut c) = world.get_mut::<Car>(car) {
        c.occupants.push(mob);
    }
    auto_assign(world, mob, car);
    refresh_buttons(world, mob);
    true
}

/// `auto_assign_occupant_flags`: the first to fit drives (`max_drivers = 1`); the clown car
/// only lets a human Clown drive and kidnaps everyone else.
fn auto_assign(world: &mut World, mob: Entity, car: Entity) {
    let Some(kind) = kind_of(world, car) else {
        return;
    };
    let no_driver = controllers(world, car, DRIVE).is_empty();
    let add = match kind {
        CarKind::Clown => {
            let clown = is_human(world, mob)
                && crate::service::server::job_title(world, mob).as_deref() == Some("Clown");
            if clown && no_driver {
                sound(world, car, CLOWN_DOOR_CLOSE, 70.0);
                DRIVE
            } else {
                KIDNAPPED
            }
        }
        CarKind::Speedwagon if no_driver => DRIVE,
        CarKind::Speedwagon => 0,
    };
    if let Some(mut o) = world.get_mut::<CarOccupant>(mob) {
        o.flags |= add;
    }
}

/// tg `remove_occupant` and `Exited`: the mob is out on the car's tile with its own look and
/// body state back. Returns the car.
fn remove_occupant(world: &mut World, mob: Entity) -> Option<Entity> {
    let occ = world.get_entity_mut(mob).ok()?.take::<CarOccupant>()?;
    let car = occ.car;
    if let Some(mut c) = world.get_mut::<Car>(car) {
        c.occupants.retain(|&m| m != mob);
    }
    if world.get::<InCloset>(mob).is_some_and(|c| c.closet == car) {
        crate::closets::let_out(world, mob);
    }
    // The density and the HUD move delay come back from the health.
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.set_changed();
    }
    refresh_buttons(world, mob);
    if let Some(s) = kind_of(world, car).and_then(CarKind::exit_sound)
        && let Some(&pos) = world.get::<TilePos>(car)
    {
        playsound(
            world,
            PlaySound::new(pos, s, 70.0, true).extrarange(MEDIUM_RANGE),
        );
    }
    Some(car)
}

/// tg `mob_exit`: out, maybe thrown a tile aside (`randomstep`), with the message.
pub fn mob_exit(world: &mut World, mob: Entity, silent: bool, randomstep: bool) -> bool {
    let Some(car) = remove_occupant(world, mob) else {
        return false;
    };
    let pos = world
        .get::<TilePos>(car)
        .or(world.get::<TilePos>(mob))
        .copied();
    if randomstep && let Some(pos) = pos {
        let i = world.resource_mut::<CombatRng>().range(0, 3);
        let target = pos.step(Dir::CARDINALS[i as usize]);
        crate::throw::throw_at(world, mob, target, 5, 10.0);
    }
    if !silent && let Some(pos) = pos {
        let (who, what) = (name_of(world, mob), name_of(world, car));
        visible(
            world,
            pos,
            Span::Notice,
            format!("{who} вываливается из {what}!"),
            &[],
        );
    }
    true
}

/// tg `dump_specific_mobs(flag)` (`dump_mobs` with every flag): the occupants fall out and
/// carbons are paralyzed 4 s.
pub fn dump_mobs(world: &mut World, car: Entity, flag: Option<u8>, randomstep: bool) {
    let mobs: Vec<Entity> = world
        .get::<Car>(car)
        .map_or_else(Vec::new, |c| c.occupants.clone());
    for mob in mobs {
        if flag.is_some_and(|f| flags_of(world, mob, car) & f == 0) {
            continue;
        }
        mob_exit(world, mob, false, randomstep);
        if crate::throw::is_carbon(world, mob) {
            crate::status_effects::apply(world, mob, Incap::Paralyze, 4.0);
        }
    }
}

// --- Entering and leaving -----------------------------------------------------------------

/// `/obj/vehicle/sealed/car/mouse_drop_receive` (from `crate::drag`): `dragged` dropped onto a
/// car by `actor`. True when the drop was the car's.
pub fn mouse_drop_receive(world: &mut World, actor: Actor, over: Entity, dragged: Entity) -> bool {
    let Some(kind) = kind_of(world, over) else {
        return false;
    };
    if world.get::<Health>(actor.mob).is_none() {
        return true;
    }
    if kind.can_kidnap() && dragged != actor.mob && world.get::<Health>(dragged).is_some() {
        if let Some(&pos) = world.get::<TilePos>(dragged) {
            let (user, victim, what) = (
                name_of(world, actor.mob),
                name_of(world, dragged),
                name_of(world, over),
            );
            visible(
                world,
                pos,
                Span::Warning,
                format!("{user} начинает заталкивать {victim} в {what}!"),
                &[],
            );
        }
        mob_try_forced_enter(world, actor, over, dragged);
    }
    if dragged == actor.mob {
        mob_try_enter(world, actor, over);
    }
    true
}

/// tg `mob_try_enter`: `enter_delay`, then `enter_checks` again.
fn mob_try_enter(world: &mut World, actor: Actor, car: Entity) {
    let Some(kind) = kind_of(world, car) else {
        return;
    };
    if !enter_checks(world, car, actor.mob) {
        return;
    }
    let actor = Actor {
        item: active_item(world, actor.mob),
        ..actor
    };
    do_after(
        world,
        actor,
        car,
        kind.enter_delay_ds(),
        0,
        Job::Car(CarJob::Enter),
    );
}

/// tg `enter_checks`: room inside (and not in something already).
fn enter_checks(world: &World, car: Entity, mob: Entity) -> bool {
    let Some(c) = world.get::<Car>(car) else {
        return false;
    };
    c.occupants.len() < c.kind.max_occupants() && !contained(world, mob)
}

/// tg `mob_enter`.
fn mob_enter(world: &mut World, mob: Entity, car: Entity) {
    let (who, what) = (name_of(world, mob), name_of(world, car));
    if let Some(&pos) = world.get::<TilePos>(mob) {
        visible(
            world,
            pos,
            Span::Notice,
            format!("{who} забирается в {what}!"),
            &[],
        );
    }
    add_occupant(world, mob, car, 0, false);
}

/// tg `mob_try_forced_enter`: the forcer holds the victim for `enter_delay` while the car
/// stands still.
fn mob_try_forced_enter(world: &mut World, actor: Actor, car: Entity, victim: Entity) {
    let (Some(kind), Some(&at)) = (kind_of(world, car), world.get::<TilePos>(car)) else {
        return;
    };
    if occupant_count(world, car) >= kind.max_occupants() {
        return;
    }
    let actor = Actor {
        item: active_item(world, actor.mob),
        ..actor
    };
    do_after(
        world,
        actor,
        victim,
        kind.enter_delay_ds(),
        0,
        Job::Car(CarJob::Force { car, at }),
    );
}

/// tg `mob_forced_enter` (the clown car's adds the Irish Car Bomb).
fn mob_forced_enter(world: &mut World, car: Entity, victim: Entity, silent: bool) {
    let Some(kind) = kind_of(world, car) else {
        return;
    };
    if !silent {
        let (who, what) = (name_of(world, victim), name_of(world, car));
        if let Some(&pos) = world.get::<TilePos>(victim) {
            visible(
                world,
                pos,
                Span::Warning,
                format!("{who} заталкивают в {what}!"),
                &[],
            );
        }
        if kind == CarKind::Clown
            && let Some(&pos) = world.get::<TilePos>(car)
        {
            playsound(
                world,
                PlaySound::new(pos, CLOWN_CAR_LOAD, 70.0, true).extrarange(MEDIUM_RANGE),
            );
        }
    }
    if !add_occupant(world, victim, car, KIDNAPPED, true) {
        return;
    }
    if kind == CarKind::Clown {
        irish_car_bomb_check(world, car, victim);
    }
}

/// `/obj/vehicle/sealed/car/clowncar/mob_forced_enter`: the Irish Car Bomb in the victim is
/// purged; 30 u or more set the engine rattling, and the car blows up 5 s later.
fn irish_car_bomb_check(world: &mut World, car: Entity, victim: Entity) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(id) = chem.id(IRISH_CAR_BOMB) else {
        return;
    };
    let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(victim) else {
        return;
    };
    let amount = reagents.blood.amount(id);
    if amount <= 0.0 {
        return;
    }
    reagents.blood.remove_reagent(id, amount);
    if amount < 30.0 {
        return;
    }
    info!("a mob was forced into a clown car with {amount} u of Irish Car Bomb: it will explode");
    if let Some(&pos) = world.get::<TilePos>(car) {
        visible(
            world,
            pos,
            Span::BoldWarning,
            "Вы слышите дребезжание из двигателя. Это не к добру...".to_owned(),
            &[],
        );
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut c) = world.get_mut::<Car>(car) {
        c.bomb_at = Some(now + BOMB_SECS);
    }
}

/// tg `/obj/vehicle/sealed/car/mob_try_exit` of `mob` itself: a kidnapped occupant pushes
/// against the trunk for `escape_time` first.
fn mob_try_exit(world: &mut World, client: Entity, mob: Entity) {
    let Some(occ) = world.get::<CarOccupant>(mob).copied() else {
        return;
    };
    let escape = kind_of(world, occ.car).map_or(0.0, CarKind::escape_ds);
    if occ.flags & KIDNAPPED == 0 || escape <= 0.0 {
        if occ.flags & KIDNAPPED != 0 {
            let (who, what) = (name_of(world, mob), name_of(world, occ.car));
            tell(
                world,
                client,
                Span::Danger,
                format!("{who} выбирается из {what}."),
            );
        }
        mob_exit(world, mob, false, false);
        return;
    }
    let what = name_of(world, occ.car);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы упираетесь в заднюю стенку багажника {what}, пытаясь выбраться."),
    );
    let actor = Actor {
        client,
        mob,
        item: active_item(world, mob),
    };
    do_after(world, actor, occ.car, escape, 0, Job::Car(CarJob::Escape));
}

/// End of a [`CarJob`] `do_after` (`crate::tools` checked that neither side moved).
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: CarJob) {
    let target = action.target;
    match job {
        CarJob::Enter => {
            if enter_checks(world, target, mob) {
                mob_enter(world, mob, target);
            }
        }
        CarJob::Force { car, at } => {
            // `is_car_stationary`; the room and the victim are checked again.
            let still = world.get::<TilePos>(car) == Some(&at);
            if still && world.get::<Health>(target).is_some() && enter_checks(world, car, target) {
                mob_forced_enter(world, car, target, false);
            }
        }
        CarJob::Escape => {
            if world
                .get::<CarOccupant>(mob)
                .is_some_and(|o| o.car == target)
            {
                let (who, what) = (name_of(world, mob), name_of(world, target));
                tell(
                    world,
                    action.client,
                    Span::Danger,
                    format!("{who} выбирается из {what}."),
                );
                mob_exit(world, mob, false, false);
            }
        }
        CarJob::OpenTrunk => {
            let what = name_of(world, target);
            if controllers(world, target, KIDNAPPED).is_empty() {
                tell(
                    world,
                    action.client,
                    Span::Notice,
                    format!("Похоже, багажник {what} пуст."),
                );
                return;
            }
            tell(
                world,
                action.client,
                Span::Notice,
                format!("Все, кто застрял в багажнике {what}, вываливаются наружу."),
            );
            dump_mobs(world, target, Some(KIDNAPPED), true);
        }
    }
}

// --- Clicks on the car ----------------------------------------------------------------------

/// `/obj/vehicle/sealed/car/attack_hand` of a car that kidnaps: the trunk opens in 3 s. True
/// when handled.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !kind_of(world, target).is_some_and(CarKind::can_kidnap) {
        return false;
    }
    let what = name_of(world, target);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы начинаете открывать багажник {what}."),
    );
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    do_after(
        world,
        actor,
        target,
        TRUNK_DS,
        0,
        Job::Car(CarJob::OpenTrunk),
    );
    true
}

/// `/obj/vehicle/sealed/item_interaction` (the key) and the clown car's banana repair. True
/// when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    _mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(kind) = kind_of(world, target) else {
        return false;
    };
    let held_path = path_of(world, held);
    let (what, name) = (name_of(world, held), name_of(world, target));
    if kind == CarKind::Clown && is_under(&held_path, BANANA) {
        let potency = world
            .get::<crate::botany::Seed>(held)
            .map_or(0.0, |s| s.potency);
        let max = world.get::<DmObj>(target).map_or(0.0, |o| o.max_integrity);
        if let Some(mut integrity) = world.get_mut::<Integrity>(target) {
            integrity.current = (integrity.current + potency).min(max);
        }
        tell(
            world,
            client,
            Span::Danger,
            format!("Вы чините {name} с помощью {what}!"),
        );
        world.despawn(held);
        return true;
    }
    let Some(key_type) = kind.key() else {
        return false;
    };
    if key_fits(world, target) || !is_under(&held_path, key_type) {
        return false;
    }
    if crate::inventory::no_drop(world, held) {
        tell(
            world,
            client,
            Span::Warning,
            format!("{what} прилип к вашей руке!"),
        );
        return true;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы вставляете {what} в {name}."),
    );
    // Just in case there's an invalid key.
    if let Some(old) = inserted_key(world, target)
        && let Some(&pos) = world.get::<TilePos>(target)
    {
        crate::storage::take_out(world, old, pos);
    }
    crate::storage::put_inside(world, held, target);
    if let Some(mut c) = world.get_mut::<Car>(target) {
        c.key = Some(held);
    }
    true
}

/// `/obj/vehicle/sealed/car/clowncar/emag_act`: "fun mode", the colourful buttons and the
/// cannon for the drivers. True when the click was the emag's.
pub fn emag(world: &mut World, client: Entity, mob: Entity, card: Entity, target: Entity) -> bool {
    if kind_of(world, target) != Some(CarKind::Clown) || !crate::emag::is_emag(world, card) {
        return false;
    }
    if !crate::emag::can_emag(world, mob, card, target)
        || world.get::<Car>(target).is_some_and(|c| c.emagged)
    {
        return true;
    }
    world.get_mut::<Car>(target).expect("checked").emagged = true;
    crate::balloon::balloon_alert(world, mob, target, "режим веселья включён");
    let what = name_of(world, target);
    tell(
        world,
        client,
        Span::Danger,
        format!(
            "Вы взламываете детский замок {what}, и появляется панель с шестью разноцветными \
             кнопками!"
        ),
    );
    for driver in controllers(world, target, DRIVE) {
        refresh_buttons(world, driver);
    }
    true
}

/// `/obj/vehicle/examine`: the integrity.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if world.get::<Car>(target).is_none() {
        return;
    }
    let max = world.get::<DmObj>(target).map_or(0.0, |o| o.max_integrity);
    if let Some(integrity) = world.get::<Integrity>(target)
        && max > 0.0
        && let Some((span, text)) =
            crate::vehicles::integrity_message(integrity.current / max * 100.0)
    {
        lines.push((span, text.to_owned()));
    }
}

/// `/obj/vehicle/sealed/car/clowncar/take_damage`: a hit spews lube foam a third of the time
/// (from `crate::obj_damage`).
pub fn damaged(world: &mut World, car: Entity) {
    if kind_of(world, car) != Some(CarKind::Clown)
        || !world.resource_mut::<CombatRng>().chance(33.0)
    {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(car) else {
        return;
    };
    let what = name_of(world, car);
    visible(
        world,
        pos,
        Span::Danger,
        format!("{what} извергает кучу космической смазки!"),
        &[],
    );
    reagent_foam(world, pos, 4, LUBE, 25.0);
}

/// `atom_destruction` of a car (from `crate::obj_damage::deconstruct`): the clown car farts,
/// everyone falls out (`Destroy` -> `dump_mobs`) and it explodes. True when it was a car.
pub fn destroyed(world: &mut World, car: Entity, pos: TilePos) -> bool {
    let Some(kind) = kind_of(world, car) else {
        return false;
    };
    if kind == CarKind::Clown {
        playsound(world, PlaySound::new(pos, FART, 100.0, true));
    }
    dump_mobs(world, car, None, true);
    crate::explosions::explosion(
        world,
        pos,
        crate::explosions::Explosion::new(0, 1, 2).flash(3),
    );
    true
}

// --- Driving --------------------------------------------------------------------------------

/// tg `/obj/vehicle/sealed/car/relaymove` for each occupant step: only a conscious driver
/// moves the car.
fn relay_moves(world: &mut World, mut relays: Local<MessageCursor<CarRelayMove>>) {
    let steps: Vec<CarRelayMove> = relays
        .read(world.resource::<Messages<CarRelayMove>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for step in steps {
        let Some(occ) = world.get::<CarOccupant>(step.mob).copied() else {
            continue;
        };
        if !occ.driver()
            || world.get::<Stat>(step.mob) != Some(&Stat::Conscious)
            || crate::status_effects::incapacitation(world, step.mob).immobilized
        {
            continue;
        }
        vehicle_move(world, occ.car, step.dir, now);
    }
}

/// tg `/obj/vehicle/sealed/car/vehicle_move` behind `relaymove`'s `canmove` and key checks.
pub fn vehicle_move(world: &mut World, car: Entity, dir: Dir, now: f64) -> bool {
    let Some(c) = world.get::<Car>(car) else {
        return false;
    };
    if !c.can_move() || now < c.next_move || !key_fits(world, car) {
        return false;
    }
    let delay = f64::from(modified_move_delay(c.kind.move_delay())) / 10.0;
    let rev = now >= c.next_engine;
    {
        let mut c = world.get_mut::<Car>(car).expect("checked");
        c.next_move = now + delay;
        if rev {
            c.next_engine = now + ENGINE_SOUND_SECS;
        }
    }
    if rev {
        sound(world, car, ENGINE_SOUND, 100.0);
    }
    world.write_message(MoveRequest { entity: car, dir });
    true
}

/// The occupants stay on the car's tile however it moved.
fn follow_cars(
    cars: Query<(&Car, &TilePos), Changed<TilePos>>,
    mobs: Query<&TilePos, Without<Car>>,
    mut commands: Commands,
) {
    for (car, &pos) in &cars {
        for &mob in &car.occupants {
            if mobs.get(mob).is_ok_and(|&p| p != pos) {
                commands.entity(mob).insert(pos);
            }
        }
    }
}

/// After a car stepped: the clown car flattens lying carbons and drops its oil trail, an
/// occupied speedwagon rams the humans around it (`Moved` -> `Bump` in range 2).
fn after_moves(world: &mut World, mut moves: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = moves
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .filter(|m| world.get::<Car>(m.entity).is_some())
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for step in moved {
        let car = step.entity;
        let Some(c) = world.get::<Car>(car) else {
            continue;
        };
        let (kind, oil, occupied) = (c.kind, now < c.oil_until, !c.occupants.is_empty());
        match kind {
            CarKind::Clown => {
                run_over(world, car, step.to);
                if oil {
                    crate::construction::spawn_obj(world, SLIPPERY_OIL, step.to);
                }
            }
            CarKind::Speedwagon if occupied => {
                let around: Vec<Entity> = world
                    .resource::<SpatialIndex>()
                    .in_radius(step.to, 2)
                    .filter(|&e| e != car)
                    .collect();
                for victim in around {
                    if world.get::<CarOccupant>(victim).is_none() {
                        speedwagon_ram(world, car, victim);
                    }
                }
            }
            CarKind::Speedwagon => {}
        }
    }
}

/// `/obj/vehicle/sealed/car/clowncar/check_crossed`: lying, able carbons on the new tile.
fn run_over(world: &mut World, car: Entity, pos: TilePos) {
    if !crate::multiz::has_gravity(world, pos) {
        return;
    }
    let victims: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            crate::throw::is_carbon(world, e)
                && !contained(world, e)
                && crate::martial::lying(world, e)
                && !crate::status_effects::incapacitation(world, e).incapacitated
        })
        .collect();
    let what = name_of(world, car);
    for victim in victims {
        let who = name_of(world, victim);
        visible(
            world,
            pos,
            Span::Warning,
            format!("{what} переезжает {who}, расплющивая как блин!"),
            &[],
        );
        crate::status_effects::apply(world, victim, Incap::Paralyze, 2.0);
        playsound(world, PlaySound::new(pos, SPLAT, 75.0, true));
    }
}

/// tg `Bump` of a car: the clown car's crashes and kidnapping, the speedwagon's ramming. The
/// doors are `crate::doors`'.
fn car_bumps(world: &mut World, mut bumps: Local<MessageCursor<Bumped>>) {
    let bumped: Vec<Bumped> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .filter(|b| world.get::<Car>(b.entity).is_some())
        .collect();
    for bump in bumped {
        match kind_of(world, bump.entity) {
            Some(CarKind::Clown) => clown_bump(world, bump.entity, bump.obstacle),
            Some(CarKind::Speedwagon) => speedwagon_ram(world, bump.entity, bump.obstacle),
            None => {}
        }
    }
}

/// `/obj/vehicle/sealed/car/clowncar/Bump`.
fn clown_bump(world: &mut World, car: Entity, obstacle: Entity) {
    let Some(&pos) = world.get::<TilePos>(car) else {
        return;
    };
    let what = name_of(world, car);
    if world
        .get::<DmTurfBuild>(obstacle)
        .is_some_and(|t| is_under(&t.path, CLOSED_TURF))
    {
        let wall = world
            .get::<ss13_mapload::DmName>(obstacle)
            .map_or_else(|| "стену".to_owned(), |n| n.name.clone());
        visible(
            world,
            pos,
            Span::Warning,
            format!("{what} врезается в {wall} и разбивается!"),
            &[],
        );
        let crash = pick(world, &CRASHES);
        playsound(world, PlaySound::new(pos, crash, 75.0, true));
        playsound(world, PlaySound::new(pos, CRASH_PINS, 75.0, true));
        dump_mobs(world, car, None, true);
        return;
    }
    if world.get::<Health>(obstacle).is_none() || contained(world, obstacle) {
        return;
    }
    let mob_path = crate::bots::mob_path(world, obstacle);
    if is_under(&mob_path, MEGAFAUNA) {
        return;
    }
    let who = name_of(world, obstacle);
    if !is_under(&mob_path, DEER) {
        if crate::throw::is_carbon(world, obstacle) {
            crate::status_effects::apply(world, obstacle, Incap::Paralyze, 4.0);
        }
        visible(
            world,
            pos,
            Span::Warning,
            format!("{what} врезается в {who} и засасывает внутрь!"),
            &[],
        );
        mob_forced_enter(world, car, obstacle, false);
        let ram = pick(world, &RAMS);
        playsound(world, PlaySound::new(pos, ram, 75.0, true));
        return;
    }
    visible(
        world,
        pos,
        Span::Warning,
        format!("{what} влетает в {who}! О ужас!"),
        &[],
    );
    let occupants = world
        .get::<Car>(car)
        .map_or_else(Vec::new, |c| c.occupants.clone());
    for occupant in occupants {
        if !crate::throw::is_carbon(world, occupant)
            || world.resource_mut::<CombatRng>().chance(35.0)
        {
            continue;
        }
        let blur = 10 + world.resource_mut::<CombatRng>().range(0, 10);
        crate::vision::set_eye_blur_if_lower(world, occupant, blur as f32);
        playsound(world, PlaySound::new(pos, FLASH_RING, 50.0, true));
    }
    crate::medical::diseases::effects::damage(world, obstacle, DamageType::Brute, 200.0);
    dump_mobs(world, car, None, true);
    playsound(world, PlaySound::new(pos, CAR_CRASH, 100.0, true));
}

/// `/obj/vehicle/sealed/car/speedwagon/Bump` of a dense human while occupied.
fn speedwagon_ram(world: &mut World, car: Entity, victim: Entity) {
    if occupant_count(world, car) == 0
        || world.get::<Density>(victim).is_none()
        || !is_human(world, victim)
        || contained(world, victim)
    {
        return;
    }
    let (Some(&pos), Some(&at)) = (world.get::<TilePos>(car), world.get::<TilePos>(victim)) else {
        return;
    };
    let dir = world.get::<Facing>(car).map_or(Dir::South, |f| f.0);
    crate::status_effects::apply(world, victim, Incap::Paralyze, 10.0);
    crate::status_effects::adjust_stamina(world, victim, 30.0);
    let brute = 20 + world.resource_mut::<CombatRng>().range(0, 15);
    crate::medical::diseases::effects::damage(world, victim, DamageType::Brute, brute as f32);
    crate::throw::throw_at(world, victim, far(at, dir, 4), 4, 3.0);
    let (what, who) = (name_of(world, car), name_of(world, victim));
    visible(
        world,
        pos,
        Span::Danger,
        format!("{} врезается в {who}!", ss13_shared::capitalize(&what)),
        &[],
    );
    playsound(world, PlaySound::new(pos, BANG, 50.0, true));
}

// --- Timers ---------------------------------------------------------------------------------

/// The car's timers: the cannon (un)folding, the singularity disguise, the Irish Car Bomb and
/// the emagged headlights' colours (`process`).
fn timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &Car)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| {
            (matches!(c.cannon, Cannon::Opening | Cannon::Closing) && now >= c.cannon_at)
                || c.singulo_until.is_some_and(|t| now >= t)
                || c.bomb_at.is_some_and(|t| now >= t)
                || (c.emagged && c.headlights && now >= c.next_light_color)
        })
        .map(|(e, _)| e)
        .collect();
    for car in due {
        let Some(c) = world.get::<Car>(car).cloned() else {
            continue;
        };
        if matches!(c.cannon, Cannon::Opening | Cannon::Closing) && now >= c.cannon_at {
            world.get_mut::<Car>(car).expect("checked").cannon = if c.cannon == Cannon::Opening {
                Cannon::Ready
            } else {
                Cannon::Inactive
            };
            for driver in controllers(world, car, DRIVE) {
                refresh_buttons(world, driver);
            }
        }
        if c.singulo_until.is_some_and(|t| now >= t) {
            world.get_mut::<Car>(car).expect("checked").singulo_until = None;
            reset_icon(world, car);
        }
        if c.emagged && c.headlights && now >= c.next_light_color {
            world.get_mut::<Car>(car).expect("checked").next_light_color = now + LIGHT_CYCLE_SECS;
            let i = world.resource_mut::<CombatRng>().range(0, 6);
            set_light(world, car, true, Some(HEADLIGHT_COLORS[i as usize]));
        }
        if c.bomb_at.is_some_and(|t| now >= t) {
            world.get_mut::<Car>(car).expect("checked").bomb_at = None;
            // `irish_car_bomb`.
            dump_mobs(world, car, None, true);
            if let Some(&pos) = world.get::<TilePos>(car) {
                crate::explosions::explosion(
                    world,
                    pos,
                    crate::explosions::Explosion::new(0, 0, 1),
                );
            }
        }
    }
}

/// `reset_icon`: back to the car's own look (with the cannon's, unless folded).
fn reset_icon(world: &mut World, car: Entity) {
    let state = match world.get::<Car>(car).map(|c| c.cannon) {
        Some(Cannon::Opening | Cannon::Ready) => "clowncar_fire",
        _ => "clowncar",
    };
    crate::obj_damage::set_icon_state(world, car, state);
}

/// `set_light_on` of the headlights, in `color` (`None`: the car's own).
fn set_light(world: &mut World, car: Entity, on: bool, color: Option<[u8; 3]>) {
    let light: Option<LightSource> = on
        .then(|| {
            let path = path_of(world, car);
            let mut dm = world.get::<DmLight>(car)?.clone();
            dm.light_on = true;
            let dir = world.get::<Facing>(car).map_or(2, |f| f.0 as u8);
            let mut light = source(&path, &dm, dir, [0, 0])?;
            if let Some(color) = color {
                light.color = color;
            }
            Some(light)
        })
        .flatten();
    let mut optics = world.get::<Optics>(car).cloned().unwrap_or_default();
    if optics.light != light {
        optics.light = light;
        world.entity_mut(car).insert(optics);
    }
}

// --- Actions --------------------------------------------------------------------------------

/// The occupant's car buttons after the others it has (tg `grant_passenger_actions` and
/// `grant_controller_actions_by_flag`), and the cannon's aim for drivers of a ready cannon.
fn refresh_buttons(world: &mut World, mob: Entity) {
    let occ = world.get::<CarOccupant>(mob).copied();
    let state = occ.and_then(|o| world.get::<Car>(o.car).cloned().map(|c| (o, c)));
    // The cannon takes the driver's clicks while it is ready (`COMSIG_MOB_CLICKON`).
    let aim = state
        .as_ref()
        .is_some_and(|(o, c)| o.driver() && c.cannon == Cannon::Ready);
    if aim && world.get::<CannonAim>(mob).is_none() {
        world.entity_mut(mob).insert((CannonAim, ClickIntercept));
    } else if !aim && world.get::<CannonAim>(mob).is_some() {
        world
            .entity_mut(mob)
            .remove::<(CannonAim, ClickIntercept)>();
    }
    if world.get::<crate::server::PlayerMob>(mob).is_none() {
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
    if let Some((o, c)) = state {
        let driver = o.driver();
        let clown = c.kind == CarKind::Clown;
        let list: [(bool, &str, &str, &str, f64); 8] = [
            (true, CLIMB_OUT, "Выбраться", "car_eject", 0.0),
            (
                driver && c.kind.key().is_some(),
                REMOVE_KEY,
                "Вынуть ключ",
                "car_removekey",
                0.0,
            ),
            (
                driver && c.kind.can_kidnap(),
                DUMP,
                "Выгрузить похищенных",
                "car_dump",
                0.0,
            ),
            (
                driver && clown,
                HORN,
                "Посигналить",
                "car_horn",
                o.next_honk,
            ),
            (driver && clown, HEADLIGHTS, "Фары", "car_headlights", 0.0),
            (
                o.flags & KIDNAPPED != 0 && clown,
                THANK,
                "Поблагодарить водителя клоунмобиля",
                "car_thanktheclown",
                o.next_thank,
            ),
            (
                driver && clown && c.emagged,
                DICE,
                "Нажать разноцветную кнопку",
                "car_rtd",
                c.next_dice,
            ),
            (
                driver && clown && c.emagged,
                CANNON,
                "Осадный режим",
                "car_cannon",
                0.0,
            ),
        ];
        let icon = |dmi: &str, state: &str| {
            world
                .get_resource::<IconIndex>()
                .and_then(|index| index.get(dmi, state))
        };
        for (granted, id, name, state, ready_at) in list {
            if !granted {
                continue;
            }
            buttons.push((
                ActionButton {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    icon: icon(ACTIONS_DMI, state),
                    background: icon(BACKGROUNDS_DMI, "bg_default"),
                    border: None,
                    available: true,
                    cooldown: 0.0,
                    active: false,
                    maptext: None,
                },
                ready_at,
            ));
        }
    }
    if buttons.is_empty() && world.get::<ActionButtons>(mob).is_none() {
        return;
    }
    set_buttons(world, mob, buttons);
}

/// A click on a car button, checked at the moment of effect: the client controls the mob, it
/// still sits in the car with the flags of the button, conscious and able to move
/// (`AB_CHECK_IMMOBILE | AB_CHECK_CONSCIOUS`).
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for (client, mob, source, id) in pending {
        if mob != source || client_of(world, mob) != Some(client) {
            continue;
        }
        if world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || crate::status_effects::incapacitation(world, mob).immobilized
        {
            continue;
        }
        let Some(occ) = world.get::<CarOccupant>(mob).copied() else {
            continue;
        };
        let car = occ.car;
        let Some(c) = world.get::<Car>(car).cloned() else {
            continue;
        };
        let (driver, clown) = (occ.driver(), c.kind == CarKind::Clown);
        match id.as_str() {
            CLIMB_OUT => mob_try_exit(world, client, mob),
            REMOVE_KEY if driver && c.kind.key().is_some() => remove_key(world, client, mob, car),
            DUMP if driver && c.kind.can_kidnap() => {
                if let Some(&pos) = world.get::<TilePos>(car) {
                    let what = name_of(world, car);
                    visible(
                        world,
                        pos,
                        Span::Danger,
                        format!("{what} начинает выгружать людей."),
                        &[],
                    );
                }
                dump_mobs(world, car, Some(KIDNAPPED), true);
            }
            HORN if driver && clown && now >= occ.next_honk => honk(world, client, mob, car, now),
            HEADLIGHTS if driver && clown => toggle_headlights(world, client, mob, car),
            THANK if occ.flags & KIDNAPPED != 0 && clown && now >= occ.next_thank => {
                thank(world, client, mob, car, now);
            }
            DICE if driver && clown && c.emagged => roll_the_dice(world, client, mob, car, now),
            CANNON if driver && clown && c.emagged => toggle_cannon(world, client, car, now),
            _ => {}
        }
    }
}

/// `/obj/vehicle/sealed/remove_key`.
fn remove_key(world: &mut World, client: Entity, mob: Entity, car: Entity) {
    let what = name_of(world, car);
    let Some(key) = inserted_key(world, car) else {
        tell(world, client, Span::Warning, format!("В {what} нет ключа!"));
        return;
    };
    let name = name_of(world, key);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы вынимаете {name} из {what}."),
    );
    if let Some(mut c) = world.get_mut::<Car>(car) {
        c.key = None;
    }
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, key);
    } else if let Some(&pos) = world.get::<TilePos>(car) {
        crate::storage::take_out(world, key, pos);
    }
}

/// `/datum/action/vehicle/sealed/horn/Trigger`: the inserted bike horn honks, else the horn.
fn honk(world: &mut World, client: Entity, mob: Entity, car: Entity, now: f64) {
    if let Some(mut o) = world.get_mut::<CarOccupant>(mob) {
        o.next_honk = now + HONK_SECS;
    }
    let what = name_of(world, car);
    if let Some(&pos) = world.get::<TilePos>(car) {
        visible(
            world,
            pos,
            Span::Danger,
            format!("{what} громко сигналит!"),
            &[mob],
        );
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы жмёте на клаксон {what}."),
    );
    let horn = inserted_key(world, car).is_some_and(|k| is_under(&path_of(world, k), BIKEHORN));
    if horn {
        sound(world, car, BIKEHORN_SOUND, 50.0);
    } else {
        sound(world, car, CAR_HORN, 75.0);
    }
    refresh_buttons(world, mob);
}

/// `/datum/action/vehicle/sealed/headlights/Trigger`.
fn toggle_headlights(world: &mut World, client: Entity, mob: Entity, car: Entity) {
    tell(
        world,
        client,
        Span::Notice,
        "Вы щёлкаете переключателем фар.",
    );
    let Some(mut c) = world.get_mut::<Car>(car) else {
        return;
    };
    c.headlights = !c.headlights;
    let on = c.headlights;
    set_light(world, car, on, None);
    sound(world, mob, if on { LIGHTS_ON } else { LIGHTS_OFF }, 40.0);
}

/// `/datum/action/vehicle/sealed/thank/Trigger` and `increment_thanks_counter`.
fn thank(world: &mut World, client: Entity, mob: Entity, car: Entity, now: f64) {
    if let Some(mut o) = world.get_mut::<CarOccupant>(mob) {
        o.next_thank = now + THANK_SECS;
    }
    refresh_buttons(world, mob);
    let drivers = controllers(world, car, DRIVE);
    if drivers.is_empty() {
        tell(
            world,
            client,
            Span::Danger,
            "Вы собираетесь поблагодарить водителя, но понимаете, что его нет.",
        );
        return;
    }
    let i = world
        .resource_mut::<CombatRng>()
        .range(0, drivers.len() as u32 - 1);
    let clown = name_of(world, drivers[i as usize]);
    if let Some(&pos) = world.get::<TilePos>(mob) {
        let name = name_of(world, mob);
        let clients = crate::chem::clients_near(world, pos, None);
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: ss13_chat::server::say_message(
                &name,
                &format!("Спасибо за веселую поездку, {clown}!"),
            ),
        });
    }
    if let Some(mut c) = world.get_mut::<Car>(car) {
        c.thanks += 1;
    }
}

/// `/obj/vehicle/sealed/car/clowncar/roll_the_dice`.
fn roll_the_dice(world: &mut World, client: Entity, mob: Entity, car: Entity, now: f64) {
    let Some(&pos) = world.get::<TilePos>(car) else {
        return;
    };
    playsound(
        world,
        PlaySound::new(pos, BUTTON_PRESS, 50.0, true).extrarange(MEDIUM_RANGE),
    );
    let Some(c) = world.get::<Car>(car) else {
        return;
    };
    if now < c.next_dice {
        tell(world, client, Span::Notice, "Панель кнопок перезаряжается.");
        return;
    }
    world.get_mut::<Car>(car).expect("checked").next_dice = now + DICE_SECS;
    for driver in controllers(world, car, DRIVE) {
        refresh_buttons(world, driver);
    }
    let (user, what) = (name_of(world, mob), name_of(world, car));
    let pressed = format!("{user} нажимает одну из разноцветных кнопок на {what}");
    let say = |world: &mut World, tail: &str| {
        visible(world, pos, Span::Danger, format!("{pressed}, {tail}"), &[]);
    };
    let roll = world.resource_mut::<CombatRng>().range(1, 6);
    match roll {
        1 => {
            say(world, "и из неё выпадает особая банановая кожура.");
            crate::construction::spawn_obj(world, SPECIAL_PEEL, pos);
        }
        2 => {
            say(world, "и из неё хлещут неизвестные химикаты.");
            if let Some(reagent) = crate::mobs::farm::random_reagent(world) {
                info!("clown car dice: foam of {reagent} at {pos:?}");
                reagent_foam(world, pos, 200, &reagent, 100.0);
            }
        }
        3 => {
            say(
                world,
                "и клоунмобиль включает систему маскировки под сингулярность.",
            );
            if let Some(id) = world
                .get_resource::<IconIndex>()
                .and_then(|i| i.get(SINGULARITY_DMI, "singularity_s1"))
                && let Some(mut look) = world.get_mut::<Appearance>(car)
            {
                look.icon_state = id;
            }
            world.get_mut::<Car>(car).expect("checked").singulo_until = Some(now + SINGULO_SECS);
        }
        4 => {
            say(world, "и клоунмобиль выпускает облако веселящего газа.");
            if let Some(holder) = reagent_holder(world, SUPERLAUGHTER, 50.0) {
                crate::effects::smoke::do_chem_smoke(
                    world,
                    4,
                    pos,
                    None,
                    &holder,
                    crate::effects::smoke::SmokeKind::Chem,
                );
            }
        }
        5 => {
            say(world, "и клоунмобиль начинает оставлять масляный след.");
            world.get_mut::<Car>(car).expect("checked").oil_until = now + OIL_SECS;
        }
        _ => {
            say(world, "и клоунмобиль издаёт комичный гудок.");
            playsound(world, PlaySound::new(pos, FART, 100.0, true));
            let laughers: Vec<Entity> = world
                .resource::<SpatialIndex>()
                .in_radius(pos, 6)
                .filter(|&e| world.get::<Health>(e).is_some())
                .collect();
            for mob in laughers {
                crate::medical::diseases::effects::emote(world, mob, "laugh");
            }
        }
    }
}

/// `/obj/vehicle/sealed/car/clowncar/toggle_cannon`.
fn toggle_cannon(world: &mut World, client: Entity, car: Entity, now: f64) {
    let Some(&pos) = world.get::<TilePos>(car) else {
        return;
    };
    let Some(cannon) = world.get::<Car>(car).map(|c| c.cannon) else {
        return;
    };
    let what = name_of(world, car);
    let (next, state, sound_file, text) = match cannon {
        Cannon::Opening | Cannon::Closing => {
            tell(
                world,
                client,
                Span::Notice,
                "Дождитесь, пока машина закончит текущее действие.",
            );
            return;
        }
        Cannon::Ready => (
            Cannon::Closing,
            "clowncar",
            CANNON_CLOSE,
            format!("{what} возвращается в походный режим."),
        ),
        Cannon::Inactive => (
            Cannon::Opening,
            "clowncar_fire",
            CANNON_OPEN,
            format!("{what} раскрывается, обнажая большую пушку."),
        ),
    };
    {
        let mut c = world.get_mut::<Car>(car).expect("checked");
        c.cannon = next;
        c.cannon_at = now + CANNON_SECS;
    }
    crate::obj_damage::set_icon_state(world, car, state);
    visible(world, pos, Span::Danger, text, &[]);
    playsound(world, PlaySound::new(pos, sound_file, 75.0, true));
    for driver in controllers(world, car, DRIVE) {
        refresh_buttons(world, driver);
    }
}

/// `fire_cannon_at`: a ready cannon's driver clicked a turf or a thing on one; a random
/// kidnapped occupant flies there.
fn cannon_clicks(world: &mut World, mut clicks: Local<MessageCursor<InterceptedClick>>) {
    let pending: Vec<InterceptedClick> = clicks
        .read(world.resource::<Messages<InterceptedClick>>())
        .copied()
        .filter(|c| world.get::<CannonAim>(c.mob).is_some())
        .collect();
    for click in pending {
        let Some(occ) = world.get::<CarOccupant>(click.mob).copied() else {
            continue;
        };
        let car = occ.car;
        if !occ.driver() || world.get::<Car>(car).map(|c| c.cannon) != Some(Cannon::Ready) {
            continue;
        }
        // The driver can still interact with its inventory.
        if world.get::<StoredIn>(click.target).is_some()
            || world
                .get::<crate::inventory::CarriedBy>(click.target)
                .is_some()
            || contained(world, click.target)
        {
            continue;
        }
        let kidnapped = controllers(world, car, KIDNAPPED);
        if kidnapped.is_empty() {
            continue;
        }
        let i = world
            .resource_mut::<CombatRng>()
            .range(0, kidnapped.len() as u32 - 1);
        let sod = kidnapped[i as usize];
        mob_exit(world, sod, true, false);
        let shot = pick(world, &CANNON_SHOTS);
        sound(world, car, shot, 75.0);
        crate::throw::throw_at(world, sod, click.target_pos, 10, 2.0);
        info!(
            "clown car cannon fired a mob towards {:?}",
            click.target_pos
        );
    }
}

// --- Reagents -------------------------------------------------------------------------------

fn reagent_holder(world: &World, path: &str, volume: f32) -> Option<ss13_reagents::Holder> {
    let chem = world.get_resource::<crate::chem::Chem>()?.0.clone();
    let id = chem.id(path)?;
    let mut holder = ss13_reagents::Holder::new(volume, 0, ss13_reagents::Owner::default());
    holder.add_reagent(
        &chem,
        id,
        volume,
        ss13_atmos::gas::T20C as f32,
        None,
        None,
        true,
        false,
    );
    Some(holder)
}

/// `do_foam(range, src, loc, reagent, volume)`.
fn reagent_foam(world: &mut World, pos: TilePos, range: u32, reagent: &str, volume: f32) {
    use crate::effects::foam::{FoamKind, FoamOptions, do_foam};
    let carry = reagent_holder(world, reagent, volume);
    do_foam(
        world,
        range,
        pos,
        None,
        carry.as_ref(),
        FoamKind::Plain,
        FoamOptions::default(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_tg() {
        assert_eq!(CarKind::of(CLOWNCAR), Some(CarKind::Clown));
        assert_eq!(CarKind::of(SPEEDWAGON), Some(CarKind::Speedwagon));
        assert_eq!(CarKind::of("/obj/vehicle/sealed/mecha/ripley"), None);
        assert_eq!(CarKind::Clown.max_occupants(), 50);
        assert_eq!(CarKind::Speedwagon.max_occupants(), 4);
        assert_eq!(CarKind::Clown.access_flags(), DRIVE | KIDNAPPED);
        assert!(CarKind::Clown.can_kidnap() && !CarKind::Speedwagon.can_kidnap());
        assert_eq!(
            far(TilePos::new(5, 5, 1), Dir::East, 4),
            TilePos::new(9, 5, 1)
        );
    }
}
