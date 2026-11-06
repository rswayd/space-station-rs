//! Suit sensors and the crew monitoring console (server), after tg `/obj/item/clothing/under`
//! (`under/_under.dm`: `set_sensor_mode`, `break_sensors`, `repair_sensors`, the "Adjust Suit
//! Sensors" verb, `emp_act`, the cable coil repair) and `/datum/crewmonitor`
//! (`code/game/machinery/computer/crew.dm`, tgui `CrewConsole`).
//!
//! A jumpsuit with `has_sensor` gets [`SuitSensor`] when it is made, with a random mode when it
//! has `random_sensor` (tg's `pick`, weighted towards vitals). A right click on it in a hand or
//! worn (tg `attack_self_secondary`/`attack_hand_secondary` -> `toggle`) asks for the mode;
//! locked (prisoner), broken or missing sensors refuse. A heavy EMP breaks working sensors, a
//! light one scrambles the mode; a cable coil repairs broken ones.
//!
//! The console lists everyone on its z-level (or any station level, both being station levels)
//! whose worn jumpsuit has working sensors set to at least "alive/dead": the name and job from
//! the ID, life status, damage from "vitals", the area from "tracking"; refreshed every 10 s
//! (`SENSORS_UPDATE_PERIOD`), sorted by job as the window's default sort.
// shortcut: no AI tracking link (`select_person`), no sorting/search controls in the window, no
// sparks while broken (`sensor_malfunction`), no suit sensor module (`/obj/item/suit_sensor`,
// wirecutters), no strip menu sensor action, no ctrl-click to tracking, no
// `TRAIT_MULTIZ_SUIT_SENSORS`, BandaStation's NanoMap (`CrewConsole220`) is the plain tg list.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmClothing, DmItem, DmName, DmPower};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject};

use super::{CrewConsoleData, CrewConsoleUi, CrewSensor, CrewVitals};
use crate::health::{COMBAT_MESSAGE_RANGE, Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::power::Unpowered;
use crate::tools::tell;

/// `has_sensor` values.
pub const BROKEN_SENSORS: i32 = -1;
pub const NO_SENSORS: i32 = 0;
pub const HAS_SENSORS: i32 = 1;
pub const LOCKED_SENSORS: i32 = 2;
/// `sensor_mode` values.
pub const SENSOR_OFF: i32 = 0;
pub const SENSOR_LIVING: i32 = 1;
pub const SENSOR_VITALS: i32 = 2;
pub const SENSOR_COORDS: i32 = 3;
/// `GLOB.suit_sensor_mode_to_defines` (BandaStation's names), by mode.
pub const MODE_NAMES: [&str; 4] = [
    "Отключено",
    "\"Жив-мертв\"",
    "Жизненные показатели",
    "Слежение",
];
/// `/obj/item/clothing/under/Initialize`: `pick(VITALS, VITALS, VITALS, LIVING, LIVING, COORDS,
/// COORDS, OFF)`.
const RANDOM_MODES: [i32; 8] = [2, 2, 2, 1, 1, 3, 3, 0];
/// `emp_act` (light): `pick(OFF, OFF, OFF, LIVING, LIVING, VITALS, VITALS, COORDS)`.
const EMP_MODES: [i32; 8] = [0, 0, 0, 1, 1, 2, 2, 3];
pub const CREW_CONSOLE: &str = "/obj/machinery/computer/crew";
/// `SENSORS_UPDATE_PERIOD`, seconds.
const UPDATE_SECS: f64 = 10.0;
/// `UNKNOWN_JOB_ID`.
pub const UNKNOWN_JOB_ID: u32 = 998;
const CABLE: &str = "/obj/item/stack/cable_coil";

/// tg `has_sensor` and `sensor_mode` of a jumpsuit.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuitSensor {
    pub has: i32,
    pub mode: i32,
}

/// A crew monitoring console and its last sensor read (`GLOB.crewmonitor.update_data(z)`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct CrewConsole {
    pub sensors: Vec<CrewSensor>,
    updated: Option<f64>,
}

/// The jumpsuit a mode prompt is about.
#[derive(Component, Clone, Copy, Debug)]
struct SensorPrompt(Entity);

pub(super) fn build(app: &mut App) {
    app.register_ui::<CrewWindow>()
        .add_observer(add_sensor)
        .add_observer(add_console)
        .add_systems(
            Update,
            (
                mode_answers
                    .in_set(ss13_tgui::server::UiSystems::Handle)
                    .after(ss13_tgui::prompt::PromptSystems::Answer),
                sensor_emps.in_set(crate::emp::EmpSystems::Effects),
                refresh_consoles.run_if(any_with_component::<CrewConsole>),
            ),
        );
}

/// `/obj/item/clothing/under/Initialize`: the sensor and its first mode.
fn add_sensor(
    add: On<Add<DmClothing>>,
    clothing: Query<&DmClothing>,
    rng: Option<ResMut<crate::combat::CombatRng>>,
    mut commands: Commands,
) {
    let Ok(c) = clothing.get(add.entity) else {
        return;
    };
    if c.has_sensor == NO_SENSORS {
        return;
    }
    let mode = match rng {
        Some(mut rng) if c.random_sensor => RANDOM_MODES[rng.range(0, 7) as usize],
        _ => c.sensor_mode,
    };
    commands.entity(add.entity).insert_if_new(SuitSensor {
        has: c.has_sensor,
        mode,
    });
}

fn add_console(add: On<Add<DmPower>>, power: Query<&DmPower>, mut commands: Commands) {
    if power
        .get(add.entity)
        .is_ok_and(|p| is_under(&p.path, CREW_CONSOLE))
    {
        commands
            .entity(add.entity)
            .insert_if_new(CrewConsole::default());
    }
}

/// tg `can_toggle_sensors`: the toggler can act, the jumpsuit is theirs or next to them, the
/// sensors work and are not locked. Tells why not.
fn can_toggle(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(sensor) = world.get::<SuitSensor>(item).copied() else {
        return false;
    };
    let able = world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated;
    if !able {
        return false;
    }
    let near = world.get::<CarriedBy>(item).map(|c| c.0) == Some(mob)
        || crate::storage::in_reach(world, mob, item);
    if !near {
        tell(world, client, Span::Warning, "слишком далеко!");
        return false;
    }
    let refusal = match sensor.has {
        LOCKED_SENSORS => Some("управление датчиков заблокировано!"),
        BROKEN_SENSORS => Some("датчики коротят!"),
        NO_SENSORS => Some("отсутствуют датчики!"),
        _ => None,
    };
    if let Some(text) = refusal {
        tell(world, client, Span::Warning, text);
        return false;
    }
    true
}

/// The "Adjust Suit Sensors" verb from a right click on a jumpsuit (in a hand or worn): asks
/// for the mode. False when `item` has no sensors at all.
pub fn toggle_prompt(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(sensor) = world.get::<SuitSensor>(item).copied() else {
        return false;
    };
    if !can_toggle(world, client, mob, item) {
        return true;
    }
    let current = MODE_NAMES
        .get(sensor.mode.clamp(0, 3) as usize)
        .map(|s| (*s).to_owned());
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Медицинские датчики",
            "Выберите режим датчиков",
            PromptKind::List {
                items: MODE_NAMES.iter().map(|s| (*s).to_owned()).collect(),
                default: current,
            },
        ),
    );
    world.entity_mut(prompt).insert(SensorPrompt(item));
    true
}

/// The chosen mode, checked again (`can_toggle_sensors` after the input).
fn mode_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(&SensorPrompt(item)) = world.get::<SensorPrompt>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        if index >= MODE_NAMES.len() || !can_toggle(world, client, mob, item) {
            continue;
        }
        set_mode(world, item, index as i32);
        if world.get::<CarriedBy>(item).map(|c| c.0) == Some(mob) {
            let text = match index as i32 {
                SENSOR_OFF => "Вы отключаете датчики на комбинезоне.",
                SENSOR_LIVING => {
                    "Ваш комбинезон теперь будет сообщать только о том, живы вы или мертвы."
                }
                SENSOR_VITALS => {
                    "Ваш комбинезон теперь будет сообщать только о ваших жизненных показателях."
                }
                _ => {
                    "Ваш комбинезон теперь будет сообщать о ваших жизненных показателях и ваши текущие координаты."
                }
            };
            tell(world, client, Span::Notice, text);
        }
    }
}

/// tg `set_sensor_mode`.
pub fn set_mode(world: &mut World, item: Entity, mode: i32) {
    if let Some(mut sensor) = world.get_mut::<SuitSensor>(item) {
        let mode = mode.clamp(SENSOR_OFF, SENSOR_COORDS);
        if sensor.mode != mode {
            sensor.mode = mode;
        }
    }
}

/// tg `break_sensors`: working sensors short out.
pub fn break_sensors(world: &mut World, item: Entity) {
    let Some(sensor) = world.get::<SuitSensor>(item).copied() else {
        return;
    };
    if matches!(sensor.has, BROKEN_SENSORS | NO_SENSORS) {
        return;
    }
    world.entity_mut(item).insert(SuitSensor {
        has: BROKEN_SENSORS,
        ..sensor
    });
    let name = crate::security::name_of(world, item);
    if let Some(mob) = world.get::<CarriedBy>(item).map(|c| c.0) {
        let text = format!("Медицинские датчики на {name} коротят!");
        crate::medical::visible_range(
            world,
            mob,
            &text,
            &text,
            Span::Warning,
            COMBAT_MESSAGE_RANGE,
        );
    }
}

/// tg `repair_sensors` without a user (a pristine `update_clothes_damaged_state`).
pub fn repair_broken(world: &mut World, item: Entity) -> bool {
    let Some(mut sensor) = world.get_mut::<SuitSensor>(item) else {
        return false;
    };
    if sensor.has != BROKEN_SENSORS {
        return false;
    }
    sensor.has = HAS_SENSORS;
    true
}

/// tg `/obj/item/clothing/under/item_interaction` with a cable coil: one piece repairs broken
/// sensors. False for other items or targets.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let cable = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, CABLE));
    let Some(sensor) = world.get::<SuitSensor>(target).copied() else {
        return false;
    };
    if !cable {
        return false;
    }
    if sensor.has != BROKEN_SENSORS {
        let text = if sensor.has == NO_SENSORS {
            "датчики отсутствуют!"
        } else {
            "датчики не сломаны!"
        };
        tell(world, client, Span::Warning, text);
        return true;
    }
    if !crate::tools::use_stack(world, held, 1) {
        return true;
    }
    repair_broken(world, target);
    let (user, name) = (
        crate::security::name_of(world, mob),
        crate::security::name_of(world, target),
    );
    crate::medical::visible(
        world,
        mob,
        &format!("Вы чините датчики на {name} проводами."),
        &format!("{user} чинит датчики на {name} проводами."),
        Span::Notice,
    );
    true
}

/// tg `/obj/item/clothing/under/emp_act`: a heavy pulse breaks the sensors, a light one
/// scrambles the mode.
fn sensor_emps(world: &mut World, mut cursor: Local<MessageCursor<crate::emp::EmpAct>>) {
    for ev in crate::emp::emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let Some(sensor) = world.get::<SuitSensor>(ev.target).copied() else {
            continue;
        };
        if matches!(sensor.has, NO_SENSORS | BROKEN_SENSORS) {
            continue;
        }
        if ev.severity <= crate::emp::EMP_HEAVY {
            break_sensors(world, ev.target);
            continue;
        }
        let pick = world
            .resource_mut::<crate::emp::EmpRng>()
            .range(0, EMP_MODES.len() as i64 - 1) as usize;
        set_mode(world, ev.target, EMP_MODES[pick]);
        let name = crate::security::name_of(world, ev.target);
        if let Some(mob) = world.get::<CarriedBy>(ev.target).map(|c| c.0) {
            let text = format!("Медицинские датчики на {name} быстро мигают и меняются!");
            crate::medical::visible_range(
                world,
                mob,
                &text,
                &text,
                Span::Warning,
                COMBAT_MESSAGE_RANGE,
            );
        }
    }
}

/// The worn jumpsuit's sensor of `mob`, when it reports at least "alive/dead" (tg
/// `GLOB.suit_sensors_list` membership: `has_sensor >= HAS_SENSORS`, `sensor_mode >=
/// SENSOR_LIVING`).
pub fn reporting(world: &World, mob: Entity) -> Option<SuitSensor> {
    let uniform = world.get::<Inventory>(mob)?.uniform?;
    world
        .get::<SuitSensor>(uniform)
        .copied()
        .filter(|s| s.has >= HAS_SENSORS && s.mode >= SENSOR_LIVING)
}

/// tg `/obj/item/pinpointer/crew/trackable` without the level check: the worn jumpsuit has
/// sensors (`has_sensor`, broken ones too: tg tests it as a truthy value) on tracking, or any mode
/// with `ignore_level`.
pub fn trackable(world: &World, mob: Entity, ignore_level: bool) -> bool {
    world
        .get::<Inventory>(mob)
        .and_then(|i| i.uniform)
        .and_then(|u| world.get::<SuitSensor>(u))
        .is_some_and(|s| s.has != NO_SENSORS && (s.mode >= SENSOR_COORDS || ignore_level))
}

/// tg `/datum/crewmonitor/var/jobs`: the sort ids of the station's jobs (heads at multiples of
/// 10), by job title.
pub fn job_id(title: &str) -> u32 {
    match title {
        "Captain" => 0,
        "Big Brother" => 1,
        "Head of Security" => 10,
        "Warden" => 11,
        "Security Officer" => 12,
        "Security Officer (Medical)" => 13,
        "Security Officer (Engineering)" => 14,
        "Security Officer (Science)" => 15,
        "Security Officer (Cargo)" => 16,
        "Detective" => 17,
        "Chief Medical Officer" => 20,
        "Chemist" => 21,
        "Medical Doctor" => 22,
        "Paramedic" => 23,
        "Coroner" => 24,
        "Research Director" => 30,
        "Scientist" => 31,
        "Roboticist" => 32,
        "Geneticist" => 33,
        "Chief Engineer" => 40,
        "Station Engineer" => 41,
        "Atmospheric Technician" => 42,
        "Quartermaster" => 50,
        "Shaft Miner" => 51,
        "Cargo Technician" => 52,
        "Bitrunner" => 53,
        "Head of Personnel" => 60,
        "Bartender" => 61,
        "Chef" => 62,
        "Cook" => 63,
        "Botanist" => 64,
        "Curator" => 65,
        "Chaplain" => 66,
        "Clown" => 67,
        "Mime" => 68,
        "Janitor" => 69,
        "Lawyer" => 71,
        "Psychologist" => 72,
        // BandaStation: sorted with the other civilians.
        "Nanotrasen Representative" | "Blueshield" | "Magistrate" => 0,
        "Assistant" => 999,
        _ => UNKNOWN_JOB_ID,
    }
}

/// tg `stat` numbers the window reads (`CONSCIOUS` 0 .. `DEAD` 4).
fn stat_number(world: &World, mob: Entity) -> u8 {
    match world.get::<Stat>(mob).copied().unwrap_or_default() {
        Stat::Dead => 4,
        Stat::HardCrit => 3,
        _ if crate::status_effects::knocked_out(world, mob) => 2,
        Stat::SoftCrit => 1,
        Stat::Conscious => 0,
    }
}

/// tg `/datum/crewmonitor/update_data(z)`.
pub fn update_data(world: &mut World, z: u16) -> Vec<CrewSensor> {
    let station = crate::events::controller::station_levels(world);
    let mut mobs = world.query::<(Entity, &TilePos, &Health)>();
    let candidates: Vec<(Entity, TilePos)> =
        mobs.iter(world).map(|(e, &pos, _)| (e, pos)).collect();
    let mut out = Vec::new();
    for (mob, pos) in candidates {
        let Some(sensor) = reporting(world, mob) else {
            continue;
        };
        if pos.z != z && !(station.contains(&pos.z) && station.contains(&z)) {
            continue;
        }
        let card = world.get::<Inventory>(mob).and_then(|i| {
            [i.id, i.hands[i.active], i.hands[1 - i.active]]
                .into_iter()
                .flatten()
                .find_map(|item| crate::inventory::get_id(world, item))
        });
        let registration = card.and_then(|c| world.get::<crate::outfit::IdRegistration>(c));
        let name = registration
            .map(|r| r.registered_name.clone())
            .unwrap_or_else(|| "Unknown".to_owned());
        let assignment = registration.map(|r| r.assignment.clone());
        let ijob = registration.map_or(UNKNOWN_JOB_ID, |r| job_id(&r.job));
        let stat = stat_number(world, mob);
        let life_status = if sensor.mode >= SENSOR_VITALS {
            stat
        } else if stat == 4 {
            4
        } else {
            0
        };
        let vitals = (sensor.mode >= SENSOR_VITALS)
            .then(|| world.get::<Health>(mob))
            .flatten()
            .map(|h| CrewVitals {
                oxy: h.oxy.round() as i32,
                tox: h.tox.round() as i32,
                burn: h.burn.round() as i32,
                brute: h.brute.round() as i32,
                health: h.health().round() as i32,
            });
        let area = (sensor.mode >= SENSOR_COORDS)
            .then(|| crate::forensics::capitalize(&crate::bots::area_name(world, pos)));
        out.push(CrewSensor {
            key: mob.to_bits(),
            name,
            assignment,
            ijob,
            life_status,
            vitals,
            area,
        });
    }
    // The window's default sort: by job id (a stable sort keeps the list's order otherwise).
    out.sort_by_key(|s| s.ijob);
    out
}

/// Every console (and handheld monitor, `crate::devices`) reads the sensors again every
/// [`UPDATE_SECS`], for the level it is on.
fn refresh_consoles(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &CrewConsole)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.updated.is_none_or(|t| now - t >= UPDATE_SECS))
        .map(|(e, _)| e)
        .collect();
    let due: Vec<(Entity, u16)> = due
        .into_iter()
        .filter_map(|e| Some((e, crate::modpc::pda::turf_of(world, e)?.z)))
        .collect();
    refresh(world, due, now);
}

fn refresh(world: &mut World, consoles: Vec<(Entity, u16)>, now: f64) {
    let mut by_z: Vec<(u16, Vec<CrewSensor>)> = Vec::new();
    for (console, z) in consoles {
        let sensors = match by_z.iter().find(|(at, _)| *at == z) {
            Some((_, s)) => s.clone(),
            None => {
                let s = update_data(world, z);
                by_z.push((z, s.clone()));
                s
            }
        };
        if let Some(mut c) = world.get_mut::<CrewConsole>(console) {
            c.updated = Some(now);
            if c.sensors != sensors {
                c.sensors = sensors;
            }
        }
    }
}

/// A click on a powered crew console opens the monitor (tg `ui_interact` ->
/// `GLOB.crewmonitor.show`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    // A handheld monitor (`crate::devices`) carries a `CrewConsole` too: it is picked up.
    if world.get::<CrewConsole>(target).is_none() || world.get::<DmPower>(target).is_none() {
        return false;
    }
    if world.get::<Unpowered>(target).is_none() {
        show(world, client, mob, target, false);
    }
    true
}

/// `GLOB.crewmonitor.show(user, source)` for a console or a handheld monitor: the window, with
/// a sensor read first when `fresh` or the source never read them.
pub fn show(world: &mut World, client: Entity, mob: Entity, target: Entity, fresh: bool) {
    let never = world
        .get::<CrewConsole>(target)
        .is_some_and(|c| c.updated.is_none());
    if (fresh || never)
        && let Some(pos) = crate::modpc::pda::turf_of(world, target)
    {
        let now = world.resource::<Time>().elapsed_secs_f64();
        refresh(world, vec![(target, pos.z)], now);
    }
    let title = world.get::<DmName>(target).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    });
    world.write_message(OpenUi::new::<CrewWindow>(client, mob, target, title));
}

/// The server half of [`CrewConsoleUi`].
pub struct CrewWindow;

impl UiInterface for CrewWindow {
    const NAME: &'static str = CrewConsoleUi::NAME;
    type Data = CrewConsoleData;
    type StaticData = ();
}

impl ServerUi for CrewWindow {
    type Source = &'static CrewConsole;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &CrewConsole) {}

    fn data(console: &CrewConsole, _: Entity, _: &()) -> CrewConsoleData {
        CrewConsoleData {
            sensors: console.sensors.clone(),
            link_allowed: false,
        }
    }

    /// `select_person` is the AI's tracking link, which is not ported.
    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// tg `/obj/item/clothing/under/examine`: the sensors' state.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(sensor) = world.get::<SuitSensor>(item) else {
        if world
            .get::<DmItem>(item)
            .is_some_and(|i| is_under(&i.path, "/obj/item/clothing/under"))
        {
            lines.push((Span::Plain, "Отсутствуют медицинские датчики.".to_owned()));
        }
        return;
    };
    let line = match (sensor.has, sensor.mode) {
        (BROKEN_SENSORS, _) => (
            Span::Warning,
            "Похоже, медицинские датчики закоротили. Вы можете починить их с помощью проводов.",
        ),
        (_, SENSOR_OFF) => (Span::Plain, "Датчики, похоже, отключены."),
        (_, SENSOR_LIVING) => (Span::Plain, "Датчики \"жив-мертв\", похоже, включены."),
        (_, SENSOR_VITALS) => (
            Span::Plain,
            "Датчики жизненных показателей, похоже, включены.",
        ),
        _ => (
            Span::Plain,
            "Датчики жизненных показателей и маячок слежения, похоже, включены.",
        ),
    };
    lines.push((line.0, line.1.to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heads_sort_first_in_their_department() {
        assert_eq!(job_id("Captain"), 0);
        assert_eq!(job_id("Head of Security") % 10, 0);
        assert_eq!(job_id("Medical Doctor"), 22);
        assert_eq!(job_id("Assistant"), 999);
        assert_eq!(job_id("Clown Prince"), UNKNOWN_JOB_ID);
    }

    #[test]
    fn only_working_sensors_on_report() {
        let mut world = World::new();
        let suit = |world: &mut World, has, mode| world.spawn(SuitSensor { has, mode }).id();
        let cases = [
            (HAS_SENSORS, SENSOR_VITALS, true),
            (LOCKED_SENSORS, SENSOR_COORDS, true),
            (HAS_SENSORS, SENSOR_OFF, false),
            (BROKEN_SENSORS, SENSOR_COORDS, false),
        ];
        for (has, mode, reports) in cases {
            let uniform = suit(&mut world, has, mode);
            let mob = world
                .spawn(Inventory {
                    uniform: Some(uniform),
                    ..default()
                })
                .id();
            assert_eq!(reporting(&world, mob).is_some(), reports, "{has} {mode}");
            assert_eq!(
                trackable(&world, mob, false),
                mode == SENSOR_COORDS,
                "{has} {mode}"
            );
        }
    }

    #[test]
    fn breaking_and_repairing() {
        let mut world = World::new();
        let uniform = world
            .spawn(SuitSensor {
                has: HAS_SENSORS,
                mode: SENSOR_COORDS,
            })
            .id();
        break_sensors(&mut world, uniform);
        assert_eq!(
            world.get::<SuitSensor>(uniform).unwrap().has,
            BROKEN_SENSORS
        );
        assert!(repair_broken(&mut world, uniform));
        assert_eq!(world.get::<SuitSensor>(uniform).unwrap().has, HAS_SENSORS);
        assert!(!repair_broken(&mut world, uniform));
        set_mode(&mut world, uniform, 7);
        assert_eq!(
            world.get::<SuitSensor>(uniform).unwrap().mode,
            SENSOR_COORDS
        );
    }
}
