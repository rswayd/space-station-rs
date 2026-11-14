//! The abductor ship (`_maps/templates/lazy_templates/abductor_ships.dmm`) and its machines
//! (`machinery/console.dm`, `pad.dm`, `experiment.dm`, `dispenser.dm`).
//!
//! - **Console** ([`AbductorConsole`]): an empty hand of someone with abductor knowledge opens
//!   `AbductorConsole`; anyone else mashes alien buttons for 10 s and sends whoever stands on the
//!   pad. The shop spends the experiment machine's credits and drops the gear on the pad
//!   ("Incoming supply!", else "Insufficent data!"). The emergency teleport sends the pad's
//!   occupants, retrieves the science tool's mark; the vest settings flip its mode, lock it and
//!   pick a scanned disguise. A science tool or an agent vest used on it links to it.
//! - **Pad** ([`AbductorPad`]): `Send` moves every living mob on it to the drop point (unset: a
//!   random station area) stunned 6 s; `Retrieve` pulls the marked specimen onto it.
//! - **Experimentation machine** ([`Experiment`]): a human without abductor knowledge dragged
//!   onto it is shut in (not while an abductor stands there); `ProbingConsole` opens and closes
//!   it and runs the experiment ([`experiment`]): a living subject with a gland and a player
//!   becomes an abductee, its glands start, the team gets a point and a credit per gland, and it
//!   is sent back asleep (16 s) and uncuffed to the drop point or arrivals. A step from inside
//!   is "the door won't budge" (5 s cooldown).
//! - **Gland storage** ([`GlandDispenser`]): every gland type in a random order and colour,
//!   1-5 of each; `dispense` drops one, a gland used on it goes back to its slot.
// shortcut: no camera console (`camera_advanced/abductor`: no remote eye, so no remote
// teleport or drop point setting), no `flick` animations or teleport visuals, no 45 s breakout
// from the experiment machine, the experiment's 1 s "processing" pause is instant; the disguise
// is picked by cycling through the scans (no radial menu).

use std::path::Path;

use bevy::ecs::message::MessageCursor;
use ss13_assets::dmm::{DmValue, DmmMap};
use ss13_mapload::{DmName, DmObj, MapLoadReport, ShuttleTemplates};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::SpatialIndex;

use super::*;
use crate::antag::abductor::{
    AbductorConsoleData, AbductorConsoleStatic, AbductorConsoleUi, AbductorGearCategory,
    AbductorGearItem, BUY, CATEGORY, COMPACT_TOGGLE, DISPENSE, DOOR, EXPERIMENT, EXPERIMENT_TYPE,
    FLIP_VEST, GLAND_ID, GlandDispenserData, GlandDispenserUi, GlandSlot, NAME, ProbingConsoleData,
    ProbingConsoleUi, SELECT, SELECT_DISGUISE, TELEPORTER_RETRIEVE, TELEPORTER_SEND, TOGGLE_VEST,
};
use crate::health::Stat;
use crate::tools::{Actor, now, tell};

pub const SHIP_MAP: &str = "_maps/templates/lazy_templates/abductor_ships.dmm";
pub const CONSOLE: &str = "/obj/machinery/abductor/console";
pub const PAD: &str = "/obj/machinery/abductor/pad";
pub const EXPERIMENT_MACHINE: &str = "/obj/machinery/abductor/experiment";
pub const GLAND_DISPENSER: &str = "/obj/machinery/abductor/gland_dispenser";
const LANDMARK_AGENT: &str = "/obj/effect/landmark/abductor/agent";
const LANDMARK_SCIENTIST: &str = "/obj/effect/landmark/abductor/scientist";
/// `console/attack_hand`: `do_after(user, 100)`.
const MASH_DS: f32 = 100.0;
/// `pad/Send`: `target.Stun(60)`.
const SEND_STUN_SECS: f32 = 6.0;
/// `send_back`: `H.Sleeping(160)`.
const SEND_BACK_SLEEP_SECS: f32 = 16.0;
/// `relaymove`: `message_cooldown = world.time + 50`.
const RELAY_MESSAGE_SECS: f64 = 5.0;
const DING: &str = "sound/machines/ding.ogg";

/// `CATEGORY_BASIC_GEAR`, `CATEGORY_ADVANCED_GEAR`, `CATEGORY_MISC_GEAR`.
const BASIC: &str = "Basic Gear";
const ADVANCED: &str = "Advanced Gear";
const MISC: &str = "Miscellaneous Gear";

/// A `/datum/abductor_gear`.
#[derive(Clone, Copy, Debug)]
pub struct GearDef {
    pub name: &'static str,
    pub desc: &'static str,
    pub cost: u32,
    pub category: &'static str,
    /// `build_path`: (type, count).
    pub build: &'static [(&'static str, u32)],
}

/// `GLOB.abductor_gear` in `subtypesof` order.
pub const GEAR: [GearDef; 12] = [
    GearDef {
        name: "Agent Helmet",
        desc: "Abduct with style - spiky style. Prevents digital tracking.",
        cost: 1,
        category: BASIC,
        build: &[(gear::HELMET, 1)],
    },
    GearDef {
        name: "Agent Vest",
        desc: "A vest outfitted with advanced stealth technology. It has two modes - combat and \
               stealth.",
        cost: 1,
        category: BASIC,
        build: &[(gear::VEST, 1)],
    },
    GearDef {
        name: "Radio Silencer",
        desc: "A compact device used to shut down communications equipment.",
        cost: 1,
        category: BASIC,
        build: &[(gear::SILENCER, 1)],
    },
    GearDef {
        name: "Science Tool",
        desc: "A dual-mode tool for retrieving specimens and scanning appearances. Scanning can \
               be done through cameras.",
        cost: 1,
        category: BASIC,
        build: &[(gear::GIZMO, 1)],
    },
    GearDef {
        name: "Advanced Baton",
        desc: "A quad-mode baton used for incapacitation and restraining of specimens.",
        cost: 2,
        category: BASIC,
        build: &[(gear::BATON, 1)],
    },
    GearDef {
        name: "Superlingual Matrix",
        desc: "A mysterious structure that allows for instant communication between users. Using \
               it inhand will attune it to your mothership's channel. Pretty impressive until \
               you need to eat something.",
        cost: 1,
        category: MISC,
        build: &[("/obj/item/organ/tongue/abductor", 1)],
    },
    GearDef {
        name: "Mental Interface Device",
        desc: "A dual-mode tool for directly communicating with sentient brains. It can be used \
               to send a direct message to a target, or to send a command to a test subject with \
               a charged gland.",
        cost: 2,
        category: ADVANCED,
        build: &[(gear::MIND_DEVICE, 1)],
    },
    GearDef {
        name: "Reagent Synthesizer",
        desc: "Synthesizes a variety of reagents using proto-matter.",
        cost: 2,
        category: ADVANCED,
        build: &[("/obj/item/abductor_machine_beacon/chem_dispenser", 1)],
    },
    GearDef {
        name: "Shrink Ray Blaster",
        desc: "This is a piece of frightening alien tech that enhances the magnetic pull of \
               atoms in a localized space to temporarily make an object shrink. That or it's \
               just space magic. Either way, it shrinks stuff.",
        cost: 2,
        category: ADVANCED,
        build: &[("/obj/item/gun/energy/recharge/shrink_ray", 1)],
    },
    GearDef {
        name: "Alien Omnitool",
        desc: "A handheld device with an absurd number of integrated tools. Can be used as a \
               convenient tool replacement for either role. Right-click it to switch between \
               medical and hacking toolsets.",
        cost: 2,
        category: ADVANCED,
        build: &[("/obj/item/abductor/alien_omnitool", 1)],
    },
    GearDef {
        name: "Spare Cow",
        desc: "Delivers a leftover specimen from an earlier abduction operation.",
        cost: 1,
        category: MISC,
        build: &[
            ("/mob/living/basic/cow", 1),
            ("/obj/item/food/grown/wheat", 3),
        ],
    },
    GearDef {
        name: "Decorative Posters",
        desc: "Some posters, to decorate the walls of the Mothership (or even the station) with.",
        cost: 1,
        category: MISC,
        build: &[("/obj/item/poster/random_abductor", 2)],
    },
];

/// The categories in `get_abductor_gear`'s order (first appearance).
pub fn gear_categories() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for g in &GEAR {
        if !out.contains(&g.category) {
            out.push(g.category);
        }
    }
    out
}

/// A landmark of the ship (`/obj/effect/landmark/abductor/*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShipLandmark {
    pub team: u32,
    pub agent: bool,
    pub pos: TilePos,
}

/// The ship loaded with the station: its landmarks.
#[derive(Resource, Clone, Debug, Default)]
pub struct AbductorShip {
    pub z: Option<u16>,
    pub landmarks: Vec<ShipLandmark>,
}

#[derive(Resource, Clone)]
pub(super) struct ShipCandidate(pub PathBuf);

#[derive(Resource, Clone)]
struct ShipTemplate(PathBuf);

/// `/obj/machinery/abductor` `team_number`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TeamNumber(pub u32);

/// `/obj/machinery/abductor/console`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct AbductorConsole {
    pub gizmo: Option<Entity>,
    pub vest: Option<Entity>,
    /// `disguises`: the scanned names (tg keeps their icon snapshots).
    pub disguises: Vec<String>,
    pub selected: Option<String>,
    pub compact_mode: bool,
}

/// `/obj/machinery/abductor/pad`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct AbductorPad {
    /// `teleport_target`.
    pub teleport_target: Option<TilePos>,
}

/// `/obj/machinery/abductor/experiment`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Experiment {
    pub points: u32,
    pub credits: u32,
    /// `history`: the processed bodies.
    pub history: Vec<Entity>,
    /// `flash`.
    pub flash: String,
    /// `message_cooldown`.
    pub message_at: f64,
}

impl Default for Experiment {
    fn default() -> Self {
        Self {
            points: 0,
            credits: 0,
            history: Vec::new(),
            flash: "Ожидание субъекта.".to_owned(),
            message_at: 0.0,
        }
    }
}

/// `/obj/machinery/abductor/gland_dispenser`: (`gland_types`, `gland_colors`, `amounts`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct GlandDispenser {
    pub slots: Vec<(&'static str, String, u32)>,
}

// ---------------------------------------------------------------------------------------------
// Loading

/// The types the ship places that have no prototype.
fn missing_types(world: &World, ship: &Path) -> Vec<String> {
    let map = match DmmMap::from_file(ship) {
        Ok(map) => map,
        Err(e) => return vec![format!("{}: {e}", ship.display())],
    };
    let Some(protos) = world.get_resource::<ss13_proto::Prototypes>() else {
        return vec!["(no prototypes)".to_owned()];
    };
    let mut missing: Vec<String> = map
        .prefabs
        .iter()
        .flat_map(|p| p.objects.iter())
        .map(|o| o.path.clone())
        .filter(|p| protos.get(p).is_none())
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

pub(super) fn finish_ship(app: &mut App) {
    let Some(ship) = app
        .world()
        .get_resource::<ShipCandidate>()
        .map(|d| d.0.clone())
    else {
        return;
    };
    let missing = missing_types(app.world(), &ship);
    if !missing.is_empty() {
        warn!(
            "abductor ship {} not loaded: no prototypes for {missing:?} (rerun `ss13-mapload \
             convert`)",
            ship.display()
        );
        return;
    }
    info!("abductor ship template {}", ship.display());
    let world = app.world_mut();
    match world.get_resource_mut::<ShuttleTemplates>() {
        Some(mut templates) => templates.0.push(ship.clone()),
        None => {
            world.insert_resource(ShuttleTemplates(vec![ship.clone()]));
        }
    }
    app.insert_resource(ShipTemplate(ship));
}

/// `team_number` of a map object (landmarks default to 1, machines to 0).
fn team_var(vars: &[(String, DmValue)], default: u32) -> u32 {
    vars.iter()
        .find(|(k, _)| k == "team_number")
        .and_then(|(_, v)| match v {
            DmValue::Number(n) => Some(*n as u32),
            _ => None,
        })
        .unwrap_or(default)
}

/// The machines and landmarks of a parsed ship at level `first_z`: (landmarks, (machine type,
/// team, tile)).
pub fn scan_ship(map: &DmmMap, first_z: u16) -> (Vec<ShipLandmark>, Vec<(String, u32, TilePos)>) {
    let mut landmarks = Vec::new();
    let mut machines = Vec::new();
    let (sx, sy, sz) = map.size;
    for mz in 1..=sz {
        for y in 1..=sy {
            for x in 1..=sx {
                let Some(i) = map.index(x, y, mz).map(|i| i as usize) else {
                    continue;
                };
                let Some(prefab) = map.prefabs.get(i) else {
                    continue;
                };
                let pos = TilePos::new(x as i32, y as i32, first_z + (mz - 1) as u16);
                for o in &prefab.objects {
                    if o.path == LANDMARK_AGENT || o.path == LANDMARK_SCIENTIST {
                        landmarks.push(ShipLandmark {
                            team: team_var(&o.vars, 1),
                            agent: o.path == LANDMARK_AGENT,
                            pos,
                        });
                    } else if crate::obj_damage::is_under(&o.path, "/obj/machinery/abductor") {
                        machines.push((o.path.clone(), team_var(&o.vars, 0), pos));
                    }
                }
            }
        }
    }
    (landmarks, machines)
}

/// The ship's landmarks and its machines' team numbers (map var edits are not components).
fn find_ship(world: &mut World) {
    let Some(path) = world.get_resource::<ShipTemplate>().map(|d| d.0.clone()) else {
        return;
    };
    let Some(first_z) = world
        .get_resource::<MapLoadReport>()
        .and_then(|r| r.maps.iter().find(|m| m.path == path).map(|m| m.first_z))
    else {
        warn!("abductor ship {} was not loaded", path.display());
        return;
    };
    let Ok(map) = DmmMap::from_file(&path) else {
        return;
    };
    let (landmarks, machines) = scan_ship(&map, first_z);
    for (machine_path, team, pos) in machines {
        let found = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| {
                world
                    .get::<DmObj>(e)
                    .is_some_and(|o| o.path == machine_path)
            });
        if let Some(e) = found {
            world.entity_mut(e).insert(TeamNumber(team));
        }
    }
    info!("abductor ship at z{first_z}: {} landmarks", landmarks.len());
    world.insert_resource(AbductorShip {
        z: Some(first_z),
        landmarks,
    });
}

/// The machines get their state when they appear (map, spawn).
fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(o) = objs.get(add.entity) else {
        return;
    };
    let e = add.entity;
    let is = |root: &str| crate::obj_damage::is_under(&o.path, root);
    if is(CONSOLE) {
        commands
            .entity(e)
            .insert_if_new((AbductorConsole::default(), TeamNumber::default()));
    } else if is(PAD) {
        commands
            .entity(e)
            .insert_if_new((AbductorPad::default(), TeamNumber::default()));
    } else if is(EXPERIMENT_MACHINE) {
        commands.entity(e).insert_if_new((
            Experiment::default(),
            TeamNumber::default(),
            crate::medical::pods::Pod { open: true },
        ));
    } else if is(GLAND_DISPENSER) {
        commands.entity(e).insert_if_new(TeamNumber::default());
        commands.queue(move |world: &mut World| {
            if world.get::<GlandDispenser>(e).is_none() {
                let dispenser = with_rng(world, |_, rng| new_dispenser(rng));
                world.entity_mut(e).insert(dispenser);
            }
        });
    }
}

/// `gland_dispenser/Initialize`: the gland types shuffled, a random colour and 1-5 of each.
pub fn new_dispenser(rng: &mut Rng) -> GlandDispenser {
    let mut types: Vec<&'static str> = GLANDS.iter().map(|g| g.path).collect();
    for i in (1..types.len()).rev() {
        let j = rng.range(0, i as i64) as usize;
        types.swap(i, j);
    }
    GlandDispenser {
        slots: types
            .into_iter()
            .map(|t| {
                let color = format!(
                    "#{:02x}{:02x}{:02x}",
                    rng.range(0, 255),
                    rng.range(0, 255),
                    rng.range(0, 255)
                );
                (t, color, rng.range(1, 5) as u32)
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------------------------
// Lookups

fn find_by_team<C: Component>(world: &mut World, team: u32) -> Option<Entity> {
    if team == 0 {
        return None;
    }
    let mut q = world.query_filtered::<(Entity, &TeamNumber), With<C>>();
    q.iter(world).find(|(_, t)| t.0 == team).map(|(e, _)| e)
}

/// `get_abductor_console(team_number)`.
pub fn console_of(world: &mut World, team: u32) -> Option<Entity> {
    find_by_team::<AbductorConsole>(world, team)
}

pub fn pad_of(world: &mut World, team: u32) -> Option<Entity> {
    find_by_team::<AbductorPad>(world, team)
}

pub fn experiment_of(world: &mut World, team: u32) -> Option<Entity> {
    find_by_team::<Experiment>(world, team)
}

fn team_of(world: &World, e: Entity) -> u32 {
    world.get::<TeamNumber>(e).map_or(0, |t| t.0)
}

fn say(world: &mut World, machine: Entity, text: &str) {
    crate::atmos_machinery::machine_say(world, machine, text);
}

fn title_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// A living entity still in the world.
fn alive(world: &World, e: Entity) -> bool {
    world.get_entity(e).is_ok() && world.get::<Health>(e).is_some()
}

// ---------------------------------------------------------------------------------------------
// Pad

/// `pad/Send`: every living mob on the pad to the drop point (unset: a random station area),
/// stunned.
pub fn pad_send(world: &mut World, pad: Entity) {
    let Some(&pos) = world.get::<TilePos>(pad) else {
        return;
    };
    let target = match world
        .get::<AbductorPad>(pad)
        .and_then(|p| p.teleport_target)
    {
        Some(t) => Some(t),
        None => random_station_tile(world),
    };
    let Some(target) = target else {
        return;
    };
    let mobs: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| e != pad && world.get::<Health>(e).is_some())
        .collect();
    for mob in mobs {
        warp(world, mob, target);
        tell_mob(
            world,
            mob,
            Span::Warning,
            "Нестабильность варпа сбивает вас с толку!",
        );
        crate::status_effects::apply(
            world,
            mob,
            crate::status_effects::Incap::Stun,
            SEND_STUN_SECS,
        );
    }
}

/// `pad/Retrieve` (`Warp`): the target onto the pad unless buckled.
pub fn pad_retrieve(world: &mut World, pad: Entity, target: Entity) {
    let Some(&pos) = world.get::<TilePos>(pad) else {
        return;
    };
    if !alive(world, target)
        || world
            .get::<crate::medical::surgery::Buckled>(target)
            .is_some()
    {
        return;
    }
    warp(world, target, pos);
}

/// `forceMove`: no teleport restrictions apply to the alien pad.
fn warp(world: &mut World, mob: Entity, dest: TilePos) {
    crate::teleport::do_teleport(
        world,
        mob,
        dest,
        crate::teleport::Tele {
            forced: true,
            no_effects: true,
            ..default()
        },
    );
}

/// `GLOB.teleportlocs[pick(GLOB.teleportlocs)]`: a free tile of a random station area.
fn random_station_tile(world: &mut World) -> Option<TilePos> {
    let areas = crate::spells::teleport_areas(world);
    if areas.is_empty() {
        return None;
    }
    let i = with_rng(world, |_, rng| {
        rng.range(0, areas.len() as i64 - 1) as usize
    });
    let tiles = crate::spells::effects::area_tiles(world, areas[i].1);
    if tiles.is_empty() {
        return None;
    }
    let j = with_rng(world, |_, rng| {
        rng.range(0, tiles.len() as i64 - 1) as usize
    });
    Some(tiles[j])
}

// ---------------------------------------------------------------------------------------------
// Console

/// `TeleporterSend`.
pub fn teleporter_send(world: &mut World, console: Entity) {
    let team = team_of(world, console);
    if let Some(pad) = pad_of(world, team) {
        pad_send(world, pad);
    }
}

/// `TeleporterRetrieve`: the linked science tool's mark onto the pad.
pub fn teleporter_retrieve(world: &mut World, console: Entity) {
    let team = team_of(world, console);
    let marked = world
        .get::<AbductorConsole>(console)
        .and_then(|c| c.gizmo)
        .and_then(|g| world.get::<ScienceTool>(g))
        .and_then(|g| g.marked);
    if let (Some(pad), Some(marked)) = (pad_of(world, team), marked) {
        pad_retrieve(world, pad, marked);
    }
}

/// `Dispense(build_path, cost)`.
pub fn dispense_gear(world: &mut World, console: Entity, gear: &GearDef) -> bool {
    let team = team_of(world, console);
    let experiment = experiment_of(world, team);
    let affordable = experiment
        .and_then(|e| world.get::<Experiment>(e))
        .is_some_and(|e| e.credits >= gear.cost);
    let Some(experiment) = experiment.filter(|_| affordable) else {
        say(world, console, "Недостаточно данных!");
        return false;
    };
    if let Some(mut e) = world.get_mut::<Experiment>(experiment) {
        e.credits -= gear.cost;
    }
    say(world, console, "Входящая поставка!");
    let drop = pad_of(world, team)
        .or(Some(console))
        .and_then(|e| world.get::<TilePos>(e).copied());
    let Some(drop) = drop else {
        return true;
    };
    for &(path, count) in gear.build {
        for _ in 0..count {
            crate::construction::spawn_obj(world, path, drop);
        }
    }
    true
}

/// `AddGizmo`: false when already linked here.
pub fn add_gizmo(world: &mut World, console: Entity, gizmo: Entity) -> bool {
    let old = world.get::<ScienceTool>(gizmo).and_then(|g| g.console);
    if old == Some(console)
        && world.get::<AbductorConsole>(console).and_then(|c| c.gizmo) == Some(gizmo)
    {
        return false;
    }
    if let Some(old) = old
        && let Some(mut c) = world.get_mut::<AbductorConsole>(old)
        && c.gizmo == Some(gizmo)
    {
        c.gizmo = None;
    }
    if let Some(mut c) = world.get_mut::<AbductorConsole>(console) {
        c.gizmo = Some(gizmo);
    }
    if let Some(mut g) = world.get_mut::<ScienceTool>(gizmo) {
        g.console = Some(console);
    }
    true
}

/// `AddVest`: one console per vest.
pub fn add_vest(world: &mut World, console: Entity, vest: Entity) -> bool {
    if world.get::<AbductorConsole>(console).and_then(|c| c.vest) == Some(vest) {
        return false;
    }
    let mut q = world.query::<&mut AbductorConsole>();
    for mut c in q.iter_mut(world) {
        if c.vest == Some(vest) {
            c.vest = None;
        }
    }
    if let Some(mut c) = world.get_mut::<AbductorConsole>(console) {
        c.vest = Some(vest);
    }
    true
}

/// `AddSnapshot`: a scan of `target` (an existing name is replaced).
pub fn add_snapshot(world: &mut World, console: Entity, target: Entity) {
    let name = name_of(world, target);
    if let Some(mut c) = world.get_mut::<AbductorConsole>(console)
        && !c.disguises.contains(&name)
    {
        c.disguises.push(name);
    }
}

/// `SelectDisguise`: the next scan after the vest's current disguise.
fn select_disguise(world: &mut World, console: Entity) {
    let Some(c) = world.get::<AbductorConsole>(console).cloned() else {
        return;
    };
    let Some(vest) = c.vest.filter(|&v| world.get_entity(v).is_ok()) else {
        return;
    };
    if c.disguises.is_empty() {
        return;
    }
    let current = world
        .get::<AgentVest>(vest)
        .and_then(|v| v.disguise.clone());
    let next = current
        .and_then(|d| c.disguises.iter().position(|n| *n == d))
        .map_or(0, |i| (i + 1) % c.disguises.len());
    if let Some(mut v) = world.get_mut::<AgentVest>(vest) {
        v.disguise = Some(c.disguises[next].clone());
    }
}

/// The console's `attack_hand`: the window for those who know, the button mashing for others.
fn console_interact(world: &mut World, client: Entity, mob: Entity, console: Entity) -> bool {
    if world.get::<AbductorConsole>(console).is_none() {
        return false;
    }
    if has_knowledge(world, mob) {
        let title = title_of(world, console);
        world.write_message(OpenUi::new::<AbductorConsoleUi>(
            client, mob, console, title,
        ));
        return true;
    }
    tell(
        world,
        client,
        Span::Warning,
        "Вы начинаете наугад нажимать инопланетные кнопки!",
    );
    crate::tools::do_after(
        world,
        Actor {
            client,
            mob,
            item: None,
        },
        console,
        MASH_DS,
        0,
        crate::construction::Job::Abductor(AbductorJob::MashButtons),
    );
    true
}

/// A held science tool or agent vest links to the console; a gland goes back into storage.
fn machine_item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    item: Entity,
    target: Entity,
) -> bool {
    if world.get::<AbductorConsole>(target).is_some() {
        if world.get::<ScienceTool>(item).is_some() && add_gizmo(world, target, item) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы привязываете инструмент к консоли.",
            );
            return true;
        }
        if world.get::<AgentVest>(item).is_some() && add_vest(world, target, item) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы привязываете жилет к консоли.",
            );
            return true;
        }
        return false;
    }
    if world.get::<GlandDispenser>(target).is_some() {
        let Some(path) = world.get::<Gland>(item).map(|g| g.def().path) else {
            return false;
        };
        if crate::inventory::no_drop(world, item) {
            return true;
        }
        let Some(mut d) = world.get_mut::<GlandDispenser>(target) else {
            return true;
        };
        if let Some(slot) = d.slots.iter_mut().find(|(t, _, _)| *t == path) {
            slot.2 += 1;
        }
        // tg keeps the gland inside; a new one comes out on `dispense`.
        world.despawn(item);
        crate::inventory::refresh_look(world, mob);
        return true;
    }
    false
}

impl ServerUi for AbductorConsoleUi {
    type Source = &'static ConsoleView;
    type UserParam = ();
    type Act = ConsoleAct;

    fn static_data(_: &ConsoleView) -> AbductorConsoleStatic {
        AbductorConsoleStatic {
            categories: gear_categories()
                .into_iter()
                .map(|cat| AbductorGearCategory {
                    name: cat.to_owned(),
                    items: GEAR
                        .iter()
                        .filter(|g| g.category == cat)
                        .map(|g| AbductorGearItem {
                            name: g.name.to_owned(),
                            cost: g.cost,
                            desc: g.desc.to_owned(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn data(view: &ConsoleView, _: Entity, _: &()) -> AbductorConsoleData {
        view.0.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ConsoleAct, UiReject> {
        Ok(match action {
            BUY => {
                let name = params.get(NAME).unwrap_or_default();
                let i = GEAR
                    .iter()
                    .position(|g| g.name == name)
                    .ok_or_else(|| UiReject::BadParams("name: no such gear".into()))?;
                ConsoleAct::Buy(i)
            }
            TELEPORTER_SEND => ConsoleAct::Send,
            TELEPORTER_RETRIEVE => ConsoleAct::Retrieve,
            FLIP_VEST => ConsoleAct::FlipVest,
            TOGGLE_VEST => ConsoleAct::ToggleVest,
            SELECT_DISGUISE => ConsoleAct::SelectDisguise,
            SELECT => {
                let category = params.get(CATEGORY).unwrap_or_default();
                if !gear_categories().contains(&category) {
                    return Err(UiReject::BadParams("category: no such category".into()));
                }
                ConsoleAct::Select(category.to_owned())
            }
            COMPACT_TOGGLE => ConsoleAct::CompactToggle,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// A parsed console act.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsoleAct {
    /// An index into [`GEAR`].
    Buy(usize),
    Send,
    Retrieve,
    FlipVest,
    ToggleVest,
    SelectDisguise,
    Select(String),
    CompactToggle,
}

/// The console's `ui_data` with its machines (the window's own data cannot query them).
pub fn console_data(world: &mut World, console: Entity) -> AbductorConsoleData {
    let team = team_of(world, console);
    let Some(c) = world.get::<AbductorConsole>(console).cloned() else {
        return AbductorConsoleData::default();
    };
    let experiment = experiment_of(world, team).and_then(|e| world.get::<Experiment>(e).cloned());
    let pad = pad_of(world, team).is_some();
    let marked = c
        .gizmo
        .and_then(|g| world.get::<ScienceTool>(g))
        .and_then(|g| g.marked)
        .is_some_and(|m| world.get_entity(m).is_ok());
    let vest = c
        .vest
        .and_then(|v| world.get::<AgentVest>(v).map(|s| (v, s.mode)));
    AbductorConsoleData {
        compact_mode: c.compact_mode,
        selected: c.selected.clone(),
        experiment: experiment.is_some(),
        points: experiment.as_ref().map(|e| e.points),
        credits: experiment.as_ref().map(|e| e.credits),
        pad,
        gizmo: pad && marked,
        vest: vest.is_some(),
        vest_mode: vest.map(|(_, m)| m),
        vest_lock: vest.map(|(v, _)| crate::inventory::no_drop(world, v)),
    }
}

/// The console's `ui_data` ([`console_data`]: its machines are other entities), kept current
/// for its windows by [`refresh_views`].
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ConsoleView(pub AbductorConsoleData);

fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<AbductorConsoleUi>>>) {
    let pending: Vec<(Entity, Entity, ConsoleAct)> = acts
        .read(world.resource::<Messages<UiActed<AbductorConsoleUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, console, act) in pending {
        if world.get::<AbductorConsole>(console).is_none() || !has_knowledge(world, actor) {
            continue;
        }
        match act {
            ConsoleAct::Buy(i) => {
                dispense_gear(world, console, &GEAR[i]);
            }
            ConsoleAct::Send => teleporter_send(world, console),
            ConsoleAct::Retrieve => teleporter_retrieve(world, console),
            ConsoleAct::FlipVest => {
                if let Some(vest) = world.get::<AbductorConsole>(console).and_then(|c| c.vest) {
                    gear::flip_vest(world, vest);
                }
            }
            ConsoleAct::ToggleVest => {
                if let Some(vest) = world.get::<AbductorConsole>(console).and_then(|c| c.vest) {
                    gear::toggle_nodrop(world, vest);
                }
            }
            ConsoleAct::SelectDisguise => select_disguise(world, console),
            ConsoleAct::Select(category) => {
                if let Some(mut c) = world.get_mut::<AbductorConsole>(console) {
                    c.selected = Some(category);
                }
            }
            ConsoleAct::CompactToggle => {
                if let Some(mut c) = world.get_mut::<AbductorConsole>(console) {
                    c.compact_mode = !c.compact_mode;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Experimentation machine

fn is_open(world: &World, machine: Entity) -> bool {
    world
        .get::<crate::medical::pods::Pod>(machine)
        .is_some_and(|p| p.open)
}

fn experiment_look(world: &mut World, machine: Entity) {
    let state = if is_open(world, machine) {
        "experiment-open"
    } else {
        "experiment"
    };
    crate::obj_damage::set_icon_state(world, machine, state);
}

/// `open_machine`: not while closed with the panel open.
pub fn experiment_open(world: &mut World, machine: Entity) {
    if is_open(world, machine) || crate::medical::pods::panel_open(world, machine) {
        return;
    }
    crate::medical::pods::open_pod(world, machine);
    experiment_look(world, machine);
}

/// `close_machine(target)`: not while someone with abductor knowledge stands in it.
pub fn experiment_close(
    world: &mut World,
    machine: Entity,
    target: Option<Entity>,
) -> Option<Entity> {
    let &pos = world.get::<TilePos>(machine)?;
    let blocked = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .to_vec()
        .into_iter()
        .any(|e| world.get::<Health>(e).is_some() && has_knowledge(world, e));
    if blocked || !is_open(world, machine) || crate::medical::pods::panel_open(world, machine) {
        return None;
    }
    let mob = crate::medical::pods::close_pod(world, machine, target);
    experiment_look(world, machine);
    mob
}

/// `mouse_drop_receive` (from `crate::drag`): a human without abductor knowledge is shut in.
/// True when `machine` is an experimentation machine.
pub fn mouse_drop_receive(world: &mut World, machine: Entity, dragged: Entity) -> bool {
    if world.get::<Experiment>(machine).is_none() {
        return false;
    }
    if world.get::<crate::human::HumanLook>(dragged).is_some() && !has_knowledge(world, dragged) {
        experiment_close(world, machine, Some(dragged));
    }
    true
}

/// tg `stat` as the window's number.
fn stat_number(stat: Stat) -> u8 {
    match stat {
        Stat::Conscious => 0,
        Stat::SoftCrit => 1,
        Stat::HardCrit => 2,
        Stat::Dead => 3,
    }
}

/// `experiment(occupant, type, user)`: the machine's report.
pub fn experiment(
    world: &mut World,
    machine: Entity,
    occupant: Entity,
    kind: u8,
    user: Entity,
) -> String {
    if world.get::<crate::human::HumanLook>(occupant).is_none() {
        return "Не гуманоид!".to_owned();
    }
    let Some(team) = world.get::<Abductor>(user).map(|a| a.team) else {
        return "Ошибка авторизации. Немедленно свяжитесь с кораблём-маткой.".to_owned();
    };
    let history = world
        .get::<Experiment>(machine)
        .map(|e| e.history.clone())
        .unwrap_or_default();
    if history.contains(&occupant) {
        return "Образец уже в базе данных.".to_owned();
    }
    if world.get::<Stat>(occupant) == Some(&Stat::Dead) {
        say(world, machine, "Образец мёртв - предоставьте свежий.");
        return "Образец мёртв.".to_owned();
    }
    let glands = glands_of(world, occupant);
    if glands.is_empty() {
        say(world, machine, "Экспериментальное вскрытие не обнаружено!");
        return "Железы не обнаружены!".to_owned();
    }
    if world.get::<PlayerMob>(occupant).is_none() {
        say(
            world,
            machine,
            "Мозговая активность отсутствует - утилизация образца...",
        );
        experiment_open(world, machine);
        send_back(world, machine, occupant);
        return "Образец без мозговой активности - утилизирован.".to_owned();
    }
    if let Some(mut e) = world.get_mut::<Experiment>(machine) {
        e.history.push(occupant);
    }
    say(world, machine, "Обработка образца...");
    let feeling = match kind {
        1 => Some("Вы чувствуете себя осквернённым."),
        2 => Some("Вы чувствуете, как вас разрезают на части и собирают обратно."),
        3 => Some("Вы чувствуете, что за вами пристально наблюдают."),
        _ => None,
    };
    if let Some(feeling) = feeling {
        tell_mob(world, occupant, Span::Warning, feeling);
    }
    if let Some(t) = world.resource_mut::<AbductorTeams>().get_mut(team)
        && !t.abductees.contains(&occupant)
    {
        t.abductees.push(occupant);
    }
    add_abductee(world, occupant);
    let mut reward = 0;
    for gland in glands {
        glands::start(world, gland);
        reward += 1;
    }
    let pos = world.get::<TilePos>(machine).copied();
    experiment_open(world, machine);
    send_back(world, machine, occupant);
    if let Some(pos) = pos {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, DING, 50.0, true));
    }
    if let Some(mut e) = world.get_mut::<Experiment>(machine) {
        e.points += reward;
        e.credits += reward;
    }
    format!("Эксперимент успешен! Собрано новых данных: {reward}.")
}

/// `send_back`: asleep, uncuffed, to the pad's drop point or arrivals.
pub fn send_back(world: &mut World, machine: Entity, mob: Entity) {
    crate::status_effects::apply(
        world,
        mob,
        crate::status_effects::Incap::Sleeping,
        SEND_BACK_SLEEP_SECS,
    );
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::restraints::uncuff(world, mob, pos);
    }
    let team = team_of(world, machine);
    let target = pad_of(world, team)
        .and_then(|p| world.get::<AbductorPad>(p))
        .and_then(|p| p.teleport_target);
    let dest = match target {
        Some(t) => t,
        // `SSjob.send_to_late_join`.
        None => crate::server::pick_latejoin(world),
    };
    warp(world, mob, dest);
}

impl ServerUi for ProbingConsoleUi {
    type Source = &'static ProbingView;
    type UserParam = ();
    type Act = ProbeAct;

    fn static_data(_: &ProbingView) {}

    fn data(view: &ProbingView, _: Entity, _: &()) -> ProbingConsoleData {
        view.0.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ProbeAct, UiReject> {
        match action {
            DOOR => Ok(ProbeAct::Door),
            EXPERIMENT => params
                .get(EXPERIMENT_TYPE)
                .and_then(|t| t.parse::<u8>().ok())
                .map(ProbeAct::Experiment)
                .ok_or_else(|| UiReject::BadParams("experiment_type: expected a number".into())),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeAct {
    Door,
    Experiment(u8),
}

/// The experimentation machine's `ui_data`, kept current for its windows.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ProbingView(pub ProbingConsoleData);

fn probing_data(world: &mut World, machine: Entity) -> ProbingConsoleData {
    let occupant = crate::medical::pods::occupant(world, machine);
    ProbingConsoleData {
        open: is_open(world, machine),
        feedback: world
            .get::<Experiment>(machine)
            .map(|e| e.flash.clone())
            .unwrap_or_default(),
        occupant: occupant.is_some(),
        occupant_name: occupant.map(|o| name_of(world, o)),
        occupant_status: occupant
            .map(|o| stat_number(world.get::<Stat>(o).copied().unwrap_or_default())),
    }
}

fn probe_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ProbingConsoleUi>>>) {
    let pending: Vec<(Entity, Entity, ProbeAct)> = acts
        .read(world.resource::<Messages<UiActed<ProbingConsoleUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, machine, act) in pending {
        if world.get::<Experiment>(machine).is_none() {
            continue;
        }
        let occupant = crate::medical::pods::occupant(world, machine);
        if occupant == Some(actor) {
            continue;
        }
        match act {
            ProbeAct::Door => {
                if is_open(world, machine) {
                    experiment_close(world, machine, None);
                } else {
                    experiment_open(world, machine);
                }
            }
            ProbeAct::Experiment(kind) => {
                let Some(occupant) = occupant else {
                    continue;
                };
                if world.get::<Stat>(occupant) == Some(&Stat::Dead) {
                    continue;
                }
                let flash = experiment(world, machine, occupant, kind, actor);
                if let Some(mut e) = world.get_mut::<Experiment>(machine) {
                    e.flash = flash;
                }
            }
        }
    }
}

/// `relaymove`: the door won't budge.
fn relay_moves(
    world: &mut World,
    mut moves: Local<MessageCursor<crate::closets::ClosetRelayMove>>,
) {
    let mobs: Vec<Entity> = moves
        .read(world.resource::<Messages<crate::closets::ClosetRelayMove>>())
        .map(|m| m.mob)
        .collect();
    let t = now(world);
    for mob in mobs {
        let Some(machine) = world.get::<crate::closets::InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        if world.get::<Experiment>(machine).is_none()
            || !matches!(
                world.get::<Stat>(mob),
                Some(Stat::Conscious) | Some(Stat::SoftCrit)
            )
        {
            continue;
        }
        let Some(mut e) = world.get_mut::<Experiment>(machine) else {
            continue;
        };
        if t < e.message_at {
            continue;
        }
        e.message_at = t + RELAY_MESSAGE_SECS;
        let name = name_of(world, machine);
        tell_mob(
            world,
            mob,
            Span::Warning,
            format!("Дверь {name} не поддаётся!"),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Gland storage

impl ServerUi for GlandDispenserUi {
    type Source = &'static GlandDispenser;
    type UserParam = ();
    type Act = u32;

    fn static_data(_: &GlandDispenser) {}

    fn data(d: &GlandDispenser, _: Entity, _: &()) -> GlandDispenserData {
        GlandDispenserData {
            glands: d
                .slots
                .iter()
                .enumerate()
                .map(|(i, (_, color, amount))| GlandSlot {
                    id: i as u32 + 1,
                    color: color.clone(),
                    amount: *amount,
                })
                .collect(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<u32, UiReject> {
        if action != DISPENSE {
            return Err(UiReject::UnknownAction);
        }
        params
            .get(GLAND_ID)
            .and_then(|id| id.parse::<u32>().ok())
            .filter(|&id| id > 0)
            .ok_or_else(|| UiReject::BadParams("gland_id: expected a slot".into()))
    }
}

/// `Dispense(count)`: one gland of the slot onto the storage's tile.
pub fn dispense_gland(world: &mut World, dispenser: Entity, id: u32) -> Option<Entity> {
    let &pos = world.get::<TilePos>(dispenser)?;
    let path = {
        let mut d = world.get_mut::<GlandDispenser>(dispenser)?;
        let slot = d.slots.get_mut(id.checked_sub(1)? as usize)?;
        if slot.2 == 0 {
            return None;
        }
        slot.2 -= 1;
        slot.0
    };
    crate::construction::spawn_obj(world, path, pos)
}

fn dispenser_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<GlandDispenserUi>>>) {
    let pending: Vec<(Entity, Entity, u32)> = acts
        .read(world.resource::<Messages<UiActed<GlandDispenserUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, dispenser, id) in pending {
        if has_knowledge(world, actor) {
            dispense_gland(world, dispenser, id);
        }
    }
}

fn machine_interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if console_interact(world, client, mob, target) {
        return true;
    }
    if world.get::<Experiment>(target).is_some() {
        let title = title_of(world, target);
        world.write_message(OpenUi::new::<ProbingConsoleUi>(client, mob, target, title));
        return true;
    }
    if world.get::<GlandDispenser>(target).is_some() {
        if has_knowledge(world, mob) {
            let title = title_of(world, target);
            world.write_message(OpenUi::new::<GlandDispenserUi>(client, mob, target, title));
        }
        return true;
    }
    false
}

/// `ui_status`: the console and the storage close for those without abductor knowledge, the
/// experiment window for its occupant.
fn cap_windows(world: &mut World) {
    let mut q = world.query::<(Entity, &UiWindow)>();
    let windows: Vec<(Entity, Entity, &'static str, Entity)> = q
        .iter(world)
        .map(|(e, w)| (e, w.actor, w.interface, w.source))
        .collect();
    for (window, actor, interface, source) in windows {
        let shut = if interface == AbductorConsoleUi::NAME || interface == GlandDispenserUi::NAME {
            (world.get::<AbductorConsole>(source).is_some()
                || world.get::<GlandDispenser>(source).is_some())
                && !has_knowledge(world, actor)
        } else if interface == ProbingConsoleUi::NAME {
            world.get::<Experiment>(source).is_some()
                && world
                    .get::<crate::closets::InCloset>(actor)
                    .map(|c| c.closet)
                    == Some(source)
        } else {
            false
        };
        if shut && let Some(mut w) = world.get_mut::<UiWindow>(window) {
            w.cap_status(UiStatus::Close);
        }
    }
}

/// The console and probing windows' views, from their machines.
// Cost: per tick one pass over the handful of abductor machines.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<Entity, With<AbductorConsole>>();
    let consoles: Vec<Entity> = q.iter(world).collect();
    for console in consoles {
        let data = console_data(world, console);
        if world
            .get::<ConsoleView>(console)
            .is_none_or(|v| v.0 != data)
        {
            world.entity_mut(console).insert(ConsoleView(data));
        }
    }
    let mut q = world.query_filtered::<Entity, With<Experiment>>();
    let machines: Vec<Entity> = q.iter(world).collect();
    for machine in machines {
        let data = probing_data(world, machine);
        if world
            .get::<ProbingView>(machine)
            .is_none_or(|v| v.0 != data)
        {
            world.entity_mut(machine).insert(ProbingView(data));
        }
    }
}

pub(super) fn build_ship(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // The console, experimentation machine and gland storage windows; the console's mashing.
    app.add_click_handler(ClickPhase::EmptyHand, 2683, |w, c| {
        machine_interact(w, c.client, c.mob, c.target)
    });
    // A science tool or vest links to the console, a gland goes back into storage.
    app.add_click_handler(ClickPhase::HeldOnTarget, 1803, |w, c| {
        c.held
            .is_some_and(|held| machine_item_interaction(w, c.client, c.mob, held, c.target))
    });
    app.register_ui::<AbductorConsoleUi>()
        .register_ui::<ProbingConsoleUi>()
        .register_ui::<GlandDispenserUi>()
        .add_observer(attach)
        .add_systems(ss13_world::MapInit, find_ship)
        .add_systems(
            Update,
            (
                refresh_views.before(UiSystems::Send),
                relay_moves.after(ss13_world::WorldSystems::Movement),
                cap_windows.in_set(UiSystems::StatusHook),
                (console_acts, probe_acts, dispenser_acts).in_set(UiSystems::Handle),
            ),
        );
}

/// The `do_after`s of the abductors (`crate::construction::Job::Abductor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbductorJob {
    /// Mashing the console's buttons: the pad sends.
    MashButtons,
    /// The science tool prepares its target for transport.
    PrepareMark,
    /// The advanced baton shapes energy cuffs.
    Cuff,
}

/// The end of an abductor `do_after`.
pub fn complete_abductor_job(
    world: &mut World,
    mob: Entity,
    action: &crate::tools::DoAfter,
    job: AbductorJob,
) {
    match job {
        AbductorJob::MashButtons => {
            if world.get::<AbductorConsole>(action.target).is_some() {
                teleporter_send(world, action.target);
            }
        }
        AbductorJob::PrepareMark => gear::finish_prepare(world, mob, action),
        AbductorJob::Cuff => gear::finish_cuff(world, mob, action),
    }
}
