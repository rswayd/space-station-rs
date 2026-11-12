//! Pyrotechnic reagents on mobs and turfs (server), after tg
//! `chemistry/reagents/pyrotechnic_reagents.dm`: their `on_mob_life` ([`metabolized`], handed
//! over by `crate::chem` every `SSmobs` tick) and `expose_*` ([`expose`], from
//! `crate::janitor::expose_by`).
//!
//! - Thermite in the blood burns (0.5 a second); nitroglycerin heals the heart.
//! - Chlorine trifluoride in the blood stokes the fire stacks and burns by them; splashed on a
//!   mob it adds stacks, ignites it and lights a fire under it; on a floor it may strip the tile
//!   or burn it, and lights the 3×3 around.
//! - Phlogiston in the blood stokes and burns; on a mob a stack, a burn and fire. Napalm in the
//!   blood stokes; touching it (or a patch, a vapor) adds up to 20 stacks.
//! - Cryostylane and pyrosium in the blood burn oxygen there to cool or heat the body.
//! - Teslium shocks its host every 5-30 ticks for 5-20; energized jelly does that to all but
//!   slimes, whom it rouses and rests instead.
// The explosions, flashes, smoke and sparks of these reagents are reactions (`crate::effects`).
// shortcut: no thermite on walls (`/datum/component/thermite`: thermite only burns the
// drinker), no doubled electrical conductivity of teslium (`PHYS_COEFF_ELEC_CONDUCTIVITY`), no
// cryostylane surgery slowdown, cyan tint, rate change by life or death, or slime damage on
// turfs, no luminescent extract cooldown of energized jelly.

use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_atmos::reactions::FIRE_MINIMUM_TEMPERATURE_TO_EXIST;
use ss13_reagents::metabolism::Metabolized;
use ss13_reagents::{Chemistry, ReagentId};
use ss13_shared::TilePos;

use crate::combat::CombatRng;
use crate::health::DamageType;
use crate::medical::organs::{self, Slot};

pub const THERMITE: &str = "/datum/reagent/thermite";
pub const NITROGLYCERIN: &str = "/datum/reagent/nitroglycerin";
pub const CLF3: &str = "/datum/reagent/clf3";
pub const PHLOGISTON: &str = "/datum/reagent/phlogiston";
pub const NAPALM: &str = "/datum/reagent/napalm";
pub const CRYOSTYLANE: &str = "/datum/reagent/cryostylane";
pub const PYROSIUM: &str = "/datum/reagent/pyrosium";
pub const TESLIUM: &str = "/datum/reagent/teslium";
pub const ENERGIZED_JELLY: &str = "/datum/reagent/teslium/energized_jelly";
const OXYGEN: &str = "/datum/reagent/oxygen";
/// `/obj/effect/hotspot` defaults (`volume`, `FIRE_MINIMUM_TEMPERATURE_TO_EXIST`).
const HOTSPOT_VOLUME: f64 = 125.0;
const FLOOR: &str = "/turf/open/floor";
const PLATING: &str = "/turf/open/floor/plating";

/// Teslium's `shock_timer` (ticks since the last shock).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TesliumTimer(pub u32);

fn rand(world: &mut World, low: u32, high: u32) -> u32 {
    world.get_resource_or_init::<CombatRng>().range(low, high)
}

fn chance(world: &mut World, percent: f32) -> bool {
    world.get_resource_or_init::<CombatRng>().chance(percent)
}

fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    crate::mobs::defense::hurt(world, mob, kind, amount, None);
}

fn fire_stacks(world: &World, mob: Entity) -> f32 {
    world
        .get::<crate::fire::FireStacks>(mob)
        .map_or(0.0, |f| f.stacks)
}

/// Burns `amount` of the blood's oxygen (as much as is there); true if there was any.
fn burn_oxygen(world: &mut World, chem: &Chemistry, mob: Entity, amount: f32) -> bool {
    let Some(oxygen) = chem.id(OXYGEN) else {
        return false;
    };
    let Some(mut m) = world.get_mut::<crate::chem::MobReagents>(mob) else {
        return false;
    };
    if m.blood.has(oxygen).is_none() {
        return false;
    }
    m.blood.remove_reagent(oxygen, amount);
    true
}

/// `adjust_bodytemperature` and, for a human, `adjust_coretemperature`.
fn warm(world: &mut World, mob: Entity, kelvin: f32) {
    let human = world.get::<crate::human::HumanLook>(mob).is_some();
    if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
        r.bodytemperature = (r.bodytemperature + f64::from(kelvin)).max(0.0);
        if human {
            r.coretemperature = (r.coretemperature + f64::from(kelvin)).max(0.0);
        }
    }
}

/// The `on_mob_life` of the pyrotechnics metabolized this tick.
pub fn metabolized(
    world: &mut World,
    chem: &Chemistry,
    mob: Entity,
    list: &[Metabolized],
    secs: f32,
) {
    let mut teslium = false;
    for m in list {
        let def = chem.def(m.id);
        let r = m.ratio;
        match def.path.as_str() {
            THERMITE => hurt(world, mob, DamageType::Burn, 0.5 * r * secs),
            NITROGLYCERIN => {
                // `normalise_creation_purity()`.
                let purity = m.creation_purity / def.purity;
                organs::adjust_organ_loss(world, mob, Slot::Heart, -0.5 * r * secs * purity);
            }
            CLF3 => {
                crate::fire::adjust_fire_stacks(world, mob, 0.1 * r * secs);
                let stacks = fire_stacks(world, mob).max(1.0);
                hurt(world, mob, DamageType::Burn, 0.015 * stacks * r * secs);
            }
            PHLOGISTON => {
                crate::fire::adjust_fire_stacks(world, mob, 0.5 * r * secs);
                let stacks = fire_stacks(world, mob).max(0.15);
                hurt(world, mob, DamageType::Burn, 0.15 * stacks * r * secs);
            }
            NAPALM => crate::fire::adjust_fire_stacks(world, mob, 0.5 * r * secs),
            CRYOSTYLANE => {
                if burn_oxygen(world, chem, mob, r * secs) {
                    warm(world, mob, -30.0 * r * secs);
                }
            }
            PYROSIUM => {
                if burn_oxygen(world, chem, mob, 0.5 * r * secs) {
                    warm(world, mob, 15.0 * r * secs);
                }
            }
            TESLIUM | ENERGIZED_JELLY => {
                let slime = world.get::<crate::human::HumanLook>(mob).is_some()
                    && crate::species::def_of(world, mob).biotypes & crate::species::biotype::SLIME
                        != 0;
                if def.path == ENERGIZED_JELLY && slime {
                    // `-20` deciseconds.
                    if let Some(mut effects) = crate::status_effects::effects_mut(world, mob) {
                        effects.adjust_all_immobility(-2.0 * r * secs);
                    }
                    crate::status_effects::adjust_stamina(world, mob, -5.0 * r * secs);
                    continue;
                }
                teslium = true;
                teslium_tick(world, mob);
            }
            _ => {}
        }
    }
    // A new teslium starts its own timer.
    if !teslium && world.get::<TesliumTimer>(mob).is_some() {
        world.entity_mut(mob).remove::<TesliumTimer>();
    }
}

/// `teslium/on_mob_life`: `shock_timer++`, a shock at `rand(5, 30)` or more.
fn teslium_tick(world: &mut World, mob: Entity) {
    let timer = world.get::<TesliumTimer>(mob).map_or(0, |t| t.0) + 1;
    if timer < rand(world, 5, 30) {
        world.entity_mut(mob).insert(TesliumTimer(timer));
        return;
    }
    world.entity_mut(mob).insert(TesliumTimer(0));
    let damage = rand(world, 5, 20) as f32;
    // `SHOCK_NOGLOVES`: from inside, the gloves do not matter.
    crate::wires::electrocute_act(world, mob, damage, mob, 1.0);
}

/// `new /obj/effect/hotspot(pos)` unless one is there (`locate(/obj/effect/hotspot)`).
fn hotspot(world: &mut World, pos: TilePos) {
    if let Some(mut atmos) = world.get_resource_mut::<Atmos>()
        && atmos.hotspot(pos).is_none()
    {
        atmos.new_hotspot(pos, HOTSPOT_VOLUME, FIRE_MINIMUM_TEMPERATURE_TO_EXIST);
    }
}

fn turf_path(world: &World, pos: TilePos) -> Option<String> {
    crate::construction::turf_at(world, pos).and_then(|t| crate::construction::path_of(world, t))
}

/// `expose_mob` / `expose_turf` of the pyrotechnics; `living` and `turf` as
/// `crate::janitor::expose_by` found the target.
#[allow(clippy::too_many_arguments)]
pub fn expose(
    world: &mut World,
    chem: &Chemistry,
    target: Entity,
    id: ReagentId,
    volume: f32,
    methods: u32,
    touch_protection: f32,
    turf: Option<TilePos>,
    living: bool,
) {
    use crate::janitor::{PATCH, TOUCH, VAPOR};
    let path = chem.def(id).path.as_str();
    match (path, turf) {
        (CLF3, Some(pos)) => clf3_turf(world, pos, volume),
        (CLF3, None) if living => {
            crate::fire::adjust_fire_stacks(world, target, (volume / 5.0).min(10.0));
            crate::fire::ignite_mob(world, target);
            if let Some(&pos) = world.get::<TilePos>(target) {
                hotspot(world, pos);
            }
        }
        (PHLOGISTON, None) if living => {
            crate::fire::adjust_fire_stacks(world, target, 1.0);
            let burn = (0.3 * fire_stacks(world, target) * (1.0 - touch_protection)).max(0.3);
            hurt(world, target, DamageType::Burn, burn);
            crate::fire::ignite_mob(world, target);
        }
        (NAPALM, None) if living && methods & (TOUCH | VAPOR | PATCH) != 0 => {
            crate::fire::adjust_fire_stacks(world, target, (volume / 4.0).min(20.0));
        }
        _ => {}
    }
}

/// `clf3/expose_turf`: plating may be scraped away (10% + 5% broken + 10% burnt), a floor
/// loses its tile (`volume`%) or else burns (`volume`%); a floor lights the 3×3 around.
fn clf3_turf(world: &mut World, pos: TilePos, volume: f32) {
    let is = |world: &World, root: &str| {
        turf_path(world, pos).is_some_and(|p| ss13_mapload::fidelity::is_under(&p, root))
    };
    if is(&*world, PLATING) {
        let damaged = crate::construction::turf_at(world, pos)
            .and_then(|t| world.get::<crate::turf_damage::TurfDamaged>(t).copied())
            .unwrap_or_default();
        let percent = 10.0
            + f32::from(u8::from(damaged.burnt)) * 1.0
            + 5.0 * f32::from(u8::from(damaged.broken));
        if chance(world, percent) {
            crate::construction::scrape_away(world, pos, true);
        }
    }
    if is(&*world, FLOOR) && chance(world, volume) {
        // `make_plating()`: plating keeps itself.
        if !is(&*world, PLATING) {
            crate::construction::scrape_away(world, pos, true);
        }
    } else if chance(world, volume) {
        crate::turf_damage::burn_tile(world, pos);
    }
    if is(&*world, FLOOR) {
        for dx in -1..=1 {
            for dy in -1..=1 {
                let near = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                if turf_path(world, near).is_some_and(|p| p.starts_with("/turf/open")) {
                    hotspot(world, near);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_reagents::{ChemData, ReagentDef};

    fn def(path: &str) -> ReagentDef {
        ReagentDef {
            path: path.into(),
            name: String::new(),
            description: String::new(),
            color: "#000000".into(),
            taste_description: String::new(),
            taste_mult: 1.0,
            metabolization_rate: 0.2,
            overdose_threshold: 0.0,
            ph: 7.0,
            specific_heat: 200.0,
            chemical_flags: 0,
            purity: 1.0,
            creation_purity: 1.0,
            inverse_chem: None,
            inverse_chem_val: 0.25,
            purge_multiplier: 1.0,
            toxpwr: 0.0,
            health_required: 0.0,
            liver_tolerance_multiplier: 1.0,
            boozepwr: 0.0,
            brute_heal: 0.0,
            nutriment_factor: 0.0,
            penetrates_skin: 0,
            default_container: String::new(),
            addiction_types: Vec::new(),
        }
    }

    #[test]
    fn napalm_sticks_by_touch_not_by_mouth() {
        use crate::janitor::{INGEST, TOUCH};
        let mut world = World::new();
        world.init_resource::<Time>();
        let chem = Chemistry::new(ChemData {
            reagents: vec![def(NAPALM)],
            ..Default::default()
        });
        let touched = world.spawn_empty().id();
        // `min(volume / 4, 20)`.
        expose(&mut world, &chem, touched, 0, 40.0, TOUCH, 0.0, None, true);
        assert_eq!(fire_stacks(&world, touched), 10.0);
        let drinker = world.spawn_empty().id();
        expose(&mut world, &chem, drinker, 0, 40.0, INGEST, 0.0, None, true);
        assert_eq!(fire_stacks(&world, drinker), 0.0);
    }

    #[test]
    fn napalm_in_the_blood_stokes_the_fire() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let chem = Chemistry::new(ChemData {
            reagents: vec![def(NAPALM)],
            ..Default::default()
        });
        let mob = world.spawn_empty().id();
        let m = Metabolized {
            id: 0,
            volume: 0.4,
            ratio: 1.0,
            overdosed: false,
            overdose_started: false,
            current_cycle: 1,
            creation_purity: 1.0,
            held: 10.0,
        };
        metabolized(&mut world, &chem, mob, &[m], 2.0);
        // `0.5 * ratio * seconds`.
        assert_eq!(fire_stacks(&world, mob), 1.0);
    }
}
