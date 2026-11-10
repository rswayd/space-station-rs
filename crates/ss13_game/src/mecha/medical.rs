//! Exosuit medical equipment (server), after tg `equipment/tools/medical_tools.dm`.
//!
//! - Mounted sleeper (`/obj/item/mecha_parts/mecha_equipment/sleeper`, melee): a click on a
//!   carbon next to the facing mech loads it after `equip_cooldown` (the mech keeps facing it;
//!   not buckled, the sleeper empty), then the cooldown and the energy. Every 2 s (`SSobj`) while
//!   the mech has `energy_drain` for it: oxygen damage -2/s, stun, knockdown, paralysis,
//!   immobilization and unconsciousness -4 s/s, 5 u epinephrine for a patient in crit with less
//!   than 5 u and no penthrite, 3 u formaldehyde for a dead one without; out of power it stops
//!   until the next patient. The patient leaves by resisting (B), by the window's "eject", or
//!   when the sleeper is detached or the mech destroyed. The medical sleeper also injects 10 u of
//!   a reagent of the mech's syringe gun while the patient has at most 10 u of it.
//! - Syringe gun (`medical/syringe_gun`, melee and ranged): a click on a syringe (or a storage
//!   holding syringes) loads up to 10 syringes, emptying them into its 75 u reservoir (no
//!   reactions); in "Analyze" mode a click on a container within 3 tiles learns its synthesizable
//!   reagents; in "Launch" mode it fires a syringe dart carrying up to 15 u of the reservoir
//!   (not for pacifists). Toggled reagents are synthesized at `seconds_per_tick × synth_speed`
//!   (5 u per 2 s) split between them, paying `energy_drain` per reagent, until the reservoir is
//!   full or the power fails.
//! - Medical beamgun (`medical/mechmedbeam`, melee and ranged): `crate::guns_special`'s beam
//!   fired from the mech (`mounted_medbeam_fire`); it lets go with the safety on and pauses
//!   without a pilot.
//!
//! Cost: one pass over sleepers and syringe guns every 2 s.
// shortcut: the syringe gun's mode, synthesis and purges are the Mecha window's buttons
// (`super::panel`); no combat log or icon2html; reagents injected into the patient are only
// moved (no `expose_mob` reactions), as `crate::chem::transfer_to_mob` does for darts.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_net::server::IntentReceived;
use ss13_reagents::reagent_flags::REAGENT_CAN_BE_SYNTHESIZED;
use ss13_reagents::{DEFAULT_REAGENT_TEMPERATURE, Holder, Owner, ReagentId, flags};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClientIntent;

use super::core::{Mech, MechClick, MechJob, has_charge, name_of, path_of, use_energy, visible};
use super::equipment::{EquipJob, action_checks, def, spend, still_aimed};
use super::{GearReagent, MechaGear, SleeperGear, SleeperPatient, SyringeGear};
use crate::chem::{Chem, MobReagents, Reagents};
use crate::closets::InCloset;
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::obj_damage::is_under;
use crate::status_effects::Incap;
use crate::tools::{Actor, DoAfter, do_after, tell};

pub const SLEEPER: &str = "/obj/item/mecha_parts/mecha_equipment/sleeper";
pub const MEDICAL_SLEEPER: &str = "/obj/item/mecha_parts/mecha_equipment/sleeper/medical";
pub const SYRINGE_GUN: &str = "/obj/item/mecha_parts/mecha_equipment/medical/syringe_gun";
pub const MEDBEAM: &str = "/obj/item/mecha_parts/mecha_equipment/medical/mechmedbeam";
const SYRINGE: &str = "/obj/item/reagent_containers/syringe";
const DART: &str = "/obj/projectile/bullet/dart/syringe";
const EPINEPHRINE: &str = "/datum/reagent/medicine/epinephrine";
const MULTIVER: &str = "/datum/reagent/medicine/c2/multiver";
const PENTHRITE: &str = "/datum/reagent/medicine/c2/penthrite";
const FORMALDEHYDE: &str = "/datum/reagent/toxin/formaldehyde";
/// `SSobj` wait: the sleeper and the syringe gun process every 2 s.
const PROCESS_SECS: f64 = 2.0;
/// `inject_amount` of the medical sleeper.
const INJECT_AMOUNT: f32 = 10.0;
/// The syringe gun's `max_syringes`, `max_volume`, `synth_speed` (per 2 s).
const MAX_SYRINGES: usize = 10;
const MAX_VOLUME: f32 = 75.0;
const SYNTH_SPEED: f32 = 2.5;
/// `/obj/item/reagent_containers/syringe` `volume`, and the dart's `create_reagents(50)`.
const SYRINGE_VOLUME: f32 = 15.0;
const DART_VOLUME: f32 = 50.0;
/// `get_dist(src, A) >= 4` refuses the analysis.
const ANALYZE_RANGE: i32 = 3;

/// A mounted sleeper's patient (tg `patient`) and its processing.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct MechSleeper {
    pub patient: Option<Entity>,
    pub mech: Option<Entity>,
    /// `START_PROCESSING` until power fails or the patient leaves.
    pub processing: bool,
    pub next_tick: f64,
}

/// A mounted syringe gun (tg `syringes`, `known_reagents`, `processed_reagents`, `mode`); its
/// reservoir is the entity's [`Reagents`].
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct MechSyringeGun {
    /// Syringes inside the equipment.
    pub syringes: Vec<Entity>,
    pub known: Vec<ReagentId>,
    pub processed: Vec<ReagentId>,
    /// `ANALYZE_SYRINGE_MODE`.
    pub analyze: bool,
    pub next_tick: f64,
}

pub struct MechMedicalPlugin;

impl Plugin for MechMedicalPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                process_sleepers,
                process_syringe_guns,
                resist_out.after(ss13_world::WorldSystems::Movement),
                follow_mechs.after(ss13_world::WorldSystems::Movement),
            ),
        )
        .add_observer(release_on_despawn);
    }
}

fn chem(world: &World) -> Option<std::sync::Arc<ss13_reagents::Chemistry>> {
    world.get_resource::<Chem>().map(|c| c.0.clone())
}

fn equip_path(world: &World, equip: Entity) -> Option<String> {
    def(world, equip).map(|d| d.path)
}

/// The medical equipment this module runs.
pub fn is_medical(world: &World, equip: Entity) -> bool {
    equip_path(world, equip).is_some_and(|p| {
        is_under(&p, SLEEPER) || is_under(&p, SYRINGE_GUN) || is_under(&p, MEDBEAM)
    })
}

/// The selected medical equipment on the click's target (`action`).
pub fn action(world: &mut World, click: MechClick, mech: Entity, equip: Entity) {
    let Some(path) = equip_path(world, equip) else {
        return;
    };
    if is_under(&path, SLEEPER) {
        sleeper_action(world, click, mech, equip);
    } else if is_under(&path, SYRINGE_GUN) {
        syringe_action(world, click, mech, equip);
    } else if is_under(&path, MEDBEAM) {
        // `mechmedbeam/action` skips `action_checks` and the parent: no cooldown or energy.
        crate::guns_special::mounted_medbeam_fire(world, mech, equip, click.target);
    }
}

// --- Sleeper -------------------------------------------------------------------------------

fn is_carbon(world: &World, e: Entity) -> bool {
    world.get::<MobReagents>(e).is_some() && world.get::<Health>(e).is_some()
}

/// `patient_insertion_check`.
fn insertion_refusal(world: &World, equip: Entity, target: Entity) -> Option<String> {
    let name = name_of(world, target);
    if world
        .get::<crate::medical::surgery::Buckled>(target)
        .is_some()
    {
        return Some(format!(
            "{name} will not fit into the sleeper because they're buckled!"
        ));
    }
    if world.get::<InCloset>(target).is_some() {
        return Some(format!("{name} will not fit into the sleeper!"));
    }
    if world
        .get::<MechSleeper>(equip)
        .is_some_and(|s| s.patient.is_some())
    {
        return Some("The sleeper is already occupied!".to_owned());
    }
    None
}

fn sleeper_action(world: &mut World, click: MechClick, mech: Entity, equip: Entity) {
    if !action_checks(world, mech, equip) || !is_carbon(world, click.target) {
        return;
    }
    if let Some(text) = insertion_refusal(world, equip, click.target) {
        tell(world, click.client, Span::Warning, text);
        return;
    }
    let (target, sleeper, mech_name) = (
        name_of(world, click.target),
        name_of(world, equip),
        name_of(world, mech),
    );
    tell(
        world,
        click.client,
        Span::Notice,
        format!("You start putting {target} into {sleeper}..."),
    );
    if let Some(&pos) = world.get::<TilePos>(mech) {
        visible(
            world,
            pos,
            Span::Warning,
            format!("{mech_name} starts putting {target} into the {sleeper}."),
            &[click.pilot],
        );
    }
    let cooldown = def(world, equip).map_or(0.0, |d| d.equip_cooldown);
    do_after(
        world,
        Actor {
            client: click.client,
            mob: click.pilot,
            item: None,
        },
        click.target,
        cooldown,
        0,
        Job::Mecha(MechJob::Equipment(EquipJob::SleeperLoad { equip })),
    );
}

/// End of the loading `do_after`: still facing it, the checks again, then in.
pub fn complete_load(world: &mut World, mob: Entity, action: &DoAfter, equip: Entity) {
    let target = action.target;
    let Some(mech) = still_aimed(world, mob, equip, action.target_pos) else {
        return;
    };
    if !is_carbon(world, target) || insertion_refusal(world, equip, target).is_some() {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mech) else {
        return;
    };
    world.entity_mut(target).insert(pos);
    crate::closets::put_mob_inside(world, target, equip);
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(equip).insert(MechSleeper {
        patient: Some(target),
        mech: Some(mech),
        processing: true,
        next_tick: now + PROCESS_SECS,
    });
    let (name, sleeper, mech_name) = (
        name_of(world, target),
        name_of(world, equip),
        name_of(world, mech),
    );
    tell(
        world,
        action.client,
        Span::Notice,
        format!("{name} successfully loaded into {sleeper}. Life support functions engaged."),
    );
    visible(
        world,
        pos,
        Span::Warning,
        format!("{mech_name} loads {name} into {sleeper}."),
        &[mob],
    );
    let cooldown = def(world, equip).map_or(0.0, |d| d.equip_cooldown);
    spend(world, mech, equip, cooldown);
}

/// `go_out`: the patient onto the mech's tile.
pub fn go_out(world: &mut World, equip: Entity) {
    let Some(state) = world.get::<MechSleeper>(equip).copied() else {
        return;
    };
    let Some(patient) = state.patient else {
        return;
    };
    world.entity_mut(equip).insert(MechSleeper::default());
    if world.get_entity(patient).is_err() {
        return;
    }
    if world
        .get::<InCloset>(patient)
        .is_some_and(|c| c.closet == equip)
    {
        crate::closets::let_out(world, patient);
    }
    let name = name_of(world, patient);
    if let Some(mech) = state.mech {
        if let Some(&pos) = world.get::<TilePos>(mech) {
            world.entity_mut(patient).insert(pos);
        }
        super::core::tell_occupant(
            world,
            mech,
            Span::Notice,
            format!("{name} ejected. Life support functions disabled."),
        );
    }
}

/// tg `detach` / `Destroy` of the equipment: the patient comes out.
pub fn on_detach(world: &mut World, equip: Entity) {
    go_out(world, equip);
}

/// A despawned sleeper lets its patient out where it was.
fn release_on_despawn(
    remove: On<Remove<MechSleeper>>,
    sleepers: Query<&MechSleeper>,
    mut commands: Commands,
) {
    let Some(patient) = sleepers.get(remove.entity).ok().and_then(|s| s.patient) else {
        return;
    };
    let equip = remove.entity;
    commands.queue(move |world: &mut World| {
        if world
            .get::<InCloset>(patient)
            .is_some_and(|c| c.closet == equip)
        {
            crate::closets::let_out(world, patient);
        }
    });
}

/// `container_resist_act`: a patient resisting (B) leaves.
fn resist_out(world: &mut World, mut intents: Local<MessageCursor<IntentReceived>>) {
    let resists: Vec<Entity> = intents
        .read(world.resource::<Messages<IntentReceived>>())
        .filter(|i| matches!(i.intent, ClientIntent::Resist))
        .map(|i| i.client)
        .collect();
    for client in resists {
        let Some(mob) = world.resource::<crate::server::Players>().mob(client) else {
            continue;
        };
        let Some(equip) = world.get::<InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        if world
            .get::<MechSleeper>(equip)
            .is_some_and(|s| s.patient == Some(mob))
        {
            go_out(world, equip);
        }
    }
}

/// Patients stay on their mech's tile.
fn follow_mechs(world: &mut World) {
    let mut q = world.query::<&MechSleeper>();
    let pairs: Vec<(Entity, Entity)> = q
        .iter(world)
        .filter_map(|s| Some((s.patient?, s.mech?)))
        .collect();
    for (patient, mech) in pairs {
        let Some(&pos) = world.get::<TilePos>(mech) else {
            continue;
        };
        if world.get::<TilePos>(patient).is_some_and(|&p| p != pos) {
            world.entity_mut(patient).insert(pos);
        }
    }
}

/// `sleeper/process` every 2 s.
fn process_sleepers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &MechSleeper)>();
    let due: Vec<(Entity, MechSleeper)> = q
        .iter(world)
        .filter(|(_, s)| s.processing && now >= s.next_tick)
        .map(|(e, s)| (e, *s))
        .collect();
    for (equip, state) in due {
        if let Some(mut s) = world.get_mut::<MechSleeper>(equip) {
            s.next_tick = now + PROCESS_SECS;
        }
        let (Some(mech), Some(patient)) = (state.mech, state.patient) else {
            continue;
        };
        let drain = def(world, equip).map_or(0.0, |d| d.energy_drain);
        if !has_charge(world, mech, drain) {
            let name = name_of(world, equip);
            super::core::tell_occupant(
                world,
                mech,
                Span::Warning,
                format!("{name} deactivated - no power."),
            );
            if let Some(mut s) = world.get_mut::<MechSleeper>(equip) {
                s.processing = false;
            }
            continue;
        }
        if world
            .get::<InCloset>(patient)
            .is_none_or(|c| c.closet != equip)
        {
            let name = name_of(world, patient);
            super::core::tell_occupant(
                world,
                mech,
                Span::Notice,
                format!("{name} no longer detected. Life support functions disabled."),
            );
            world.entity_mut(equip).insert(MechSleeper::default());
            continue;
        }
        life_support(world, patient, PROCESS_SECS as f32);
        use_energy(world, mech, drain);
    }
}

/// The sleeper's effect on the patient over `secs`.
fn life_support(world: &mut World, patient: Entity, secs: f32) {
    crate::medical::diseases::damage(world, patient, DamageType::Oxy, -2.0 * secs);
    for kind in [
        Incap::Stun,
        Incap::Knockdown,
        Incap::Paralyze,
        Incap::Immobilize,
        Incap::Unconscious,
    ] {
        crate::status_effects::adjust(world, patient, kind, -4.0 * secs);
    }
    let Some(chem) = chem(world) else {
        return;
    };
    let stat = world.get::<Stat>(patient).copied().unwrap_or_default();
    let amount = |world: &World, path: &str| -> f32 {
        let Some(id) = chem.id(path) else {
            return 0.0;
        };
        world
            .get::<MobReagents>(patient)
            .map_or(0.0, |r| r.blood.amount(id))
    };
    let crit = stat != Stat::Conscious;
    let add = (crit && amount(world, EPINEPHRINE) < 5.0 && amount(world, PENTHRITE) <= 0.0)
        .then_some((EPINEPHRINE, 5.0))
        .into_iter()
        .chain(
            (stat == Stat::Dead && amount(world, FORMALDEHYDE) <= 0.0)
                .then_some((FORMALDEHYDE, 3.0)),
        )
        .collect::<Vec<_>>();
    for (path, units) in add {
        let Some(id) = chem.id(path) else {
            continue;
        };
        if let Some(mut r) = world.get_mut::<MobReagents>(patient) {
            r.blood.add_reagent(
                &chem,
                id,
                units,
                DEFAULT_REAGENT_TEMPERATURE,
                None,
                None,
                false,
                false,
            );
        }
    }
}

/// The medical sleeper's `inject_reagent` from the mech's syringe gun.
fn inject_reagent(world: &mut World, mech: Entity, sleeper: Entity, name: &str) {
    let Some(patient) = world.get::<MechSleeper>(sleeper).and_then(|s| s.patient) else {
        return;
    };
    let Some(gun) = syringe_gun_of(world, mech) else {
        return;
    };
    let Some(chem) = chem(world) else {
        return;
    };
    let Some((id, volume)) = world.get::<Reagents>(gun).and_then(|r| {
        r.0.reagents
            .iter()
            .find(|x| chem.def(x.id).name == name)
            .map(|x| (x.id, x.volume))
    }) else {
        return;
    };
    let to_inject = volume.min(INJECT_AMOUNT);
    let has = world
        .get::<MobReagents>(patient)
        .map_or(0.0, |r| r.blood.amount(id));
    if to_inject <= 0.0 || has + to_inject > INJECT_AMOUNT * 2.0 {
        return;
    }
    let patient_name = name_of(world, patient);
    super::core::tell_occupant(
        world,
        mech,
        Span::Notice,
        format!("Injecting {patient_name} with {to_inject} units of {name}."),
    );
    let Some(mut reservoir) = world
        .get_mut::<Reagents>(gun)
        .map(|mut r| std::mem::replace(&mut r.0, Holder::new(0.0, 0, Owner::default())))
    else {
        return;
    };
    if let Some(mut r) = world.get_mut::<MobReagents>(patient) {
        reservoir.trans_to(&chem, &mut r.blood, to_inject, 1.0, Some(id), false, false);
    }
    if let Some(mut r) = world.get_mut::<Reagents>(gun) {
        r.0 = reservoir;
    }
}

// --- Syringe gun ---------------------------------------------------------------------------

/// The syringe gun attached to `mech` (`locate() in chassis`).
fn syringe_gun_of(world: &World, mech: Entity) -> Option<Entity> {
    world
        .get::<Mech>(mech)?
        .equipment()
        .find(|&e| equip_path(world, e).is_some_and(|p| is_under(&p, SYRINGE_GUN)))
}

/// `Initialize`: the reservoir and the starting reagents, made on first use.
fn ensure_gun(world: &mut World, gun: Entity) {
    if world.get::<MechSyringeGun>(gun).is_some() {
        return;
    }
    let known = chem(world)
        .map(|c| {
            [EPINEPHRINE, MULTIVER]
                .iter()
                .filter_map(|p| c.id(p))
                .collect()
        })
        .unwrap_or_default();
    world.entity_mut(gun).insert((
        MechSyringeGun { known, ..default() },
        Reagents(Holder::new(MAX_VOLUME, flags::NO_REACT, Owner::default())),
    ));
}

fn syringe_action(world: &mut World, click: MechClick, mech: Entity, gun: Entity) {
    if !action_checks(world, mech, gun) {
        return;
    }
    ensure_gun(world, gun);
    let target = click.target;
    let target_path = path_of(world, target).unwrap_or_default();
    if is_under(&target_path, SYRINGE) {
        load_syringe(world, click.client, mech, gun, target);
        return;
    }
    if world.get::<crate::storage::Storage>(target).is_some() {
        let syringes: Vec<Entity> = world
            .get::<crate::storage::Stored>(target)
            .map(|s| s.items().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|&e| path_of(world, e).is_some_and(|p| is_under(&p, SYRINGE)))
            .collect();
        for syringe in syringes {
            load_syringe(world, click.client, mech, gun, syringe);
        }
        return;
    }
    let state = world
        .get::<MechSyringeGun>(gun)
        .cloned()
        .unwrap_or_default();
    if state.analyze {
        analyze(world, click.client, mech, gun, target);
        return;
    }
    if state.syringes.is_empty() {
        tell(world, click.client, Span::Warning, "No syringes loaded.");
        return;
    }
    let volume = world.get::<Reagents>(gun).map_or(0.0, |r| r.0.total_volume);
    if volume <= 0.0 {
        tell(
            world,
            click.client,
            Span::Warning,
            "No available reagents to load syringe with.",
        );
        return;
    }
    if world
        .get::<crate::medical::traumas::Pacifist>(click.pilot)
        .is_some()
    {
        tell(
            world,
            click.client,
            Span::Warning,
            "The syringe gun might be lethally chambered! You don't want to risk harming anyone...",
        );
        return;
    }
    fire(world, click, mech, gun);
    let cooldown = def(world, gun).map_or(0.0, |d| d.equip_cooldown);
    spend(world, mech, gun, cooldown);
}

/// `load_syringe`: its reagents into the reservoir, the syringe inside.
fn load_syringe(world: &mut World, client: Entity, mech: Entity, gun: Entity, syringe: Entity) {
    let count = world
        .get::<MechSyringeGun>(gun)
        .map_or(0, |s| s.syringes.len());
    if count >= MAX_SYRINGES {
        tell(
            world,
            client,
            Span::Warning,
            "The syringe gun's syringe chamber is full!",
        );
        return;
    }
    // `chassis.Adjacent(S)`: on the floor next to the mech, or in a storage there.
    let near = |world: &World| {
        let (Some(&mech_pos), Some(pos)) = (
            world.get::<TilePos>(mech),
            crate::modpc::pda::turf_of(world, syringe),
        ) else {
            return false;
        };
        mech_pos.z == pos.z && (mech_pos.x - pos.x).abs() <= 1 && (mech_pos.y - pos.y).abs() <= 1
    };
    if !near(world) {
        tell(world, client, Span::Warning, "Unable to load syringe!");
        return;
    }
    crate::chem::transfer(world, syringe, gun, f32::MAX);
    crate::storage::put_inside(world, syringe, gun);
    if let Some(mut s) = world.get_mut::<MechSyringeGun>(gun) {
        s.syringes.push(syringe);
    }
    tell(world, client, Span::Notice, "Syringe loaded.");
}

/// `analyze_reagents`.
fn analyze(world: &mut World, client: Entity, mech: Entity, gun: Entity, target: Entity) {
    let close = match (
        world.get::<TilePos>(mech),
        crate::modpc::pda::turf_of(world, target),
    ) {
        (Some(&a), Some(b)) => {
            a.z == b.z && a.chebyshev(b).is_some_and(|d| d as i32 <= ANALYZE_RANGE)
        }
        _ => false,
    };
    if !close {
        tell(world, client, Span::Notice, "The object is too far away!");
        return;
    }
    let name = name_of(world, target);
    let Some(holder) = world
        .get::<Reagents>(target)
        .filter(|_| world.get::<Health>(target).is_none())
        .map(|r| r.0.clone())
    else {
        tell(
            world,
            client,
            Span::Warning,
            format!("No reagent info gained from {name}."),
        );
        return;
    };
    let Some(chem) = chem(world) else {
        return;
    };
    tell(world, client, Span::Notice, "Analyzing reagents...");
    for r in &holder.reagents {
        let def = chem.def(r.id);
        if def.chemical_flags & REAGENT_CAN_BE_SYNTHESIZED == 0 {
            continue;
        }
        let added = world.get_mut::<MechSyringeGun>(gun).is_some_and(|mut s| {
            if s.known.contains(&r.id) {
                return false;
            }
            s.known.push(r.id);
            true
        });
        if added {
            tell(
                world,
                client,
                Span::Notice,
                format!(
                    "Reagent analyzed, identified as {} and added to database.",
                    def.name
                ),
            );
        }
    }
    tell(world, client, Span::Notice, "Analysis complete.");
}

/// The dart of `/obj/item/ammo_casing/syringegun` (`/obj/projectile/bullet/dart/syringe`).
fn dart_data() -> ss13_mapload::ProjectileData {
    ss13_mapload::ProjectileData {
        path: DART.to_owned(),
        damage: 6.0,
        damage_type: "brute".into(),
        armor_flag: "bullet".into(),
        speed: 1.25,
        range: 50.0,
        pass_flags: crate::projectile::PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ricochet_decay_chance: 0.7,
        ricochet_decay_damage: 0.7,
        ricochet_incidence_leeway: 40.0,
        wound_falloff_tile: -5.0,
        // `SHARP_POINTY`.
        sharpness: 2,
        ..default()
    }
}

/// `chambered.fire_casing`: the first syringe's worth of the reservoir in a dart, the syringe
/// used up.
fn fire(world: &mut World, click: MechClick, mech: Entity, gun: Entity) {
    let Some(syringe) = world
        .get_mut::<MechSyringeGun>(gun)
        .and_then(|mut s| (!s.syringes.is_empty()).then(|| s.syringes.remove(0)))
    else {
        return;
    };
    if world.get_entity(syringe).is_ok() {
        world.despawn(syringe);
    }
    let Some(chem) = chem(world) else {
        return;
    };
    // The dart reacts (`reagents.flags &= ~NO_REACT`).
    let mut dart = Holder::new(DART_VOLUME, 0, Owner::default());
    if let Some(mut r) = world.get_mut::<Reagents>(gun) {
        let amount = SYRINGE_VOLUME.min(r.0.total_volume);
        r.0.trans_to(&chem, &mut dart, amount, 1.0, None, false, false);
    }
    let Some(&from) = world.get::<TilePos>(mech) else {
        return;
    };
    let zone = world
        .get::<crate::health::ZoneSelected>(click.pilot)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone();
    let target = Some(click.target).filter(|&t| world.get_entity(t).is_ok());
    let data = dart_data();
    crate::projectile::fire_with(
        world,
        crate::projectile::Shot {
            data: &data,
            firer: mech,
            from,
            target,
            target_pos: click.target_pos,
            zone,
            hit_prone: false,
            pixel: [0, 0],
            deviation: 0.0,
            fired_from: Some(gun),
        },
        crate::guns_special::Dart {
            reagents: Some(dart),
            dna: None,
            piercing: false,
            hook: false,
        },
    );
}

/// `synthesize`: a known reagent in or out of production.
fn toggle_synthesis(world: &mut World, client: Entity, gun: Entity, id: ReagentId) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut s) = world.get_mut::<MechSyringeGun>(gun) else {
        return;
    };
    if let Some(i) = s.processed.iter().position(|&p| p == id) {
        s.processed.remove(i);
        return;
    }
    let first = s.processed.is_empty();
    s.processed.push(id);
    if first {
        s.next_tick = now + PROCESS_SECS;
    }
    let name = chem(world)
        .map(|c| c.def(id).name.clone())
        .unwrap_or_default();
    tell(
        world,
        client,
        Span::Notice,
        format!("{name} added to production"),
    );
    tell(world, client, Span::Notice, "Reagent processing started.");
}

/// `syringe_gun/process` every 2 s while reagents are in production.
fn process_syringe_guns(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &MechSyringeGun)>();
    let due: Vec<(Entity, Vec<ReagentId>)> = q
        .iter(world)
        .filter(|(_, s)| !s.processed.is_empty() && now >= s.next_tick)
        .map(|(e, s)| (e, s.processed.clone()))
        .collect();
    let Some(chem) = chem(world) else {
        return;
    };
    for (gun, processed) in due {
        if let Some(mut s) = world.get_mut::<MechSyringeGun>(gun) {
            s.next_tick = now + PROCESS_SECS;
        }
        let mech = world
            .get::<crate::storage::StoredIn>(gun)
            .map(|s| s.0)
            .filter(|&m| {
                world
                    .get::<Mech>(m)
                    .is_some_and(|m| m.equipment().any(|e| e == gun))
            });
        let Some(mech) = mech else {
            continue;
        };
        let drain = def(world, gun).map_or(0.0, |d| d.energy_drain);
        let full = world
            .get::<Reagents>(gun)
            .is_none_or(|r| r.0.total_volume >= r.0.maximum_volume);
        if full || !has_charge(world, mech, drain) {
            super::core::tell_occupant(world, mech, Span::Warning, "Reagent processing stopped.");
            if let Some(mut s) = world.get_mut::<MechSyringeGun>(gun) {
                s.processed.clear();
            }
            continue;
        }
        let amount = PROCESS_SECS as f32 * SYNTH_SPEED / processed.len() as f32;
        for id in processed {
            if let Some(mut r) = world.get_mut::<Reagents>(gun) {
                r.0.add_reagent(
                    &chem,
                    id,
                    amount,
                    DEFAULT_REAGENT_TEMPERATURE,
                    None,
                    None,
                    true,
                    false,
                );
            }
            use_energy(world, mech, drain);
        }
    }
}

// --- The Mecha window ----------------------------------------------------------------------

fn reagent_lines(chem: &ss13_reagents::Chemistry, holder: &Holder) -> Vec<GearReagent> {
    holder
        .reagents
        .iter()
        .map(|r| GearReagent {
            name: chem.def(r.id).name.clone(),
            volume: (r.volume * 100.0).round() / 100.0,
        })
        .collect()
}

/// `get_snowflake_data` of the medical equipment into `gear`.
pub fn gear_data(world: &World, mech: Entity, equip: Entity, gear: &mut MechaGear) {
    let Some(path) = equip_path(world, equip) else {
        return;
    };
    let Some(chem) = chem(world) else {
        return;
    };
    if is_under(&path, SLEEPER) {
        let patient = world
            .get::<MechSleeper>(equip)
            .and_then(|s| s.patient)
            .map(|p| patient_data(world, &chem, p));
        let injectible_reagents = (is_under(&path, MEDICAL_SLEEPER) && patient.is_some())
            .then(|| syringe_gun_of(world, mech))
            .flatten()
            .and_then(|g| world.get::<Reagents>(g))
            .map(|r| reagent_lines(&chem, &r.0));
        gear.sleeper = Some(SleeperGear {
            patient,
            injectible_reagents,
        });
    } else if is_under(&path, SYRINGE_GUN) {
        let state = world
            .get::<MechSyringeGun>(equip)
            .cloned()
            .unwrap_or_else(|| MechSyringeGun {
                known: [EPINEPHRINE, MULTIVER]
                    .iter()
                    .filter_map(|p| chem.id(p))
                    .collect(),
                ..default()
            });
        let holder = world.get::<Reagents>(equip).map(|r| r.0.clone());
        gear.syringe = Some(SyringeGear {
            mode: if state.analyze { "Analyze" } else { "Launch" }.to_owned(),
            syringe: state.syringes.len() as u32,
            max_syringe: MAX_SYRINGES as u32,
            reagents: holder.as_ref().map_or(0.0, |h| h.total_volume),
            total_reagents: MAX_VOLUME,
            analyzed_reagents: state
                .known
                .iter()
                .map(|&id| (chem.def(id).name.clone(), state.processed.contains(&id)))
                .collect(),
            contained_reagents: holder
                .as_ref()
                .map(|h| reagent_lines(&chem, h))
                .unwrap_or_default(),
        });
    }
}

fn patient_data(world: &World, chem: &ss13_reagents::Chemistry, patient: Entity) -> SleeperPatient {
    let health = world.get::<Health>(patient).cloned().unwrap_or_default();
    let state = match world.get::<Stat>(patient).copied().unwrap_or_default() {
        Stat::Conscious => "Stable",
        Stat::Dead => "*Dead*",
        Stat::SoftCrit | Stat::HardCrit => "Critical",
    };
    let core_temp = world
        .get::<crate::breath::Respiration>(patient)
        .map(|r| (r.bodytemperature - 273.15) as f32);
    SleeperPatient {
        patient_name: name_of(world, patient),
        patient_health: health.health() / health.max.max(1.0),
        patient_state: state.to_owned(),
        core_temp,
        brute_loss: health.brute,
        burn_loss: health.burn,
        toxin_loss: health.tox,
        oxygen_loss: health.oxy,
        contained_reagents: world
            .get::<MobReagents>(patient)
            .map(|r| reagent_lines(chem, &r.blood))
            .unwrap_or_default(),
        has_brain_damage: crate::medical::organs::state(
            world,
            patient,
            crate::medical::organs::Slot::Brain,
        )
        .is_some_and(|o| o.damage != 0.0),
        has_traumas: world
            .get::<crate::medical::traumas::Traumas>(patient)
            .is_some_and(|t| !t.0.is_empty()),
    }
}

/// `handle_ui_act` of the equipment (`equip_act`'s `gear_action`).
pub fn ui_act(
    world: &mut World,
    client: Entity,
    _actor: Entity,
    mech: Entity,
    equip: Entity,
    action: &str,
) {
    let Some(path) = equip_path(world, equip) else {
        return;
    };
    if is_under(&path, SLEEPER) {
        if action == "eject" {
            go_out(world, equip);
        } else if is_under(&path, MEDICAL_SLEEPER)
            && let Some(name) = action.strip_prefix("inject_reagent_")
        {
            inject_reagent(world, mech, equip, name);
        }
        return;
    }
    if !is_under(&path, SYRINGE_GUN) {
        return;
    }
    ensure_gun(world, equip);
    if action == "change_mode" {
        if let Some(mut s) = world.get_mut::<MechSyringeGun>(equip) {
            s.analyze = !s.analyze;
        }
        return;
    }
    if action == "purge_all" {
        if let Some(mut r) = world.get_mut::<Reagents>(equip) {
            r.0.clear();
        }
        return;
    }
    let Some(chem) = chem(world) else {
        return;
    };
    let known = world
        .get::<MechSyringeGun>(equip)
        .map(|s| s.known.clone())
        .unwrap_or_default();
    for id in known {
        let name = &chem.def(id).name;
        if action.strip_prefix("purge_reagent_") == Some(name.as_str()) {
            if let Some(mut r) = world.get_mut::<Reagents>(equip) {
                let volume = r.0.amount(id);
                r.0.remove_reagent(id, volume);
            }
            return;
        }
        if action.strip_prefix("toggle_reagent_") == Some(name.as_str()) {
            toggle_synthesis(world, client, equip, id);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesis_splits_the_speed_between_reagents() {
        // `seconds_per_tick * synth_speed / LAZYLEN(processed_reagents)` per `SSobj` tick.
        let per_reagent = |n: usize| PROCESS_SECS as f32 * SYNTH_SPEED / n as f32;
        assert_eq!(per_reagent(1), 5.0);
        assert_eq!(per_reagent(2), 2.5);
    }

    #[test]
    fn the_dart_is_a_six_brute_bullet() {
        let d = dart_data();
        assert_eq!(d.path, DART);
        assert_eq!(d.damage, 6.0);
        assert_eq!(d.armor_flag, "bullet");
    }
}
