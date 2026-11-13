//! The fermenting barrel (tg `/obj/structure/fermenting_barrel`, `fermenting_barrel.dm`): a 600 u
//! reagent holder with a lid. Open, it takes grown food and pours (refillable); closed, it
//! drains from its tap and ferments one fruit per `SSobj` fire (2 s) into its `distill_reagent`
//! or fruit wine (`/obj/item/food/grown/ferment`), the fruit's reagents into the barrel. An empty
//! hand opens or closes the lid, a right click empties the fruits out of an open barrel. The
//! gunpowder and thermite barrels never open and start with 500 u.
// shortcut: no plant bags, wrench anchoring, labels or `can_distill`
// (every grown food ferments).

use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::plants::GROWN_ROOT;
use ss13_reagents::{Holder, Owner, flags, is_under};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use super::Botany;
use super::produce::ferment;
use super::seeds::Seed;
use crate::chem::{Chem, Reagents, name_of};
use crate::combat::CombatRng;
use crate::storage::{Stored, put_inside, take_out};
use crate::tools::tell;

pub const BARREL_ROOT: &str = "/obj/structure/fermenting_barrel";
const GUNPOWDER_BARREL: &str = "/obj/structure/fermenting_barrel/gunpowder";
const THERMITE_BARREL: &str = "/obj/structure/fermenting_barrel/thermite";
const GUNPOWDER: &str = "/datum/reagent/gunpowder";
const THERMITE: &str = "/datum/reagent/thermite";
const STRUCTURES_DMI: &str = "icons/obj/structures.dmi";
const LID_OPEN_SOUND: &str = "sound/items/handling/cardboard_box/cardboardbox_pickup.ogg";
const LID_CLOSE_SOUND: &str = "sound/effects/footstep/woodclaw2.ogg";
const SOUND_VOLUME: f32 = 25.0;
const VOLUME: f32 = 600.0;
/// `SSobj` `wait`, seconds.
const SSOBJ_WAIT: f64 = 2.0;

/// The barrel's lid and fermentation (`open`, `can_open`, `potential_volume`, `fermenting`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Barrel {
    pub open: bool,
    pub can_open: bool,
    pub potential_volume: f32,
    pub fermenting: bool,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(ss13_world::MapInit, setup_barrels)
        .add_systems(
            Update,
            (
                process_barrels,
                // `soundloop` while `fermenting` (`sound_volume` 25, the loop's own).
                crate::looping_sound::sync::<Barrel>(&crate::looping_sound::defs::BOILING, |b| {
                    b.fermenting
                }),
            ),
        );
}

/// `Initialize`: closed, drainable; the pirate barrels full of their powder.
fn setup_barrels(world: &mut World) {
    let barrels: Vec<(Entity, String)> = world
        .query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>()
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, BARREL_ROOT))
        .map(|(e, o)| (e, o.path.clone()))
        .collect();
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    for (e, path) in barrels {
        let mut holder = Holder::new(
            VOLUME,
            flags::DRAINABLE,
            Owner {
                path: path.clone(),
                living: false,
            },
        );
        let preset = if is_under(&path, GUNPOWDER_BARREL) {
            Some(GUNPOWDER)
        } else if is_under(&path, THERMITE_BARREL) {
            Some(THERMITE)
        } else {
            None
        };
        if let (Some(reagent), Some(chem)) = (preset, &chem)
            && let Some(id) = chem.id(reagent)
        {
            holder.add_reagent_list(chem, &[(id, 500.0)], None, true);
        }
        world.entity_mut(e).insert((
            Barrel {
                can_open: preset.is_none(),
                ..default()
            },
            Reagents(holder),
        ));
    }
}

/// `item_interaction` of an open barrel: grown food goes in. True when the click was this.
pub fn item_interaction(world: &mut World, client: Entity, held: Entity, barrel: Entity) -> bool {
    let Some(state) = world.get::<Barrel>(barrel).copied() else {
        return false;
    };
    let is_fruit = world
        .get::<ss13_mapload::DmItem>(held)
        .is_some_and(|i| is_under(&i.path, GROWN_ROOT));
    if !state.open || !is_fruit {
        return false;
    }
    let (total, max) = world
        .get::<Reagents>(barrel)
        .map_or((0.0, VOLUME), |r| (r.0.total_volume, r.0.maximum_volume));
    if total + state.potential_volume > max {
        tell(world, client, Span::Warning, "it's full!");
        return true;
    }
    let fruit_volume = world
        .get::<Reagents>(held)
        .map_or(0.0, |r| r.0.total_volume);
    put_inside(world, held, barrel);
    if let Some(mut b) = world.get_mut::<Barrel>(barrel) {
        b.potential_volume += fruit_volume;
    }
    tell(world, client, Span::Notice, "added fruit");
    true
}

/// `attack_hand` (the lid) and `attack_hand_secondary` (emptying an open barrel). True when the
/// click was a barrel's.
pub fn attack_hand(world: &mut World, client: Entity, barrel: Entity, right: bool) -> bool {
    let Some(mut state) = world.get::<Barrel>(barrel).copied() else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(barrel) else {
        return true;
    };
    if right {
        if !state.open {
            return false;
        }
        let fruits: Vec<Entity> = world
            .get::<Stored>(barrel)
            .map(|s| s.items().to_vec())
            .unwrap_or_default();
        if fruits.is_empty() {
            tell(world, client, Span::Warning, "empty!");
            return true;
        }
        for fruit in fruits {
            take_out(world, fruit, pos);
        }
        // tg keeps `potential_volume` after emptying, so the barrel later refuses fruit as full.
        state.potential_volume = 0.0;
        world.entity_mut(barrel).insert(state);
        let name = name_of(world, barrel);
        tell(world, client, Span::Notice, format!("emptied {name}"));
        return true;
    }
    if !state.can_open {
        return false;
    }
    state.open = !state.open;
    let sound = if state.open {
        state.fermenting = false;
        LID_OPEN_SOUND
    } else {
        LID_CLOSE_SOUND
    };
    if let Some(mut r) = world.get_mut::<Reagents>(barrel) {
        if state.open {
            r.0.flags = (r.0.flags & !flags::DRAINABLE) | flags::REFILLABLE | flags::TRANSPARENT;
        } else {
            r.0.flags = (r.0.flags | flags::DRAINABLE) & !(flags::REFILLABLE | flags::TRANSPARENT);
        }
    }
    world.entity_mut(barrel).insert(state);
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, sound, SOUND_VOLUME, true),
    );
    if !state.open {
        start_fermentation(world, barrel);
    }
    let icon = if state.open { "barrel_open" } else { "barrel" };
    let id = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(STRUCTURES_DMI, icon));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(barrel)) {
        appearance.icon_state = id;
    }
    true
}

/// `start_fermentation`: closed, not full, with food inside.
fn start_fermentation(world: &mut World, barrel: Entity) {
    let full = world
        .get::<Reagents>(barrel)
        .is_some_and(|r| r.0.total_volume >= r.0.maximum_volume);
    let has_food = world
        .get::<Stored>(barrel)
        .is_some_and(|s| !s.items().is_empty());
    if let Some(mut state) = world.get_mut::<Barrel>(barrel)
        && !state.open
        && !full
        && has_food
    {
        state.fermenting = true;
    }
}

/// `process_fermentation` of every fermenting barrel each `SSobj` fire.
// Cost: one pass over the fermenting barrels (a handful at most) every 2 s.
fn process_barrels(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next || !world.contains_resource::<Botany>() {
        return;
    }
    *next = now + SSOBJ_WAIT;
    let barrels: Vec<Entity> = world
        .query::<(Entity, &Barrel)>()
        .iter(world)
        .filter(|(_, b)| b.fermenting)
        .map(|(e, _)| e)
        .collect();
    for barrel in barrels {
        ferment_next(world, barrel);
    }
}

fn ferment_next(world: &mut World, barrel: Entity) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let full = world
        .get::<Reagents>(barrel)
        .is_none_or(|r| r.0.total_volume >= r.0.maximum_volume);
    let fruit = world.get::<Stored>(barrel).and_then(|s| {
        s.items().iter().copied().find(|&f| {
            world
                .get::<ss13_mapload::DmItem>(f)
                .is_some_and(|i| is_under(&i.path, GROWN_ROOT))
        })
    });
    let open = world.get::<Barrel>(barrel).is_none_or(|b| b.open);
    let Some(fruit) = fruit.filter(|_| !full && !open) else {
        if let Some(mut b) = world.get_mut::<Barrel>(barrel) {
            b.fermenting = false;
        }
        return;
    };
    let path = world
        .get::<ss13_mapload::DmItem>(fruit)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let distill = world
        .resource::<Botany>()
        .0
        .grown_def(&path)
        .map(|d| d.distill_reagent.clone())
        .unwrap_or_default();
    let purity = world.get::<Seed>(fruit).cloned().map_or(1.0, |seed| {
        world.resource_scope(|_, mut rng: Mut<CombatRng>| seed.reagent_purity(&mut *rng))
    });
    if let Some(mut holder) = world.get::<Reagents>(fruit).map(|r| r.0.clone()) {
        ferment(&chem, &mut holder, &distill, purity);
        let volume = holder.total_volume;
        if let Some(mut b) = world.get_mut::<Barrel>(barrel) {
            b.potential_volume = (b.potential_volume - volume).max(0.0);
        }
        if let Some(mut target) = world.get_mut::<Reagents>(barrel) {
            holder.trans_to(&chem, &mut target.0, volume, 1.0, None, false, false);
        }
    }
    world.despawn(fruit);
}

/// tg `examine` of a barrel.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(state) = world.get::<Barrel>(target) else {
        return;
    };
    if state.open {
        let count = world.get::<Stored>(target).map_or(0, |s| s.items().len());
        if count > 0 {
            let s = if count == 1 { "" } else { "s" };
            lines.push((
                Span::Notice,
                format!("It contains {count} fruit{s} ready to be fermented."),
            ));
            lines.push((
                Span::Notice,
                "Right-click to take them out of it.".to_owned(),
            ));
        }
        lines.push((
            Span::Notice,
            "It is currently open, letting you fill it with fruits or reagents.".to_owned(),
        ));
    } else {
        lines.push((
            Span::Notice,
            "It is currently closed, letting it ferment fruits or draw reagents from its tap."
                .to_owned(),
        ));
    }
}
