//! Air alarms (`code/modules/atmospherics/machinery/air_alarm/`): each samples the air of its
//! tile every SSair fire (tg `check_danger` on `COMSIG_TURF_EXPOSE`), rates it against its
//! thresholds (`/datum/tlv`), raises its area's atmos alarm, speaks a warning every 10 s, and
//! applies its mode (`/datum/air_alarm_mode`) to the vents and scrubbers of its area.
//!
//! Access: the interface opens locked; an ID with `req_access` (`atmos_equip`) toggles the
//! lock (click with the ID, or right click), as tg `togglelock`.
// shortcut: no wires, no emag (flood and vent siphon modes stay hidden), no construction
// stages, no air sensors (`link` helper), no fire alarm / fault status; add with wires and
// atmos control consoles.

use std::collections::HashSet;

use bevy::prelude::*;
use ss13_atmos::gas::ONE_ATMOSPHERE;
use ss13_atmos::{Atmos, GAS_COUNT, GasId, GasMixture};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmAtmosDevice, DmName};
use ss13_net::chat::server::SendChat;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;

use super::devices::{EXTERNAL_BOUND, INTERNAL_BOUND, VentPump, VentScrubber};
use super::{AreaAtmos, gas_mask};
use crate::power::Unpowered;

pub const AIR_ALARM: &str = "/obj/machinery/airalarm";
const HELPER: &str = "/obj/effect/mapping_helpers/airalarm";

/// `TLV_VALUE_IGNORE`.
pub const TLV_IGNORE: f64 = -1.0;
/// `AIR_ALARM_ALERT_*`.
pub const ALERT_NONE: u8 = 0;
pub const ALERT_WARNING: u8 = 1;
pub const ALERT_HAZARD: u8 = 2;
/// `TLV_VAR_*` bits.
pub const TLV_WARNING_MIN: u8 = 1;
pub const TLV_HAZARD_MIN: u8 = 2;
pub const TLV_WARNING_MAX: u8 = 4;
pub const TLV_HAZARD_MAX: u8 = 8;
/// `AIRALARM_WARNING_COOLDOWN`.
const WARNING_COOLDOWN: f64 = 10.0;
/// `BODYTEMP_COLD_WARNING_1`, `BODYTEMP_HEAT_WARNING_1`.
const BODYTEMP_COLD_WARNING_1: f64 = 310.15 - 40.0;
const BODYTEMP_HEAT_WARNING_1: f64 = 310.15 + 30.0;
/// `META_GAS_DANGER` (`cargo_flags & GAS_DANGEROUS`), CO2 aside: it has its own thresholds.
const DANGEROUS: [GasId; 13] = [
    GasId::PLASMA,
    GasId::N2O,
    GasId::NITRIUM,
    GasId::TRITIUM,
    GasId::BZ,
    GasId::MIASMA,
    GasId::FREON,
    GasId::HYDROGEN,
    GasId::HEALIUM,
    GasId::PROTO_NITRATE,
    GasId::ZAUKER,
    GasId::HALON,
    GasId::ANTINOBLIUM,
];

/// `/datum/tlv`: thresholds of one value; [`TLV_IGNORE`] disables a bound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tlv {
    pub warning_min: f64,
    pub hazard_min: f64,
    pub warning_max: f64,
    pub hazard_max: f64,
}

impl Tlv {
    pub const fn new(hazard_min: f64, warning_min: f64, warning_max: f64, hazard_max: f64) -> Self {
        Self {
            warning_min,
            hazard_min,
            warning_max,
            hazard_max,
        }
    }

    pub const NO_CHECKS: Self = Self::new(TLV_IGNORE, TLV_IGNORE, TLV_IGNORE, TLV_IGNORE);
    pub const DANGEROUS: Self = Self::new(TLV_IGNORE, TLV_IGNORE, 0.2, 0.5);
    pub const OXYGEN: Self = Self::new(16.0, 19.0, TLV_IGNORE, TLV_IGNORE);
    pub const CARBON_DIOXIDE: Self = Self::new(TLV_IGNORE, TLV_IGNORE, 5.0, 10.0);
    /// `WARNING_LOW_PRESSURE` .. `HAZARD_HIGH_PRESSURE`.
    pub const PRESSURE: Self = Self::new(20.0, 50.0, 325.0, 550.0);
    pub const TEMPERATURE: Self = Self::new(
        BODYTEMP_COLD_WARNING_1,
        BODYTEMP_COLD_WARNING_1 + 10.0,
        BODYTEMP_HEAT_WARNING_1 - 27.0,
        BODYTEMP_HEAT_WARNING_1,
    );
    pub const COLD_ROOM_PRESSURE: Self = Self::new(
        ONE_ATMOSPHERE * 0.8,
        ONE_ATMOSPHERE * 0.9,
        ONE_ATMOSPHERE * 1.1,
        ONE_ATMOSPHERE * 1.2,
    );
    pub const COLD_ROOM_TEMPERATURE: Self = Self::new(
        super::COLD_ROOM_TEMP - 40.0,
        super::COLD_ROOM_TEMP - 20.0,
        super::COLD_ROOM_TEMP + 20.0,
        super::COLD_ROOM_TEMP + 40.0,
    );
    pub const KITCHEN_TEMPERATURE: Self = Self::new(
        super::COLD_ROOM_TEMP - 40.0,
        super::COLD_ROOM_TEMP - 20.0,
        BODYTEMP_HEAT_WARNING_1 - 27.0,
        BODYTEMP_HEAT_WARNING_1,
    );

    /// `check_value`.
    pub fn check(&self, value: f64) -> u8 {
        let set = |bound: f64| bound != TLV_IGNORE;
        if set(self.hazard_max) && value >= self.hazard_max
            || set(self.hazard_min) && value <= self.hazard_min
        {
            ALERT_HAZARD
        } else if set(self.warning_max) && value >= self.warning_max
            || set(self.warning_min) && value <= self.warning_min
        {
            ALERT_WARNING
        } else {
            ALERT_NONE
        }
    }

    /// `set_value(threshold_type, value)`.
    pub fn set(&mut self, bits: u8, value: f64) {
        if bits & TLV_WARNING_MIN != 0 {
            self.warning_min = value;
        }
        if bits & TLV_HAZARD_MIN != 0 {
            self.hazard_min = value;
        }
        if bits & TLV_WARNING_MAX != 0 {
            self.warning_max = value;
        }
        if bits & TLV_HAZARD_MAX != 0 {
            self.hazard_max = value;
        }
    }

    /// `reset_value(threshold_type)` against the type's initial values.
    pub fn reset(&mut self, bits: u8, initial: &Tlv) {
        for (bit, value) in [
            (TLV_WARNING_MIN, initial.warning_min),
            (TLV_HAZARD_MIN, initial.hazard_min),
            (TLV_WARNING_MAX, initial.warning_max),
            (TLV_HAZARD_MAX, initial.hazard_max),
        ] {
            self.set(bits & bit, value);
        }
    }
}

/// `tlv_collection` defaults per gas.
pub fn default_gas_tlv(gas: GasId) -> Tlv {
    if gas == GasId::O2 {
        Tlv::OXYGEN
    } else if gas == GasId::CO2 {
        Tlv::CARBON_DIOXIDE
    } else if DANGEROUS.contains(&gas) {
        Tlv::DANGEROUS
    } else {
        Tlv::NO_CHECKS
    }
}

/// `/datum/air_alarm_mode` subtypes, in tg's `subtypesof` order (path order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlarmMode {
    Contaminated,
    Cycle,
    Draught,
    Filtering,
    Flood,
    Off,
    PanicSiphon,
    Refill,
    Siphon,
    VentSiphon,
}

impl AlarmMode {
    pub const ALL: [AlarmMode; 10] = [
        Self::Contaminated,
        Self::Cycle,
        Self::Draught,
        Self::Filtering,
        Self::Flood,
        Self::Off,
        Self::PanicSiphon,
        Self::Refill,
        Self::Siphon,
        Self::VentSiphon,
    ];

    /// `(path, name, desc, danger, emag)`.
    pub fn info(self) -> (&'static str, &'static str, &'static str, bool, bool) {
        match self {
            Self::Filtering => (
                "/datum/air_alarm_mode/filtering",
                "Filtering",
                "Scrubs out contaminants",
                false,
                false,
            ),
            Self::Contaminated => (
                "/datum/air_alarm_mode/contaminated",
                "Contaminated",
                "Scrubs out ALL contaminants quickly",
                false,
                false,
            ),
            Self::Draught => (
                "/datum/air_alarm_mode/draught",
                "Draught",
                "Siphons out air while replacing",
                false,
                false,
            ),
            Self::Refill => (
                "/datum/air_alarm_mode/refill",
                "Refill",
                "Triple vent output",
                true,
                false,
            ),
            Self::Cycle => (
                "/datum/air_alarm_mode/cycle",
                "Cycle",
                "Siphons air before replacing",
                true,
                false,
            ),
            Self::Siphon => (
                "/datum/air_alarm_mode/siphon",
                "Siphon",
                "Siphons air out of the room",
                true,
                false,
            ),
            Self::PanicSiphon => (
                "/datum/air_alarm_mode/panic_siphon",
                "Panic Siphon",
                "Siphons air out of the room quickly",
                true,
                false,
            ),
            Self::Off => (
                "/datum/air_alarm_mode/off",
                "Off",
                "Shuts off vents and scrubbers",
                false,
                false,
            ),
            Self::Flood => (
                "/datum/air_alarm_mode/flood",
                "Flood",
                "Shuts off scrubbers and opens vents",
                true,
                true,
            ),
            Self::VentSiphon => (
                "/datum/air_alarm_mode/vent_siphon",
                "Vent siphon",
                "Disables scrubbers and turns vents to siphon",
                true,
                true,
            ),
        }
    }

    pub fn path(self) -> &'static str {
        self.info().0
    }

    pub fn from_path(path: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.path() == path)
    }
}

/// Every gas but O2, N2 and pluoxium (the contaminated mode's filters).
fn contaminants() -> u32 {
    let all = (1u32 << GAS_COUNT) - 1;
    all & !(1 << GasId::O2.0) & !(1 << GasId::N2.0) & !(1 << GasId::PLUOXIUM.0)
}

fn vent_release(vent: &mut VentPump, bound: f64) {
    vent.on = true;
    vent.pressure_checks = EXTERNAL_BOUND;
    vent.external_pressure_bound = bound;
    vent.releasing = true;
}

fn scrub_co2(scrubber: &mut VentScrubber) {
    scrubber.on = true;
    scrubber.filter = gas_mask(&["co2".to_owned()]);
    scrubber.scrubbing = true;
    scrubber.widenet = false;
}

/// `apply(area)` on the area's vents and scrubbers.
pub fn apply_mode<'a>(
    mode: AlarmMode,
    vents: impl IntoIterator<Item = &'a mut VentPump>,
    scrubbers: impl IntoIterator<Item = &'a mut VentScrubber>,
) {
    for vent in vents {
        match mode {
            AlarmMode::Filtering | AlarmMode::Contaminated => vent_release(vent, ONE_ATMOSPHERE),
            AlarmMode::Draught => vent_release(vent, ONE_ATMOSPHERE * 2.0),
            AlarmMode::Refill => vent_release(vent, ONE_ATMOSPHERE * 3.0),
            AlarmMode::Cycle | AlarmMode::Siphon | AlarmMode::PanicSiphon | AlarmMode::Off => {
                vent.on = false;
            }
            AlarmMode::Flood => {
                vent.on = true;
                vent.pressure_checks = INTERNAL_BOUND;
                vent.internal_pressure_bound = 0.0;
                vent.releasing = true;
            }
            AlarmMode::VentSiphon => {
                vent.on = true;
                vent.pressure_checks = 0;
                vent.internal_pressure_bound = 0.0;
                vent.external_pressure_bound = 0.0;
                vent.releasing = false;
            }
        }
    }
    for scrubber in scrubbers {
        match mode {
            AlarmMode::Filtering | AlarmMode::Refill => scrub_co2(scrubber),
            AlarmMode::Contaminated => {
                scrubber.on = true;
                scrubber.filter = contaminants();
                scrubber.scrubbing = true;
                scrubber.widenet = true;
            }
            AlarmMode::Draught | AlarmMode::Siphon => {
                scrubber.on = true;
                scrubber.widenet = false;
                scrubber.scrubbing = false;
            }
            AlarmMode::Cycle | AlarmMode::PanicSiphon => {
                scrubber.on = true;
                scrubber.widenet = true;
                scrubber.scrubbing = false;
            }
            AlarmMode::Off | AlarmMode::Flood | AlarmMode::VentSiphon => scrubber.on = false,
        }
    }
}

/// `/obj/machinery/airalarm`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct AirAlarm {
    /// `my_area`.
    pub area: Option<Entity>,
    pub mode: AlarmMode,
    pub danger_level: u8,
    /// `area_danger`: an atmos alarm is active in the area.
    pub area_danger: bool,
    pub locked: bool,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    pub pressure: Tlv,
    pub temperature: Tlv,
    pub gases: [Tlv; GAS_COUNT],
    /// The `tlv_collection` an alarm starts with (`reset_value`).
    pub initial: (Tlv, Tlv),
    pub warning: Option<&'static str>,
    pub next_warning: f64,
    /// Total moles and temperature last rated: the same air is not rated again (tg rates on
    /// `COMSIG_TURF_EXPOSE`, i.e. only while the tile's air changes).
    sampled: Option<(f64, f64)>,
}

impl AirAlarm {
    pub fn new(area: Option<Entity>, dm: &DmAtmosDevice) -> Self {
        Self {
            area,
            mode: AlarmMode::Filtering,
            danger_level: ALERT_NONE,
            area_danger: false,
            locked: dm.locked,
            req_access: dm.req_access.clone(),
            req_one_access: dm.req_one_access.clone(),
            pressure: Tlv::PRESSURE,
            temperature: Tlv::TEMPERATURE,
            gases: std::array::from_fn(|g| default_gas_tlv(GasId(g as u8))),
            initial: (Tlv::PRESSURE, Tlv::TEMPERATURE),
            warning: None,
            next_warning: 0.0,
            sampled: None,
        }
    }

    /// Rate the air again on the next fire (`check_enviroment` after a threshold change).
    pub fn resample(&mut self) {
        self.sampled = None;
    }

    /// The thresholds of `tlv_collection[id]`: `"pressure"`, `"temperature"` or a gas id.
    pub fn tlv_mut(&mut self, id: &str) -> Option<&mut Tlv> {
        match id {
            "pressure" => Some(&mut self.pressure),
            "temperature" => Some(&mut self.temperature),
            gas => GasId::parse(gas).map(|g| &mut self.gases[usize::from(g.0)]),
        }
    }

    /// The type's thresholds for `reset_threshold`.
    pub fn initial_tlv(&self, id: &str) -> Option<Tlv> {
        match id {
            "pressure" => Some(self.initial.0),
            "temperature" => Some(self.initial.1),
            gas => GasId::parse(gas).map(default_gas_tlv),
        }
    }

    /// `check_danger` minus the side effects: the alert level of an air sample and the
    /// warning tg would speak.
    pub fn rate(&self, env: &GasMixture) -> (u8, Option<&'static str>) {
        let total = env.total_moles();
        let pressure = env.pressure();
        let temp = env.temperature;
        let mut danger = self
            .pressure
            .check(pressure)
            .max(self.temperature.check(temp));
        if total > 0.0 {
            for (g, tlv) in self.gases.iter().enumerate() {
                danger = danger.max(tlv.check(pressure * env.moles[g] / total));
            }
        }
        if danger == ALERT_NONE {
            return (danger, None);
        }
        let hazard = |tlv: &Tlv, max: bool, v: f64| {
            if max {
                tlv.hazard_max != TLV_IGNORE && v >= tlv.hazard_max
            } else {
                tlv.hazard_min != TLV_IGNORE && v <= tlv.hazard_min
            }
        };
        let high_p = hazard(&self.pressure, true, pressure);
        let low_p = hazard(&self.pressure, false, pressure);
        let high_t = hazard(&self.temperature, true, temp);
        let low_t = hazard(&self.temperature, false, temp);
        let warning = match (low_p, high_p, low_t, high_t) {
            (true, _, true, _) => Some("Тревога! Низкое давление и температура."),
            (true, _, _, true) => Some("Тревога! Низкое давление и высокая температура."),
            (_, true, _, true) => Some("Тревога! Высокое давление и температура."),
            (_, true, true, _) => Some("Тревога! Высокое давление и низкая температура."),
            (true, ..) => Some("Тревога! Низкое давление."),
            (_, true, ..) => Some("Тревога! Высокое давление."),
            (_, _, true, _) => Some("Тревога! Низкая температура."),
            (_, _, _, true) => Some("Тревога! Высокая температура."),
            _ => None,
        };
        (danger, warning)
    }
}

/// Air alarms from the map, with their mapping helpers applied.
pub fn init_alarms(world: &mut World) {
    let mut query = world.query::<(Entity, &DmAtmosDevice, &TilePos)>();
    let mut alarms = Vec::new();
    let mut helpers = Vec::new();
    for (e, dm, &pos) in query.iter(world) {
        if is_under(&dm.path, AIR_ALARM) {
            alarms.push((e, dm.clone(), pos));
        } else if is_under(&dm.path, HELPER) {
            helpers.push((e, dm.path.clone(), pos));
        }
    }
    let count = alarms.len();
    for (e, dm, pos) in alarms {
        let area = world.resource::<Areas>().area_at(pos);
        let mut alarm = AirAlarm::new(area, &dm);
        for (_, path, _) in helpers.iter().filter(|(_, _, p)| *p == pos) {
            let helper = path.strip_prefix(HELPER).unwrap_or_default();
            match helper {
                "/unlocked" => alarm.locked = false,
                "/engine_access" | "/mixingchamber_access" => {
                    alarm.locked = false;
                    alarm.req_access.clear();
                    alarm.req_one_access = vec![
                        "atmospherics".into(),
                        if helper == "/engine_access" {
                            "engineering".into()
                        } else {
                            "ordnance".into()
                        },
                    ];
                }
                "/all_access" => {
                    alarm.locked = false;
                    alarm.req_access.clear();
                    alarm.req_one_access.clear();
                }
                "/syndicate_access" => alarm.req_access = vec!["syndicate".into()],
                "/away_general_access" => alarm.req_access = vec!["away_general".into()],
                "/tlv_cold_room" => {
                    alarm.temperature = Tlv::COLD_ROOM_TEMPERATURE;
                    alarm.pressure = Tlv::COLD_ROOM_PRESSURE;
                    alarm.initial = (Tlv::COLD_ROOM_PRESSURE, Tlv::COLD_ROOM_TEMPERATURE);
                }
                "/tlv_kitchen" => {
                    alarm.temperature = Tlv::KITCHEN_TEMPERATURE;
                    alarm.initial.1 = Tlv::KITCHEN_TEMPERATURE;
                }
                "/tlv_no_checks" => {
                    alarm.pressure = Tlv::NO_CHECKS;
                    alarm.temperature = Tlv::NO_CHECKS;
                    alarm.gases = [Tlv::NO_CHECKS; GAS_COUNT];
                    alarm.initial = (Tlv::NO_CHECKS, Tlv::NO_CHECKS);
                }
                // shortcut: `link` (air sensors) and `surgery` do nothing yet.
                _ => {}
            }
        }
        world.entity_mut(e).insert(alarm);
    }
    for (e, ..) in helpers {
        world.despawn(e);
    }
    world.resource_mut::<super::AtmosMachineryReport>().alarms = count;
}

/// Words the alarms speak this fire, for [`speak_alarms`].
#[derive(Resource, Default)]
pub struct AlarmSpeech(pub Vec<(Entity, &'static str)>);

/// Every fire: `check_danger` of every powered alarm, the area alarm flags, the warning
/// speech and `selected_mode.replace` (the cycle mode refills below 5% of an atmosphere).
pub fn process_alarms(
    atmos: Res<Atmos>,
    time: Res<Time>,
    area_atmos: Res<AreaAtmos>,
    mut alarms: Query<(Entity, &TilePos, &mut AirAlarm, Has<Unpowered>)>,
    mut vents: Query<&mut VentPump>,
    mut scrubbers: Query<&mut VentScrubber>,
    mut speech: Local<Vec<(Entity, &'static str)>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    let mut alarmed: HashSet<Entity> = HashSet::new();
    let mut refill = Vec::new();
    for (e, &pos, mut alarm, unpowered) in &mut alarms {
        if unpowered {
            continue;
        }
        let Some(env) = atmos.air(pos) else { continue };
        let sample = Some((env.total_moles(), env.temperature));
        if alarm.sampled != sample {
            alarm.sampled = sample;
            let (danger, warning) = alarm.rate(env);
            if alarm.danger_level != danger {
                alarm.danger_level = danger;
            }
            if alarm.warning != warning {
                alarm.warning = warning;
            }
        }
        let (danger, warning) = (alarm.danger_level, alarm.warning);
        if danger != ALERT_NONE
            && let Some(area) = alarm.area
        {
            alarmed.insert(area);
        }
        if let Some(text) = warning
            && now >= alarm.next_warning
        {
            alarm.next_warning = now + WARNING_COOLDOWN;
            speech.push((e, text));
        }
        if alarm.mode == AlarmMode::Cycle && env.pressure() < ONE_ATMOSPHERE * 0.05 {
            refill.extend(alarm.area);
        }
    }
    for (_, _, mut alarm, _) in &mut alarms {
        let danger = alarm.area.is_some_and(|a| alarmed.contains(&a));
        if alarm.area_danger != danger {
            alarm.area_danger = danger;
        }
    }
    // `cycle/replace`: back to filtering-like vents and CO2 scrubbing.
    for area in refill {
        let Some(devices) = area_atmos.0.get(&area) else {
            continue;
        };
        for &v in &devices.vents {
            if let Ok(mut vent) = vents.get_mut(v) {
                vent_release(&mut vent, ONE_ATMOSPHERE);
            }
        }
        for &s in &devices.scrubbers {
            if let Ok(mut scrubber) = scrubbers.get_mut(s) {
                scrub_co2(&mut scrubber);
            }
        }
    }
    if !speech.is_empty() {
        let words = std::mem::take(&mut *speech);
        commands.queue(move |world: &mut World| speak_alarms(world, words));
    }
}

/// tg `say()` range of a machine.
const SAY_RANGE: u32 = 7;

/// `speak(warning_message)`: to the players within view of each alarm.
fn speak_alarms(world: &mut World, words: Vec<(Entity, &'static str)>) {
    for (alarm, text) in words {
        let Some(&pos) = world.get::<TilePos>(alarm) else {
            continue;
        };
        let name = world
            .get::<DmName>(alarm)
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_else(|| "air alarm".to_owned());
        let mut query = world.query_filtered::<Entity, With<ClientToken>>();
        let clients: Vec<Entity> = query
            .iter(world)
            .filter(|&c| {
                let mob = world.resource::<crate::server::Players>().mob(c);
                mob.and_then(|m| world.get::<TilePos>(m))
                    .and_then(|p| p.chebyshev(pos))
                    .is_some_and(|d| d <= SAY_RANGE)
            })
            .collect();
        if clients.is_empty() {
            continue;
        }
        world.write_message(SendChat {
            clients,
            message: ss13_chat::server::say_message(&name, text),
        });
    }
}

/// `select_mode(mode)` + `apply(my_area)`.
pub fn select_mode(world: &mut World, alarm: Entity, mode: AlarmMode) {
    let Some(mut a) = world.get_mut::<AirAlarm>(alarm) else {
        return;
    };
    let (_, _, _, _, emag) = mode.info();
    if emag {
        return;
    }
    a.mode = mode;
    let Some(area) = a.area else { return };
    let devices = world
        .resource::<AreaAtmos>()
        .0
        .get(&area)
        .cloned()
        .unwrap_or_default();
    let mut vents: Vec<VentPump> = devices
        .vents
        .iter()
        .filter_map(|&v| world.get::<VentPump>(v).cloned())
        .collect();
    let mut scrubbers: Vec<VentScrubber> = devices
        .scrubbers
        .iter()
        .filter_map(|&s| world.get::<VentScrubber>(s).cloned())
        .collect();
    apply_mode(mode, vents.iter_mut(), scrubbers.iter_mut());
    for (&e, v) in devices.vents.iter().zip(vents) {
        world.entity_mut(e).insert(v);
    }
    for (&e, s) in devices.scrubbers.iter().zip(scrubbers) {
        world.entity_mut(e).insert(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_atmos::gas::T20C;

    fn air(text: &str) -> GasMixture {
        GasMixture::parse(text).unwrap()
    }

    #[test]
    fn thresholds_rate_the_air_like_tg() {
        let alarm = AirAlarm::new(None, &DmAtmosDevice::default());
        // Station air: 101 kPa, 21% O2 (21 kPa > 19), 293 K.
        assert_eq!(alarm.rate(&air("o2=22;n2=82;TEMP=293.15")).0, ALERT_NONE);
        // 40 kPa: under WARNING_LOW_PRESSURE (50), over HAZARD_LOW_PRESSURE (20); its O2 partial
        // pressure (~8.5 kPa) is under the oxygen hazard minimum (16): hazard.
        let thin = air("o2=8.7;n2=32.4;TEMP=293.15");
        assert_eq!(alarm.rate(&thin).0, ALERT_HAZARD);
        // Vacuum: hazard and the low pressure warning.
        let (danger, warning) = alarm.rate(&GasMixture::vacuum());
        assert_eq!(danger, ALERT_HAZARD);
        assert_eq!(warning, Some("Тревога! Низкое давление и температура."));
        // CO2 at 6 kPa partial pressure: warning (5), not hazard (10).
        let mut co2 = air("o2=22;n2=76;TEMP=293.15");
        co2.add(GasId::CO2, 6.0 * 2500.0 / (8.31 * T20C));
        assert_eq!(alarm.rate(&co2).0, ALERT_WARNING);
    }

    #[test]
    fn modes_set_vents_and_scrubbers() {
        let mut vent = VentPump {
            on: false,
            releasing: false,
            pressure_checks: 0,
            external_pressure_bound: 0.0,
            internal_pressure_bound: 0.0,
            welded: false,
            area: None,
        };
        let mut scrubber = VentScrubber {
            on: false,
            scrubbing: true,
            filter: 0,
            widenet: false,
            volume_rate: 200.0,
            welded: false,
            area: None,
        };
        apply_mode(AlarmMode::Refill, [&mut vent], [&mut scrubber]);
        assert!(vent.on && vent.releasing);
        assert_eq!(vent.external_pressure_bound, ONE_ATMOSPHERE * 3.0);
        assert_eq!(scrubber.filter, 1 << GasId::CO2.0);
        apply_mode(AlarmMode::PanicSiphon, [&mut vent], [&mut scrubber]);
        assert!(!vent.on);
        assert!(scrubber.on && !scrubber.scrubbing && scrubber.widenet);
        assert_eq!(
            AlarmMode::from_path("/datum/air_alarm_mode/panic_siphon"),
            Some(AlarmMode::PanicSiphon)
        );
        let mut tlv = Tlv::PRESSURE;
        tlv.set(TLV_HAZARD_MIN | TLV_WARNING_MIN, 10.0);
        assert_eq!((tlv.hazard_min, tlv.warning_min), (10.0, 10.0));
        tlv.reset(TLV_WARNING_MIN, &Tlv::PRESSURE);
        assert_eq!((tlv.hazard_min, tlv.warning_min), (10.0, 50.0));
    }
}
