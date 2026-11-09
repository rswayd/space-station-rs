//! Gas jetpacks (server), after tg `/obj/item/tank/jetpack`
//! (`code/game/objects/items/tanks/jetpack.dm`) and `/datum/component/jetpack`.
//!
//! An `/obj/item/tank/jetpack` is a gas tank ([`crate::internals`]) with three item actions while
//! worn in its slot (`crate::clothing::gear`): on/off, stabilization and internals. On, it files
//! [`GasJetpack`] on its wearer and `crate::spacemove` asks [`holds`], [`always_drifts`], [`ready`]
//! and [`inertia_multiplier`] as it does for MOD jetpacks; a floating mob with `full_speed` moves
//! faster ([`speed_modifier`]). Thrust spends gas (`allow_thrust`), which goes into the tile.
//! An EMP switches it off and keeps it off for 4 s.
// shortcut: one ion trail component per mob (an active MOD jetpack and a gas one share it);
// button icons stay the item's sprite; the improvised jetpack's crafting recipe is not here.

use bevy::prelude::*;
use ss13_atmos::GasMixture;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem};
use ss13_shared::Dir;
use ss13_shared::chat::Span;

use crate::combat::CombatRng;
use crate::effects::trail::{Trail, TrailKind};
use crate::internals::TankAir;
use crate::inventory::{CarriedBy, Inventory, slot_bit};
use crate::tools::{tell, tell_mob};

pub const JETPACK: &str = "/obj/item/tank/jetpack";
const IMPROVISED: &str = "/obj/item/tank/jetpack/improvised";
const CAPTAIN: &str = "/obj/item/tank/jetpack/captain";
/// `drift_force`, newtons.
const DRIFT_FORCE: f32 = 1.5;
const IMPROVISED_DRIFT_FORCE: f32 = 1.0;
const CAPTAIN_DRIFT_FORCE: f32 = 2.0;
/// `CALLBACK(src, PROC_REF(allow_thrust), 0.01)`: moles a thrust takes.
const THRUST_MOLES: f64 = 0.01;
/// `allow_thrust`: below this, nothing comes out.
const THRUST_MIN_REMOVED: f64 = 0.005;
/// `/datum/movespeed_modifier/jetpack/full_speed`, deciseconds per tile.
const FULL_SPEED_SLOWDOWN: f32 = -0.3;
/// `emp_act`.
const EMP_DISABLE_SECS: f64 = 4.0;

/// `on`, `stabilize` and `disabled` of a jetpack item.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct JetpackState {
    pub on: bool,
    pub stabilize: bool,
    pub disabled_until: f64,
}

/// On a mob with an active gas jetpack.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GasJetpack {
    pub item: Entity,
    pub stabilize: bool,
    pub drift_force: f32,
    pub full_speed: bool,
}

pub struct JetpackPlugin;

impl Plugin for JetpackPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                watch_wearers.run_if(any_with_component::<GasJetpack>),
                jetpack_emps.in_set(crate::emp::EmpSystems::Effects),
            ),
        );
    }
}

fn path_of(world: &World, item: Entity) -> Option<String> {
    world.get::<DmItem>(item).map(|i| i.path.clone())
}

fn drift_force(path: &str) -> f32 {
    if is_under(path, IMPROVISED) {
        IMPROVISED_DRIFT_FORCE
    } else if is_under(path, CAPTAIN) {
        CAPTAIN_DRIFT_FORCE
    } else {
        DRIFT_FORCE
    }
}

fn state_of(world: &World, item: Entity) -> JetpackState {
    world.get::<JetpackState>(item).copied().unwrap_or_default()
}

/// `update_icon_state`: `"[initial(icon_state)][on ? "-on" : ""]"`. The improvised jetpack has a
/// fixed `worn_icon_state`.
fn update_sprite(world: &mut World, item: Entity, on: bool) {
    let Some(base) = world.get::<DmAtom>(item).map(|a| a.icon_state.clone()) else {
        return;
    };
    let improvised = path_of(world, item).is_some_and(|p| is_under(&p, IMPROVISED));
    let state = format!("{base}{}", if on { "-on" } else { "" });
    let worn = if improvised {
        world
            .get::<DmItem>(item)
            .map(|i| i.worn_icon_state.clone())
            .unwrap_or_default()
    } else {
        state.clone()
    };
    crate::clothing::toggles::set_states(world, item, &state, worn);
}

/// `allow_thrust(num, use_fuel)`: whether the jetpack on a mob can give a thrust, spending the
/// gas with `use_fuel`; a jetpack that cannot switches off.
fn allow_thrust(world: &mut World, item: Entity, use_fuel: bool) -> bool {
    let Some(holder) = world.get::<CarriedBy>(item).map(|c| c.0) else {
        return false;
    };
    if path_of(world, item).is_some_and(|p| is_under(&p, IMPROVISED))
        && world.resource_mut::<CombatRng>().range(0, 250) == 0
    {
        tell_mob(
            world,
            holder,
            Span::Notice,
            "Вы чувствуете, как двигатели джетпака глохнут.",
        );
        turn_off(world, holder, item);
        return false;
    }
    let has_gas = world
        .get::<TankAir>(item)
        .is_some_and(|t| t.air.total_moles() >= THRUST_MOLES);
    if !has_gas {
        turn_off(world, holder, item);
        return false;
    }
    if !use_fuel {
        return true;
    }
    let removed: Option<GasMixture> = world
        .get_mut::<TankAir>(item)
        .and_then(|mut t| t.air.remove(THRUST_MOLES));
    let Some(removed) = removed.filter(|r| r.total_moles() >= THRUST_MIN_REMOVED) else {
        turn_off(world, holder, item);
        return false;
    };
    if let Some(pos) = crate::modpc::pda::turf_of(world, item)
        && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>()
    {
        atmos.merge_air(pos, &removed);
    }
    true
}

/// tg `turn_on`.
fn turn_on(world: &mut World, mob: Entity, item: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let state = state_of(world, item);
    if now < state.disabled_until || !allow_thrust(world, item, true) {
        return false;
    }
    let path = path_of(world, item).unwrap_or_default();
    let on = JetpackState { on: true, ..state };
    world.entity_mut(item).insert(on);
    update_sprite(world, item, true);
    world.entity_mut(mob).insert((
        GasJetpack {
            item,
            stabilize: state.stabilize,
            drift_force: drift_force(&path),
            full_speed: !is_under(&path, IMPROVISED),
        },
        Trail::new(TrailKind::Ion),
    ));
    true
}

/// tg `turn_off`.
pub fn turn_off(world: &mut World, mob: Entity, item: Entity) {
    let state = state_of(world, item);
    if !state.on {
        return;
    }
    world
        .entity_mut(item)
        .insert(JetpackState { on: false, ..state });
    update_sprite(world, item, false);
    if world.get::<GasJetpack>(mob).is_some_and(|j| j.item == item) {
        world.entity_mut(mob).remove::<(GasJetpack, Trail)>();
    }
}

/// tg `cycle`: the jetpack action.
pub fn cycle(world: &mut World, client: Entity, mob: Entity, item: Entity) {
    if state_of(world, item).on {
        turn_off(world, mob, item);
        tell(world, client, Span::Notice, "Вы выключаете джетпак.");
    } else if turn_on(world, mob, item) {
        tell(world, client, Span::Notice, "Вы включаете джетпак.");
    } else {
        tell(
            world,
            client,
            Span::Notice,
            "Вам не удаётся включить джетпак.",
        );
    }
}

/// tg `configure_jetpack(!stabilize, user)`: only while on.
pub fn toggle_stabilization(world: &mut World, client: Entity, mob: Entity, item: Entity) {
    let state = state_of(world, item);
    if !state.on {
        return;
    }
    let stabilize = !state.stabilize;
    world
        .entity_mut(item)
        .insert(JetpackState { stabilize, ..state });
    if let Some(mut jet) = world.get_mut::<GasJetpack>(mob) {
        jet.stabilize = stabilize;
    }
    let text = if stabilize {
        "Вы включаете стабилизацию джетпака."
    } else {
        "Вы выключаете стабилизацию джетпака."
    };
    tell(world, client, Span::Notice, text);
}

/// `jetpack/stabilize` with `check_on_move.Invoke(FALSE)`: holds a weightless wearer when
/// stabilized or on its own step, which pays (`move_react`).
pub fn holds(world: &mut World, mob: Entity, dir: Option<Dir>, continuous: bool) -> bool {
    let Some(jet) = world.get::<GasJetpack>(mob).copied() else {
        return false;
    };
    let own_step = !continuous && dir.is_some();
    if !(jet.stabilize || own_step) || !allow_thrust(world, jet.item, false) {
        return false;
    }
    if own_step {
        allow_thrust(world, jet.item, true);
    }
    true
}

/// Whether a weightless `mob` can thrust at all (its HUD does not say it is stuck).
pub fn ready(world: &World, mob: Entity) -> bool {
    world.get::<GasJetpack>(mob).is_some_and(|j| {
        world
            .get::<TankAir>(j.item)
            .is_some_and(|t| t.air.total_moles() >= THRUST_MOLES)
    })
}

/// `TRAIT_NOGRAV_ALWAYS_DRIFT`: an active jetpack without stabilization.
pub fn always_drifts(world: &World, mob: Entity) -> bool {
    world.get::<GasJetpack>(mob).is_some_and(|j| !j.stabilize)
}

/// `inertia_move_multiplier_active /= drift_force`.
pub fn inertia_multiplier(world: &World, e: Entity) -> f64 {
    world
        .get::<GasJetpack>(e)
        .map_or(1.0, |j| 1.0 / f64::from(j.drift_force.max(1.0)))
}

/// `full_speed`: added to the equipment slowdown of a floating wearer
/// (`movespeed_modifier/jetpack/full_speed`, `movetypes = FLOATING`).
pub fn speed_modifier(jet: Option<&GasJetpack>) -> f32 {
    if jet.is_some_and(|j| j.full_speed) {
        FULL_SPEED_SLOWDOWN
    } else {
        0.0
    }
}

/// tg `equipped`, `dropped`: a jetpack that is no longer in its slot switches off.
// Cost: per tick, the mobs with an active jetpack.
fn watch_wearers(world: &mut World, mut mobs: Local<QueryState<(Entity, &'static GasJetpack)>>) {
    let jets: Vec<(Entity, Entity)> = mobs.iter(world).map(|(m, j)| (m, j.item)).collect();
    for (mob, item) in jets {
        let in_slot = world.get::<Inventory>(mob).is_some_and(|inv| {
            inv.slot_of(item).is_some_and(|slot| {
                world
                    .get::<DmItem>(item)
                    .is_some_and(|dm| dm.slot_flags & slot_bit(slot) != 0)
            })
        });
        if !in_slot {
            turn_off(world, mob, item);
            if world.get::<GasJetpack>(mob).is_some_and(|j| j.item == item) {
                world.entity_mut(mob).remove::<(GasJetpack, Trail)>();
            }
        }
    }
}

fn jetpack_emps(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::emp::EmpAct>>,
) {
    for ev in crate::emp::emp_acts(world, &mut cursor) {
        if !ev.hits_self() || !path_of(world, ev.target).is_some_and(|p| is_under(&p, JETPACK)) {
            continue;
        }
        if let Some(holder) = world.get::<CarriedBy>(ev.target).map(|c| c.0) {
            turn_off(world, holder, ev.target);
        }
        let now = world.resource::<Time>().elapsed_secs_f64();
        let state = state_of(world, ev.target);
        world.entity_mut(ev.target).insert(JetpackState {
            on: false,
            disabled_until: now + EMP_DISABLE_SECS,
            ..state
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internals::TankAir;
    use ss13_atmos::GasId;

    fn setup(moles: f64, improvised: bool) -> (World, Entity, Entity, Entity) {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.insert_resource(CombatRng(1));
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let mut air = GasMixture::vacuum();
        air.moles[usize::from(GasId::O2.0)] = moles;
        air.temperature = 293.15;
        let path = if improvised { IMPROVISED } else { JETPACK };
        let item = world
            .spawn((
                DmItem {
                    path: path.to_owned(),
                    slot_flags: slot_bit(ss13_shared::protocol::InventorySlot::Back),
                    ..default()
                },
                DmAtom {
                    icon_state: "jetpack".into(),
                    ..default()
                },
                TankAir {
                    air,
                    volume: 70.0,
                    distribute_pressure: 16.0,
                },
            ))
            .id();
        let mob = world
            .spawn(Inventory {
                back: Some(item),
                ..default()
            })
            .id();
        world.entity_mut(item).insert(CarriedBy(mob));
        let client = world.spawn_empty().id();
        (world, client, mob, item)
    }

    #[test]
    fn thrust_spends_gas_and_stabilizes() {
        let (mut world, client, mob, item) = setup(1.0, false);
        cycle(&mut world, client, mob, item);
        let jet = *world.get::<GasJetpack>(mob).expect("on");
        assert_eq!((jet.stabilize, jet.drift_force), (false, 1.5));
        let moles = |w: &World| w.get::<TankAir>(item).unwrap().air.total_moles();
        let start = moles(&world);
        assert!(always_drifts(&world, mob), "no stabilization: it drifts");
        assert!(ready(&world, mob));
        assert!(holds(&mut world, mob, Some(Dir::East), false), "own step");
        assert!(moles(&world) < start, "a step pays gas");
        assert!(!holds(&mut world, mob, None, true), "continuous: drifts");
        toggle_stabilization(&mut world, client, mob, item);
        assert!(world.get::<GasJetpack>(mob).unwrap().stabilize);
        assert!(holds(&mut world, mob, None, true), "stabilized: holds");
        assert!(!always_drifts(&world, mob));
        cycle(&mut world, client, mob, item);
        assert!(world.get::<GasJetpack>(mob).is_none(), "off");
    }

    #[test]
    fn an_empty_jetpack_will_not_start() {
        let (mut world, client, mob, item) = setup(0.0, false);
        cycle(&mut world, client, mob, item);
        assert!(world.get::<GasJetpack>(mob).is_none());
        assert!(!state_of(&world, item).on);
    }

    #[test]
    fn improvised_jetpack_is_slower_and_has_no_speed_boost() {
        assert_eq!(drift_force(IMPROVISED), IMPROVISED_DRIFT_FORCE);
        assert_eq!(drift_force(CAPTAIN), CAPTAIN_DRIFT_FORCE);
        assert_eq!(speed_modifier(None), 0.0);
    }
}
