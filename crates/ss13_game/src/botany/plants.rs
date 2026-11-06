//! What players do with plants: tray clicks (`item_interaction`, `attack_hand`, `propagate_plant`),
//! the harvest (`/obj/item/seeds/proc/harvest` and `/obj/item/food/grown/Initialize`), the
//! seed extractor (`seedify`) and the plant analyzer (`analyze`, as text).

use bevy::prelude::*;
use ss13_mapload::plants::{GROWN_INEDIBLE_ROOT, GROWN_ROOT, SEED_ROOT};
use ss13_mapload::{DmFood, DmItem, DmTool};
use ss13_net::chat::server::SendChat;
use ss13_reagents::{Holder, is_under};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;

use super::genes::{
    Gene, MAXCHEM, PLANT_GENE_MUTATABLE, REPEATED_HARVEST, SOIL_LOVER, trait_can_add,
};
use super::seeds::{
    Dice, MUTATE_EARLY, NO_PLANTING, SOIL, SOIL_LOVER_HYDRO_POTENCY_MAX,
    SOIL_LOVER_HYDRO_POTENCY_MIN, Seed, dm_round,
};
use super::tray::{MAX_TRAY_PESTS, MAX_TRAY_TOXINS, MAX_TRAY_WEEDS, PlantStatus, Tray};
use super::{Botany, BotanyJob, Plants, refresh_tray};
use crate::chem::{Chem, Reagents, TransferAmount, dm_num, name_of, visible};
use crate::combat::CombatRng;
use crate::construction::Job;
use crate::tools::{Actor, DoAfter, tell, use_tool};

pub const ANALYZER_ROOT: &str = "/obj/item/plant_analyzer";
pub const EXTRACTOR_ROOT: &str = "/obj/machinery/seed_extractor";
const CULTIVATOR_ROOT: &str = "/obj/item/cultivator";
const CONTAINER_ROOT: &str = crate::chem::CONTAINER_ROOT;
const SYRINGE_ROOT: &str = "/obj/item/reagent_containers/syringe";
const PILL_ROOT: &str = "/obj/item/reagent_containers/applicator/pill";
const WATER: &str = "/datum/reagent/water";
/// `/obj/machinery/seed_extractor::seed_multiplier` with a tier 1 servo.
const SEED_MULTIPLIER: i32 = 1;
/// `use_tool(src, user, 5 SECONDS)` of a shovel on a tray, deciseconds.
const DIG_TIME: f32 = 50.0;

fn item_path(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

fn plants(world: &World) -> std::sync::Arc<Plants> {
    world.resource::<Botany>().0.clone()
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn is_grown(path: &str) -> bool {
    is_under(path, GROWN_ROOT) || is_under(path, GROWN_INEDIBLE_ROOT)
}

/// The tray's name as tg's `update_name` makes it: `"hydroponics tray (Tomato Plants)"`.
fn tray_target(world: &World, tray: Entity) -> String {
    match world.get::<Tray>(tray).and_then(|t| t.myseed.as_ref()) {
        Some(seed) => seed.plantname.clone(),
        None => name_of(world, tray),
    }
}

/// `/obj/machinery/hydroponics/item_interaction` (and `_secondary`).
pub fn tray_item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    tray: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let path = item_path(world, held);
    if modifiers.right {
        if world
            .get::<DmTool>(held)
            .is_some_and(|t| t.behaviour == "shovel")
            && super::soil::shovel_secondary(world, client, mob, held, tray)
        {
            return true;
        }
        if is_under(&path, SYRINGE_ROOT) {
            tell(
                world,
                client,
                Span::Warning,
                "You can't get any extract out of this plant.",
            );
            return true;
        }
        return false;
    }
    let edible = world.get::<DmFood>(held).is_some();
    if (edible || is_under(&path, CONTAINER_ROOT)) && world.get::<Reagents>(held).is_some() {
        pour(world, client, mob, held, tray, edible, &path);
        return true;
    }
    if is_under(&path, SEED_ROOT) {
        propagate_plant(world, client, mob, held, tray);
        return true;
    }
    if is_under(&path, super::grafts::SECATEURS_ROOT) {
        super::grafts::take_graft(world, client, mob, tray);
        return true;
    }
    if is_under(&path, ss13_mapload::plants::GRAFT) {
        super::grafts::use_graft(world, client, held, tray);
        return true;
    }
    if is_under(&path, CULTIVATOR_ROOT) {
        let Some(mut state) = world.get::<Tray>(tray).cloned() else {
            return true;
        };
        if state.weedlevel <= 0.0 {
            tell(
                world,
                client,
                Span::Warning,
                "This plot is completely devoid of weeds! It doesn't need uprooting.",
            );
            return true;
        }
        state.weedlevel = 0.0;
        world.entity_mut(tray).insert(state);
        let user = name_of(world, mob);
        let name = name_of(world, tray);
        visible(
            world,
            mob,
            &format!("You remove the weeds from {name}."),
            &format!("{user} uproots the weeds."),
            Span::Notice,
        );
        refresh_tray(world, tray);
        return true;
    }
    if world
        .get::<DmTool>(held)
        .is_some_and(|t| t.behaviour == "shovel")
    {
        let has_plant = world
            .get::<Tray>(tray)
            .is_some_and(|t| t.myseed.is_some() || t.weedlevel > 0.0);
        let name = name_of(world, tray);
        if !has_plant {
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} doesn't have any plants or weeds!"),
            );
            return true;
        }
        let user = name_of(world, mob);
        visible(
            world,
            mob,
            &format!("You start digging out {name}'s plants..."),
            &format!("{user} starts digging out {name}'s plants..."),
            Span::Notice,
        );
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        use_tool(world, actor, tray, DIG_TIME, 0, Job::Botany(BotanyJob::Dig));
        return true;
    }
    false
}

/// The end of digging: the plant (and the weeds) gone if there still is one.
pub fn complete_dig(world: &mut World, mob: Entity, action: &DoAfter) {
    let Some(mut state) = world.get::<Tray>(action.target).cloned() else {
        return;
    };
    if state.myseed.is_none() && state.weedlevel <= 0.0 {
        return;
    }
    state.remove_plant();
    state.weedlevel = 0.0;
    world.entity_mut(action.target).insert(state);
    let (user, name) = (name_of(world, mob), name_of(world, action.target));
    visible(
        world,
        mob,
        &format!("You dig out all of {name}'s plants!"),
        &format!("{user} digs out the plants in {name}!"),
        Span::Notice,
    );
    refresh_tray(world, action.target);
}

/// Reagent containers and food on a tray: water fills it, the rest feeds it; food is composted.
fn pour(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    tray: Entity,
    edible: bool,
    path: &str,
) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let source_name = name_of(world, held);
    let (Some(mut source), Some(mut target)) = (
        world.get::<Reagents>(held).map(|r| r.0.clone()),
        world.get::<Reagents>(tray).map(|r| r.0.clone()),
    ) else {
        return;
    };
    if source.total_volume <= 0.0 {
        tell(
            world,
            client,
            Span::Warning,
            format!("{source_name} is empty!"),
        );
        return;
    }
    let water = chem.id(WATER);
    let has_water = water.is_some_and(|w| source.amount(w) >= 1.0);
    if target.holder_full() && !has_water {
        let name = name_of(world, tray);
        tell(world, client, Span::Notice, format!("{name} is full."));
        return;
    }
    let plant = tray_target(world, tray);
    let user = name_of(world, mob);
    let (transfer, message) = if edible {
        (
            source.total_volume,
            format!("{user} composts {source_name}, spreading it through {plant}"),
        )
    } else {
        let per = world.get::<TransferAmount>(held).map_or(5.0, |t| t.0);
        let message = if is_under(path, SYRINGE_ROOT) {
            format!("{user} injects {plant} with {source_name}")
        } else {
            String::new()
        };
        (per.min(source.total_volume), message)
    };
    if !message.is_empty() {
        visible(
            world,
            mob,
            &format!("{message}."),
            &format!("{message}."),
            Span::Notice,
        );
    }
    let Some(mut state) = world.get::<Tray>(tray).cloned() else {
        return;
    };
    match water.filter(|&w| source.amount(w) > 0.0) {
        Some(w) => {
            let water_amt = source.amount(w) * transfer / source.total_volume;
            let added = state.adjust_waterlevel(dm_round(water_amt));
            source.remove_reagent(w, added);
            // tg divides by the total after the water left.
            let others: Vec<(u16, f32)> = source
                .reagents
                .iter()
                .filter(|r| r.id != w)
                .map(|r| (r.id, r.volume))
                .collect();
            for (id, volume) in others {
                if source.total_volume <= 0.0 {
                    break;
                }
                let amount = volume * transfer / source.total_volume;
                source.trans_to(&chem, &mut target, amount, 1.0, Some(id), false, false);
            }
        }
        None => {
            source.trans_to(&chem, &mut target, transfer, 1.0, None, false, false);
        }
    }
    world.entity_mut(tray).insert(state);
    if let Some(mut r) = world.get_mut::<Reagents>(tray) {
        r.0 = target;
    }
    if edible || is_under(path, PILL_ROOT) {
        world.despawn(held);
    } else if let Some(mut r) = world.get_mut::<Reagents>(held) {
        r.0 = source;
    }
    refresh_tray(world, tray);
}

/// `propagate_plant(seed, user)`.
fn propagate_plant(world: &mut World, client: Entity, _mob: Entity, held: Entity, tray: Entity) {
    let plants = plants(world);
    let name = name_of(world, tray);
    if world.get::<Tray>(tray).is_some_and(|t| t.myseed.is_some()) {
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} already has a plant growing in it!"),
        );
        return;
    }
    let Some(seed) = world.get::<Seed>(held).cloned() else {
        return;
    };
    let seed_name = name_of(world, held);
    if plants
        .seed_def(&seed.path)
        .is_some_and(|d| d.seed_flags & NO_PLANTING != 0)
    {
        tell(
            world,
            client,
            Span::Warning,
            format!("{seed_name} cannot be planted in {name}!"),
        );
        return;
    }
    let t = now(world);
    let Some(mut state) = world.get::<Tray>(tray).cloned() else {
        return;
    };
    state.plant(seed, t);
    world.entity_mut(tray).insert(state);
    // The packet goes into the tray; it never comes back out.
    world.despawn(held);
    tell(
        world,
        client,
        Span::Notice,
        format!("You plant {seed_name}."),
    );
    refresh_tray(world, tray);
}

/// `attack_hand`: harvest, remove the dead plant, or examine.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, tray: Entity) {
    let Some(status) = world.get::<Tray>(tray).map(|t| t.plant_status) else {
        return;
    };
    match status {
        PlantStatus::Harvestable => harvest(world, client, mob, tray),
        PlantStatus::Dead => {
            let name = name_of(world, tray);
            if let Some(mut state) = world.get::<Tray>(tray).cloned() {
                state.set_seed(None);
                world.entity_mut(tray).insert(state);
            }
            tell(
                world,
                client,
                Span::Notice,
                format!("You remove the dead plant from {name}."),
            );
            refresh_tray(world, tray);
        }
        _ => {
            let (name, lines) = crate::examine::examine_lines(world, mob, tray);
            for message in crate::examine::examine_messages(&name, lines) {
                world.write_message(SendChat {
                    clients: vec![client],
                    message,
                });
            }
        }
    }
}

/// `/obj/item/seeds/proc/harvest(user)` then `update_tray`: `getYield()` products onto the
/// harvester's tile, each with a copy of the seed (or, unstable and lucky, a wild mutation's).
pub fn harvest(world: &mut World, client: Entity, mob: Entity, tray: Entity) {
    let plants = plants(world);
    let (Some(mut state), Some(&pos)) =
        (world.get::<Tray>(tray).cloned(), world.get::<TilePos>(mob))
    else {
        return;
    };
    let Some(seed) = state.myseed.clone() else {
        return;
    };
    let Some(def) = plants.seed_def(&seed.path).cloned() else {
        return;
    };
    let count = seed
        .harvest_yield(state.tray_flags, state.yieldmod)
        .max(0.0) as u32;
    let mut made = 0;
    // tg loops until `count` products exist; a mutation without a product is rolled again.
    let mut attempts = 0;
    while made < count && attempts < count * 20 {
        attempts += 1;
        let wild = seed.instability >= 30.0
            && def.seed_flags & MUTATE_EARLY != 0
            && !seed.mutatelist.is_empty()
            && roll(world, |d| d.prob(seed.instability / 3.0));
        if wild {
            let i = roll(world, |d| d.pick(seed.mutatelist.len()));
            let Some(mdef) = plants.seed_def(&seed.mutatelist[i]) else {
                continue;
            };
            if mdef.product.is_empty() {
                continue;
            }
            let mut mutated = Seed::new(mdef, &plants);
            for gene in &seed.genes {
                if let Gene::Trait { path, .. } = gene
                    && gene.mutability() & PLANT_GENE_MUTATABLE != 0
                    && plants
                        .trait_def(path)
                        .is_some_and(|t| trait_can_add(t, &mutated))
                {
                    mutated.genes.push(gene.clone());
                }
            }
            mutated.set_instability(dm_round(seed.instability * 0.5));
            let product = mdef.product.clone();
            spawn_produce(world, &plants, &product, pos, &mutated);
            made += 1;
            continue;
        }
        let mut descendent = seed.copy(&plants);
        if state.tray_flags & SOIL == 0 && seed.has_gene(SOIL_LOVER) {
            let factor = roll(world, |d| {
                d.decimal(SOIL_LOVER_HYDRO_POTENCY_MIN, SOIL_LOVER_HYDRO_POTENCY_MAX)
            });
            descendent.set_potency(dm_round(seed.potency * factor));
        }
        if !def.product.is_empty() {
            spawn_produce(world, &plants, &def.product, pos, &descendent);
        }
        made += 1;
    }
    // `update_tray`.
    state.lastproduce = state.age;
    let text = if count == 0 {
        (
            Span::Warning,
            "You fail to harvest anything useful!".to_owned(),
        )
    } else {
        (
            Span::Notice,
            format!("You harvest {count} items from the {}.", seed.plantname),
        )
    };
    tell(world, client, text.0, text.1);
    if seed.has_gene(REPEATED_HARVEST) {
        state.plant_status = PlantStatus::Growing;
    } else {
        state.set_seed(None);
        // No reason to pay for an empty tray.
        state.self_sustaining = false;
    }
    world.entity_mut(tray).insert(state);
    refresh_tray(world, tray);
}

fn roll<T>(world: &mut World, f: impl FnOnce(&mut CombatRng) -> T) -> T {
    let mut rng = world.resource_mut::<CombatRng>();
    f(&mut rng)
}

/// `new product(loc, new_seed = seed)`: a grown item with a copy of `seed`, its reagents from
/// `prepare_result` at the seed's potency and its potency bite size.
pub fn spawn_produce(
    world: &mut World,
    plants: &Plants,
    product: &str,
    pos: TilePos,
    seed: &Seed,
) -> Option<Entity> {
    let e = crate::construction::spawn_obj(world, product, pos)?;
    let seed = seed.copy(plants);
    if let Some(mut food) = world.get::<DmFood>(e).cloned() {
        // Densified chemicals double `max_volume` (`on_new_plant`, before the bite size).
        let rate = seed
            .genes
            .iter()
            .find_map(|g| match g {
                Gene::Trait { path, rate, .. } if is_under(path, MAXCHEM) => Some(*rate),
                _ => None,
            })
            .unwrap_or(1.0);
        let volume = food.volume * rate;
        let bite_mod = plants.grown(product).map_or(1.0, |(_, m)| m);
        food.bite_consumption = seed.bite_size(volume, bite_mod);
        world.entity_mut(e).insert(food);
        let purity = roll(world, |d| seed.reagent_purity(d));
        if let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
            && let Some(mut reagents) = world.get_mut::<Reagents>(e)
        {
            let holder: &mut Holder = &mut reagents.0;
            holder.clear();
            holder.maximum_volume = volume;
            let list: Vec<(u16, f32)> = seed
                .prepare_amounts(volume)
                .into_iter()
                .filter_map(|(p, a)| chem.id(&p).map(|id| (id, a)))
                .collect();
            holder.add_reagent_list(&chem, &list, Some(purity), false);
        }
        super::produce::on_harvest(world, e, &seed);
    }
    world.entity_mut(e).insert(seed);
    Some(e)
}

/// `/obj/machinery/seed_extractor/item_interaction`: produce becomes `rand(1, 4) *
/// seed_multiplier + 1` seeds (tg's `for(var/_ in 0 to t_max)`) on the extractor's tile.
pub fn extractor_interaction(
    world: &mut World,
    client: Entity,
    _mob: Entity,
    held: Entity,
    extractor: Entity,
) -> bool {
    let path = item_path(world, held);
    if !is_grown(&path) {
        return false;
    }
    let (Some(seed), Some(&pos)) = (
        world.get::<Seed>(held).cloned(),
        world.get::<TilePos>(extractor),
    ) else {
        return false;
    };
    let plants = plants(world);
    let t_max = roll(world, |d| d.rand(1, 4)) * SEED_MULTIPLIER;
    world.despawn(held);
    for _ in 0..=t_max {
        spawn_seed(world, &plants, &seed, pos);
    }
    tell(world, client, Span::Notice, "You extract some seeds.");
    true
}

/// `seed.Copy()` as a packet on `pos`.
pub fn spawn_seed(world: &mut World, plants: &Plants, seed: &Seed, pos: TilePos) -> Option<Entity> {
    let e = crate::construction::spawn_obj(world, &seed.path, pos)?;
    world.entity_mut(e).insert(seed.copy(plants));
    Some(e)
}

/// A `path` item on `pos` holding `seed.Copy()` (a graft's `plant_dna`).
pub fn spawn_seed_like(world: &mut World, path: &str, seed: &Seed, pos: TilePos) -> Option<Entity> {
    let plants = plants(world);
    let e = crate::construction::spawn_obj(world, path, pos)?;
    world.entity_mut(e).insert(seed.copy(&plants));
    Some(e)
}

/// `/obj/item/plant_analyzer/proc/analyze`: the fields of tg's analyzer window as chat lines.
/// False when the target has nothing to scan.
pub fn analyze(world: &mut World, client: Entity, target: Entity) -> bool {
    let plants = plants(world);
    let state = world.get::<Tray>(target).cloned();
    let seed = match &state {
        Some(t) => t.myseed.clone(),
        None => world.get::<Seed>(target).cloned(),
    };
    if state.is_none() && seed.is_none() {
        return false;
    }
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let reagent_name = |path: &str| {
        chem.as_ref()
            .and_then(|c| c.id(path).map(|id| c.def(id).name.clone()))
            .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_owned())
    };
    let n = dm_num;
    let mut lines: Vec<(Span, String)> = Vec::new();
    if let Some(t) = &state {
        let nutrients = world.get::<Reagents>(target).map(|r| r.0.clone());
        let pos = world.get::<TilePos>(target).copied();
        let light = pos.and_then(|p| super::lumcount(world, p)).unwrap_or(1.0);
        lines.push((Span::Bold, name_of(world, target)));
        if let Some(s) = &t.myseed {
            lines.push((
                Span::Info,
                format!(
                    "Health: {}/{}{} | Age: {}",
                    n(t.plant_health),
                    n(s.endurance),
                    if t.plant_status == PlantStatus::Dead {
                        " (dead)"
                    } else {
                        ""
                    },
                    n(t.age)
                ),
            ));
        }
        lines.push((
            Span::Info,
            format!(
                "Water: {}/{} | Nutrients: {}/{} | Light: {}",
                n(t.waterlevel),
                n(t.maxwater),
                n(nutrients.as_ref().map_or(0.0, |h| h.total_volume)),
                n(t.maxnutri),
                n((light.clamp(0.0, 1.0) * 100.0).round() / 100.0)
            ),
        ));
        lines.push((
            Span::Info,
            format!(
                "Weeds: {}/{} | Pests: {}/{} | Toxins: {}/{} | Yield mod: {}",
                n(t.weedlevel),
                n(MAX_TRAY_WEEDS),
                n(t.pestlevel),
                n(MAX_TRAY_PESTS),
                n(t.toxic),
                n(MAX_TRAY_TOXINS),
                n(t.yieldmod)
            ),
        ));
        if let (Some(h), Some(c)) = (&nutrients, &chem)
            && !h.reagents.is_empty()
        {
            let list: Vec<String> = h
                .reagents
                .iter()
                .map(|r| format!("{} {}u", c.def(r.id).name, n(r.volume)))
                .collect();
            lines.push((Span::Info, format!("Tray reagents: {}", list.join(", "))));
        }
    }
    if let Some(s) = &seed {
        let def = plants.seed_def(&s.path);
        lines.push((Span::Bold, s.plantname.clone()));
        lines.push((
            Span::Info,
            format!(
                "Potency: {} | Yield: {} | Instability: {}",
                n(s.potency),
                n(s.yield_),
                n(s.instability)
            ),
        ));
        lines.push((
            Span::Info,
            format!(
                "Maturation: {} | Production: {} | Lifespan: {} | Endurance: {}",
                n(s.maturation),
                n(s.production),
                n(s.lifespan),
                n(s.endurance)
            ),
        ));
        lines.push((
            Span::Info,
            format!(
                "Weeds: rate {}, chance {}%",
                n(s.weed_rate),
                n(s.weed_chance)
            ),
        ));
        let traits: Vec<String> = s
            .genes
            .iter()
            .filter_map(|g| match g {
                Gene::Trait { path, .. } => Some(
                    plants
                        .trait_def(path)
                        .map_or(path.clone(), |t| t.name.clone()),
                ),
                Gene::Reagent { .. } => None,
            })
            .collect();
        if !traits.is_empty() {
            lines.push((Span::Info, format!("Traits: {}", traits.join(", "))));
        }
        let reagents: Vec<String> = s
            .reagents_add()
            .iter()
            .map(|(p, rate)| format!("{} {}%", reagent_name(p), n(rate * 100.0)))
            .collect();
        if !reagents.is_empty() {
            lines.push((Span::Info, format!("Reagents: {}", reagents.join(", "))));
        }
        let mutations: Vec<String> = s
            .mutatelist
            .iter()
            .map(|m| {
                plants
                    .seed_def(m)
                    .map_or(m.clone(), |d| d.plantname.clone())
            })
            .collect();
        if !mutations.is_empty() {
            lines.push((Span::Info, format!("Mutations: {}", mutations.join(", "))));
        }
        if def.is_some_and(|d| d.yield_ == -1.0) {
            lines.push((Span::Info, "It cannot be harvested.".to_owned()));
        }
    }
    if state.is_none()
        && let (Some(h), Some(c)) = (world.get::<Reagents>(target).map(|r| r.0.clone()), &chem)
        && !h.reagents.is_empty()
    {
        let list: Vec<String> = h
            .reagents
            .iter()
            .map(|r| format!("{} {}u", c.def(r.id).name, n(r.volume)))
            .collect();
        lines.push((Span::Info, format!("Contents: {}", list.join(", "))));
    }
    for (span, text) in lines {
        tell(world, client, span, text);
    }
    true
}
