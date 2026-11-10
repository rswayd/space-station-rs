//! Fire extinguishers (`/obj/item/extinguisher`, `game/objects/items/tools/extinguisher.dm`,
//! the spray `/obj/effect/particle_effect/water/extinguisher`).
//!
//! In hand the safety toggles. With it off, a click (adjacent or far) spends 5 units on five
//! particles flying from the user to random tiles of a row across the target (`precision`: one
//! each on five tiles), `power` tiles at most, one step per 0.2 s; every tile they cross and
//! everything on it is exposed to the particle's unit (water and foam put out fires and burning
//! mobs), what stops them too. Spraying pushes a user drifting in space (`newtonian_move`).
//! A click on a tank of its `tanktypes` refills it; Alt-click in hand empties it on the floor.
// shortcut: no sounds, no wall-mounted `tank_holder` sprites, no firebot assembly here, no wind-up
// head bash (`attack_secondary`), no office chair ride (`move_chair`), no cooling of containers
// a particle bumps, no suicide; particles do not leave foam (`firefighting` foam is not ported).
// Backpack nozzles (`crate::watertank`) are extinguishers using their tank's holder.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::engmisc::EXTINGUISHER_WATER;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmExtinguisher, DmItem, Turf};
use ss13_reagents::{DEFAULT_REAGENT_TEMPERATURE, Holder, Owner, flags};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, TilePos};
use ss13_world::{SpatialIndex, WorldSystems};

use crate::chem::{Chem, Reagents};
use crate::click::{RangedClick, ResolveClicks};
use crate::click_handlers::{AddClickHandler, ClickCtx, ClickPhase};
use crate::combat::{CombatMode, CombatRng};
use crate::construction::{path_of, spawn_obj, turf_at};
use crate::disposals::pipes;
use crate::inventory::Inventory;
use crate::tools::{tell, tell_mob};

/// `world.time < last_use + 12`.
const USE_COOLDOWN_SECS: f64 = 1.2;
/// `move_particles`: `delay = 2`.
const STEP_SECS: f64 = 0.2;
const PARTICLES: usize = 5;
const UNIT: f32 = 1.0;

/// `/obj/item/extinguisher`, live.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Extinguisher {
    pub safety: bool,
    last_use: f64,
    pub power: u32,
    pub precision: bool,
    pub cooling_power: f32,
    pub max_water: f32,
    /// `chem`.
    pub chem: String,
    pub sprite_name: String,
    pub tanktypes: Vec<String>,
}

/// `/obj/effect/particle_effect/water/extinguisher` on its way.
#[derive(Component, Clone, Debug)]
pub struct Spray {
    holder: Holder,
    target: TilePos,
    steps_left: u32,
    next_at: f64,
    cooling_power: f32,
}

pub struct ExtinguisherPlugin;

impl Plugin for ExtinguisherPlugin {
    fn build(&self, app: &mut App) {
        app.add_click_handler(ClickPhase::AttackSelf, 3275, attack_self);
        // A tank refills it before the chemistry code treats it as a container.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1505, refill_click);
        // After the targets' own handlers: a table or bag takes it, a cabinet stores it.
        app.add_click_handler(ClickPhase::HeldOnTarget, 4745, spray_click);
        app.add_systems(
            Update,
            (
                init_new,
                ranged_clicks.after(ResolveClicks),
                move_sprays.after(WorldSystems::Movement),
            ),
        );
    }
}

fn init_new(
    mut commands: Commands,
    added: Query<(Entity, &DmExtinguisher), Added<DmExtinguisher>>,
) {
    for (e, dm) in &added {
        let dm = dm.clone();
        commands.queue(move |world: &mut World| add_state(world, e, &dm));
    }
}

fn add_state(world: &mut World, e: Entity, dm: &DmExtinguisher) {
    if world.get_entity(e).is_err() {
        return;
    }
    let owner = Owner {
        path: path_of(world, e).unwrap_or_default(),
        living: false,
    };
    let mut holder = Holder::new(dm.max_water, flags::AMOUNT_VISIBLE, owner);
    if dm.starting_water
        && let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
        && let Some(id) = chem.id(&dm.chem)
    {
        holder.add_reagent(
            &chem,
            id,
            dm.max_water,
            DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            true,
            false,
        );
    }
    let nozzle = is_under(
        &path_of(world, e).unwrap_or_default(),
        crate::watertank::NOZZLE,
    );
    world.entity_mut(e).insert((
        Reagents(holder),
        Extinguisher {
            safety: !nozzle,
            last_use: f64::NEG_INFINITY,
            power: dm.power,
            precision: dm.precision,
            cooling_power: dm.cooling_power,
            max_water: dm.max_water,
            chem: dm.chem.clone(),
            sprite_name: dm.sprite_name.clone(),
            tanktypes: dm.tanktypes.clone(),
        },
    ));
}

fn attack_self(world: &mut World, c: &ClickCtx) -> bool {
    let Some(mut ext) = world.get_mut::<Extinguisher>(c.target) else {
        return false;
    };
    ext.safety = !ext.safety;
    let safety = ext.safety;
    let state = format!("{}{}", ext.sprite_name, u8::from(!safety));
    let id = world.get::<DmAtom>(c.target).and_then(|a| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&a.icon, &state))
    });
    if let Some(id) = id
        && let Some(mut look) = world.get_mut::<Appearance>(c.target)
    {
        look.icon_state = id;
    }
    let text = if safety {
        "Предохранитель включён."
    } else {
        "Предохранитель снят."
    };
    tell(world, c.client, Span::Notice, text);
    true
}

/// `AttemptRefill`. True when `target` is one of its tanks.
fn refill_click(world: &mut World, c: &ClickCtx) -> bool {
    let Some(held) = c.held else {
        return false;
    };
    let Some(ext) = world.get::<Extinguisher>(held).cloned() else {
        return false;
    };
    let path = path_of(world, c.target).unwrap_or_default();
    if !ext.tanktypes.iter().any(|t| is_under(&path, t)) {
        return false;
    }
    let source = crate::watertank::reagent_source(world, held);
    let (Some(chem), Some(mut own), Some(mut tank)) = (
        world.get_resource::<Chem>().map(|r| r.0.clone()),
        world.get::<Reagents>(source).map(|r| r.0.clone()),
        world.get::<Reagents>(c.target).map(|r| r.0.clone()),
    ) else {
        return true;
    };
    if own.total_volume >= own.maximum_volume {
        tell(world, c.client, Span::Warning, "Уже полон!");
        return true;
    }
    // A nozzle's `chem` is null: it takes what is in its tank.
    if !ext.chem.is_empty() && !chem.id(&ext.chem).is_some_and(|id| tank.has(id).is_some()) {
        tell(
            world,
            c.client,
            Span::Warning,
            "Этой жидкостью заправить нельзя!",
        );
        return true;
    }
    let moved = tank.trans_to(&chem, &mut own, ext.max_water, 1.0, None, false, false);
    world.entity_mut(source).insert(Reagents(own));
    world.entity_mut(c.target).insert(Reagents(tank));
    if moved > 0.0 {
        tell(
            world,
            c.client,
            Span::Notice,
            format!("Заправлено {} ед.", crate::chem::dm_num(moved)),
        );
    } else {
        tell(world, c.client, Span::Warning, "Бак пуст!");
    }
    true
}

/// `interact_with_atom`: the safety on leaves the click to the rest of the chain, and in combat
/// mode the item also swings after spraying (`ITEM_INTERACT_SKIP_TO_ATTACK`).
fn spray_click(world: &mut World, c: &ClickCtx) -> bool {
    let Some(held) = c.held else {
        return false;
    };
    if !world.get::<Extinguisher>(held).is_some_and(|e| !e.safety) {
        return false;
    }
    if let Some(handled) = crate::watertank::nozzle_fire(world, c.mob, held, c.target, c.pos) {
        return handled;
    }
    spray(world, c.mob, held, c.pos);
    !world.get::<CombatMode>(c.mob).is_some_and(|m| m.0)
}

/// `ranged_interact_with_atom`.
fn ranged_clicks(world: &mut World, mut cursor: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = cursor
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let held = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&h| world.get::<Extinguisher>(h).is_some_and(|e| !e.safety));
        if let Some(held) = held
            && crate::watertank::nozzle_fire(world, click.mob, held, click.target, click.target_pos)
                .is_none()
        {
            spray(world, click.mob, held, click.target_pos);
        }
    }
}

/// `the_targets`: the clicked tile, one to each side, two to each side with `precision`.
pub fn spread_targets(target: TilePos, direction: Option<Dir>, precision: bool) -> Vec<TilePos> {
    let mut out = vec![target];
    let Some(direction) = direction else {
        return out;
    };
    let side = |angle: i32| Dir::from_bits(pipes::turn(direction as u8, angle));
    let (Some(left), Some(right)) = (side(90), side(-90)) else {
        return out;
    };
    out.push(target.step(left));
    out.push(target.step(right));
    if precision {
        out.push(target.step(left).step(left));
        out.push(target.step(right).step(right));
    }
    out
}

/// The body of `ranged_interact_with_atom` with the safety off.
fn spray(world: &mut World, mob: Entity, item: Entity, target: TilePos) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let source = crate::watertank::reagent_source(world, item);
    let (Some(ext), Some(mut holder), Some(&from), Some(chem)) = (
        world.get::<Extinguisher>(item).cloned(),
        world.get::<Reagents>(source).map(|r| r.0.clone()),
        world.get::<TilePos>(mob),
        world.get_resource::<Chem>().map(|c| c.0.clone()),
    ) else {
        return;
    };
    if holder.total_volume < UNIT {
        tell_mob(world, mob, Span::Warning, "Пусто!");
        return;
    }
    if now < ext.last_use + USE_COOLDOWN_SECS {
        return;
    }
    world.get_mut::<Extinguisher>(item).expect("read").last_use = now;
    let direction = crate::pull::dir_between(from, target);
    if let Some(d) = direction
        && world.get::<crate::medical::surgery::Buckled>(mob).is_none()
    {
        crate::spacemove::newtonian_move(world, mob, d.reverse(), false, 1.0);
    }
    let mut targets = spread_targets(target, direction, ext.precision);
    for _ in 0..PARTICLES {
        if targets.is_empty() {
            break;
        }
        let index = world
            .get_resource_or_init::<CombatRng>()
            .range(0, targets.len() as u32 - 1) as usize;
        let to = if ext.precision {
            targets.remove(index)
        } else {
            targets[index]
        };
        let mut unit = Holder::new(5.0, 0, Owner::default());
        holder.trans_to(&chem, &mut unit, UNIT, 1.0, None, true, false);
        let Some(particle) = spawn_obj(world, EXTINGUISHER_WATER, from) else {
            continue;
        };
        world.entity_mut(particle).insert(Spray {
            holder: unit,
            target: to,
            steps_left: ext.power,
            next_at: now,
            cooling_power: ext.cooling_power,
        });
    }
    world.entity_mut(source).insert(Reagents(holder));
}

/// `PASSTABLE | PASSMACHINE | PASSSTRUCTURE | PASSGRILLE | PASSBLOB | PASSVEHICLE`: not windows
/// (`PASSGLASS`), not doors (`PASSDOORS`), not mobs.
fn passes(world: &World, blocker: Entity) -> bool {
    let Some(path) = world.get::<ss13_mapload::DmObj>(blocker).map(|o| &o.path) else {
        return false;
    };
    (is_under(path, "/obj/structure") && !is_under(path, "/obj/structure/window"))
        || (is_under(path, "/obj/machinery") && !is_under(path, "/obj/machinery/door"))
}

/// `/obj/effect/particle_effect/water/extinguisher/Move`.
fn expose_tile(world: &mut World, particle: Entity, pos: TilePos, spray: &Spray) {
    let cool = spray.cooling_power;
    if let Some(turf) = turf_at(world, pos) {
        crate::janitor::expose_cooled(world, turf, &spray.holder, 1.0, cool);
    }
    let on_tile: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
    for thing in on_tile {
        if thing != particle && world.get::<Turf>(thing).is_none() {
            crate::janitor::expose_cooled(world, thing, &spray.holder, 1.0, cool);
        }
    }
}

fn move_sprays(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query_filtered::<Entity, With<Spray>>();
    let mut list: Vec<Entity> = q.iter(world).collect();
    list.sort();
    for particle in list {
        let (Some(spray), Some(&pos)) = (
            world.get::<Spray>(particle).cloned(),
            world.get::<TilePos>(particle),
        ) else {
            continue;
        };
        if now < spray.next_at {
            continue;
        }
        let Some(dir) =
            crate::pull::dir_between(pos, spray.target).filter(|_| spray.steps_left > 0)
        else {
            world.despawn(particle);
            continue;
        };
        match crate::pull::step_passing(world, particle, dir, passes) {
            Ok(()) => {
                let at = pos.step(dir);
                expose_tile(world, particle, at, &spray);
                let mut s = world.get_mut::<Spray>(particle).expect("read");
                s.steps_left -= 1;
                s.next_at = now + STEP_SECS;
            }
            Err(blockers) => {
                // `Bump`, then the loop ends.
                for blocker in blockers {
                    crate::janitor::expose_cooled(
                        world,
                        blocker,
                        &spray.holder,
                        1.0,
                        spray.cooling_power,
                    );
                }
                world.despawn(particle);
            }
        }
    }
}

/// `click_alt`, `EmptyExtinguisher`. True when `item` is an extinguisher.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<Extinguisher>(item).is_none() {
        return false;
    }
    let holding = world
        .get::<Inventory>(mob)
        .is_some_and(|inv| inv.hands.iter().flatten().any(|&h| h == item));
    if !holding {
        tell(world, client, Span::Notice, "Нужно держать его в руках!");
        return true;
    }
    let source = crate::watertank::reagent_source(world, item);
    let Some(mut holder) = world.get::<Reagents>(source).map(|r| r.0.clone()) else {
        return true;
    };
    if holder.total_volume <= 0.0 {
        return true;
    }
    if let Some(turf) = world
        .get::<TilePos>(mob)
        .copied()
        .and_then(|p| turf_at(world, p))
    {
        crate::janitor::expose(world, turf, &holder, 1.0);
    }
    holder.clear();
    world.entity_mut(source).insert(Reagents(holder));
    tell(
        world,
        client,
        Span::Info,
        "Вы тихо опорожняете огнетушитель через клапан.",
    );
    true
}

/// `examine`.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(ext) = world.get::<Extinguisher>(item) else {
        return;
    };
    let state = if ext.safety {
        "включён"
    } else {
        "снят"
    };
    lines.push((Span::Notice, format!("Предохранитель {state}.")));
    if world
        .get::<Reagents>(item)
        .is_some_and(|r| r.0.total_volume > 0.0)
        && world.get::<DmItem>(item).is_some()
    {
        lines.push((Span::Notice, "Alt-клик, чтобы опорожнить.".to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spray_covers_a_row_across_its_direction() {
        let t = TilePos::new(5, 8, 1);
        assert_eq!(
            spread_targets(t, Some(Dir::North), false),
            vec![t, TilePos::new(4, 8, 1), TilePos::new(6, 8, 1)]
        );
        let wide = spread_targets(t, Some(Dir::North), true);
        assert_eq!(wide.len(), 5);
        assert!(wide.contains(&TilePos::new(3, 8, 1)));
        assert!(wide.contains(&TilePos::new(7, 8, 1)));
        assert_eq!(spread_targets(t, None, true), vec![t], "on one's own tile");
    }
}
