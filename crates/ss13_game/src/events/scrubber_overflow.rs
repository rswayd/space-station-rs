//! Scrubber Overflow (`code/modules/events/scrubber_overflow.dm`): half of the station's unwelded
//! scrubbers (all of them for "Every Vent") each release a short-lived foam of one random
//! reagent; with `danger_chance` the reagent is any random one (`get_random_reagent_id`) and a
//! cockroach and a bloodroach crawl out. BandaStation dropped random glitter from the safe list.
// shortcut: no admin setup (`forced_reagent_type`) and no deadchat line naming it.

use bevy::prelude::*;
use ss13_reagents::{Holder, Owner};
use ss13_shared::TilePos;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::atmos_machinery::VentScrubber;
use crate::effects::foam::{FoamKind, FoamOptions, do_foam};

pub const NORMAL: &str = "Scrubber Overflow: Normal";
pub const THREATENING: &str = "Scrubber Overflow: Threatening";
pub const CATASTROPHIC: &str = "Scrubber Overflow: Catastrophic";
pub const EVERY_VENT: &str = "Scrubber Overflow: Every Vent";

const COCKROACH: &str = "/mob/living/basic/cockroach";
const BLOODROACH: &str = "/mob/living/basic/cockroach/bloodroach";

/// `safer_chems` (BandaStation: without `/datum/reagent/glitter/random`).
pub const SAFER_CHEMS: &[&str] = &[
    "/datum/reagent/water",
    "/datum/reagent/carbon",
    "/datum/reagent/consumable/flour",
    "/datum/reagent/space_cleaner",
    "/datum/reagent/carpet/royal/blue",
    "/datum/reagent/carpet/orange",
    "/datum/reagent/consumable/nutriment",
    "/datum/reagent/consumable/condensedcapsaicin",
    "/datum/reagent/drug/mushroomhallucinogen",
    "/datum/reagent/lube",
    "/datum/reagent/cryptobiolin",
    "/datum/reagent/blood",
    "/datum/reagent/medicine/c2/multiver",
    "/datum/reagent/water/holywater",
    "/datum/reagent/consumable/ethanol",
    "/datum/reagent/consumable/hot_coco",
    "/datum/reagent/consumable/yoghurt",
    "/datum/reagent/consumable/tinlux",
    "/datum/reagent/hydrogen_peroxide",
    "/datum/reagent/bluespace",
    "/datum/reagent/pax",
    "/datum/reagent/consumable/laughter",
    "/datum/reagent/concentrated_barbers_aid",
    "/datum/reagent/baldium",
    "/datum/reagent/colorful_reagent",
    "/datum/reagent/consumable/salt",
    "/datum/reagent/consumable/ethanol/beer",
    "/datum/reagent/hair_dye",
    "/datum/reagent/consumable/sugar",
    "/datum/reagent/gravitum",
    "/datum/reagent/growthserum",
    "/datum/reagent/yuck",
];

/// `/datum/round_event/scrubber_overflow` and its subtypes' vars.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrubberOverflow {
    /// `danger_chance`, percent.
    pub danger_chance: f32,
    /// `reagents_amount`, units per scrubber (also the foam's size).
    pub reagents_amount: u32,
    /// `overflow_probability`, percent per scrubber.
    pub overflow_probability: f32,
    /// `scrubbers`.
    pub scrubbers: Vec<Entity>,
}

impl ScrubberOverflow {
    fn new(danger_chance: f32, reagents_amount: u32, overflow_probability: f32) -> Self {
        Self {
            danger_chance,
            reagents_amount,
            overflow_probability,
            scrubbers: Vec::new(),
        }
    }
}

pub fn make_normal(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ScrubberOverflow::new(1.0, 50, 50.0))
}

pub fn make_threatening(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ScrubberOverflow::new(10.0, 100, 50.0))
}

pub fn make_catastrophic(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ScrubberOverflow::new(30.0, 150, 50.0))
}

pub fn make_every_vent(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ScrubberOverflow::new(1.0, 100, 100.0))
}

/// The unwelded scrubbers on station levels, in entity order.
pub fn station_scrubbers(world: &mut World) -> Vec<Entity> {
    let levels = station_levels(world);
    let mut q = world.query::<(Entity, &VentScrubber, &TilePos)>();
    let mut scrubbers: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s, p)| !s.welded && levels.contains(&p.z))
        .map(|(e, ..)| e)
        .collect();
    scrubbers.sort();
    scrubbers
}

/// The control's `can_spawn_event`: at least one unwelded station scrubber.
pub fn has_scrubbers(world: &mut World) -> bool {
    !station_scrubbers(world).is_empty()
}

impl RoundEvent for ScrubberOverflow {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
        ev.start_when = 5;
        let candidates = station_scrubbers(world);
        let rng = &mut world.resource_mut::<EventRng>().0;
        self.scrubbers = candidates
            .into_iter()
            .filter(|_| rng.prob(self.overflow_probability))
            .collect();
        if self.scrubbers.is_empty() {
            ev.killed = true;
        }
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _fake: bool) {
        super::priority_announce(
            world,
            &format!("{}: Инженерный отдел", super::COMMAND_NAME),
            "Зафиксирован скачок обратного давления в системе вытяжных труб. Возможен выброс \
             содержимого.",
        );
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        for scrubber in std::mem::take(&mut self.scrubbers) {
            let Some(&pos) = world.get::<TilePos>(scrubber) else {
                continue;
            };
            overflow(world, pos, self.danger_chance, self.reagents_amount);
        }
    }
}

/// One scrubber's overflow at `pos`: the reagent (and with danger the roaches), then
/// `create_foam(/datum/effect_system/fluid_spread/foam/short, reagents_amount)`.
pub fn overflow(world: &mut World, pos: TilePos, danger_chance: f32, amount: u32) {
    let dangerous = world.resource_mut::<EventRng>().0.prob(danger_chance);
    let reagent = if dangerous {
        crate::mobs::farm::random_reagent(world)
    } else {
        world
            .resource_mut::<EventRng>()
            .pick(SAFER_CHEMS)
            .map(|p| (*p).to_owned())
    };
    if dangerous {
        crate::mobs::spawn::spawn_mob(world, COCKROACH, pos);
        crate::mobs::spawn::spawn_mob(world, BLOODROACH, pos);
    }
    let carry = reagent.and_then(|path| {
        let chem = world.get_resource::<crate::chem::Chem>()?.0.clone();
        let id = chem.id(&path)?;
        let mut holder = Holder::new(amount as f32, 0, Owner::default());
        holder.add_reagent(
            &chem,
            id,
            amount as f32,
            ss13_atmos::gas::T20C as f32,
            None,
            None,
            true,
            false,
        );
        Some(holder)
    });
    do_foam(
        world,
        0,
        pos,
        Some(amount),
        carry.as_ref(),
        FoamKind::ShortLife,
        FoamOptions::default(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_have_tg_vars() {
        let normal = ScrubberOverflow::new(1.0, 50, 50.0);
        assert_eq!(normal.reagents_amount, 50);
        assert_eq!(SAFER_CHEMS.len(), 32);
        assert!(!SAFER_CHEMS.iter().any(|c| c.contains("glitter")));
    }
}
