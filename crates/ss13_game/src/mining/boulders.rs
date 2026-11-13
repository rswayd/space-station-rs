//! Boulders and the boulder machines (server), after tg `/obj/item/boulder`
//! (`code/modules/mining/boulder_processing/boulder.dm`, `boulder_types.dm`), the boulder
//! retrieval matrix (`brm.dm`), the bouldertech refinery and smelter (`_boulder_processing.dm`,
//! `refinery.dm`) and the boulder beacon (`beacon.dm`).
//!
//! - A boulder ([`Boulder`]) holds materials (`custom_materials`), a `durability` and its vent's
//!   size. A mining tool on one chips at it, `2 × toolspeed` seconds a step (4 stamina each);
//!   at durability 0 it crumbles into ore (`convert_to_ore`): per material
//!   `clamp(floor((units - 100) / 100), 1, 10)` ore, an artifact boulder's relic too.
//!   Artifact boulders carry a strange object and 1-5 sheets of bluespace or diamond; gulag
//!   boulders one of gold, iron, plasma or silver; the shabby one a little iron and glass.
//! - The BRM teleports a boulder lying on a tapped vent ([`super::vents::OreGeneration`]) onto
//!   its own tile: a click every 1.5 s, or with right click toggled on, a batch of
//!   `ceil(parts / 12 × 7)` every 3 s (1.5 s apart). A boulder already on its tile blocks it.
//! - The refinery takes boulders (by hand, or riding a conveyor through it along its axis) up to
//!   its matter bins' tiers, and every machine tick wears `ceil(servos / 8 × bins)` of them down
//!   a step. Worn through (`breakdown_boulder`), the materials it refines (glass, plasma,
//!   diamond, bluespace, bananium, plastic; the smelter: iron, titanium, silver, gold, uranium)
//!   go into the station's ore silo (sheets on its tile without one) and add their mining
//!   points; the rest stays in the boulder, which is put back out with its durability rolled
//!   lower. An emptied boulder breaks apart (an artifact adds 100 points). An ID card claims the
//!   held points (`tgui_input_number`); right click takes a boulder out.
//! - A boulder beacon in hand drops a BRM, then a refinery, then a smelter, a second after each
//!   use.
// shortcut: no chemical boosts (`check_for_boosts`, the plumbing reagents and the plunger), no
// golem processing, no wrench anchoring (the machines stay as mapped or built), no lava
// platforms (`create_platform`), no two-handed grip or slowdown, boulders do not block each other
// on belts, no icon state changes for power or panel, no energy use; add each with its mechanic.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, DmObj, DmTool};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind};
use ss13_world::{Moved, SpatialIndex};

use super::rocks::MiningJob;
use super::vents::{MINER_POINT_MULTIPLIER, OreGeneration, SIZE_LARGE, SIZE_MEDIUM, SIZE_SMALL};
use crate::combat::{CombatMode, CombatRng};
use crate::construction::{Job, PanelOpen, path_of, spawn_obj, spawn_stack};
use crate::health::DEFAULT_MESSAGE_RANGE;
use crate::inventory::{CarriedBy, IdCard, Inventory};
use crate::power::Unpowered;
use crate::science::lathe::MaterialStore;
use crate::science::techweb::ResearchDb;
use crate::sound::{PlaySound, playsound};
use crate::stock_parts::PartKind;
use crate::storage::{Stored, StoredIn, put_inside, take_out};
use crate::tools::{Actor, DoAfter, do_after, now, tell};

pub const BOULDER: &str = "/obj/item/boulder";
const ARTIFACT: &str = "/obj/item/boulder/artifact";
const ARTIFACT_BLUESPACE: &str = "/obj/item/boulder/artifact/bluespace";
const ARTIFACT_DIAMOND: &str = "/obj/item/boulder/artifact/diamond";
const GULAG_VENT_BOULDER: &str = "/obj/item/boulder/gulag_vent";
const GULAG_BOULDER: &str = "/obj/item/boulder/gulag";
const SHABBY_BOULDER: &str = "/obj/item/boulder/shabby";
/// `/obj/item/boulder/artifact::artifact_type`.
const RELIC: &str = "/obj/item/assembly/relic/lavaland";
pub const BRM: &str = "/obj/machinery/brm";
pub const BOULDERTECH: &str = "/obj/machinery/bouldertech";
const REFINERY: &str = "/obj/machinery/bouldertech/refinery";
const SMELTER: &str = "/obj/machinery/bouldertech/refinery/smelter";
const BOULDER_BEACON: &str = "/obj/item/boulder_beacon";

/// `SHEET_MATERIAL_AMOUNT`.
const SHEET: f64 = ss13_mapload::research::SHEET_MATERIAL_AMOUNT;
/// `INNATE_BOULDER_SPEED_MULTIPLIER` is for golems; a tool's step is `2 × toolspeed` seconds.
const STEP_DS: f32 = 20.0;
/// `apply_damage(4 * skill_modifier, STAMINA)` per step.
const STEP_STAMINA: f32 = 4.0;
/// `MINING_SKILL_BOULDER_SIZE_XP * 0.2`.
const CRUSH_XP: f32 = 2.0;
/// The machines' `process()` (`SSmachines`).
const MACHINE_TICK: f64 = 2.0;
/// `accept_cooldown` after taking a boulder.
const ACCEPT_COOLDOWN: f64 = 1.5;
/// `sound_cooldown` of the no-points buzz.
const SOUND_COOLDOWN: f64 = 1.5;
/// BRM `TELEPORTATION_TIME` and `BATCH_COOLDOWN`.
const TELEPORT_TIME: f64 = 1.5;
const BATCH_COOLDOWN: f64 = 3.0;
const MANUAL_TELEPORT_SOUND: &str = "sound/machines/mining/manual_teleport.ogg";
const AUTO_TELEPORT_SOUND: &str = "sound/machines/mining/auto_teleport.ogg";
/// The beacon's `addtimer(..., 1 SECONDS)`.
const BEACON_DELAY: f64 = 1.0;

/// Ore and mining points of the vent materials: `ore_type` and `points_per_boulder_unit` per
/// sheet.
const MATERIALS: [(&str, &str, f64); 11] = [
    ("/datum/material/iron", "/obj/item/stack/ore/iron", 1.0),
    (
        "/datum/material/glass",
        "/obj/item/stack/ore/glass/basalt",
        1.0,
    ),
    ("/datum/material/silver", "/obj/item/stack/ore/silver", 3.5),
    ("/datum/material/gold", "/obj/item/stack/ore/gold", 4.0),
    (
        "/datum/material/diamond",
        "/obj/item/stack/ore/diamond",
        10.0,
    ),
    (
        "/datum/material/uranium",
        "/obj/item/stack/ore/uranium",
        3.5,
    ),
    ("/datum/material/plasma", "/obj/item/stack/ore/plasma", 2.0),
    (
        "/datum/material/bluespace",
        "/obj/item/stack/ore/bluespace_crystal",
        10.0,
    ),
    (
        "/datum/material/bananium",
        "/obj/item/stack/ore/bananium",
        15.0,
    ),
    (
        "/datum/material/titanium",
        "/obj/item/stack/ore/titanium",
        3.0,
    ),
    ("/datum/material/plastic", "/obj/item/stack/ore/slag", 4.0),
];

/// `/obj/machinery/bouldertech/refinery/can_process_material`.
const REFINERY_MATERIALS: [&str; 6] = [
    "/datum/material/glass",
    "/datum/material/plasma",
    "/datum/material/diamond",
    "/datum/material/bluespace",
    "/datum/material/bananium",
    "/datum/material/plastic",
];
/// `/obj/machinery/bouldertech/refinery/smelter/can_process_material`.
const SMELTER_MATERIALS: [&str; 5] = [
    "/datum/material/iron",
    "/datum/material/titanium",
    "/datum/material/silver",
    "/datum/material/gold",
    "/datum/material/uranium",
];

/// A boulder's live state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Boulder {
    /// `custom_materials`: material type and units.
    pub materials: Vec<(String, f64)>,
    pub durability: u32,
    /// `boulder_size`.
    pub size: u32,
    /// `boulder_string`: the vent's boulder look.
    pub boulder_string: &'static str,
    /// `artifact_inside`: the relic an artifact boulder holds.
    pub artifact: Option<&'static str>,
    /// `processed_by`: the machine that last broke it down.
    pub processed_by: Option<Entity>,
}

impl Default for Boulder {
    fn default() -> Self {
        Self {
            materials: Vec::new(),
            durability: 5,
            size: SIZE_SMALL,
            boulder_string: "boulder",
            artifact: None,
            processed_by: None,
        }
    }
}

impl Boulder {
    /// `custom_materials[material] += units`.
    pub fn add_material(&mut self, material: &str, units: f64) {
        match self.materials.iter_mut().find(|(m, _)| m == material) {
            Some((_, n)) => *n += units,
            None => self.materials.push((material.to_owned(), units)),
        }
    }

    /// The `Initialize` of a boulder type.
    pub fn of_type(path: &str, rng: &mut CombatRng) -> Self {
        let mut b = Self::default();
        if is_under(path, SHABBY_BOULDER) {
            b.materials = vec![
                ("/datum/material/iron".into(), SHEET * 1.1),
                ("/datum/material/glass".into(), SHEET * 1.1),
            ];
            b.durability = 1;
        } else if is_under(path, GULAG_VENT_BOULDER) {
            b.materials = vec![("/datum/material/iron".into(), SHEET * 16.0)];
        } else if is_under(path, GULAG_BOULDER) {
            // `gulag_minerals`: gold 1, iron 5, plasma 4, silver 2.
            let roll = rng.range(1, 12);
            let material = match roll {
                1 => "/datum/material/gold",
                2..=6 => "/datum/material/iron",
                7..=10 => "/datum/material/plasma",
                _ => "/datum/material/silver",
            };
            let amount = f64::from(rng.range(10, 13));
            b.materials = vec![(material.into(), SHEET * amount)];
        } else if is_under(path, ARTIFACT) {
            b.artifact = Some(RELIC);
            let bonus = if is_under(path, ARTIFACT_BLUESPACE) {
                Some("/datum/material/bluespace")
            } else if is_under(path, ARTIFACT_DIAMOND) {
                Some("/datum/material/diamond")
            } else {
                None
            };
            if let Some(material) = bonus {
                let sheets = f64::from(rng.range(1, 5));
                b.add_material(material, sheets * SHEET);
            }
        }
        b
    }
}

/// `/obj/machinery/bouldertech`: the refinery or the smelter.
#[derive(Component, Clone, Debug, Default)]
pub struct Bouldertech {
    pub smelter: bool,
    /// `points_held`.
    pub points: i64,
    /// The ore silo it fills (`silo_materials`).
    pub silo: Option<Entity>,
    accept_ready: f64,
    sound_ready: f64,
}

/// `/obj/machinery/brm`.
#[derive(Component, Clone, Debug, Default)]
pub struct Brm {
    pub toggled_on: bool,
    batch_processing: bool,
    manual_ready: f64,
    batch_ready: f64,
    /// The next teleport of a batch: due time, boulders left, feedback.
    next: Option<(f64, u32, bool)>,
}

/// A boulder beacon's uses (`uses`, 3) and its pending launches.
#[derive(Component, Clone, Debug)]
pub struct BoulderBeacon {
    pub uses: u32,
    launches: Vec<f64>,
}

/// The `tgui_input_number` of a points claim.
#[derive(Component, Clone, Copy, Debug)]
struct ClaimPrompt {
    machine: Entity,
    card: Entity,
}

pub(super) struct BouldersPlugin;

impl Plugin for BouldersPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Right click takes a boulder out of a refinery or toggles a BRM; a BRM teleports one.
        app.add_click_handler(ClickPhase::EmptyHand, 2903, |w, c| {
            attack_hand(w, c.mob, c.target, c.modifiers.right)
        });
        // A boulder beacon calls down its next machine.
        app.add_click_handler(ClickPhase::AttackSelf, 3193, |w, c| {
            beacon_attack_self(w, c.mob, c.target)
        });
        app.add_systems(
            ss13_world::MapInit,
            setup_mapped.after(crate::science::lathe::LatheSetup),
        )
        .add_systems(
            Update,
            (
                boulders_enter.after(ss13_world::WorldSystems::Movement),
                machine_process.run_if(bevy::time::common_conditions::on_timer(
                    std::time::Duration::from_secs_f64(MACHINE_TICK),
                )),
                brm_batches.run_if(any_with_component::<Brm>),
                beacon_launches.run_if(any_with_component::<BoulderBeacon>),
                claim_answers
                    .in_set(ss13_tgui::server::UiSystems::Handle)
                    .after(ss13_tgui::prompt::PromptSystems::Answer),
            ),
        )
        .add_observer(boulder_added);
    }
}

/// A new boulder of `path` on `pos` with its type's materials.
pub fn new_boulder(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    let boulder = spawn_obj(world, path, pos)?;
    let state = Boulder::of_type(path, &mut world.resource_mut::<CombatRng>());
    world.entity_mut(boulder).insert(state);
    update_icon(world, boulder);
    Some(boulder)
}

/// Boulders spawned some other way (maps, gulag) get their type's state.
fn boulder_added(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let boulder = add.entity;
    let Some(path) = items
        .get(boulder)
        .ok()
        .filter(|i| is_under(&i.path, BOULDER))
        .map(|i| i.path.clone())
    else {
        return;
    };
    commands.queue(move |world: &mut World| {
        if world.get_entity(boulder).is_err() || world.get::<Boulder>(boulder).is_some() {
            return;
        }
        let state = Boulder::of_type(&path, &mut world.resource_mut::<CombatRng>());
        world.entity_mut(boulder).insert(state);
        update_icon(world, boulder);
    });
}

/// `update_icon_state`: `[boulder_string]_[small|medium|large]`; artifacts keep theirs.
pub fn update_icon(world: &mut World, boulder: Entity) {
    let Some(b) = world.get::<Boulder>(boulder) else {
        return;
    };
    if b.artifact.is_some() {
        return;
    }
    let size = match b.size {
        SIZE_MEDIUM => "medium",
        SIZE_LARGE => "large",
        _ => "small",
    };
    let state = format!("{}_{size}", b.boulder_string);
    crate::obj_damage::set_icon_state(world, boulder, &state);
}

fn quantity_of_ore(units: f64) -> u32 {
    ((units - SHEET) / SHEET).floor().clamp(1.0, 10.0) as u32
}

/// `convert_to_ore`: ore stacks of the boulder's materials onto its tile, the relic too.
pub fn convert_to_ore(world: &mut World, boulder: Entity) {
    let Some(state) = world.get::<Boulder>(boulder).cloned() else {
        return;
    };
    let Some(pos) = crate::explosions::turf_of(world, boulder) else {
        return;
    };
    for (material, units) in &state.materials {
        let Some(&(_, ore, _)) = MATERIALS.iter().find(|(m, ..)| m == material) else {
            warn!("boulder found containing material type {material} with no set ore_type");
            continue;
        };
        spawn_stack(world, ore, pos, quantity_of_ore(*units));
    }
    if let Some(relic) = state.artifact {
        spawn_obj(world, relic, pos);
    }
}

/// `break_apart`: what it holds falls out, then it is gone.
fn break_apart(world: &mut World, boulder: Entity) {
    let artifact = world.get::<Boulder>(boulder).and_then(|b| b.artifact);
    if let Some(relic) = artifact
        && let Some(pos) = crate::explosions::turf_of(world, boulder)
    {
        let quip = {
            let quips = ["Clang!", "Crack!", "Bang!", "Clunk!", "Clank!"];
            let i = world.resource_mut::<CombatRng>().range(0, 4) as usize;
            quips[i]
        };
        let name = crate::security::name_of(world, boulder);
        crate::security::show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Notice,
                format!("{quip} Something falls out of the {name}!"),
            ),
            &[],
        );
        playsound(
            world,
            PlaySound::new(pos, "sound/effects/pickaxe/picaxe1.ogg", 60.0, false),
        );
        spawn_obj(world, relic, pos);
    }
    world.despawn(boulder);
}

fn is_mining_tool(world: &World, item: Entity) -> bool {
    world
        .get::<DmTool>(item)
        .is_some_and(|t| t.behaviour == "mining")
}

/// tg `item_interaction` of boulders and the boulder machines: a mining tool on a boulder, a
/// boulder or an ID card on a refinery. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Boulder>(target).is_some() {
        if !is_mining_tool(world, held) {
            return false;
        }
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        manual_process(world, actor, target, false);
        return true;
    }
    if world.get::<Bouldertech>(target).is_none() {
        return false;
    }
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    if panel_open(world, target) || combat {
        return false;
    }
    if world.get::<Boulder>(held).is_some() {
        let text = if accept_boulder(world, target, held, Some(mob)) {
            "accepted"
        } else {
            "cannot accept!"
        };
        crate::balloon::balloon_alert_to_viewers(world, target, text, None, &[]);
        return true;
    }
    if world.get::<IdCard>(held).is_some() {
        claim_points(world, client, mob, target, held);
        return true;
    }
    false
}

/// `manual_process`: a step of `2 × toolspeed` seconds, or done at durability 0.
fn manual_process(world: &mut World, actor: Actor, boulder: Entity, continued: bool) {
    if world.get::<DoAfter>(actor.mob).is_some() {
        return;
    }
    let durability = world.get::<Boulder>(boulder).map_or(0, |b| b.durability);
    let name = crate::security::name_of(world, boulder);
    if !continued {
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("You swing at the {name}..."),
        );
    }
    if durability == 0 {
        finish(world, actor, boulder);
        return;
    }
    let speed = actor
        .item
        .and_then(|i| world.get::<DmTool>(i))
        .map_or(1.0, |t| t.speed);
    let skill = super::rocks::mining_speed(world, actor.mob);
    do_after(
        world,
        actor,
        boulder,
        STEP_DS * speed * skill,
        0,
        Job::Mining(MiningJob::CrushBoulder),
    );
}

/// The end of a step: durability down, crumbled at 0, else the next step.
pub fn complete_crush(world: &mut World, mob: Entity, action: &DoAfter) {
    let boulder = action.target;
    if world.get::<Boulder>(boulder).is_none() {
        return;
    }
    let near = match (
        world.get::<TilePos>(mob),
        crate::explosions::turf_of(world, boulder),
    ) {
        (Some(a), Some(b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
        _ => false,
    };
    if !near {
        return;
    }
    let left = {
        let mut b = world.get_mut::<Boulder>(boulder).expect("checked");
        b.durability = b.durability.saturating_sub(1);
        b.durability
    };
    let skill = super::rocks::mining_speed(world, mob);
    crate::status_effects::adjust_stamina(world, mob, STEP_STAMINA * skill);
    let actor = Actor {
        client: action.client,
        mob,
        item: action.item,
    };
    if left == 0 {
        finish(world, actor, boulder);
        return;
    }
    let name = crate::security::name_of(world, boulder);
    let msg = if left == 1 {
        "is crumbling!"
    } else {
        "looks weaker!"
    };
    tell(
        world,
        action.client,
        Span::Notice,
        format!("The {name} {msg}"),
    );
    manual_process(world, actor, boulder, true);
}

fn finish(world: &mut World, actor: Actor, boulder: Entity) {
    let name = crate::security::name_of(world, boulder);
    let pos = crate::explosions::turf_of(world, boulder);
    convert_to_ore(world, boulder);
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("You finish working on the {name}, and it crumbles into ore."),
    );
    if let Some(pos) = pos {
        playsound(
            world,
            PlaySound::new(pos, "sound/effects/rock/rock_break.ogg", 50.0, true),
        );
    }
    crate::skills::adjust_experience(
        world,
        actor.mob,
        crate::skills::Skill::Mining,
        CRUSH_XP,
        false,
    );
    crate::skills::adjust_experience(
        world,
        actor.mob,
        crate::skills::Skill::Athletics,
        CRUSH_XP,
        false,
    );
    world.despawn(boulder);
}

fn panel_open(world: &World, e: Entity) -> bool {
    world.get::<PanelOpen>(e).is_some_and(|p| p.0)
}

/// `anchored && !panel_open && is_operational`.
fn working(world: &World, machine: Entity) -> bool {
    crate::obj_damage::anchored(world, machine)
        && !panel_open(world, machine)
        && world.get::<Unpowered>(machine).is_none()
}

fn machine_dir(world: &World, machine: Entity) -> Dir {
    world
        .get::<Facing>(machine)
        .map(|f| f.0)
        .or_else(|| {
            world
                .get::<DmAtom>(machine)
                .and_then(|a| Dir::from_bits(a.dir))
        })
        .unwrap_or(Dir::South)
}

fn held_boulders(world: &World, machine: Entity) -> Vec<Entity> {
    world.get::<Stored>(machine).map_or_else(Vec::new, |s| {
        s.items()
            .iter()
            .copied()
            .filter(|&e| world.get::<Boulder>(e).is_some())
            .collect()
    })
}

/// `boulders_held_max` (the matter bins' tiers) and `boulders_processing_count`
/// (`ceil(servo tiers / 8 × held_max)`), from `RefreshParts`.
fn capacity(world: &World, machine: Entity) -> (u32, u32) {
    let ratings = crate::stock_parts::ratings(world, machine);
    let held = ratings.sum(PartKind::MatterBin);
    let servos = ratings.sum(PartKind::Servo);
    let processing = (f64::from(servos) / 8.0 * f64::from(held)).ceil() as u32;
    (held, processing)
}

/// `can_process_boulder`.
fn can_process_boulder(world: &World, machine: Entity, boulder: Entity) -> bool {
    let Some(tech) = world.get::<Bouldertech>(machine) else {
        return false;
    };
    if !working(world, machine) {
        return false;
    }
    let Some(b) = world.get::<Boulder>(boulder) else {
        return false;
    };
    if b.processed_by.is_some() {
        return false;
    }
    let (held_max, _) = capacity(world, machine);
    if held_boulders(world, machine).len() as u32 >= held_max {
        return false;
    }
    now(world) >= tech.accept_ready
}

/// `accept_boulder`: in it goes (out of `holder`'s hands when held).
fn accept_boulder(
    world: &mut World,
    machine: Entity,
    boulder: Entity,
    holder: Option<Entity>,
) -> bool {
    if !can_process_boulder(world, machine, boulder) {
        return false;
    }
    if let Some(mob) = holder
        && let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(boulder))
    {
        *slot = None;
    }
    world.entity_mut(boulder).remove::<CarriedBy>();
    put_inside(world, boulder, machine);
    let t = now(world);
    if let Some(mut tech) = world.get_mut::<Bouldertech>(machine) {
        tech.accept_ready = t + ACCEPT_COOLDOWN;
    }
    true
}

/// tg `CanAllowThrough` of the boulder machines for a belt step of `mover` into `blocker`'s tile
/// along `dir`: a BRM lets boulders by, a refinery or smelter boulders it can take and sheets
/// along its axis.
pub fn lets_through(world: &World, mover: Entity, blocker: Entity, dir: Dir) -> bool {
    if !crate::obj_damage::anchored(world, blocker) {
        return false;
    }
    if world.get::<Brm>(blocker).is_some() {
        return world.get::<Boulder>(mover).is_some();
    }
    if world.get::<Bouldertech>(blocker).is_none() {
        return false;
    }
    let axis = machine_dir(world, blocker);
    if axis != dir && axis != dir.reverse() {
        return false;
    }
    if path_of(world, mover).is_some_and(|p| is_under(&p, "/obj/item/stack/sheet")) {
        return true;
    }
    world.get::<Boulder>(mover).is_some() && can_process_boulder(world, blocker, mover)
}

/// `on_entered`: a boulder arriving on a refinery's tile goes in.
fn boulders_enter(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moves: Vec<(Entity, TilePos)> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| (m.entity, m.to))
        .collect();
    for (boulder, to) in moves {
        if world.get::<Boulder>(boulder).is_none() {
            continue;
        }
        let machine = world
            .resource::<SpatialIndex>()
            .on_tile(to)
            .iter()
            .copied()
            .find(|&e| world.get::<Bouldertech>(e).is_some());
        if let Some(machine) = machine {
            accept_boulder(world, machine, boulder, None);
        }
    }
}

/// `breakdown_boulder`: what the machine refines into the silo (or sheets), the rest stays.
fn breakdown_boulder(world: &mut World, machine: Entity, boulder: Entity) {
    if world.get::<StoredIn>(boulder).map(|s| s.0) != Some(machine) {
        return;
    }
    let Some(state) = world.get::<Boulder>(boulder).cloned() else {
        return;
    };
    let Some(tech) = world.get::<Bouldertech>(machine).cloned() else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(machine) else {
        return;
    };
    if state.processed_by.is_none() {
        let accepted: &[&str] = if tech.smelter {
            &SMELTER_MATERIALS
        } else {
            &REFINERY_MATERIALS
        };
        let mut rejected = Vec::new();
        let mut points = 0i64;
        for (material, units) in state.materials {
            if !accepted.contains(&material.as_str()) {
                rejected.push((material, units));
                continue;
            }
            let per_sheet = MATERIALS
                .iter()
                .find(|(m, ..)| *m == material)
                .map_or(0.0, |&(_, _, p)| p);
            points += (units * per_sheet / SHEET).round() as i64;
            if !insert_into_silo(world, tech.silo, &material, units) {
                let sheet = world
                    .get_resource::<ResearchDb>()
                    .and_then(|db| db.materials.get(*db.material_index.get(&material)?))
                    .and_then(|m| m.sheet_type.clone());
                if let Some(sheet) = sheet {
                    spawn_stack(world, &sheet, pos, (units / SHEET).floor() as u32);
                }
            }
        }
        let empty = rejected.is_empty();
        if let Some(mut t) = world.get_mut::<Bouldertech>(machine) {
            t.points += points;
            if empty && state.artifact.is_some() {
                // Artifacts give bonus points.
                t.points += MINER_POINT_MULTIPLIER;
            }
        }
        if empty {
            playsound(world, PlaySound::new(pos, usage_sound(&tech), 50.0, true));
            break_apart(world, boulder);
            return;
        }
        if let Some(mut b) = world.get_mut::<Boulder>(boulder) {
            b.materials = rejected;
            b.processed_by = Some(machine);
        }
    }
    remove_boulder(world, machine, boulder);
}

/// `silo_materials.mat_container.insert_amount_mat`. False without a silo or room.
fn insert_into_silo(world: &mut World, silo: Option<Entity>, material: &str, units: f64) -> bool {
    let Some(silo) = silo else {
        return false;
    };
    let Some(index) = world
        .get_resource::<ResearchDb>()
        .and_then(|db| db.material_index.get(material).copied())
    else {
        return false;
    };
    let Some(mut store) = world.get_mut::<MaterialStore>(silo) else {
        return false;
    };
    let amount = units.round() as u32;
    if store
        .max
        .is_some_and(|max| store.total() + u64::from(amount) > u64::from(max))
    {
        return false;
    }
    match store.amounts.get_mut(index) {
        Some(n) => {
            *n += amount;
            true
        }
        None => false,
    }
}

fn usage_sound(tech: &Bouldertech) -> &'static str {
    if tech.smelter {
        "sound/machines/mining/smelter.ogg"
    } else {
        "sound/machines/mining/refinery.ogg"
    }
}

/// `remove_boulder`: out onto the machine's tile with its durability rolled lower. False when a
/// boulder already lies there.
fn remove_boulder(world: &mut World, machine: Entity, boulder: Entity) -> bool {
    if world.get_entity(boulder).is_err() {
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(machine) else {
        return false;
    };
    let blocked = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| e != boulder && world.get::<Boulder>(e).is_some());
    if blocked {
        return false;
    }
    let Some(state) = world.get::<Boulder>(boulder).cloned() else {
        return true;
    };
    if state.materials.is_empty() {
        break_apart(world, boulder);
        return true;
    }
    let durability = {
        let mut rng = world.resource_mut::<CombatRng>();
        if state.size == SIZE_SMALL {
            rng.range(2, SIZE_SMALL - 1)
        } else {
            rng.range(SIZE_SMALL, state.size.max(SIZE_SMALL + 1) - 1)
        }
    };
    if let Some(mut b) = world.get_mut::<Boulder>(boulder) {
        b.durability = durability;
        b.processed_by = None;
    }
    take_out(world, boulder, pos);
    playsound(
        world,
        PlaySound::new(pos, "sound/machines/ping.ogg", 50.0, false),
    );
    true
}

/// The machines' `process()`: the refineries wear their boulders down, the toggled BRMs pull a
/// batch.
fn machine_process(world: &mut World) {
    let mut query = world.query_filtered::<Entity, With<Bouldertech>>();
    let machines: Vec<Entity> = query.iter(world).collect();
    for machine in machines {
        if !working(world, machine) {
            continue;
        }
        let (_, mut budget) = capacity(world, machine);
        let mut found = false;
        for boulder in held_boulders(world, machine) {
            found = true;
            if budget == 0 {
                break;
            }
            budget -= 1;
            let left = {
                let mut b = world.get_mut::<Boulder>(boulder).expect("held boulder");
                if b.durability > 0 {
                    b.durability -= 1;
                }
                b.durability
            };
            if left > 0 {
                continue;
            }
            breakdown_boulder(world, machine, boulder);
            found = false;
        }
        if found {
            let (Some(&pos), Some(tech)) = (
                world.get::<TilePos>(machine),
                world.get::<Bouldertech>(machine).cloned(),
            ) else {
                continue;
            };
            playsound(world, PlaySound::new(pos, usage_sound(&tech), 29.0, false));
            let action = if tech.smelter { "smelting" } else { "crushing" };
            crate::balloon::balloon_alert_to_viewers(world, machine, action, None, &[]);
        }
    }
    let t = now(world);
    let mut query = world.query::<(Entity, &Brm)>();
    let brms: Vec<Entity> = query
        .iter(world)
        .filter(|(_, b)| b.toggled_on && !b.batch_processing && t >= b.batch_ready)
        .map(|(e, _)| e)
        .collect();
    for brm in brms {
        let max = brm_batch(world, brm);
        pre_collect(world, brm, false, max);
    }
}

/// BRM `boulders_processing_max`: `ceil(part tiers / 12 × 7)`.
fn brm_batch(world: &World, brm: Entity) -> u32 {
    let ratings = crate::stock_parts::ratings(world, brm);
    let tiers: u32 = ratings.parts.iter().map(|&(_, t)| t).sum();
    (f64::from(tiers) / 12.0 * 7.0).ceil() as u32
}

/// What `pre_collect_boulder` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collect {
    /// Not operating, or no boulder to pull.
    Nothing,
    /// `TURF_BLOCKED_BY_BOULDER`.
    Blocked,
    Teleported,
}

/// `pre_collect_boulder(feedback, boulders_remaining)`: a random available boulder onto the
/// BRM's tile; more of the batch follow every [`TELEPORT_TIME`].
pub fn pre_collect(world: &mut World, brm: Entity, feedback: bool, remaining: u32) -> Collect {
    let Some(&pos) = world.get::<TilePos>(brm) else {
        return Collect::Nothing;
    };
    let set = |world: &mut World, batch: bool, next: Option<(f64, u32, bool)>| {
        if let Some(mut b) = world.get_mut::<Brm>(brm) {
            b.batch_processing = batch;
            b.next = next;
        }
    };
    if !working(world, brm) {
        set(world, false, None);
        return Collect::Nothing;
    }
    let blocked = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| world.get::<Boulder>(e).is_some());
    if blocked {
        set(world, false, None);
        return Collect::Blocked;
    }
    // Only boulders still lying where they were made (not carried off since the last fire).
    let mut available = std::mem::take(&mut world.resource_mut::<OreGeneration>().available);
    available.retain(|&b| world.get::<Boulder>(b).is_some() && world.get::<TilePos>(b).is_some());
    if available.is_empty() {
        world.resource_mut::<OreGeneration>().available = available;
        if feedback {
            playsound(
                world,
                PlaySound::new(pos, "sound/machines/synth/synth_no.ogg", 30.0, true),
            );
            crate::balloon::balloon_alert_to_viewers(
                world,
                brm,
                "no boulders to collect!",
                None,
                &[],
            );
        }
        set(world, false, None);
        return Collect::Nothing;
    }
    let i = world
        .resource_mut::<CombatRng>()
        .range(0, available.len() as u32 - 1) as usize;
    let boulder = available.remove(i);
    world.resource_mut::<OreGeneration>().available = available;
    let toggled = world.get::<Brm>(brm).is_some_and(|b| b.toggled_on);
    let sound = if toggled {
        AUTO_TELEPORT_SOUND
    } else {
        MANUAL_TELEPORT_SOUND
    };
    playsound(world, PlaySound::new(pos, sound, 50.0, true));
    take_out(world, boulder, pos);
    crate::balloon::balloon_alert_to_viewers(world, brm, "boulder appears!", None, &[]);
    let left = remaining.saturating_sub(1);
    if left == 0 {
        let t = now(world);
        if let Some(mut b) = world.get_mut::<Brm>(brm) {
            b.batch_ready = t + BATCH_COOLDOWN;
        }
        set(world, false, None);
    } else {
        let t = now(world);
        set(world, true, Some((t + TELEPORT_TIME, left, feedback)));
    }
    Collect::Teleported
}

/// The `addtimer` of the rest of a BRM batch.
fn brm_batches(world: &mut World) {
    let t = now(world);
    let mut query = world.query::<(Entity, &Brm)>();
    let due: Vec<(Entity, u32, bool)> = query
        .iter(world)
        .filter_map(|(e, b)| {
            let (at, left, feedback) = b.next?;
            (t >= at).then_some((e, left, feedback))
        })
        .collect();
    for (brm, left, feedback) in due {
        pre_collect(world, brm, feedback, left);
    }
}

/// tg `attack_hand` of a BRM (a boulder teleported) and `attack_hand_secondary` of a BRM
/// (automatic retrieval toggled) or a refinery (a boulder taken out). True when handled.
pub fn attack_hand(world: &mut World, mob: Entity, target: Entity, right: bool) -> bool {
    let brm = world.get::<Brm>(target).cloned();
    let is_tech = world.get::<Bouldertech>(target).is_some();
    if (brm.is_none() && !is_tech) || panel_open(world, target) {
        return false;
    }
    if right && !crate::obj_damage::anchored(world, target) {
        let text = "anchor it first!";
        crate::balloon::balloon_alert(world, mob, target, text);
        return true;
    }
    if let Some(brm) = brm {
        if right {
            if world.get::<Unpowered>(target).is_none() {
                let on = !brm.toggled_on;
                if let Some(mut b) = world.get_mut::<Brm>(target) {
                    b.toggled_on = on;
                }
                let state = if on { "brm-toggled" } else { "brm" };
                crate::obj_damage::set_icon_state(world, target, state);
            }
            return true;
        }
        let t = now(world);
        if t < brm.manual_ready {
            return true;
        }
        if brm.batch_processing {
            crate::balloon::balloon_alert(world, mob, target, "batch still processing!");
            return true;
        }
        if let Some(&pos) = world.get::<TilePos>(target) {
            playsound(
                world,
                PlaySound::new(pos, MANUAL_TELEPORT_SOUND, 50.0, true),
            );
        }
        match pre_collect(world, target, true, 1) {
            Collect::Blocked => crate::balloon::balloon_alert(world, mob, target, "no space!"),
            Collect::Teleported => {
                crate::balloon::balloon_alert(world, mob, target, "teleporting...");
            }
            Collect::Nothing => {}
        }
        if let Some(mut b) = world.get_mut::<Brm>(target) {
            b.manual_ready = t + TELEPORT_TIME;
        }
        return true;
    }
    if !right {
        return false;
    }
    let Some(boulder) = held_boulders(world, target).first().copied() else {
        crate::balloon::balloon_alert_to_viewers(
            world,
            target,
            "no boulders to remove!",
            None,
            &[],
        );
        return true;
    };
    if !remove_boulder(world, target, boulder) {
        crate::balloon::balloon_alert_to_viewers(world, target, "no space to remove!", None, &[]);
    }
    true
}

/// An ID card on a refinery: the points it holds, claimed by `tgui_input_number`.
fn claim_points(world: &mut World, client: Entity, mob: Entity, machine: Entity, card: Entity) {
    let Some(tech) = world.get::<Bouldertech>(machine).cloned() else {
        return;
    };
    if tech.points <= 0 {
        crate::balloon::balloon_alert_to_viewers(world, machine, "no points to claim!", None, &[]);
        let t = now(world);
        if t < tech.sound_ready {
            return;
        }
        if let Some(mut m) = world.get_mut::<Bouldertech>(machine) {
            m.sound_ready = t + SOUND_COOLDOWN;
        }
        if let Some(&pos) = world.get::<TilePos>(machine) {
            playsound(
                world,
                PlaySound::new(pos, "sound/machines/buzz/buzz-sigh.ogg", 30.0, false),
            );
        }
        return;
    }
    let balance = crate::cargo::economy::account_of(world, card)
        .and_then(|a| {
            world
                .get_resource::<crate::cargo::economy::Economy>()?
                .account(a)
                .map(|acc| acc.mining_points)
        })
        .unwrap_or(0);
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Transfer Points",
            format!(
                "How many mining points do you wish to claim? ID Balance: {balance}, stored mining \
                 points: {}",
                tech.points
            ),
            PromptKind::Number {
                default: 0.0,
                min: 0.0,
                max: tech.points as f64,
                round: true,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(ClaimPrompt { machine, card });
}

/// The claim's answer, with the card still in hand by the machine.
fn claim_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(ClaimPrompt { machine, card }) = world.get::<ClaimPrompt>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Number(amount)) = answer else {
            continue;
        };
        let amount = amount.max(0.0).floor() as i64;
        if amount == 0 {
            continue;
        }
        let holding = world.get::<Inventory>(actor).is_some_and(|i| i.holds(card));
        let near = match (world.get::<TilePos>(actor), world.get::<TilePos>(machine)) {
            (Some(a), Some(m)) => a.chebyshev(*m).is_some_and(|d| d <= 1),
            _ => false,
        };
        let Some(held) = world.get::<Bouldertech>(machine).map(|t| t.points) else {
            continue;
        };
        if !holding || !near || held <= 0 {
            continue;
        }
        let amount = amount.min(held);
        let Some(account) = crate::cargo::economy::account_of(world, card) else {
            continue;
        };
        let paid = world
            .get_resource_mut::<crate::cargo::economy::Economy>()
            .and_then(|mut e| e.account_mut(account).map(|a| a.mining_points += amount))
            .is_some();
        if !paid {
            continue;
        }
        if let Some(mut t) = world.get_mut::<Bouldertech>(machine) {
            t.points -= amount;
        }
        let machine_name = crate::security::name_of(world, machine);
        let card_name = crate::security::name_of(world, card);
        tell(
            world,
            client,
            Span::Notice,
            format!("You claim {amount} mining points from the {machine_name} to {card_name}."),
        );
    }
}

/// The live state of a BRM or a refinery, mapped or built.
pub fn setup(world: &mut World, machine: Entity) {
    let Some(path) = path_of(world, machine) else {
        return;
    };
    if is_under(&path, BRM) {
        world.entity_mut(machine).insert(Brm::default());
        return;
    }
    if !is_under(&path, BOULDERTECH) {
        return;
    }
    let z = world.get::<TilePos>(machine).map(|p| p.z);
    let silo = super::orm::silo_on(world, z);
    world.entity_mut(machine).insert(Bouldertech {
        smelter: is_under(&path, SMELTER),
        silo,
        ..Default::default()
    });
}

fn setup_mapped(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let machines: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, BRM) || is_under(&o.path, BOULDERTECH))
        .map(|(e, _)| e)
        .collect();
    for machine in machines {
        setup(world, machine);
    }
}

/// A boulder beacon used in hand (`attack_self`): beeps, and a second later its next machine.
/// True when `item` is one.
fn beacon_attack_self(world: &mut World, mob: Entity, item: Entity) -> bool {
    if path_of(world, item).is_none_or(|p| !is_under(&p, BOULDER_BEACON)) {
        return false;
    }
    let t = now(world);
    match world.get_mut::<BoulderBeacon>(item) {
        Some(mut b) => b.launches.push(t + BEACON_DELAY),
        None => {
            world.entity_mut(item).insert(BoulderBeacon {
                uses: 3,
                launches: vec![t + BEACON_DELAY],
            });
        }
    }
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        let name = crate::security::name_of(world, item);
        crate::security::show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (Span::Warning, format!("The {name} begins to beep loudly!")),
            &[],
        );
    }
    true
}

/// `launch_payload`: a BRM, a refinery, then a smelter (and the beacon is spent).
fn beacon_launches(world: &mut World) {
    let t = now(world);
    let mut query = world.query::<(Entity, &BoulderBeacon)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, b)| b.launches.iter().any(|&at| at <= t))
        .map(|(e, _)| e)
        .collect();
    for beacon in due {
        let Some(pos) = crate::explosions::turf_of(world, beacon) else {
            continue;
        };
        let uses = {
            let Some(mut b) = world.get_mut::<BoulderBeacon>(beacon) else {
                continue;
            };
            let Some(i) = b.launches.iter().position(|&at| at <= t) else {
                continue;
            };
            b.launches.remove(i);
            let uses = b.uses;
            b.uses = b.uses.saturating_sub(1);
            uses
        };
        playsound(world, PlaySound::new(pos, "sparks", 80.0, true));
        let path = match uses {
            3 => BRM,
            2 => REFINERY,
            1 => SMELTER,
            _ => continue,
        };
        if let Some(machine) = spawn_obj(world, path, pos) {
            crate::machine_frame::setup_built(world, machine);
        }
        if uses == 1 {
            world.despawn(beacon);
        }
    }
}

/// `examine` of boulders and the boulder machines.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(b) = world.get::<Boulder>(target) {
        lines.push((
            Span::Notice,
            format!(
                "This boulder would take {} more steps to refine or break.",
                b.durability
            ),
        ));
    }
    if let Some(tech) = world.get::<Bouldertech>(target) {
        let (held_max, processing) = capacity(world, target);
        let count = held_boulders(world, target).len();
        lines.push((
            Span::Notice,
            format!(
                "The machine reads that it has {} mining points stored. Swipe an ID to claim them.",
                tech.points
            ),
        ));
        if count >= 1 {
            lines.push((
                Span::Notice,
                "Right Click to manually remove a stored boulder.".into(),
            ));
        }
        lines.push((
            Span::Info,
            format!("Storage capacity = {count}/{held_max} boulders."),
        ));
        lines.push((
            Span::Info,
            format!("This machine can process up to {processing} boulders at a time."),
        ));
    }
    if world.get::<Brm>(target).is_some() {
        let available = world
            .get_resource::<OreGeneration>()
            .map_or(0, |g| g.available.len());
        let toggled = world.get::<Brm>(target).is_some_and(|b| b.toggled_on);
        lines.push((
            Span::Notice,
            format!("The small screen reads there are {available} boulders available to teleport."),
        ));
        lines.push((
            Span::Notice,
            format!(
                "Can collect up to {} boulders at a time.",
                brm_batch(world, target)
            ),
        ));
        lines.push((
            Span::Notice,
            format!(
                "Automatic boulder retrieval can be toggled {} with Right Click.",
                if toggled { "Off" } else { "On" }
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_crushing_gives_a_sheet_less_capped_at_ten() {
        assert_eq!(quantity_of_ore(SHEET * 1.1), 1);
        assert_eq!(quantity_of_ore(SHEET * 3.5), 2);
        assert_eq!(quantity_of_ore(SHEET * 16.0), 10);
        assert_eq!(quantity_of_ore(50.0), 1);
    }

    #[test]
    fn boulder_types_roll_their_materials() {
        let mut rng = CombatRng(11);
        let shabby = Boulder::of_type(SHABBY_BOULDER, &mut rng);
        assert_eq!(shabby.durability, 1);
        assert_eq!(shabby.materials.len(), 2);
        let gulag_vent = Boulder::of_type(GULAG_VENT_BOULDER, &mut rng);
        assert_eq!(
            gulag_vent.materials,
            vec![("/datum/material/iron".to_owned(), SHEET * 16.0)]
        );
        for _ in 0..20 {
            let gulag = Boulder::of_type(GULAG_BOULDER, &mut rng);
            let (_, units) = &gulag.materials[0];
            assert!((SHEET * 10.0..=SHEET * 13.0).contains(units));
            let bs = Boulder::of_type(ARTIFACT_BLUESPACE, &mut rng);
            assert_eq!(bs.artifact, Some(RELIC));
            assert_eq!(bs.materials[0].0, "/datum/material/bluespace");
            assert!((SHEET..=SHEET * 5.0).contains(&bs.materials[0].1));
        }
        assert!(Boulder::of_type(ARTIFACT, &mut rng).materials.is_empty());
    }

    #[test]
    fn refinery_and_smelter_split_the_minerals() {
        for m in SMELTER_MATERIALS {
            assert!(!REFINERY_MATERIALS.contains(&m));
            assert!(MATERIALS.iter().any(|(x, ..)| *x == m));
        }
        for m in REFINERY_MATERIALS {
            assert!(MATERIALS.iter().any(|(x, ..)| *x == m));
        }
    }
}
