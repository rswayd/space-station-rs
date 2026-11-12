//! Special projectiles' `on_hit` (server), called by `crate::projectile` after the hit:
//!
//! - the thermal pistols' nanite rounds (`projectile/energy/thermal.dm`): the inferno round
//!   blows up a human colder than its cold damage limit − 150 K (light 2, flame 3) and warms it
//!   back to normal; the cryo round knocks a human hotter than its heat limit + 300 K down for
//!   10 s with 20 burn and cools it back to normal (not on `TRAIT_RESISTCOLD`/`RESISTHEAT`);
//! - the temperature gun's beams (`special/temperature.dm`, `/obj/projectile/temp` and `hot`):
//!   the body temperature moves by twice the beam's `temperature`;
//! - explosive rounds (`special/rocket.dm`, `bullets/grenade.dm`): the gyrojet bolt, the PM9
//!   rockets (HE, HEAP, low-yield, PEP: their `do_boom`; 1% of HE hits on a conscious mob gib it)
//!   and the 40 mm grenades (incendiary ones also ignite a carbon and set the area burning);
//! - the meteor (`special/meteor.dm`): a hit mob gets a light-2 explosion;
//! - the ink spit (`special/spit.dm`): blinds for 4.5 s (up to 10) and confuses for 1.5 s (up
//!   to 6).
// shortcut: worn insulation does not soften the temperature beams (no
// `get_insulation_protection`), the ink spit always lands (no `splat` checks, smudge or moodlet,
// no fish-organ bonus), the tear gas grenade drops nothing, the rockets' anti-armour damage to
// mechs and silicons is not dealt. Add with insulation, splats and mech damage.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;

use crate::explosions::{Explosion, explosion};
use crate::health::{DamageType, Health, Stat};
use crate::projectile::Projectile;

const INFERNO: &str = "/obj/projectile/energy/inferno";
const CRYO: &str = "/obj/projectile/energy/cryo";
const GYRO: &str = "/obj/projectile/bullet/gyro";
const ROCKET: &str = "/obj/projectile/bullet/rocket";
const A40MM: &str = "/obj/projectile/bullet/a40mm";
const METEOR: &str = "/obj/projectile/meteor";
const INK_SPIT: &str = "/obj/projectile/ink_spit";

/// `/obj/projectile/temp::temperature` of the temperature gun's beams (exact types: the
/// genetics and watcher beams have their own handling).
pub fn gun_beam_temperature(path: &str) -> Option<f64> {
    match path {
        "/obj/projectile/temp" => Some(-50.0),
        "/obj/projectile/temp/hot" => Some(100.0),
        _ => None,
    }
}

/// What an explosive round's `on_hit` blows up: (devastation, heavy, light, flame, flash).
pub fn boom(path: &str, living: bool, dense_structure: Option<bool>) -> Option<[i32; 5]> {
    if is_under(path, GYRO) {
        return Some([-1, 0, 2, 2, -1]);
    }
    if is_under(path, A40MM) {
        return if is_under(path, "/obj/projectile/bullet/a40mm/tear_gas") {
            None
        } else if is_under(path, "/obj/projectile/bullet/a40mm/incendiary") {
            Some([-1, 0, 1, 4, 1])
        } else {
            Some([-1, 0, 2, 3, 1])
        };
    }
    if !is_under(path, ROCKET) {
        return None;
    }
    if is_under(path, "/obj/projectile/bullet/rocket/heap") {
        return Some([-1, 1, 3, 4, 1]);
    }
    if is_under(path, "/obj/projectile/bullet/rocket/pep") {
        // Silicons, structures, machines, vehicles and walls only.
        return dense_structure.map(|dense| {
            if dense {
                [0, 1, 1, 1, 2]
            } else {
                [0, 0, 1, 1, 2]
            }
        });
    }
    Some(if living {
        [0, 0, 2, 3, 4]
    } else {
        [0, 1, 2, 3, 4]
    })
}

fn living(world: &World, e: Entity) -> bool {
    world.get::<Health>(e).is_some()
}

fn human(world: &World, e: Entity) -> bool {
    living(world, e)
        && world.get::<crate::inventory::Inventory>(e).is_some()
        && world.get::<crate::breath::Respiration>(e).is_some()
}

fn play(world: &mut World, at: TilePos, file: &str, volume: f32) {
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(at, file, volume, true).extrarange(-1.0),
    );
}

/// The `on_hit` of projectile `p` on `target`.
pub fn on_hit(world: &mut World, p: &Projectile, target: Entity) {
    let path = p.data.path.as_str();
    let Some(&at) = world.get::<TilePos>(target) else {
        return;
    };
    if is_under(path, INFERNO) || is_under(path, CRYO) {
        thermal_hit(world, is_under(path, INFERNO), target, at);
        return;
    }
    if let Some(temperature) = gun_beam_temperature(path) {
        if let Some(mut body) = world.get_mut::<crate::breath::Respiration>(target) {
            body.bodytemperature = (body.bodytemperature + 2.0 * temperature).max(0.0);
        }
        return;
    }
    if is_under(path, INK_SPIT) {
        if living(world, target) {
            // `adjust_temp_blindness_up_to(4.5 s, 10 s)`.
            let blind = world
                .get::<crate::vision::Vision>(target)
                .map_or(0.0, |v| v.temp_blind);
            crate::vision::adjust_temp_blindness(world, target, 4.5f32.min(10.0 - blind).max(0.0));
            if let Some(mut e) = crate::status_effects::effects_mut(world, target) {
                e.adjust_confusion_up_to(1.5, 6.0);
            }
        }
        return;
    }
    if is_under(path, METEOR) {
        if living(world, target) {
            explosion(world, at, Explosion::new(-1, 0, 2).flame(0).flash(1));
            play(world, at, "sound/effects/meteorimpact.ogg", 40.0);
        }
        return;
    }
    let is_living = living(world, target);
    let structure = (!is_living
        || world
            .get::<crate::silicon::cyborg::Cyborg>(target)
            .is_some())
    .then(|| structure_density(world, target, is_living))
    .flatten();
    let Some([dev, heavy, light, flame, flash]) = boom(path, is_living, structure) else {
        return;
    };
    // `rocket/on_hit`: 1% of the HE family's hits gib a conscious mob ("random crits").
    let crit = is_under(path, ROCKET)
        && !is_under(path, "/obj/projectile/bullet/rocket/heap")
        && !is_under(path, "/obj/projectile/bullet/rocket/weak")
        && !is_under(path, "/obj/projectile/bullet/rocket/pep")
        && is_living
        && world.get::<Stat>(target) == Some(&Stat::Conscious)
        && world.resource_mut::<crate::combat::CombatRng>().chance(1.0);
    let mut ex = Explosion::new(dev, heavy, light).flame(flame).flash(flash);
    ex.cause = path.to_owned();
    explosion(world, at, ex);
    if crit && world.get_entity(target).is_ok() {
        crate::health::gib(world, target);
    }
    // `a40mm/incendiary/grenade_extra_effect`: a carbon takes `damage * 2` fire stacks and
    // ignites; the turfs around catch fire.
    if is_under(path, "/obj/projectile/bullet/a40mm/incendiary") {
        if human(world, target) {
            crate::fire::adjust_fire_stacks(world, target, p.data.damage * 2.0);
            crate::fire::ignite_mob(world, target);
        }
        if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
            for dx in -4..=4 {
                for dy in -4..=4 {
                    let pos = TilePos::new(at.x + dx, at.y + dy, at.z);
                    atmos.hotspot_expose(pos, 1000.0, 2500.0, false);
                }
            }
        }
    }
}

/// The PEP rocket's targets: `Some(density)` for a silicon, a structure, a machine, a vehicle or
/// a closed turf.
fn structure_density(world: &World, target: Entity, is_living: bool) -> Option<bool> {
    if is_living {
        // A silicon: `explosion(light 1, flash 2)` (not dense-structure strength).
        return Some(false);
    }
    if world.get::<ss13_mapload::Turf>(target).is_some() {
        return world
            .get::<ss13_world::Density>(target)
            .is_some()
            .then_some(true);
    }
    let path = world.get::<ss13_mapload::DmObj>(target)?.path.as_str();
    ["/obj/structure", "/obj/machinery", "/obj/vehicle"]
        .iter()
        .any(|r| is_under(path, r))
        .then(|| world.get::<ss13_world::Density>(target).is_some())
}

/// The inferno or cryo round on a human.
fn thermal_hit(world: &mut World, inferno: bool, target: Entity, at: TilePos) {
    if !human(world, target) {
        return;
    }
    let resist = if inferno {
        crate::species::Trait::ResistCold
    } else {
        crate::species::Trait::ResistHeat
    };
    if crate::species::has_trait(world, target, resist) {
        return;
    }
    let Some(body) = world.get::<crate::breath::Respiration>(target) else {
        return;
    };
    let (temp, life) = (body.bodytemperature, body.body);
    if inferno {
        if temp >= life.cold_limit - 150.0 {
            return;
        }
        explosion(world, at, Explosion::new(-1, -1, 2).flame(3));
        if let Some(mut body) = world.get_mut::<crate::breath::Respiration>(target) {
            body.bodytemperature = life.normal;
        }
        play(world, at, "sound/items/weapons/sear.ogg", 30.0);
    } else {
        if temp <= life.heat_limit + 300.0 {
            return;
        }
        crate::status_effects::apply(world, target, crate::status_effects::Incap::Knockdown, 10.0);
        crate::martial::hurt(world, target, None, DamageType::Burn, 20.0, None);
        if let Some(mut body) = world.get_mut::<crate::breath::Respiration>(target) {
            body.bodytemperature = life.normal;
        }
        play(world, at, "sound/items/weapons/sonic_jackhammer.ogg", 30.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explosive_rounds_follow_tg_do_boom() {
        assert_eq!(boom(GYRO, true, None), Some([-1, 0, 2, 2, -1]));
        assert_eq!(boom(ROCKET, false, None), Some([0, 1, 2, 3, 4]));
        assert_eq!(boom(ROCKET, true, None), Some([0, 0, 2, 3, 4]));
        assert_eq!(
            boom("/obj/projectile/bullet/rocket/heap", true, None),
            Some([-1, 1, 3, 4, 1])
        );
        assert_eq!(boom("/obj/projectile/bullet/rocket/pep", true, None), None);
        assert_eq!(
            boom("/obj/projectile/bullet/rocket/pep", false, Some(true)),
            Some([0, 1, 1, 1, 2])
        );
        assert_eq!(
            boom("/obj/projectile/bullet/a40mm/tear_gas", true, None),
            None
        );
        assert_eq!(boom("/obj/projectile/bullet/c10mm", true, None), None);
        assert_eq!(
            gun_beam_temperature("/obj/projectile/temp/hot"),
            Some(100.0)
        );
        assert_eq!(gun_beam_temperature("/obj/projectile/temp/cryo"), None);
    }
}
