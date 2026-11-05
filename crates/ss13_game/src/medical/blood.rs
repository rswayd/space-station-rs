//! Blood (server), after tg `code/modules/mob/living/blood.dm` (`handle_blood`, `bleed`,
//! `bleed_warn`, `transfer_blood_to`), `blood_types.dm`, `carbon/life.dm` (`reagent_tick` of the
//! restoration chem, `reagent_expose` of injected blood), `reagent_containers/blood_pack.dm` and
//! `game/machinery/iv_drip.dm`.
//!
//! - Every human has [`Blood`]: 560 units of a blood type rolled with tg's
//!   `random_human_blood_type` weights. Each `Life()` (2 s) of a living human below 560 it
//!   regrows `0.25 × round(nutrition / 450, 0.2)` units per second (×1.25 when well fed),
//!   paid in nutrition; iron in the blood adds 0.25 per second while it lasts; bleeding wounds
//!   (`crate::medical::wounds`) and open vessels drain it. Low blood blurs, takes breaths and
//!   stamina, knocks out and below 112 units kills; missing blood turns into oxygen damage.
//! - A syringe draws blood (right click; someone else after 3 s); blood goes into containers as
//!   `/datum/reagent/blood` carrying its type ([`ss13_reagents::Reagent::data`]); injected blood
//!   becomes blood volume at once, incompatible blood adds half its volume of toxin.
//! - Blood packs start full of their type. An IV drip takes a pack (or a cup) clicked on it, is
//!   dragged onto a mob to attach (1 s), injects or draws `transfer_rate` (5 u/s) every
//!   `SSmachines` tick (2 s), and tears out (3 brute and a puncture on an arm) when the mob moves
//!   away. An empty-hand right click detaches it, ejects the container or switches the mode.
// shortcut: no blood sprays or hit splatters (drips and pools: `crate::janitor`), no dragging
// bleed, no DNA data (viruses: `super::diseases::BloodDiseases`), dizziness is only its message, `Unconscious` is a paralysis,
// no saline/salglu dilution or synthetic blood content; iron in the blood also metabolizes at its
// own rate besides refilling blood (tg skips the normal tick). The IV drip has no tgui window
// (`IVDrip`: rate, mode, eject) and no beam; its rate stays at 5 u/s.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay};

use crate::chem::{Chem, MobReagents, Reagents};
use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat, update_stat};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::server::PlayerMob;
use crate::tools::{Actor, DoAfter, do_after, tell};
use crate::zone::BodyZone;

/// `BLOOD_VOLUME_*` (`code/__DEFINES/mobs.dm`).
pub const BLOOD_VOLUME_MAXIMUM: f32 = 2000.0;
pub const BLOOD_VOLUME_MAX_LETHAL: f32 = BLOOD_VOLUME_MAXIMUM * 1.075;
pub const BLOOD_VOLUME_EXCESS: f32 = BLOOD_VOLUME_MAXIMUM * 1.05;
pub const BLOOD_VOLUME_NORMAL: f32 = 560.0;
pub const BLOOD_VOLUME_SAFE: f32 = BLOOD_VOLUME_NORMAL * (1.0 - 0.15);
pub const BLOOD_VOLUME_OKAY: f32 = BLOOD_VOLUME_NORMAL * (1.0 - 0.30);
pub const BLOOD_VOLUME_RISKY: f32 = BLOOD_VOLUME_NORMAL * (1.0 - 0.45);
pub const BLOOD_VOLUME_BAD: f32 = BLOOD_VOLUME_NORMAL * (1.0 - 0.60);
pub const BLOOD_VOLUME_SURVIVE: f32 = BLOOD_VOLUME_NORMAL * (1.0 - 0.80);
/// `BLOOD_REGEN_FACTOR`, `BLOOD_STOP_TEMP`, `BLEEDING_MESSAGE_BASE_CD` (seconds).
pub const BLOOD_REGEN_FACTOR: f32 = 0.25;
pub const BLOOD_STOP_TEMP: f64 = 225.0;
pub const BLEEDING_MESSAGE_BASE_CD: f64 = 10.0;
/// `NUTRITION_LEVEL_WELL_FED`, `HUNGER_FACTOR`, `OXYLOSS_PASSOUT_THRESHOLD`.
const NUTRITION_LEVEL_WELL_FED: f32 = 450.0;
const HUNGER_FACTOR: f32 = 0.05;
const OXYLOSS_PASSOUT_THRESHOLD: f32 = 50.0;
pub const BLOOD_REAGENT: &str = "/datum/reagent/blood";
const TOXIN_REAGENT: &str = "/datum/reagent/toxin";
const IRON_REAGENT: &str = "/datum/reagent/iron";
pub const BLOOD_PACK_ROOT: &str = "/obj/item/reagent_containers/blood";
pub const IV_DRIP_ROOT: &str = "/obj/machinery/iv_drip";
const CUP_ROOT: &str = "/obj/item/reagent_containers/cup";
/// IV drip `DEFAULT_IV_TRANSFER_RATE` (units per second), `SSmachines` wait.
pub const IV_TRANSFER_RATE: f32 = 5.0;
pub const MACHINES_WAIT_SECS: f64 = 2.0;

/// The human blood types (`/datum/blood_type/human/*`), the universal one and the species'
/// exotic ones (`exotic_bloodtype`, `crate::species`), each compatible only with itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BloodType {
    APlus,
    AMinus,
    BPlus,
    BMinus,
    AbPlus,
    AbMinus,
    OPlus,
    OMinus,
    Universal,
    Vampire,
    Lizard,
    Ethereal,
    Oil,
    Slime,
    Water,
    Snail,
    /// BandaStation `/datum/blood_type/skrell`, `/datum/blood_type/moth`.
    Skrell,
    Moth,
}

impl BloodType {
    pub const HUMAN: [Self; 8] = [
        Self::OMinus,
        Self::OPlus,
        Self::AMinus,
        Self::APlus,
        Self::BMinus,
        Self::BPlus,
        Self::AbMinus,
        Self::AbPlus,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::APlus => "A+",
            Self::AMinus => "A-",
            Self::BPlus => "B+",
            Self::BMinus => "B-",
            Self::AbPlus => "AB+",
            Self::AbMinus => "AB-",
            Self::OPlus => "O+",
            Self::OMinus => "O-",
            Self::Universal => "U",
            Self::Vampire => "V",
            Self::Lizard => "L",
            Self::Ethereal => "LE",
            Self::Oil => "Oil",
            Self::Slime => "TOX",
            Self::Water => "H2O",
            Self::Snail => "S",
            Self::Skrell => "SK",
            Self::Moth => "M",
        }
    }

    pub fn path(self) -> &'static str {
        match self {
            Self::APlus => "/datum/blood_type/human/a_plus",
            Self::AMinus => "/datum/blood_type/human/a_minus",
            Self::BPlus => "/datum/blood_type/human/b_plus",
            Self::BMinus => "/datum/blood_type/human/b_minus",
            Self::AbPlus => "/datum/blood_type/human/ab_plus",
            Self::AbMinus => "/datum/blood_type/human/ab_minus",
            Self::OPlus => "/datum/blood_type/human/o_plus",
            Self::OMinus => "/datum/blood_type/human/o_minus",
            Self::Universal => "/datum/blood_type/universal",
            Self::Vampire => "/datum/blood_type/universal/vampire",
            Self::Lizard => "/datum/blood_type/lizard",
            Self::Ethereal => "/datum/blood_type/ethereal",
            Self::Oil => "/datum/blood_type/oil",
            Self::Slime => "/datum/blood_type/slime",
            Self::Water => "/datum/blood_type/water",
            Self::Snail => "/datum/blood_type/snail",
            Self::Skrell => "/datum/blood_type/skrell",
            Self::Moth => "/datum/blood_type/moth",
        }
    }

    /// Every type.
    pub const ALL: [Self; 18] = [
        Self::OMinus,
        Self::OPlus,
        Self::AMinus,
        Self::APlus,
        Self::BMinus,
        Self::BPlus,
        Self::AbMinus,
        Self::AbPlus,
        Self::Universal,
        Self::Vampire,
        Self::Lizard,
        Self::Ethereal,
        Self::Oil,
        Self::Slime,
        Self::Water,
        Self::Snail,
        Self::Skrell,
        Self::Moth,
    ];

    pub fn from_path(path: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.path() == path)
    }

    /// `compatible_types` of this (recipient) type.
    pub fn compatible(self) -> &'static [Self] {
        use BloodType::*;
        match self {
            AMinus => &[AMinus, OMinus],
            APlus => &[AMinus, APlus, OMinus, OPlus],
            BMinus => &[BMinus, OMinus],
            BPlus => &[BMinus, BPlus, OMinus, OPlus],
            AbMinus => &[AMinus, BMinus, AbMinus, OMinus],
            AbPlus => &[AMinus, APlus, BMinus, BPlus, OMinus, OPlus, AbMinus, AbPlus],
            OMinus => &[OMinus],
            OPlus => &[OMinus, OPlus],
            // `compatible_types = subtypesof(/datum/blood_type)`.
            Universal | Vampire => &Self::ALL,
            Lizard => &[Lizard],
            Ethereal => &[Ethereal],
            Oil => &[Oil],
            Slime => &[Slime],
            Water => &[Water],
            Snail => &[Snail],
            Skrell => &[Skrell],
            Moth => &[Moth],
        }
    }

    /// The blood of `donor` (a type path) is safe for this recipient. A path that is no known
    /// type (another species' blood) is not.
    pub fn accepts(self, donor: &str) -> bool {
        Self::from_path(donor).is_some_and(|d| self.compatible().contains(&d))
    }

    /// tg `random_human_blood_type()`.
    pub fn random_human(rng: &mut CombatRng) -> Self {
        const WEIGHTS: [u32; 8] = [4, 36, 3, 28, 1, 20, 1, 5];
        let mut roll = rng.range(1, WEIGHTS.iter().sum());
        for (t, w) in Self::HUMAN.into_iter().zip(WEIGHTS) {
            if roll <= w {
                return t;
            }
            roll -= w;
        }
        Self::OPlus
    }
}

/// A human's blood (tg `blood_volume` and `dna.blood_type`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Blood {
    pub volume: f32,
    pub blood_type: BloodType,
    /// `bleeding_message_cd`: `Time::elapsed_secs_f64` of the next bleeding warning.
    pub next_bleed_warn: f64,
}

impl Blood {
    pub fn new(blood_type: BloodType) -> Self {
        Self {
            volume: BLOOD_VOLUME_NORMAL,
            blood_type,
            next_bleed_warn: 0.0,
        }
    }

    /// tg `adjust_blood_volume`: the change actually made.
    pub fn adjust(&mut self, amount: f32, minimum: f32, maximum: f32) -> f32 {
        if amount == 0.0 {
            return 0.0;
        }
        let old = self.volume;
        if amount < 0.0 {
            if old <= minimum {
                return 0.0;
            }
            self.volume = (old + amount).max(minimum);
        } else {
            if old >= maximum {
                return 0.0;
            }
            self.volume = (old + amount).min(maximum);
        }
        self.volume - old
    }
}

/// The blood regrown in `secs` and the nutrition it costs (tg `handle_blood`'s regeneration),
/// `heart` the heart's `get_blood_regeneration_multiplier`: `(restored, nutrition_cost)`.
pub fn regeneration(
    blood: &mut Blood,
    nutrition: f32,
    satiety: f32,
    secs: f32,
    heart: f32,
) -> (f32, f32) {
    if blood.volume >= BLOOD_VOLUME_NORMAL || heart <= 0.0 {
        return (0.0, 0.0);
    }
    // DM `round(x, 0.2)`.
    let mut ratio = (nutrition / NUTRITION_LEVEL_WELL_FED / 0.2).round() * 0.2;
    if satiety > 80.0 {
        ratio *= 1.25;
    }
    let to_restore = BLOOD_REGEN_FACTOR * heart * ratio * secs;
    let restored = blood.adjust(to_restore, 0.0, BLOOD_VOLUME_NORMAL);
    if restored > 0.0 {
        (
            restored,
            ratio * HUNGER_FACTOR * secs * (restored / to_restore),
        )
    } else {
        (0.0, 0.0)
    }
}

/// The oxygen damage missing blood deals in `secs` (the end of tg `handle_blood`).
pub fn blood_oxyloss(volume: f32, oxyloss: f32, unconscious: bool, secs: f32) -> f32 {
    let ratio = volume / BLOOD_VOLUME_NORMAL;
    let target = ((1.0 - ratio) * 100.0).max(0.0);
    if target > 0.0 && (oxyloss < target || (target >= OXYLOSS_PASSOUT_THRESHOLD && unconscious)) {
        // DM `round(x, 0.25)`.
        ((0.01 * (BLOOD_VOLUME_NORMAL - volume)) / 0.25).round() * 0.25 * secs
    } else {
        0.0
    }
}

/// The bands of tg `handle_blood`'s `switch(modified_blood_volume)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BloodBand {
    Lethal,
    Excess,
    Normal,
    Okay,
    Risky,
    Bad,
    Survive,
    Dead,
}

impl BloodBand {
    /// DM `switch` ranges are inclusive and the first match wins.
    pub fn of(volume: f32) -> Self {
        if (BLOOD_VOLUME_EXCESS..=BLOOD_VOLUME_MAX_LETHAL).contains(&volume) {
            Self::Lethal
        } else if (BLOOD_VOLUME_MAXIMUM..=BLOOD_VOLUME_EXCESS).contains(&volume) {
            Self::Excess
        } else if (BLOOD_VOLUME_OKAY..=BLOOD_VOLUME_SAFE).contains(&volume) {
            Self::Okay
        } else if (BLOOD_VOLUME_RISKY..=BLOOD_VOLUME_OKAY).contains(&volume) {
            Self::Risky
        } else if (BLOOD_VOLUME_BAD..=BLOOD_VOLUME_RISKY).contains(&volume) {
            Self::Bad
        } else if (BLOOD_VOLUME_SURVIVE..=BLOOD_VOLUME_BAD).contains(&volume) {
            Self::Survive
        } else if volume < BLOOD_VOLUME_SURVIVE {
            Self::Dead
        } else {
            Self::Normal
        }
    }
}

/// tg `bleed_warn` text for a bleed rate and its cooldown multiplier.
pub fn bleed_warning(rate: f32) -> Option<(&'static str, f64)> {
    Some(if rate <= 0.0 {
        return None;
    } else if rate <= 1.0 {
        ("Вы чувствуете легкие струйки крови на коже", 2.5)
    } else if rate <= 3.0 {
        (
            "Вы чувствуете небольшую струйку крови, текущую через ваше тело",
            2.0,
        )
    } else if rate <= 5.0 {
        (
            "Вы чувствуете, как ваша кожа становится липкой от протекающей крови",
            1.7,
        )
    } else if rate <= 7.0 {
        (
            "Вы чувствуете, как ваше тело немеет всё больше и больше по мере кровотечения",
            1.5,
        )
    } else {
        (
            "Ваше сердце бешенно стучит, пытаясь справиться с кровопотерей",
            1.0,
        )
    })
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

/// tg `bleed(amount)`: blood volume lost, sometimes a splatter on the floor
/// ([`crate::janitor::bleed_splatter`]).
pub fn bleed(world: &mut World, mob: Entity, amount: f32) {
    if crate::species::has_trait(world, mob, crate::species::Trait::NoBlood) {
        return;
    }
    let Some(mut blood) = world.get_mut::<Blood>(mob) else {
        return;
    };
    let bled = -blood.adjust(-amount, 0.0, BLOOD_VOLUME_MAXIMUM);
    crate::janitor::bleed_splatter(world, mob, bled);
}

/// One tg `handle_blood(seconds_per_tick)` of a living human.
pub fn handle_blood(world: &mut World, mob: Entity, secs: f32, now: f64) {
    let Some(mut blood) = world.get::<Blood>(mob).cloned() else {
        return;
    };
    if world.get::<Stat>(mob) == Some(&Stat::Dead)
        || crate::species::has_trait(world, mob, crate::species::Trait::NoBlood)
    {
        return;
    }
    let bodytemp = world
        .get::<crate::breath::Respiration>(mob)
        .map_or(crate::breath::BODYTEMP_NORMAL, |r| r.bodytemperature);
    if bodytemp < BLOOD_STOP_TEMP {
        return;
    }
    // Regeneration, paid in nutrition.
    if let Some(hunger) = world.get::<crate::hunger::Hunger>(mob).cloned() {
        let heart = super::organs::heart_blood_multiplier(world, mob);
        let (_, cost) = regeneration(&mut blood, hunger.nutrition, hunger.satiety, secs, heart);
        if cost > 0.0
            && let Some(mut h) = world.get_mut::<crate::hunger::Hunger>(mob)
        {
            h.adjust(-cost);
        }
    }
    restore_with_iron(world, mob, &mut blood, secs);
    // Bleeding.
    let lying = world
        .get::<crate::health::BodyPosition>(mob)
        .is_some_and(|b| b.lying());
    let rate = world
        .get::<super::wounds::Limbs>(mob)
        .map_or(0.0, |l| l.bleed_rate(lying));
    if rate > 0.0 {
        let bled = -blood.adjust(-rate * secs, 0.0, BLOOD_VOLUME_MAXIMUM);
        crate::janitor::bleed_splatter(world, mob, bled);
        if now >= blood.next_bleed_warn
            && world.get::<PlayerMob>(mob).is_some()
            && let Some((text, cd)) = bleed_warning(rate)
        {
            blood.next_bleed_warn = now + BLEEDING_MESSAGE_BASE_CD * cd;
            let trend = bleed_trend(world, mob);
            tell_mob(world, mob, Span::Warning, format!("{text}{trend}"));
        }
    }
    let volume = blood.volume;
    world.entity_mut(mob).insert(blood);
    blood_loss_effects(world, mob, volume, secs);
}

/// tg `bleed_warn`'s `rate_of_change`.
fn bleed_trend(world: &World, mob: Entity) -> &'static str {
    use super::wounds::{Series, WoundKind};
    let Some(limbs) = world.get::<super::wounds::Limbs>(mob) else {
        return ", но кровотечение чуть замедляется.";
    };
    let mut steady = false;
    for limb in &limbs.parts {
        for wound in limb.wounds.iter().filter(|w| w.blood_flow > 0.0) {
            let def = wound.def();
            let gauzed = limb.gauze.is_some();
            match def.series {
                Series::Slash if wound.kind == WoundKind::SlashCritical && !gauzed => {
                    return ", <b>и всё становится только хуже!</b>";
                }
                Series::Pierce if !gauzed && def.clot_rate == 0.0 => steady = true,
                _ => {}
            }
        }
    }
    if steady {
        ", и скорость кровопотери не изменяется."
    } else {
        ", но кровотечение чуть замедляется."
    }
}

/// tg `reagent_tick` of the restoration chem (iron) below normal blood.
fn restore_with_iron(world: &mut World, mob: Entity, blood: &mut Blood, secs: f32) {
    if blood.volume >= BLOOD_VOLUME_NORMAL {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(iron) = chem.id(IRON_REAGENT) else {
        return;
    };
    let Some(mut reagents) = world.get_mut::<MobReagents>(mob) else {
        return;
    };
    if reagents.blood.amount(iron) <= 0.0 {
        return;
    }
    blood.adjust(BLOOD_REGEN_FACTOR * secs, 0.0, BLOOD_VOLUME_MAXIMUM);
    let rate = reagents
        .blood
        .has(iron)
        .map_or(0.0, |r| r.metabolization_rate);
    reagents.blood.remove_reagent(iron, rate * secs);
}

/// The effects of low (or too much) blood: messages, breath, stamina, fainting, death, and the
/// oxygen damage of missing blood.
fn blood_loss_effects(world: &mut World, mob: Entity, volume: f32, secs: f32) {
    let mut rng = super::fork_rng(world);
    let stamina = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .map_or(0.0, |e| e.stamina);
    let word = ["головокружение", "дурость", "слабость"][rng.range(0, 2) as usize];
    let spt = |rng: &mut CombatRng, p: f32| super::wounds::spt_prob(rng, p, secs);
    match BloodBand::of(volume) {
        // shortcut: no `inflate_gib`: lethal excess only warns.
        BloodBand::Lethal | BloodBand::Excess => {
            if spt(&mut rng, 5.0) {
                tell_mob(
                    world,
                    mob,
                    Span::Warning,
                    "Вы чувствуете, как ужасно вздулись.",
                );
            }
        }
        BloodBand::Okay => {
            if spt(&mut rng, 2.5) {
                crate::vision::set_eye_blur_if_lower(world, mob, 2.0);
                if rng.chance(50.0) {
                    tell_mob(
                        world,
                        mob,
                        Span::Danger,
                        format!("Вы чувствуете {word}. Становится немного сложнее дышать."),
                    );
                    lose_breath(world, mob, 0.5 * secs);
                } else if stamina < 25.0 {
                    tell_mob(
                        world,
                        mob,
                        Span::Danger,
                        format!(
                            "Вы чувствуете {word}. Становится немного сложнее сфокусироваться."
                        ),
                    );
                    crate::status_effects::adjust_stamina(world, mob, 5.0 * secs);
                }
            }
        }
        BloodBand::Risky | BloodBand::Bad => {
            let bad = BloodBand::of(volume) == BloodBand::Bad;
            if spt(&mut rng, 5.0) {
                crate::vision::set_eye_blur_if_lower(world, mob, if bad { 4.0 } else { 2.0 });
                if rng.chance(50.0) {
                    let text = if bad {
                        format!(
                            "Вы очень сильно чувствуете {word}! Становится гораздо сложнее дышать!"
                        )
                    } else {
                        format!("Вы сильно чувствуете {word}. Становится сложнее дышать!")
                    };
                    tell_mob(world, mob, Span::Danger, text);
                    lose_breath(world, mob, if bad { 1.5 } else { 1.0 });
                } else if stamina < if bad { 80.0 } else { 40.0 } {
                    let text = if bad {
                        format!(
                            "Вы очень сильно чувствуете {word}! Становится гораздо сложнее оставаться в сознании!"
                        )
                    } else {
                        format!(
                            "Вы сильно чувствуете {word}. Становится сложнее оставаться в сознании!"
                        )
                    };
                    tell_mob(world, mob, Span::Danger, text);
                    crate::status_effects::adjust_stamina(world, mob, if bad { 10.0 } else { 7.5 });
                }
            }
        }
        BloodBand::Survive => {
            if spt(&mut rng, 7.5) {
                let secs = rng.range(10, 20) as f32 / 10.0;
                crate::status_effects::apply(
                    world,
                    mob,
                    crate::status_effects::Incap::Paralyze,
                    secs,
                );
                tell_mob(
                    world,
                    mob,
                    Span::BoldWarning,
                    "Вы на мгновение вырубаетесь!",
                );
            }
        }
        BloodBand::Dead => {
            if let Some(mut stat) = world.get_mut::<Stat>(mob) {
                stat.set_if_neq(Stat::Dead);
            }
            return;
        }
        BloodBand::Normal => {}
    }
    let Some(mut health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    let mut stat = world.get::<Stat>(mob).copied().unwrap_or_default();
    let unconscious = health.knocked_out(stat)
        || world
            .get::<crate::status_effects::StatusEffects>(mob)
            .is_some_and(|e| e.paralyze > 0.0);
    let oxy = blood_oxyloss(volume, health.oxy, unconscious, secs);
    if oxy > 0.0 {
        health.adjust(DamageType::Oxy, oxy);
        update_stat(&health, &mut stat);
        world.entity_mut(mob).insert(health);
        if let Some(mut s) = world.get_mut::<Stat>(mob) {
            s.set_if_neq(stat);
        }
    }
}

fn lose_breath(world: &mut World, mob: Entity, amount: f32) {
    if let Some(mut resp) = world.get_mut::<crate::breath::Respiration>(mob) {
        resp.losebreath += amount;
    }
}

/// tg `transfer_blood_to(receiver)` from a living mob into a container: blood with the donor's
/// type, at most what keeps the donor above `BLOOD_VOLUME_BAD` (unless `ignore_low_blood`).
/// Returns the units moved.
pub fn transfer_blood_to(
    world: &mut World,
    donor: Entity,
    container: Entity,
    amount: f32,
    ignore_low: bool,
) -> f32 {
    let Some(blood) = world.get::<Blood>(donor).cloned() else {
        return 0.0;
    };
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return 0.0;
    };
    let Some(id) = chem.id(BLOOD_REAGENT) else {
        return 0.0;
    };
    if blood.volume <= 0.0 || amount <= 0.0 || world.get::<Reagents>(container).is_none() {
        return 0.0;
    }
    if blood.volume < BLOOD_VOLUME_BAD && !ignore_low {
        return 0.0;
    }
    let mut amount = amount.min(blood.volume);
    if !ignore_low {
        amount = amount.min(blood.volume - BLOOD_VOLUME_BAD);
    }
    let temp = world
        .get::<crate::breath::Respiration>(donor)
        .map_or(crate::breath::BODYTEMP_NORMAL, |r| r.bodytemperature) as f32;
    let Some(mut reagents) = world.get_mut::<Reagents>(container) else {
        return 0.0;
    };
    let added = reagents
        .0
        .add_reagent(&chem, id, amount, temp, None, None, false, false);
    reagents.0.set_data_if_none(id, blood.blood_type.path());
    if let Some(mut b) = world.get_mut::<Blood>(donor) {
        b.adjust(-added, 0.0, BLOOD_VOLUME_MAXIMUM);
    }
    if added > 0.0 {
        super::diseases::blood_drawn(world, donor, container);
    }
    added
}

/// tg carbon `reagent_expose(blood, INJECT)`: injected blood joins the volume at once; blood of
/// an incompatible type also adds half as much toxin.
pub fn expose_inject(world: &mut World, mob: Entity, injected: f32, data: Option<String>) {
    if injected <= 0.0 {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let (Some(blood_id), Some(blood)) = (chem.id(BLOOD_REAGENT), world.get::<Blood>(mob).cloned())
    else {
        return;
    };
    let mut new = blood.clone();
    let added = new.adjust(
        (injected * 100.0).round() / 100.0,
        0.0,
        BLOOD_VOLUME_MAXIMUM,
    );
    world.entity_mut(mob).insert(new);
    let Some(mut reagents) = world.get_mut::<MobReagents>(mob) else {
        return;
    };
    reagents.blood.remove_reagent(blood_id, added);
    if let Some(donor) = data
        && !blood.blood_type.accepts(&donor)
        && let Some(toxin) = chem.id(TOXIN_REAGENT)
    {
        reagents.blood.add_reagent(
            &chem,
            toxin,
            injected * 0.5,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            false,
            false,
        );
    }
}

/// The blood type a blood pack starts with (tg `blood_type`, `/random` picks a roundstart
/// human type).
pub fn pack_blood_type(path: &str, blood_type: &str, rng: &mut CombatRng) -> Option<&'static str> {
    if !is_under(path, BLOOD_PACK_ROOT) {
        return None;
    }
    if is_under(path, "/obj/item/reagent_containers/blood/random") {
        let pick = BloodType::HUMAN[rng.range(0, 7) as usize];
        return Some(pick.path());
    }
    BloodType::from_path(blood_type).map(BloodType::path)
}

/// tg `/datum/blood_type/get_type` for a blood reagent's data.
pub fn type_name(path: &str) -> &str {
    BloodType::from_path(path).map_or(path, |t| t.name())
}

/// tg `syringe/interact_with_atom_secondary` on a living mob: a blood sample (someone else after
/// 3 s). True when the click was handled.
pub fn syringe_draw_blood(world: &mut World, actor: Actor, syringe: Entity, target: Entity) {
    if target != actor.mob {
        if world.get::<DoAfter>(actor.mob).is_some() {
            return;
        }
        let (user, victim) = (
            super::name_of(world, actor.mob),
            super::name_of(world, target),
        );
        super::visible(
            world,
            actor.mob,
            &format!("Вы пытаетесь взять пробу крови у {victim}!"),
            &format!("{user} пытается взять пробу крови у {victim}!"),
            Span::Danger,
        );
        do_after(
            world,
            actor,
            target,
            crate::chem::CHEM_INTERACT_DELAY,
            0,
            crate::construction::Job::Medical(super::MedJob::DrawBlood),
        );
        return;
    }
    finish_draw(world, actor, syringe, target);
}

/// The end of a blood sample (after the `do_after` for someone else).
pub fn finish_draw(world: &mut World, actor: Actor, syringe: Entity, target: Entity) {
    let Some(holder) = world.get::<Reagents>(syringe).map(|r| r.0.clone()) else {
        return;
    };
    if holder.holder_full() {
        return;
    }
    let drawn = holder.maximum_volume - holder.total_volume;
    let (user, victim) = (
        super::name_of(world, actor.mob),
        super::name_of(world, target),
    );
    if transfer_blood_to(world, target, syringe, drawn, false) > 0.0 {
        let text = format!("{user} takes a blood sample from {victim}.");
        super::visible(world, actor.mob, &text, &text, Span::Notice);
    } else {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("You are unable to draw any blood from {victim}!"),
        );
    }
}

/// An IV drip (`/obj/machinery/iv_drip`): its container (inside it), the attached mob and the
/// mode (tg `mode`: `IV_INJECTING` true, taking blood false).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct IvDrip {
    pub container: Option<Entity>,
    pub attached: Option<Entity>,
    pub injecting: bool,
    pub transfer_rate: f32,
}

impl Default for IvDrip {
    fn default() -> Self {
        Self {
            container: None,
            attached: None,
            injecting: true,
            transfer_rate: IV_TRANSFER_RATE,
        }
    }
}

/// tg `iv_drip/item_interaction`: a blood pack or a cup goes onto the drip.
pub fn iv_item_interaction(world: &mut World, actor: Actor, held: Entity, drip: Entity) -> bool {
    let Some(state) = world.get::<IvDrip>(drip).cloned() else {
        return false;
    };
    let path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if !(is_under(&path, BLOOD_PACK_ROOT) || is_under(&path, CUP_ROOT)) {
        return false;
    }
    if state.container.is_some() {
        return true;
    }
    crate::storage::put_inside(world, held, drip);
    world.entity_mut(drip).insert(IvDrip {
        container: Some(held),
        ..state
    });
    refresh_drip(world, drip);
    let _ = actor;
    true
}

/// tg `iv_drip/attack_hand_secondary` -> `quick_toggle`.
pub fn iv_quick_toggle(world: &mut World, actor: Actor, drip: Entity) -> bool {
    let Some(mut state) = world.get::<IvDrip>(drip).cloned() else {
        return false;
    };
    if state.attached.is_some() {
        state.attached = None;
        tell(world, actor.client, Span::Notice, "Вы вынимаете иглу.");
    } else if let Some(container) = state.container.take() {
        if let Some(&pos) = world.get::<TilePos>(drip) {
            crate::storage::take_out(world, container, pos);
        }
    } else {
        state.injecting = !state.injecting;
        let mode = if state.injecting {
            "вливание"
        } else {
            "забор крови"
        };
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("Режим капельницы: {mode}."),
        );
    }
    world.entity_mut(drip).insert(state);
    refresh_drip(world, drip);
    true
}

/// tg `iv_drip/mouse_drop_dragged` onto a mob: `attach_iv` after 1 s.
pub fn iv_mouse_drop(world: &mut World, actor: Actor, drip: Entity, target: Entity) -> bool {
    let Some(state) = world.get::<IvDrip>(drip) else {
        return false;
    };
    if state.container.is_none() || world.get::<MobReagents>(target).is_none() {
        return false;
    }
    let actor = Actor {
        item: world
            .get::<Inventory>(actor.mob)
            .and_then(Inventory::active_item),
        ..actor
    };
    do_after(
        world,
        actor,
        target,
        10.0,
        0,
        crate::construction::Job::Medical(super::MedJob::AttachIv { drip }),
    );
    true
}

/// The end of `attach_iv`.
pub fn finish_attach(world: &mut World, actor: Actor, drip: Entity, target: Entity) {
    let Some(mut state) = world.get::<IvDrip>(drip).cloned() else {
        return;
    };
    let in_reach = match (world.get::<TilePos>(drip), world.get::<TilePos>(target)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    };
    if state.container.is_none() || !in_reach {
        return;
    }
    state.attached = Some(target);
    world.entity_mut(drip).insert(state);
    let (user, victim) = (
        super::name_of(world, actor.mob),
        super::name_of(world, target),
    );
    super::visible(
        world,
        actor.mob,
        &format!("Вы подключаете капельницу к {victim}."),
        &format!("{user} подключает капельницу к {victim}."),
        Span::Notice,
    );
    refresh_drip(world, drip);
}

/// tg `iv_drip/process` of every attached drip each `SSmachines` tick.
// Cost: every 2 s one pass over the IV drips (a handful per station).
pub fn process_drips(world: &mut World, secs: f32) {
    let drips: Vec<Entity> = world
        .query_filtered::<Entity, With<IvDrip>>()
        .iter(world)
        .collect();
    for drip in drips {
        let Some(mut state) = world.get::<IvDrip>(drip).cloned() else {
            continue;
        };
        let Some(mob) = state.attached else {
            continue;
        };
        let near = match (world.get::<TilePos>(drip), world.get::<TilePos>(mob)) {
            (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
            _ => false,
        };
        if !near {
            // The needle rips out of an arm.
            let zone = if world.resource_mut::<CombatRng>().chance(50.0) {
                BodyZone::LArm
            } else {
                BodyZone::RArm
            };
            if let Some(mut health) = world.get::<Health>(mob).cloned() {
                health.damage_zone(zone, DamageType::Brute, 3.0);
                world.entity_mut(mob).insert(health);
            }
            if let Some(mut limbs) = world.get::<super::wounds::Limbs>(mob).cloned() {
                let limb = limbs.limb_mut(zone);
                if limb.wound(super::wounds::Series::Pierce).is_none() {
                    super::wounds::apply_wound(limb, super::wounds::WoundKind::PierceModerate);
                }
                world.entity_mut(mob).insert(limbs);
            }
            tell_mob(
                world,
                mob,
                Span::BoldWarning,
                "Игла капельницы вырывается из вашей руки!",
            );
            state.attached = None;
            world.entity_mut(drip).insert(state);
            refresh_drip(world, drip);
            continue;
        }
        let Some(container) = state.container else {
            continue;
        };
        let amount = state.transfer_rate * secs;
        if state.injecting {
            crate::chem::inject_into_mob(world, container, mob, amount);
        } else if let Some(holder) = world.get::<Reagents>(container).map(|r| r.0.clone()) {
            let room = (holder.maximum_volume - holder.total_volume).min(amount);
            if room > 0.0 {
                transfer_blood_to(world, mob, container, room, false);
            }
        }
        refresh_drip(world, drip);
    }
}

/// tg `iv_drip/update_icon_state` and `update_overlays`.
pub fn refresh_drip(world: &mut World, drip: Entity) {
    let Some(state) = world.get::<IvDrip>(drip).cloned() else {
        return;
    };
    let Some(icons) = world.get_resource::<ss13_mapload::IconIndex>() else {
        return;
    };
    let dmi = "icons/obj/medical/iv_drip.dmi";
    let base = if state.transfer_rate > 0.0 && state.attached.is_some() {
        if state.injecting {
            "iv_drip_injecting"
        } else {
            "iv_drip_donating"
        }
    } else if state.injecting {
        "iv_drip_injectidle"
    } else {
        "iv_drip_donateidle"
    };
    let icon = icons.get(dmi, base);
    let mut overlays = Vec::new();
    if let Some(container) = state.container {
        let beaker = if state.attached.is_some() {
            "beakeractive"
        } else {
            "beakeridle"
        };
        if let Some(id) = icons.get(dmi, beaker) {
            overlays.push(Overlay::new(id));
        }
        if let Some(holder) = world.get::<Reagents>(container).map(|r| &r.0)
            && holder.maximum_volume > 0.0
        {
            let percent = (100.0 * holder.total_volume / holder.maximum_volume).ceil();
            let threshold = [0.0, 10.0, 25.0, 50.0, 75.0, 80.0, 90.0]
                .into_iter()
                .rfind(|t| percent >= *t);
            if let (Some(t), Some(chem)) = (threshold, world.get_resource::<Chem>())
                && let Some(id) = icons.get(dmi, &format!("reagent{t}"))
            {
                let mut fill = Overlay::new(id);
                fill.color = ss13_reagents::mix_color(&chem.0, &holder.reagents);
                overlays.push(fill);
            }
        }
    }
    if let Some(mut appearance) = world.get_mut::<Appearance>(drip) {
        if let Some(icon) = icon
            && appearance.icon_state != icon
        {
            appearance.icon_state = icon;
        }
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blood_loss_math_follows_handle_blood() {
        // Half the blood: target oxyloss 50, 2.75 per second (round(2.8, 0.25)).
        assert_eq!(blood_oxyloss(280.0, 0.0, false, 2.0), 5.5);
        // At the target nothing more, unless unconscious past the passout threshold.
        assert_eq!(blood_oxyloss(280.0, 50.0, false, 2.0), 0.0);
        assert_eq!(blood_oxyloss(280.0, 60.0, true, 2.0), 5.5);
        // 90% blood: 0.5 per second, up to 10 oxyloss.
        assert_eq!(blood_oxyloss(504.0, 0.0, false, 1.0), 0.5);
        assert_eq!(blood_oxyloss(504.0, 10.5, false, 1.0), 0.0);
        assert_eq!(blood_oxyloss(560.0, 0.0, false, 2.0), 0.0);
        // Bands.
        assert_eq!(BloodBand::of(560.0), BloodBand::Normal);
        assert_eq!(BloodBand::of(BLOOD_VOLUME_SAFE), BloodBand::Okay);
        assert_eq!(BloodBand::of(350.0), BloodBand::Risky);
        assert_eq!(BloodBand::of(250.0), BloodBand::Bad);
        assert_eq!(BloodBand::of(150.0), BloodBand::Survive);
        assert_eq!(BloodBand::of(111.0), BloodBand::Dead);
        assert_eq!(BloodBand::of(2050.0), BloodBand::Excess);
    }

    #[test]
    fn regeneration_scales_with_nutrition_and_satiety() {
        let mut blood = Blood::new(BloodType::OPlus);
        blood.volume = 500.0;
        // 400 nutrition: round(0.888, 0.2) = 0.8 -> 0.2 u/s, 0.04 nutrition/s.
        let (restored, cost) = regeneration(&mut blood, 400.0, 0.0, 2.0, 1.0);
        assert!((restored - 0.4).abs() < 1e-5, "{restored}");
        assert!((cost - 0.08).abs() < 1e-5, "{cost}");
        // Well fed (satiety > 80): ×1.25.
        let (restored, _) = regeneration(&mut blood, 400.0, 100.0, 2.0, 1.0);
        assert!((restored - 0.5).abs() < 1e-5, "{restored}");
        // Starving or without a beating heart: nothing regrows; a half-healthy heart, half.
        assert_eq!(regeneration(&mut blood, 40.0, 0.0, 2.0, 1.0), (0.0, 0.0));
        assert_eq!(regeneration(&mut blood, 400.0, 0.0, 2.0, 0.0), (0.0, 0.0));
        let (restored, _) = regeneration(&mut blood, 400.0, 0.0, 2.0, 0.5);
        assert!((restored - 0.2).abs() < 1e-4, "{restored}");
        // Capped at normal.
        blood.volume = 559.9;
        let (restored, _) = regeneration(&mut blood, 450.0, 0.0, 2.0, 1.0);
        assert!((restored - 0.1).abs() < 1e-3 && blood.volume == 560.0);
    }

    #[test]
    fn blood_types_compatibility_and_weights() {
        assert!(BloodType::AbPlus.accepts(BloodType::OMinus.path()));
        assert!(BloodType::OPlus.accepts(BloodType::OMinus.path()));
        assert!(!BloodType::OMinus.accepts(BloodType::OPlus.path()));
        assert!(!BloodType::APlus.accepts(BloodType::BMinus.path()));
        assert!(!BloodType::OPlus.accepts("/datum/blood_type/lizard"));
        assert!(BloodType::Universal.accepts(BloodType::BPlus.path()));
        assert_eq!(type_name("/datum/blood_type/human/ab_minus"), "AB-");
        let mut rng = CombatRng(5);
        let o_plus = (0..9800)
            .filter(|_| BloodType::random_human(&mut rng) == BloodType::OPlus)
            .count();
        assert!((3200..3900).contains(&o_plus), "36 of 98: {o_plus}");
        let mut b = Blood::new(BloodType::OPlus);
        assert_eq!(b.adjust(-100.0, 0.0, BLOOD_VOLUME_MAXIMUM), -100.0);
        assert_eq!(b.adjust(1000.0, 0.0, BLOOD_VOLUME_NORMAL), 100.0);
        assert_eq!(b.adjust(10.0, 0.0, BLOOD_VOLUME_NORMAL), 0.0);
    }
}
