//! Breathing, pressure and body temperature of humans (server), after tg `Life()`:
//! `mob/living/life.dm`, `carbon/life.dm` (`handle_breathing`, `breathe`), the lungs organ
//! (`surgery/organs/internal/lungs/_lungs.dm` `check_breath`), `human/life.dm` and the species'
//! `handle_environment_pressure` / `handle_body_temperature` (`human/_species.dm`).
//!
//! Every `SSmobs` fire ([`SSMOBS_WAIT_SECS`], `seconds_per_tick` 2) each living mob:
//! - breathes on every 4th fire, or every fire after a failed breath: soft/hard crit miss
//!   breaths (`losebreath`), otherwise the breath is [`BREATH_VOLUME`] litres from the open
//!   internals tank (`crate::internals`) or [`BREATH_PERCENTAGE`] of the tile's moles; the lungs
//!   check O2 (suffocation oxyloss), CO2, plasma and N2O against their thresholds, exchange O2
//!   for CO2, and the breath goes back to the tile;
//! - takes pressure damage below [`HAZARD_LOW_PRESSURE`] and above [`HAZARD_HIGH_PRESSURE`]
//!   unless clothing with `STOPSPRESSUREDAMAGE` covers it;
//! - moves its skin and core temperature towards the air and its normal temperature (clothing
//!   insulation, the space suit's thermal regulator), and burns above/below the damage limits.
//!
//! The tg screen alerts it throws ([`Alerts`]) reach the owner as [`AlertHudUi`].
//! A z-level without simulated air (`ss13_atmos` built none: fixtures without
//! `initial_gas_mix`) is breathed as standard station air and never changes.

use bevy::prelude::*;
use ss13_atmos::gas::{CELL_VOLUME, R_IDEAL_GAS_EQUATION, TCMB, byond_round};
use ss13_atmos::{Atmos, GAS_COUNT, GasId, GasMixture};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmClothing, DmItem, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject, UiSystems};

use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, HealthSystems, Stat, update_stat};
use crate::hud::{AlertHudData, AlertHudUi, HudAlert};
use crate::internals::{
    Internals, STOPSPRESSUREDAMAGE, TankAir, can_breathe_internals, cutoff_message,
};
use crate::inventory::Inventory;
use crate::server::PlayerMob;

/// `SSmobs` `wait`: how often `Life()` runs; `seconds_per_tick` of every fire.
pub const SSMOBS_WAIT_SECS: f64 = 2.0;
/// `BREATH_VOLUME` litres, `BREATH_PERCENTAGE` of a turf's moles.
pub const BREATH_VOLUME: f64 = 1.99;
pub const BREATH_PERCENTAGE: f64 = BREATH_VOLUME / CELL_VOLUME;
/// `SUFFOCATION_OXYLOSS`, `SUFFOCATION_OXYLOSS_CRIT_MODIFIER`.
pub const SUFFOCATION_OXYLOSS: f32 = 3.0;
pub const SUFFOCATION_OXYLOSS_CRIT_MODIFIER: f32 = 0.22;

/// `/obj/item/organ/lungs` thresholds, kPa of the breath.
pub const SAFE_OXYGEN_MIN: f64 = 16.0;
pub const SAFE_CO2_MAX: f64 = 10.0;
pub const SAFE_PLASMA_MAX: f64 = 0.05;
pub const N2O_DETECT_MIN: f64 = 0.08;
pub const N2O_PARA_MIN: f64 = 1.0;
/// `MIN_TOXIC_GAS_DAMAGE`, `MAX_TOXIC_GAS_DAMAGE`.
pub const MIN_TOXIC_GAS_DAMAGE: f64 = 1.0;
pub const MAX_TOXIC_GAS_DAMAGE: f64 = 10.0;
/// `PLUOXIUM_PROPORTION`: lungs read a mole of pluoxium as 8 of oxygen.
const PLUOXIUM_PROPORTION: f64 = 8.0;

/// Pressure limits (`atmos_mob_interaction.dm`), kPa.
pub const HAZARD_HIGH_PRESSURE: f64 = 550.0;
pub const WARNING_HIGH_PRESSURE: f64 = 325.0;
pub const WARNING_LOW_PRESSURE: f64 = 50.0;
pub const HAZARD_LOW_PRESSURE: f64 = 20.0;
pub const ONE_ATMOSPHERE: f64 = 101.325;
pub const PRESSURE_DAMAGE_COEFFICIENT: f64 = 2.0;
pub const MAX_HIGH_PRESSURE_DAMAGE: f64 = 2.0;
pub const BASE_LOW_PRESSURE_DAMAGE: f64 = 2.0;
pub const MAX_LOW_PRESSURE_DAMAGE: f64 = BASE_LOW_PRESSURE_DAMAGE * 2.5;

/// Body temperature (`atmos_mob_interaction.dm`), kelvin.
pub const BODYTEMP_NORMAL: f64 = 310.15;
const BODYTEMP_AUTORECOVERY_DIVISOR: f64 = 28.0;
const BODYTEMP_STANDARD_CHANGE_RATE: f64 = 0.06;
const BODYTEMP_CORE_CHANGE_RATE: f64 = BODYTEMP_STANDARD_CHANGE_RATE * 0.25;
const BODYTEMP_SKIN_CORE_CHANGE_RATE: f64 = BODYTEMP_STANDARD_CHANGE_RATE * 0.65;
const BODYTEMP_CORE_SKIN_CHANGE_RATE: f64 = BODYTEMP_STANDARD_CHANGE_RATE * 0.75;
const BODYTEMP_AREA_SKIN_CHANGE_RATE: f64 = BODYTEMP_STANDARD_CHANGE_RATE * 0.85;
const BODYTEMP_SUIT_CHANGE_RATE: f64 = BODYTEMP_STANDARD_CHANGE_RATE * 1.35;
pub const BODYTEMP_HEAT_DAMAGE_LIMIT: f64 = BODYTEMP_NORMAL + 30.0;
pub const BODYTEMP_COLD_DAMAGE_LIMIT: f64 = BODYTEMP_NORMAL - 40.0;
const BODYTEMP_HEAT_WARNING_2: f64 = BODYTEMP_HEAT_DAMAGE_LIMIT + 120.0;
const BODYTEMP_HEAT_WARNING_3: f64 = BODYTEMP_HEAT_DAMAGE_LIMIT + 360.0;
const BODYTEMP_COLD_WARNING_2: f64 = BODYTEMP_COLD_DAMAGE_LIMIT - 70.0;
const BODYTEMP_COLD_WARNING_3: f64 = BODYTEMP_COLD_DAMAGE_LIMIT - 150.0;
/// `COLD_DAMAGE_LEVEL_1..3` (`mobs.dm`), burn per second.
const COLD_DAMAGE_LEVEL: [f64; 3] = [0.25, 0.75, 1.5];
/// `SPACE_SUIT_MIN_TEMP_PROTECT`: a space suit's cold protection with its regulator on.
const SPACE_SUIT_MIN_TEMP_PROTECT: f32 = 2.0;
const SPACE_SUIT: &str = "/obj/item/clothing/suit/space";
/// `THERMAL_PROTECTION_*` (`human/life.dm`) by body part bit (`CHEST` = bit 0 .. `HAND_RIGHT`).
const THERMAL_PROTECTION: [f64; 11] = [
    0.15, 0.15, 0.3, 0.075, 0.075, 0.025, 0.025, 0.075, 0.075, 0.025, 0.025,
];
/// Body part bits (`code/__DEFINES/inventory.dm`).
const CHEST: u32 = 1 << 0;
const HEAD: u32 = 1 << 2;
/// `HANDS` (`HAND_LEFT | HAND_RIGHT`).
const HANDS: u32 = (1 << 9) | (1 << 10);

/// tg alert categories this module throws (`code/__DEFINES/alerts.dm`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlertCategory {
    NotEnoughOxygen,
    /// `ALERT_NOT_ENOUGH_PLASMA` (plasmaman lungs).
    NotEnoughPlasma,
    TooMuchCo2,
    TooMuchPlasma,
    TooMuchN2o,
    Pressure,
    Temperature,
    Fire,
    /// `ALERT_CHARGE` (`crate::mecha`: the exosuit's cell).
    Charge,
    /// `ALERT_MECH_DAMAGE`.
    MechDamage,
}

/// One thrown alert: its type's base `icon_state` and `severity` (0: none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Alert {
    pub category: AlertCategory,
    pub state: &'static str,
    pub severity: u8,
}

impl Alert {
    /// tg `throw_alert`: `"[initial(icon_state)][severity]"` when there is a severity.
    pub fn icon_state(&self) -> String {
        if self.severity > 0 {
            format!("{}{}", self.state, self.severity)
        } else {
            self.state.to_owned()
        }
    }

    /// `name` and `desc` of the alert type (`screen_objects/alert.dm`, BandaStation).
    pub fn text(&self) -> (&'static str, &'static str) {
        match self.state {
            "not_enough_oxy" => (
                "Удушье (Нехватка O2)",
                "Недостаток кислорода. Используйте средства защиты дыхания или найдите источник \
                 O2.",
            ),
            "not_enough_plas" => (
                "Удушье (Нехватка плазмы)",
                "Недостаток плазмы. Используйте средства защиты дыхания или найдите источник \
                 плазмы.",
            ),
            "too_much_co2" => (
                "Удушье (CO2)",
                "Воздух перенасыщен углекислым газом! Используйте средства защиты дыхания или \
                 найдите место с воздухом.",
            ),
            "too_much_plas" => (
                "Удушье (Плазма)",
                "Воздух отравлен плазмой! Используйте средства защиты дыхания или найдите место с \
                 воздухом.",
            ),
            "too_much_n2o" => (
                "Удушье (N2O)",
                "Воздух перенасыщен оксидом азота! Используйте средства защиты дыхания или найдите \
                 место с воздухом.",
            ),
            "highpressure" => (
                "Высокое давление",
                "Воздух вокруг опасно плотный. Противопожарный костюм защитит вас.",
            ),
            "lowpressure" => (
                "Низкое давление",
                "Воздух вокруг опасно разрежен. Скафандр защитит вас.",
            ),
            "hot" => (
                "Слишком горячо",
                "Ваше тело нагрето до предела! Найдите место попрохладнее и снимите любую \
                 изолирующую одежду, например, пожарный костюм.",
            ),
            "fire" => (
                "В огне",
                "Вы горите! Катайтесь по полу, используйте огнетушитель или выйдите в пространство \
                 с вакуумом.",
            ),
            "cold" => (
                "Слишком холодно",
                "Вы ужасно замерзли! Найдите что-нибудь потеплее и снимите любую изолирующую \
                 одежду, например, скафандр.",
            ),
            "low_cell" => ("Низкий заряд", "Ядро юнита на низком заряде."),
            "empty_cell" => (
                "Без питания",
                "Ядро юнита разряжено. Модули недоступны, пока батарея не будет заряжена.",
            ),
            "no_cell" => (
                "Отсутствует батарея",
                "Батарея юнита отсутствует. Модули недоступны, пока батарея не будет установлена. \
                 Робототехника может оказать помощь.",
            ),
            "low_mech_integrity" => ("Мех повреждён", "Низкая целостность меха."),
            _ => ("Alert", ""),
        }
    }
}

/// tg `mob.alerts`, in throw order.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Alerts(pub Vec<Alert>);

impl Alerts {
    /// tg `throw_alert`: the same type at the same severity (or without one) stays; anything
    /// else replaces the category's alert, at the end of the list.
    pub fn throw(&mut self, category: AlertCategory, state: &'static str, severity: u8) {
        if let Some(i) = self.0.iter().position(|a| a.category == category) {
            let old = self.0[i];
            if old.state == state && (severity == 0 || severity == old.severity) {
                return;
            }
            self.0.remove(i);
        }
        self.0.push(Alert {
            category,
            state,
            severity,
        });
    }

    pub fn clear(&mut self, category: AlertCategory) {
        self.0.retain(|a| a.category != category);
    }

    pub fn get(&self, category: AlertCategory) -> Option<&Alert> {
        self.0.iter().find(|a| a.category == category)
    }
}

/// What a `Life()` tick makes the mob do or read, besides damage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LifeEvent {
    /// An involuntary emote seen around (`ss13_chat::emote` key).
    Emote(&'static str),
    /// `to_chat` to the mob's own player.
    Chat(Span, &'static str),
}

/// What one `Life()` reads and writes besides the mob's [`Respiration`].
pub struct LifeCtx<'a> {
    pub health: &'a mut Health,
    /// The stat when the step started (tg reads it at the moment of each check).
    pub stat: Stat,
    pub alerts: &'a mut Alerts,
    pub rng: &'a mut CombatRng,
    /// Game time, seconds.
    pub now: f64,
    pub events: Vec<LifeEvent>,
}

impl LifeCtx<'_> {
    fn damage(&mut self, kind: DamageType, amount: f64) {
        self.health.adjust(kind, amount as f32);
    }

    fn chance(&mut self, percent: f64) -> bool {
        self.rng.chance(percent as f32)
    }
}

/// The breathing and temperature state of a carbon mob (tg vars on `/mob/living/carbon`,
/// `/mob/living/carbon/human` and its lungs).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Respiration {
    /// tg `losebreath`: missed breaths owed.
    pub losebreath: f32,
    /// tg `failed_last_breath`: breathe again next `Life()` instead of every 4th.
    pub failed_last_breath: bool,
    /// Lungs `last_partial_pressures`, kPa by gas.
    pub last_pp: [f64; GAS_COUNT],
    /// tg `co2overloadtime`, game seconds (0: not breathing too much CO2).
    pub co2_overload_since: f64,
    /// tg `bodytemperature` (the skin), `coretemperature`, `old_bodytemperature`, kelvin.
    pub bodytemperature: f64,
    pub coretemperature: f64,
    pub old_bodytemperature: f64,
    /// tg `seconds_in_low_pressure`.
    pub seconds_in_low_pressure: f64,
    /// What the body's species and lungs make of breathing and temperature, refreshed every
    /// `Life()` ([`BodyLife::of`]).
    pub body: BodyLife,
}

/// The species vars and traits `Life()` reads (`crate::species`) and the lungs' gas thresholds;
/// the default is a human with human lungs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyLife {
    /// `bodytemp_normal`, `bodytemp_heat_damage_limit`, `bodytemp_cold_damage_limit`.
    pub normal: f64,
    pub heat_limit: f64,
    pub cold_limit: f64,
    /// `heatmod`, `coldmod` (`PHYS_COEFF_HEAT`, `PHYS_COEFF_COLD`).
    pub heatmod: f64,
    pub coldmod: f64,
    pub resist_heat: bool,
    pub resist_cold: bool,
    pub resist_low_pressure: bool,
    pub resist_high_pressure: bool,
    /// `TRAIT_COLD_BLOODED`: no core stabilisation.
    pub cold_blooded: bool,
    pub no_crit_damage: bool,
    /// The lungs' `safe_oxygen_min`, `safe_plasma_min`, `safe_plasma_max` (0: that reaction is
    /// not added, as tg's `add_gas_reaction` guards).
    pub safe_oxygen_min: f64,
    pub safe_plasma_min: f64,
    pub safe_plasma_max: f64,
}

impl Default for BodyLife {
    fn default() -> Self {
        Self {
            normal: BODYTEMP_NORMAL,
            heat_limit: BODYTEMP_HEAT_DAMAGE_LIMIT,
            cold_limit: BODYTEMP_COLD_DAMAGE_LIMIT,
            heatmod: 1.0,
            coldmod: 1.0,
            resist_heat: false,
            resist_cold: false,
            resist_low_pressure: false,
            resist_high_pressure: false,
            cold_blooded: false,
            no_crit_damage: false,
            safe_oxygen_min: SAFE_OXYGEN_MIN,
            safe_plasma_min: 0.0,
            safe_plasma_max: SAFE_PLASMA_MAX,
        }
    }
}

impl BodyLife {
    /// The species of `mob` and its lungs' type (`/obj/item/organ/lungs/plasmaman` breathes
    /// plasma, `slime` is not hurt by it).
    // shortcut: the other lungs subtypes (lavaland, ethereal, fish, ...) breathe as human lungs.
    pub fn of(world: &World, mob: Entity) -> Self {
        use crate::species::{Trait, def_of, has_trait};
        let def = def_of(world, mob);
        let has = |t| has_trait(world, mob, t);
        let lungs = crate::medical::organs::state(world, mob, crate::medical::organs::Slot::Lungs)
            .map(|l| l.path.as_str())
            .unwrap_or("");
        let (safe_oxygen_min, safe_plasma_min, safe_plasma_max) =
            if is_under(lungs, "/obj/item/organ/lungs/plasmaman") {
                (0.0, 4.0, 0.0)
            } else if is_under(lungs, "/obj/item/organ/lungs/slime") {
                (SAFE_OXYGEN_MIN, 0.0, 0.0)
            } else {
                (SAFE_OXYGEN_MIN, 0.0, SAFE_PLASMA_MAX)
            };
        Self {
            normal: def.bodytemp_normal,
            heat_limit: def.bodytemp_heat_damage_limit,
            cold_limit: def.bodytemp_cold_damage_limit,
            heatmod: def.heatmod,
            coldmod: def.coldmod,
            resist_heat: has(Trait::ResistHeat),
            resist_cold: has(Trait::ResistCold),
            resist_low_pressure: has(Trait::ResistLowPressure),
            resist_high_pressure: has(Trait::ResistHighPressure),
            cold_blooded: has(Trait::ColdBlooded),
            no_crit_damage: has(Trait::NoCritDamage),
            safe_oxygen_min,
            safe_plasma_min,
            safe_plasma_max,
        }
    }
}

impl Default for Respiration {
    fn default() -> Self {
        Self {
            losebreath: 0.0,
            failed_last_breath: false,
            last_pp: [0.0; GAS_COUNT],
            co2_overload_since: 0.0,
            bodytemperature: BODYTEMP_NORMAL,
            coretemperature: BODYTEMP_NORMAL,
            old_bodytemperature: BODYTEMP_NORMAL,
            seconds_in_low_pressure: 0.0,
            body: BodyLife::default(),
        }
    }
}

/// `get_breath_partial_pressure`: kPa of `moles` in a breath.
fn breath_pp(moles: f64, temperature: f64) -> f64 {
    moles * R_IDEAL_GAS_EQUATION * temperature / BREATH_VOLUME
}

fn gas(id: GasId) -> usize {
    usize::from(id.0)
}

/// `/proc/get_temp_change_amount`.
pub fn temp_change_amount(diff: f64, change_rate: f64) -> f64 {
    let half = BODYTEMP_AUTORECOVERY_DIVISOR / 2.0;
    if diff < 0.0 {
        -half * (1.0 - diff * change_rate).ln()
    } else {
        half * (1.0 + diff * change_rate).ln()
    }
}

impl Respiration {
    /// The missed breaths of tg `breathe` (failing lungs, crit): `true` when this breath is
    /// missed (`losebreath`).
    pub fn misses_breath(&mut self, stat: Stat, lungs_failing: bool) -> bool {
        match stat {
            _ if lungs_failing => self.losebreath += 1.0,
            Stat::HardCrit => self.losebreath += 1.0,
            Stat::SoftCrit => self.losebreath += 0.25,
            _ => {}
        }
        if self.losebreath >= 1.0 {
            self.losebreath -= 1.0;
            return true;
        }
        false
    }

    /// Human `check_breath` without lungs: `SUFFOCATION_OXYLOSS + 1` (less in crit), a failed
    /// breath; the air is breathed back unchanged.
    pub fn no_lungs(&mut self, ctx: &mut LifeCtx) {
        let mut damage = f64::from(SUFFOCATION_OXYLOSS + 1.0);
        if matches!(ctx.stat, Stat::SoftCrit | Stat::HardCrit) {
            damage *= self.crit_modifier();
        }
        ctx.damage(DamageType::Oxy, damage);
        self.failed_last_breath = true;
        ctx.alerts
            .throw(AlertCategory::NotEnoughOxygen, "not_enough_oxy", 0);
    }

    /// Lungs `check_breath` of a human. `breath` (`None`: nothing to breathe) is changed in
    /// place: O2 becomes CO2, pluoxium is consumed, it takes the body temperature. Returns
    /// whether the breath succeeded.
    // shortcut: BZ, freon, healium, helium, hypernoblium, miasma, nitrium, tritium, zauker and
    // antinoblium have no effect yet; N2O throws its alert but does not knock out or sleep, CO2
    // does not knock out (no unconsciousness status yet); breath temperature does not damage the
    // lungs (tg's early return, see below).
    pub fn check_breath(&mut self, breath: Option<&mut GasMixture>, ctx: &mut LifeCtx) -> bool {
        let mut empty = GasMixture::vacuum();
        let breath = breath.unwrap_or(&mut empty);
        let has_moles = breath.total_moles() != 0.0;
        if !has_moles {
            self.failed_last_breath = true;
        }
        let temperature = breath.temperature;
        let present: [bool; GAS_COUNT] = std::array::from_fn(|g| breath.moles[g] > 0.0);
        let mut pp = [0.0; GAS_COUNT];
        for (g, p) in pp.iter_mut().enumerate() {
            if present[g] {
                *p = breath_pp(breath.moles[g], temperature);
            }
        }
        let o2 = gas(GasId::O2);
        let pluoxium = gas(GasId::parse("pluoxium").expect("tg gas"));
        pp[o2] += pp[pluoxium] * PLUOXIUM_PROPORTION;
        let last = self.last_pp;
        // `breath_out`: what the lungs exhale, merged back at the end.
        let mut out = GasMixture::vacuum();
        out.temperature = temperature;

        if self.body.safe_oxygen_min > 0.0 {
            self.breathe_oxygen(breath, &mut out, pp[o2], last[o2], ctx);
        }
        let plasma = gas(GasId::PLASMA);
        if self.body.safe_plasma_min > 0.0 {
            self.breathe_plasma(breath, &mut out, pp[plasma], last[plasma], ctx);
        }
        if present[pluoxium] {
            breath.moles[pluoxium] = 0.0;
        }
        let co2 = gas(GasId::CO2);
        let n2o = gas(GasId::N2O);
        let plasma_toxic = self.body.safe_plasma_max > 0.0;
        if present[co2] && self.too_much_co2(pp[co2], last[co2], ctx) {
            self.safe_co2(ctx);
        }
        if plasma_toxic
            && present[plasma]
            && self.too_much_plasma(breath.moles[plasma], pp[plasma], last[plasma], ctx)
        {
            ctx.alerts.clear(AlertCategory::TooMuchPlasma);
        }
        if present[n2o] && too_much_n2o(pp[n2o], last[n2o], ctx) {
            ctx.alerts.clear(AlertCategory::TooMuchN2o);
        }
        // Gases of the last breath that this one lacks.
        if last[co2] > 0.0 && breath.moles[co2] == 0.0 {
            self.safe_co2(ctx);
        }
        if plasma_toxic && last[plasma] > 0.0 && breath.moles[plasma] == 0.0 {
            ctx.alerts.clear(AlertCategory::TooMuchPlasma);
        }
        if last[n2o] > 0.0 && breath.moles[n2o] == 0.0 {
            ctx.alerts.clear(AlertCategory::TooMuchN2o);
        }
        self.last_pp = pp;
        if has_moles {
            // `handle_breath_temperature`: tg returns before any damage when the breath is
            // above `cold_level_1_threshold` OR below `heat_level_1_threshold`, which every
            // temperature is, so a breath only takes the body temperature.
            breath.temperature = self.bodytemperature;
            breath.merge(&out);
        }
        breath.garbage_collect();
        !self.failed_last_breath
    }

    fn breathe_oxygen(
        &mut self,
        breath: &mut GasMixture,
        out: &mut GasMixture,
        o2_pp: f64,
        old_o2_pp: f64,
        ctx: &mut LifeCtx,
    ) {
        let o2 = gas(GasId::O2);
        let safe_min = self.body.safe_oxygen_min;
        if o2_pp < safe_min {
            ctx.alerts
                .throw(AlertCategory::NotEnoughOxygen, "not_enough_oxy", 0);
            let breathed = self.handle_suffocation(o2_pp, safe_min, breath.moles[o2], ctx);
            if o2_pp > 0.0 {
                exchange(breath, out, GasId::O2, GasId::CO2, breathed);
            }
            return;
        }
        if old_o2_pp < safe_min {
            self.failed_last_breath = false;
            ctx.alerts.clear(AlertCategory::NotEnoughOxygen);
        }
        exchange(breath, out, GasId::O2, GasId::CO2, f64::INFINITY);
        if ctx.stat == Stat::Conscious && ctx.health.oxy > 0.0 {
            ctx.damage(DamageType::Oxy, -5.0);
        }
    }

    /// `SUFFOCATION_OXYLOSS_CRIT_MODIFIER`, 0 with `TRAIT_NOCRITDAMAGE`.
    fn crit_modifier(&self) -> f64 {
        if self.body.no_crit_damage {
            0.0
        } else {
            f64::from(SUFFOCATION_OXYLOSS_CRIT_MODIFIER)
        }
    }

    /// Lungs `handle_suffocation`: oxyloss scaled by how far below `safe_min` the breath was;
    /// returns the moles breathed of a partial breath.
    fn handle_suffocation(
        &mut self,
        breath_pp: f64,
        safe_min: f64,
        moles: f64,
        ctx: &mut LifeCtx,
    ) -> f64 {
        self.failed_last_breath = true;
        if ctx.chance(25.0) {
            ctx.events.push(LifeEvent::Emote("gasp"));
        }
        let mut damage = f64::from(SUFFOCATION_OXYLOSS);
        let mut breathed = 0.0;
        if breath_pp > 0.0 {
            damage *= (safe_min - breath_pp) / safe_min;
            breathed = moles;
        }
        if matches!(ctx.stat, Stat::SoftCrit | Stat::HardCrit) {
            damage *= self.crit_modifier();
        }
        if damage > 0.0 {
            ctx.damage(DamageType::Oxy, damage);
        }
        breathed
    }

    /// Lungs `breathe_plasma` (plasmaman lungs: `safe_plasma_min`): plasma becomes CO2.
    fn breathe_plasma(
        &mut self,
        breath: &mut GasMixture,
        out: &mut GasMixture,
        plasma_pp: f64,
        old_plasma_pp: f64,
        ctx: &mut LifeCtx,
    ) {
        let plasma = gas(GasId::PLASMA);
        let safe_min = self.body.safe_plasma_min;
        if plasma_pp < safe_min {
            ctx.alerts
                .throw(AlertCategory::NotEnoughPlasma, "not_enough_plas", 0);
            let breathed = self.handle_suffocation(plasma_pp, safe_min, breath.moles[plasma], ctx);
            if plasma_pp > 0.0 {
                exchange(breath, out, GasId::PLASMA, GasId::CO2, breathed);
            }
            return;
        }
        if old_plasma_pp < safe_min {
            self.failed_last_breath = false;
            ctx.alerts.clear(AlertCategory::NotEnoughPlasma);
        }
        exchange(breath, out, GasId::PLASMA, GasId::CO2, f64::INFINITY);
        // `breather.health >= breather.crit_threshold`.
        if ctx.health.health() >= crate::health::HEALTH_THRESHOLD_CRIT && ctx.health.oxy > 0.0 {
            ctx.damage(DamageType::Oxy, -5.0);
        }
    }

    /// Lungs `too_much_co2`; `true`: the CO2 is gone (`BREATH_LOST`).
    fn too_much_co2(&mut self, co2_pp: f64, old_co2_pp: f64, ctx: &mut LifeCtx) -> bool {
        if co2_pp <= SAFE_CO2_MAX {
            return old_co2_pp > SAFE_CO2_MAX;
        }
        if old_co2_pp < SAFE_CO2_MAX {
            self.co2_overload_since = ctx.now;
        }
        if ctx.chance(20.0) {
            ctx.events.push(LifeEvent::Emote("cough"));
        }
        let overloaded = ctx.now - self.co2_overload_since;
        if overloaded > 12.0 {
            ctx.alerts
                .throw(AlertCategory::TooMuchCo2, "too_much_co2", 0);
            ctx.damage(DamageType::Oxy, 3.0);
            if overloaded > 30.0 {
                ctx.damage(DamageType::Oxy, 8.0);
            }
        }
        false
    }

    fn safe_co2(&mut self, ctx: &mut LifeCtx) {
        self.co2_overload_since = 0.0;
        ctx.alerts.clear(AlertCategory::TooMuchCo2);
    }

    /// Lungs `too_much_plasma`: tox from the breath's plasma moles.
    fn too_much_plasma(
        &mut self,
        moles: f64,
        plasma_pp: f64,
        old_plasma_pp: f64,
        ctx: &mut LifeCtx,
    ) -> bool {
        if plasma_pp <= SAFE_PLASMA_MAX {
            return old_plasma_pp > SAFE_PLASMA_MAX;
        }
        if old_plasma_pp < SAFE_PLASMA_MAX {
            ctx.alerts
                .throw(AlertCategory::TooMuchPlasma, "too_much_plas", 0);
        }
        let ratio = moles / SAFE_PLASMA_MAX * 10.0;
        ctx.damage(
            DamageType::Tox,
            ratio.clamp(MIN_TOXIC_GAS_DAMAGE, MAX_TOXIC_GAS_DAMAGE),
        );
        if ctx.chance(plasma_pp) {
            ctx.events.push(LifeEvent::Chat(
                Span::Warning,
                "You feel a burning sensation in your chest",
            ));
        }
        false
    }

    /// tg `handle_environment_pressure` with the pressure the body feels; `spt` seconds.
    pub fn environment_pressure(&mut self, adjusted: f64, spt: f64, ctx: &mut LifeCtx) {
        if adjusted >= HAZARD_HIGH_PRESSURE && self.body.resist_high_pressure {
            ctx.alerts.clear(AlertCategory::Pressure);
        } else if adjusted >= HAZARD_HIGH_PRESSURE {
            let damage = ((adjusted / HAZARD_HIGH_PRESSURE - 1.0) * PRESSURE_DAMAGE_COEFFICIENT)
                .min(MAX_HIGH_PRESSURE_DAMAGE)
                * spt;
            ctx.damage(DamageType::Brute, damage);
            ctx.alerts.throw(AlertCategory::Pressure, "highpressure", 2);
        } else if adjusted >= WARNING_HIGH_PRESSURE {
            ctx.alerts.throw(AlertCategory::Pressure, "highpressure", 1);
        } else if adjusted >= WARNING_LOW_PRESSURE {
            ctx.alerts.clear(AlertCategory::Pressure);
            self.seconds_in_low_pressure = 0.0;
        } else if adjusted >= HAZARD_LOW_PRESSURE {
            if self.body.resist_low_pressure {
                ctx.alerts.clear(AlertCategory::Pressure);
            } else {
                ctx.alerts.throw(AlertCategory::Pressure, "lowpressure", 1);
            }
            self.seconds_in_low_pressure = 0.0;
        } else if self.body.resist_low_pressure {
            ctx.alerts.clear(AlertCategory::Pressure);
            self.seconds_in_low_pressure += spt;
        } else {
            // `80 SECONDS` is 800 deciseconds against a count of seconds, as in tg.
            let ramp = byond_round(1.0 + self.seconds_in_low_pressure / 800.0, 0.05);
            let damage = (ramp * BASE_LOW_PRESSURE_DAMAGE).min(MAX_LOW_PRESSURE_DAMAGE) * spt;
            ctx.damage(DamageType::Brute, damage);
            ctx.alerts.throw(AlertCategory::Pressure, "lowpressure", 2);
            self.seconds_in_low_pressure += spt;
        }
    }

    /// The space suit's thermal regulator (`/obj/item/clothing/suit/space/process`, `SSobj`).
    pub fn suit_regulator(&mut self, spt: f64) {
        self.bodytemperature += temp_change_amount(
            BODYTEMP_NORMAL - self.bodytemperature,
            BODYTEMP_SUIT_CHANGE_RATE * spt,
        );
    }

    /// tg `handle_body_temperature`: core stabilisation (alive only), skin against the air
    /// (`area_temp`) through `worn`'s insulation, alerts, then temperature damage. `on_fire`:
    /// the fire stacks of a burning mob (`crate::fire`): the skin does not cool towards the air
    /// or the core, and the heat burns harder.
    // shortcut: no cold slowdown, no heat wounds (`heat_exposure_stacks`).
    pub fn body_temperature(
        &mut self,
        area_temp: f64,
        worn: &Worn,
        alive: bool,
        on_fire: Option<f64>,
        spt: f64,
        ctx: &mut LifeCtx,
    ) {
        // `body_temperature_core`: not for the cold-blooded.
        if alive && !self.body.cold_blooded {
            let change = temp_change_amount(
                self.body.normal - self.coretemperature,
                BODYTEMP_CORE_CHANGE_RATE * spt,
            );
            self.coretemperature += change;
        }
        // `body_temperature_skin`.
        let skin_core = self.bodytemperature - self.coretemperature;
        self.coretemperature += temp_change_amount(skin_core, BODYTEMP_SKIN_CORE_CHANGE_RATE * spt);
        let protection = if area_temp > self.bodytemperature {
            worn.heat_protection(area_temp)
        } else {
            worn.cold_protection(area_temp)
        };
        let area_skin_diff = area_temp - self.bodytemperature;
        if on_fire.is_none() || area_skin_diff > 0.0 {
            let mut area_skin =
                temp_change_amount(area_skin_diff, BODYTEMP_AREA_SKIN_CHANGE_RATE * spt);
            area_skin *= if self.body.normal + 10.0 < self.coretemperature {
                1.0 - protection * 0.7
            } else {
                1.0 - protection
            };
            self.adjust_bodytemperature(area_skin);
        }
        if on_fire.is_none() {
            let core_skin_diff = self.coretemperature - self.bodytemperature;
            let mut core_skin = (1.0 + protection)
                * temp_change_amount(core_skin_diff, BODYTEMP_CORE_SKIN_CHANGE_RATE * spt);
            core_skin = if core_skin_diff > 0.0 {
                core_skin.min(core_skin_diff)
            } else {
                core_skin.max(core_skin_diff)
            };
            self.adjust_bodytemperature(core_skin);
        }

        self.temperature_alerts(ctx);
        self.temperature_damage(on_fire, spt, ctx);
    }

    /// `/mob/living/carbon/adjust_bodytemperature` without insulation or steps.
    fn adjust_bodytemperature(&mut self, amount: f64) {
        self.bodytemperature = (self.bodytemperature + amount).max(0.0);
    }

    /// `body_temperature_alerts`.
    fn temperature_alerts(&mut self, ctx: &mut LifeCtx) {
        let body = self.bodytemperature;
        let (heat_limit, cold_limit) = (self.body.heat_limit, self.body.cold_limit);
        if body > heat_limit && !self.body.resist_heat {
            let severity = if body <= BODYTEMP_HEAT_WARNING_2 {
                1
            } else if body <= BODYTEMP_HEAT_WARNING_3 {
                2
            } else {
                3
            };
            ctx.alerts
                .throw(AlertCategory::Temperature, "hot", severity);
        } else if body < cold_limit && !self.body.resist_cold {
            let severity = if body >= BODYTEMP_COLD_WARNING_2 {
                1
            } else if body >= BODYTEMP_COLD_WARNING_3 {
                2
            } else {
                3
            };
            ctx.alerts
                .throw(AlertCategory::Temperature, "cold", severity);
        } else if self.old_bodytemperature > heat_limit || self.old_bodytemperature < cold_limit {
            ctx.alerts.clear(AlertCategory::Temperature);
        }
        self.old_bodytemperature = body;
    }

    /// `body_temperature_damage` of a human (not hulk); `on_fire`: its fire stacks.
    fn temperature_damage(&mut self, on_fire: Option<f64>, spt: f64, ctx: &mut LifeCtx) {
        let core = self.coretemperature;
        let body = self.body;
        if core > body.heat_limit && !body.resist_heat {
            // `log(2 - firemodifier, core - normal) - 5`, `firemodifier = fire_stacks / 50`
            // while burning (without wet stacks it is 0 otherwise).
            let firemodifier = on_fire.map_or(0.0, |stacks| stacks / 50.0);
            let burn = ((core - body.normal).log(2.0 - firemodifier) - 5.0).max(0.0)
                * body.heatmod
                * 0.5
                * spt;
            if ctx.stat == Stat::Conscious && ctx.chance(burn) {
                ctx.events.push(LifeEvent::Emote("scream"));
            }
            ctx.damage(DamageType::Burn, burn);
        }
        // Cold damage stops at the death threshold for the dead.
        if f64::from(ctx.health.burn) >= 100.0 && ctx.stat == Stat::Dead {
            return;
        }
        if core < body.cold_limit && !body.resist_cold {
            // tg `in 201 to limit`, `in 120 to 200`, else: 200..201 is level 3.
            let level = if (201.0..=body.cold_limit).contains(&core) {
                0
            } else if (120.0..=200.0).contains(&core) {
                1
            } else {
                2
            };
            ctx.damage(
                DamageType::Burn,
                COLD_DAMAGE_LEVEL[level] * body.coldmod * spt,
            );
        }
    }
}

/// Lungs `breathe_gas_volume`: up to `volume` moles of `remove` out of the breath, as
/// `exchange` into the exhaled gas.
fn exchange(
    breath: &mut GasMixture,
    out: &mut GasMixture,
    remove: GasId,
    into: GasId,
    volume: f64,
) {
    let moved = volume.min(breath.moles[gas(remove)]);
    breath.moles[gas(remove)] -= moved;
    out.moles[gas(into)] += moved;
}

/// Lungs `too_much_n2o`; `true`: the N2O is gone (`BREATH_LOST`).
// shortcut: no euphoria, giggles, unconsciousness or sleep; only the alert.
fn too_much_n2o(n2o_pp: f64, old_n2o_pp: f64, ctx: &mut LifeCtx) -> bool {
    if n2o_pp < N2O_PARA_MIN {
        if n2o_pp <= N2O_DETECT_MIN {
            return old_n2o_pp > N2O_DETECT_MIN;
        }
        if old_n2o_pp >= N2O_PARA_MIN || old_n2o_pp <= N2O_DETECT_MIN {
            ctx.alerts.clear(AlertCategory::TooMuchN2o);
        }
        return false;
    }
    if old_n2o_pp < N2O_PARA_MIN {
        ctx.alerts
            .throw(AlertCategory::TooMuchN2o, "too_much_n2o", 0);
    }
    false
}

/// What a worn clothing item protects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WornClothing {
    pub flags: u32,
    pub covered: u32,
    pub heat_protection: u32,
    pub cold_protection: u32,
    pub min_cold: f32,
    pub max_heat: f32,
}

impl WornClothing {
    /// A space suit's `min_cold_protection_temperature` follows its regulator, which tg turns
    /// on at `Initialize`.
    // shortcut: the regulator never runs out of cell charge.
    pub fn new(path: &str, c: &DmClothing) -> Self {
        let min_cold = if is_under(path, SPACE_SUIT) {
            SPACE_SUIT_MIN_TEMP_PROTECT
        } else {
            c.min_cold_protection_temperature
        };
        Self {
            flags: c.clothing_flags,
            covered: c.body_parts_covered,
            heat_protection: c.heat_protection,
            cold_protection: c.cold_protection,
            min_cold,
            max_heat: c.max_heat_protection_temperature,
        }
    }
}

/// The clothing a human's environment checks read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Worn {
    /// Head, outer suit, uniform, shoes, gloves, mask: what insulates (`get_*_protection_flags`).
    pub insulating: Vec<WornClothing>,
    /// Every equipped clothing item (`get_equipped_items`).
    pub equipped: Vec<WornClothing>,
    /// A space suit with its thermal regulator is worn as the outer suit.
    pub regulator: bool,
}

impl Worn {
    pub fn read(world: &World, inventory: &Inventory) -> Self {
        let clothing = |item: Entity| {
            let c = world.get::<DmClothing>(item)?;
            let path = world.get::<DmItem>(item).map_or("", |i| i.path.as_str());
            Some(WornClothing::new(path, c))
        };
        let insulating = [
            inventory.head,
            inventory.suit,
            inventory.uniform,
            inventory.shoes,
            inventory.gloves,
            inventory.mask,
        ]
        .into_iter()
        .flatten()
        .filter_map(clothing)
        .collect();
        let regulator = inventory.suit.is_some_and(|s| {
            world
                .get::<DmItem>(s)
                .is_some_and(|i| is_under(&i.path, SPACE_SUIT))
        });
        Self {
            insulating,
            equipped: inventory.equipped().filter_map(clothing).collect(),
            regulator,
        }
    }

    /// tg `calculate_affecting_pressure`: one atmosphere when chest, head, hands and feet are
    /// covered by `STOPSPRESSUREDAMAGE` clothing.
    pub fn affecting_pressure(&self, pressure: f64) -> f64 {
        let stops = |c: &&WornClothing| c.flags & STOPSPRESSUREDAMAGE != 0;
        let chest = self
            .equipped
            .iter()
            .filter(stops)
            .any(|c| c.covered & CHEST != 0);
        let head = self
            .equipped
            .iter()
            .filter(stops)
            .any(|c| c.covered & HEAD != 0);
        // tg tests `body_parts_covered & HANDS|ARMS` (and `& FEET|LEGS`): `&` binds first, so
        // any pressure-proof item covers the hands and feet.
        let limbs = self.equipped.iter().any(|c| stops(&c));
        if chest && head && limbs {
            ONE_ATMOSPHERE
        } else {
            pressure
        }
    }

    /// tg `is_atmos_sealed(additional_flags, check_hands)`: pressure-proof (or
    /// `additional`-flagged) clothing over the chest and the head, and the hands covered.
    pub fn atmos_sealed(&self, additional: u32, check_hands: bool) -> bool {
        let seals = |c: &&WornClothing| c.flags & (STOPSPRESSUREDAMAGE | additional) != 0;
        let chest = self
            .equipped
            .iter()
            .filter(seals)
            .any(|c| c.covered & CHEST != 0);
        let head = self
            .equipped
            .iter()
            .filter(seals)
            .any(|c| c.covered & HEAD != 0);
        let hands = self.equipped.iter().any(|c| c.covered & HANDS != 0);
        chest && head && (hands || !check_hands)
    }

    /// tg `get_heat_protection`.
    pub fn heat_protection(&self, temperature: f64) -> f64 {
        let flags = self
            .insulating
            .iter()
            .filter(|c| c.max_heat > 0.0 && f64::from(c.max_heat) >= temperature)
            .fold(0, |f, c| f | c.heat_protection);
        protection_of(flags)
    }

    /// tg `get_cold_protection` (space is read as at least 2.7 K).
    pub fn cold_protection(&self, temperature: f64) -> f64 {
        let temperature = temperature.max(TCMB);
        let flags = self
            .insulating
            .iter()
            .filter(|c| c.min_cold > 0.0 && f64::from(c.min_cold) <= temperature)
            .fold(0, |f, c| f | c.cold_protection);
        protection_of(flags)
    }
}

fn protection_of(flags: u32) -> f64 {
    let sum: f64 = THERMAL_PROTECTION
        .iter()
        .enumerate()
        .filter(|(bit, _)| flags & (1 << bit) != 0)
        .map(|(_, p)| p)
        .sum();
    byond_round(sum, 0.001).min(1.0)
}

/// Standard station air (`OPENTURF_DEFAULT_ATMOS`) for levels without simulated air.
pub(crate) fn station_air() -> GasMixture {
    GasMixture::parse("o2=22;n2=82;TEMP=293.15").expect("valid gas string")
}

/// The SSmobs clock: fires every [`SSMOBS_WAIT_SECS`].
#[derive(Resource, Default)]
pub struct MobsClock {
    elapsed: f64,
    pub times_fired: u64,
}

impl ServerUi for AlertHudUi {
    type Source = &'static AlertHudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &AlertHudData) {}

    fn data(source: &AlertHudData, _: Entity, _: &()) -> AlertHudData {
        source.clone()
    }

    /// Alerts are not clickable yet.
    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

pub struct BreathPlugin;

impl Plugin for BreathPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<AlertHudUi>()
            .init_resource::<MobsClock>()
            .add_observer(give_lungs)
            .add_systems(
                Update,
                (life, sync_alert_hud)
                    .chain()
                    .before(HealthSystems)
                    .before(UiSystems::Open),
            );
    }
}

/// Every mob with [`Health`] breathes (tg carbons with lungs).
fn give_lungs(
    add: On<Add<Health>>,
    silicons: Query<(), With<crate::silicon::Silicon>>,
    basics: Query<(), With<crate::mobs::BasicMob>>,
    mut commands: Commands,
) {
    // Silicons do not breathe; basic mobs have `crate::mobs` atmos requirements instead.
    if silicons.contains(add.entity) || basics.contains(add.entity) {
        return;
    }
    commands.entity(add.entity).insert((
        Respiration::default(),
        Alerts::default(),
        AlertHudData::default(),
        Internals::default(),
    ));
}

/// tg `SSmobs` -> `Life()` of every living mob each [`SSMOBS_WAIT_SECS`].
// Cost: every 2 s one pass over mobs with lungs (one per player): a breath is a few hundred
// float ops and two atmos tile writes on the 4th fire (or every fire while suffocating).
fn life(world: &mut World) {
    let (delta, now) = {
        let time = world.resource::<Time>();
        (time.delta_secs_f64(), time.elapsed_secs_f64())
    };
    let mut fires = Vec::new();
    {
        let mut clock = world.resource_mut::<MobsClock>();
        clock.elapsed += delta;
        while clock.elapsed >= SSMOBS_WAIT_SECS {
            clock.elapsed -= SSMOBS_WAIT_SECS;
            clock.times_fired += 1;
            fires.push(clock.times_fired);
        }
    }
    for fired in fires {
        let mobs: Vec<Entity> = world
            .query_filtered::<Entity, (With<Respiration>, With<Health>, With<TilePos>)>()
            .iter(world)
            .collect();
        for mob in mobs {
            live(world, mob, fired, now);
        }
    }
}

/// One `Life()` of `mob`.
fn live(world: &mut World, mob: Entity, fired: u64, now: f64) {
    let entity = world.entity(mob);
    let (Some(&pos), Some(&stat), Some(health), Some(resp)) = (
        entity.get::<TilePos>(),
        entity.get::<Stat>(),
        entity.get::<Health>(),
        entity.get::<Respiration>(),
    ) else {
        return;
    };
    let (old_health, mut resp) = (health.clone(), resp.clone());
    let mut health = old_health.clone();
    resp.body = BodyLife::of(world, mob);
    let no_breath = crate::species::has_trait(world, mob, crate::species::Trait::NoBreath);
    let mut alerts = entity.get::<Alerts>().cloned().unwrap_or_default();
    let inventory = entity.get::<Inventory>().cloned().unwrap_or_default();
    let internal = entity.get::<Internals>().and_then(|i| i.0);
    let worn = Worn::read(world, &inventory);
    let simulated = world
        .get_resource::<Atmos>()
        .is_some_and(|a| a.level(pos.z).is_some());
    let environment = if simulated {
        let atmos = world.resource::<Atmos>();
        atmos.air(pos).cloned().unwrap_or_default()
    } else {
        station_air()
    };
    let mut rng = CombatRng(world.get_resource_or_init::<CombatRng>().0);
    let mut cutoff = false;
    let mut ctx = LifeCtx {
        health: &mut health,
        stat,
        alerts: &mut alerts,
        rng: &mut rng,
        now,
        events: Vec::new(),
    };

    // The lungs and heart organs (`crate::medical::organs`): a mob without modelled organs
    // breathes with healthy ones.
    use crate::medical::organs::{self, Slot};
    let host = world.get::<organs::OrganHost>(mob).is_some();
    let lungs = organs::state(world, mob, Slot::Lungs);
    let has_lungs = !host || lungs.is_some();
    let lungs_failing = lungs.is_some_and(|l| l.failing);
    let strained =
        |o: Option<&organs::Organ>| o.is_some_and(|o| o.damage > o.spec().high_threshold);
    let next_breath = 4
        - u64::from(strained(lungs))
        - u64::from(strained(organs::state(world, mob, Slot::Heart)));
    // `handle_breathing`: every 4th fire (3rd or 2nd with badly damaged lungs or heart), or every
    // fire after a failed breath.
    // human `breathe()`: nothing with `TRAIT_NOBREATH`.
    if stat != Stat::Dead
        && !no_breath
        && (fired.is_multiple_of(next_breath) || resp.failed_last_breath)
    {
        let mut breath = None;
        if !resp.misses_breath(stat, lungs_failing) {
            // `get_breath_from_internal`; `Some(None)` is an open but empty tank.
            let mut from_tank = None;
            if let Some(tank) = internal {
                if !inventory.holds(tank) || !can_breathe_internals(world, mob, &inventory) {
                    cutoff = true;
                } else if let Some(mut air) = world.get_mut::<TankAir>(tank) {
                    from_tank = Some(air.remove_air_volume(BREATH_VOLUME));
                }
            }
            breath = match from_tank {
                Some(tank_breath) => tank_breath,
                None => {
                    let moles = environment.total_moles() * BREATH_PERCENTAGE;
                    if simulated {
                        world.resource_mut::<Atmos>().remove_air(pos, moles)
                    } else {
                        station_air().remove(moles)
                    }
                }
            };
        }
        if has_lungs {
            resp.check_breath(breath.as_mut(), &mut ctx);
        } else {
            resp.no_lungs(&mut ctx);
        }
        // `loc.assume_air(breath)`: the exhaled breath goes to the tile, also from internals.
        if let Some(breath) = breath
            && simulated
        {
            world.resource_mut::<Atmos>().merge_air(pos, &breath);
        }
        update_stat(ctx.health, &mut ctx.stat);
    }
    // Human `Life()`: crit damage for those who do not suffocate in crit.
    if no_breath
        && ctx.stat != Stat::Dead
        && ctx.health.health() < crate::health::HEALTH_THRESHOLD_CRIT
        && !resp.body.no_crit_damage
    {
        ctx.damage(DamageType::Brute, 0.5 * SSMOBS_WAIT_SECS);
        update_stat(ctx.health, &mut ctx.stat);
    }

    // `handle_environment` -> species `handle_environment_pressure`.
    // `TRAIT_RESISTLOWPRESSURE` + `TRAIT_RESISTHIGHPRESSURE`: neither damage nor alert.
    let pressure = worn.affecting_pressure(environment.pressure());
    let pressure =
        if crate::genetics::has_effect(world, mob, crate::genetics::Effect::PressureAdaptation) {
            pressure.clamp(WARNING_LOW_PRESSURE, WARNING_HIGH_PRESSURE - 1.0)
        } else {
            pressure
        };
    resp.environment_pressure(pressure, SSMOBS_WAIT_SECS, &mut ctx);
    update_stat(ctx.health, &mut ctx.stat);
    // human `Life()` -> species `handle_body_temperature`, also for the dead.
    if worn.regulator {
        resp.suit_regulator(SSMOBS_WAIT_SECS);
    }
    let alive = ctx.stat != Stat::Dead;
    let on_fire = world
        .get::<crate::fire::FireStacks>(mob)
        .filter(|f| f.on_fire)
        .map(|f| f64::from(f.stacks));
    resp.body_temperature(
        environment.temperature,
        &worn,
        alive,
        on_fire,
        SSMOBS_WAIT_SECS,
        &mut ctx,
    );
    update_stat(ctx.health, &mut ctx.stat);
    crate::species::plasma_ignition(world, mob, &worn, &environment, SSMOBS_WAIT_SECS);

    let LifeCtx {
        stat: new_stat,
        events,
        ..
    } = ctx;
    world.resource_mut::<CombatRng>().0 = rng.0;
    let mut entity = world.entity_mut(mob);
    if health != old_health {
        *entity.get_mut::<Health>().expect("read above") = health;
    }
    if new_stat != stat {
        debug!("stat {stat:?} -> {new_stat:?} from Life()");
        *entity.get_mut::<Stat>().expect("read above") = new_stat;
    }
    if entity.get::<Respiration>() != Some(&resp) {
        entity.insert(resp);
    }
    if entity.get::<Alerts>() != Some(&alerts) {
        entity.insert(alerts);
    }
    if cutoff {
        entity.insert(Internals(None));
    }
    let client = entity.get::<PlayerMob>().map(|p| p.client);
    let name = entity
        .get::<Name>()
        .map_or_else(|| "Кто-то".to_owned(), |n| n.to_string());
    if cutoff && let (Some(client), Some(tank)) = (client, internal) {
        let tank = world
            .get::<ss13_mapload::DmName>(tank)
            .map_or_else(|| "баллон".to_owned(), |n| n.name.clone());
        world.write_message(cutoff_message(client, tank));
    }
    for event in events {
        let message = match event {
            LifeEvent::Emote(key) => {
                let Some(emote) = ss13_chat::emote::find(key) else {
                    continue;
                };
                let clients = clients_around(world, pos);
                world.write_message(SendChat {
                    clients,
                    message: ss13_chat::server::emote_message(&name, emote.message),
                });
                continue;
            }
            LifeEvent::Chat(span, text) => ChatMessage {
                channel: ChatChannel::Warning,
                segments: vec![ChatSegment::new(span, text)],
            },
        };
        if let Some(client) = client {
            world.write_message(SendChat {
                clients: vec![client],
                message,
            });
        }
    }
}

/// Clients whose mob is within `DEFAULT_MESSAGE_RANGE` of `pos` (tg `visible_message`).
fn clients_around(world: &World, pos: TilePos) -> Vec<Entity> {
    crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[])
}

/// The owner's alert HUD follows its [`Alerts`].
fn sync_alert_hud(
    mut mobs: Query<(Entity, &Alerts, &mut AlertHudData, Option<&PlayerMob>), Changed<Alerts>>,
    icons: Option<Res<IconIndex>>,
    mut open: MessageWriter<OpenUi>,
) {
    for (mob, alerts, mut hud, player) in &mut mobs {
        let data = AlertHudData {
            alerts: alerts
                .0
                .iter()
                .map(|a| {
                    let (name, desc) = a.text();
                    HudAlert {
                        icon: icons
                            .as_deref()
                            .and_then(|i| i.get("icons/hud/screen_alert.dmi", &a.icon_state())),
                        name: name.to_owned(),
                        desc: desc.to_owned(),
                    }
                })
                .collect(),
        };
        hud.set_if_neq(data);
        if let Some(player) = player {
            open.write(OpenUi::new::<AlertHudUi>(player.client, mob, mob, ""));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Body {
        health: Health,
        alerts: Alerts,
        rng: CombatRng,
    }

    impl Body {
        fn new() -> Self {
            Self {
                health: Health::default(),
                alerts: Alerts::default(),
                rng: CombatRng(1),
            }
        }

        fn ctx(&mut self, stat: Stat) -> LifeCtx<'_> {
            LifeCtx {
                health: &mut self.health,
                stat,
                alerts: &mut self.alerts,
                rng: &mut self.rng,
                now: 0.0,
                events: Vec::new(),
            }
        }
    }

    /// A breath of `BREATH_VOLUME` litres at 293.15 K with these partial pressures, kPa.
    fn breath_of(pp: &[(GasId, f64)]) -> GasMixture {
        let mut mix = GasMixture::vacuum();
        mix.temperature = 293.15;
        for &(g, p) in pp {
            mix.moles[gas(g)] = p * BREATH_VOLUME / (R_IDEAL_GAS_EQUATION * 293.15);
        }
        mix
    }

    #[test]
    fn breath_from_station_air_is_safe_and_exchanges_o2_for_co2() {
        let mut body = Body::new();
        body.health.adjust(DamageType::Oxy, 7.0);
        let mut resp = Respiration::default();
        // tg `breathe`: `BREATH_PERCENTAGE` of a station tile.
        let mut tile = station_air();
        let mut breath = tile.remove(tile.total_moles() * BREATH_PERCENTAGE).unwrap();
        let o2 = breath.get(GasId::O2);
        // o2 = 22 mol * 1.99/2500, read in 1.99 L: the tile's 21.43 kPa.
        assert!((breath_pp(o2, breath.temperature) - 21.43).abs() < 0.05);
        assert!(resp.check_breath(Some(&mut breath), &mut body.ctx(Stat::Conscious)));
        assert_eq!(breath.get(GasId::O2), 0.0);
        assert!((breath.get(GasId::CO2) - o2).abs() < 1e-12);
        // The breath takes the body temperature, then the exhaled CO2 (still at the inhaled
        // temperature) mixes in.
        assert!(breath.temperature > 293.15 && breath.temperature < BODYTEMP_NORMAL);
        // A good breath above crit heals 5 oxy.
        assert_eq!(body.health.oxy, 2.0);
        assert!(body.alerts.0.is_empty());
    }

    #[test]
    fn suffocation_follows_tg_handle_suffocation() {
        // No breath at all: 3 oxy, the alert, the next breath comes next tick.
        let mut body = Body::new();
        let mut resp = Respiration::default();
        assert!(!resp.check_breath(None, &mut body.ctx(Stat::Conscious)));
        assert_eq!(body.health.oxy, 3.0);
        assert!(resp.failed_last_breath);
        assert!(body.alerts.get(AlertCategory::NotEnoughOxygen).is_some());
        // Half the safe pressure: 3 * (16 - 8) / 16 = 1.5; the 8 kPa of O2 still turn to CO2.
        let mut body = Body::new();
        let mut breath = breath_of(&[(GasId::O2, 8.0)]);
        let moles = breath.get(GasId::O2);
        assert!(!resp.check_breath(Some(&mut breath), &mut body.ctx(Stat::Conscious)));
        assert!((body.health.oxy - 1.5).abs() < 1e-5, "{}", body.health.oxy);
        assert!((breath.get(GasId::CO2) - moles).abs() < 1e-12);
        // 15.99 kPa is still not enough; 16 is.
        let mut body = Body::new();
        resp.check_breath(
            Some(&mut breath_of(&[(GasId::O2, 15.99)])),
            &mut body.ctx(Stat::Conscious),
        );
        assert!(body.health.oxy > 0.0);
        let mut body = Body::new();
        assert!(resp.check_breath(
            Some(&mut breath_of(&[(GasId::O2, 16.001)])),
            &mut body.ctx(Stat::Conscious)
        ));
        assert_eq!(body.health.oxy, 0.0);
        assert!(!resp.failed_last_breath);
        assert!(body.alerts.get(AlertCategory::NotEnoughOxygen).is_none());
        // In crit the damage is 22%: 3 * 0.22 per missed breath.
        let mut body = Body::new();
        resp.check_breath(None, &mut body.ctx(Stat::HardCrit));
        assert!((body.health.oxy - 0.66).abs() < 1e-5);
    }

    #[test]
    fn crit_misses_breaths_like_tg_losebreath() {
        let mut resp = Respiration::default();
        assert!(resp.misses_breath(Stat::HardCrit, false));
        assert!(resp.misses_breath(Stat::HardCrit, false));
        // Soft crit: one breath in four.
        let misses: Vec<bool> = (0..4)
            .map(|_| resp.misses_breath(Stat::SoftCrit, false))
            .collect();
        assert_eq!(misses, [false, false, false, true]);
        assert!(!resp.misses_breath(Stat::Conscious, false));
        // Failing lungs miss every breath, awake or not.
        assert!(resp.misses_breath(Stat::Conscious, true));
        assert!(resp.misses_breath(Stat::SoftCrit, true));
    }

    #[test]
    fn no_lungs_suffocate_like_human_check_breath() {
        let mut resp = Respiration::default();
        let mut body = Body::new();
        resp.no_lungs(&mut body.ctx(Stat::Conscious));
        assert_eq!(body.health.oxy, SUFFOCATION_OXYLOSS + 1.0);
        assert!(resp.failed_last_breath);
        assert!(body.alerts.get(AlertCategory::NotEnoughOxygen).is_some());
        let mut body = Body::new();
        resp.no_lungs(&mut body.ctx(Stat::HardCrit));
        assert!((body.health.oxy - 4.0 * SUFFOCATION_OXYLOSS_CRIT_MODIFIER).abs() < 1e-5);
    }

    #[test]
    fn plasma_and_co2_thresholds() {
        // Plasma: tox = clamp(moles / 0.05 * 10, 1, 10), the alert from the first breath.
        let mut body = Body::new();
        let mut resp = Respiration::default();
        let mut breath = breath_of(&[(GasId::O2, 21.0), (GasId::PLASMA, 1.0)]);
        let moles = breath.get(GasId::PLASMA);
        resp.check_breath(Some(&mut breath), &mut body.ctx(Stat::Conscious));
        let expected = (moles / 0.05 * 10.0).clamp(1.0, 10.0);
        assert!((f64::from(body.health.tox) - expected).abs() < 1e-5);
        assert_eq!(expected, 1.0, "tiny breath moles: the minimum");
        assert!(body.alerts.get(AlertCategory::TooMuchPlasma).is_some());
        // 0.05 kPa is safe, and the alert clears.
        resp.check_breath(
            Some(&mut breath_of(&[(GasId::O2, 21.0), (GasId::PLASMA, 0.05)])),
            &mut body.ctx(Stat::Conscious),
        );
        assert!(body.alerts.get(AlertCategory::TooMuchPlasma).is_none());

        // CO2 over 10 kPa: nothing for 12 s, then 3 oxy, after 30 s 3 + 8.
        let mut resp = Respiration::default();
        let co2 = || breath_of(&[(GasId::O2, 21.0), (GasId::CO2, 11.0)]);
        let at = |now: f64, resp: &mut Respiration| {
            let mut body = Body::new();
            let mut ctx = body.ctx(Stat::Conscious);
            ctx.now = now;
            resp.check_breath(Some(&mut co2()), &mut ctx);
            (
                body.health.oxy,
                body.alerts.get(AlertCategory::TooMuchCo2).is_some(),
            )
        };
        assert_eq!(at(100.0, &mut resp), (0.0, false));
        assert_eq!(at(112.0, &mut resp), (0.0, false));
        assert_eq!(at(112.5, &mut resp), (3.0, true));
        assert_eq!(at(131.0, &mut resp), (11.0, true));
        // A breath without CO2 resets it.
        let mut body = Body::new();
        resp.check_breath(
            Some(&mut breath_of(&[(GasId::O2, 21.0)])),
            &mut body.ctx(Stat::Conscious),
        );
        assert_eq!(resp.co2_overload_since, 0.0);
        assert!(body.alerts.get(AlertCategory::TooMuchCo2).is_none());
    }

    #[test]
    fn n2o_alert_from_one_kpa() {
        let mut body = Body::new();
        let mut resp = Respiration::default();
        let mut n2o = |pp: f64, body: &mut Body| {
            resp.check_breath(
                Some(&mut breath_of(&[(GasId::O2, 21.0), (GasId::N2O, pp)])),
                &mut body.ctx(Stat::Conscious),
            );
            body.alerts.get(AlertCategory::TooMuchN2o).is_some()
        };
        assert!(!n2o(0.5, &mut body));
        assert!(n2o(1.01, &mut body));
        assert!(!n2o(0.5, &mut body));
    }

    #[test]
    fn pressure_damage_follows_tg_numbers() {
        let mut resp = Respiration::default();
        let hit = |pressure: f64, resp: &mut Respiration| {
            let mut body = Body::new();
            resp.environment_pressure(pressure, 2.0, &mut body.ctx(Stat::Conscious));
            (
                body.health.brute,
                body.alerts
                    .get(AlertCategory::Pressure)
                    .map(Alert::icon_state),
            )
        };
        // Station pressure: nothing.
        assert_eq!(hit(101.325, &mut resp), (0.0, None));
        // 30 kPa: the grey alert only; 325: the orange one.
        assert_eq!(hit(30.0, &mut resp), (0.0, Some("lowpressure1".into())));
        assert_eq!(hit(400.0, &mut resp), (0.0, Some("highpressure1".into())));
        // 825 kPa: min((825/550 - 1) * 2, 2) * 2 s = 2 brute.
        let (brute, alert) = hit(825.0, &mut resp);
        assert!((brute - 2.0).abs() < 1e-5);
        assert_eq!(alert.as_deref(), Some("highpressure2"));
        // 5000 kPa caps at 2 * 2 s.
        assert!((hit(5000.0, &mut resp).0 - 4.0).abs() < 1e-5);
        // Vacuum: round(1 + s / 800, 0.05) * 2 per second, capped at 5: 4 per 2 s fire.
        let mut resp = Respiration::default();
        let (brute, alert) = hit(0.0, &mut resp);
        assert!((brute - 4.0).abs() < 1e-5);
        assert_eq!(alert.as_deref(), Some("lowpressure2"));
        assert_eq!(resp.seconds_in_low_pressure, 2.0);
        // After 40 s: round(1.05, 0.05) * 2 * 2 = 4.2.
        resp.seconds_in_low_pressure = 40.0;
        assert!((hit(0.0, &mut resp).0 - 4.2).abs() < 1e-5);
        // Back in air the counter resets.
        hit(101.0, &mut resp);
        assert_eq!(resp.seconds_in_low_pressure, 0.0);
    }

    fn clothing(flags: u32, covered: u32) -> WornClothing {
        WornClothing {
            flags,
            covered,
            ..default()
        }
    }

    #[test]
    fn pressure_suits_need_chest_and_head() {
        let suit = clothing(STOPSPRESSUREDAMAGE, 2043);
        let helmet = clothing(STOPSPRESSUREDAMAGE, HEAD);
        let jumpsuit = clothing(0, 411);
        let worn = |items: Vec<WornClothing>| Worn {
            equipped: items,
            ..default()
        };
        assert_eq!(worn(vec![suit.clone()]).affecting_pressure(0.0), 0.0);
        assert_eq!(
            worn(vec![helmet.clone(), jumpsuit]).affecting_pressure(0.0),
            0.0
        );
        assert_eq!(
            worn(vec![suit, helmet]).affecting_pressure(0.0),
            ONE_ATMOSPHERE
        );
    }

    #[test]
    fn insulation_sums_tg_body_part_shares() {
        // A space suit (everything but the head) protects 0.7 down to its 2 K, the helmet 0.3.
        let suit = WornClothing {
            cold_protection: 2043,
            min_cold: 2.0,
            ..default()
        };
        let helmet = WornClothing {
            cold_protection: HEAD,
            min_cold: 2.0,
            ..default()
        };
        let worn = Worn {
            insulating: vec![suit.clone(), helmet],
            ..default()
        };
        assert_eq!(worn.cold_protection(TCMB), 1.0);
        let only_suit = Worn {
            insulating: vec![suit],
            ..default()
        };
        assert!((only_suit.cold_protection(TCMB) - 0.7).abs() < 1e-9);
        // An unpowered suit (72 K) does not protect in space.
        let off = Worn {
            insulating: vec![WornClothing {
                cold_protection: 2043,
                min_cold: 72.0,
                ..default()
            }],
            ..default()
        };
        assert_eq!(off.cold_protection(TCMB), 0.0);
    }

    /// 30 `Life()` fires (60 s) of a living human in `area_temp` K with `worn`; the core and
    /// skin temperatures at the end.
    fn temperature_after_60s(area_temp: f64, worn: &Worn, body: &mut Body) -> (f64, f64) {
        let mut resp = Respiration::default();
        for _ in 0..30 {
            resp.body_temperature(
                area_temp,
                worn,
                true,
                None,
                2.0,
                &mut body.ctx(Stat::Conscious),
            );
        }
        (resp.coretemperature, resp.bodytemperature)
    }

    #[test]
    fn station_air_keeps_the_body_in_tg_band() {
        // Reference numbers from an independent run of tg's `body_temperature_core` and
        // `body_temperature_skin` (BODYTEMP_*_CHANGE_RATE of #97364, seconds_per_tick 2).
        let sneakers = WornClothing {
            cold_protection: (1 << 5) | (1 << 6),
            min_cold: 2.0,
            ..default()
        };
        let assistant = Worn {
            insulating: vec![sneakers],
            ..default()
        };
        // An assistant (only sneakers insulate: FEET = 0.05) in 293.15 K air settles near
        // 28.4 °C: tg's environment-led model, inside the no-alert, no-damage band.
        let mut body = Body::new();
        let (core, skin) = temperature_after_60s(293.15, &assistant, &mut body);
        assert!((core - 301.5905).abs() < 1e-3, "{core}");
        assert!((skin - 301.4952).abs() < 1e-3, "{skin}");
        for t in [core, skin] {
            assert!((BODYTEMP_COLD_DAMAGE_LIMIT..BODYTEMP_HEAT_DAMAGE_LIMIT).contains(&t));
        }
        assert!(body.alerts.get(AlertCategory::Temperature).is_none());
        assert_eq!(body.health.burn, 0.0);
        // 0.3 protection or more (a coat over chest and groin) holds 310.15 K exactly.
        let coat = WornClothing {
            cold_protection: 0b11,
            min_cold: 2.0,
            ..default()
        };
        let coated = Worn {
            insulating: vec![coat],
            ..default()
        };
        let (core, skin) = temperature_after_60s(293.15, &coated, &mut Body::new());
        assert_eq!((core, skin), (BODYTEMP_NORMAL, BODYTEMP_NORMAL));
        // Naked in vacuum: 10 s cool the core to 256.74 K and the skin to 210.29 K.
        let mut resp = Respiration::default();
        let mut body = Body::new();
        for _ in 0..5 {
            resp.body_temperature(
                TCMB,
                &Worn::default(),
                true,
                None,
                2.0,
                &mut body.ctx(Stat::Conscious),
            );
        }
        assert!((resp.coretemperature - 256.7396).abs() < 1e-3);
        assert!((resp.bodytemperature - 210.2890).abs() < 1e-3);
    }

    #[test]
    fn body_temperature_drifts_and_hurts_like_tg() {
        // get_temp_change_amount: 14 * ln(1 + diff * rate).
        assert!((temp_change_amount(10.0, 0.1) - 14.0 * 2f64.ln()).abs() < 1e-9);
        assert!((temp_change_amount(-10.0, 0.1) + 14.0 * 2f64.ln()).abs() < 1e-9);

        // Unprotected in space (2.7 K): the skin falls, the core follows, cold damage starts.
        let mut resp = Respiration::default();
        let worn = Worn::default();
        let mut body = Body::new();
        resp.body_temperature(TCMB, &worn, true, None, 2.0, &mut body.ctx(Stat::Conscious));
        // Core: +0 (at normal) then skin->core of 0; skin: 14 * ln(1 - (2.7 - 310.15) * 0.102)
        // down, then core->skin back up by 14 * ln(1 + diff * 0.09) (capped at the difference).
        let area = -14.0 * (1.0 + (310.15 - TCMB) * 0.051 * 2.0).ln();
        let skin = BODYTEMP_NORMAL + area;
        let back = (14.0 * (1.0 + (BODYTEMP_NORMAL - skin) * 0.045 * 2.0).ln())
            .min(BODYTEMP_NORMAL - skin);
        assert!((resp.bodytemperature - (skin + back)).abs() < 1e-9);
        assert_eq!(resp.coretemperature, BODYTEMP_NORMAL);
        assert_eq!(body.health.burn, 0.0);
        let mut fires = 1;
        while resp.coretemperature >= BODYTEMP_COLD_DAMAGE_LIMIT {
            resp.body_temperature(TCMB, &worn, true, None, 2.0, &mut body.ctx(Stat::Conscious));
            fires += 1;
            assert!(fires < 200);
        }
        assert!(body.alerts.get(AlertCategory::Temperature).is_some());
        // Below the limit: COLD_DAMAGE_LEVEL_1 0.25 per second (0.5 per fire) or more.
        let before = body.health.burn;
        resp.coretemperature = 250.0;
        resp.body_temperature(TCMB, &worn, true, None, 2.0, &mut body.ctx(Stat::Conscious));
        assert!(body.health.burn - before >= 0.5 - 1e-6);

        // Heat: core 400 K burns max(log2(400 - 310.15) - 5, 0) * 0.5 * 2 s.
        let mut resp = Respiration {
            coretemperature: 400.0,
            bodytemperature: 400.0,
            ..default()
        };
        let mut body = Body::new();
        let mut ctx = body.ctx(Stat::Dead);
        resp.temperature_damage(None, 2.0, &mut ctx);
        let expected = (89.85f64.log2() - 5.0) * 0.5 * 2.0;
        // Spread over the bodyparts, each share rounded to `DAMAGE_PRECISION` (tg
        // `take_overall_damage`).
        assert!((f64::from(body.health.burn) - expected).abs() < 0.1);
        // Hot alerts by skin temperature.
        let mut ctx = body.ctx(Stat::Dead);
        resp.temperature_alerts(&mut ctx);
        assert_eq!(
            body.alerts
                .get(AlertCategory::Temperature)
                .map(Alert::icon_state)
                .as_deref(),
            Some("hot1")
        );
    }

    #[test]
    fn alerts_replace_within_a_category_at_the_end() {
        let mut alerts = Alerts::default();
        alerts.throw(AlertCategory::Pressure, "lowpressure", 1);
        alerts.throw(AlertCategory::NotEnoughOxygen, "not_enough_oxy", 0);
        alerts.throw(AlertCategory::Pressure, "lowpressure", 1);
        assert_eq!(alerts.0[0].category, AlertCategory::Pressure);
        alerts.throw(AlertCategory::Pressure, "lowpressure", 2);
        assert_eq!(alerts.0[1].icon_state(), "lowpressure2");
        alerts.clear(AlertCategory::Pressure);
        assert_eq!(alerts.0.len(), 1);
    }
}
