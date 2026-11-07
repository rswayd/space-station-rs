//! Raptors (server), after tg `/mob/living/basic/raptor` (`code/modules/mob/living/basic/
//! lavaland/raptor/`): ranch mounts in seven breeds (`raptor_color`), each with its own
//! health, bite and speed, in three growth stages. Their tree is in
//! `crate::mobs::ai::trees::lavaland`.
//!
//! - The breed comes from the mapped subtype (`raptor/red`, `raptor/young/blue`...), else it is
//!   picked by `spawn_chance` (33 each, black 1).
//! - `setup_adult`/`setup_young`/`setup_baby`: the breed's health (half for a youngling, an
//!   eighth for a baby, the damage taken scaled along), bite (floored halves and thirds) and
//!   speed (babies 4.5 slower); the name and sprite `<stage>_<color>` (`setup_appearance`).
//! - A baby grows by itself (`Life`: 80% a second, 0.8-1.2 progress, 100 to grow) into a
//!   youngling.
//! - Raptors fight back (`ai_retaliate`).
// shortcut: no inheritance (stat modifiers, personality traits, food stat mods), breeding and
// eggs, growth from food, eating and troughs, happiness, taming, riding (and the breeds'
// riding perks: the redirected shots, the purple's carrying, the white's healing, the green's
// and black's mining, the blue's lava immunity), the raptor dex, fleeing when hurt, the
// mother's care and the babies' following, pet commands, crusher trophy; the aggressive
// breeds' no-faction-check retaliation is the plain one.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_world::Mover;

use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat};
use crate::mobs::BasicMob;
use crate::mobs::ai::{AiController, Retaliates};

pub const RAPTOR: &str = "/mob/living/basic/raptor";
const BIG_DMI: &str = "icons/mob/simple/lavaland/raptor_big.dmi";
const BABY_DMI: &str = "icons/mob/simple/lavaland/raptor_baby.dmi";
const CONTROLLER: &str = "/datum/ai_controller/basic_controller/raptor";
const AGGRESSIVE_CONTROLLER: &str = "/datum/ai_controller/basic_controller/raptor/aggressive";
const BABY_CONTROLLER: &str = "/datum/ai_controller/basic_controller/baby_raptor";
/// `RAPTOR_GROWTH_REQUIRED`, `RAPTOR_BABY_GROWTH_LOWER`/`UPPER`, `growth_probability`.
const GROWTH_REQUIRED: f32 = 100.0;
const BABY_GROWTH: (u32, u32) = (80, 120);
const GROWTH_PROBABILITY: f64 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Breed {
    Red,
    Purple,
    Green,
    White,
    Yellow,
    Blue,
    Black,
}

/// A `/datum/raptor_color`: `health`, `speed`, `melee_damage_lower`/`upper`, `spawn_chance`.
pub struct BreedStats {
    pub color: &'static str,
    pub health: f32,
    pub speed: f32,
    pub damage: (f32, f32),
    pub spawn_chance: u32,
}

impl Breed {
    pub const ALL: [Self; 7] = [
        Self::Red,
        Self::Purple,
        Self::Green,
        Self::White,
        Self::Yellow,
        Self::Blue,
        Self::Black,
    ];

    pub fn stats(self) -> BreedStats {
        let base = BreedStats {
            color: "",
            health: 220.0,
            speed: 0.5,
            damage: (10.0, 15.0),
            spawn_chance: 33,
        };
        match self {
            Self::Red => BreedStats {
                color: "red",
                health: 300.0,
                damage: (15.0, 20.0),
                ..base
            },
            Self::Purple => BreedStats {
                color: "purple",
                health: 140.0,
                ..base
            },
            Self::Green => BreedStats {
                color: "green",
                health: 400.0,
                ..base
            },
            Self::White => BreedStats {
                color: "white",
                ..base
            },
            Self::Yellow => BreedStats {
                color: "yellow",
                speed: 0.0,
                ..base
            },
            Self::Blue => BreedStats {
                color: "blue",
                health: 300.0,
                ..base
            },
            Self::Black => BreedStats {
                color: "black",
                health: 400.0,
                speed: 0.0,
                damage: (20.0, 25.0),
                spawn_chance: 1,
            },
        }
    }

    fn of_path(path: &str) -> Option<Self> {
        let last = path.rsplit('/').next()?;
        Self::ALL.into_iter().find(|b| b.stats().color == last)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Baby,
    Young,
    Adult,
}

/// A raptor's breed, growth stage and `growth_progress`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Raptor {
    pub breed: Breed,
    pub stage: Stage,
    pub growth: f32,
}

/// What a breed gives a stage: max health, bite, speed (deciseconds per tile).
pub fn stage_stats(breed: Breed, stage: Stage) -> (f32, (f32, f32), f32) {
    let s = breed.stats();
    match stage {
        Stage::Adult => (s.health, s.damage, s.speed),
        Stage::Young => (
            s.health / 2.0,
            ((s.damage.0 / 2.0).floor(), (s.damage.1 / 2.0).floor()),
            s.speed,
        ),
        Stage::Baby => (
            s.health / 8.0,
            ((s.damage.0 / 3.0).floor(), (s.damage.1 / 3.0).floor()),
            s.speed + 4.5,
        ),
    }
}

pub(super) fn build(app: &mut App) {
    app.add_systems(Update, grow_babies.before(crate::health::HealthSystems));
}

pub(super) fn attach(world: &mut World, mob: Entity, path: &str) {
    if !is_under(path, RAPTOR) {
        return;
    }
    let breed = Breed::of_path(path).unwrap_or_else(|| {
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let total: u32 = Breed::ALL.iter().map(|b| b.stats().spawn_chance).sum();
        let mut roll = rng.range(1, total);
        world.resource_mut::<CombatRng>().0 = rng.0;
        Breed::ALL
            .into_iter()
            .find(|b| {
                let w = b.stats().spawn_chance;
                if roll <= w {
                    return true;
                }
                roll -= w;
                false
            })
            .unwrap_or(Breed::Red)
    });
    let stage = if is_under(path, &format!("{RAPTOR}/baby")) {
        Stage::Baby
    } else if is_under(path, &format!("{RAPTOR}/young")) {
        Stage::Young
    } else {
        Stage::Adult
    };
    world.entity_mut(mob).insert(Raptor {
        breed,
        stage,
        growth: 0.0,
    });
    setup(world, mob);
}

/// `change_growth_stage`'s visuals and the breed's `setup_<stage>` + `setup_appearance`.
fn setup(world: &mut World, mob: Entity) {
    let Some(raptor) = world.get::<Raptor>(mob).copied() else {
        return;
    };
    let (max, damage, speed) = stage_stats(raptor.breed, raptor.stage);
    let (base, icon, noun) = match raptor.stage {
        Stage::Baby => ("baby", BABY_DMI, "детёныш раптора"),
        Stage::Young => ("young", BIG_DMI, "молодой раптор"),
        Stage::Adult => ("raptor", BIG_DMI, "раптор"),
    };
    let color = raptor.breed.stats().color;
    if let Some(mut basic) = world.get_mut::<BasicMob>(mob) {
        let def = std::sync::Arc::make_mut(&mut basic.def);
        def.melee_damage_lower = damage.0;
        def.melee_damage_upper = damage.1;
        def.speed = speed;
        def.max_health = max;
        def.icon = icon.to_owned();
        basic.icon_living = format!("{base}_{color}");
        basic.icon_dead = format!("{base}_{color}_dead");
    }
    // `setup_raptor`: the breed's controller (the base type has none); a baby has its own.
    let controller = match (raptor.stage, raptor.breed) {
        (Stage::Baby, _) => BABY_CONTROLLER,
        (_, Breed::Red | Breed::Black) => AGGRESSIVE_CONTROLLER,
        _ => CONTROLLER,
    };
    let swap = world
        .get::<BasicMob>(mob)
        .is_some_and(|b| b.def.ai_controller != controller);
    if swap {
        let def = world.get_mut::<BasicMob>(mob).map(|mut basic| {
            std::sync::Arc::make_mut(&mut basic.def).ai_controller = controller.to_owned();
            basic.def.clone()
        });
        if let Some(controller) = def.and_then(|d| AiController::for_mob(&d)) {
            world.entity_mut(mob).insert((controller, Retaliates));
        }
    }
    // `health *= real_health / maxHealth`: the same fraction of the new maximum.
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        let fraction = (health.health() / health.max).clamp(0.0, 1.0);
        let damage_now = health.max - health.health();
        let wanted = max * (1.0 - fraction);
        health.max = max;
        health.adjust(DamageType::Brute, wanted - damage_now);
    }
    world.entity_mut(mob).insert((
        Mover::new(speed.max(0.0) / 10.0),
        Name::new(format!("{color} {noun}")),
    ));
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    crate::mobs::set_look(world, mob, dead);
}

/// One `Life` second of a baby: whether it gains progress, and how much.
pub fn baby_growth(rng: &mut CombatRng, seconds: f64) -> f32 {
    let p = 1.0 - (1.0 - GROWTH_PROBABILITY).powf(seconds);
    if f64::from(rng.unit()) >= p {
        return 0.0;
    }
    rng.range(BABY_GROWTH.0, BABY_GROWTH.1) as f32 / 100.0
}

/// `raptor/Life`: babies grow into younglings (`change_growth_stage(RAPTOR_YOUNG)`).
fn grow_babies(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs_f64();
    if dt <= 0.0 {
        return;
    }
    let babies: Vec<Entity> = world
        .query::<(Entity, &Raptor, &Stat)>()
        .iter(world)
        .filter(|(_, r, s)| r.stage == Stage::Baby && **s != Stat::Dead)
        .map(|(e, _, _)| e)
        .collect();
    for baby in babies {
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let gained = baby_growth(&mut rng, dt);
        world.resource_mut::<CombatRng>().0 = rng.0;
        let Some(mut raptor) = world.get_mut::<Raptor>(baby) else {
            continue;
        };
        raptor.growth += gained;
        if raptor.growth >= GROWTH_REQUIRED {
            raptor.growth = 0.0;
            raptor.stage = Stage::Young;
            setup(world, baby);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breeds_scale_their_stats_by_growth_stage() {
        assert_eq!(
            Breed::of_path("/mob/living/basic/raptor/young/blue"),
            Some(Breed::Blue)
        );
        assert_eq!(Breed::of_path("/mob/living/basic/raptor"), None);
        // A red adult bites 15-20 with 300 health; a youngling 7-10 with 150; a baby 5-6 with
        // 37.5 and 4.5 slower.
        assert_eq!(
            stage_stats(Breed::Red, Stage::Adult),
            (300.0, (15.0, 20.0), 0.5)
        );
        assert_eq!(
            stage_stats(Breed::Red, Stage::Young),
            (150.0, (7.0, 10.0), 0.5)
        );
        assert_eq!(
            stage_stats(Breed::Red, Stage::Baby),
            (37.5, (5.0, 6.0), 5.0)
        );
        // The yellow is the fast one, the black the rare one.
        assert_eq!(Breed::Yellow.stats().speed, 0.0);
        let total: u32 = Breed::ALL.iter().map(|b| b.stats().spawn_chance).sum();
        assert_eq!((Breed::Black.stats().spawn_chance, total), (1, 199));
    }

    #[test]
    fn a_baby_grows_up_in_about_two_minutes() {
        let mut rng = CombatRng(7);
        let mut progress = 0.0;
        let mut seconds = 0;
        while progress < GROWTH_REQUIRED {
            progress += baby_growth(&mut rng, 1.0);
            seconds += 1;
        }
        // 100 / (0.8 * 1.0) = 125 s on average.
        assert!((100..=170).contains(&seconds), "{seconds}");
    }
}
