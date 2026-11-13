//! Santa after tg `code/modules/antagonists/santa/santa.dm` (`/datum/antagonist/santa`), the
//! Christmas event "Visit by Santa" (`code/modules/events/holiday/xmas.dm`) and Santa's gift bag
//! (`/obj/item/storage/backpack/santabag`).
//!
//! - The event (Christmas only, weight 20, once, from 30 minutes) announces "Санта прибывает в
//!   город!" and polls the ghosts for 15 s; the chosen one becomes Santa Claus on a random
//!   `blobstart` landmark ([`spawn_santa`]): the Santa outfit (`/datum/outfit/santa`: red suit,
//!   Santa hat, gift bag with five Christmas gifts), long white hair and full beard, the Santa
//!   job, the Santa Teleport spell (`crate::spells::Spell::SantaTeleport`), the greeting and the
//!   objective "Bring joy and presents to the station!".
//! - Santa ([`SantaClaus`], tg `TRAIT_CANNOT_OPEN_PRESENTS`) cannot open presents
//!   (`crate::gifts::open`).
//! - Santa's gift bag adds a Christmas gift (`/obj/item/gift/anything`) every 30 s while Santa
//!   carries it, even when full (`attempt_insert(override = TRUE)`).
// shortcut: the traits sit on the body, not the mind; no present vision (gift contents are
// rolled when opened), no prayer listening (`listen_prayers`), no antag panel or round-end
// entry; add them with mind traits and prayers.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::jobs::OutfitSlot;
use ss13_shared::chat::Span;

use crate::events::controller::{EventRng, EventState, RoundEvent};
use crate::ghost_poll::{self, Candidate, PollSpec};
use crate::inventory::CarriedBy;
use crate::outfit::OutfitTemplates;
use crate::tools::{now, tell};

pub const NAME: &str = "Visit by Santa";
pub const SANTA_OUTFIT: &str = "/datum/outfit/santa";
const SANTA_JOB: &str = "/datum/job/santa";
const SANTA_BAG: &str = "/obj/item/storage/backpack/santabag";
const GIFT: &str = "/obj/item/gift/anything";
/// `poll_time`.
const POLL_SECS: f64 = 15.0;
/// `regenerate_presents`' timer.
const REGEN_SECS: f64 = 30.0;
const OBJECTIVE: &str = "Bring joy and presents to the station!";

/// tg `TRAIT_CANNOT_OPEN_PRESENTS` and `TRAIT_PRESENT_VISION` from the Santa antagonist.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SantaClaus;

/// Santa's gift bag: the next `regenerate_presents`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct SantaBag {
    pub next: f64,
}

pub struct SantaPlugin;

impl Plugin for SantaPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(bag_added).add_systems(
            Update,
            regenerate_presents.run_if(any_with_component::<SantaBag>),
        );
    }
}

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(SantaVisit)
}

/// `/datum/round_event/ghost_role/santa`.
struct SantaVisit;

impl RoundEvent for SantaVisit {
    fn announce(&mut self, world: &mut World, _: &mut EventState, _fake: bool) {
        crate::events::priority_announce(
            world,
            "Неизвестная радиопередача",
            "Санта прибывает в город!",
        );
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let spec = PollSpec {
            question: "Санта прибывает в город! Хотите стать Сантой?".to_owned(),
            role_name: "Santa".to_owned(),
            secs: POLL_SECS,
            ..default()
        };
        let group = ghost_poll::ghosts(world);
        ghost_poll::poll_candidates(world, spec, group, |world, mut signed_up| {
            match ghost_poll::pick_candidate(world, &mut signed_up) {
                Some(chosen) => {
                    spawn_santa(world, chosen);
                }
                None => info!("{NAME}: not enough players signed up"),
            }
        });
    }
}

/// The rest of `santa/start()`: Santa Claus on a random `blobstart`, or `None` without one.
pub fn spawn_santa(world: &mut World, chosen: Candidate) -> Option<Entity> {
    let spots = world
        .get_resource::<crate::server::SpawnPoints>()
        .map(|s| s.blobstart.clone())
        .unwrap_or_default();
    let pos = *world.resource_mut::<EventRng>().pick(&spots)?;
    let mob = crate::server::spawn_mob(world, chosen.client, pos, "Santa Claus".to_owned());
    equip(world, mob);
    crate::noncrew_jobs::set_assigned_role(world, mob, SANTA_JOB);
    // `/datum/outfit/santa/post_equip`.
    if let Some(mut look) = world.get_mut::<crate::human::HumanLook>(mob) {
        look.hair_style = Some("hair_long3".to_owned());
        look.facial_hair_style = Some("facial_fullbeard".to_owned());
        look.hair_color = [255, 255, 255];
        look.facial_hair_color = [255, 255, 255];
    }
    // `on_gain`.
    world.entity_mut(mob).insert(SantaClaus);
    crate::spells::learn(world, mob, crate::spells::Spell::SantaTeleport);
    crate::spells::refresh(world, mob);
    tell(
        world,
        chosen.client,
        Span::BoldWarning,
        "Your objective is to bring joy to the people on this station. You have a magical bag, \
         which generates presents as long as you have it! You can examine the presents to take a \
         peek inside, to make sure that you give the right gift to the right person.",
    );
    super::raiders::announce_objectives(world, chosen.client, &[OBJECTIVE.to_owned()]);
    info!("{mob} is Santa at {pos:?}");
    Some(mob)
}

/// `equipOutfit(/datum/outfit/santa)`: the slots, the gift bag on the back with the box and the
/// gifts.
fn equip(world: &mut World, mob: Entity) {
    let Some(outfit) = world
        .get_resource::<super::raiders::RaiderData>()
        .and_then(|d| d.outfit(SANTA_OUTFIT))
        .cloned()
    else {
        warn!("{SANTA_OUTFIT}: no outfit data (rerun `ss13-mapload convert`)");
        return;
    };
    if !world.contains_resource::<OutfitTemplates>() {
        world.init_resource::<OutfitTemplates>();
    }
    let mut inventory = world.resource_scope(|world, templates: Mut<OutfitTemplates>| {
        crate::outfit::equip_outfit(world, mob, &outfit, &templates.0)
    });
    crate::species::equip_species_outfit(world, mob, &mut inventory);
    if let Some(back) = outfit.item(OutfitSlot::Back) {
        inventory.back = crate::outfit::spawn_back(world, mob, back);
        if let Some(pack) = inventory.back {
            crate::outfit::fill_backpack(world, pack, &outfit);
        }
    }
    world.entity_mut(mob).insert(inventory);
    crate::inventory::refresh_look(world, mob);
}

fn bag_added(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let e = add.entity;
    if items.get(e).is_ok_and(|i| is_under(&i.path, SANTA_BAG)) {
        commands.queue(move |world: &mut World| {
            let next = now(world);
            if let Ok(mut bag) = world.get_entity_mut(e) {
                bag.insert(SantaBag { next });
            }
        });
    }
}

/// The mob `e` is on or in (tg `get(loc, /mob)`).
fn carrier(world: &World, mut e: Entity) -> Option<Entity> {
    for _ in 0..8 {
        e = world
            .get::<CarriedBy>(e)
            .map(|c| c.0)
            .or_else(|| world.get::<crate::storage::StoredIn>(e).map(|s| s.0))?;
        if world.get::<crate::server::PlayerMob>(e).is_some()
            || world.get::<crate::health::Health>(e).is_some()
        {
            return Some(e);
        }
    }
    None
}

/// `regenerate_presents`: every 30 s a bag carried by Santa gets a Christmas gift.
fn regenerate_presents(world: &mut World) {
    let now = now(world);
    let mut q = world.query::<(Entity, &mut SantaBag)>();
    let due: Vec<Entity> = q
        .iter_mut(world)
        .filter(|(_, b)| b.next <= now)
        .map(|(e, mut b)| {
            b.next = now + REGEN_SECS;
            e
        })
        .collect();
    for bag in due {
        let Some(user) = carrier(world, bag) else {
            continue;
        };
        if world.get::<SantaClaus>(user).is_none() {
            continue;
        }
        let gift = world.resource_scope(|world, templates: Mut<OutfitTemplates>| {
            Some(templates.0.get(GIFT)?.spawn(world, None))
        });
        let Some(gift) = gift else {
            continue;
        };
        crate::storage::finish_item(world, gift, 1);
        world.entity_mut(gift).insert(ss13_net::server::Networked);
        crate::storage::put_inside(world, gift, bag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bag_finds_its_carrier_through_storage() {
        let mut world = World::new();
        let santa = world.spawn(crate::health::Health::default()).id();
        let bag = world.spawn(CarriedBy(santa)).id();
        let inner = world.spawn(crate::storage::StoredIn(bag)).id();
        let loose = world.spawn_empty().id();
        assert_eq!(carrier(&world, bag), Some(santa));
        assert_eq!(carrier(&world, inner), Some(santa));
        assert_eq!(carrier(&world, loose), None);
    }
}
