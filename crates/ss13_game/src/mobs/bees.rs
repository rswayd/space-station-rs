//! Bees (server), after tg `/mob/living/basic/bee` (`farm_animals/bee/_bee.dm`,
//! `bee_ai_behavior.dm`, `bee_bt.dm`). Their trees are in `crate::mobs::ai::trees` (`space`).
//!
//! - A bee is drawn as its base, a stripe layer in its reagent's colour (`#e5e500` without
//!   one) and its wings; a bee with a reagent is "bee (name)" and its sting injects 1-5 u of it
//!   (`venomous`). Toxin bees pick a random toxin; timed bees die after 50 s (short ones 25 s).
//!   A dead bee leaves `/obj/item/trash/bee` (timed bees just vanish).
//! - They sting whoever is not bee-friendly (`bee_friendly`: thick suit and hat, drones, bots,
//!   butterflies) and fly around otherwise.
//! - Every 10 s a worker (85%) may pick a tray in view within 10 with a living plant that no bee
//!   visited in 20 s, flies to it and pollinates it (`pollinate`): the plant heals by half the
//!   bee's health, 33% pests -10, 33% yield +1 (and `yieldmod` 2), 50% potency +1.
// shortcut: no hives (the apiary, honeycombs and the queen bee item belong to botany): no
// homes, honey resources, hive commands or swarming; no allergenic histamine stings, no
// picking bees up, no plant-mob (`MOB_PLANT`) friendliness.

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::mobs::MobDef;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

use super::BasicMob;
use super::ai::behaviors::{DELAY_FAIL, DELAY_SUCCEED, FAIL, Perform, can_see};
use super::ai::{AiCtx, Key, Value};
use crate::combat::CombatRng;
use crate::health::{Health, Stat};

pub const BEE: &str = "/mob/living/basic/bee";
const QUEEN: &str = "/mob/living/basic/bee/queen";
const TOXIN_BEE: &str = "/mob/living/basic/bee/toxin";
const TIMED: &str = "/mob/living/basic/bee/timed";
const TIMED_SHORT: &str = "/mob/living/basic/bee/timed/short";
/// What a dead bee leaves (`GAME_RUNTIME_TYPES`).
pub const BEE_CORPSE: &str = "/obj/item/trash/bee";
const TOXIN_ROOT: &str = "/datum/reagent/toxin";
/// `BEE_DEFAULT_COLOUR`.
const DEFAULT_COLOUR: [u8; 4] = [0xe5, 0xe5, 0x00, 0xff];
/// `lifespan` of the timed bees.
const LIFESPAN: f64 = 50.0;
const LIFESPAN_SHORT: f64 = 25.0;
/// `BEE_TRAY_RECENT_VISIT`, `BEE_POLLINATE_*_CHANCE`.
const RECENT_VISIT: f64 = 20.0;
const PEST_CHANCE: f32 = 33.0;
const YIELD_CHANCE: f32 = 33.0;
const POTENCY_CHANCE: f32 = 50.0;
/// `pollinatable_hydro` `vision_range`.
const TRAY_VISION: u32 = 10;

/// A bee's `beegent` (reagent path), `is_queen` and the time a timed bee dies.
#[derive(Component, Clone, Debug, Default)]
pub struct Bee {
    pub beegent: Option<String>,
    pub queen: bool,
    timed: bool,
    dies_at: Option<f64>,
}

/// A tray's `recent_bee_visit` until then.
#[derive(Component, Clone, Copy, Debug)]
pub struct BeeVisited(f64);

pub(super) fn build(app: &mut App) {
    app.add_systems(Update, timed_deaths.after(crate::health::HealthSystems));
}

pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    if !is_under(&def.path, BEE) {
        return;
    }
    let t = crate::tools::now(world);
    let timed = is_under(&def.path, TIMED);
    let lifespan = if is_under(&def.path, TIMED_SHORT) {
        LIFESPAN_SHORT
    } else {
        LIFESPAN
    };
    world.entity_mut(mob).insert(Bee {
        beegent: None,
        queen: is_under(&def.path, QUEEN),
        timed,
        dies_at: timed.then_some(t + lifespan),
    });
    if is_under(&def.path, TOXIN_BEE) {
        // `assign_random_toxin_reagent`: `get_random_reagent_id` among the toxins.
        let pool: Vec<String> = world
            .get_resource::<crate::botany::Botany>()
            .map(|b| {
                b.0.data
                    .random_reagents
                    .iter()
                    .filter(|p| is_under(p, TOXIN_ROOT))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if !pool.is_empty() {
            let i = world
                .resource_mut::<CombatRng>()
                .range(0, pool.len() as u32 - 1) as usize;
            assign_reagent(world, mob, &pool[i]);
            return;
        }
    }
    generate_visuals(world, mob);
}

/// `assign_reagent`: the bee's venom, name and stripes.
pub fn assign_reagent(world: &mut World, bee: Entity, path: &str) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(id) = chem.id(path) else {
        return;
    };
    let reagent = chem.def(id).name.clone();
    let base = world
        .get::<BasicMob>(bee)
        .map_or_else(|| "bee".to_owned(), |b| b.def.name.clone());
    if let Some(mut b) = world.get_mut::<Bee>(bee) {
        b.beegent = Some(path.to_owned());
    }
    world
        .entity_mut(bee)
        .insert(Name::new(format!("{base} ({reagent})")));
    generate_visuals(world, bee);
}

/// `generate_bee_visuals`: `[icon_base]_base` with the tinted `_grey` stripes and the wings.
fn generate_visuals(world: &mut World, bee: Entity) {
    let Some(b) = world.get::<Bee>(bee).cloned() else {
        return;
    };
    let base = if b.queen { "queen" } else { "bee" };
    let color = b
        .beegent
        .as_deref()
        .and_then(|path| {
            let chem = world.get_resource::<crate::chem::Chem>()?;
            let id = chem.0.id(path)?;
            ss13_reagents::parse_color(&chem.0.def(id).color)
        })
        .unwrap_or(DEFAULT_COLOUR);
    let Some(dmi) = world.get::<BasicMob>(bee).map(|m| m.def.icon.clone()) else {
        return;
    };
    let state = |world: &World, s: String| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&dmi, &s))
    };
    let body = format!("{base}_base");
    let (stripes, wings) = (
        state(world, format!("{base}_grey")),
        state(world, format!("{base}_wings")),
    );
    if let Some(mut basic) = world.get_mut::<BasicMob>(bee) {
        basic.icon_living = body.clone();
        basic.icon_dead = body.clone();
    }
    super::set_icon_state(world, bee, &dmi, &body);
    let overlay = |icon_state, color| ss13_shared::Overlay {
        icon_state,
        dir: None,
        color,
        pixel_offset: [0, 0],
        plane: None,
        only_dirs: 0,
    };
    if let Some(mut appearance) = world.get_mut::<ss13_shared::Appearance>(bee) {
        appearance.overlays = [
            stripes.map(|s| overlay(s, Some(color))),
            wings.map(|w| overlay(w, Some([255; 4]))),
        ]
        .into_iter()
        .flatten()
        .collect();
    }
}

/// `bee_friendly`: plants (not ported), drones, bots, butterflies, and whoever wears a thick
/// suit and a thick hat.
pub fn bee_friendly(world: &World, mob: Entity) -> bool {
    if let Some(b) = world.get::<BasicMob>(mob) {
        return [
            "/mob/living/basic/butterfly",
            "/mob/living/basic/bot",
            "/mob/living/basic/drone",
        ]
        .iter()
        .any(|root| is_under(&b.def.path, root));
    }
    let Some(inventory) = world.get::<crate::inventory::Inventory>(mob) else {
        return false;
    };
    let flags = |item: Option<Entity>| {
        item.and_then(|i| world.get::<ss13_mapload::DmClothing>(i))
            .map(|c| c.clothing_flags)
    };
    match (flags(inventory.suit), flags(inventory.head)) {
        (Some(suit), Some(hat)) => suit & hat & crate::chem::THICKMATERIAL != 0,
        _ => false,
    }
}

/// `venomous` after a landed sting: 1-5 u of the bee's reagent.
pub fn after_melee(world: &mut World, bee: Entity, target: Entity) {
    let Some(path) = world.get::<Bee>(bee).and_then(|b| b.beegent.clone()) else {
        return;
    };
    if world.get::<Stat>(target).is_none_or(|s| *s == Stat::Dead) {
        return;
    }
    let amount = world.resource_mut::<CombatRng>().range(1, 5) as f32;
    crate::medical::cyberimps::add_blood_reagent(world, target, &path, amount);
}

/// `death` -> `spawn_corpse` (before `DEL_ON_DEATH` takes the bee).
pub(super) fn on_death(world: &mut World, bee: Entity, pos: TilePos) {
    if world.get::<Bee>(bee).is_some_and(|b| !b.timed) {
        crate::construction::spawn_obj(world, BEE_CORPSE, pos);
    }
}

/// The timed bees' `addtimer(death, lifespan)`.
fn timed_deaths(world: &mut World) {
    let t = crate::tools::now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &Bee)>()
        .iter(world)
        .filter(|(_, b)| b.dies_at.is_some_and(|at| t >= at))
        .map(|(e, _)| e)
        .collect();
    for bee in due {
        if let Some(mut b) = world.get_mut::<Bee>(bee) {
            b.dies_at = None;
        }
        if let Some(mut stat) = world.get_mut::<Stat>(bee) {
            stat.set_if_neq(Stat::Dead);
        }
    }
}

/// `can_bee_pollinate`: a seed, a plant that is not dead, no bee there lately.
fn pollinatable(world: &World, tray: Entity, now: f64) -> bool {
    let Some(state) = world.get::<crate::botany::tray::Tray>(tray) else {
        return false;
    };
    state.myseed.is_some()
        && state.plant_status != crate::botany::tray::PlantStatus::Dead
        && world.get::<BeeVisited>(tray).is_none_or(|v| now >= v.0)
}

/// `update_interaction_target` over `oview_single_type/hydroponics` (`pollinatable_hydro`).
pub(crate) fn acquire_tray(ctx: &mut AiCtx, key: Key) -> Perform {
    let Some(from) = ctx.pawn_pos() else {
        return DELAY_FAIL;
    };
    let now = ctx.now;
    let world = &*ctx.world;
    let mut trays: Vec<(u32, Entity)> = world
        .resource::<SpatialIndex>()
        .in_radius(from, TRAY_VISION)
        .filter(|&e| pollinatable(world, e, now))
        .filter_map(|e| {
            let to = *world.get::<TilePos>(e)?;
            can_see(world, from, to, TRAY_VISION).then(|| (from.chebyshev(to).unwrap_or(0), e))
        })
        .collect();
    trays.sort_by_key(|&(d, e)| (d, e.to_bits()));
    match trays.first() {
        Some(&(_, tray)) => {
            ctx.bb.set(key, Value::Entity(tray));
            DELAY_SUCCEED
        }
        None => DELAY_FAIL,
    }
}

/// `pollinate_hydro` next to the tray.
pub(crate) fn pollinate_leaf(ctx: &mut AiCtx, key: Key) -> Perform {
    let Some(tray) = ctx.bb.entity(key) else {
        return FAIL;
    };
    let (Some(from), Some(&to)) = (ctx.pawn_pos(), ctx.world.get::<TilePos>(tray)) else {
        return DELAY_FAIL;
    };
    if from.chebyshev(to).is_none_or(|d| d > 1) {
        return DELAY_FAIL;
    }
    pollinate(ctx.world, ctx.pawn, tray);
    DELAY_SUCCEED
}

/// `pollinate(hydro)`.
pub fn pollinate(world: &mut World, bee: Entity, tray: Entity) -> bool {
    let t = crate::tools::now(world);
    if !pollinatable(world, tray, t) {
        return false;
    }
    world.entity_mut(tray).insert(BeeVisited(t + RECENT_VISIT));
    // "Health also means how many bees are in the swarm, roughly."
    let growth = world.get::<Health>(bee).map_or(0.0, Health::health);
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let (pests, more, stronger) = (
        rng.chance(PEST_CHANCE),
        rng.chance(YIELD_CHANCE),
        rng.chance(POTENCY_CHANCE),
    );
    world.resource_mut::<CombatRng>().0 = rng.0;
    let Some(mut state) = world.get_mut::<crate::botany::tray::Tray>(tray) else {
        return false;
    };
    state.adjust_plant_health(growth * 0.5);
    if pests {
        state.adjust_pestlevel(-10.0);
    }
    if more {
        if let Some(seed) = state.myseed.as_mut() {
            seed.adjust_yield(1.0);
        }
        state.yieldmod = 2.0;
    }
    if stronger && let Some(seed) = state.myseed.as_mut() {
        seed.adjust_potency(1.0);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thick_suit_and_hat_make_one_bee_friendly() {
        let mut world = World::new();
        let thick = |world: &mut World, flags: u32| {
            world
                .spawn(ss13_mapload::DmClothing {
                    clothing_flags: flags,
                    ..default()
                })
                .id()
        };
        let suit = thick(&mut world, crate::chem::THICKMATERIAL);
        let hat = thick(&mut world, crate::chem::THICKMATERIAL);
        let thin_hat = thick(&mut world, 0);
        let keeper = world
            .spawn(crate::inventory::Inventory {
                suit: Some(suit),
                head: Some(hat),
                ..default()
            })
            .id();
        let tourist = world
            .spawn(crate::inventory::Inventory {
                suit: Some(suit),
                head: Some(thin_hat),
                ..default()
            })
            .id();
        assert!(bee_friendly(&world, keeper));
        assert!(!bee_friendly(&world, tourist));
    }
}
