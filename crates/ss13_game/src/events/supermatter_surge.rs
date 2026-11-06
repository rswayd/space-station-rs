//! Supermatter Surge (`code/modules/events/supermatter_surge.dm`): for 240 to 270 event seconds
//! the main engine's crystal gets more MeV from emitter beams (`bullet_energy` = class + 5) and
//! nitrogen makes it keep its energy (`powerloss_inhibition`), less cold and more heat as the
//! class (1-4: 14/20/35/31%) grows; the end restores tg's values. Needs a chief engineer or
//! more than three engineers and atmospheric technicians on the manifest.
// shortcut: no admin intensity setup; "Poly's Revenge" (Poly thrown into the crystal) is not
// ported: there is no Poly.

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::power::supermatter::{SM_GASES, SUPERMATTER_DEFAULT_BULLET_ENERGY, SmGas, Supermatter};

pub const NAME: &str = "Supermatter Surge";
const ALERT: &str = "sound/machines/engine_alert/engine_alert3.ogg";
/// `SURGE_BULLET_ENERGY_ADDITION`.
const BULLET_ENERGY_ADDITION: f64 = 5.0;
/// `SURGE_BASE_POWERLOSS_INHIBITION`.
const BASE_POWERLOSS_INHIBITION: f64 = 0.55;
/// `SURGE_POWERLOSS_INHIBITION_MODIFIER`.
const POWERLOSS_INHIBITION_MODIFIER: f64 = 0.175;
/// `SURGE_POWER_GENERATION_MODIFIER`.
const POWER_GENERATION_MODIFIER: f64 = 0.075;
/// `SURGE_HEAT_MODIFIER`.
const HEAT_MODIFIER: f64 = 0.25;
/// `JOB_GROUP_ENGINEERS`, `JOB_CHIEF_ENGINEER`.
const ENGINEERS: [&str; 2] = ["Station Engineer", "Atmospheric Technician"];
const CHIEF_ENGINEER: &str = "Chief Engineer";
/// tg's nitrogen behaviour (`initial()`).
const NITROGEN: usize = ss13_atmos::GasId::N2.0 as usize;

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Surge {
        engine: None,
        class: 0,
    })
}

struct Surge {
    engine: Option<Entity>,
    /// `surge_class`, 1-4.
    class: u8,
}

/// `can_spawn_event`: `SSjob.has_minimum_jobs(crew_threshold = 3, JOB_GROUP_ENGINEERS,
/// head_jobs = list(JOB_CHIEF_ENGINEER))` over the manifest's trims.
pub fn has_engineers(world: &mut World) -> bool {
    let Some(manifest) = world.get_resource::<crate::records::manifest::Manifest>() else {
        return false;
    };
    let mut employees = 0;
    for record in &manifest.general {
        if record.trim == CHIEF_ENGINEER {
            return true;
        }
        if ENGINEERS.contains(&record.trim.as_str()) {
            employees += 1;
        }
    }
    employees > 3
}

/// `surge_class` from `rand(1, 100)`.
pub fn class_for(roll: i64) -> u8 {
    match roll {
        ..=14 => 1,
        15..=34 => 2,
        35..=69 => 3,
        _ => 4,
    }
}

/// The surged nitrogen behaviour of `class`.
pub fn surged_nitrogen(class: u8) -> SmGas {
    let class = f64::from(class);
    SmGas {
        powerloss_inhibition: class * POWERLOSS_INHIBITION_MODIFIER + BASE_POWERLOSS_INHIBITION,
        heat_power_generation: class * POWER_GENERATION_MODIFIER - 1.0,
        heat_modifier: class * HEAT_MODIFIER - 1.0,
        ..SM_GASES[NITROGEN]
    }
}

/// `GLOB.main_supermatter_engine`.
fn main_engine(world: &mut World) -> Option<Entity> {
    let mut q = world.query::<(Entity, &Supermatter)>();
    q.iter(world)
        .filter(|(_, sm)| sm.crystal.is_main_engine && !sm.disabled)
        .map(|(e, _)| e)
        .min()
}

/// Every crystal's nitrogen (tg changes the one shared `/datum/sm_gas/nitrogen`).
fn set_nitrogen(world: &mut World, gas: SmGas) {
    let mut q = world.query::<&mut Supermatter>();
    for mut sm in q.iter_mut(world) {
        sm.crystal.nitrogen = gas;
    }
}

impl RoundEvent for Surge {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        ev.announce_when = 4;
        self.engine = main_engine(world);
        if self.engine.is_none() {
            warn!("SM surge event failed to find a supermatter engine");
            ev.killed = true;
            return;
        }
        let rng = &mut world.resource_mut::<EventRng>().0;
        self.class = class_for(rng.range(1, 100));
        // `rand(240, 270) EVENT_SECONDS`: half as many fires.
        ev.end_when = rng.range(120, 135) as u32;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        let class = if fake {
            world.resource_mut::<EventRng>().0.range(1, 4) as u8
        } else {
            self.class
        };
        super::priority_announce_sound(
            world,
            &format!("Всплеск суперматерии класса {class}"),
            "Система мониторинга целостности кристала обнаружила необычные атмосферные параметры \
             в камере суперматерии, выходная мощность кристалла суперматерии значительно \
             увеличилась. Требуется инженерное вмешательство для стабилизации работы двигателя.",
            ALERT,
        );
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let Some(engine) = self.engine else {
            return;
        };
        if let Some(mut sm) = world.get_mut::<Supermatter>(engine) {
            sm.crystal.bullet_energy = f64::from(self.class) + BULLET_ENERGY_ADDITION;
        }
        set_nitrogen(world, surged_nitrogen(self.class));
    }

    fn end(&mut self, world: &mut World, _: &mut EventState) {
        if let Some(engine) = self.engine.take()
            && let Some(mut sm) = world.get_mut::<Supermatter>(engine)
        {
            sm.crystal.bullet_energy = SUPERMATTER_DEFAULT_BULLET_ENERGY;
        }
        set_nitrogen(world, SM_GASES[NITROGEN]);
        super::priority_announce(
            world,
            "Аномалия устранена",
            "Всплеск суперматерии рассеялся, выходные показатели кристалла нормализовались.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_follow_tg_weights() {
        assert_eq!(class_for(1), 1);
        assert_eq!(class_for(14), 1);
        assert_eq!(class_for(15), 2);
        assert_eq!(class_for(34), 2);
        assert_eq!(class_for(35), 3);
        assert_eq!(class_for(69), 3);
        assert_eq!(class_for(70), 4);
        assert_eq!(class_for(100), 4);
    }

    #[test]
    fn class_four_nitrogen() {
        let n = surged_nitrogen(4);
        assert!((n.powerloss_inhibition - 1.25).abs() < 1e-9);
        assert!((n.heat_power_generation - -0.7).abs() < 1e-9);
        assert!((n.heat_modifier - 0.0).abs() < 1e-9);
        assert_eq!(n.power_transmission, SM_GASES[NITROGEN].power_transmission);
    }
}
