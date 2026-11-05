//! Grafting (tg `grafts.dm`, `seeds.dm` `create_graft`/`apply_graft`, the secateurs and graft
//! branches of `/obj/machinery/hydroponics/item_interaction`): secateurs cut a graft off a
//! harvestable plant (once, three times in a multigraft tray), costing it 5 health; a graft on a
//! growing plant gives it the mother plant's `graft_gene` and pulls its stats two thirds of the
//! way up to the graft's; a graft medium tray takes a graft in place of a seed.
//!
//! A graft is an `/obj/item/graft` carrying a copy of the mother plant's [`Seed`] (its
//! `plant_dna`), which the plant analyzer reads like any seed.

use bevy::prelude::*;
use ss13_mapload::plants::GRAFT;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use super::genes::{Gene, REPEATED_HARVEST, trait_can_add};
use super::plants::spawn_seed_like;
use super::produce::gene_name;
use super::seeds::{Dice, Seed, dm_round};
use super::tray::{PlantStatus, Tray};
use super::{Botany, Plants, refresh_tray};
use crate::chem::{name_of, visible};
use crate::combat::CombatRng;
use crate::tools::tell;

pub const SECATEURS_ROOT: &str = "/obj/item/secateurs";
/// Tray flags `MULTIGRAFT`, `GRAFT_MEDIUM`; `MULTI_GRAFT_MAX_COUNT`.
pub const MULTIGRAFT: u32 = 1 << 2;
pub const GRAFT_MEDIUM: u32 = 1 << 3;
pub const MULTI_GRAFT_MAX_COUNT: u32 = 3;
const EQUIPMENT_DMI: &str = "icons/obj/service/hydroponics/equipment.dmi";
/// `pick(10 ; "graft_plant", 5 ; "graft_flower", 4 ; "graft_mushroom", 1 ; "graft_doom")`.
const GRAFT_STATES: [(&str, i32); 4] = [
    ("graft_plant", 10),
    ("graft_flower", 5),
    ("graft_mushroom", 4),
    ("graft_doom", 1),
];

/// The graft's live data: the mother plant's seed.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Graft;

/// `/obj/item/seeds/proc/apply_graft(snip)`: the graft's `graft_gene` (default perennial
/// growth) when the seed can take it, and every stat moved two thirds of the way up to the
/// graft's (never down). Returns the added gene.
pub fn apply_graft(seed: &mut Seed, graft: &Seed, plants: &Plants) -> Option<Gene> {
    let graft_gene = plants
        .seed_def(&graft.path)
        .map(|d| d.graft_gene.as_str())
        .filter(|g| !g.is_empty())
        .unwrap_or(REPEATED_HARVEST);
    let added = plants
        .trait_def(graft_gene)
        .filter(|def| trait_can_add(def, seed))
        .map(Gene::new_trait);
    if let Some(gene) = &added {
        seed.genes.push(gene.clone());
    }
    let pull = |mine: f32, theirs: f32| dm_round(mine.max(mine + (2.0 / 3.0) * (theirs - mine)));
    seed.set_lifespan(pull(seed.lifespan, graft.lifespan));
    seed.set_endurance(pull(seed.endurance, graft.endurance));
    seed.set_production(pull(seed.production, graft.production));
    seed.set_weed_rate(pull(seed.weed_rate, graft.weed_rate));
    seed.set_weed_chance(pull(seed.weed_chance, graft.weed_chance));
    seed.set_yield(pull(seed.yield_, graft.yield_));
    added
}

/// Secateurs on a tray: a graft off a harvestable plant onto the tray's tile.
pub fn take_graft(world: &mut World, client: Entity, mob: Entity, tray: Entity) {
    let Some(mut state) = world.get::<Tray>(tray).cloned() else {
        return;
    };
    let Some(seed) = state.myseed.clone() else {
        tell(world, client, Span::Notice, "This plot is empty.");
        return;
    };
    if state.plant_status != PlantStatus::Harvestable {
        tell(
            world,
            client,
            Span::Notice,
            "This plant must be harvestable in order to be grafted.",
        );
        return;
    }
    let max = if state.tray_flags & MULTIGRAFT != 0 {
        MULTI_GRAFT_MAX_COUNT
    } else {
        1
    };
    if seed.grafts_taken >= max {
        tell(
            world,
            client,
            Span::Notice,
            "You can't take any more cuttings from this plant!",
        );
        return;
    }
    let (user, name) = (name_of(world, mob), name_of(world, tray));
    visible(
        world,
        mob,
        &format!("You carefully graft off a portion of {name}."),
        &format!("{user} grafts off a limb from {name}."),
        Span::Notice,
    );
    // `drop_location()` of the tray: its own tile.
    let Some(&pos) = world.get::<TilePos>(tray) else {
        return;
    };
    if spawn_graft(world, &seed, pos).is_none() {
        return;
    }
    if let Some(s) = state.myseed.as_mut() {
        s.grafts_taken += 1;
    }
    state.adjust_plant_health(-5.0);
    world.entity_mut(tray).insert(state);
    refresh_tray(world, tray);
}

/// `new /obj/item/graft(loc, mother_plant)`: a copy of the seed, a random sprite, the plant's
/// name in its own.
pub fn spawn_graft(world: &mut World, mother: &Seed, pos: TilePos) -> Option<Entity> {
    let graft = spawn_seed_like(world, GRAFT, mother, pos)?;
    world.entity_mut(graft).insert(Graft);
    let base = name_of(world, graft);
    world
        .entity_mut(graft)
        .insert(Name::new(format!("{base} ({})", mother.plantname)));
    let total: i32 = GRAFT_STATES.iter().map(|(_, w)| w).sum();
    let mut roll = world.resource_mut::<CombatRng>().rand(1, total);
    let state = GRAFT_STATES
        .iter()
        .find(|(_, w)| {
            roll -= w;
            roll <= 0
        })
        .map_or(GRAFT_STATES[0].0, |(s, _)| *s);
    let id = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(EQUIPMENT_DMI, state));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(graft)) {
        appearance.icon_state = id;
    }
    Some(graft)
}

/// A graft on a tray: grafted onto its plant, or planted in an empty graft medium tray. The
/// graft is used up either way (but not on an empty ordinary tray).
pub fn use_graft(world: &mut World, client: Entity, held: Entity, tray: Entity) {
    let (Some(mut state), Some(graft)) = (
        world.get::<Tray>(tray).cloned(),
        world.get::<Seed>(held).cloned(),
    ) else {
        return;
    };
    let plants = world.resource::<Botany>().0.clone();
    if state.myseed.is_none() {
        if state.tray_flags & GRAFT_MEDIUM == 0 {
            tell(world, client, Span::Notice, "The tray is empty.");
            return;
        }
        let name = name_of(world, held);
        let now = world.resource::<Time>().elapsed_secs_f64();
        state.plant(graft.copy(&plants), now);
        world.entity_mut(tray).insert(state);
        world.despawn(held);
        tell(world, client, Span::Notice, format!("You plant {name}."));
        refresh_tray(world, tray);
        return;
    }
    let Some(seed) = state.myseed.as_mut() else {
        return;
    };
    let plantname = seed.plantname.clone();
    let snip = name_of(world, held);
    let text = match apply_graft(seed, &graft, &plants) {
        Some(gene) => format!(
            "You carefully integrate the grafted plant limb onto {plantname}, granting it {}.",
            gene_name(&plants, &gene)
        ),
        None => format!(
            "You try to integrate the grafted plant limb onto {plantname}, but it rejects the trait from the {snip}."
        ),
    };
    world.entity_mut(tray).insert(state);
    world.despawn(held);
    tell(world, client, Span::Notice, text);
    refresh_tray(world, tray);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::botany::test_plants;

    #[test]
    fn graft_adds_its_gene_and_pulls_stats_up() {
        let plants = test_plants();
        let def = plants.seed_def("/obj/item/seeds/starthistle").unwrap();
        let mut seed = Seed::new(def, &plants);
        let mut graft = Seed::new(plants.seed_def("/obj/item/seeds/tomato").unwrap(), &plants);
        graft.lifespan = 55.0;
        graft.endurance = 10.0;
        graft.yield_ = 8.0;
        let gene = apply_graft(&mut seed, &graft, &plants).expect("perennial growth");
        assert_eq!(gene.path(), REPEATED_HARVEST);
        // 25 + 2/3 * 30 = 45; endurance never goes down; 2 + 2/3 * 6 = 6.
        assert_eq!(seed.lifespan, 45.0);
        assert_eq!(seed.endurance, 15.0);
        assert_eq!(seed.yield_, 6.0);
        // A second graft of the same gene adds none, but still moves the stats.
        graft.lifespan = 90.0;
        assert!(apply_graft(&mut seed, &graft, &plants).is_none());
        assert_eq!(seed.lifespan, 75.0);
    }
}
