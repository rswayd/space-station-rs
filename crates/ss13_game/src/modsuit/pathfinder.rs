//! The MOD pathfinder module (server), after tg `code/modules/mod/modules/module_pathfinder.dm`:
//!
//! - The module holds one pathfinder implant (`/obj/item/implant/mod`,
//!   `crate::implants::ImplantKind::ModPathfinder`). Used while the suit is worn, it implants the
//!   wearer; the module in hand on a human implants them after 1.5 s.
//! - The implant's "Recall MOD" button (5 s cooldown) calls the suit to its host: not while the
//!   cover is open, the suit is in transit or someone (the host too) carries it. After 2 s the
//!   suit leaves its spot (a closet it is in opens, or is busted open), flies for the pod's
//!   transit time and lands on the host's tile as it was at launch. A host next to it then wears
//!   it on the back (whatever was there drops) and it deploys its parts.
//! - With the implant used and no wearer, using the module recalls the suit as well.
// shortcut: no supply pod (the suit vanishes and reappears; no pod sprite, jet overlay or shake),
// no suit storage unit release, no examine lines of the module (implant in or missing).

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;

use super::ModJob;
use super::modules::{Kind, def_of, drain_power};
use super::server::{InSuit, ModSuit, quick_deploy, tell_mob};
use crate::construction::Job;
use crate::health::Health;
use crate::implants::{Implant, ImplantKind, MOD_PATHFINDER};
use crate::inventory::{CarriedBy, Inventory};
use crate::storage::StoredIn;
use crate::tools::{Actor, DoAfter, do_after, now};

const SPRAY: &str = "sound/effects/spray.ogg";
const ROCKET: &str = "sound/vehicles/rocketlaunch.ogg";
const LANDING: &str = "sound/items/handling/toolbox/toolbox_drop.ogg";
const PING: &str = "sound/machines/ping.ogg";
const BANG: &str = "sound/effects/bang.ogg";
/// `PATHFINDER_PRE_ANIMATE_TIME`, the recall button's cooldown, the implanting `do_after`.
const PRE_LAUNCH_SECS: f64 = 2.0;
const RECALL_COOLDOWN_SECS: f64 = 5.0;
const IMPLANT_DS: f32 = 15.0;

/// The module's state: the implant still inside, or the mob and implant id it went into.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Pathfinder {
    pub implant_inside: bool,
    pub host: Option<(Entity, u32)>,
    pub in_transit: bool,
    /// The recall button's `recall_cooldown`.
    pub recall_ready: f64,
}

impl Default for Pathfinder {
    fn default() -> Self {
        Self {
            implant_inside: true,
            host: None,
            in_transit: false,
            recall_ready: 0.0,
        }
    }
}

/// On a pathfinder module: a recall on its way (`recall` -> `do_recall` -> the pod).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Recall {
    pub control: Entity,
    pub recaller: Entity,
    pub launch_at: f64,
    /// Set at launch: where and when it lands.
    pub landing: Option<(TilePos, f64)>,
}

/// On a control unit that landed next to its recaller: deploy once it is worn.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeployOnWear(pub Entity);

pub(super) fn plugin(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // The module in hand on a human implants them (`pathfinder/attack`).
    app.add_click_handler(ClickPhase::HeldOnTarget, 707, |w, c| {
        c.held
            .is_some_and(|held| attack(w, c.client, c.mob, held, c.target))
    })
    .add_systems(
        Update,
        (
            recalls.run_if(any_with_component::<Recall>),
            deploy_on_wear.run_if(any_with_component::<DeployOnWear>),
        ),
    );
}

/// `Initialize`: the implant inside.
pub fn init(world: &mut World, module: Entity, kind: Kind) {
    if kind == Kind::Pathfinder && world.get::<Pathfinder>(module).is_none() {
        world.entity_mut(module).insert(Pathfinder::default());
    }
}

fn sound(world: &mut World, e: Entity, file: &str, vol: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, e) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, vol, true));
    }
}

/// `ishuman`.
fn is_human(world: &World, mob: Entity) -> bool {
    crate::throw::is_carbon(world, mob)
}

/// `implant.implant(target, user)`: the module's implant into `target`.
fn implant_into(world: &mut World, module: Entity, target: Entity, user: Entity) -> bool {
    if !world
        .get::<Pathfinder>(module)
        .is_some_and(|p| p.implant_inside)
        || !is_human(world, target)
    {
        return false;
    }
    let id = crate::implants::next_id(world);
    let Some(imp) = Implant::new(MOD_PATHFINDER, id) else {
        return false;
    };
    if crate::implants::implant(world, target, Some(user), imp).is_err() {
        return false;
    }
    if let Some(mut p) = world.get_mut::<Pathfinder>(module) {
        p.implant_inside = false;
        p.host = Some((target, id));
    }
    sound(world, module, SPRAY, 30.0);
    true
}

/// `pathfinder/attack`: a human, then a 1.5 s `do_after`.
fn attack(world: &mut World, client: Entity, mob: Entity, held: Entity, target: Entity) -> bool {
    let inside = world.get::<Pathfinder>(held).map(|p| p.implant_inside);
    if inside.is_none() || world.get::<Health>(target).is_none() {
        return false;
    }
    if inside != Some(true) || !is_human(world, target) {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let job = ModJob::PathfinderImplant { module: held };
    do_after(world, actor, target, IMPLANT_DS, 0, Job::ModSuit(job));
    true
}

/// The end of `pathfinder/attack`'s `do_after`.
pub fn complete_implant(world: &mut World, user: Entity, action: &DoAfter, module: Entity) {
    let target = action.target;
    if !implant_into(world, module, target, user) {
        tell_mob(world, user, Span::Warning, "не удаётся имплантировать!");
        return;
    }
    if target == user {
        tell_mob(
            world,
            user,
            Span::Notice,
            "Вы имплантируете себе имплант МОД-навигатора.",
        );
    } else {
        let (who, whom) = (
            crate::medical::name_of(world, user),
            crate::medical::name_of(world, target),
        );
        crate::medical::visible(
            world,
            target,
            &format!("{who} имплантирует вам имплант МОД-навигатора."),
            &format!("{who} имплантирует {whom}."),
            Span::Notice,
        );
    }
}

/// `pathfinder/on_use`: implant the wearer, else recall the unworn suit.
pub fn on_use(world: &mut World, control: Entity, module: Entity, activator: Entity) {
    let Some(state) = world.get::<Pathfinder>(module).copied() else {
        return;
    };
    let wearer = world.get::<ModSuit>(control).and_then(|s| s.wearer);
    match wearer {
        Some(wearer) if state.implant_inside => {
            if !is_human(world, wearer) {
                return;
            }
            if !implant_into(world, module, wearer, wearer) {
                tell_mob(
                    world,
                    activator,
                    Span::Warning,
                    "не удаётся имплантировать!",
                );
                return;
            }
            tell_mob(world, activator, Span::Notice, "имплантировано");
            if activator != wearer {
                tell_mob(world, wearer, Span::Notice, "трекер имплантирован!");
            }
        }
        Some(_) => tell_mob(world, activator, Span::Warning, "костюм уже надет!"),
        None => {
            recall(world, control, module, activator);
        }
    }
}

/// The outermost thing holding `e` (tg `get_atom_on_turf`).
pub fn atom_on_turf(world: &World, mut e: Entity) -> Entity {
    for _ in 0..8 {
        if world.get::<TilePos>(e).is_some() {
            return e;
        }
        let Some(next) = world
            .get::<CarriedBy>(e)
            .map(|c| c.0)
            .or_else(|| world.get::<StoredIn>(e).map(|s| s.0))
            .or_else(|| world.get::<InSuit>(e).map(|s| s.0))
        else {
            return e;
        };
        e = next;
    }
    e
}

/// Whether `mob` still carries the implant `id`.
fn has_implant(world: &World, mob: Entity, id: u32) -> bool {
    crate::implants::implants_of(world, mob)
        .iter()
        .any(|i| i.id == id && i.kind == ImplantKind::ModPathfinder)
}

/// Why a recall by `recaller` is refused, if it is (`recall`'s checks).
pub fn recall_refusal(
    world: &World,
    control: Entity,
    module: Entity,
    recaller: Entity,
) -> Option<&'static str> {
    let state = world.get::<Pathfinder>(module)?;
    let Some((host, id)) = state.host else {
        return Some("нет импланта!");
    };
    // `allow_suit_activation = FALSE`: only the implant's host.
    if host != recaller || !has_implant(world, host, id) {
        return Some("неверный пользователь!");
    }
    let suit = world.get::<ModSuit>(control)?;
    if suit.open {
        return Some("крышка открыта!");
    }
    if state.in_transit {
        return Some("костюм в пути!");
    }
    let top = atom_on_turf(world, control);
    if world.get::<Health>(top).is_some() {
        return Some(if top == recaller {
            "уже надет!"
        } else {
            "костюм у кого-то другого!"
        });
    }
    None
}

/// `pathfinder/recall`: the suit takes off in 2 s.
pub fn recall(world: &mut World, control: Entity, module: Entity, recaller: Entity) -> bool {
    if let Some(refusal) = recall_refusal(world, control, module, recaller) {
        tell_mob(world, recaller, Span::Warning, refusal);
        return false;
    }
    let now = now(world);
    if let Some(mut p) = world.get_mut::<Pathfinder>(module) {
        p.in_transit = true;
    }
    world.entity_mut(module).insert(Recall {
        control,
        recaller,
        launch_at: now + PRE_LAUNCH_SECS,
        landing: None,
    });
    tell_mob(world, recaller, Span::Notice, "костюм вызван");
    true
}

/// The implant's "Recall MOD" button (`mod_recall/do_effect`).
pub fn recall_action(world: &mut World, mob: Entity, id: u32) {
    let mut q = world.query::<(Entity, &Pathfinder, &InSuit)>();
    let Some((module, control)) = q
        .iter(world)
        .find(|(_, p, _)| p.host == Some((mob, id)))
        .map(|(m, _, s)| (m, s.0))
    else {
        tell_mob(world, mob, Span::Warning, "нет сигнала костюма!");
        return;
    };
    let now = now(world);
    if world
        .get::<Pathfinder>(module)
        .is_some_and(|p| now < p.recall_ready)
    {
        tell_mob(world, mob, Span::Warning, "на перезарядке!");
        return;
    }
    if recall(world, control, module, mob) {
        tell_mob(world, mob, Span::Notice, "костюм летит...");
        if let Some(mut p) = world.get_mut::<Pathfinder>(module) {
            p.recall_ready = now + RECALL_COOLDOWN_SECS;
        }
    }
}

fn end_transit(world: &mut World, module: Entity) {
    world.entity_mut(module).remove::<Recall>();
    if let Some(mut p) = world.get_mut::<Pathfinder>(module) {
        p.in_transit = false;
    }
}

/// `do_recall` at launch, `pod_landed` at landing.
fn recalls(world: &mut World) {
    let now = now(world);
    let mut q = world.query::<(Entity, &Recall)>();
    let pending: Vec<(Entity, Recall)> = q.iter(world).map(|(e, r)| (e, *r)).collect();
    for (module, recall) in pending {
        let control = recall.control;
        if world.get_entity(control).is_err()
            || world.get::<InSuit>(module).map(|s| s.0) != Some(control)
        {
            if world.get_entity(module).is_ok() {
                end_transit(world, module);
            }
            continue;
        }
        match recall.landing {
            None if now >= recall.launch_at => launch(world, module, recall),
            Some((pos, at)) if now >= at => land(world, module, recall, pos),
            _ => {}
        }
    }
}

/// `do_recall`: out of where it is and into the air.
fn launch(world: &mut World, module: Entity, recall: Recall) {
    let control = recall.control;
    let container = atom_on_turf(world, control);
    if world.get::<Health>(container).is_some() {
        tell_mob(world, recall.recaller, Span::Warning, "запуск прерван!");
        end_transit(world, module);
        return;
    }
    if world
        .get::<crate::closets::Closet>(container)
        .is_some_and(|c| !c.opened)
        && !crate::closets::open(world, container)
    {
        // `bust_open`.
        sound(world, container, BANG, 50.0);
        if let Some(mut closet) = world.get_mut::<crate::closets::Closet>(container) {
            closet.locked = false;
            closet.welded = false;
            closet.broken = true;
        }
        crate::closets::open_with(world, container, false);
    }
    let Some(dest) = crate::explosions::turf_of(world, recall.recaller) else {
        end_transit(world, module);
        return;
    };
    sound(world, control, ROCKET, 80.0);
    world
        .entity_mut(control)
        .remove::<(TilePos, StoredIn, CarriedBy)>();
    let land_at = now(world) + crate::cargo::supplypod::TRANSIT_SECS;
    if let Some(mut r) = world.get_mut::<Recall>(module) {
        r.landing = Some((dest, land_at));
    }
}

/// `pod_landed`: on the tile, then onto a host next to it.
fn land(world: &mut World, module: Entity, recall: Recall, pos: TilePos) {
    let control = recall.control;
    end_transit(world, module);
    crate::storage::take_out(world, control, pos);
    sound(world, control, LANDING, 80.0);
    let Some((host, id)) = world.get::<Pathfinder>(module).and_then(|p| p.host) else {
        return;
    };
    let near = world
        .get::<TilePos>(host)
        .is_some_and(|h| h.chebyshev(pos).is_some_and(|d| d <= 1));
    if near && has_implant(world, host, id) {
        attach(world, control, module, host);
    }
}

/// `pathfinder/attach`: onto the back (what was there drops), then the parts deploy.
fn attach(world: &mut World, control: Entity, module: Entity, user: Entity) {
    if !is_human(world, user) {
        return;
    }
    let Some(at) = world.get::<TilePos>(user).copied() else {
        return;
    };
    let slot = InventorySlot::Back;
    if let Some(old) = world.get::<Inventory>(user).and_then(|i| i.slot(slot)) {
        if world.get::<crate::inventory::NoDrop>(old).is_some() {
            return;
        }
        crate::inventory::unequip(world, user, at, slot);
        world.entity_mut(old).remove::<CarriedBy>();
        crate::storage::take_out(world, old, at);
    }
    let inventory = world.get::<Inventory>(user).cloned().unwrap_or_default();
    if !crate::inventory::can_equip(world, user, &inventory, control, slot) {
        return;
    }
    world
        .entity_mut(control)
        .remove::<(TilePos, StoredIn)>()
        .insert(CarriedBy(user));
    if let Some(mut inv) = world.get_mut::<Inventory>(user) {
        *inv.slot_mut(slot) = Some(control);
    }
    world.entity_mut(control).insert(DeployOnWear(user));
    sound(world, control, PING, 50.0);
    let cost = def_of(world, module).map_or(0.0, |d| d.use_energy_cost);
    drain_power(world, control, cost);
}

/// `mod.quick_deploy(user)` once the suit's tick took the new wearer.
fn deploy_on_wear(world: &mut World) {
    let mut q = world.query::<(Entity, &DeployOnWear, &ModSuit)>();
    let ready: Vec<(Entity, Entity, Option<Entity>)> =
        q.iter(world).map(|(e, d, s)| (e, d.0, s.wearer)).collect();
    for (control, user, wearer) in ready {
        if wearer == Some(user) {
            world.entity_mut(control).remove::<DeployOnWear>();
            quick_deploy(world, control, user);
        } else if world.get::<CarriedBy>(control).map(|c| c.0) != Some(user) {
            world.entity_mut(control).remove::<DeployOnWear>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_outermost_holder_is_found() {
        let mut world = World::new();
        let closet = world.spawn(TilePos::new(3, 3, 1)).id();
        let bag = world.spawn(StoredIn(closet)).id();
        let suit = world.spawn(StoredIn(bag)).id();
        let module = world.spawn(InSuit(suit)).id();
        assert_eq!(atom_on_turf(&world, module), closet);
        assert_eq!(atom_on_turf(&world, closet), closet);
    }
}
