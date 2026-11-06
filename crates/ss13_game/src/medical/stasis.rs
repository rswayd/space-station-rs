//! Stasis beds (server): tg `/obj/machinery/stasis` (`code/game/machinery/stasis.dm`),
//! `/datum/status_effect/grouped/stasis` (`TRAIT_STASIS`) and `/datum/component/free_operation`.
//!
//! - A mob is buckled to the bed lying, like to an operating table (drag-and-drop, an empty hand
//!   unbuckles: `super::surgery`). While buckled it is [`FreeOperation`]: every bodypart is ready
//!   for surgery without drapes.
//! - While the bed runs (`stasis_running`: enabled, powered, not broken, panel shut) its mob is
//!   [`InStasis`]: immobilized with its hands blocked (`crate::status_effects`), no breathing or
//!   pressure damage (`crate::breath`), no wounds, organs, blood or diseases
//!   (`crate::medical::medical_life`), no metabolism (`crate::chem`), no hunger, no SSD sleep,
//!   no radiation effects; surgery on it is painless. The fire on it goes out and the bed draws
//!   its active power while it holds someone in stasis.
//! - Alt-click turns it on or off, at most once per 5 s; the mattress overlay shows it running.
//! - Re-checked every machine tick (power, breaking) and on every buckle and unbuckle.
// shortcut: no sounds, no ripple filter, no `update_time_of_death`, no `REAGENT_IGNORE_STASIS`
// reagents (nothing metabolizes), no burn wound `on_stasis` healing, no energy rating power
// scaling of the parts, no silicon reach; the other `TRAIT_STASIS` sources (timestop, netpods)
// are not ported.

use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay};

use crate::medical::surgery::Buckled;
use crate::obj_damage::is_under;
use crate::power::{MachineTicks, Unpowered};

pub const STASIS_ROOT: &str = "/obj/machinery/stasis";
/// `STASIS_TOGGLE_COOLDOWN`: 50 deciseconds.
pub const TOGGLE_COOLDOWN_SECS: f64 = 5.0;
/// `AddElement(/datum/element/elevation, pixel_shift = 6)`.
const ELEVATION: i16 = 6;

/// A stasis bed's live state.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct StasisBed {
    /// `stasis_enabled`.
    pub enabled: bool,
    /// `stasis_can_toggle`: game seconds.
    pub can_toggle_at: f64,
}

impl Default for StasisBed {
    fn default() -> Self {
        Self {
            enabled: true,
            can_toggle_at: 0.0,
        }
    }
}

/// tg `TRAIT_STASIS` from a stasis bed (`STASIS_MACHINE_EFFECT`): the bed holding the mob.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct InStasis(pub Entity);

/// tg `/datum/component/free_operation` (`TRAIT_READY_TO_OPERATE` on every bodypart).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FreeOperation;

pub(super) fn build(app: &mut App) {
    app.add_observer(attach)
        .add_observer(on_buckle)
        .add_observer(on_unbuckle)
        .add_observer(on_thaw)
        .add_systems(
            Update,
            process_beds
                .after(crate::power::PowerSystems)
                .run_if(any_with_component::<StasisBed>),
        );
}

/// `/obj/machinery/stasis` and its subtypes.
pub fn is_bed(path: &str) -> bool {
    is_under(path, STASIS_ROOT)
}

fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if let Ok(o) = objs.get(add.entity)
        && is_bed(&o.path)
    {
        let e = add.entity;
        commands
            .entity(e)
            .insert_if_new((StasisBed::default(), crate::tables::Elevating(ELEVATION)));
        commands.queue(move |world: &mut World| refresh_look(world, e));
    }
}

/// The mob is [`InStasis`]: tg `HAS_TRAIT(mob, TRAIT_STASIS)`.
pub fn in_stasis(world: &World, mob: Entity) -> bool {
    world.get::<InStasis>(mob).is_some()
}

/// `stasis_running`: enabled and `is_operational` (powered, not broken, panel shut).
pub fn running(world: &World, bed: Entity) -> bool {
    world.get::<StasisBed>(bed).is_some_and(|b| b.enabled)
        && world.get::<Unpowered>(bed).is_none()
        && !world
            .get::<crate::obj_damage::Integrity>(bed)
            .is_some_and(|i| i.broken)
        && !world
            .get::<crate::construction::PanelOpen>(bed)
            .is_some_and(|p| p.0)
}

/// The stasis bed `mob` is buckled to.
fn bed_of(world: &World, mob: Entity) -> Option<Entity> {
    world
        .get::<Buckled>(mob)
        .map(|b| b.0)
        .filter(|&bed| world.get::<StasisBed>(bed).is_some())
}

/// `post_buckle_mob`/`post_unbuckle_mob`/`process`: `mob`'s stasis and free operation follow
/// the bed it lies on.
pub fn sync(world: &mut World, mob: Entity) {
    if world.get_entity(mob).is_err() {
        return;
    }
    let bed = bed_of(world, mob);
    let free = bed.is_some();
    if free != world.get::<FreeOperation>(mob).is_some() {
        if free {
            world.entity_mut(mob).insert(FreeOperation);
        } else {
            world.entity_mut(mob).remove::<FreeOperation>();
        }
    }
    let want = bed.filter(|&b| running(world, b));
    let have = world.get::<InStasis>(mob).map(|s| s.0);
    if want == have {
        return;
    }
    // `thaw_them` (the bed back to idle power is [`on_thaw`]).
    if have.is_some() {
        world.entity_mut(mob).remove::<InStasis>();
    }
    // `chill_out`.
    if let Some(bed) = want {
        world.entity_mut(mob).insert(InStasis(bed));
        crate::fire::extinguish_mob(world, mob);
        crate::power::update_use_power(world, bed, true);
    }
}

fn on_buckle(add: On<Add<Buckled>>, mut commands: Commands) {
    let mob = add.entity;
    commands.queue(move |world: &mut World| {
        sync(world, mob);
        if let Some(bed) = bed_of(world, mob) {
            refresh_look(world, bed);
        }
    });
}

fn on_unbuckle(remove: On<Remove<Buckled>>, mut commands: Commands) {
    let mob = remove.entity;
    commands.queue(move |world: &mut World| sync(world, mob));
}

/// `thaw_them`: the bed goes back to idle power (also when the mob is gone).
fn on_thaw(remove: On<Remove<InStasis>>, stasis: Query<&InStasis>, mut commands: Commands) {
    let Ok(&InStasis(bed)) = stasis.get(remove.entity) else {
        return;
    };
    commands.queue(move |world: &mut World| {
        if world.get::<StasisBed>(bed).is_some() {
            crate::power::update_use_power(world, bed, false);
        }
    });
}

/// `process` every machine tick: stasis follows power and breakage.
// Cost: one pass over the buckled mobs (a few per round) per machine tick (2 s).
fn process_beds(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let mut q = world.query_filtered::<Entity, Or<(With<Buckled>, With<InStasis>)>>();
    let mobs: Vec<Entity> = q.iter(world).collect();
    for mob in mobs {
        sync(world, mob);
    }
    let mut q = world.query_filtered::<Entity, With<StasisBed>>();
    let beds: Vec<Entity> = q.iter(world).collect();
    for bed in beds {
        refresh_look(world, bed);
    }
}

/// `click_alt`: on or off, once per [`TOGGLE_COOLDOWN_SECS`]. True when `target` is a bed.
pub fn click_alt(world: &mut World, mob: Entity, target: Entity) -> bool {
    let Some(state) = world.get::<StasisBed>(target).copied() else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < state.can_toggle_at {
        return true;
    }
    let enabled = !state.enabled;
    world.entity_mut(target).insert(StasisBed {
        enabled,
        can_toggle_at: now + TOGGLE_COOLDOWN_SECS,
    });
    let what = super::name_of(world, target);
    let (own, others) = if enabled {
        (
            format!("Вы включаете {what}."),
            format!("{what} включается."),
        )
    } else {
        (
            format!("Вы выключаете {what}."),
            format!("{what} выключается."),
        )
    };
    super::visible(world, mob, &own, &others, Span::Notice);
    if let Some(patient) = super::surgery::buckled_to(world, target) {
        sync(world, patient);
    }
    refresh_look(world, target);
    true
}

/// `update_icon_state` (`stasis`, `_broken`, `_maintenance`) and the `stasis_on` mattress while
/// it runs.
pub fn refresh_look(world: &mut World, bed: Entity) {
    if world.get::<StasisBed>(bed).is_none() {
        return;
    }
    let broken = world
        .get::<crate::obj_damage::Integrity>(bed)
        .is_some_and(|i| i.broken);
    let panel = world
        .get::<crate::construction::PanelOpen>(bed)
        .is_some_and(|p| p.0);
    let state = if broken {
        "stasis_broken"
    } else if panel {
        "stasis_maintenance"
    } else {
        "stasis"
    };
    crate::obj_damage::set_icon_state(world, bed, state);
    let mattress = world
        .get::<ss13_mapload::DmAtom>(bed)
        .zip(world.get_resource::<ss13_mapload::IconIndex>())
        .and_then(|(atom, icons)| icons.get(&atom.icon, "stasis_on"))
        .filter(|_| running(world, bed));
    let overlays: Vec<Overlay> = mattress.into_iter().map(Overlay::new).collect();
    if let Some(mut look) = world.get_mut::<Appearance>(bed)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world
    }

    #[test]
    fn a_running_bed_puts_its_patient_in_stasis() {
        let mut world = world();
        let bed = world.spawn(StasisBed::default()).id();
        let mob = world.spawn(Buckled(bed)).id();
        sync(&mut world, mob);
        assert_eq!(world.get::<InStasis>(mob), Some(&InStasis(bed)));
        assert!(world.get::<FreeOperation>(mob).is_some());
        // Unpowered: out of stasis, still free to operate on.
        world.entity_mut(bed).insert(Unpowered);
        sync(&mut world, mob);
        assert!(!in_stasis(&world, mob));
        assert!(world.get::<FreeOperation>(mob).is_some());
        world.entity_mut(bed).remove::<Unpowered>();
        sync(&mut world, mob);
        assert!(in_stasis(&world, mob));
        // Unbuckled: neither.
        world.entity_mut(mob).remove::<Buckled>();
        sync(&mut world, mob);
        assert!(!in_stasis(&world, mob));
        assert!(world.get::<FreeOperation>(mob).is_none());
    }

    #[test]
    fn a_disabled_or_broken_bed_does_not_run() {
        let mut world = world();
        let bed = world
            .spawn(StasisBed {
                enabled: false,
                can_toggle_at: 0.0,
            })
            .id();
        assert!(!running(&world, bed));
        world.entity_mut(bed).insert(StasisBed::default());
        assert!(running(&world, bed));
        world.entity_mut(bed).insert(crate::obj_damage::Integrity {
            current: 0.0,
            broken: true,
        });
        assert!(!running(&world, bed));
        // Another kind of furniture is not a bed.
        let table = world.spawn_empty().id();
        let mob = world.spawn(Buckled(table)).id();
        sync(&mut world, mob);
        assert!(!in_stasis(&world, mob) && world.get::<FreeOperation>(mob).is_none());
    }
}
