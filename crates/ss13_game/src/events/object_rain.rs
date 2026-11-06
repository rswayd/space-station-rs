//! Indoor Weather (`code/modules/events/wizard/object_rain.dm`): for 20 event ticks (10 for
//! the animals) up to 3 things a tick fall from the ceiling on the free tiles of one random
//! department (`RAIN_TARGET_DEPARTMENTS`), each tile taken once: small animals, food, money or
//! fish. Wizard events: only in wizard mode or forced.
// shortcut: no see-through pod or its 3 s fall animation (the thing appears after the fall
// delay); the food and fish are fixed copies of `get_sane_item_types(/obj/item/food)` (a
// sample of plain foods) and `subtypesof(/obj/item/fish)` (the runtime has no type tree); no
// admin region choice.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;

use super::controller::{EventRng, EventState, RoundEvent, pick_weight};

pub const ANIMALS: &str = "Indoor Weather: Cats and Dogs (and Frogs)";
pub const FOOD: &str = "Indoor Weather: Chance of Meatballs";
pub const CASH: &str = "Indoor Weather: Loose Change";
pub const FISH: &str = "Indoor Weather: Fish";

/// `RAIN_TARGET_DEPARTMENTS`.
pub const REGIONS: [(&str, &str); 7] = [
    ("Cargo Bay", "/area/station/cargo"),
    ("Engineering Division", "/area/station/engineering"),
    ("Medbay", "/area/station/medical"),
    (
        "Recreation and Relaxation Division",
        "/area/station/commons",
    ),
    ("Science Division", "/area/station/science"),
    ("Security Department", "/area/station/security"),
    ("Station Corridors", "/area/station/hallway"),
];

/// `max_things_per_second`.
const MAX_PER_TICK: i64 = 3;
/// `POD_FALLING`.
const FALL_SECS: f64 = 3.0;

const ANIMAL_PATHS: [(&str, i64); 7] = [
    ("/mob/living/basic/pet/dog/corgi/puppy", 5),
    ("/mob/living/basic/pet/cat/kitten", 5),
    ("/mob/living/basic/frog", 5),
    ("/mob/living/basic/frog/rare", 1),
    ("/mob/living/basic/axolotl", 1),
    ("/mob/living/basic/pet/dog/corgi/puppy/void", 1),
    ("/mob/living/basic/pet/dog/corgi/puppy/slime", 1),
];

const CASH_PATHS: [(&str, i64); 19] = [
    ("/obj/item/coin/iron", 100),
    ("/obj/item/coin/plastic", 100),
    ("/obj/item/coin/silver", 60),
    ("/obj/item/coin/plasma", 30),
    ("/obj/item/coin/uranium", 30),
    ("/obj/item/coin/titanium", 30),
    ("/obj/item/coin/diamond", 20),
    ("/obj/item/coin/bananium", 20),
    ("/obj/item/coin/adamantine", 20),
    ("/obj/item/coin/mythril", 20),
    ("/obj/item/coin/runite", 20),
    ("/obj/item/coin/twoheaded", 10),
    ("/obj/item/coin/antagtoken", 10),
    ("/obj/item/stack/spacecash/c10", 80),
    ("/obj/item/stack/spacecash/c20", 50),
    ("/obj/item/stack/spacecash/c50", 30),
    ("/obj/item/stack/spacecash/c100", 10),
    ("/obj/item/stack/spacecash/c500", 5),
    ("/obj/item/stack/spacecash/c1000", 1),
];

const FOOD_PATHS: [&str; 20] = [
    "/obj/item/food/meatball",
    "/obj/item/food/meatball/bear",
    "/obj/item/food/meatball/chicken",
    "/obj/item/food/meatball/corgi",
    "/obj/item/food/meatball/human",
    "/obj/item/food/meatball/xeno",
    "/obj/item/food/spaghetti/meatballspaghetti",
    "/obj/item/food/meatbun",
    "/obj/item/food/baguette",
    "/obj/item/food/burger/plain",
    "/obj/item/food/burrito",
    "/obj/item/food/cheesiehonkers",
    "/obj/item/food/chips",
    "/obj/item/food/cookie",
    "/obj/item/food/donut/plain",
    "/obj/item/food/fries",
    "/obj/item/food/hotdog",
    "/obj/item/food/pie/cream",
    "/obj/item/food/pizza/margherita",
    "/obj/item/food/sandwich",
];

/// `subtypesof(/obj/item/fish)` without the unit test dummies.
const FISH_PATHS: [&str; 71] = [
    "/obj/item/fish/angelfish",
    "/obj/item/fish/arctic_char",
    "/obj/item/fish/armorfish",
    "/obj/item/fish/babbelfish",
    "/obj/item/fish/baby_carp",
    "/obj/item/fish/boned",
    "/obj/item/fish/bumpy",
    "/obj/item/fish/cardinal",
    "/obj/item/fish/catfish",
    "/obj/item/fish/chainsawfish",
    "/obj/item/fish/chasm_crab",
    "/obj/item/fish/chasm_crab/ice",
    "/obj/item/fish/chasm_crab/instant_growth",
    "/obj/item/fish/clownfish",
    "/obj/item/fish/clownfish/lube",
    "/obj/item/fish/dolphish",
    "/obj/item/fish/donkfish",
    "/obj/item/fish/emulsijack",
    "/obj/item/fish/firefish",
    "/obj/item/fish/flumpulus",
    "/obj/item/fish/fryish",
    "/obj/item/fish/fryish/fritterish",
    "/obj/item/fish/fryish/nessie",
    "/obj/item/fish/goldfish",
    "/obj/item/fish/goldfish/gill",
    "/obj/item/fish/goldfish/three_eyes",
    "/obj/item/fish/goldfish/three_eyes/gill",
    "/obj/item/fish/greenchromis",
    "/obj/item/fish/gullion",
    "/obj/item/fish/gunner_jellyfish",
    "/obj/item/fish/guppy",
    "/obj/item/fish/holo",
    "/obj/item/fish/holo/angel",
    "/obj/item/fish/holo/checkered",
    "/obj/item/fish/holo/clown",
    "/obj/item/fish/holo/crab",
    "/obj/item/fish/holo/halffish",
    "/obj/item/fish/holo/puffer",
    "/obj/item/fish/jumpercable",
    "/obj/item/fish/lanternfish",
    "/obj/item/fish/lavaloop",
    "/obj/item/fish/lavaloop/plasma_river",
    "/obj/item/fish/mastodon",
    "/obj/item/fish/monkfish",
    "/obj/item/fish/moonfish",
    "/obj/item/fish/moonfish/dwarf",
    "/obj/item/fish/mossglob",
    "/obj/item/fish/needlefish",
    "/obj/item/fish/perch",
    "/obj/item/fish/pike",
    "/obj/item/fish/pike/armored",
    "/obj/item/fish/plaice",
    "/obj/item/fish/plasmatetra",
    "/obj/item/fish/pufferfish",
    "/obj/item/fish/ratfish",
    "/obj/item/fish/sacabambaspis",
    "/obj/item/fish/sand_crab",
    "/obj/item/fish/sand_surfer",
    "/obj/item/fish/skin_crab",
    "/obj/item/fish/slimefish",
    "/obj/item/fish/sludgefish",
    "/obj/item/fish/sludgefish/purple",
    "/obj/item/fish/sockeye_salmon",
    "/obj/item/fish/soul",
    "/obj/item/fish/squid",
    "/obj/item/fish/starfish",
    "/obj/item/fish/starfish/chrystarfish",
    "/obj/item/fish/stingray",
    "/obj/item/fish/swordfish",
    "/obj/item/fish/tadpole",
    "/obj/item/fish/zipzap",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Animals,
    Food,
    Cash,
    Fish,
}

pub fn make_animals(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ObjectRain::new(Kind::Animals))
}

pub fn make_food(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ObjectRain::new(Kind::Food))
}

pub fn make_cash(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ObjectRain::new(Kind::Cash))
}

pub fn make_fish(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ObjectRain::new(Kind::Fish))
}

struct ObjectRain {
    kind: Kind,
    /// `target_region`: an index into [`REGIONS`].
    region: usize,
    /// `target_turfs`.
    turfs: Vec<TilePos>,
    /// Things on their way down: landing time, tile, type.
    falling: Vec<(f64, TilePos, &'static str)>,
}

impl ObjectRain {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            region: 0,
            turfs: Vec::new(),
            falling: Vec::new(),
        }
    }

    /// `get_item()`.
    fn pick_item(&self, rng: &mut EventRng) -> Option<&'static str> {
        match self.kind {
            Kind::Animals => pick_weight(&ANIMAL_PATHS, &mut rng.0),
            Kind::Cash => pick_weight(&CASH_PATHS, &mut rng.0),
            Kind::Food => rng.pick(&FOOD_PATHS).copied(),
            Kind::Fish => rng.pick(&FISH_PATHS).copied(),
        }
    }

    fn land(&mut self, world: &mut World, all: bool) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        let (due, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.falling)
            .into_iter()
            .partition(|&(at, _, _)| all || at <= now);
        self.falling = kept;
        for (_, pos, path) in due {
            if self.kind == Kind::Animals {
                crate::mobs::spawn::spawn_mob(world, path, pos);
            } else {
                crate::construction::spawn_obj(world, path, pos);
            }
        }
    }
}

/// The region's free tiles: open floor without a dense non-mob thing
/// (`is_blocked_turf(exclude_mobs = TRUE)`).
fn region_turfs(world: &World, root: &str) -> Vec<TilePos> {
    let index = world.resource::<ss13_world::SpatialIndex>();
    super::station_tiles(world)
        .into_iter()
        .filter(|&(area, _)| super::area_type(world, area).is_some_and(|p| is_under(p, root)))
        .map(|(_, pos)| pos)
        .filter(|&pos| super::electrical_storm::is_floor(world, pos))
        .filter(|&pos| {
            index
                .dense_on_tile(pos)
                .iter()
                .all(|&e| world.get::<crate::health::Stat>(e).is_some())
        })
        .collect()
}

impl RoundEvent for ObjectRain {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        // These are mobs so don't make too many.
        ev.end_when = if self.kind == Kind::Animals { 10 } else { 20 };
        self.region = world
            .resource_mut::<EventRng>()
            .0
            .range(0, REGIONS.len() as i64 - 1) as usize;
        self.turfs = region_turfs(world, REGIONS[self.region].1);
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        if fake {
            self.region = world
                .resource_mut::<EventRng>()
                .0
                .range(0, REGIONS.len() as i64 - 1) as usize;
        }
        super::priority_announce(
            world,
            "Уведомление уборщиков",
            &format!(
                "Прогнозируются аномальные погодные явления в {}.",
                REGIONS[self.region].0
            ),
        );
    }

    fn tick(&mut self, world: &mut World, _: &mut EventState) {
        self.land(world, false);
        let now = world.resource::<Time>().elapsed_secs_f64();
        let count = world.resource_mut::<EventRng>().0.range(0, MAX_PER_TICK);
        for _ in 0..count {
            // `pick_n_take(target_turfs)`.
            if self.turfs.is_empty() {
                break;
            }
            let mut rng = world.resource_mut::<EventRng>();
            let Some(path) = self.pick_item(&mut rng) else {
                continue;
            };
            let at = rng.0.range(0, self.turfs.len() as i64 - 1) as usize;
            let delay = rng.unit();
            let pos = self.turfs.swap_remove(at);
            self.falling.push((now + delay + FALL_SECS, pos, path));
        }
    }

    fn end(&mut self, world: &mut World, _: &mut EventState) {
        // The pods already in the air still land.
        self.land(world, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_follow_their_tables() {
        let mut rng = EventRng(ss13_mapload::spawner::Rng::new(5));
        let rain = ObjectRain::new(Kind::Cash);
        for _ in 0..200 {
            let path = rain.pick_item(&mut rng).expect("a cash type");
            assert!(CASH_PATHS.iter().any(|&(p, _)| p == path));
        }
        let rain = ObjectRain::new(Kind::Animals);
        let path = rain.pick_item(&mut rng).expect("an animal");
        assert!(path.starts_with("/mob/living/basic/"));
        assert!(FISH_PATHS.iter().all(|p| !p.contains("testdummy")));
    }
}
