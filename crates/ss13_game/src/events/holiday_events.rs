//! Holiday events (`code/modules/events/holiday/`): they spawn only on their holiday
//! ([`super::EventControl::holiday`]); a negative weight triggers them at the first random
//! pick of the round.
//!
//! - 2 SPOOKY! (`halloween.dm`): a trick-o-treat bag of six treats in every human's backpack.
//! - Easter Eggselence (`easter.dm`): the Easter announcement.
//! - Release the Rabbits! (`easter.dm`): Easter rabbits on 15% of the maintenance landmarks and
//!   space rabbits on 15% of the carp spawns.
// shortcut: Halloween's Ian in a bedsheet and ghost Poly, Easter's surprise eggs in maintenance
// loot and the 35% rabbits on `/obj/effect/landmark/event_spawn` (those landmarks are not
// tracked) are not ported; Visit by Santa and Valentines! need ghost polls and their antagonists.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_net::server::Networked;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::inventory::Inventory;

pub const SPOOKY: &str = "2 SPOOKY! (Halloween)";
pub const EASTER: &str = "Easter Eggselence";
pub const RABBITS: &str = "Release the Rabbits!";

const BACKPACK: &str = "/obj/item/storage/backpack";
const TREAT_BAG: &str = "/obj/item/storage/spooky";
/// `/obj/item/storage/spooky/Initialize`'s picks.
const TREATS: [&str; 7] = [
    "/obj/item/food/cookie/sugar/spookyskull",
    "/obj/item/food/cookie/sugar/spookycoffin",
    "/obj/item/food/candy_corn",
    "/obj/item/food/candy",
    "/obj/item/food/candiedapple",
    "/obj/item/food/chocolatebar",
    "/obj/item/organ/brain",
];
const RABBIT: &str = "/mob/living/basic/rabbit/easter";
const SPACE_RABBIT: &str = "/mob/living/basic/rabbit/easter/space";

pub fn make_spooky(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Spooky)
}

pub fn make_easter(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Easter)
}

pub fn make_rabbits(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Rabbits)
}

struct Spooky;

impl RoundEvent for Spooky {
    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        const LINES: [&str; 5] = [
            "RATTLE ME BONES!",
            "THE RIDE NEVER ENDS!",
            "A SKELETON POPS OUT!",
            "SPOOKY SCARY SKELETONS!",
            "CREWMEMBERS BEWARE, YOU'RE IN FOR A SCARE!",
        ];
        let text = world.resource_mut::<EventRng>().pick(&LINES).copied();
        if let Some(text) = text {
            super::priority_announce(world, "THE CALL IS COMING FROM INSIDE THE HOUSE", text);
        }
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        // `GLOB.human_list`: the dead too.
        let mut packs: Vec<Entity> = world
            .query_filtered::<&Inventory, With<crate::human::HumanLook>>()
            .iter(world)
            .filter_map(|inv| inv.back)
            .collect();
        packs.sort();
        packs.retain(|&p| {
            world
                .get::<DmItem>(p)
                .is_some_and(|i| is_under(&i.path, BACKPACK))
        });
        for pack in packs {
            give_treat_bag(world, pack);
        }
    }
}

/// `new /obj/item/storage/spooky(backpack)`: the bag and its six treats.
pub fn give_treat_bag(world: &mut World, pack: Entity) -> Option<Entity> {
    let bag = crate::construction::spawn_detached(world, TREAT_BAG)?;
    world.entity_mut(bag).insert(Networked);
    crate::storage::put_inside(world, bag, pack);
    for _ in 0..6 {
        let Some(&path) = world.resource_mut::<EventRng>().pick(&TREATS) else {
            continue;
        };
        if let Some(treat) = crate::construction::spawn_detached(world, path) {
            world.entity_mut(treat).insert(Networked);
            crate::storage::put_inside(world, treat, bag);
        }
    }
    Some(bag)
}

struct Easter;

impl RoundEvent for Easter {
    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        const LINES: [&str; 4] = [
            "Hip-hop into Easter!",
            "Find some Bunny's stash!",
            "Today is National 'Hunt a Wabbit' Day.",
            "Be kind, give Chocolate Eggs!",
        ];
        let text = world.resource_mut::<EventRng>().pick(&LINES).copied();
        if let Some(text) = text {
            super::priority_announce(world, "", text);
        }
    }
}

struct Rabbits;

impl RoundEvent for Rabbits {
    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        let text = format!(
            "На борту {} обнаружены неопознанные пушистые объекты. Остерегайтесь очаровашек.",
            super::station_name(world)
        );
        super::priority_announce_sound(
            world,
            "Пушистая тревога",
            &text,
            crate::announce::ANNOUNCER_ALIENS,
        );
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let spawns = world.resource::<crate::server::SpawnPoints>();
        let (maint, carp) = (spawns.maintenance.clone(), spawns.carp.clone());
        for (tiles, path) in [(maint, RABBIT), (carp, SPACE_RABBIT)] {
            for pos in tiles {
                if world.resource_mut::<EventRng>().0.prob(15.0) {
                    crate::mobs::spawn::spawn_mob(world, path, pos);
                }
            }
        }
    }
}
