//! The scanner gate (server), after tg `/obj/machinery/scanner_gate`
//! (`code/game/machinery/scanner_gate.dm`) and `/datum/wires/scanner_gate`.
//!
//! - A gate ([`ScannerGate`]) scans what enters its tile (`perform_scan`) when powered, anchored
//!   and with its panel shut: for a mindshield, a disease at or over the threshold, a gun, a
//!   warrant, a species (zombies including dormant ones) or nutrition. A false reading comes
//!   every so often (`base_false_beep`). A hit buzzes, says what it found and shows the alarm line
//!   for 2 s (buzzing once per 2 s); no hit shows the scan line for 1 s.
//! - An ID on it locks it with that card's access; a locked gate needs that access to unlock.
//!   An emag strips the lock for good. The window (`ScannerGate`) is closed while locked.
//! - Its wires pulse attached assemblies on a hit or a miss unless `WIRE_DISABLE` was pulsed.
// shortcut: no rotation with a wrench (`simple_rotation`); the scanning module tier's bonus to
// false beeps is fixed at tier 1 (the default parts; `RefreshParts`); no power draw per scan
// (`use_energy`); the warrant lookup uses the mob's real name like the turret's (tg: face/ID
// name); no `TRAIT_CONTRABAND_BLOCKER`. Upgrade each with its mechanic. The luxury shuttle's
// ticket field is `crate::shuttle_special`'s.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmItem, DmName, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::{Moved, SpatialIndex};

use super::*;
use crate::antag::zombie::ZombieInfection;
use crate::combat::CombatRng;
use crate::inventory::{IdCard, get_id, mob_access};
use crate::medical::diseases::{HIDDEN_SCANNER, Severity};
use crate::medical::organs::Slot;
use crate::power::Unpowered;
use crate::silicon::interact::has_silicon_access;
use crate::species::{SpeciesId, species_of};
use crate::tools::{now, tell};
use crate::wires::{WireAssemblies, Wires};

pub const SCANNER_GATE: &str = "/obj/machinery/scanner_gate";
const LUXURY: &str = "/obj/machinery/scanner_gate/luxury_shuttle";
const PRESET_GUNS: &str = "/obj/machinery/scanner_gate/preset_guns";
const GUN_ROOT: &str = "/obj/item/gun";
const DMI: &str = "icons/obj/machines/scangate.dmi";
const BUZZ: &str = "sound/machines/scanner/scanbuzz.ogg";
const SCAN_SFX: &str = "industrial_scan";
const ACCESS_WEAPONS: &str = "weapons";
const ACCESS_SECURITY: &str = "security";

/// `base_false_beep` minus the default parts' `minus_false_beep`.
const FALSE_BEEP: f32 = 4.0;
const BEEP_COOLDOWN: f64 = 2.0;
const ALARM_SECS: f64 = 2.0;
const SCAN_SECS: f64 = 1.0;
/// `MEDIUM_RANGE_SOUND_EXTRARANGE`.
const BUZZ_EXTRARANGE: f32 = -3.0;
const BUZZ_FALLOFF: f32 = 4.0;

/// The wire colours' `WIRE_ACCEPT`, `WIRE_DENY`.
pub const WIRE_ACCEPT: &str = "Scan Success";
pub const WIRE_DENY: &str = "Scan Fail";

/// `available_species`.
pub const GATE_SPECIES: [(&str, SpeciesId); 10] = [
    ("human", SpeciesId::Human),
    ("lizard", SpeciesId::Lizard),
    ("fly", SpeciesId::Fly),
    ("felinid", SpeciesId::Felinid),
    ("plasmaman", SpeciesId::Plasmaman),
    ("moth", SpeciesId::Moth),
    ("jelly", SpeciesId::Jelly),
    ("pod", SpeciesId::Pod),
    ("golem", SpeciesId::Golem),
    ("zombie", SpeciesId::Zombie),
];

/// `all_disease_thresholds`.
const THRESHOLDS: [Severity; 7] = [
    Severity::Positive,
    Severity::Nonthreat,
    Severity::Minor,
    Severity::Medium,
    Severity::Harmful,
    Severity::Dangerous,
    Severity::Biohazard,
];

/// `/obj/machinery/scanner_gate` live state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ScannerGate {
    pub locked: bool,
    pub req_access: Vec<String>,
    /// `scangate_mode`, one of [`GATE_MODES`].
    pub mode: &'static str,
    pub disease_threshold: Severity,
    pub species: SpeciesId,
    pub reverse: bool,
    pub nutrition: u32,
    /// `light_pass`, `light_fail`, `ignore_signals`.
    pub light_pass: bool,
    pub light_fail: bool,
    pub ignore_signals: bool,
    pub emagged: bool,
    /// `next_beep`.
    pub next_beep: f64,
    /// A timed scanline over the passive one: its state and when it ends.
    pub scanline: Option<(&'static str, f64)>,
}

impl ScannerGate {
    /// `Initialize` of `path`.
    pub fn of(path: &str) -> Self {
        let preset = is_under(path, PRESET_GUNS);
        Self {
            locked: preset,
            req_access: if preset {
                vec![ACCESS_SECURITY.to_owned()]
            } else {
                Vec::new()
            },
            mode: if preset { MODE_GUNS } else { MODE_OFF },
            disease_threshold: Severity::Minor,
            species: SpeciesId::Human,
            reverse: false,
            nutrition: NUTRITION_FAT,
            light_pass: false,
            light_fail: false,
            ignore_signals: false,
            emagged: false,
            next_beep: 0.0,
            scanline: None,
        }
    }
}

pub struct ScannerGatePlugin;

impl Plugin for ScannerGatePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<ScannerGateWindow>()
            .add_observer(add_gate)
            .add_systems(
                Update,
                (
                    (stepped_through, refresh_gates)
                        .after(crate::click::ResolveClicks)
                        .run_if(any_with_component::<ScannerGate>),
                    gate_acts.in_set(UiSystems::Handle),
                ),
            );
    }
}

fn add_gate(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if let Ok(obj) = objs.get(add.entity)
        && is_under(&obj.path, SCANNER_GATE)
        && !is_under(&obj.path, LUXURY)
    {
        commands
            .entity(add.entity)
            .try_insert_if_new(ScannerGate::of(&obj.path));
    }
}

/// tg `connect_loc` `COMSIG_ATOM_ENTERED` -> `auto_scan`.
// Cost: (steps this tick) x (things on the tile entered), a handful; only while a gate exists.
fn stepped_through(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moves: Vec<(Entity, TilePos)> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| (m.entity, m.to))
        .collect();
    for (mover, to) in moves {
        let gates: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(to)
            .iter()
            .copied()
            .filter(|&e| e != mover && world.get::<ScannerGate>(e).is_some())
            .collect();
        for gate in gates {
            if world.get::<Unpowered>(gate).is_none()
                && crate::obj_damage::anchored(world, gate)
                && !crate::construction::panel_open(world, gate)
            {
                perform_scan(world, gate, mover);
            }
        }
    }
}

/// What a scan found (`beep`, `detected_thing`, `bypassed`).
struct Scan {
    beep: bool,
    detected: Option<String>,
    bypassed: bool,
}

/// `isgun`.
fn is_gun(world: &World, e: Entity) -> bool {
    world
        .get::<DmItem>(e)
        .is_some_and(|i| is_under(&i.path, GUN_ROOT))
}

/// `get_all_contents` of a non-mob: everything stored inside, nested.
fn contents(world: &World, thing: Entity) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut pending = vec![thing];
    while let Some(e) = pending.pop() {
        if let Some(stored) = world.get::<crate::storage::Stored>(e) {
            for &item in stored.items() {
                out.push(item);
                pending.push(item);
            }
        }
    }
    out
}

/// `check_virus`: the worst disease a scanner can see.
fn worst_disease(world: &World, mob: Entity) -> Option<Severity> {
    crate::medical::diseases::of_mob(world, mob)
        .iter()
        .filter(|d| d.visibility & HIDDEN_SCANNER == 0)
        .map(|d| d.severity)
        .max()
}

/// `is_species`: a type or any of its subtypes.
fn is_species(actual: SpeciesId, wanted: SpeciesId) -> bool {
    is_under(actual.def().path, wanted.def().path)
}

/// The `switch(scangate_mode)` of `perform_scan`.
fn scan(world: &World, gate: &ScannerGate, thing: Entity) -> Scan {
    let human = crate::throw::is_carbon(world, thing);
    let mut out = Scan {
        beep: false,
        detected: None,
        bypassed: false,
    };
    match gate.mode {
        MODE_WANTED if human => {
            out.detected = Some("ордер на задержание".to_owned());
            let name = world
                .get::<Name>(thing)
                .map(|n| n.as_str().to_owned())
                .unwrap_or_default();
            let record = world
                .get_resource::<crate::records::manifest::Manifest>()
                .and_then(|m| m.find(&name));
            out.beep = record.is_none_or(|r| r.wanted_status == crate::records::WANTED_ARREST);
        }
        MODE_MINDSHIELD => {
            out.detected = Some("имплант защиты разума".to_owned());
            out.beep = human && crate::antag::revolution::server::mindshielded(world, thing);
        }
        MODE_DISEASE => {
            out.detected = Some(format!(
                "инфекцию уровня «{}»",
                gate.disease_threshold.text()
            ));
            out.beep =
                human && worst_disease(world, thing).is_some_and(|s| s >= gate.disease_threshold);
        }
        MODE_SPECIES if human => {
            let wanted = gate.species;
            out.detected = Some(wanted.def().name.to_owned());
            out.beep = is_species(species_of(world, thing), wanted);
            if wanted == SpeciesId::Zombie {
                out.detected = Some("вирус Ромерола".to_owned());
                out.beep |= crate::medical::organs::organ(world, thing, Slot::Zombie)
                    .is_some_and(|o| world.get::<ZombieInfection>(o).is_some());
            }
        }
        MODE_GUNS => {
            out.detected = Some("оружие".to_owned());
            if is_gun(world, thing) {
                out.beep = true;
            } else if human {
                let gun = crate::antag::objectives::all_contents(world, thing)
                    .into_iter()
                    .any(|i| is_gun(world, i));
                if gun {
                    // A mindshield or an ID with weapons access (the bartender's) passes.
                    let authorized = crate::antag::revolution::server::mindshielded(world, thing)
                        || mob_access(world, thing).iter().any(|a| a == ACCESS_WEAPONS);
                    out.beep = !authorized;
                    out.bypassed = authorized;
                }
            } else {
                out.beep = contents(world, thing).into_iter().any(|i| is_gun(world, i));
            }
        }
        MODE_NUTRITION if human => {
            if let Some(hunger) = world.get::<crate::hunger::Hunger>(thing) {
                if hunger.nutrition <= NUTRITION_STARVING as f32
                    && gate.nutrition == NUTRITION_STARVING
                {
                    out.beep = true;
                    out.detected = Some("голодание".to_owned());
                }
                if hunger.nutrition >= NUTRITION_FAT as f32 && gate.nutrition == NUTRITION_FAT {
                    out.beep = true;
                    out.detected = Some("ожирение".to_owned());
                }
            }
        }
        _ => {}
    }
    out
}

/// tg `perform_scan`.
fn perform_scan(world: &mut World, gate_e: Entity, thing: Entity) {
    let Some(gate) = world.get::<ScannerGate>(gate_e).cloned() else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(gate_e) else {
        return;
    };
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, SCAN_SFX, 20.0, true).extrarange(-2.0),
    );
    if gate.mode == MODE_OFF {
        return;
    }
    let found = scan(world, &gate, thing);
    let mut beep = found.beep ^ gate.reverse;
    let mut rng = world.resource_mut::<CombatRng>();
    if rng.chance(FALSE_BEEP) {
        beep = rng.chance(50.0);
    }
    if beep {
        alarm_beep(world, gate_e, pos, found.detected.as_deref(), gate.reverse);
        if !gate.ignore_signals {
            fire_wire(world, gate_e, WIRE_ACCEPT);
        }
    } else {
        if found.bypassed
            && let Some(thing) = &found.detected
        {
            crate::atmos_machinery::machine_say(
                world,
                gate_e,
                &format!("Отмена сканирования на {thing}."),
            );
        }
        if !gate.ignore_signals {
            fire_wire(world, gate_e, WIRE_DENY);
        }
        let until = now(world) + SCAN_SECS;
        set_scanline(world, gate_e, "scanning", until);
    }
}

/// `assembly?.activate()` of what is attached to `wire`.
fn fire_wire(world: &mut World, gate: Entity, wire: &str) {
    let Some(color) = world.get::<Wires>(gate).and_then(|w| w.color_of(wire)) else {
        return;
    };
    let Some(assembly) = world.get::<WireAssemblies>(gate).and_then(|a| a.at(color)) else {
        return;
    };
    crate::assemblies::server::activate(world, assembly);
}

/// tg `alarm_beep`: once per 2 s it says what it found and buzzes.
fn alarm_beep(
    world: &mut World,
    gate: Entity,
    pos: TilePos,
    detected: Option<&str>,
    reverse: bool,
) {
    let t_now = now(world);
    let Some(mut state) = world.get_mut::<ScannerGate>(gate) else {
        return;
    };
    if t_now < state.next_beep {
        return;
    }
    state.next_beep = t_now + BEEP_COOLDOWN;
    if let Some(detected) = detected {
        let text = format!(
            "Сканирование{}обнаружило {detected}!",
            if reverse { " не " } else { " " }
        );
        crate::atmos_machinery::machine_say(world, gate, &text);
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, BUZZ, 30.0, false)
            .extrarange(BUZZ_EXTRARANGE)
            .falloff_distance(BUZZ_FALLOFF),
    );
    set_scanline(world, gate, "alarm", t_now + ALARM_SECS);
}

fn set_scanline(world: &mut World, gate: Entity, state: &'static str, until: f64) {
    if let Some(mut g) = world.get_mut::<ScannerGate>(gate) {
        g.scanline = Some((state, until));
    }
}

/// The timed scanlines, and tg `update_icon_state` / `power_change`: the frame shows the panel,
/// the line (passive unless timed) only while powered.
// Cost: one pass over the gates (a handful on a map) per tick.
fn refresh_gates(world: &mut World) {
    let t_now = now(world);
    let gates: Vec<Entity> = world
        .query_filtered::<Entity, With<ScannerGate>>()
        .iter(world)
        .collect();
    for e in gates {
        let Some(mut g) = world.get_mut::<ScannerGate>(e) else {
            continue;
        };
        if g.scanline.is_some_and(|(_, until)| until <= t_now) {
            g.scanline = None;
        }
        let line = g.scanline.map_or("passive", |(state, _)| state);
        let powered = world.get::<Unpowered>(e).is_none();
        let frame = if crate::construction::panel_open(world, e) {
            "scangate_open"
        } else {
            "scangate"
        };
        let Some(icons) = world.get_resource::<IconIndex>() else {
            continue;
        };
        let frame = icons.get(DMI, frame);
        let line = powered.then(|| icons.get(DMI, line)).flatten();
        let Some(mut look) = world.get_mut::<Appearance>(e) else {
            continue;
        };
        if let Some(frame) = frame
            && look.icon_state != frame
        {
            look.icon_state = frame;
        }
        let wanted: Vec<Overlay> = line.map(Overlay::new).into_iter().collect();
        if look.overlays != wanted {
            look.overlays = wanted;
        }
    }
}

/// tg `can_interact`: a locked gate does not open. True when `target` is a gate.
pub fn open(world: &mut World, client: Entity, user: Entity, target: Entity) -> bool {
    let Some(gate) = world.get::<ScannerGate>(target) else {
        return false;
    };
    if gate.locked {
        return true;
    }
    let title = world.get::<DmName>(target).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    });
    world.write_message(OpenUi::new::<ScannerGateWindow>(
        client, user, target, title,
    ));
    true
}

fn allowed(world: &World, req: &[String], mob: Entity) -> bool {
    has_silicon_access(world, mob) || crate::inventory::id_allowed(world, mob, req, &[])
}

/// tg `item_interaction` (an ID locks or unlocks) and `emag_act`. True when the click was the
/// gate's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(gate) = world.get::<ScannerGate>(target).cloned() else {
        return false;
    };
    if crate::emag::is_emag(world, held) {
        if !gate.emagged && crate::emag::can_emag(world, mob, held, target) {
            if let Some(mut g) = world.get_mut::<ScannerGate>(target) {
                g.locked = false;
                g.req_access.clear();
                g.emagged = true;
            }
            tell(world, client, Span::Notice, "ID-сканер отключён.");
        }
        return true;
    }
    let Some(card) = get_id(world, held) else {
        return false;
    };
    if gate.locked {
        if allowed(world, &gate.req_access, mob) {
            if let Some(mut g) = world.get_mut::<ScannerGate>(target) {
                g.locked = false;
                g.req_access.clear();
            }
            tell(world, client, Span::Notice, "Разблокировано.");
        }
        return true;
    }
    if gate.emagged {
        tell(world, client, Span::Warning, "Ничего не происходит!");
        return true;
    }
    let access = world
        .get::<IdCard>(card)
        .map(|c| c.access.clone())
        .unwrap_or_default();
    if let Some(mut g) = world.get_mut::<ScannerGate>(target) {
        g.req_access = access;
        g.locked = true;
    }
    tell(world, client, Span::Notice, "Заблокировано.");
    true
}

// ---------------------------------------------------------------------------------------------
// Window

pub struct ScannerGateWindow;

impl UiInterface for ScannerGateWindow {
    const NAME: &'static str = ScannerGateUi::NAME;
    type Data = ScannerGateData;
    type StaticData = ScannerGateStatic;
}

/// A checked act of the window.
#[derive(Clone, Debug, PartialEq)]
pub enum GateAct {
    SetMode(&'static str),
    ToggleReverse,
    ToggleLock,
    SetDisease(Severity),
    SetSpecies(SpeciesId),
    SetNutrition(u32),
}

impl ServerUi for ScannerGateWindow {
    type Source = &'static ScannerGate;
    type UserParam = ();
    type Act = GateAct;

    fn static_data(_: &ScannerGate) -> ScannerGateStatic {
        ScannerGateStatic {
            available_species: GATE_SPECIES
                .iter()
                .map(|&(id, species)| GateSpecies {
                    specie_name: species.def().name.to_owned(),
                    specie_id: id.to_owned(),
                })
                .collect(),
        }
    }

    fn data(
        gate: &ScannerGate,
        _: Entity,
        _: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> ScannerGateData {
        let species_id = GATE_SPECIES
            .iter()
            .find(|(_, s)| *s == gate.species)
            .map_or("human", |(id, _)| *id);
        ScannerGateData {
            locked: gate.locked,
            scan_mode: gate.mode.to_owned(),
            reverse: gate.reverse,
            disease_threshold: gate.disease_threshold.text().to_owned(),
            target_species_id: species_id.to_owned(),
            target_nutrition: gate.nutrition,
            target_zombie: gate.species == SpeciesId::Zombie,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<GateAct, UiReject> {
        let bad = |what: &str| UiReject::BadParams(what.to_owned());
        match action {
            SET_MODE => {
                let wanted = params.get(NEW_MODE).ok_or_else(|| bad("new_mode"))?;
                GATE_MODES
                    .iter()
                    .find(|m| **m == wanted)
                    .map(|m| GateAct::SetMode(m))
                    .ok_or_else(|| bad("new_mode"))
            }
            TOGGLE_REVERSE => Ok(GateAct::ToggleReverse),
            TOGGLE_LOCK => Ok(GateAct::ToggleLock),
            SET_THRESHOLD => {
                let wanted = params
                    .get(NEW_THRESHOLD)
                    .ok_or_else(|| bad("new_threshold"))?;
                THRESHOLDS
                    .iter()
                    .find(|s| s.text() == wanted)
                    .map(|s| GateAct::SetDisease(*s))
                    .ok_or_else(|| bad("new_threshold"))
            }
            SET_SPECIES => {
                let wanted = params
                    .get(NEW_SPECIES_ID)
                    .ok_or_else(|| bad("new_species_id"))?;
                GATE_SPECIES
                    .iter()
                    .find(|(id, _)| *id == wanted)
                    .map(|(_, s)| GateAct::SetSpecies(*s))
                    .ok_or_else(|| bad("new_species_id"))
            }
            SET_NUTRITION => match params.get(NEW_NUTRITION) {
                Some(STARVING) => Ok(GateAct::SetNutrition(NUTRITION_STARVING)),
                Some(OBESE) => Ok(GateAct::SetNutrition(NUTRITION_FAT)),
                _ => Err(bad("new_nutrition")),
            },
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `ui_act`, at the moment of effect: not locked, powered; the lock needs `allowed`.
fn gate_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ScannerGateWindow>>>) {
    let pending: Vec<(Entity, Entity, GateAct)> = acts
        .read(world.resource::<Messages<UiActed<ScannerGateWindow>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, e, act) in pending {
        let Some(gate) = world.get::<ScannerGate>(e).cloned() else {
            continue;
        };
        if gate.locked || world.get::<Unpowered>(e).is_some() {
            continue;
        }
        let allowed_now = allowed(world, &gate.req_access, actor);
        let Some(mut g) = world.get_mut::<ScannerGate>(e) else {
            continue;
        };
        match act {
            GateAct::SetMode(mode) => g.mode = mode,
            GateAct::ToggleReverse => g.reverse = !g.reverse,
            GateAct::ToggleLock => {
                if allowed_now {
                    g.locked = !g.locked;
                }
            }
            GateAct::SetDisease(severity) => g.disease_threshold = severity,
            GateAct::SetSpecies(species) => g.species = species,
            GateAct::SetNutrition(level) => g.nutrition = level,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preset_starts_locked_on_guns() {
        let plain = ScannerGate::of(SCANNER_GATE);
        assert!(!plain.locked && plain.mode == MODE_OFF && plain.req_access.is_empty());
        let preset = ScannerGate::of(PRESET_GUNS);
        assert!(preset.locked);
        assert_eq!(preset.mode, MODE_GUNS);
        assert_eq!(preset.req_access, vec![ACCESS_SECURITY.to_owned()]);
    }

    #[test]
    fn act_params_are_checked_against_the_lists() {
        let good = [("new_mode".to_owned(), "Guns".to_owned())];
        assert_eq!(
            ScannerGateWindow::parse_act(SET_MODE, &UiParams::new(&good)).unwrap(),
            GateAct::SetMode(MODE_GUNS)
        );
        let bad = [("new_mode".to_owned(), "Bogus".to_owned())];
        assert!(ScannerGateWindow::parse_act(SET_MODE, &UiParams::new(&bad)).is_err());
        let species = [("new_species_id".to_owned(), "lizard".to_owned())];
        assert_eq!(
            ScannerGateWindow::parse_act(SET_SPECIES, &UiParams::new(&species)).unwrap(),
            GateAct::SetSpecies(SpeciesId::Lizard)
        );
        let nutrition = [("new_nutrition".to_owned(), STARVING.to_owned())];
        assert_eq!(
            ScannerGateWindow::parse_act(SET_NUTRITION, &UiParams::new(&nutrition)).unwrap(),
            GateAct::SetNutrition(NUTRITION_STARVING)
        );
    }

    #[test]
    fn species_match_their_subtypes() {
        assert!(is_species(SpeciesId::Felinid, SpeciesId::Human));
        assert!(is_species(SpeciesId::Ashwalker, SpeciesId::Lizard));
        assert!(!is_species(SpeciesId::Lizard, SpeciesId::Human));
    }
}
