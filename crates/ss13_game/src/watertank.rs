//! Backpack sprayers (`game/objects/items/tanks/watertank.dm`).
//!
//! A tank on the back gives its nozzle to a free hand (the button or a click on it in its slot);
//! the nozzle shares the tank's reagents (`reagent_source`) and snaps back when it leaves the hand
//! or the tank leaves the back. The water tank's, janitor's and pepper mister are sprays
//! (`crate::sprays`); the firefighter's nozzle is an extinguisher (`crate::extinguisher`) that
//! switches in hand between spraying, a resin launcher (100 u, 10 s cooldown, a capsule flying 5
//! tiles at most, foam of range 4 where it lands) and a resin foamer (one tile next to the user,
//! five charges back in 10 s each). A chemical tank on the back injects its wearer while on, 5 u
//! per 1 u used.
// shortcut: the tank's icon does not follow the nozzle mode (`waterbackpackatmos_N`), no `drag_pickup`,
// no worn fill overlay on the chemical tank.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_reagents::{DEFAULT_REAGENT_TEMPERATURE, Holder, Owner, flags};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::WorldSystems;

use crate::chem::{Chem, MobReagents, Reagents};
use crate::click_handlers::{AddClickHandler, ClickCtx, ClickPhase};
use crate::construction::{path_of, spawn_obj};
use crate::effects::foam::{FoamKind, FoamOptions, RESIN, do_foam};
use crate::inventory::{CarriedBy, Inventory};
use crate::tools::{tell, tell_mob};

pub const WATERTANK: &str = "/obj/item/watertank";
pub const CHEMTANK: &str = "/obj/item/reagent_containers/chemtank";
const RESIN_CONTAINER: &str = "/obj/effect/resin_container";
const WATER: &str = "/datum/reagent/water";
const SPACE_CLEANER: &str = "/datum/reagent/space_cleaner";
const CAPSAICIN: &str = "/datum/reagent/consumable/condensedcapsaicin";
const MISTER: &str = "/obj/item/reagent_containers/spray/mister";
const MISTER_JANITOR: &str = "/obj/item/reagent_containers/spray/mister/janitor";
const MISTER_PEPPER: &str = "/obj/item/reagent_containers/spray/mister/pepperspray";
pub const NOZZLE: &str = "/obj/item/extinguisher/mini/nozzle";
/// The launcher: `remove_all(100)`, `resin_cooldown`, the capsule's `delay` and steps.
const LAUNCH_COST: f32 = 100.0;
const LAUNCH_COOLDOWN: f64 = 10.0;
const CAPSULE_STEP_SECS: f64 = 0.2;
const CAPSULE_STEPS: u32 = 5;
/// `do_foam(4, ...)` of the capsule.
const CAPSULE_FOAM_RANGE: u32 = 4;
/// The foamer: `metal_synthesis_cooldown` limit and the time a charge takes to return.
const FOAM_CHARGES: usize = 5;
const FOAM_RECHARGE: f64 = 10.0;
/// `injection_amount`, `usage_ratio`.
const INJECTION_PER_SEC: f32 = 0.5;
const USAGE_RATIO: f32 = 5.0;

/// `volume`, `make_noz()` and what `Initialize` fills it with.
struct TankKind {
    volume: f32,
    nozzle: &'static str,
    fill: Option<&'static str>,
}

fn kind_of(path: &str) -> TankKind {
    if is_under(path, "/obj/item/watertank/janitor") {
        TankKind {
            volume: 500.0,
            nozzle: MISTER_JANITOR,
            fill: Some(SPACE_CLEANER),
        }
    } else if is_under(path, "/obj/item/watertank/pepperspray") {
        TankKind {
            volume: 1000.0,
            nozzle: MISTER_PEPPER,
            fill: Some(CAPSAICIN),
        }
    } else if is_under(path, "/obj/item/watertank/atmos") {
        TankKind {
            volume: 200.0,
            nozzle: NOZZLE,
            fill: Some(WATER),
        }
    } else {
        TankKind {
            volume: 500.0,
            nozzle: MISTER,
            fill: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NozzleMode {
    #[default]
    Extinguisher,
    ResinLauncher,
    ResinFoam,
}

/// `/obj/item/watertank`, live.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Watertank {
    pub nozzle: Option<Entity>,
    pub mode: NozzleMode,
    resin_ready_at: f64,
    /// When each spent foam charge returns.
    foam_charges: Vec<f64>,
}

/// On a nozzle: the tank it belongs to.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct NozzleOf(pub Entity);

/// `/obj/effect/resin_container` flying.
#[derive(Component, Clone, Copy, Debug)]
struct ResinCapsule {
    target: TilePos,
    steps_left: u32,
    next_at: f64,
}

/// `/obj/item/reagent_containers/chemtank`: `on`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Chemtank {
    pub on: bool,
}

pub struct WatertankPlugin;

impl Plugin for WatertankPlugin {
    fn build(&self, app: &mut App) {
        // Before the safety of `extinguisher::attack_self`.
        app.add_click_handler(ClickPhase::AttackSelf, 3267, nozzle_attack_self);
        app.add_observer(add_tank).add_systems(
            Update,
            (
                snap_back,
                reset_dropped,
                move_capsules.after(WorldSystems::Movement),
                process_chemtanks.run_if(any_with_component::<Chemtank>),
            ),
        );
    }
}

/// `make_noz` with its `reagents = loc.reagents`: whose holder a spray or nozzle uses.
pub fn reagent_source(world: &World, item: Entity) -> Entity {
    world.get::<NozzleOf>(item).map_or(item, |n| n.0)
}

fn add_tank(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let e = add.entity;
    if is_under(&item.path, WATERTANK) {
        let path = item.path.clone();
        commands.queue(move |world: &mut World| init_tank(world, e, &path));
    } else if is_under(&item.path, CHEMTANK) {
        commands.entity(e).insert(Chemtank::default());
    }
}

/// `create_reagents(volume, OPENCONTAINER)` and the fill of `Initialize`.
fn init_tank(world: &mut World, e: Entity, path: &str) {
    if world.get_entity(e).is_err() {
        return;
    }
    let kind = kind_of(path);
    let owner = Owner {
        path: path.to_owned(),
        living: false,
    };
    let mut holder = Holder::new(kind.volume, flags::OPENCONTAINER, owner);
    if let Some(fill) = kind.fill
        && let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
        && let Some(id) = chem.id(fill)
    {
        holder.add_reagent(
            &chem,
            id,
            kind.volume,
            DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            true,
            false,
        );
    }
    world
        .entity_mut(e)
        .insert((Reagents(holder), Watertank::default()));
}

/// `toggle_mister`: the nozzle out into a free hand, or back. Also the button.
pub fn toggle_mister(world: &mut World, client: Entity, mob: Entity, tank: Entity) {
    let Some(state) = world.get::<Watertank>(tank).cloned() else {
        return;
    };
    if world
        .get::<Inventory>(mob)
        .is_none_or(|inv| inv.back != Some(tank))
    {
        tell(world, client, Span::Warning, "Бак нужно надеть на спину!");
        return;
    }
    if world
        .get::<crate::status_effects::StatusEffects>(mob)
        .is_some_and(|s| s.incapacitated())
    {
        return;
    }
    if let Some(nozzle) = state.nozzle {
        retract(world, tank, nozzle);
        return;
    }
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    if !inventory.hands.contains(&None) {
        tell(
            world,
            client,
            Span::Warning,
            "Нужна свободная рука, чтобы взять распылитель!",
        );
        return;
    }
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    let path = path_of(world, tank).unwrap_or_default();
    let Some(nozzle) = spawn_obj(world, kind_of(&path).nozzle, pos) else {
        return;
    };
    world.entity_mut(nozzle).insert(NozzleOf(tank));
    crate::storage::put_in_hands(world, mob, &inventory, nozzle);
    if let Some(mut t) = world.get_mut::<Watertank>(tank) {
        t.nozzle = Some(nozzle);
    }
}

fn retract(world: &mut World, tank: Entity, nozzle: Entity) {
    world.try_despawn(nozzle).ok();
    if let Some(mut t) = world.get_mut::<Watertank>(tank) {
        t.nozzle = None;
    }
}

/// tg `watertank/attack_hand`: worn on the back, a click toggles the nozzle. True when it was
/// a tank.
pub fn attack_hand_worn(world: &mut World, client: Entity, mob: Entity, worn: Entity) -> bool {
    if world.get::<Watertank>(worn).is_none()
        || world
            .get::<Inventory>(mob)
            .is_none_or(|inv| inv.back != Some(worn))
    {
        return false;
    }
    toggle_mister(world, client, mob, worn);
    true
}

/// tg `watertank/item_interaction`: the nozzle put back on its tank. True when it was.
pub fn nozzle_on_tank(world: &mut World, held: Entity, tank: Entity) -> bool {
    if world.get::<NozzleOf>(held).is_none_or(|n| n.0 != tank) {
        return false;
    }
    retract(world, tank, held);
    true
}

/// `dropped`, `equipped`, `noz_move`: a nozzle out of the hand of the tank's wearer goes back.
// Cost: per tick, the nozzles that are out (a few).
fn snap_back(world: &mut World) {
    let nozzles: Vec<(Entity, Entity, Option<Entity>)> = world
        .query::<(Entity, &NozzleOf, Option<&CarriedBy>)>()
        .iter(world)
        .map(|(e, n, c)| (e, n.0, c.map(|c| c.0)))
        .collect();
    for (nozzle, tank, carried) in nozzles {
        let held = carried.is_some_and(|c| {
            world
                .get::<Inventory>(c)
                .is_some_and(|inv| inv.hands.contains(&Some(nozzle)) && inv.back == Some(tank))
        });
        if !held {
            retract(world, tank, nozzle);
        }
    }
}

/// `watertank/atmos/dropped`: a tank off its wearer forgets the nozzle's mode.
fn reset_dropped(mut tanks: Query<&mut Watertank, Without<CarriedBy>>) {
    for mut tank in &mut tanks {
        if tank.mode != NozzleMode::Extinguisher {
            tank.mode = NozzleMode::Extinguisher;
        }
    }
}

fn nozzle_tank(world: &World, item: Entity) -> Option<Entity> {
    world
        .get::<NozzleOf>(item)
        .map(|n| n.0)
        .filter(|&t| world.get::<Watertank>(t).is_some())
}

/// `nozzle/attack_self`: extinguisher, resin launcher, resin foam.
fn nozzle_attack_self(world: &mut World, c: &ClickCtx) -> bool {
    let Some(tank) = nozzle_tank(world, c.target) else {
        return false;
    };
    if !path_of(world, c.target).is_some_and(|p| is_under(&p, NOZZLE)) {
        return false;
    }
    let Some(mut t) = world.get_mut::<Watertank>(tank) else {
        return true;
    };
    let (mode, text) = match t.mode {
        NozzleMode::Extinguisher => (
            NozzleMode::ResinLauncher,
            "Режим: пусковая установка смолы.",
        ),
        NozzleMode::ResinLauncher => (NozzleMode::ResinFoam, "Режим: смоляная пена."),
        NozzleMode::ResinFoam => (NozzleMode::Extinguisher, "Режим: огнетушитель."),
    };
    t.mode = mode;
    tell(world, c.client, Span::Notice, text);
    true
}

/// `nozzle/ranged_interact_with_atom` with the nozzle in a resin mode: `None` when `held` is
/// not one or is spraying (the extinguisher's own code goes on), otherwise whether the click was
/// handled (false: the chain goes on to the attack).
pub fn nozzle_fire(
    world: &mut World,
    mob: Entity,
    held: Entity,
    target: Entity,
    target_pos: TilePos,
) -> Option<bool> {
    let tank = nozzle_tank(world, held)?;
    let state = world.get::<Watertank>(tank)?.clone();
    if state.mode == NozzleMode::Extinguisher {
        return None;
    }
    let from = crate::explosions::turf_of(world, mob)?;
    let adjacent = from.chebyshev(target_pos).is_some_and(|d| d <= 1);
    Some(match state.mode {
        NozzleMode::ResinLauncher => launch_resin(world, mob, tank, from, target_pos, adjacent),
        NozzleMode::ResinFoam => foam_resin(world, mob, tank, target, target_pos, adjacent),
        NozzleMode::Extinguisher => return None,
    })
}

fn launch_resin(
    world: &mut World,
    mob: Entity,
    tank: Entity,
    from: TilePos,
    target: TilePos,
    adjacent: bool,
) -> bool {
    if adjacent
        && world
            .get::<crate::combat::CombatMode>(mob)
            .is_some_and(|m| m.0)
    {
        return false;
    }
    let Some(mut holder) = world.get::<Reagents>(tank).map(|r| r.0.clone()) else {
        return true;
    };
    if holder.total_volume < LAUNCH_COST {
        tell_mob(world, mob, Span::Warning, "Не хватает воды!");
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let ready_at = world
        .get::<Watertank>(tank)
        .map_or(0.0, |t| t.resin_ready_at);
    if now < ready_at {
        tell_mob(world, mob, Span::Warning, "Ещё заряжается!");
        return true;
    }
    if let Some(mut t) = world.get_mut::<Watertank>(tank) {
        t.resin_ready_at = now + LAUNCH_COOLDOWN;
    }
    holder.remove_all(LAUNCH_COST);
    world.entity_mut(tank).insert(Reagents(holder));
    if let Some(capsule) = spawn_obj(world, RESIN_CONTAINER, from) {
        world.entity_mut(capsule).insert(ResinCapsule {
            target,
            steps_left: CAPSULE_STEPS,
            next_at: now,
        });
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(from, "sound/items/syringeproj.ogg", 40.0, true),
    );
    true
}

fn foam_resin(
    world: &mut World,
    mob: Entity,
    tank: Entity,
    target: Entity,
    target_pos: TilePos,
    adjacent: bool,
) -> bool {
    if world.get::<ss13_mapload::Turf>(target).is_none() {
        return false;
    }
    if !adjacent {
        tell_mob(world, mob, Span::Warning, "Слишком далеко!");
        return true;
    }
    let covered = crate::effects::on_tile(world, target_pos)
        .into_iter()
        .any(|e| {
            world
                .get::<crate::effects::foam::Foam>(e)
                .is_some_and(|f| matches!(f.kind, FoamKind::Resin | FoamKind::ResinHalon))
                || path_of(world, e).is_some_and(|p| is_under(&p, RESIN))
        });
    if covered {
        tell_mob(world, mob, Span::Warning, "Смола тут уже есть!");
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut state) = world.get_mut::<Watertank>(tank) else {
        return true;
    };
    state.foam_charges.retain(|&back_at| back_at > now);
    if state.foam_charges.len() >= FOAM_CHARGES {
        tell_mob(world, mob, Span::Warning, "Смола ещё синтезируется!");
        return true;
    }
    state.foam_charges.push(now + FOAM_RECHARGE);
    do_foam(
        world,
        0,
        target_pos,
        Some(0),
        None,
        FoamKind::Resin,
        FoamOptions::default(),
    );
    true
}

/// The capsule's flight (`resin_stop_check`, `resin_landed`).
fn move_capsules(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let capsules: Vec<(Entity, ResinCapsule, TilePos)> = world
        .query::<(Entity, &ResinCapsule, &TilePos)>()
        .iter(world)
        .map(|(e, c, pos)| (e, *c, *pos))
        .collect();
    for (e, mut capsule, pos) in capsules {
        if now < capsule.next_at {
            continue;
        }
        let dir = crate::pull::dir_between(pos, capsule.target).filter(|_| capsule.steps_left > 0);
        let moved =
            dir.is_some_and(|dir| crate::pull::step_passing(world, e, dir, is_table).is_ok());
        if moved {
            capsule.steps_left -= 1;
            capsule.next_at = now + CAPSULE_STEP_SECS;
            world.entity_mut(e).insert(capsule);
            continue;
        }
        land(world, e, pos);
    }
}

/// `pass_flags = PASSTABLE`.
fn is_table(world: &World, blocker: Entity) -> bool {
    world
        .get::<ss13_mapload::DmObj>(blocker)
        .is_some_and(|o| is_under(&o.path, "/obj/structure/table"))
}

/// `resin_container/Smoke`.
fn land(world: &mut World, capsule: Entity, pos: TilePos) {
    do_foam(
        world,
        CAPSULE_FOAM_RANGE,
        pos,
        None,
        None,
        FoamKind::Resin,
        FoamOptions::default(),
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/bamf.ogg", 100.0, true),
    );
    world.despawn(capsule);
}

/// `toggle_injection`. Also the button.
pub fn toggle_injector(world: &mut World, client: Entity, mob: Entity, tank: Entity) {
    if world.get::<Chemtank>(tank).is_none() {
        return;
    }
    if world
        .get::<Inventory>(mob)
        .is_none_or(|inv| inv.back != Some(tank))
    {
        tell(
            world,
            client,
            Span::Warning,
            "Химический бак нужно надеть на спину, прежде чем включать!",
        );
        return;
    }
    let on = !world.get::<Chemtank>(tank).is_some_and(|c| c.on);
    world.get_mut::<Chemtank>(tank).expect("read").on = on;
    let text = if on {
        "Инжектор включается."
    } else {
        "Инжектор выключается."
    };
    tell(world, client, Span::Notice, text);
}

/// `chemtank/process` once per second: `trans_to(user, used_amount, usage_ratio, INJECT)`.
fn process_chemtanks(world: &mut World, mut last: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *last + 1.0 {
        return;
    }
    *last = now;
    let tanks: Vec<Entity> = world
        .query::<(Entity, &Chemtank)>()
        .iter(world)
        .filter(|(_, c)| c.on)
        .map(|(e, _)| e)
        .collect();
    for tank in tanks {
        let wearer = world.get::<CarriedBy>(tank).map(|c| c.0).filter(|&m| {
            world
                .get::<Inventory>(m)
                .is_some_and(|inv| inv.back == Some(tank))
        });
        let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
        let holder = world.get::<Reagents>(tank).map(|r| r.0.clone());
        let (Some(mob), Some(chem), Some(mut holder)) = (wearer, chem, holder) else {
            turn_off(world, tank, None);
            continue;
        };
        if holder.total_volume <= 0.0 || world.get::<MobReagents>(mob).is_none() {
            turn_off(world, tank, Some(mob));
            continue;
        }
        let used = INJECTION_PER_SEC / USAGE_RATIO;
        if let Some(mut m) = world.get_mut::<MobReagents>(mob) {
            holder.trans_to(&chem, &mut m.blood, used, USAGE_RATIO, None, false, false);
        }
        world.entity_mut(tank).insert(Reagents(holder));
    }
}

fn turn_off(world: &mut World, tank: Entity, mob: Option<Entity>) {
    if let Some(mut c) = world.get_mut::<Chemtank>(tank) {
        c.on = false;
    }
    if let Some(mob) = mob {
        tell_mob(world, mob, Span::Notice, "Инжектор выключается.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_tank_has_its_nozzle_and_volume() {
        assert_eq!(kind_of("/obj/item/watertank").nozzle, MISTER);
        assert_eq!(
            kind_of("/obj/item/watertank/janitor").fill,
            Some(SPACE_CLEANER)
        );
        assert_eq!(kind_of("/obj/item/watertank/pepperspray").volume, 1000.0);
        assert_eq!(kind_of("/obj/item/watertank/atmos").nozzle, NOZZLE);
    }
}
