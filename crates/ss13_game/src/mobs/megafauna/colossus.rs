//! The colossus, after tg `megafauna/colossus.dm` and its `projectile_attack` abilities
//! (`code/datums/actions/mobs/projectileattack.dm`): death bolts (`/obj/projectile/colossus`,
//! 25 brute through all armor, 5 tiles a second) in patterns, each announced and preceded by
//! a 1.5 s wind-up:
//!
//! - "Judgement." spiral (20% + anger): 80 bolts turning by 22.5 degrees every 0.1 s; at a third
//!   of its health two spirals turning opposite ways, a second later;
//! - "Wrath." (20%): 32 bolts at random angles;
//! - "Retribution." shotgun (60% + anger of the rest): 6 bolts 5 degrees apart at the target;
//! - "Lament.": the diagonals, then the cardinals, twice, a second apart;
//! - "Perish." (once, at a tenth of its health): `colossus_final`, 20 shotgun waves with bolts at
//!   random turfs around, three random bursts and three alternating salvos.
//!
//! Anger is a point per 40 health lost (at most 20). A sleeping carp target enrages it: bolts
//! in all eight directions every 3 s and a run at 0.3 s a tile. A bolt that hits a turf or an
//! object blows it up (a heavy explosion on that spot); every hit on the colossus flashes its
//! anti-toolbox field. Its chest holds an anomalous crystal, colossus vocal cords and Cain and
//! Abel.
// shortcut: no screen flash on the telegraph (`flash_color`), no dusting of dead targets, bolts
// stop on its chest instead of piercing it (`BULLET_ACT_FORCE_PIERCE`).

use bevy::prelude::*;
use ss13_mapload::ProjectileData;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::{
    ABOVE_GAME_PLANE, FLY_LAYER, Megafauna, Step as MegaStep, Visual, roll, say, shake, show,
    sound, spawn_visual,
};
use crate::health::Health;
use crate::projectile::{PASSTABLE, Shot};
use crate::zone::BodyZone;

pub const BOLT: &str = "/obj/projectile/colossus";
pub const CHEST: &str = "/obj/structure/closet/crate/necropolis/colossus";
const VOCAL_CORDS: &str = "/obj/item/organ/vocal_cords/colossus";
const CAIN_AND_ABEL: &str = "/obj/item/cain_and_abel";
/// `subtypesof(/obj/machinery/anomalous_crystal)`.
const CRYSTALS: [&str; 6] = [
    "/obj/machinery/anomalous_crystal/dark_reprise",
    "/obj/machinery/anomalous_crystal/emitter",
    "/obj/machinery/anomalous_crystal/helpers",
    "/obj/machinery/anomalous_crystal/honk",
    "/obj/machinery/anomalous_crystal/possessor",
    "/obj/machinery/anomalous_crystal/theme_warp",
];
const INVOKE: &str = "sound/effects/magic/clockwork/invoke_general.ogg";
const NARSIE: &str = "sound/effects/magic/clockwork/narsie_attack.ogg";
/// The colossus' abilities wait this long before firing (`SLEEP_CHECK_DEATH(1.5 SECONDS)`).
const WIND_UP: f64 = 1.5;
/// `shotgun_blast` `shot_angles`.
pub const SHOTGUN: [f32; 6] = [12.5, 7.5, 2.5, -2.5, -7.5, -12.5];
/// `dir2angle` of `GLOB.diagonals` and `GLOB.cardinals`.
pub const DIAGONALS: [f32; 4] = [45.0, 315.0, 135.0, 225.0];
pub const CARDINALS: [f32; 4] = [0.0, 180.0, 90.0, 270.0];
/// `enrage`: `ranged_cooldown` and `move_to_delay` (seconds a tile).
const ENRAGE_COOLDOWN: f64 = 3.0;
const ENRAGED_MOVE_DELAY: f32 = 0.3;
/// `colossus_final`'s scatter radius.
const FINALE_RANGE: i32 = 12;

#[derive(Debug, Default)]
pub struct State {
    /// `final_available` is gone.
    pub final_used: bool,
}

/// Where a bolt goes (`shoot_projectile`'s `target` and `set_angle`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Aim {
    /// At the target, `spread` degrees off.
    Target(f32),
    /// Degrees clockwise from north.
    Angle(f32),
    Tile(TilePos),
}

/// Bolts fired at once, with the ability's sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Volley {
    Shotgun,
    Diagonals,
    Cardinals,
    /// `GLOB.alldirs` (the enraged attack).
    All,
    /// `random_aoe`: 32 random angles.
    Random,
}

#[derive(Debug, PartialEq)]
pub enum Step {
    /// One spiral bolt (with its quiet sound).
    Spiral {
        target: Entity,
        angle: f32,
    },
    Volley {
        target: Entity,
        volley: Volley,
    },
    /// `colossus_final`: a bolt at each turf within 12 tiles with `percent` chance.
    Scatter {
        percent: f32,
    },
    Telegraph,
    Say(&'static str),
    /// `icon_state`: `eva_attack` while spiralling.
    Attacking(bool),
}

/// `/obj/projectile/colossus` with the `/obj/projectile` defaults.
pub fn death_bolt() -> ProjectileData {
    ProjectileData {
        path: BOLT.to_owned(),
        damage: 25.0,
        damage_type: "brute".to_owned(),
        armor_flag: "bullet".to_owned(),
        armour_penetration: 100.0,
        speed: 0.5,
        range: 50.0,
        pass_flags: PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ..default()
    }
}

/// The spiral's angles (`create_spiral_attack`): from 8, one step of 22.5 degrees per bolt,
/// wrapping within 1..=16.
pub fn spiral_angles(negative: bool) -> Vec<f32> {
    let mut counter = 8;
    (0..80)
        .map(|_| {
            counter += if negative { -1 } else { 1 };
            if counter > 16 {
                counter = 1;
            }
            if counter < 1 {
                counter = 16;
            }
            counter as f32 * 22.5
        })
        .collect()
}

/// The steps of `colossus_final` from its start and its length, seconds: 1.5 s, then 20
/// waves (a telegraphed shotgun while the counter is above 4, the scatter, a pause of the
/// counter + 1 deciseconds), three random bursts and three alternating salvos.
pub fn finale(target: Entity) -> (Vec<(f64, Step)>, f64) {
    let mut steps = vec![(0.0, Step::Say("Perish."))];
    let mut t = WIND_UP;
    let mut counter: u32 = 10;
    for _ in 0..20 {
        if counter > 4 {
            steps.push((t, Step::Telegraph));
            steps.push((
                t,
                Step::Volley {
                    target,
                    volley: Volley::Shotgun,
                },
            ));
        }
        if counter > 1 {
            counter -= 1;
        }
        steps.push((
            t,
            Step::Scatter {
                percent: counter.min(2) as f32,
            },
        ));
        t += f64::from(counter + 1) / 10.0;
    }
    for _ in 0..3 {
        steps.push((t, Step::Telegraph));
        steps.push((
            t,
            Step::Volley {
                target,
                volley: Volley::Random,
            },
        ));
        counter += 6;
        t += f64::from(counter) / 10.0;
    }
    for _ in 0..3 {
        steps.push((t, Step::Telegraph));
        steps.extend(alternating(target, t));
        t += 3.0 + 1.0;
    }
    (steps, t)
}

/// `dir_shots/alternating`: diagonals, cardinals, diagonals, cardinals, a second apart.
fn alternating(target: Entity, start: f64) -> Vec<(f64, Step)> {
    [
        Volley::Diagonals,
        Volley::Cardinals,
        Volley::Diagonals,
        Volley::Cardinals,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, volley)| (start + i as f64, Step::Volley { target, volley }))
    .collect()
}

/// `/mob/living/simple_animal/hostile/megafauna/colossus/OpenFire`.
pub(super) fn open_fire(world: &mut World, mob: Entity, target: Entity, now: f64) {
    let (Some(health), Some(initial_delay)) = (
        world.get::<Health>(mob).cloned(),
        world
            .get::<crate::mobs::BasicMob>(mob)
            .map(|b| b.def.speed.max(0.0) / 10.0),
    ) else {
        return;
    };
    let (hp, max) = (health.health(), health.max);
    let anger = ((max - hp) / 40.0).clamp(0.0, 20.0);
    if enrage(world, target) {
        let delay = world.get::<ss13_world::Mover>(mob).map(|m| m.delay);
        if delay == Some(initial_delay)
            && let Some(&pos) = world.get::<TilePos>(mob)
        {
            show(world, pos, Span::Bold, "\"You can't dodge.\"".to_owned());
        }
        if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
            m.ranged_ready = now + ENRAGE_COOLDOWN;
        }
        telegraph(world, mob);
        volley(world, mob, target, Volley::All);
        set_move_delay(world, mob, ENRAGED_MOVE_DELAY);
        return;
    }
    set_move_delay(world, mob, initial_delay);
    let Some(m) = world.get::<Megafauna>(mob) else {
        return;
    };
    if !m.ability_available(now) {
        return;
    }
    let final_ready = hp <= max / 10.0 && !m.colossus.final_used;
    let (spiral, random, shotgun) = roll(world, |rng| {
        (
            rng.chance(20.0 + anger),
            rng.chance(20.0),
            rng.chance(60.0 + anger),
        )
    });
    let (steps, length, cooldown) = if final_ready {
        if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
            m.colossus.final_used = true;
        }
        let (steps, length) = finale(target);
        (steps, length, 2.5)
    } else if spiral {
        // `start_attack`: telegraph, the attack sprite, the line.
        telegraph(world, mob);
        let enraged = hp <= max / 3.0;
        let negative = roll(world, |rng| rng.chance(50.0));
        let start = WIND_UP + if enraged { 1.0 } else { 0.0 };
        let spirals = if enraged {
            vec![true, false]
        } else {
            vec![negative]
        };
        let mut steps = vec![(0.0, Step::Attacking(true)), (0.0, Step::Say("Judgement."))];
        for negative in spirals {
            for (i, angle) in spiral_angles(negative).into_iter().enumerate() {
                steps.push((start + i as f64 * 0.1, Step::Spiral { target, angle }));
            }
        }
        let length = start + 8.0;
        steps.push((length, Step::Attacking(false)));
        (steps, length, 1.5)
    } else if random {
        let steps = vec![
            (0.0, Step::Say("Wrath.")),
            (
                WIND_UP,
                Step::Volley {
                    target,
                    volley: Volley::Random,
                },
            ),
        ];
        (steps, WIND_UP, 1.5)
    } else if shotgun {
        let steps = vec![
            (0.0, Step::Say("Retribution.")),
            (
                WIND_UP,
                Step::Volley {
                    target,
                    volley: Volley::Shotgun,
                },
            ),
        ];
        (steps, WIND_UP, 0.5)
    } else {
        let mut steps = vec![(0.0, Step::Say("Lament."))];
        steps.extend(alternating(target, WIND_UP));
        (steps, WIND_UP + 3.0, 2.5)
    };
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.cooldown(now + length, cooldown);
        for (offset, step) in steps {
            m.at(now + offset, MegaStep::Colossus(step));
        }
    }
}

/// `enrage`: a human whose active martial art is the sleeping carp.
fn enrage(world: &World, target: Entity) -> bool {
    crate::martial::active_style(world, target) == Some(crate::martial::Style::SleepingCarp)
}

fn set_move_delay(world: &mut World, mob: Entity, delay: f32) {
    if let Some(mut mover) = world.get_mut::<ss13_world::Mover>(mob)
        && mover.delay != delay
    {
        mover.delay = delay;
    }
}

pub(super) fn run(world: &mut World, mob: Entity, _at: f64, step: Step) {
    match step {
        Step::Spiral { target, angle } => {
            shoot(world, mob, target, Aim::Angle(angle));
            if let Some(&pos) = world.get::<TilePos>(mob) {
                sound(world, pos, INVOKE, 20.0, 0.0);
            }
        }
        Step::Volley { target, volley: v } => volley(world, mob, target, v),
        Step::Scatter { percent } => scatter(world, mob, percent),
        Step::Telegraph => telegraph(world, mob),
        Step::Say(text) => say(world, mob, text),
        Step::Attacking(on) => {
            let icon = world
                .get::<crate::mobs::BasicMob>(mob)
                .map(|b| b.def.icon.clone());
            match (on, icon) {
                (true, Some(icon)) => crate::mobs::set_icon_state(world, mob, &icon, "eva_attack"),
                _ => crate::mobs::set_look(world, mob, false),
            }
        }
    }
}

/// `telegraph`: the screens of the viewers within 10 tiles shake, a roar.
fn telegraph(world: &mut World, mob: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    shake(world, pos, 10, 4.0, 3.0);
    sound(world, pos, NARSIE, 200.0, 0.0);
}

/// One salvo with its sound (`fire_shotgun`, `fire_in_directions`, `random_aoe`).
fn volley(world: &mut World, mob: Entity, target: Entity, volley: Volley) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let aims: Vec<Aim> = match volley {
        Volley::Shotgun => SHOTGUN.iter().map(|&s| Aim::Target(s)).collect(),
        Volley::Diagonals => DIAGONALS.iter().map(|&a| Aim::Angle(a)).collect(),
        Volley::Cardinals => CARDINALS.iter().map(|&a| Aim::Angle(a)).collect(),
        Volley::All => CARDINALS
            .iter()
            .chain(DIAGONALS.iter())
            .map(|&a| Aim::Angle(a))
            .collect(),
        Volley::Random => roll(world, |rng| {
            (0..32)
                .map(|_| Aim::Angle(rng.range(0, 360) as f32))
                .collect()
        }),
    };
    let (vol, extra) = match volley {
        Volley::Random => (300.0, 5.0),
        _ => (200.0, 2.0),
    };
    sound(world, pos, INVOKE, vol, extra);
    for aim in aims {
        shoot(world, mob, target, aim);
    }
}

/// `colossus_final`'s bolts at random turfs within 12 tiles.
fn scatter(world: &mut World, mob: Entity, percent: f32) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let tiles: Vec<TilePos> = roll(world, |rng| {
        let mut out = Vec::new();
        for dy in -FINALE_RANGE..=FINALE_RANGE {
            for dx in -FINALE_RANGE..=FINALE_RANGE {
                if (dx, dy) != (0, 0) && rng.chance(percent) {
                    out.push(TilePos::new(pos.x + dx, pos.y + dy, pos.z));
                }
            }
        }
        out
    });
    let index = world.resource::<ss13_world::SpatialIndex>();
    let tiles: Vec<TilePos> = tiles.into_iter().filter(|&t| index.in_bounds(t)).collect();
    for tile in tiles {
        shoot(world, mob, Entity::PLACEHOLDER, Aim::Tile(tile));
    }
}

/// `shoot_projectile`: a death bolt from the colossus' tile.
fn shoot(world: &mut World, mob: Entity, target: Entity, aim: Aim) {
    let Some(&from) = world.get::<TilePos>(mob) else {
        return;
    };
    let (target, target_pos, deviation) = match aim {
        Aim::Target(spread) => {
            let Some(&to) = world.get::<TilePos>(target) else {
                return;
            };
            (Some(target), to, spread)
        }
        Aim::Angle(angle) => (None, from, angle),
        Aim::Tile(to) => (None, to, 0.0),
    };
    let data = death_bolt();
    crate::projectile::fire(
        world,
        Shot {
            data: &data,
            firer: mob,
            from,
            target,
            target_pos,
            zone: BodyZone::Chest,
            // `can_hit_target`: every living thing it meets is a direct target.
            hit_prone: true,
            pixel: [0, 0],
            deviation,
            fired_from: None,
        },
    );
}

/// `/obj/projectile/colossus/on_hit` on what is not living: turfs and objects (not vehicles)
/// take a heavy explosion.
pub(super) fn bolt_hit(world: &mut World, target: Entity) {
    let Ok(e) = world.get_entity(target) else {
        return;
    };
    if e.contains::<Health>() || e.contains::<crate::mecha::Mech>() {
        return;
    }
    let chest = e
        .get::<ss13_mapload::DmObj>()
        .is_some_and(|o| ss13_mapload::fidelity::is_under(&o.path, CHEST));
    if chest {
        return;
    }
    if e.contains::<ss13_mapload::Turf>() {
        if let Some(&pos) = e.get::<TilePos>() {
            crate::explosions::queue_turfs(world, &[pos], crate::explosions::Severity::Heavy);
        }
    } else if e.contains::<ss13_mapload::DmObj>() || e.contains::<ss13_mapload::DmItem>() {
        crate::explosions::queue_movables(world, &[target], crate::explosions::Severity::Heavy);
    }
}

/// `projectile_shield`: an anti-toolbox field flashes somewhere over the colossus.
pub(super) fn shield(world: &mut World, mob: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    if world.get::<crate::health::Stat>(mob) == Some(&crate::health::Stat::Dead) {
        return;
    }
    let (x, y) = {
        let mut rng = world.get_resource_or_init::<crate::animation::AnimationRng>();
        (rng.signed_unit(), rng.signed_unit())
    };
    let offset = [(x * 32.0) as i16, ((y + 1.0) * 36.0) as i16];
    spawn_visual(
        world,
        pos,
        Visual {
            layer: FLY_LAYER,
            plane: ABOVE_GAME_PLANE,
            offset,
            ..Visual::new("icons/effects/effects.dmi", "at_shield2", 0.8)
        },
    );
}

/// `necropolis/colossus/PopulateContents`.
pub(super) fn loot(world: &mut World) -> Vec<&'static str> {
    let i = roll(world, |rng| rng.range(0, CRYSTALS.len() as u32 - 1)) as usize;
    vec![CRYSTALS[i], VOCAL_CORDS, CAIN_AND_ABEL]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiral_turns_by_a_sixteenth_and_wraps() {
        let cw = spiral_angles(false);
        assert_eq!(cw.len(), 80);
        assert_eq!(&cw[..3], &[202.5, 225.0, 247.5]);
        assert_eq!(cw[8], 22.5, "wrapped past 16 to 1");
        let ccw = spiral_angles(true);
        assert_eq!(&ccw[..2], &[157.5, 135.0]);
        assert_eq!(ccw[7], 360.0, "wrapped below 1 to 16");
    }

    #[test]
    fn finale_waves_slow_down_and_end_with_salvos() {
        let target = Entity::PLACEHOLDER;
        let (steps, length) = finale(target);
        let shotguns = steps
            .iter()
            .filter(|(_, s)| {
                matches!(
                    s,
                    Step::Volley {
                        volley: Volley::Shotgun,
                        ..
                    }
                )
            })
            .count();
        // The counter starts at 10: shotguns while it is 10..=5 before the decrement.
        assert_eq!(shotguns, 6);
        let scatters = steps
            .iter()
            .filter(|(_, s)| matches!(s, Step::Scatter { .. }))
            .count();
        assert_eq!(scatters, 20);
        let randoms = steps
            .iter()
            .filter(|(_, s)| {
                matches!(
                    s,
                    Step::Volley {
                        volley: Volley::Random,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(randoms, 3);
        // 1.5 + waves (9 pauses of 10..2 ds then 11 of 2 ds) + bursts (7, 13, 19 ds) + 3 x 4 s.
        let waves: f64 = (1..=9).rev().map(|c| f64::from(c + 1)).sum::<f64>() + 11.0 * 2.0;
        let expected = WIND_UP + waves / 10.0 + (7.0 + 13.0 + 19.0) / 10.0 + 12.0;
        assert!((length - expected).abs() < 1e-9, "{length} vs {expected}");
        assert!(steps.iter().all(|(t, _)| *t <= length));
    }

    #[test]
    fn bolt_matches_tg() {
        let bolt = death_bolt();
        assert_eq!((bolt.damage, bolt.speed), (25.0, 0.5));
        assert_eq!(bolt.armour_penetration, 100.0);
    }
}
