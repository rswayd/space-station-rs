//! The chemical grenade casing (server), after tg `chem_grenade.dm`, `/datum/wires/explosive/
//! chem_grenade` (`explosive.dm`) and `chem_splash` (`chem_splash.dm`).
//!
//! - A casing goes empty -> wired (one cable coil; its wires window opens on use in hand or
//!   with an assembly) -> ready (a screwdriver, with a beaker or bottle inside; wirecutters
//!   unlock it again). A wrench takes the beakers out, or the trigger and the cable when it is
//!   empty. The large casing takes any cup and heats by 25 K instead of 10 K, at 1.1x.
//! - The trigger is an assembly on its one wire (`crate::wires::attach_assembly`): a timer sets
//!   the fuse to its time, a proximity sensor makes it a landmine (arming in hand starts the
//!   sensor), a health sensor starts scanning. Its pulse primes the grenade (a landmine goes
//!   off at once).
//! - Going off (`crate::grenades::detonate`) pours every beaker into the casing's holder at
//!   `threatscale`, heats it by `ignition_temp` and reacts it now: explosions and EMPs of the
//!   reactions, and the smoke and foam of its smoke powder and foam reactions
//!   (`crate::effects::reactions`), happen on the grenade's tile
//!   (`crate::chem::refresh_container`). The casing stays, used up until a screwdriver resets
//!   its trigger.
// shortcut: what is left after the reactions vanishes (`spread_reagents` exposes nothing: the
// reagents' `expose_*` procs are not ported, see `crate::chem`); no premade grenades (metal
// foam, incendiary...), no slime extracts in the
// large casing, no 2 s `do_after` when resetting the trigger; logging is the server log.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmName};
use ss13_reagents::{Holder, Owner};
use ss13_shared::chat::Span;
use ss13_tgui::server::AlwaysState;

use super::server::{
    Assembly, Device, drop_out, insert, name_of, path_of, set_icon_state, tell, toggle_secure,
    within_reach,
};
use crate::chem::{Chem, Reagents};
use crate::storage::StoredIn;
use crate::tools::{ToolKind, tool_of};
use crate::wires::{WireAssemblies, WireColors, WireHolder, Wires, chem_grenade_set};

pub const CASING: &str = "/obj/item/grenade/chem_grenade";
const LARGE: &str = "/obj/item/grenade/chem_grenade/large";
const CABLE_COIL: &str = "/obj/item/stack/cable_coil";
const BEAKER: &str = "/obj/item/reagent_containers/cup/beaker";
const BOTTLE: &str = "/obj/item/reagent_containers/cup/bottle";
const BLUESPACE: &str = "/obj/item/reagent_containers/cup/beaker/bluespace";
const CUP: &str = "/obj/item/reagent_containers/cup";
const CONDIMENT: &str = "/obj/item/reagent_containers/condiment";
/// `casing_holder_volume`, `TCMB`.
const CASING_VOLUME: f32 = 1000.0;
const TCMB: f32 = 2.7;
/// `det_time` of the casing (5 s) and the screwdriver's other fuse (3 s).
const LONG_FUSE: f64 = 5.0;
const SHORT_FUSE: f64 = 3.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    Empty,
    Wired,
    Ready,
}

/// tg `/obj/item/grenade/chem_grenade` vars.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Casing {
    pub stage: Stage,
    /// `beakers`, in insertion order (still inside: [`beakers_of`]).
    pub beakers: Vec<Entity>,
    pub large: bool,
    /// `active` of a primed landmine (its proximity sensor is arming or scanning).
    pub active: bool,
}

impl Casing {
    /// `ignition_temp`, `threatscale`.
    fn ignition(&self) -> (f32, f32) {
        if self.large { (25.0, 1.1) } else { (10.0, 1.0) }
    }
}

/// The casing types (the plain and the large one).
pub fn is_casing(path: &str) -> bool {
    path == CASING || path == LARGE
}

/// The casing state of `item`, given on first use with its wires.
pub fn casing(world: &mut World, item: Entity) -> Option<Casing> {
    if let Some(c) = world.get::<Casing>(item) {
        return Some(c.clone());
    }
    let path = path_of(world, item)?.to_owned();
    if !is_casing(&path) {
        return None;
    }
    let set = chem_grenade_set();
    let wires = match world.get_resource_mut::<WireColors>() {
        Some(mut colors) => colors.wires_for(WireHolder::ChemGrenade, &set),
        None => Wires {
            holder: WireHolder::ChemGrenade,
            proper_name: set.proper_name,
            colors: crate::wires::colors_for(0, &set),
            cut: Vec::new(),
        },
    };
    let c = Casing {
        large: path == LARGE,
        ..default()
    };
    // The wires window of a held casing follows `interactable` (`wires_interactable`).
    world
        .entity_mut(item)
        .insert((c.clone(), wires, AlwaysState));
    stage_change(world, item, Stage::Empty);
    Some(world.get::<Casing>(item)?.clone())
}

/// The beakers still inside (tg `Exited` drops the others).
pub fn beakers_of(world: &World, item: Entity) -> Vec<Entity> {
    world.get::<Casing>(item).map_or_else(Vec::new, |c| {
        c.beakers
            .iter()
            .copied()
            .filter(|&b| world.get::<StoredIn>(b).is_some_and(|s| s.0 == item))
            .collect()
    })
}

pub fn ready(world: &World, item: Entity) -> bool {
    world
        .get::<Casing>(item)
        .is_some_and(|c| c.stage == Stage::Ready)
}

/// `display_timer` is off: a casing that is not locked.
pub fn hides_timer(world: &World, item: Entity) -> bool {
    match world.get::<Casing>(item) {
        Some(c) => c.stage != Stage::Ready,
        None => path_of(world, item).is_some_and(is_casing),
    }
}

/// The trigger on the wire (tg `wires.get_attached(wires.get_wire(1))`).
fn trigger(world: &World, item: Entity) -> Option<Entity> {
    let wires = world.get::<WireAssemblies>(item)?;
    wires
        .0
        .first()
        .map(|(_, a)| *a)
        .filter(|&a| world.get::<StoredIn>(a).is_some_and(|s| s.0 == item))
}

/// `landminemode`: a proximity sensor is the trigger.
fn landmine(world: &World, item: Entity) -> Option<Entity> {
    trigger(world, item).filter(|&a| {
        world
            .get::<Assembly>(a)
            .is_some_and(|a| matches!(a.device, Device::Prox { .. }))
    })
}

fn initial_name(world: &World, item: Entity) -> String {
    world
        .get::<DmName>(item)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_else(|| "chemical grenade".to_owned())
}

/// `stage_change`: the name (`update_name`) and icon (`update_icon_state`) follow the stage.
fn stage_change(world: &mut World, item: Entity, stage: Stage) {
    if let Some(mut c) = world.get_mut::<Casing>(item) {
        c.stage = stage;
    }
    let base = initial_name(world, item);
    let name = match stage {
        Stage::Empty => format!("{base} casing"),
        Stage::Wired => format!("unsecured {base}"),
        Stage::Ready => base,
    };
    world.entity_mut(item).insert(Name::new(name));
    refresh_icon(world, item);
}

fn refresh_icon(world: &mut World, item: Entity) {
    let Some(base) = world.get::<DmAtom>(item).map(|a| a.icon_state.clone()) else {
        return;
    };
    let Some(c) = world.get::<Casing>(item) else {
        return;
    };
    let armed = c.active || world.get::<crate::grenades::Armed>(item).is_some();
    let state = if armed {
        format!("{base}_active")
    } else {
        match c.stage {
            Stage::Empty => base,
            Stage::Wired => format!("{base}_ass"),
            Stage::Ready => format!("{base}_locked"),
        }
    };
    set_icon_state(world, item, &state);
}

/// `allowed_containers` minus `banned_containers`.
fn fits(large: bool, path: &str) -> Option<bool> {
    use ss13_mapload::fidelity::is_under;
    let allowed = if large {
        is_under(path, CUP) || is_under(path, CONDIMENT)
    } else {
        is_under(path, BEAKER) || is_under(path, BOTTLE)
    };
    if !allowed {
        return None;
    }
    Some(large || !is_under(path, BLUESPACE))
}

/// `attack_self`: a locked casing is primed, a wired one opens its wires. False for anything
/// that is not a casing.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(c) = casing(world, item) else {
        return false;
    };
    match c.stage {
        Stage::Ready => {
            if !c.active && world.get::<crate::grenades::Armed>(item).is_none() {
                arm(world, item, Some((client, mob)));
            }
        }
        Stage::Wired => crate::wires::interact(world, client, mob, item),
        Stage::Empty => {}
    }
    true
}

/// `chem_grenade/arm_grenade`: a landmine starts its sensor, else the fuse runs.
fn arm(world: &mut World, item: Entity, user: Option<(Entity, Entity)>) {
    let Some(sensor) = landmine(world, item) else {
        crate::grenades::arm(world, item, user, None);
        refresh_icon(world, item);
        return;
    };
    info!(
        "{:?} activated a proxy {} at {:?}",
        user.map(|u| u.1),
        name_of(world, item),
        crate::explosions::turf_of(world, item)
    );
    if let Some((client, _)) = user {
        let name = name_of(world, item);
        tell(
            world,
            client,
            Span::Warning,
            format!("You prime {name}, activating its proximity sensor."),
        );
    }
    if let Some(pos) = crate::explosions::turf_of(world, item) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/items/weapons/armbomb.ogg", 60.0, true),
        );
    }
    if let Some(mut c) = world.get_mut::<Casing>(item) {
        c.active = true;
    }
    // `toggle_scan(FALSE)`, then `activate()`: the sensor starts arming.
    if let Some(mut a) = world.get_mut::<Assembly>(sensor)
        && let Device::Prox { scanning, .. } = &mut a.device
    {
        *scanning = false;
    }
    super::server::activate(world, sensor);
    refresh_icon(world, item);
}

/// `/datum/wires/explosive/chem_grenade/on_pulse` and `on_cut` -> `explode()`: a locked
/// grenade primes (a landmine goes off).
pub fn wire_explode(world: &mut World, item: Entity) {
    if !ready(world, item) {
        return;
    }
    info!(
        "an assembly pulsed {} at {:?}",
        name_of(world, item),
        crate::explosions::turf_of(world, item)
    );
    if landmine(world, item).is_some() {
        crate::grenades::detonate(world, item);
    } else if world.get::<crate::grenades::Armed>(item).is_none() {
        arm(world, item, None);
    }
}

/// `chem_grenade/attach_assembly`: the trigger is secured; a timer sets the fuse, a proximity
/// sensor arms with the fuse's time, a health sensor starts scanning.
pub fn on_wire_attach(world: &mut World, item: Entity, a: Entity) {
    if world.get::<Assembly>(a).is_some_and(|a| !a.secured) {
        toggle_secure(world, a);
    }
    let det_time = crate::grenades::grenade(world, item).map_or(LONG_FUSE, |g| g.det_time);
    let mut fuse = None;
    if let Some(mut asm) = world.get_mut::<Assembly>(a) {
        match &mut asm.device {
            Device::Timer { saved_time, .. } => fuse = Some(*saved_time),
            Device::Prox { landmine, time, .. } => {
                *landmine = true;
                *time = det_time;
            }
            Device::Health { scanning, .. } => *scanning = true,
            _ => {}
        }
    }
    if let Some(fuse) = fuse
        && let Some(mut g) = world.get_mut::<crate::grenades::Grenade>(item)
    {
        g.det_time = fuse;
    }
}

/// `chem_grenade/detach_assembly`: a removed sensor is no landmine any more.
pub fn on_wire_detach(world: &mut World, _item: Entity, a: Entity) {
    if let Some(mut asm) = world.get_mut::<Assembly>(a)
        && let Device::Prox { landmine, .. } = &mut asm.device
    {
        *landmine = false;
    }
}

/// `/datum/wires/explosive/chem_grenade/interactable`: wired, and the user can reach it.
pub fn wires_interactable(world: &World, item: Entity, user: Entity) -> bool {
    world
        .get::<Casing>(item)
        .is_some_and(|c| c.stage == Stage::Wired)
        && within_reach(world, user, item)
}

/// Tools, cable, beakers and assemblies on a casing (tg `item_interaction`, `screwdriver_act`,
/// `wirecutter_act`, `wrench_act`). False when the click is not the casing's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if held == target || path_of(world, target).is_none_or(|p| !is_casing(p)) {
        return false;
    }
    if !within_reach(world, mob, target) {
        return false;
    }
    let Some(c) = casing(world, target) else {
        return false;
    };
    let base = initial_name(world, target);
    match tool_of(world, held).map(|t| t.0) {
        Some(ToolKind::Screwdriver) => {
            screwdriver(world, client, target, &c, &base);
            return true;
        }
        Some(ToolKind::Wirecutters) => {
            let active = c.active || world.get::<crate::grenades::Armed>(target).is_some();
            if c.stage != Stage::Ready || active {
                return false;
            }
            stage_change(world, target, Stage::Wired);
            tell(
                world,
                client,
                Span::Notice,
                format!("You unlock the {base} assembly."),
            );
            return true;
        }
        Some(ToolKind::Wrench) => {
            if c.stage != Stage::Wired {
                return false;
            }
            wrench(world, client, mob, target, &base);
            return true;
        }
        _ => {}
    }
    if world.get::<Assembly>(held).is_some() && c.stage == Stage::Wired {
        crate::wires::interact(world, client, mob, target);
        return true;
    }
    let held_path = path_of(world, held).unwrap_or_default().to_owned();
    if c.stage == Stage::Empty && ss13_mapload::fidelity::is_under(&held_path, CABLE_COIL) {
        if !crate::tools::use_stack(world, held, 1) {
            tell(
                world,
                client,
                Span::Warning,
                "You need one length of coil to wire the assembly!",
            );
            return true;
        }
        // In case the cable was removed and added again.
        if let Some(mut g) = world.get_mut::<crate::grenades::Grenade>(target) {
            g.det_time = LONG_FUSE;
        }
        stage_change(world, target, Stage::Wired);
        tell(
            world,
            client,
            Span::Notice,
            format!("You rig the {base} assembly."),
        );
        return true;
    }
    if c.stage != Stage::Wired || world.get::<Reagents>(held).is_none() {
        return false;
    }
    match fits(c.large, &held_path) {
        None => return false,
        Some(false) => {
            let name = crate::cooking::capitalize(&name_of(world, target));
            let what = name_of(world, held);
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} is too small to fit {what}!"),
            );
            return true;
        }
        Some(true) => {}
    }
    let beakers = beakers_of(world, target);
    if beakers.len() >= 2 {
        let name = crate::cooking::capitalize(&name_of(world, target));
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} can not hold more containers!"),
        );
        return true;
    }
    let what = name_of(world, held);
    insert(world, held, target);
    if let Some(mut c) = world.get_mut::<Casing>(target) {
        c.beakers = beakers;
        c.beakers.push(held);
    }
    info!("{mob} inserted {what} into a {base} assembly");
    tell(
        world,
        client,
        Span::Notice,
        format!("You add {what} to the {base} assembly."),
    );
    true
}

/// `chem_grenade/screwdriver_act`.
fn screwdriver(world: &mut World, client: Entity, item: Entity, c: &Casing, base: &str) {
    let used = crate::grenades::grenade(world, item).is_some_and(|g| g.used);
    if used {
        if let Some(mut g) = world.get_mut::<crate::grenades::Grenade>(item) {
            g.used = false;
        }
        tell(world, client, Span::Notice, "trigger reset");
        return;
    }
    match c.stage {
        Stage::Wired => {
            if !beakers_of(world, item).is_empty() {
                stage_change(world, item, Stage::Ready);
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("You lock the {base} assembly."),
                );
                return;
            }
            let armed_sensor = landmine(world, item).filter(|&s| {
                matches!(
                    world.get::<Assembly>(s).map(|a| &a.device),
                    Some(Device::Prox { scanning, timing, .. }) if *scanning || *timing
                )
            });
            let Some(sensor) = armed_sensor else {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!(
                        "You need to add at least one beaker before locking the {base} assembly!"
                    ),
                );
                return;
            };
            if let Some(mut a) = world.get_mut::<Assembly>(sensor)
                && let Device::Prox {
                    scanning, timing, ..
                } = &mut a.device
            {
                *scanning = false;
                *timing = false;
            }
            let name = name_of(world, sensor);
            tell(
                world,
                client,
                Span::Notice,
                format!("You disarm the {name}."),
            );
        }
        Stage::Ready => {
            let current = crate::grenades::grenade(world, item).map_or(LONG_FUSE, |g| g.det_time);
            let det_time = if current == LONG_FUSE {
                SHORT_FUSE
            } else {
                LONG_FUSE
            };
            if let Some(mut g) = world.get_mut::<crate::grenades::Grenade>(item) {
                g.det_time = det_time;
            }
            if let Some(sensor) = landmine(world, item)
                && let Some(mut a) = world.get_mut::<Assembly>(sensor)
                && let Device::Prox { time, .. } = &mut a.device
            {
                *time = det_time;
            }
            tell(
                world,
                client,
                Span::Notice,
                format!(
                    "You modify the time delay. It's set for {}.",
                    crate::jobs::display_time_text(det_time)
                ),
            );
        }
        Stage::Empty => tell(world, client, Span::Warning, "You need to add a wire!"),
    }
}

/// `chem_grenade/wrench_act`: the beakers out, or (empty) the trigger and the cable.
fn wrench(world: &mut World, client: Entity, mob: Entity, item: Entity, base: &str) {
    let beakers = beakers_of(world, item);
    if beakers.is_empty() {
        let color = world
            .get::<Wires>(item)
            .and_then(|w| w.colors.first().map(|(c, _)| *c));
        if let Some(color) = color {
            crate::wires::detach_assembly(world, item, color);
        }
        if let Some(pos) = crate::explosions::turf_of(world, item) {
            crate::construction::spawn_stack(world, CABLE_COIL, pos, 1);
        }
        stage_change(world, item, Stage::Empty);
        tell(
            world,
            client,
            Span::Notice,
            format!("You remove the activation mechanism from the {base} assembly."),
        );
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("You open the {base} assembly and remove the payload."),
    );
    for beaker in beakers {
        drop_out(world, beaker, item);
        info!("{mob} removed {} from a {base}", name_of(world, beaker));
    }
    if let Some(mut c) = world.get_mut::<Casing>(item) {
        c.beakers.clear();
    }
}

/// tg `chem_splash(get_turf(src), reagents, affected_area, beakers, ignition_temp,
/// threatscale)` from `chem_grenade/detonate`: the beakers into the casing's holder, heated,
/// reacted now.
pub fn splash(world: &mut World, item: Entity) {
    let Some(c) = world.get::<Casing>(item).cloned() else {
        return;
    };
    if let Some(mut c) = world.get_mut::<Casing>(item) {
        c.active = false;
    }
    let (heat, scale) = c.ignition();
    let beakers = beakers_of(world, item);
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let pos = crate::explosions::turf_of(world, item);
    if let (Some(chem), Some(pos)) = (chem, pos) {
        let reactants: Vec<(Entity, Holder)> = beakers
            .iter()
            .filter_map(|&b| world.get::<Reagents>(b).map(|r| (b, r.0.clone())))
            .collect();
        let total: f32 = reactants.iter().map(|(_, h)| h.total_volume).sum();
        if total > 0.0 {
            let maximum: f32 =
                CASING_VOLUME + reactants.iter().map(|(_, h)| h.maximum_volume).sum::<f32>();
            let owner = Owner {
                path: path_of(world, item).unwrap_or(CASING).to_owned(),
                living: false,
            };
            let mut holder = Holder::new(maximum * scale, 0, owner);
            for (beaker, mut h) in reactants {
                let volume = h.total_volume;
                h.trans_to(&chem, &mut holder, volume, scale, None, true, false);
                if let Some(mut r) = world.get_mut::<Reagents>(beaker) {
                    r.0 = h;
                }
            }
            holder.set_temperature((holder.chem_temp + heat).max(TCMB));
            holder.handle_reactions(&chem);
            info!(
                "a {} detonated at {pos:?} with {} units",
                name_of(world, item),
                holder.total_volume
            );
            world.entity_mut(item).insert(Reagents(holder));
            crate::chem::refresh_container(world, &chem, item);
            // shortcut: the rest is not spread onto the area (see the module docs).
            if world.get_entity(item).is_ok() {
                world
                    .entity_mut(item)
                    .remove::<(Reagents, crate::chem::Reacting)>();
            }
        }
    }
    if world.get_entity(item).is_ok() {
        refresh_icon(world, item);
    }
}

/// `chem_grenade/examine`: what is inside (by container names).
// shortcut: no reagent scan for those who can see reagents (no examiner here).
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(c) = world.get::<Casing>(target) else {
        return;
    };
    let beakers = beakers_of(world, target);
    if c.stage == Stage::Ready || beakers.is_empty() {
        return;
    }
    let names: Vec<String> = beakers.iter().map(|&b| name_of(world, b)).collect();
    if names.len() == 2 && names[0] == names[1] {
        lines.push((
            Span::Notice,
            format!("You see two {}s inside the grenade.", names[0]),
        ));
        return;
    }
    for name in names {
        lines.push((
            Span::Notice,
            format!("You see a {name} inside the grenade."),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casings_take_their_containers() {
        assert_eq!(
            fits(false, "/obj/item/reagent_containers/cup/beaker"),
            Some(true)
        );
        assert_eq!(
            fits(false, "/obj/item/reagent_containers/cup/bottle"),
            Some(true)
        );
        assert_eq!(fits(false, BLUESPACE), Some(false));
        assert_eq!(fits(true, BLUESPACE), Some(true));
        assert_eq!(
            fits(false, "/obj/item/reagent_containers/cup/glass/mug"),
            None
        );
        assert_eq!(
            fits(true, "/obj/item/reagent_containers/cup/glass/mug"),
            Some(true)
        );
        assert_eq!(fits(true, "/obj/item/reagent_containers/syringe"), None);
        assert!(is_casing(CASING) && is_casing(LARGE));
        assert!(!is_casing("/obj/item/grenade/chem_grenade/metalfoam"));
    }

    #[test]
    fn large_casings_burn_hotter() {
        let small = Casing::default();
        let large = Casing {
            large: true,
            ..default()
        };
        assert_eq!(small.ignition(), (10.0, 1.0));
        assert_eq!(large.ignition(), (25.0, 1.1));
    }
}
