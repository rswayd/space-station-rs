//! The experimental cloner (`code/game/machinery/experimental_cloner/`): the scanner
//! (`/obj/machinery/experimental_cloner_scanner`), the control console
//! (`/obj/machinery/computer/experimental_cloner`, tgui `ExperimentalCloner`), the cloning pod
//! (`/obj/machinery/experimental_cloner`), the record (`/datum/experimental_cloning_record`) and
//! the mistakes (`/datum/experimental_cloner_fuckup`).
//!
//! - The scanner: an empty hand opens or closes it (closing takes a living mob on its tile), a
//!   carbon dragged onto it is shut in, a step from inside opens it unless it is locked. The
//!   console's scan irradiates the occupant, locks the door for 10 s and stores a record in every
//!   console linked to the scanner: name, species, body, DNA (identity and gene sequence, no
//!   mutations), blood type, quirks and brain traumas.
//! - The console links the scanner and the pod within 5 tiles at map load, or the ones a
//!   multitool saved (a multitool on a scanner or pod saves it, on the console links it).
//! - The pod grows the stored record for a minute, then polls ghosts for 10 s: a naked body with
//!   the record applied (mutations of complete blocks active), with a chance
//!   (`GLOB.experimental_cloner_fuckup_chance`, 50% and +1..5 per success) of a weighted mistake.
//!   Nobody, or the body leaving the pod, dying or vanishing: it is flushed out as gibs. Power
//!   loss stops a scan or turns the growing clone into gibs.
// shortcut: no evil clone antagonist (2%), height reroll, age, athletics, TTS voice or policy
// text; no breaking out of the locked scanner (a step only tells it is locked), no welder
// deconstruction or sound loops. Mistakes without the systems they need are left out of the
// pool: bioscrambling, flesh limbs and psykers. A random brain trauma picks its category
// first (tg picks among all trauma types); felinisation of a non-human gives the cat ears and
// tongue but no tail (no tail organ yet).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::experimental_cloner::{
    ACT_CLEAR_RECORD, ACT_START_CLONE, ACT_START_SCAN, ExperimentalClonerData, ExperimentalClonerUi,
};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::{MapInitLate, Mapped};

use super::{Dna, MINOR_NEGATIVE, NEGATIVE};
use crate::closets::{ClosetRelayMove, InCloset};
use crate::health::Stat;
use crate::human::HumanLook;
use crate::medical::organs::{self, Slot};
use crate::medical::pods::{self, Pod};
use crate::sound::{PlaySound, playsound};
use crate::species::SpeciesId;
use crate::tools::{ToolKind, now, tell, tell_mob};

pub const POD_PATH: &str = "/obj/machinery/experimental_cloner";
pub const CONSOLE_PATH: &str = "/obj/machinery/computer/experimental_cloner";
pub const SCANNER_PATH: &str = "/obj/machinery/experimental_cloner_scanner";
/// `scan_time`, `cloning_time`, the poll's `poll_time`.
const SCAN_SECS: f64 = 10.0;
const CLONING_SECS: f64 = 60.0;
const POLL_SECS: f64 = 10.0;
/// `breakout_time`: the locked door's message cooldown.
const BREAKOUT_SECS: f64 = 3.0;
/// `connect_nearby_machines`: `range(5)`.
const LINK_RANGE: u32 = 5;
/// `GLOB.experimental_cloner_fuckup_chance`'s start.
const FUCKUP_CHANCE: f32 = 50.0;
/// `POLL_IGNORE_EXPERIMENTAL_CLONER`.
const POLL_IGNORE: &str = "experimental_cloner";
const FLESHBLOB: &str = "/mob/living/basic/fleshblob";
const CAT_EARS: &str = "/obj/item/organ/ears/cat";
const CAT_TONGUE: &str = "/obj/item/organ/tongue/cat";
const SCANBUZZ: &str = "sound/machines/scanner/scanbuzz.ogg";
const UNLOCK: &str = "sound/machines/closet/closet_unlock.ogg";
const BUZZ: &str = "sound/machines/buzz/buzz-two.ogg";

/// `/datum/experimental_cloning_record`.
#[derive(Clone, Debug, PartialEq)]
pub struct CloneRecord {
    pub name: String,
    pub species: SpeciesId,
    /// The body (naked, whole).
    pub look: HumanLook,
    /// `copy_dna` with `COPY_DNA_SE`: identity and sequence, no mutations.
    pub dna: Option<Dna>,
    pub blood: Option<crate::medical::blood::BloodType>,
    pub quirks: Vec<crate::quirks::Quirk>,
    pub traumas: Vec<crate::medical::traumas::Kind>,
}

/// The scanner's state (the door is [`Pod`]).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CloneScanner {
    pub locked: bool,
    /// `scan_timer`: when the scan completes.
    pub scan_until: Option<f64>,
    /// `message_cooldown` of the locked door.
    pub message_at: f64,
}

/// The console: its links, the stored record and the window data.
#[derive(Component, Clone, Debug, Default)]
pub struct CloneConsole {
    pub scanner: Option<Entity>,
    pub pod: Option<Entity>,
    pub record: Option<CloneRecord>,
    view: ExperimentalClonerData,
}

/// The pod's state.
#[derive(Component, Clone, Debug, Default)]
pub struct ClonePod {
    pub running: bool,
    /// `running_timer`: when the body is done.
    pub until: f64,
    pub record: Option<CloneRecord>,
    /// Waiting for the ghost poll with this body inside.
    pub clone: Option<Entity>,
}

/// `GLOB.experimental_cloner_fuckup_chance` (persists for the round).
#[derive(Resource, Clone, Copy, Debug)]
pub struct FuckupChance(pub f32);

impl Default for FuckupChance {
    fn default() -> Self {
        Self(FUCKUP_CHANCE)
    }
}

/// The `/datum/experimental_cloner_fuckup` subtypes ported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fuckup {
    Felinise,
    Bald,
    BrainTrauma,
    Mutate,
    FlyMishap,
    Monkey,
    Skeletised,
    TotalFailure,
}

impl Fuckup {
    pub const ALL: [Self; 8] = [
        Self::Felinise,
        Self::Bald,
        Self::BrainTrauma,
        Self::Mutate,
        Self::FlyMishap,
        Self::Monkey,
        Self::Skeletised,
        Self::TotalFailure,
    ];

    /// `weight`: `CLONER_FAILURE_COMMON` 3, `CLONER_FAILURE_RARE` 1.
    pub fn weight(self) -> u32 {
        match self {
            Self::Monkey | Self::Skeletised | Self::TotalFailure => 1,
            _ => 3,
        }
    }

    /// `is_valid(species_type)`.
    pub fn is_valid(self, species: SpeciesId) -> bool {
        match self {
            Self::Felinise => species != SpeciesId::Felinid,
            // `HEAD_HAIR || HEAD_FACIAL_HAIR` of the species' head.
            Self::Bald => species.def().limbs.hair,
            _ => true,
        }
    }
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // tg `interact` of the scanner (open, close) and `ui_interact` of the console.
    app.add_click_handler(ClickPhase::EmptyHand, 2605, |w, c| {
        interact(w, c.client, c.mob, c.target)
    });
    // tg `multitool_act` of the three machines.
    app.add_click_handler(ClickPhase::HeldOnTarget, 1605, |w, c| {
        c.held
            .is_some_and(|h| multitool_act(w, c.client, c.mob, h, c.target))
    });
    app.register_ui::<ClonerWindow>()
        .init_resource::<FuckupChance>()
        .add_observer(attach)
        .add_systems(MapInitLate, connect_mapped)
        .add_systems(
            Update,
            (
                refresh_views.before(UiSystems::Open),
                apply_acts.in_set(UiSystems::Handle),
                relay_moves.after(ss13_world::WorldSystems::Movement),
                timers,
            ),
        );
}

fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    let e = add.entity;
    if is_under(&obj.path, SCANNER_PATH) {
        commands
            .entity(e)
            .insert_if_new((Pod { open: false }, CloneScanner::default()));
    } else if is_under(&obj.path, CONSOLE_PATH) {
        commands.entity(e).insert_if_new(CloneConsole::default());
    } else if is_under(&obj.path, POD_PATH) {
        commands.entity(e).insert_if_new(ClonePod::default());
    } else {
        return;
    }
    commands.queue(move |world: &mut World| refresh_look(world, e));
}

/// `find_connections = mapload` -> `connect_nearby_machines`: the nearest scanner and pod within
/// 5 tiles of a mapped console.
fn connect_mapped(world: &mut World) {
    let consoles: Vec<Entity> = world
        .query_filtered::<Entity, (With<CloneConsole>, With<Mapped>)>()
        .iter(world)
        .collect();
    for console in consoles {
        let scanner = nearest::<CloneScanner>(world, console);
        let pod = nearest::<ClonePod>(world, console);
        if let Some(mut c) = world.get_mut::<CloneConsole>(console) {
            c.scanner = scanner;
            c.pod = pod;
        }
    }
}

fn nearest<T: Component>(world: &mut World, from: Entity) -> Option<Entity> {
    let pos = *world.get::<TilePos>(from)?;
    let mut q = world.query_filtered::<(Entity, &TilePos), With<T>>();
    q.iter(world)
        .filter_map(|(e, p)| Some((pos.chebyshev(*p)?, e)))
        .filter(|(d, _)| *d <= LINK_RANGE)
        .min()
        .map(|(_, e)| e)
}

fn name_of(world: &World, e: Entity) -> String {
    crate::medical::name_of(world, e)
}

fn sound(world: &mut World, at: Entity, file: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        playsound(world, PlaySound::new(pos, file, vol, false));
    }
}

fn balloon(world: &mut World, mob: Entity, source: Entity, text: &str) {
    crate::balloon::balloon_alert(world, mob, source, text);
}

/// The scanner's occupant.
pub fn occupant(world: &mut World, scanner: Entity) -> Option<Entity> {
    pods::occupant(world, scanner)
}

/// The scanner's `update_icon_state`; the pod's `pod_[running]`.
fn refresh_look(world: &mut World, e: Entity) {
    if let Some(pod) = world.get::<ClonePod>(e) {
        let state = if pod.running { "pod_1" } else { "pod_0" };
        crate::obj_damage::set_icon_state(world, e, state);
        return;
    }
    if world.get::<CloneScanner>(e).is_none() {
        return;
    }
    let open = world.get::<Pod>(e).is_some_and(|p| p.open);
    let state = if !pods::operational(world, e) {
        if open {
            "scanner_open_unpowered"
        } else {
            "scanner_unpowered"
        }
    } else if occupant(world, e).is_some() {
        "scanner_occupied"
    } else if open {
        "scanner_open"
    } else {
        "scanner"
    };
    crate::obj_damage::set_icon_state(world, e, state);
}

/// `open_machine` of the scanner: a running scan fails.
fn open_scanner(world: &mut World, scanner: Entity) {
    pods::open_pod(world, scanner);
    if world
        .get::<CloneScanner>(scanner)
        .is_some_and(|s| s.scan_until.is_some())
    {
        fail_scan(world, scanner);
    }
    refresh_look(world, scanner);
}

fn close_scanner(world: &mut World, scanner: Entity, target: Option<Entity>) {
    pods::close_pod(world, scanner, target);
    refresh_look(world, scanner);
}

/// `attack_hand`: the scanner's `toggle_open`, the console's window.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if let Some(scanner) = world.get::<CloneScanner>(target).copied() {
        if world.get::<Pod>(target).is_some_and(|p| p.open) {
            close_scanner(world, target, None);
        } else if scanner.locked {
            balloon(world, mob, target, "заблокировано!");
        } else {
            open_scanner(world, target);
        }
        return true;
    }
    if world.get::<CloneConsole>(target).is_none() {
        return false;
    }
    refresh_view(world, target);
    let title = name_of(world, target);
    world.write_message(OpenUi::new::<ClonerWindow>(client, mob, target, title));
    true
}

/// `mouse_drop_receive`: a carbon dragged onto an open, empty scanner is shut in.
pub fn mouse_drop_receive(world: &mut World, scanner: Entity, dragged: Entity) -> bool {
    if world.get::<CloneScanner>(scanner).is_none() {
        return false;
    }
    if world.get::<Pod>(scanner).is_some_and(|p| p.open)
        && world.get::<Dna>(dragged).is_some()
        && occupant(world, scanner).is_none()
    {
        close_scanner(world, scanner, Some(dragged));
    }
    true
}

/// `multitool_act`: the scanner and pod go into the buffer, the console links what it holds.
pub fn multitool_act(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tool: Entity,
    target: Entity,
) -> bool {
    if crate::tools::tool_of(world, tool).map(|(k, _)| k) != Some(ToolKind::Multitool) {
        return false;
    }
    if world.get::<CloneScanner>(target).is_some() || world.get::<ClonePod>(target).is_some() {
        crate::tools::set_multitool_buffer(world, tool, target);
        balloon(world, mob, target, "частота сохранена");
        return true;
    }
    if world.get::<CloneConsole>(target).is_none() || !pods::operational(world, target) {
        return false;
    }
    let Some(buffer) = crate::tools::multitool_buffer(world, tool) else {
        return false;
    };
    let scanner = world.get::<CloneScanner>(buffer).is_some();
    if !scanner && world.get::<ClonePod>(buffer).is_none() {
        return false;
    }
    if let Some(mut c) = world.get_mut::<CloneConsole>(target) {
        if scanner {
            c.scanner = Some(buffer);
        } else {
            c.pod = Some(buffer);
        }
    }
    let alert = if scanner {
        "сканер подключён"
    } else {
        "капсула подключена"
    };
    balloon(world, mob, target, alert);
    let (what, console) = (name_of(world, buffer), name_of(world, target));
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы связываете {what} с {console}."),
    );
    true
}

/// `relaymove`: a conscious occupant's step opens the scanner; locked, it says so.
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let mobs: Vec<Entity> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| m.mob)
        .collect();
    let now = now(world);
    for mob in mobs {
        let Some(scanner) = world.get::<InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        let Some(state) = world.get::<CloneScanner>(scanner).copied() else {
            continue;
        };
        let conscious = world.get::<Stat>(mob) == Some(&Stat::Conscious);
        if !conscious || state.locked {
            if now >= state.message_at {
                if let Some(mut s) = world.get_mut::<CloneScanner>(scanner) {
                    s.message_at = now + BREAKOUT_SECS;
                }
                balloon(world, mob, scanner, "дверь заблокирована!");
            }
            continue;
        }
        open_scanner(world, scanner);
    }
}

/// `start_scan`: the occupant irradiated, the door locked until the scan ends.
fn start_scan(world: &mut World, scanner: Entity) -> bool {
    let Some(mob) = occupant(world, scanner).filter(|_| pods::operational(world, scanner)) else {
        sound(world, scanner, SCANBUZZ, 100.0);
        return false;
    };
    crate::radiation::irradiate(world, mob);
    let until = now(world) + SCAN_SECS;
    if let Some(mut s) = world.get_mut::<CloneScanner>(scanner) {
        s.locked = true;
        s.scan_until = Some(until);
    }
    sound(world, scanner, UNLOCK, 100.0);
    true
}

/// `complete_scan`: the record goes to every console linked to the scanner.
fn complete_scan(world: &mut World, scanner: Entity) {
    let Some(mob) = occupant(world, scanner) else {
        fail_scan(world, scanner);
        return;
    };
    let record = create_profile(world, mob);
    let consoles: Vec<Entity> = world
        .query::<(Entity, &CloneConsole)>()
        .iter(world)
        .filter(|(_, c)| c.scanner == Some(scanner))
        .map(|(e, _)| e)
        .collect();
    for console in consoles {
        if let Some(mut c) = world.get_mut::<CloneConsole>(console) {
            c.record = Some(record.clone());
        }
    }
    on_scan_stopped(world, scanner);
}

fn fail_scan(world: &mut World, scanner: Entity) {
    sound(world, scanner, SCANBUZZ, 100.0);
    on_scan_stopped(world, scanner);
}

/// `on_scan_stopped`.
fn on_scan_stopped(world: &mut World, scanner: Entity) {
    let Some(mut s) = world.get_mut::<CloneScanner>(scanner) else {
        return;
    };
    let was_locked = s.locked;
    s.locked = false;
    s.scan_until = None;
    if was_locked {
        sound(world, scanner, UNLOCK, 100.0);
    }
}

/// `create_profile`.
pub fn create_profile(world: &mut World, mob: Entity) -> CloneRecord {
    let look = world
        .get::<HumanLook>(mob)
        .cloned()
        .map(|look| HumanLook {
            uniform: None,
            shoes: None,
            husk: false,
            missing: 0,
            robotic: 0,
            eyeless: false,
            ..look
        })
        .unwrap_or_else(|| crate::human::AppearancePrefs::random(0).body());
    let dna = world.get::<Dna>(mob).cloned().map(|dna| Dna {
        mutations: Vec::new(),
        stability: 100,
        melt_until: None,
        ..dna
    });
    let traumas = organs::organ(world, mob, Slot::Brain)
        .and_then(|brain| world.get::<crate::medical::traumas::Traumas>(brain))
        .map(|t| t.0.iter().map(|t| t.kind).collect())
        .unwrap_or_default();
    CloneRecord {
        name: name_of(world, mob),
        species: crate::species::species_of(world, mob),
        look,
        dna,
        blood: world
            .get::<crate::medical::blood::Blood>(mob)
            .map(|b| b.blood_type),
        quirks: world
            .get::<crate::quirks::Quirks>(mob)
            .map(|q| q.0.clone())
            .unwrap_or_default(),
        traumas,
    }
}

/// `create_result_mob` -> `apply_profile`: a naked body of the record at `pos`.
pub fn create_clone(world: &mut World, record: &CloneRecord, pos: TilePos) -> Entity {
    let look = HumanLook {
        species: record.species,
        ..record.look.clone()
    };
    let mob = crate::server::spawn_body(world, pos, record.name.clone(), look);
    world.flush();
    if let Some(dna) = record.dna.clone() {
        world.entity_mut(mob).insert(dna);
    }
    if let (Some(blood), Some(mut b)) = (
        record.blood,
        world.get_mut::<crate::medical::blood::Blood>(mob),
    ) {
        b.blood_type = blood;
    }
    for &quirk in &record.quirks {
        crate::quirks::add_quirk(world, mob, quirk, false, false);
    }
    if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
        for &kind in &record.traumas {
            crate::medical::traumas::gain(world, brain, kind, None);
        }
    }
    super::domutcheck(world, mob);
    crate::inventory::refresh_look(world, mob);
    mob
}

/// `start_cloning`.
fn start_cloning(world: &mut World, pod: Entity, record: CloneRecord) {
    let until = now(world) + CLONING_SECS;
    let Some(mut p) = world.get_mut::<ClonePod>(pod) else {
        return;
    };
    if p.running {
        return;
    }
    p.running = true;
    p.until = until;
    p.record = Some(record);
    refresh_look(world, pod);
}

/// `fail_growing`: gibs and a flush.
fn fail_growing(world: &mut World, pod: Entity) {
    if let Some(clone) = world
        .get_mut::<ClonePod>(pod)
        .and_then(|mut p| p.clone.take())
    {
        world.try_despawn(clone).ok();
    }
    if let Some(&pos) = world.get::<TilePos>(pod) {
        crate::effects::gibs::spawn_gibs(world, crate::effects::gibs::Gibspawner::Human, pos);
        playsound(
            world,
            PlaySound::new(pos, "sound/machines/toilet_flush.ogg", 40.0, true),
        );
    }
    on_finished(world, pod);
}

/// `on_finished`.
fn on_finished(world: &mut World, pod: Entity) {
    if let Some(mut p) = world.get_mut::<ClonePod>(pod) {
        p.running = false;
        p.record = None;
        p.clone = None;
    }
    refresh_look(world, pod);
}

/// `get_cloning_mistake`: none, or a weighted pick of the valid ones.
pub fn roll_mistake(
    chance: f32,
    species: SpeciesId,
    rng: &mut crate::combat::CombatRng,
) -> Option<Fuckup> {
    if !rng.chance(chance) {
        return None;
    }
    let valid: Vec<Fuckup> = Fuckup::ALL
        .into_iter()
        .filter(|f| f.is_valid(species))
        .collect();
    let total: u32 = valid.iter().map(|f| f.weight()).sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.range(1, total);
    for f in valid {
        if roll <= f.weight() {
            return Some(f);
        }
        roll -= f.weight();
    }
    None
}

/// `apply_to_mob` of a mistake, before anyone takes the body.
fn apply_mistake(world: &mut World, mob: Entity, mistake: Fuckup) {
    let mut rng = crate::medical::fork_rng(world);
    match mistake {
        Fuckup::Felinise => {
            if crate::species::species_of(world, mob) == SpeciesId::Human {
                crate::species::set_species(world, mob, SpeciesId::Felinid);
            } else {
                for path in [CAT_EARS, CAT_TONGUE] {
                    if let Some(organ) = organs::spawn_organ(world, path)
                        && !organs::insert(world, organ, mob)
                    {
                        world.despawn(organ);
                    }
                }
            }
        }
        Fuckup::Bald => {
            if let Some(mut look) = world.get_mut::<HumanLook>(mob) {
                look.facial_hair_style = None;
                look.hair_style = None;
            }
        }
        Fuckup::BrainTrauma => {
            use crate::medical::traumas::{Category, gain_type};
            let Some(brain) = organs::organ(world, mob, Slot::Brain) else {
                return;
            };
            let times = if rng.chance(50.0) { 1 } else { 2 };
            for _ in 0..times {
                let category =
                    [Category::Mild, Category::Severe, Category::Special][rng.range(0, 2) as usize];
                gain_type(world, brain, category, None, false, &mut rng);
            }
        }
        Fuckup::Mutate => {
            super::easy_random_mutate(world, mob, NEGATIVE | MINOR_NEGATIVE);
            if !rng.chance(50.0) {
                super::easy_random_mutate(world, mob, NEGATIVE | MINOR_NEGATIVE);
                if !rng.chance(75.0) {
                    super::easy_random_mutate(world, mob, NEGATIVE | MINOR_NEGATIVE);
                }
            }
        }
        Fuckup::FlyMishap => crate::species::set_species(world, mob, SpeciesId::Fly),
        Fuckup::Monkey | Fuckup::Skeletised | Fuckup::TotalFailure => {}
    }
    crate::inventory::refresh_look(world, mob);
}

/// `post_emerged` of a mistake, once the body is someone's and out of the pod.
fn post_emerged(world: &mut World, mob: Entity, mistake: Fuckup) {
    let name = name_of(world, mob);
    match mistake {
        Fuckup::Monkey => {
            crate::medical::visible(
                world,
                mob,
                "Как только вы выходите из капсулы, все волосы на вашем теле начинают расти!",
                &format!("Волосы {name} начинают стремительно расти!"),
                Span::BoldWarning,
            );
            crate::transform::monkeyize(world, mob, false);
        }
        Fuckup::Skeletised => {
            crate::medical::diseases::emote(world, mob, "scream");
            crate::medical::visible(
                world,
                mob,
                "Как только вы выходите из капсулы, кожа сползает с вас на пол!",
                &format!("Плоть {name} сползает отвратительной кучей!"),
                Span::BoldWarning,
            );
            crate::species::set_species(world, mob, SpeciesId::Skeleton);
            crate::inventory::refresh_look(world, mob);
            if let Some(&pos) = world.get::<TilePos>(mob) {
                crate::effects::gibs::spawn_gibs(
                    world,
                    crate::effects::gibs::Gibspawner::HumanBodypartless,
                    pos,
                );
            }
        }
        Fuckup::TotalFailure => {
            crate::medical::diseases::emote(world, mob, "scream");
            crate::medical::visible(
                world,
                mob,
                "Как только вы выходите из капсулы, ваша бескостная плоть оседает извивающейся \
                 кучей!",
                &format!("{name} оседает бескостной извивающейся кучей плоти!"),
                Span::BoldWarning,
            );
            let Some(&pos) = world.get::<TilePos>(mob) else {
                return;
            };
            if let Some(blob) = crate::mobs::spawn::spawn_mob(world, FLESHBLOB, pos) {
                world.entity_mut(blob).insert(Name::new(name));
                crate::medical::brainmob::transfer_mind(world, mob, blob);
            }
            crate::health::gib(world, mob);
        }
        _ => {}
    }
}

/// `finish_cloning`: the body, the mistake and the ghost poll.
fn finish_cloning(world: &mut World, pod: Entity) {
    let Some(record) = world.get::<ClonePod>(pod).and_then(|p| p.record.clone()) else {
        on_finished(world, pod);
        return;
    };
    let Some(&pos) = world.get::<TilePos>(pod) else {
        return;
    };
    let chance = world.resource::<FuckupChance>().0;
    let mut rng = crate::medical::fork_rng(world);
    let mistake = roll_mistake(chance, record.species, &mut rng);
    let clone = create_clone(world, &record, pos);
    crate::closets::put_mob_inside(world, clone, pod);
    if let Some(mistake) = mistake {
        apply_mistake(world, clone, mistake);
    }
    sound(
        world,
        pod,
        "sound/machines/microwave/microwave-end.ogg",
        100.0,
    );
    if let Some(mut p) = world.get_mut::<ClonePod>(pod) {
        p.clone = Some(clone);
    }
    let spec = crate::ghost_poll::PollSpec {
        question: format!("Вы хотите сыграть за клона {}?", record.name),
        role_name: format!("Клон {}", record.name),
        secs: POLL_SECS,
        ignore: Some(POLL_IGNORE),
        target: Some(clone),
        ..default()
    };
    let group = crate::ghost_poll::ghosts(world);
    crate::ghost_poll::poll_candidates(world, spec, group, move |world, mut signed_up| {
        // The body left, died or vanished meanwhile: [`timers`] has flushed it already.
        if !clone_waiting(world, pod, clone) {
            return;
        }
        let Some(chosen) = crate::ghost_poll::pick_candidate(world, &mut signed_up) else {
            fail_growing(world, pod);
            return;
        };
        if !crate::medical::brainmob::possess(world, chosen.client, chosen.mob, clone) {
            fail_growing(world, pod);
            return;
        }
        world.despawn(chosen.mob);
        let gain = crate::medical::fork_rng(world).range(1, 5) as f32;
        world.resource_mut::<FuckupChance>().0 += gain;
        if let Some(mut p) = world.get_mut::<ClonePod>(pod) {
            p.clone = None;
        }
        crate::closets::let_out(world, clone);
        tell_mob(
            world,
            clone,
            Span::BoldNotice,
            format!(
                "Вы — {}! Вы не совсем понимаете, где находитесь и как сюда попали.",
                record.name
            ),
        );
        if let Some(mistake) = mistake {
            post_emerged(world, clone, mistake);
        }
        on_finished(world, pod);
    });
}

/// The body still waits in its pod, alive (tg unregisters `on_clone_failed` only on success).
fn clone_waiting(world: &World, pod: Entity, clone: Entity) -> bool {
    world.get::<ClonePod>(pod).and_then(|p| p.clone) == Some(clone)
        && world
            .get::<InCloset>(clone)
            .is_some_and(|c| c.closet == pod)
        && world.get::<Stat>(clone).is_some_and(|s| *s != Stat::Dead)
}

/// The scan and growth timers, power loss, and a waiting body that left, died or vanished
/// (`on_clone_failed`).
// Cost: per tick one pass over the cloner machines (a few per map).
fn timers(world: &mut World) {
    let now = now(world);
    let scanners: Vec<(Entity, Option<f64>)> = world
        .query::<(Entity, &CloneScanner)>()
        .iter(world)
        .map(|(e, s)| (e, s.scan_until))
        .collect();
    for (scanner, until) in scanners {
        let Some(until) = until else {
            continue;
        };
        if !pods::operational(world, scanner) {
            fail_scan(world, scanner);
        } else if now >= until {
            complete_scan(world, scanner);
        }
    }
    let pods: Vec<(Entity, f64, Option<Entity>)> = world
        .query::<(Entity, &ClonePod)>()
        .iter(world)
        .filter(|(_, p)| p.running)
        .map(|(e, p)| (e, p.until, p.clone))
        .collect();
    for (pod, until, clone) in pods {
        if !pods::operational(world, pod) {
            fail_growing(world, pod);
            continue;
        }
        match clone {
            Some(clone) => {
                if !clone_waiting(world, pod, clone) {
                    fail_growing(world, pod);
                }
            }
            None if now >= until => finish_cloning(world, pod),
            None => {}
        }
    }
}

/// `ui_data`.
fn compute_view(world: &mut World, console: Entity) -> ExperimentalClonerData {
    let Some(state) = world.get::<CloneConsole>(console).cloned() else {
        return default();
    };
    let species_name = |id: SpeciesId| id.def().name.to_owned();
    let mut data = ExperimentalClonerData {
        record_name: state.record.as_ref().map(|r| r.name.clone()),
        record_species: state.record.as_ref().map(|r| species_name(r.species)),
        ..default()
    };
    if let Some(scanner) = state
        .scanner
        .filter(|&s| world.get::<CloneScanner>(s).is_some())
    {
        data.linked_scanner = true;
        data.is_scanning = world
            .get::<CloneScanner>(scanner)
            .is_some_and(|s| s.scan_until.is_some());
        if let Some(mob) = occupant(world, scanner) {
            data.scanner_occupant = Some(name_of(world, mob));
            data.scanner_species = world
                .get::<Dna>(mob)
                .map(|_| species_name(crate::species::species_of(world, mob)));
        }
    }
    if let Some(pod) = state.pod.and_then(|p| world.get::<ClonePod>(p)).cloned() {
        data.linked_pod = true;
        data.is_cloning = pod.running;
        if let Some(record) = &pod.record {
            data.cloning_name = Some(record.name.clone());
            data.cloning_species = Some(species_name(record.species));
        }
        if pod.running {
            let left = if pod.clone.is_some() {
                0.0
            } else {
                (pod.until - now(world)).max(0.0)
            };
            data.cloning_progress = ((CLONING_SECS - left) / CLONING_SECS * 100.0) as f32;
        }
    }
    data
}

fn refresh_view(world: &mut World, console: Entity) {
    let view = compute_view(world, console);
    if let Some(mut c) = world.get_mut::<CloneConsole>(console)
        && c.view != view
    {
        c.view = view;
    }
}

/// Consoles with an open window get fresh data.
// Cost: per tick, one pass over the open windows.
fn refresh_views(world: &mut World) {
    let mut q = world.query::<&ss13_tgui::server::UiWindow>();
    let mut consoles: Vec<Entity> = q
        .iter(world)
        .filter(|w| w.interface == ClonerWindow::NAME)
        .map(|w| w.source)
        .collect();
    consoles.sort();
    consoles.dedup();
    for console in consoles {
        refresh_view(world, console);
    }
}

/// The server side of the `ExperimentalCloner` interface.
pub struct ClonerWindow;

impl UiInterface for ClonerWindow {
    const NAME: &'static str = ExperimentalClonerUi::NAME;
    type Data = ExperimentalClonerData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClonerAct {
    ClearRecord,
    StartScan,
    StartClone,
}

impl ServerUi for ClonerWindow {
    type Source = &'static CloneConsole;
    type UserParam = ();
    type Act = ClonerAct;

    fn static_data(_: &CloneConsole) {}

    fn data(console: &CloneConsole, _: Entity, _: &()) -> ExperimentalClonerData {
        console.view.clone()
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<ClonerAct, UiReject> {
        Ok(match action {
            ACT_CLEAR_RECORD => ClonerAct::ClearRecord,
            ACT_START_SCAN => ClonerAct::StartScan,
            ACT_START_CLONE => ClonerAct::StartClone,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn apply_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ClonerWindow>>>) {
    let pending: Vec<(Entity, Entity, ClonerAct)> = acts
        .read(world.resource::<Messages<UiActed<ClonerWindow>>>())
        .map(|a| (a.source, a.actor, a.act))
        .collect();
    for (console, user, act) in pending {
        if world.get::<CloneConsole>(console).is_none() {
            continue;
        }
        ui_act(world, console, user, act);
        refresh_view(world, console);
    }
}

/// `ui_act`: each action checks the machines' state at the moment of effect.
pub fn ui_act(world: &mut World, console: Entity, user: Entity, act: ClonerAct) {
    let Some(state) = world.get::<CloneConsole>(console).cloned() else {
        return;
    };
    let refuse = |world: &mut World, text: &str| {
        balloon(world, user, console, text);
        if let Some(&pos) = world.get::<TilePos>(console) {
            playsound(world, PlaySound::new(pos, BUZZ, 50.0, true));
        }
    };
    match act {
        ClonerAct::ClearRecord => {
            if state.record.is_none() {
                return;
            }
            if let Some(mut c) = world.get_mut::<CloneConsole>(console) {
                c.record = None;
            }
            sound(world, console, "sound/machines/ping.ogg", 100.0);
        }
        ClonerAct::StartScan => {
            let Some(scanner) = state
                .scanner
                .filter(|&s| world.get::<CloneScanner>(s).is_some())
            else {
                refuse(world, "сканер не подключён!");
                return;
            };
            let Some(mob) = occupant(world, scanner) else {
                refuse(world, "сканер пуст!");
                return;
            };
            if world.get::<Dna>(mob).is_none() {
                refuse(world, "неподходящий субъект!");
                return;
            }
            if world
                .get::<CloneScanner>(scanner)
                .is_some_and(|s| s.scan_until.is_some())
            {
                refuse(world, "сканер занят!");
                return;
            }
            info!("{user} irradiated {mob} via experimental clone scan");
            start_scan(world, scanner);
        }
        ClonerAct::StartClone => {
            let Some(pod) = state.pod.filter(|&p| world.get::<ClonePod>(p).is_some()) else {
                refuse(world, "капсула не подключена!");
                return;
            };
            if world.get::<ClonePod>(pod).is_some_and(|p| p.running) {
                refuse(world, "капсула занята!");
                return;
            }
            let Some(record) = state.record else {
                refuse(world, "нет сохранённой ДНК!");
                return;
            };
            info!(
                "{user} began growing an experimental clone of {}",
                record.name
            );
            start_cloning(world, pod, record);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mistakes_follow_weights_and_species() {
        let mut rng = crate::combat::CombatRng::default();
        assert_eq!(roll_mistake(0.0, SpeciesId::Human, &mut rng), None);
        let mut seen = Vec::new();
        for _ in 0..400 {
            let m = roll_mistake(100.0, SpeciesId::Felinid, &mut rng).unwrap();
            assert_ne!(m, Fuckup::Felinise);
            if !seen.contains(&m) {
                seen.push(m);
            }
        }
        assert_eq!(seen.len(), Fuckup::ALL.len() - 1);
        assert!(Fuckup::Felinise.is_valid(SpeciesId::Human));
        assert_eq!(Fuckup::Monkey.weight(), 1);
        assert_eq!(Fuckup::Mutate.weight(), 3);
    }

    fn wait(world: &mut World, secs: u64) {
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(secs));
        timers(world);
    }

    #[test]
    fn a_scan_records_the_occupant_and_the_pod_grows_it() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let pos = TilePos::new(2, 2, 1);
        let scanner = world
            .spawn((Pod { open: false }, CloneScanner::default(), pos))
            .id();
        let pod = world.spawn((ClonePod::default(), pos)).id();
        let console = world
            .spawn(CloneConsole {
                scanner: Some(scanner),
                pod: Some(pod),
                ..default()
            })
            .id();
        let user = world.spawn(Name::new("Geneticist")).id();
        // Nothing to clone yet, nobody to scan.
        ui_act(&mut world, console, user, ClonerAct::StartClone);
        assert!(!world.get::<ClonePod>(pod).unwrap().running);
        ui_act(&mut world, console, user, ClonerAct::StartScan);
        assert!(
            world
                .get::<CloneScanner>(scanner)
                .unwrap()
                .scan_until
                .is_none()
        );
        let dna = Dna {
            unique_enzymes: "e".into(),
            unique_identity: "i".into(),
            mutation_index: Vec::new(),
            default_mutation_genes: Vec::new(),
            mutations: Vec::new(),
            stability: 40,
            melt_until: None,
        };
        let subject = world.spawn((Name::new("Bob"), dna, pos)).id();
        crate::closets::put_mob_inside(&mut world, subject, scanner);
        ui_act(&mut world, console, user, ClonerAct::StartScan);
        assert!(world.get::<CloneScanner>(scanner).unwrap().locked);
        let view = compute_view(&mut world, console);
        assert!(view.is_scanning && view.linked_pod);
        assert_eq!(view.scanner_occupant.as_deref(), Some("Bob"));
        wait(&mut world, 5);
        assert!(world.get::<CloneConsole>(console).unwrap().record.is_none());
        wait(&mut world, 6);
        let scanned = world.get::<CloneScanner>(scanner).copied().unwrap();
        assert!(!scanned.locked && scanned.scan_until.is_none());
        let record = world
            .get::<CloneConsole>(console)
            .unwrap()
            .record
            .clone()
            .unwrap();
        assert_eq!(record.name, "Bob");
        // `copy_dna`: the identity, stability reset, no mutations.
        assert_eq!(record.dna.as_ref().map(|d| d.stability), Some(100));
        ui_act(&mut world, console, user, ClonerAct::StartClone);
        assert!(world.get::<ClonePod>(pod).unwrap().running);
        wait(&mut world, 30);
        let view = compute_view(&mut world, console);
        assert!(view.is_cloning);
        assert_eq!(view.cloning_name.as_deref(), Some("Bob"));
        assert!((view.cloning_progress - 50.0).abs() < 1.0);
        // A second start while it runs is refused.
        ui_act(&mut world, console, user, ClonerAct::StartClone);
        assert!(world.get::<ClonePod>(pod).unwrap().running);
        ui_act(&mut world, console, user, ClonerAct::ClearRecord);
        assert!(world.get::<CloneConsole>(console).unwrap().record.is_none());
    }
}
