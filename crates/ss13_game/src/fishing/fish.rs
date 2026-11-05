//! Fish items (server), after tg `/obj/item/fish` (`code/modules/fishing/fish/_fish.dm`): size
//! and weight with the weight class they give, hunger, health in and out of water, death.
// shortcut: no growth, breeding, petting, flopping, electrogenesis, fillets, grinding, eating
// or bites; fish out of water always suffocate (no `safe_air_limits` check for air breathers or
// amphibious fish: those are always safe), and water turfs do not keep them alive.

use bevy::prelude::*;
use ss13_mapload::fishing::DmFish;
use ss13_mapload::{DmAtom, DmWeight, IconIndex};
use ss13_shared::chat::{ChatChannel, Span};

use super::aquarium::{Aquarium, AquariumMode, fluid_compatible};
use crate::combat::CombatRng;
use crate::storage::StoredIn;

/// `MAX_FISH_DEVIATION_COEFF`.
const MAX_DEVIATION: f32 = 2.5;
/// `FISH_STARVING_THRESHOLD`.
const STARVING_THRESHOLD: f64 = 1.0;
/// `NEW_FISH_LAST_FEEDING_MULT`: new fish start a third hungry.
const NEW_FISH_HUNGER: f64 = 0.33;
/// Health above `integrity_failure` (`max_integrity` 200, failure at half): the fish dies at 0.
pub const MAX_HEALTH: f32 = 100.0;
/// How often fish are processed, seconds (rates are per second).
const PROCESS_SECS: f64 = 1.0;

/// A live fish item.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Fish {
    /// Centimetres, "kiloclams".
    pub size: f32,
    pub weight: f32,
    pub alive: bool,
    pub health: f32,
    /// `last_feeding`, as `Time` seconds.
    pub last_fed: f64,
}

pub(super) fn plugin(app: &mut App) {
    app.add_observer(add_fish).add_systems(Update, process_fish);
}

/// `Initialize`: the average size and weight, a third hungry.
fn add_fish(add: On<Add<DmFish>>, mut commands: Commands) {
    let e = add.entity;
    commands.queue(move |world: &mut World| {
        let Some(def) = world.get::<DmFish>(e).cloned() else {
            return;
        };
        if world.get::<Fish>(e).is_some() {
            return;
        }
        let now = world.resource::<Time>().elapsed_secs_f64();
        world.entity_mut(e).insert(Fish {
            size: def.average_size,
            weight: def.average_weight,
            alive: true,
            health: MAX_HEALTH,
            last_fed: now - f64::from(def.feeding_frequency) * NEW_FISH_HUNGER,
        });
        update_weight_class(world, e);
    });
}

/// `update_size_and_weight`'s `w_class` by size.
pub fn weight_class(size: f32) -> u8 {
    match size {
        s if s <= 30.0 => 1,
        s if s <= 50.0 => 2,
        s if s <= 80.0 => 3,
        s if s <= 120.0 => 4,
        s if s <= 165.0 => 5,
        _ => 6,
    }
}

fn update_weight_class(world: &mut World, e: Entity) {
    let Some(size) = world.get::<Fish>(e).map(|f| f.size) else {
        return;
    };
    let w_class = weight_class(size);
    if let Some(mut w) = world.get_mut::<DmWeight>(e)
        && w.w_class != w_class
    {
        w.w_class = w_class;
    }
}

/// `gaussian(mean, deviation)` from two uniforms (Box-Muller).
fn gaussian(rng: &mut CombatRng, mean: f32, deviation: f32) -> f32 {
    let u1 = rng.unit().max(f32::MIN_POSITIVE);
    let u2 = rng.unit();
    mean + deviation * (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

/// `randomize_size_and_weight`: a caught fish, 20% around the average, within 2.5x of it.
pub fn randomize_size_and_weight(world: &mut World, e: Entity) {
    let Some(def) = world.get::<DmFish>(e).cloned() else {
        return;
    };
    let (size, weight) = {
        let mut res = world.resource_mut::<CombatRng>();
        let rng: &mut CombatRng = &mut res;
        let roll = |rng: &mut CombatRng, avg: f32| {
            gaussian(rng, avg, 0.2 * avg)
                .clamp(avg / MAX_DEVIATION, avg * MAX_DEVIATION)
                .round()
        };
        let size = roll(&mut *rng, def.average_size);
        (size, roll(&mut *rng, def.average_weight))
    };
    if let Some(mut fish) = world.get_mut::<Fish>(e) {
        fish.size = size;
        fish.weight = weight;
    } else {
        let now = world.resource::<Time>().elapsed_secs_f64();
        world.entity_mut(e).insert(Fish {
            size,
            weight,
            alive: true,
            health: MAX_HEALTH,
            last_fed: now - f64::from(def.feeding_frequency) * NEW_FISH_HUNGER,
        });
    }
    update_weight_class(world, e);
}

/// `get_hunger` (0 fed .. `cap`).
pub fn hunger(fish: &Fish, def: &DmFish, now: f64, cap: f64) -> f64 {
    ((now - fish.last_fed) / f64::from(def.feeding_frequency.max(1.0))).clamp(0.0, cap)
}

/// `proper_environment`: in an aquarium of the right fluid and temperature; out of one, only
/// air breathers and amphibious fish.
pub fn proper_environment(def: &DmFish, aquarium: Option<&Aquarium>) -> bool {
    let amphibious = def
        .fish_traits
        .iter()
        .any(|t| t == "/datum/fish_trait/amphibious");
    match aquarium {
        Some(a) => {
            let fluid = fluid_compatible(&def.required_fluid_type, &a.fluid)
                || (a.fluid == "Air" && amphibious);
            fluid
                && (def.required_temperature_min..=def.required_temperature_max)
                    .contains(&a.temperature)
        }
        None => def.required_fluid_type == "Air" || amphibious,
    }
}

/// `process_health`'s change per second.
pub fn health_change(proper: bool, hunger: f64) -> f32 {
    let mut change = 0.0;
    if !proper {
        change -= 2.5;
    }
    if hunger >= STARVING_THRESHOLD {
        change -= 0.25 * hunger as f32;
    } else {
        change += 0.5;
    }
    change
}

/// `get_happiness_value` without petting or bites.
pub fn happiness(fish: &Fish, def: &DmFish, aquarium: Option<&Aquarium>, now: f64) -> i32 {
    let mut value = 0;
    if hunger(fish, def, now, STARVING_THRESHOLD) < 0.5 {
        value += 1;
    }
    if proper_environment(def, aquarium) {
        value += 2;
    }
    if fish.health / MAX_HEALTH < 0.6 {
        value -= 1;
    }
    value.clamp(-1, 4)
}

/// `process`: hunger and health of every live fish, once a second.
fn process_fish(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    let dt = if *next == 0.0 {
        PROCESS_SECS
    } else {
        now - *next + PROCESS_SECS
    };
    *next = now + PROCESS_SECS;
    let fish: Vec<(Entity, DmFish, Option<Entity>)> = world
        .query::<(Entity, &Fish, &DmFish, Option<&StoredIn>)>()
        .iter(world)
        .filter(|(_, f, ..)| f.alive)
        .map(|(e, _, d, s)| (e, d.clone(), s.map(|s| s.0)))
        .collect();
    let mut died = Vec::new();
    for (e, def, container) in fish {
        let aquarium = container.and_then(|c| world.get::<Aquarium>(c)).cloned();
        let Some(mut f) = world.get_mut::<Fish>(e) else {
            continue;
        };
        // Stasis (`TRAIT_STOP_FISH_REPRODUCTION_AND_GROWTH`): hunger waits, nothing hurts.
        if aquarium
            .as_ref()
            .is_some_and(|a| a.mode == AquariumMode::Stasis)
        {
            f.last_fed += dt;
            continue;
        }
        // Inside something that is not an aquarium (a bag, a rod): as out of water.
        let proper = proper_environment(&def, aquarium.as_ref());
        let change = health_change(proper, hunger(&f, &def, now, STARVING_THRESHOLD * 2.0));
        f.health = (f.health + change * dt as f32).min(MAX_HEALTH);
        if f.health <= 0.0 {
            f.health = 0.0;
            f.alive = false;
            died.push((e, def));
        }
    }
    for (e, def) in died {
        on_death(world, e, &def);
    }
}

/// `set_status(FISH_DEAD)`: the dead sprite and the death line.
fn on_death(world: &mut World, e: Entity, def: &DmFish) {
    let dead_state = (!def.icon_state_dead.is_empty())
        .then(|| world.get::<DmAtom>(e).map(|a| a.icon.clone()))
        .flatten()
        .and_then(|icon| {
            world
                .get_resource::<IconIndex>()?
                .get(&icon, &def.icon_state_dead)
        });
    if let Some(id) = dead_state
        && let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(e)
    {
        look.icon_state = id;
    }
    let name = crate::chem::name_of(world, e);
    let text = def.death_text.replace("%SRC", &name);
    if let Some(at) = crate::chem::map_pos(world, e) {
        let lines =
            crate::pull::visible_message(world, at, ChatChannel::Local, (Span::Warning, text), &[]);
        for line in lines {
            world.write_message(line);
        }
    }
}

/// `examine`: size, weight and health.
// shortcut: everyone sees them (tg: the fishing skill's `TRAIT_EXAMINE_FISH` and
// `TRAIT_EXAMINE_DEEPER_FISH`).
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(fish) = world.get::<Fish>(target) else {
        return;
    };
    lines.push((Span::Notice, format!("It's {} cm long.", fish.size)));
    lines.push((
        Span::Notice,
        format!("It weighs {} kiloclams.", fish.weight),
    ));
    if !fish.alive {
        lines.push((Span::Warning, "It's dead.".to_owned()));
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(def) = world.get::<DmFish>(target) else {
        return;
    };
    let aquarium = world
        .get::<StoredIn>(target)
        .and_then(|s| world.get::<Aquarium>(s.0));
    let mut warnings = Vec::new();
    if hunger(fish, def, now, STARVING_THRESHOLD * 2.0) >= STARVING_THRESHOLD {
        warnings.push("starving");
    }
    if !proper_environment(def, aquarium) {
        warnings.push("drowning");
    }
    match fish.health / MAX_HEALTH {
        r if r <= 0.25 => warnings.push("dying"),
        r if r <= 0.5 => warnings.push("very unhealthy"),
        r if r <= 0.75 => warnings.push("unhealthy"),
        r if r <= 0.9 => warnings.push("a bit unhealthy"),
        _ => {}
    }
    if !warnings.is_empty() {
        lines.push((Span::Warning, format!("It's {}.", warnings.join(", "))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_sets_the_weight_class() {
        assert_eq!(weight_class(20.0), 1);
        assert_eq!(weight_class(30.0), 1);
        assert_eq!(weight_class(31.0), 2);
        assert_eq!(weight_class(100.0), 4);
        assert_eq!(weight_class(140.0), 5);
        assert_eq!(weight_class(300.0), 6);
    }

    #[test]
    fn fish_out_of_water_die_and_fed_ones_heal() {
        let goldfish = DmFish {
            required_fluid_type: "Freshwater".into(),
            required_temperature_min: 291.15,
            required_temperature_max: 299.15,
            feeding_frequency: 300.0,
            ..default()
        };
        assert!(!proper_environment(&goldfish, None));
        // Out of water and fed: 100 health in 50 s.
        assert_eq!(health_change(false, 0.3), -2.0);
        assert_eq!(health_change(true, 0.3), 0.5);
        assert_eq!(health_change(true, 2.0), -0.5);
        let tank = Aquarium {
            fluid: "Freshwater".into(),
            temperature: 297.15,
            ..Aquarium::default()
        };
        assert!(proper_environment(&goldfish, Some(&tank)));
        let salty = Aquarium {
            fluid: "Saltwater".into(),
            ..tank.clone()
        };
        assert!(!proper_environment(&goldfish, Some(&salty)));
        let fish = Fish {
            size: 20.0,
            weight: 200.0,
            alive: true,
            health: MAX_HEALTH,
            last_fed: 0.0,
        };
        assert_eq!(hunger(&fish, &goldfish, 150.0, 1.0), 0.5);
        assert_eq!(happiness(&fish, &goldfish, Some(&tank), 0.0), 3);
    }
}
