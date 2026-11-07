//! The exosuit (server), after tg `/obj/vehicle/sealed/mecha` (`_mecha.dm`, `mecha_movement.dm`,
//! `mecha_mob_interaction.dm`, `mecha_defense.dm`, `mecha_damage.dm`, `mech_melee_attack.dm`,
//! `mecha_wreckage.dm`) and `/obj/vehicle/sealed` (`sealed.dm`).
//!
//! - Parts: the cell, scanning module, capacitor and servo sit inside the mech ([`StoredIn`]);
//!   a mech from the map gets tg's `populate_parts` (a high-capacity cell, tier 1 parts), a built
//!   one keeps the chassis' parts. Cells carry their charge ([`ItemCell`]).
//! - Entering: a human drags itself onto the mech (`mouse_drop_receive` -> `mob_try_enter`):
//!   `enter_delay` `do_after` with tg's `enter_checks` (conscious, empty hands, room, not
//!   destroyed), the ID lock. The pilot ([`Pilot`]) keeps the mech's tile, hidden and
//!   unclickable; the mech shows its occupied state. Resist (B) is the ejection
//!   (`container_resist_act`: `exit_delay`).
//! - Driving: the pilot's steps are the mech's (`relaymove` -> `vehicle_move`): the move cooldown
//!   is BandaStation's `modified_move_delay(movedelay)`, every attempt costs `step_energy_drain`
//!   from the cell (tg `can_move`, also for a turn) and needs the cell, capacitor and servo; a
//!   step in another direction turns the mech first (a second step moves, unless `pivot_step`);
//!   strafing (alt+click on the mech) keeps the facing. Ripleys switch `movedelay` with the
//!   tile pressure (`update_pressure`). Bumped doors open with the pilot's access; an unanchored
//!   object or a mob in the way is pushed a step (`try_bumpsmash`).
//! - Clicks of the pilot (`on_mouseclick`): middle toggles the safety, alt on the mech toggles
//!   strafing; otherwise, facing the target, the left arm's (right click: the right arm's)
//!   equipment acts on an adjacent target ([`super::equipment`]), else the mech punches it
//!   (`mech_melee_attack`: mobs are pushed, or hit in combat mode; structures and machines take
//!   `force * 3`).
//! - Damage: the mech takes the hits instead of the pilot (`take_damage` with its armor and
//!   tg's facing modifiers: front ×0.5, side ×1, back ×1.5); an item hit, a punch ("Nothing
//!   happens"), an emitter beam. At 0 integrity (`atom_destruction`) the pilot is thrown out
//!   (paralyzed 2 s for tg's sleep), the cargo drops, a wreckage takes 30% of the detachable
//!   equipment and the cell, and the mech is gone. Wreckages give salvage to a welder (30% fail,
//!   `salvage_num` times) and a crowbar (the equipment and cell).
//! - The pilot's screen alerts: charge (`ALERT_CHARGE`) and integrity (`ALERT_MECH_DAMAGE`).
//!
//! Cost: one relay per pilot step, one alert pass per occupied mech per tick.
// shortcut: no Mecha tgui window (stats, equipment selection, the drill toggle, cargo eject,
// lights, cabin seal, internals, DNA lock), no action buttons; the drill acts on clicks while
// attached. No internal damage, overclock, thrusters, MMI/AI pilots, mech bay recharging,
// tracking beacons, mouse pointer, sounds, speech bubbles, open-canopy hits on the pilot,
// thrown-item hits, wall punches or `TRAIT_HANDS_BLOCKED` for the HUD slots of a pilot.

use std::collections::HashMap;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::mecha::{CellDef, MechDef, MechaData};
use ss13_mapload::{DmItem, DmName, IconIndex, MapLoadPlugin};
use ss13_net::chat::server::SendChat;
use ss13_net::server::IntentReceived;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::{Appearance, Dir, Facing, MouseOpacity, TilePos};
use ss13_world::{Bumped, Density, MoveRequest, Moved, Mover, SpatialIndex, WorldSystems};

use super::equipment::{self, MechEquip};
use crate::breath::{AlertCategory, Alerts};
use crate::combat::{CombatMode, CombatRng, NextMove};
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::{IdCard, Inventory};
use crate::obj_damage::{Anchored, strip};
use crate::power::model::Cell;
use crate::storage::{put_inside, take_out};
use crate::tools::{Actor, DoAfter, do_after, tell};

/// `ID_LOCK_ON`, `CAN_STRAFE`, `IS_ENCLOSED`, `OMNIDIRECTIONAL_ATTACKS` (`__DEFINES/mecha.dm`).
pub const ID_LOCK_ON: u32 = 1 << 1;
pub const CAN_STRAFE: u32 = 1 << 2;
pub const IS_ENCLOSED: u32 = 1 << 5;
pub const OMNIDIRECTIONAL_ATTACKS: u32 = 1 << 12;
/// BandaStation `TG_SPEED` and the config's `RUN_DELAY` (`modified_move_delay`), deciseconds.
const TG_SPEED: f32 = 1.5;
const RP_SPEED: f32 = crate::RUN_DELAY_SECS * 10.0;
/// `LAVALAND_EQUIPMENT_EFFECT_PRESSURE`, kPa.
const LAVALAND_EQUIPMENT_EFFECT_PRESSURE: f64 = 50.0;
/// `COOLDOWN_MECHA_MESSAGE`.
const MESSAGE_COOLDOWN_SECS: f64 = 2.0;
/// `CLICK_CD_SLOW` (`melee_cooldown`), `CLICK_CD_MELEE`.
const MELEE_COOLDOWN_SECS: f64 = 1.0;
const CLICK_CD_MELEE_SECS: f64 = 0.8;
/// `/obj/vehicle/sealed/mecha::force`, `destruction_sleep_duration`.
const MECH_FORCE: f32 = 5.0;
const DESTRUCTION_SLEEP_SECS: f32 = 2.0;
const SCANNING_MODULE: &str = "/obj/item/stock_parts/scanning_module";
const CAPACITOR: &str = "/obj/item/stock_parts/capacitor";
const SERVO: &str = "/obj/item/stock_parts/servo";
const CELL: &str = "/obj/item/stock_parts/power_store";
/// `populate_parts`.
const DEFAULT_PARTS: [&str; 4] = [
    "/obj/item/stock_parts/power_store/cell/high",
    SCANNING_MODULE,
    CAPACITOR,
    SERVO,
];
const AIRLOCK: &str = "/obj/machinery/door";

/// `<assets>/mecha.ron` with lookups by type.
#[derive(Resource, Debug, Default)]
pub struct MechaDb {
    pub data: MechaData,
    mechs: HashMap<String, usize>,
    equipment: HashMap<String, usize>,
    chassis: HashMap<String, usize>,
    wreckages: HashMap<String, usize>,
    cells: HashMap<String, usize>,
    ratings: HashMap<String, u32>,
}

impl MechaDb {
    pub fn new(data: MechaData) -> Self {
        let index = |paths: Vec<&String>| {
            paths
                .into_iter()
                .enumerate()
                .map(|(i, p)| (p.clone(), i))
                .collect::<HashMap<_, _>>()
        };
        Self {
            mechs: index(data.mechs.iter().map(|m| &m.path).collect()),
            equipment: index(data.equipment.iter().map(|e| &e.path).collect()),
            chassis: index(data.constructions.iter().map(|c| &c.chassis).collect()),
            wreckages: index(data.wreckages.iter().map(|w| &w.path).collect()),
            cells: index(data.cells.iter().map(|c| &c.path).collect()),
            ratings: data.stock_parts.iter().cloned().collect(),
            data,
        }
    }

    pub fn mech(&self, path: &str) -> Option<usize> {
        self.mechs.get(path).copied()
    }

    pub fn equipment(&self, path: &str) -> Option<usize> {
        self.equipment.get(path).copied()
    }

    pub fn construction(&self, chassis: &str) -> Option<usize> {
        self.chassis.get(chassis).copied()
    }

    pub fn wreckage(&self, path: &str) -> Option<usize> {
        self.wreckages.get(path).copied()
    }

    pub fn cell(&self, path: &str) -> Option<&CellDef> {
        self.cells.get(path).map(|&i| &self.data.cells[i])
    }

    /// A stock part's `rating` (1 when unknown).
    pub fn rating(&self, path: &str) -> u32 {
        self.ratings.get(path).copied().unwrap_or(1)
    }
}

/// A power cell item's charge (tg `/obj/item/stock_parts/power_store`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ItemCell(pub Cell);

/// The live exosuit (tg `/obj/vehicle/sealed/mecha` vars).
#[derive(Component, Clone, Debug)]
pub struct Mech {
    /// Index into [`MechaData::mechs`].
    pub def: usize,
    pub integrity: f32,
    pub occupant: Option<Entity>,
    pub cell: Option<Entity>,
    pub scanmod: Option<Entity>,
    pub capacitor: Option<Entity>,
    pub servo: Option<Entity>,
    pub l_arm: Option<Entity>,
    pub r_arm: Option<Entity>,
    /// `equip_by_category` utility, power and armor modules.
    pub modules: Vec<Entity>,
    /// The cargo compartment's contents (`cargo_hold.contents`).
    pub cargo: Vec<Entity>,
    /// Current `movedelay`, deciseconds.
    pub movedelay: f32,
    /// Current `step_energy_drain` and `melee_energy_drain`, joules.
    pub step_energy_drain: f64,
    pub melee_energy_drain: f64,
    /// `cooldown_vehicle_move`: when the next move may start.
    pub next_move: f64,
    /// `COOLDOWN_MECHA_MELEE_ATTACK`.
    pub next_melee: f64,
    /// `COOLDOWN_MECHA_EQUIPMENT(type)`.
    pub equipment_cooldowns: Vec<(usize, f64)>,
    /// `COOLDOWN_MECHA_MESSAGE`.
    pub next_message: f64,
    /// `mecha_flags`.
    pub flags: u32,
    pub weapons_safety: bool,
    pub strafe: bool,
    pub is_currently_ejecting: bool,
    /// The facing to keep after a strafing step.
    strafe_facing: Option<Dir>,
}

impl Mech {
    pub fn new(def: usize, db: &MechaDb) -> Self {
        let d = &db.data.mechs[def];
        Self {
            def,
            integrity: d.max_integrity,
            occupant: None,
            cell: None,
            scanmod: None,
            capacitor: None,
            servo: None,
            l_arm: None,
            r_arm: None,
            modules: Vec::new(),
            cargo: Vec::new(),
            movedelay: d.movedelay,
            step_energy_drain: 2.0 * d.step_energy_drain,
            melee_energy_drain: d.melee_energy_drain,
            next_move: 0.0,
            next_melee: 0.0,
            equipment_cooldowns: Vec::new(),
            next_message: 0.0,
            flags: d.mecha_flags,
            weapons_safety: false,
            strafe: false,
            is_currently_ejecting: false,
            strafe_facing: None,
        }
    }

    pub fn equipment(&self) -> impl Iterator<Item = Entity> + '_ {
        self.l_arm
            .into_iter()
            .chain(self.r_arm)
            .chain(self.modules.iter().copied())
    }

    /// `TIMER_COOLDOWN_RUNNING(src, COOLDOWN_MECHA_EQUIPMENT(type))`.
    pub fn equipment_ready(&self, equip: usize, now: f64) -> bool {
        self.equipment_cooldowns
            .iter()
            .all(|&(e, until)| e != equip || now >= until)
    }

    pub fn start_equipment_cooldown(&mut self, equip: usize, until: f64) {
        self.equipment_cooldowns.retain(|&(e, _)| e != equip);
        self.equipment_cooldowns.push((equip, until));
    }
}

/// On the pilot: the mech it is in and what it looked like outside.
#[derive(Component, Clone, Copy, Debug)]
pub struct Pilot {
    pub mech: Entity,
    /// The mech's step time in seconds (the pilot's HUD `move_delay`, which the client predicts
    /// with).
    pub move_delay: f32,
    look: (Option<[u8; 4]>, MouseOpacity),
}

/// A step of a pilot (written by `crate::server` instead of moving it): tg `relaymove`.
#[derive(Message, Clone, Copy, Debug)]
pub struct MechRelayMove {
    pub pilot: Entity,
    pub dir: Dir,
}

/// A click of a pilot that passed `crate::click`'s sight checks: tg `on_mouseclick`.
#[derive(Message, Clone, Copy, Debug)]
pub struct MechClick {
    pub client: Entity,
    pub pilot: Entity,
    pub target: Entity,
    pub target_pos: TilePos,
    pub modifiers: ClickModifiers,
}

/// The timed steps of exosuits (`crate::construction::Job::Mecha`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MechJob {
    /// The target mech is entered (`mob_try_enter`).
    Enter,
    /// The pilot leaves (`container_resist_act`).
    Exit,
    /// Equipment of the pilot's mech acts on the target when the `do_after` ends.
    Equipment(equipment::EquipJob),
}

/// Wreckage salvage state (tg `/obj/structure/mecha_wreckage` vars).
#[derive(Component, Clone, Debug, Default)]
pub struct Wreckage {
    pub welder_salvage: Vec<String>,
    pub salvage_num: u32,
    /// Equipment and the cell, inside it.
    pub crowbar_salvage: Vec<Entity>,
}

/// After [`MechCorePlugin`]'s startup: the mechs and their parts exist.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MechaLoaded;

pub struct MechCorePlugin;

impl Plugin for MechCorePlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` of a mech: in combat mode "Nothing happens".
        app.add_click_handler(ClickPhase::EmptyHand, 2300, |w, c| {
            crate::mecha::core::attack_hand(w, c.client, c.mob, c.target)
        });
        app.add_message::<MechRelayMove>()
            .add_message::<MechClick>()
            .add_systems(
                Startup,
                load_mecha
                    .in_set(MechaLoaded)
                    .after(crate::storage::StorageStartup),
            )
            .add_systems(
                ss13_world::MapInit,
                (init_cells, init_mapped_mechs)
                    .chain()
                    .in_set(MechaLoaded)
                    .after(crate::storage::StorageStartup),
            )
            .add_systems(
                Update,
                (
                    relay_moves
                        .after(crate::server::forward_moves)
                        .before(WorldSystems::Movement),
                    (after_moves, mech_bumps)
                        .chain()
                        .after(WorldSystems::Movement)
                        .before(crate::server::ack_moves),
                    mech_clicks.after(crate::click::ResolveClicks),
                    eject_on_resist.after(WorldSystems::Movement),
                    (pilot_alerts, hide_pilots).after(crate::inventory::inventory_intents),
                ),
            )
            .add_observer(cell_charge)
            .add_observer(release_on_despawn);
    }
}

/// Reads `<assets>/mecha.ron` unless a test inserted [`MechaDb`].
fn load_mecha(world: &mut World) {
    if world.contains_resource::<MechaDb>() {
        return;
    }
    let path = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.join(ss13_mapload::mecha::MECHA_FILE));
    let data = match path.as_deref().map(MechaData::read) {
        Some(Ok(data)) => data,
        Some(Err(e)) => {
            warn!("{e:#}; no exosuits (rerun `ss13-mapload convert`)");
            MechaData::default()
        }
        None => MechaData::default(),
    };
    info!(
        "mecha: {} mech types, {} equipment types, {} chassis",
        data.mechs.len(),
        data.equipment.len(),
        data.constructions.len()
    );
    world.insert_resource(MechaDb::new(data));
}

fn new_cell(def: &CellDef) -> ItemCell {
    ItemCell(Cell {
        charge: if def.empty { 0.0 } else { def.maxcharge },
        maxcharge: def.maxcharge,
        chargerate: def.chargerate,
    })
}

/// Cells spawned later (printed, from a crate) get their charge.
fn cell_charge(
    add: On<Add<DmItem>>,
    items: Query<&DmItem, Without<ItemCell>>,
    db: Option<Res<MechaDb>>,
    mut commands: Commands,
) {
    let (Ok(item), Some(db)) = (items.get(add.entity), db) else {
        return;
    };
    if let Some(def) = db.cell(&item.path) {
        commands.entity(add.entity).insert(new_cell(def));
    }
}

/// Cells already on the map.
fn init_cells(world: &mut World) {
    let mut query =
        world.query_filtered::<(Entity, &DmItem), (Without<ItemCell>, With<ss13_world::Mapped>)>();
    let db = world.resource::<MechaDb>();
    let cells: Vec<(Entity, ItemCell)> = query
        .iter(world)
        .filter_map(|(e, i)| Some((e, new_cell(db.cell(&i.path)?))))
        .collect();
    world.insert_batch(cells);
}

/// The DM type of an atom.
pub(crate) fn path_of(world: &World, e: Entity) -> Option<String> {
    crate::construction::path_of(world, e)
}

/// Mechs placed by the map: tg `Initialize` with `populate_parts` and the default equipment.
fn init_mapped_mechs(world: &mut World) {
    let mut query =
        world.query_filtered::<(Entity, &ss13_mapload::DmObj), (With<TilePos>, Without<Mech>, With<ss13_world::Mapped>)>();
    let db = world.resource::<MechaDb>();
    let mapped: Vec<(Entity, String)> = query
        .iter(world)
        .filter(|(_, o)| db.mech(&o.path).is_some())
        .map(|(e, o)| (e, o.path.clone()))
        .collect();
    let count = mapped.len();
    for (e, path) in mapped {
        init_mech(world, e, &path, false);
    }
    info!("mecha: {count} mechs on the map");
}

/// tg `/obj/vehicle/sealed/mecha/Initialize(mapload, built_manually)` on a spawned mech entity.
pub fn init_mech(world: &mut World, mech: Entity, path: &str, built_manually: bool) {
    let Some(def) = world.resource::<MechaDb>().mech(path) else {
        return;
    };
    let state = Mech::new(def, world.resource::<MechaDb>());
    let defaults = world.resource::<MechaDb>().data.mechs[def]
        .equipment
        .clone();
    world.entity_mut(mech).insert((state, Mover::new(0.0)));
    if world.get::<Facing>(mech).is_none() {
        world.entity_mut(mech).insert(Facing(Dir::South));
    }
    if !built_manually {
        for part in DEFAULT_PARTS {
            spawn_inside(world, part, mech);
        }
    }
    locate_parts(world, mech);
    for (category, equip) in defaults {
        if let Some(item) = spawn_inside(world, &equip, mech) {
            equipment::attach(world, mech, item, category == "mecha_r_arm");
        }
    }
    refresh_look(world, mech);
}

/// `new path(src)`: an item spawned inside `container`.
pub(crate) fn spawn_inside(world: &mut World, path: &str, container: Entity) -> Option<Entity> {
    let pos = *world.get::<TilePos>(container)?;
    let item = crate::construction::spawn_obj(world, path, pos)?;
    put_inside(world, item, container);
    Some(item)
}

/// tg `locate_parts` + `update_part_values`: the stock parts inside.
pub fn locate_parts(world: &mut World, mech: Entity) {
    let contents: Vec<Entity> = world
        .get::<crate::storage::Stored>(mech)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    let of = |root: &str| {
        contents.iter().copied().find(|&e| {
            world
                .get::<DmItem>(e)
                .is_some_and(|i| crate::obj_damage::is_under(&i.path, root))
        })
    };
    let (cell, scanmod, capacitor, servo) =
        (of(CELL), of(SCANNING_MODULE), of(CAPACITOR), of(SERVO));
    if let Some(mut m) = world.get_mut::<Mech>(mech) {
        m.cell = cell;
        m.scanmod = scanmod;
        m.capacitor = capacitor;
        m.servo = servo;
    }
    update_energy_drain(world, mech);
}

/// tg `update_energy_drain`.
fn update_energy_drain(world: &mut World, mech: Entity) {
    let Some(m) = world.get::<Mech>(mech) else {
        return;
    };
    let db = world.resource::<MechaDb>();
    let def = &db.data.mechs[m.def];
    let rating = |part: Option<Entity>| {
        part.and_then(|p| world.get::<DmItem>(p))
            .map(|i| db.rating(&i.path) as f64)
    };
    let step = match rating(m.servo) {
        Some(r) => def.step_energy_drain / r,
        None => 2.0 * def.step_energy_drain,
    };
    let melee = rating(m.capacitor).map_or(def.melee_energy_drain, |r| def.melee_energy_drain / r);
    let mut m = world.get_mut::<Mech>(mech).expect("checked above");
    m.step_energy_drain = step;
    m.melee_energy_drain = melee;
}

/// BandaStation `modified_move_delay`: tg delays scaled to the server's run speed.
pub fn modified_move_delay(move_delay: f32) -> f32 {
    if move_delay == 0.0 {
        return ((RP_SPEED - TG_SPEED).max(0.0) * TG_SPEED * 100.0).round() / 100.0;
    }
    (RP_SPEED / TG_SPEED * move_delay * 100.0).round() / 100.0
}

/// The mech's cell charge (`get_charge`), 0 without a cell.
pub fn charge(world: &World, mech: Entity) -> f64 {
    world
        .get::<Mech>(mech)
        .and_then(|m| m.cell)
        .and_then(|c| world.get::<ItemCell>(c))
        .map_or(0.0, |c| c.0.charge)
}

/// tg `has_charge`.
pub fn has_charge(world: &World, mech: Entity, amount: f64) -> bool {
    charge(world, mech) >= amount
}

/// tg `use_energy`: all or nothing.
pub fn use_energy(world: &mut World, mech: Entity, amount: f64) -> bool {
    let Some(cell) = world.get::<Mech>(mech).and_then(|m| m.cell) else {
        return false;
    };
    let Some(mut c) = world.get_mut::<ItemCell>(cell) else {
        return false;
    };
    c.0.take(amount, false) > 0.0 || amount <= 0.0
}

/// tg `get_mecha_occupancy_state`: `<base>-open` while empty.
pub fn refresh_look(world: &mut World, mech: Entity) {
    let Some(m) = world.get::<Mech>(mech) else {
        return;
    };
    let def = &world.resource::<MechaDb>().data.mechs[m.def];
    let state = if m.occupant.is_some() {
        def.base_icon_state.clone()
    } else {
        format!("{}-open", def.base_icon_state)
    };
    let icon = def.icon.clone();
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&icon, &state))
    else {
        return;
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(mech)
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
}

/// The display name of an atom, without text macros.
pub(crate) fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| strip(&n.name).replace("\\\"", "\""))
        .or_else(|| world.get::<Name>(e).map(|n| n.as_str().to_owned()))
        .unwrap_or_else(|| "что-то".to_owned())
}

pub(crate) use crate::tools::client_of;

/// `to_chat(occupants, ...)`.
pub(crate) fn tell_occupant(world: &mut World, mech: Entity, span: Span, text: impl Into<String>) {
    let Some(client) = world
        .get::<Mech>(mech)
        .and_then(|m| m.occupant)
        .and_then(|p| client_of(world, p))
    else {
        return;
    };
    tell(world, client, span, text);
}

/// `visible_message` around `pos` (within 7 tiles), except `exclude`.
pub(crate) fn visible(
    world: &mut World,
    pos: TilePos,
    span: Span,
    text: String,
    exclude: &[Entity],
) {
    let clients = crate::construction::clients_near(world, pos, 7, exclude);
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![ChatSegment::new(span, text)],
        },
    });
}

/// A message on the mech's own `COOLDOWN_MECHA_MESSAGE`.
fn throttled_message(world: &mut World, mech: Entity, now: f64, text: String) {
    let Some(mut m) = world.get_mut::<Mech>(mech) else {
        return;
    };
    if now < m.next_message {
        return;
    }
    m.next_message = now + MESSAGE_COOLDOWN_SECS;
    tell_occupant(world, mech, Span::Warning, text);
}

// --- Entering and leaving -----------------------------------------------------------------

/// `/obj/vehicle/sealed/mouse_drop_receive`: the user dragged itself onto a mech. True when the
/// drop was the mech's.
pub fn mouse_drop_receive(world: &mut World, actor: Actor, over: Entity, dragged: Entity) -> bool {
    if world.get::<Mech>(over).is_none() {
        return false;
    }
    if dragged == actor.mob {
        mob_try_enter(world, actor, over);
    }
    true
}

/// tg `mob_try_enter` (`/obj/vehicle/sealed/mecha` then `/obj/vehicle/sealed`).
fn mob_try_enter(world: &mut World, actor: Actor, mech: Entity) {
    // `ishuman(M)`: no silicons in mechas.
    if world.get::<Inventory>(actor.mob).is_none()
        || world
            .get::<crate::silicon::cyborg::Cyborg>(actor.mob)
            .is_some()
    {
        return;
    }
    if let Some(text) = id_lock_refusal(world, mech, actor.mob) {
        tell(world, actor.client, Span::Warning, text);
        return;
    }
    if let Err(text) = enter_checks(world, mech, actor.mob) {
        if let Some(text) = text {
            tell(world, actor.client, Span::Warning, text);
        }
        return;
    }
    let delay = world
        .get::<Mech>(mech)
        .map(|m| world.resource::<MechaDb>().data.mechs[m.def].enter_delay)
        .unwrap_or(0.0);
    do_after(world, actor, mech, delay, 0, Job::Mecha(MechJob::Enter));
}

/// `(mecha_flags & ID_LOCK_ON) && !allowed(M)`.
fn id_lock_refusal(world: &World, mech: Entity, mob: Entity) -> Option<&'static str> {
    let m = world.get::<Mech>(mech)?;
    if m.flags & ID_LOCK_ON == 0 {
        return None;
    }
    let def = &world.resource::<MechaDb>().data.mechs[m.def];
    // `update_access`: `one_access` puts the accesses in `req_one_access`.
    let access = mob_access(world, mob);
    (!crate::inventory::check_access(&[], &def.accesses, &access))
        .then_some("Access denied. Insufficient operation keycodes.")
}

fn mob_access(world: &World, mob: Entity) -> Vec<String> {
    world
        .get::<Inventory>(mob)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default()
}

/// tg `enter_checks`; `Err(Some(text))` tells the mob why.
fn enter_checks(world: &World, mech: Entity, mob: Entity) -> Result<(), Option<&'static str>> {
    let Some(m) = world.get::<Mech>(mech) else {
        return Err(None);
    };
    if world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || crate::status_effects::incapacitation(world, mob).incapacitated
        || world.get::<Pilot>(mob).is_some()
    {
        return Err(None);
    }
    if m.integrity <= 0.0 {
        return Err(Some(
            "You cannot get in the exosuit, it has been destroyed!",
        ));
    }
    if world.get::<crate::medical::surgery::Buckled>(mob).is_some() {
        return Err(Some("You can't enter the exosuit while buckled."));
    }
    let hands = world.get::<Inventory>(mob).map(|i| i.hands);
    if hands.is_some_and(|h| h.iter().any(Option::is_some)) {
        return Err(Some(
            "You can't enter the exosuit while your hands are occupied!",
        ));
    }
    // `occupant_amount() < max_occupants` (1).
    if m.occupant.is_some() {
        return Err(None);
    }
    Ok(())
}

/// tg `mob_enter` + `moved_inside`.
fn moved_inside(world: &mut World, client: Entity, mob: Entity, mech: Entity) {
    let (Some(&pos), Some(&mob_pos)) = (world.get::<TilePos>(mech), world.get::<TilePos>(mob))
    else {
        return;
    };
    if !world
        .resource::<SpatialIndex>()
        .adjacent(mob_pos, pos, |e| {
            e == mech || world.get::<Mover>(e).is_some()
        })
    {
        return;
    }
    let look = world
        .get::<Appearance>(mob)
        .map_or((None, MouseOpacity::Icon), |a| (a.color, a.mouse_opacity));
    let delay = world
        .get::<Mech>(mech)
        .map_or(0.0, |m| modified_move_delay(m.movedelay) / 10.0);
    world.entity_mut(mob).remove::<crate::pull::Pulling>();
    if let Some(puller) = world.get::<crate::pull::PulledBy>(mob).map(|p| p.puller()) {
        world.entity_mut(puller).remove::<crate::pull::Pulling>();
    }
    world.entity_mut(mob).remove::<Density>().insert((
        Pilot {
            mech,
            move_delay: delay,
            look,
        },
        pos,
        Facing(Dir::South),
    ));
    if let Some(mut m) = world.get_mut::<Mech>(mech) {
        m.occupant = Some(mob);
    }
    world.entity_mut(mech).insert(Facing(Dir::South));
    refresh_look(world, mech);
    let (name, mech_name) = (name_of(world, mob), name_of(world, mech));
    visible(
        world,
        pos,
        Span::Notice,
        format!("{name} climbs into the {mech_name}!"),
        &[mob],
    );
    tell(
        world,
        client,
        Span::Notice,
        format!("You climb into the {mech_name}."),
    );
}

/// tg `mob_exit` of the pilot: back onto the mech's tile with its look.
pub fn mob_exit(world: &mut World, mech: Entity) -> Option<Entity> {
    let pilot = world.get_mut::<Mech>(mech)?.occupant.take()?;
    if let Some(&pos) = world.get::<TilePos>(mech) {
        world.entity_mut(pilot).insert(pos);
    }
    release_pilot(world, pilot);
    world.entity_mut(mech).insert(Facing(Dir::South));
    refresh_look(world, mech);
    Some(pilot)
}

/// The pilot's own state back: its look, density, HUD move delay; no mech alerts.
fn release_pilot(world: &mut World, pilot: Entity) {
    let Ok(mut entity) = world.get_entity_mut(pilot) else {
        return;
    };
    if let Some(p) = entity.take::<Pilot>()
        && let Some(mut appearance) = entity.get_mut::<Appearance>()
    {
        (appearance.color, appearance.mouse_opacity) = p.look;
    }
    entity.insert(Density);
    // The look is rebuilt from the inventory; the body state (density while lying, the HUD
    // move delay) from the health.
    if let Some(mut inventory) = entity.get_mut::<Inventory>() {
        inventory.set_changed();
    }
    if let Some(mut health) = entity.get_mut::<Health>() {
        health.set_changed();
    }
    if let Some(mut alerts) = entity.get_mut::<Alerts>() {
        alerts.clear(AlertCategory::Charge);
        alerts.clear(AlertCategory::MechDamage);
    }
}

/// A mech despawned some other way lets its pilot out where it stood.
fn release_on_despawn(remove: On<Remove<Mech>>, mechs: Query<&Mech>, mut commands: Commands) {
    if let Some(pilot) = mechs.get(remove.entity).ok().and_then(|m| m.occupant) {
        commands.queue(move |world: &mut World| release_pilot(world, pilot));
    }
}

/// tg `container_resist_act`: B from inside starts the ejection.
fn eject_on_resist(world: &mut World, mut intents: Local<MessageCursor<IntentReceived>>) {
    let resists: Vec<Entity> = intents
        .read(world.resource::<Messages<IntentReceived>>())
        .filter(|i| matches!(i.intent, ClientIntent::Resist))
        .map(|i| i.client)
        .collect();
    for client in resists {
        let Some(mob) = world.resource::<crate::server::Players>().mob(client) else {
            continue;
        };
        let Some(pilot) = world.get::<Pilot>(mob).copied() else {
            continue;
        };
        if world.get::<DoAfter>(mob).is_some() {
            continue;
        }
        let Some(m) = world.get::<Mech>(pilot.mech) else {
            continue;
        };
        let delay = world.resource::<MechaDb>().data.mechs[m.def].exit_delay;
        tell(
            world,
            client,
            Span::Notice,
            "You begin the ejection procedure. Equipment is disabled during this process. Hold \
             still to finish ejecting.",
        );
        let actor = Actor {
            client,
            mob,
            item: None,
        };
        if do_after(
            world,
            actor,
            pilot.mech,
            delay,
            0,
            Job::Mecha(MechJob::Exit),
        ) && let Some(mut m) = world.get_mut::<Mech>(pilot.mech)
        {
            m.is_currently_ejecting = true;
        }
    }
}

/// End of a [`MechJob`] `do_after` (`crate::tools` checked that neither side moved).
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: MechJob) {
    let target = action.target;
    match job {
        MechJob::Enter => {
            if enter_checks(world, target, mob).is_ok()
                && id_lock_refusal(world, target, mob).is_none()
            {
                moved_inside(world, action.client, mob, target);
            }
        }
        MechJob::Exit => {
            if let Some(mut m) = world.get_mut::<Mech>(target) {
                m.is_currently_ejecting = false;
            }
            if world.get::<Pilot>(mob).is_some_and(|p| p.mech == target) {
                tell(world, action.client, Span::Notice, "You exit the mech.");
                mob_exit(world, target);
            }
        }
        MechJob::Equipment(job) => equipment::complete(world, mob, action, job),
    }
}

/// Every pilot stays invisible and unclickable, also after its look is rebuilt.
fn hide_pilots(
    mut mobs: Query<&mut Appearance, (With<Pilot>, Or<(Changed<Appearance>, Added<Pilot>)>)>,
) {
    let hidden = (Some([255, 255, 255, 0]), MouseOpacity::Transparent);
    for mut appearance in &mut mobs {
        if (appearance.color, appearance.mouse_opacity) != hidden {
            (appearance.color, appearance.mouse_opacity) = hidden;
        }
    }
}

/// tg `process_occupants`: the charge and integrity alerts.
fn pilot_alerts(
    mechs: Query<&Mech>,
    cells: Query<&ItemCell>,
    db: Res<MechaDb>,
    mut pilots: Query<(&Pilot, &mut Alerts)>,
) {
    for (pilot, mut alerts) in &mut pilots {
        let Ok(m) = mechs.get(pilot.mech) else {
            continue;
        };
        let (charge, damage) = mech_alerts(m, m.cell.and_then(|c| cells.get(c).ok()), &db);
        let mut next = alerts.clone();
        match charge {
            Some((state, severity)) => next.throw(AlertCategory::Charge, state, severity),
            None => next.clear(AlertCategory::Charge),
        }
        match damage {
            Some(severity) => next.throw(AlertCategory::MechDamage, "low_mech_integrity", severity),
            None => next.clear(AlertCategory::MechDamage),
        }
        alerts.set_if_neq(next);
    }
}

/// The alerts `process_occupants` throws: (charge state, severity), integrity severity.
pub fn mech_alerts(
    m: &Mech,
    cell: Option<&ItemCell>,
    db: &MechaDb,
) -> (Option<(&'static str, u8)>, Option<u8>) {
    let charge = match cell.filter(|c| c.0.maxcharge > 0.0) {
        Some(c) => {
            let fraction = c.0.charge / c.0.maxcharge;
            match fraction {
                f if f >= 0.75 => None,
                f if f >= 0.5 => Some(("low_cell", 1)),
                f if f >= 0.25 => Some(("low_cell", 2)),
                f if f >= 0.01 => Some(("low_cell", 3)),
                _ => Some(("empty_cell", 0)),
            }
        }
        None => Some(("no_cell", 0)),
    };
    let max = db.data.mechs[m.def].max_integrity.max(1.0);
    let integrity = m.integrity / max * 100.0;
    // tg's `switch` takes the first matching range: 30-45 before 15-35.
    let damage = match integrity {
        i if (30.0..=45.0).contains(&i) => Some(1),
        i if (15.0..=35.0).contains(&i) => Some(2),
        i if i < 15.0 => Some(3),
        _ => None,
    };
    (charge, damage)
}

// --- Driving ------------------------------------------------------------------------------

/// tg `relaymove` -> `vehicle_move` for each pilot step.
fn relay_moves(world: &mut World, mut relays: Local<MessageCursor<MechRelayMove>>) {
    let steps: Vec<MechRelayMove> = relays
        .read(world.resource::<Messages<MechRelayMove>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for step in steps {
        let Some(pilot) = world.get::<Pilot>(step.pilot).copied() else {
            continue;
        };
        vehicle_move(world, pilot.mech, step.dir, now);
    }
}

/// tg `vehicle_move`: the cooldown starts with every attempt.
pub fn vehicle_move(world: &mut World, mech: Entity, dir: Dir, now: f64) -> bool {
    let Some(m) = world.get::<Mech>(mech) else {
        return false;
    };
    if now < m.next_move {
        return false;
    }
    let delay = f64::from(modified_move_delay(m.movedelay)) / 10.0;
    let (strafe, pivot, diagonal_ok) = {
        let def = &world.resource::<MechaDb>().data.mechs[m.def];
        (m.strafe, def.pivot_step, def.allow_diagonal)
    };
    world
        .get_mut::<Mech>(mech)
        .expect("checked above")
        .next_move = now + delay;
    if !can_move(world, mech, now) {
        return false;
    }
    if !diagonal_ok && !Dir::CARDINALS.contains(&dir) {
        return true;
    }
    let facing = world.get::<Facing>(mech).map_or(Dir::South, |f| f.0);
    if facing != dir && !strafe {
        set_dir(world, mech, dir);
        if !pivot {
            return true;
        }
    }
    if strafe {
        world
            .get_mut::<Mech>(mech)
            .expect("checked above")
            .strafe_facing = Some(facing);
    }
    world.write_message(MoveRequest { entity: mech, dir });
    true
}

/// tg `can_move`: parts present, then the step's energy.
fn can_move(world: &mut World, mech: Entity, now: f64) -> bool {
    let Some(m) = world.get::<Mech>(mech) else {
        return false;
    };
    let mut missing = Vec::new();
    if m.cell.is_none() {
        missing.push("power cell");
    }
    if m.capacitor.is_none() {
        missing.push("capacitor");
    }
    if m.servo.is_none() {
        missing.push("servo");
    }
    if !missing.is_empty() {
        let list = english_list(&missing);
        throttled_message(world, mech, now, format!("Missing {list}."));
        return false;
    }
    let drain = m.step_energy_drain;
    if drain != 0.0 && !use_energy(world, mech, drain) {
        throttled_message(world, mech, now, "Insufficient power to move!".to_owned());
        return false;
    }
    true
}

/// DM `english_list`.
fn english_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// tg `setDir` of the mech: its occupants turn with it.
fn set_dir(world: &mut World, mech: Entity, dir: Dir) {
    if let Some(mut f) = world.get_mut::<Facing>(mech) {
        f.set_if_neq(Facing(dir));
    }
    if let Some(pilot) = world.get::<Mech>(mech).and_then(|m| m.occupant)
        && let Some(mut f) = world.get_mut::<Facing>(pilot)
    {
        f.set_if_neq(Facing(dir));
    }
}

/// After a mech moved: the pilot follows, a strafing mech keeps its facing, a Ripley's speed
/// follows the pressure (`update_pressure`).
fn after_moves(world: &mut World, mut moves: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = moves
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .filter(|m| world.get::<Mech>(m.entity).is_some())
        .collect();
    for step in moved {
        let mech = step.entity;
        let Some(m) = world.get::<Mech>(mech) else {
            continue;
        };
        let (occupant, keep) = (m.occupant, m.strafe_facing);
        if let Some(dir) = keep {
            if let Some(mut f) = world.get_mut::<Facing>(mech) {
                f.set_if_neq(Facing(dir));
            }
            world.get_mut::<Mech>(mech).expect("checked").strafe_facing = None;
        }
        update_pressure(world, mech, step.to);
        let facing = world.get::<Facing>(mech).copied();
        if let Some(pilot) = occupant {
            let delay = world
                .get::<Mech>(mech)
                .map_or(0.0, |m| modified_move_delay(m.movedelay) / 10.0);
            let mut e = world.entity_mut(pilot);
            if e.get::<TilePos>() != Some(&step.to) {
                e.insert(step.to);
            }
            if let Some(facing) = facing {
                e.insert(facing);
            }
            if let Some(mut p) = e.get_mut::<Pilot>()
                && p.move_delay != delay
            {
                p.move_delay = delay;
            }
        }
    }
}

/// `/obj/vehicle/sealed/mecha/ripley/proc/update_pressure`.
fn update_pressure(world: &mut World, mech: Entity, pos: TilePos) {
    let Some(m) = world.get::<Mech>(mech) else {
        return;
    };
    let Some((fast, slow)) = world.resource::<MechaDb>().data.mechs[m.def].pressure_step_in else {
        return;
    };
    let low = world
        .get_resource::<ss13_atmos::Atmos>()
        .and_then(|a| a.pressure(pos))
        .is_some_and(|p| p <= LAVALAND_EQUIPMENT_EFFECT_PRESSURE);
    let delay = if low { fast } else { slow };
    if let Some(mut m) = world.get_mut::<Mech>(mech)
        && m.movedelay != delay
    {
        m.movedelay = delay;
    }
}

/// tg `/obj/vehicle/sealed/Bump` (doors open with the pilot's access, in `crate::doors`) and
/// `try_bumpsmash`: an unanchored object or a mob in the way steps on.
fn mech_bumps(world: &mut World, mut bumps: Local<MessageCursor<Bumped>>) {
    let bumped: Vec<Bumped> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .filter(|b| world.get::<Mech>(b.entity).is_some())
        .collect();
    for bump in bumped {
        let Some(dir) = world.get::<Facing>(bump.entity).map(|f| f.0) else {
            continue;
        };
        equipment::bump_mine(world, bump.entity, bump.obstacle);
        let obstacle = bump.obstacle;
        let path = path_of(world, obstacle).unwrap_or_default();
        if crate::obj_damage::is_under(&path, AIRLOCK) {
            continue;
        }
        let pushable = world.get::<Health>(obstacle).is_some()
            || world.get::<Anchored>(obstacle).is_some_and(|a| !a.0)
            || world.get::<DmItem>(obstacle).is_some();
        if pushable && world.get::<Mech>(obstacle).is_none() {
            push_step(world, obstacle, dir);
        }
    }
}

/// tg `step(thing, dir)` of something pushed: one tile if nothing blocks it.
pub(crate) fn push_step(world: &mut World, e: Entity, dir: Dir) -> bool {
    crate::pull::step_once(world, e, dir, None).is_ok()
}

// --- Clicks -------------------------------------------------------------------------------

/// tg `on_mouseclick` and `on_click_alt` of each pilot click.
fn mech_clicks(world: &mut World, mut clicks: Local<MessageCursor<MechClick>>) {
    let pending: Vec<MechClick> = clicks
        .read(world.resource::<Messages<MechClick>>())
        .copied()
        .collect();
    for click in pending {
        on_mouseclick(world, click);
    }
}

fn on_mouseclick(world: &mut World, click: MechClick) {
    let Some(pilot) = world.get::<Pilot>(click.pilot).copied() else {
        return;
    };
    let mech = pilot.mech;
    let Some(m) = world.get::<Mech>(mech).cloned() else {
        return;
    };
    let modifiers = click.modifiers;
    if modifiers.middle {
        let safe = !m.weapons_safety;
        world.get_mut::<Mech>(mech).expect("checked").weapons_safety = safe;
        let text = if safe {
            "equipment safe"
        } else {
            "equipment ready"
        };
        tell(world, click.client, Span::Notice, text);
        return;
    }
    if modifiers.alt {
        // `on_click_alt`: only the mech itself toggles strafing.
        if click.target == mech {
            toggle_strafe(world, click.client, mech);
        }
        return;
    }
    if m.weapons_safety || modifiers.shift {
        return;
    }
    if click.target == mech || !can_interact_with(world, mech, click.pilot) {
        return;
    }
    let Some(&mech_pos) = world.get::<TilePos>(mech) else {
        return;
    };
    let facing = world.get::<Facing>(mech).map_or(Dir::South, |f| f.0);
    let to_target = dir_between(mech_pos, click.target_pos);
    if m.flags & OMNIDIRECTIONAL_ATTACKS == 0
        && let Some(d) = to_target
        && d as u8 & facing as u8 == 0
    {
        return;
    }
    let adjacent = world
        .resource::<SpatialIndex>()
        .adjacent(mech_pos, click.target_pos, |e| {
            e == click.target || e == mech || world.get::<Mover>(e).is_some()
        });
    let selected = if modifiers.right { m.r_arm } else { m.l_arm };
    // A ranged weapon on a target out of reach (`selected.range & MECHA_RANGED`).
    if let Some(equip) = selected
        && !adjacent
        && world
            .get::<MechEquip>(equip)
            .is_some_and(|e| world.resource::<MechaDb>().data.equipment[e.def].range & 2 != 0)
    {
        if world
            .get::<crate::emp::power::MechEquipmentDisabled>(mech)
            .is_some()
        {
            tell_occupant(
                world,
                mech,
                Span::Warning,
                "Error -- Equipment control unit is unresponsive.",
            );
            return;
        }
        super::weapons::fire(world, click, mech, equip);
        return;
    }
    if let Some(equip) = selected
        && adjacent
        && world
            .get::<MechEquip>(equip)
            .is_some_and(|e| world.resource::<MechaDb>().data.equipment[e.def].range & 1 != 0)
    {
        // `action_checks`: an EMP took the equipment control unit down.
        if world
            .get::<crate::emp::power::MechEquipmentDisabled>(mech)
            .is_some()
        {
            tell_occupant(
                world,
                mech,
                Span::Warning,
                "Error -- Equipment control unit is unresponsive.",
            );
            return;
        }
        equipment::action(world, click, mech, equip);
        return;
    }
    let disabled = world
        .get::<crate::emp::power::MechEquipmentDisabled>(mech)
        .is_some();
    // A drill takes the melee clicks while attached (tg: while toggled on in the Mecha window).
    if adjacent && !disabled && equipment::drill_click(world, click, mech) {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < m.next_melee || !adjacent {
        return;
    }
    if !has_charge(world, mech, m.melee_energy_drain) {
        return;
    }
    use_energy(world, mech, m.melee_energy_drain);
    if mech_melee_attack(world, click, mech) {
        world.get_mut::<Mech>(mech).expect("checked").next_melee = now + MELEE_COOLDOWN_SECS;
    }
}

/// tg `toggle_strafe`.
fn toggle_strafe(world: &mut World, client: Entity, mech: Entity) {
    let Some(mut m) = world.get_mut::<Mech>(mech) else {
        return;
    };
    if m.flags & CAN_STRAFE == 0 {
        tell(world, client, Span::Warning, "this mech can't strafe!");
        return;
    }
    m.strafe = !m.strafe;
    let text = if m.strafe {
        "strafing mode on"
    } else {
        "strafing mode off"
    };
    tell(world, client, Span::Notice, text);
}

/// tg `can_interact_with`.
fn can_interact_with(world: &World, mech: Entity, user: Entity) -> bool {
    let Some(m) = world.get::<Mech>(mech) else {
        return false;
    };
    !m.is_currently_ejecting
        && world.get::<Stat>(user) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, user).incapacitated
        && charge(world, mech) > 0.0
}

/// BYOND `get_dir`.
pub fn dir_between(from: TilePos, to: TilePos) -> Option<Dir> {
    let (dx, dy) = ((to.x - from.x).signum(), (to.y - from.y).signum());
    Dir::DMI_ORDER.into_iter().find(|d| d.offset() == (dx, dy))
}

/// tg `mech_melee_attack` of the clicked atom. True when it hit (the melee cooldown starts).
fn mech_melee_attack(world: &mut World, click: MechClick, mech: Entity) -> bool {
    let target = click.target;
    let combat = world.get::<CombatMode>(click.pilot).is_some_and(|c| c.0);
    let (mech_name, thing) = (name_of(world, mech), name_of(world, target));
    let Some(&pos) = world.get::<TilePos>(mech) else {
        return false;
    };
    if world.get::<Health>(target).is_some() {
        let away = dir_between(pos, click.target_pos);
        if !combat {
            if let Some(dir) = away {
                push_step(world, target, dir);
            }
            visible(
                world,
                pos,
                Span::Warning,
                format!("{mech_name} pushes {thing} out of the way."),
                &[click.pilot],
            );
            tell(
                world,
                click.client,
                Span::Danger,
                format!("You push {thing} out of the way."),
            );
            return false;
        }
        if let Some(dir) = away {
            push_step(world, target, dir);
        }
        let damage = {
            let mut rng = world.resource_mut::<CombatRng>();
            let low = (MECH_FORCE * 0.5) as u32;
            rng.range(low, MECH_FORCE as u32) as f32
        };
        hurt_mob(world, target, damage, DamageType::Brute);
        visible(
            world,
            pos,
            Span::Danger,
            format!("{mech_name} hits {thing}!"),
            &[click.pilot],
        );
        tell(
            world,
            click.client,
            Span::Danger,
            format!("You hit {thing}!"),
        );
        return true;
    }
    if !combat || world.get::<crate::obj_damage::Integrity>(target).is_none() {
        return false;
    }
    visible(
        world,
        pos,
        Span::Danger,
        format!("{mech_name} hits {thing}!"),
        &[click.pilot],
    );
    tell(
        world,
        click.client,
        Span::Danger,
        format!("You hit {thing}!"),
    );
    crate::obj_damage::take_damage(world, target, MECH_FORCE * 3.0, DamageType::Brute);
    true
}

/// `apply_damage` to a random bodypart (`get_random_valid_zone(even_weights = TRUE)`).
// shortcut: the target's armor is not checked (`run_armor_check`); armor is not modelled for
// melee yet.
pub(crate) fn hurt_mob(world: &mut World, mob: Entity, amount: f32, kind: DamageType) {
    let zone = {
        let mut rng = world.resource_mut::<CombatRng>();
        crate::zone::BodyZone::ALL[rng.range(0, 5) as usize]
    };
    if let Ok(mut e) = world.get_entity_mut(mob)
        && let Ok((mut health, mut stat)) = e.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        crate::health::apply_damage_zone(&mut health, &mut stat, zone, kind, amount);
    }
}

// --- Damage -------------------------------------------------------------------------------

/// BYOND `dir2angle`.
fn dir2angle(dir: Dir) -> i32 {
    match dir {
        Dir::North => 0,
        Dir::NorthEast => 45,
        Dir::East => 90,
        Dir::SouthEast => 135,
        Dir::South => 180,
        Dir::SouthWest => 225,
        Dir::West => 270,
        Dir::NorthWest => 315,
    }
}

/// tg `get_armour_facing(abs(dir2angle(dir) - dir2angle(attack_dir)))`: the facing modifier of
/// a hit from `attack_dir` (the direction from the mech to the attacker).
pub fn facing_modifier(def: &MechDef, facing: Dir, attack_dir: Dir) -> f32 {
    match (dir2angle(facing) - dir2angle(attack_dir)).abs() {
        180 => def.facing[2],
        0 | 45 => def.facing[0],
        _ => def.facing[1],
    }
}

/// tg `run_atom_armor` of the mech: the armor rating of `flag`, then the facing modifier.
pub fn armor_damage(
    def: &MechDef,
    amount: f32,
    flag: &str,
    facing: Dir,
    attack_dir: Option<Dir>,
) -> f32 {
    let armor = def.armor.rating(flag).clamp(-100.0, 100.0);
    let mut damage = ((amount * (100.0 - armor) * 0.01) / 0.1).round() * 0.1;
    if let Some(attack_dir) = attack_dir {
        damage *= facing_modifier(def, facing, attack_dir);
    }
    damage
}

/// tg `take_damage` of the mech: armor, facing, the occupant's warning, destruction at 0.
/// Returns the damage taken.
pub fn take_damage(
    world: &mut World,
    mech: Entity,
    amount: f32,
    kind: DamageType,
    flag: &str,
    attack_dir: Option<Dir>,
) -> f32 {
    let Some(m) = world.get::<Mech>(mech) else {
        return 0.0;
    };
    if m.integrity <= 0.0 || !matches!(kind, DamageType::Brute | DamageType::Burn) {
        return 0.0;
    }
    let facing = world.get::<Facing>(mech).map_or(Dir::South, |f| f.0);
    let def = &world.resource::<MechaDb>().data.mechs[m.def];
    let damage = armor_damage(def, amount, flag, facing, attack_dir);
    if damage < 0.1 {
        return 0.0;
    }
    let left = {
        let mut m = world.get_mut::<Mech>(mech).expect("checked above");
        m.integrity = (m.integrity - damage).max(0.0);
        m.integrity
    };
    let warn = damage >= 5.0 || world.resource_mut::<CombatRng>().chance(33.0);
    if warn {
        tell_occupant(world, mech, Span::Danger, "Taking damage!");
    }
    if left <= 0.0 {
        atom_destruction(world, mech);
    }
    damage
}

/// tg `/obj/attacked_by` of a held item on the mech; then `attackby`'s over-penetration (no item
/// has armour penetration here).
pub fn attacked_by(world: &mut World, actor: Actor, mech: Entity) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    let Some(dm) = world.get::<DmItem>(item).cloned() else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut next) = world.get_mut::<NextMove>(actor.mob) {
        next.0 = now + f64::from(dm.attack_speed) / 10.0;
    }
    if dm.force <= 0.0 {
        return true;
    }
    let (Some(&pos), Some(&user_pos)) =
        (world.get::<TilePos>(mech), world.get::<TilePos>(actor.mob))
    else {
        return true;
    };
    let kind = DamageType::from_dm(&dm.damtype).unwrap_or(DamageType::Brute);
    let (user, thing, weapon) = (
        name_of(world, actor.mob),
        name_of(world, mech),
        name_of(world, item),
    );
    let damage = take_damage(
        world,
        mech,
        dm.force,
        kind,
        "melee",
        dir_between(pos, user_pos),
    );
    let tail = if damage > 0.0 {
        "."
    } else {
        ", не оставляя следов!"
    };
    visible(
        world,
        pos,
        Span::Danger,
        format!("{user} атакует {thing} с помощью {weapon}{tail}"),
        &[actor.mob],
    );
    tell(
        world,
        actor.client,
        Span::Danger,
        format!("Вы атакуете {thing} с помощью {weapon}{tail}"),
    );
    true
}

/// tg `/obj/vehicle/sealed/mecha/attack_hand` in combat mode: nothing happens.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, mech: Entity) -> bool {
    if world.get::<Mech>(mech).is_none() {
        return false;
    }
    if !world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut next) = world.get_mut::<NextMove>(mob) {
        next.0 = now + CLICK_CD_MELEE_SECS;
    }
    let (user, thing) = (name_of(world, mob), name_of(world, mech));
    if let Some(&pos) = world.get::<TilePos>(mech) {
        visible(
            world,
            pos,
            Span::Danger,
            format!("{user} hits {thing}. Nothing happens."),
            &[mob],
        );
    }
    tell(
        world,
        client,
        Span::Danger,
        format!("You hit {thing}. Nothing happens."),
    );
    true
}

/// tg `bullet_act` of a projectile hitting the mech: `flag` is its `armor_flag`, `from` the
/// direction it came from. True when `target` is a mech.
pub fn bullet_act(
    world: &mut World,
    target: Entity,
    damage: f32,
    kind: DamageType,
    flag: &str,
    from: Option<Dir>,
) -> bool {
    if world.get::<Mech>(target).is_none() {
        return false;
    }
    take_damage(world, target, damage, kind, flag, from);
    true
}

/// tg `atom_destruction`: the pilot is thrown out, the cargo drops, the wreckage takes some
/// equipment and the cell, the mech is gone.
pub fn atom_destruction(world: &mut World, mech: Entity) {
    let Some(&pos) = world.get::<TilePos>(mech) else {
        return;
    };
    if let Some(pilot) = mob_exit(world, mech) {
        crate::status_effects::apply(
            world,
            pilot,
            crate::status_effects::Incap::Paralyze,
            DESTRUCTION_SLEEP_SECS,
        );
    }
    let Some(m) = world.get::<Mech>(mech).cloned() else {
        return;
    };
    let wreckage_path = world.resource::<MechaDb>().data.mechs[m.def]
        .wreckage
        .clone();
    let wreck = wreckage_path.and_then(|w| spawn_wreckage(world, &w, pos));
    let equipment: Vec<Entity> = m.equipment().collect();
    for equip in equipment {
        let detachable = world
            .get::<MechEquip>(equip)
            .is_some_and(|e| world.resource::<MechaDb>().data.equipment[e.def].detachable);
        equipment::detach(world, mech, equip, pos);
        let salvaged =
            wreck.is_some() && detachable && world.resource_mut::<CombatRng>().chance(30.0);
        match wreck {
            Some(wreck) if salvaged => {
                put_inside(world, equip, wreck);
                if let Some(mut w) = world.get_mut::<Wreckage>(wreck) {
                    w.crowbar_salvage.push(equip);
                }
            }
            _ => {
                world.despawn(equip);
            }
        }
    }
    if let (Some(wreck), Some(cell)) = (wreck, m.cell) {
        put_inside(world, cell, wreck);
        let drained = {
            let charge = world.get::<ItemCell>(cell).map_or(0.0, |c| c.0.charge);
            let mut rng = world.resource_mut::<CombatRng>();
            charge * f64::from(rng.range(0, 1000)) / 1000.0
        };
        if let Some(mut c) = world.get_mut::<ItemCell>(cell) {
            c.0.take(drained, true);
        }
        if let Some(mut w) = world.get_mut::<Wreckage>(wreck) {
            w.crowbar_salvage.push(cell);
        }
        if let Some(mut m) = world.get_mut::<Mech>(mech) {
            m.cell = None;
        }
    }
    let name = name_of(world, mech);
    visible(
        world,
        pos,
        Span::Danger,
        format!("{name} is destroyed!"),
        &[],
    );
    // `Destroy`: the parts still inside go with it.
    world.despawn(mech);
    crate::construction::tile_changed(world, pos);
}

/// `new wreckage(loc)`: two of its `parts` may join the welder salvage (60% skip each).
fn spawn_wreckage(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    let wreck = crate::construction::spawn_obj(world, path, pos)?;
    let def = world
        .resource::<MechaDb>()
        .wreckage(path)
        .map(|i| world.resource::<MechaDb>().data.wreckages[i].clone())
        .unwrap_or_default();
    let mut salvage = def.welder_salvage.clone();
    let mut parts = def.parts.clone();
    for _ in 0..2 {
        if parts.is_empty() {
            break;
        }
        let mut rng = world.resource_mut::<CombatRng>();
        if rng.chance(60.0) {
            continue;
        }
        let i = rng.range(0, parts.len() as u32 - 1) as usize;
        salvage.push(parts[i].clone());
    }
    parts.clear();
    world.entity_mut(wreck).insert(Wreckage {
        welder_salvage: salvage,
        salvage_num: def.salvage_num,
        crowbar_salvage: Vec::new(),
    });
    Some(wreck)
}

/// tg `/obj/structure/mecha_wreckage` `welder_act` and `crowbar_act`. True when the click was
/// the wreckage's.
pub fn wreckage_tool(
    world: &mut World,
    actor: Actor,
    kind: crate::tools::ToolKind,
    target: Entity,
) -> bool {
    let Some(wreck) = world.get::<Wreckage>(target).cloned() else {
        return false;
    };
    let Some(&user_pos) = world.get::<TilePos>(actor.mob) else {
        return true;
    };
    let name = name_of(world, target);
    match kind {
        crate::tools::ToolKind::Welder => {
            if wreck.salvage_num == 0 || wreck.welder_salvage.is_empty() {
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    "You don't see anything that can be cut with the welder!",
                );
                return true;
            }
            let Some(item) = actor.item else {
                return true;
            };
            if !crate::tools::tool_usable(world, item, 0, 0.0) {
                let text = crate::tools::unusable_message(world, item, 0);
                tell(world, actor.client, Span::Warning, text);
                return true;
            }
            let (fail, pick) = {
                let mut rng = world.resource_mut::<CombatRng>();
                (
                    rng.chance(30.0),
                    rng.range(0, wreck.welder_salvage.len() as u32 - 1) as usize,
                )
            };
            if fail {
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    format!("You fail to salvage anything valuable from {name}!"),
                );
                return true;
            }
            let path = wreck.welder_salvage[pick].clone();
            let stack = world
                .get_resource::<ss13_proto::Prototypes>()
                .and_then(|p| p.get(&path))
                .and_then(|p| p.get::<ss13_mapload::DmStack>())
                .is_some();
            let spawned = if stack {
                crate::construction::spawn_stack(world, &path, user_pos, 1)
            } else {
                crate::construction::spawn_obj(world, &path, user_pos)
            };
            if let Some(mut w) = world.get_mut::<Wreckage>(target) {
                if !stack {
                    w.welder_salvage.remove(pick);
                }
                w.salvage_num -= 1;
            }
            if let Some(item) = spawned {
                let thing = name_of(world, item);
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    format!("You cut {thing} from {name}."),
                );
            }
            true
        }
        crate::tools::ToolKind::Crowbar => {
            let Some(&part) = wreck.crowbar_salvage.first() else {
                tell(
                    world,
                    actor.client,
                    Span::Warning,
                    "You don't see anything that can be pried with the crowbar!",
                );
                return true;
            };
            take_out(world, part, user_pos);
            if let Some(mut w) = world.get_mut::<Wreckage>(target) {
                w.crowbar_salvage.retain(|&e| e != part);
            }
            let thing = name_of(world, part);
            tell(
                world,
                actor.client,
                Span::Notice,
                format!("You pry {thing} from {name}."),
            );
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ripley() -> MechDef {
        MechDef {
            max_integrity: 200.0,
            armor: ss13_mapload::mecha::Armor {
                melee: 40.0,
                laser: 10.0,
                ..default()
            },
            facing: [0.5, 1.0, 1.5],
            ..default()
        }
    }

    #[test]
    fn move_delay_follows_bandastation_speed() {
        // A Ripley at station pressure: 2 ds * 2.2 / 1.5.
        assert!((modified_move_delay(2.0) - 2.93).abs() < 1e-4);
        assert!((modified_move_delay(1.5) - 2.2).abs() < 1e-4);
        assert!((modified_move_delay(0.0) - 1.05).abs() < 1e-4);
    }

    #[test]
    fn armor_and_facing_modify_damage() {
        let def = ripley();
        // A 20 force hit through 40 melee armor: 12, from the front x0.5, side x1, back x1.5.
        let hit = |from| armor_damage(&def, 20.0, "melee", Dir::North, Some(from));
        assert!((hit(Dir::North) - 6.0).abs() < 1e-4);
        assert!((hit(Dir::NorthEast) - 6.0).abs() < 1e-4);
        assert!((hit(Dir::East) - 12.0).abs() < 1e-4);
        assert!((hit(Dir::South) - 18.0).abs() < 1e-4);
        // `abs(0 - 315)` is not 45: a hit from the north-west counts as a side hit.
        assert!((hit(Dir::NorthWest) - 12.0).abs() < 1e-4);
        assert!((armor_damage(&def, 30.0, "laser", Dir::North, None) - 27.0).abs() < 1e-4);
    }

    #[test]
    fn alerts_follow_charge_and_integrity() {
        let db = MechaDb::new(MechaData {
            mechs: vec![ripley()],
            ..default()
        });
        let mut m = Mech::new(0, &db);
        let cell = |charge| {
            ItemCell(Cell {
                charge,
                maxcharge: 100.0,
                chargerate: 0.0,
            })
        };
        assert_eq!(mech_alerts(&m, Some(&cell(80.0)), &db), (None, None));
        assert_eq!(
            mech_alerts(&m, Some(&cell(60.0)), &db).0,
            Some(("low_cell", 1))
        );
        assert_eq!(
            mech_alerts(&m, Some(&cell(0.5)), &db).0,
            Some(("empty_cell", 0))
        );
        assert_eq!(mech_alerts(&m, None, &db).0, Some(("no_cell", 0)));
        m.integrity = 80.0;
        assert_eq!(mech_alerts(&m, None, &db).1, Some(1));
        m.integrity = 50.0;
        assert_eq!(mech_alerts(&m, None, &db).1, Some(2));
        m.integrity = 20.0;
        assert_eq!(mech_alerts(&m, None, &db).1, Some(3));
    }

    #[test]
    fn english_lists() {
        assert_eq!(english_list(&["power cell"]), "power cell");
        assert_eq!(
            english_list(&["power cell", "capacitor", "servo"]),
            "power cell, capacitor and servo"
        );
    }

    /// A Ripley on a bare world: its parts inside, a charged cell.
    fn mech_world(charge: f64) -> (World, Entity) {
        let mut world = World::new();
        world.init_resource::<Messages<MoveRequest>>();
        world.init_resource::<Messages<SendChat>>();
        world.insert_resource(MechaDb::new(MechaData {
            mechs: vec![MechDef {
                movedelay: 2.0,
                step_energy_drain: 80.0,
                ..ripley()
            }],
            stock_parts: vec![(SERVO.to_owned(), 1), (CAPACITOR.to_owned(), 1)],
            ..default()
        }));
        let item = |world: &mut World, path: &str| {
            world
                .spawn(DmItem {
                    path: path.to_owned(),
                    ..default()
                })
                .id()
        };
        let servo = item(&mut world, SERVO);
        let capacitor = item(&mut world, CAPACITOR);
        let cell = world
            .spawn(ItemCell(Cell {
                charge,
                maxcharge: 10_000.0,
                chargerate: 0.0,
            }))
            .id();
        let mut m = Mech::new(0, world.resource::<MechaDb>());
        (m.cell, m.servo, m.capacitor) = (Some(cell), Some(servo), Some(capacitor));
        let mech = world
            .spawn((m, Facing(Dir::South), TilePos::new(5, 5, 1)))
            .id();
        update_energy_drain(&mut world, mech);
        (world, mech)
    }

    fn requests(world: &mut World) -> usize {
        world
            .resource_mut::<Messages<MoveRequest>>()
            .drain()
            .count()
    }

    #[test]
    fn steps_cost_energy_and_wait_for_the_cooldown() {
        let (mut world, mech) = mech_world(10_000.0);
        assert_eq!(
            world.get::<Mech>(mech).unwrap().step_energy_drain,
            80.0,
            "tier 1 servo"
        );
        // Facing south, a step east turns the mech (no `pivot_step`) and costs a step.
        assert!(vehicle_move(&mut world, mech, Dir::East, 0.0));
        assert_eq!(requests(&mut world), 0);
        assert_eq!(world.get::<Facing>(mech).unwrap().0, Dir::East);
        assert_eq!(charge(&world, mech), 9_920.0);
        // The cooldown: 2 ds * 2.2 / 1.5 = 0.293 s.
        assert!(!vehicle_move(&mut world, mech, Dir::East, 0.2));
        assert_eq!(charge(&world, mech), 9_920.0, "a refused attempt is free");
        assert!(vehicle_move(&mut world, mech, Dir::East, 0.3));
        assert_eq!(requests(&mut world), 1);
        assert_eq!(charge(&world, mech), 9_840.0);
        // Strafing keeps the facing for the step.
        world.get_mut::<Mech>(mech).unwrap().strafe = true;
        assert!(vehicle_move(&mut world, mech, Dir::North, 0.6));
        assert_eq!(requests(&mut world), 1);
        assert_eq!(
            world.get::<Mech>(mech).unwrap().strafe_facing,
            Some(Dir::East)
        );
    }

    #[test]
    fn no_power_or_parts_no_step() {
        let (mut world, mech) = mech_world(50.0);
        assert!(!vehicle_move(&mut world, mech, Dir::South, 0.0));
        assert_eq!(requests(&mut world), 0);
        assert_eq!(charge(&world, mech), 50.0, "all or nothing");
        let (mut world, mech) = mech_world(10_000.0);
        world.get_mut::<Mech>(mech).unwrap().servo = None;
        update_energy_drain(&mut world, mech);
        assert_eq!(world.get::<Mech>(mech).unwrap().step_energy_drain, 160.0);
        assert!(
            !vehicle_move(&mut world, mech, Dir::South, 0.0),
            "missing servo"
        );
        assert_eq!(charge(&world, mech), 10_000.0);
    }
}
