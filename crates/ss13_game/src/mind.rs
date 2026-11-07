//! The mind (server), after tg `/datum/mind` (`code/datums/mind/_mind.dm`, `initialization.dm`).
//!
//! - [`MindData`] is the player's mind on the mob it belongs to (tg `mind.current`): the key, the
//!   name, the death counters and the time of the last death. A mob that gets a client without
//!   a mind gets a new one (tg `mind_initialize` from `Login`); a ghost has none (its mind stays
//!   with the body it left, tg `get_ghost`).
//! - [`transfer`] is tg `transfer_to` for the mind's data: [`MindData`] and what lives on the
//!   mind in tg (addictions, skills, the traitor, blood brother, changeling, vampire, cultist,
//!   heretic and revolutionary datums with their HUDs and buttons) move from the old mob to the
//!   new one; teams and objectives that named the old mob name the new one (tg objectives
//!   target minds). The client follows in
//!   `crate::medical::brainmob::transfer_mind` (brain removal and insertion, bitrunning) and in
//!   the AI shell deploy (`crate::silicon::ai_shell`). A client that only takes another mob
//!   (tg `key = ...`: ghost possession, imaginary friend, split personality) takes no mind.
//! - [`get_ghost`] / [`grab_ghost`]: the ghost of a body, also a DNR one when forced (tg
//!   `even_if_they_cant_reenter`), back into its body.
//! - [`witnessed_death`] / [`experienced_death`]: every human death seen (also the own death)
//!   lowers the mind's `desensitized_level` by 0.025 down to 0.1, which scales the `saw_death`
//!   moodlet (`crate::mood::send_death_moodlets`).
// shortcut: no `/datum/memory` list, no mind languages, no `original_character`, no observer
// transfer and no `antag_hud` alt appearance re-creation (the antag HUD is a window here and
// reopens with the body's HUDs); the malfunctioning AI's datum stays on its core (its hacked
// APCs and powers belong to the core, the mind rides the eye); the wizard's, nuclear
// operative's and the event antags' datums stay on their body; a moved cultist's Nar'Sie
// language and red eyes stay with the old body.

use bevy::prelude::*;
use ss13_net::server::ClientToken;

use ss13_shared::admin::{AntagRole, MindPanel};

use crate::antag::AntagHudData;
use crate::antag::brother::{Brother, BrotherTeam};
use crate::antag::changeling::ChangelingHudData;
use crate::antag::changeling::server::{Changeling, LingObjective};
use crate::antag::cult::server::{CultInfo, CultObjective, CultTeam, Cultist};
use crate::antag::heretic::server::Heretic;
use crate::antag::objectives::{Objective, ObjectiveKind};
use crate::antag::revolution::server::{RevInfo, RevolutionHandler, Revolutionary};
use crate::antag::traitor::Traitor;
use crate::antag::vampire::VampireHudData;
use crate::antag::vampire::server::{Vampire, VampireObjective};
use crate::death::TimeOfDeath;
use crate::ghost::Ghost;
use crate::server::PlayerMob;

/// `DESENSITIZED_MINIMUM`, `DESENSITIZED_REDUCTION_PER_DEATH`, `DESENSITIZED_THRESHOLD`.
pub const DESENSITIZED_MINIMUM: f32 = 0.1;
pub const DESENSITIZED_REDUCTION_PER_DEATH: f32 = 0.025;
pub const DESENSITIZED_THRESHOLD: f32 = 0.5;

/// tg `/datum/mind`, on its `current` mob.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct MindData {
    /// `key`: the player's handshake token.
    pub key: String,
    /// `name`: the mob's name when the mind was made.
    pub name: String,
    /// `desensitized_level`: the multiplier of death moodlets.
    pub desensitized_level: f32,
    /// `deaths_witnessed`.
    pub deaths_witnessed: u32,
    /// `last_death`: server seconds of the last death of the mind's mob (`set_death_time`).
    pub last_death: Option<f64>,
}

impl MindData {
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            desensitized_level: 1.0,
            deaths_witnessed: 0,
            last_death: None,
        }
    }
}

/// On a ghost that may not re-enter its body (tg `can_reenter_corpse = FALSE`, DNR): the body
/// whose mind it is, for a forced [`grab_ghost`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DnrBody(pub Entity);

pub struct MindPlugin;

impl Plugin for MindPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (mind_initialize, set_death_time));
    }
}

/// tg `mind_initialize` from `Login`: a mob with a client and no mind gets one.
fn mind_initialize(
    mobs: Query<(Entity, &PlayerMob, Option<&Name>), (Added<PlayerMob>, Without<MindData>)>,
    ghosts: Query<(), With<Ghost>>,
    tokens: Query<&ClientToken>,
    mut commands: Commands,
) {
    for (mob, player, name) in &mobs {
        if ghosts.contains(mob) {
            continue;
        }
        let key = tokens
            .get(player.client)
            .map(|t| t.0.clone())
            .unwrap_or_default();
        let name = name.map(|n| n.to_string()).unwrap_or_default();
        commands
            .entity(mob)
            .try_insert_if_new(MindData::new(key, name));
    }
}

/// tg `set_death_time` on `COMSIG_LIVING_DEATH` of the mind's mob.
fn set_death_time(mut minds: Query<(&TimeOfDeath, &mut MindData), Added<TimeOfDeath>>) {
    for (died, mut mind) in &mut minds {
        mind.last_death = Some(died.0);
    }
}

fn move_component<T: Component>(world: &mut World, from: Entity, to: Entity) {
    let Ok(mut old) = world.get_entity_mut(from) else {
        return;
    };
    if let Some(c) = old.take::<T>()
        && let Ok(mut new) = world.get_entity_mut(to)
    {
        new.insert(c);
    }
}

fn retarget(objectives: &mut [Objective], from: Entity, to: Entity) {
    for o in objectives {
        if let ObjectiveKind::Assassinate { target } | ObjectiveKind::Maroon { target } =
            &mut o.kind
            && *target == Some(from)
        {
            *target = Some(to);
        }
    }
}

/// tg `transfer_to` for the mind's data: what the mind of `from` holds moves to `to` (whose own
/// mind, if any, is left without a body: tg `new_character.mind.set_current(null)`), and the
/// objectives targeting `from` target `to`. Nothing when `from` has no mind or is `to`.
pub fn transfer(world: &mut World, from: Entity, to: Entity) {
    if from == to || world.get::<MindData>(from).is_none() || world.get_entity(to).is_err() {
        return;
    }
    if let Ok(mut new) = world.get_entity_mut(to) {
        new.remove::<MindData>();
    }
    move_component::<MindData>(world, from, to);
    move_component::<crate::addictions::Addictions>(world, from, to);
    move_component::<crate::skills::Skills>(world, from, to);
    // The antag HUD goes with the mind's datums (other datums, such as the wizard's or a
    // nuclear operative's, stay on the body with theirs).
    let hud = has_mind_antag(world, from);
    // Datums with action buttons: the old body loses them, the new one gets them (`refresh`).
    let buttons = world.get::<Changeling>(from).is_some()
        || world.get::<Vampire>(from).is_some()
        || world.get::<Cultist>(from).is_some()
        || world.get::<Heretic>(from).is_some();
    move_component::<Traitor>(world, from, to);
    move_component::<Brother>(world, from, to);
    move_component::<Changeling>(world, from, to);
    move_component::<ChangelingHudData>(world, from, to);
    move_component::<Vampire>(world, from, to);
    move_component::<VampireHudData>(world, from, to);
    move_component::<Cultist>(world, from, to);
    move_component::<CultInfo>(world, from, to);
    move_component::<Heretic>(world, from, to);
    move_component::<Revolutionary>(world, from, to);
    move_component::<RevInfo>(world, from, to);
    if hud {
        move_component::<AntagHudData>(world, from, to);
    }
    // What was under way belongs to the old body (tg `on_body_transfer` ends it).
    if let Some(mut ling) = world.get_mut::<Changeling>(to) {
        ling.absorbing = None;
    }
    if let Some(mut vamp) = world.get_mut::<Vampire>(to) {
        vamp.draining = None;
        vamp.aiming = None;
        vamp.cloaking = false;
        vamp.cloak_alpha = None;
    }
    if let Some(mut cultist) = world.get_mut::<Cultist>(to) {
        cultist.carving = None;
        cultist.scribing = None;
        // The new body's eyes were never turned red.
        cultist.own_eyes = None;
    }
    // Cost: one pass over the antagonists and teams per transfer (rare).
    for mut traitor in world.query::<&mut Traitor>().iter_mut(world) {
        retarget(&mut traitor.objectives, from, to);
    }
    for mut brother in world.query::<&mut Brother>().iter_mut(world) {
        retarget(&mut brother.objectives, from, to);
    }
    for mut team in world.query::<&mut BrotherTeam>().iter_mut(world) {
        retarget(&mut team.objectives, from, to);
        swap(&mut team.members, from, to);
    }
    for mut ling in world.query::<&mut Changeling>().iter_mut(world) {
        for o in &mut ling.objectives {
            match o {
                LingObjective::EscapeIdentity { target, .. } if *target == Some(from) => {
                    *target = Some(to);
                }
                LingObjective::Generic(o) => retarget(std::slice::from_mut(o), from, to),
                _ => {}
            }
        }
    }
    for mut vamp in world.query::<&mut Vampire>().iter_mut(world) {
        for o in &mut vamp.objectives {
            match o {
                VampireObjective::Protect { target, .. } if *target == Some(from) => {
                    *target = Some(to);
                }
                VampireObjective::Generic(o) => retarget(std::slice::from_mut(o), from, to),
                _ => {}
            }
        }
    }
    if let Some(mut team) = world.get_resource_mut::<CultTeam>() {
        swap(&mut team.members, from, to);
        if team.leader == Some(from) {
            team.leader = Some(to);
        }
        for o in &mut team.objectives {
            if let CultObjective::Sacrifice { target, .. } = o
                && *target == Some(from)
            {
                *target = Some(to);
            }
        }
    }
    if let Some(mut handler) = world.get_resource_mut::<RevolutionHandler>() {
        swap(&mut handler.members, from, to);
        swap(&mut handler.targets, from, to);
    }
    if buttons {
        crate::actions::set_buttons(world, from, Vec::new());
        crate::antag::changeling::server::refresh(world, to);
        crate::antag::vampire::server::refresh(world, to);
        crate::antag::cult::server::refresh(world, to);
        crate::antag::heretic::server::refresh(world, to);
    }
    info!("mind of {from} transferred to {to}");
}

fn swap(list: &mut [Entity], from: Entity, to: Entity) {
    for e in list.iter_mut().filter(|e| **e == from) {
        *e = to;
    }
}

/// `mob` holds an antag datum that lives on the mind here.
pub fn has_mind_antag(world: &World, mob: Entity) -> bool {
    let Ok(e) = world.get_entity(mob) else {
        return false;
    };
    e.contains::<Traitor>()
        || e.contains::<Brother>()
        || e.contains::<Changeling>()
        || e.contains::<Vampire>()
        || e.contains::<Cultist>()
        || e.contains::<Heretic>()
        || e.contains::<Revolutionary>()
}

/// tg `get_ghost(even_if_they_cant_reenter)`: the ghost that left `body`; with `force` also the
/// ghost that gave up on it (DNR).
pub fn get_ghost(world: &mut World, body: Entity, force: bool) -> Option<Entity> {
    if let Some(ghost) = crate::medical::brainmob::ghost_of(world, body) {
        return Some(ghost);
    }
    if !force {
        return None;
    }
    world
        .query::<(Entity, &DnrBody)>()
        .iter(world)
        .find(|(_, d)| d.0 == body)
        .map(|(e, _)| e)
}

/// tg `grab_ghost(force)`: the ghost of `body` (with `force` also a DNR one) re-enters it.
/// Returns the ghost that went back.
pub fn grab_ghost(world: &mut World, body: Entity, force: bool) -> Option<Entity> {
    let ghost = get_ghost(world, body, force)?;
    world.get::<PlayerMob>(ghost)?;
    if let Some(mut g) = world.get_mut::<Ghost>(ghost) {
        g.body = Some(body);
    }
    if let Ok(mut e) = world.get_entity_mut(ghost) {
        e.remove::<DnrBody>();
    }
    crate::ghost::return_to_body(world, body);
    world.get_entity(ghost).is_err().then_some(ghost)
}

/// tg `witnessed_death`: `viewer`'s mind saw the human `dead` die.
pub fn witnessed_death(world: &mut World, viewer: Entity, dead: Entity) {
    if world.get::<crate::human::HumanLook>(dead).is_none() {
        return;
    }
    let Some(mut mind) = world.get_mut::<MindData>(viewer) else {
        return;
    };
    mind.desensitized_level =
        (mind.desensitized_level - DESENSITIZED_REDUCTION_PER_DEATH).max(DESENSITIZED_MINIMUM);
    mind.deaths_witnessed += 1;
    // shortcut: no achievements, so no `desensitized` award.
}

/// tg `experienced_death`: the own death desensitizes too.
pub fn experienced_death(world: &mut World, mob: Entity) {
    witnessed_death(world, mob, mob);
}

/// The `saw_death` multiplier of `viewer`'s mind (`max(DESENSITIZED_MINIMUM, level || 1)`).
pub fn desensitized_level(world: &World, viewer: Entity) -> f32 {
    world
        .get::<MindData>(viewer)
        .map_or(1.0, |m| m.desensitized_level.max(DESENSITIZED_MINIMUM))
}

/// The panel's names of the revolution's datums (as the admin player list).
pub const HEAD_REV: &str = "Head Revolutionary";
pub const REV: &str = "Revolutionary";

/// tg `traitor_panel` of the mind on `mob` (`ckey` is the admin's to fill).
pub fn panel(world: &World, mob: Entity) -> MindPanel {
    let mut lines = Vec::new();
    let body = world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_default();
    let job = world
        .get::<crate::jobs::PlayerJob>(mob)
        .and_then(|j| {
            world
                .get_resource::<crate::jobs::Jobs>()?
                .data
                .jobs
                .get(j.0)
        })
        .map(|j| j.title.clone())
        .or_else(|| crate::noncrew_jobs::title_of(world, mob).map(str::to_owned))
        .unwrap_or_else(|| "нет".to_owned());
    lines.push(format!("Должность: {job}"));
    match world.get::<MindData>(mob) {
        Some(mind) => {
            lines.push(format!("Разум: {} (ключ {})", mind.name, mind.key));
            lines.push(format!("Тело: {body}"));
            lines.push(format!(
                "Увидено смертей: {}, десенсибилизация {:.3}",
                mind.deaths_witnessed, mind.desensitized_level
            ));
            lines.push(match mind.last_death {
                Some(t) => format!("Последняя смерть: {t:.0} с"),
                None => "Не умирал".to_owned(),
            });
        }
        None => lines.push(format!("Тело без разума: {body}")),
    }
    if let Some(addictions) = world.get::<crate::addictions::Addictions>(mob) {
        for (a, points) in &addictions.points {
            let active = if addictions.cycle(*a).is_some() {
                ", зависимость"
            } else {
                ""
            };
            lines.push(format!(
                "Очки зависимости {}: {points:.0}{active}",
                a.name()
            ));
        }
    }
    let mut antag = Vec::new();
    let mut objectives = Vec::new();
    if let Some(t) = world.get::<Traitor>(mob) {
        antag.push(crate::antag::ROLE_TRAITOR.to_owned());
        objectives.extend(t.objectives.iter().map(|o| o.explanation.clone()));
    }
    if let Some(b) = world.get::<Brother>(mob) {
        antag.push(crate::antag::ROLE_BROTHER.to_owned());
        objectives.extend(b.objectives.iter().map(|o| o.explanation.clone()));
    }
    // The other datums' objectives are their own types: listed, not edited here.
    if let Some(ling) = world.get::<Changeling>(mob) {
        antag.push(crate::antag::changeling::ROLE_CHANGELING.to_owned());
        for o in &ling.objectives {
            lines.push(format!("Цель генокрада: {}", o.explanation()));
        }
    }
    if let Some(vamp) = world.get::<Vampire>(mob) {
        antag.push(crate::antag::vampire::ROLE_VAMPIRE.to_owned());
        for o in &vamp.objectives {
            lines.push(format!("Цель вампира: {}", o.explanation()));
        }
    }
    if world.get::<Cultist>(mob).is_some() {
        antag.push(crate::antag::cult::ROLE_CULTIST.to_owned());
    }
    if let Some(heretic) = world.get::<Heretic>(mob) {
        antag.push(crate::antag::heretic::ROLE_HERETIC.to_owned());
        for o in &heretic.objectives {
            lines.push(format!("Цель еретика: {}", o.explanation()));
        }
    }
    if let Some(rev) = world.get::<Revolutionary>(mob) {
        antag.push(if rev.head { HEAD_REV } else { REV }.to_owned());
    }
    MindPanel {
        ckey: String::new(),
        lines,
        antag,
        objectives,
    }
}

/// tg `obj_delete`: the objective at `index` of [`panel`]'s list. False when there is none.
pub fn remove_objective(world: &mut World, mob: Entity, index: usize) -> bool {
    let traitor = world.get::<Traitor>(mob).map_or(0, |t| t.objectives.len());
    if index < traitor {
        world
            .get_mut::<Traitor>(mob)
            .expect("counted above")
            .objectives
            .remove(index);
        return true;
    }
    match world.get_mut::<Brother>(mob) {
        Some(mut b) if index - traitor < b.objectives.len() => {
            b.objectives.remove(index - traitor);
            true
        }
        _ => false,
    }
}

/// tg `remove_antag_datum`: the datum of `role` leaves `mob`. The cult and the revolution use
/// their own deconversion; the others lose the datum, its HUD and buttons, and the antag HUD
/// when no mind datum is left. False when `mob` is not that role.
// shortcut: a removed changeling, vampire or heretic keeps the effects already on the body
// (cloak, organs, living heart); tg's `on_removal` undoes some of them.
pub fn remove_antag(world: &mut World, mob: Entity, role: AntagRole) -> bool {
    if world.get_entity(mob).is_err() {
        return false;
    }
    match role {
        AntagRole::Traitor => {
            if world.entity_mut(mob).take::<Traitor>().is_none() {
                return false;
            }
        }
        AntagRole::Brother => {
            let Some(brother) = world.entity_mut(mob).take::<Brother>() else {
                return false;
            };
            if let Some(mut team) = world.get_mut::<BrotherTeam>(brother.team) {
                team.members.retain(|&m| m != mob);
            }
        }
        AntagRole::Changeling => {
            if world.entity_mut(mob).take::<Changeling>().is_none() {
                return false;
            }
            world.entity_mut(mob).remove::<ChangelingHudData>();
            crate::actions::set_buttons(world, mob, Vec::new());
        }
        AntagRole::Vampire => {
            if world.entity_mut(mob).take::<Vampire>().is_none() {
                return false;
            }
            world.entity_mut(mob).remove::<VampireHudData>();
            crate::actions::set_buttons(world, mob, Vec::new());
        }
        AntagRole::Heretic => {
            if world.entity_mut(mob).take::<Heretic>().is_none() {
                return false;
            }
            crate::actions::set_buttons(world, mob, Vec::new());
        }
        AntagRole::Cultist => {
            if world.get::<Cultist>(mob).is_none() {
                return false;
            }
            crate::antag::cult::server::remove_cultist(world, mob, false);
            return true;
        }
        AntagRole::HeadRev | AntagRole::Revolutionary => {
            let head = matches!(role, AntagRole::HeadRev);
            if world
                .get::<Revolutionary>(mob)
                .is_none_or(|r| r.head != head)
            {
                return false;
            }
            return crate::antag::revolution::server::remove_revolutionary(
                world,
                mob,
                crate::antag::revolution::server::Deconverter::Admin,
            );
        }
    }
    if !has_mind_antag(world, mob) {
        world.entity_mut(mob).remove::<AntagHudData>();
    }
    info!("{mob} is no longer {role:?}");
    true
}

/// tg `obj_add` (a custom objective) to the traitor, else the blood brother. False for neither.
pub fn add_objective(world: &mut World, mob: Entity, text: String) -> bool {
    let objective = Objective {
        kind: ObjectiveKind::Custom,
        explanation: text,
    };
    if let Some(mut t) = world.get_mut::<Traitor>(mob) {
        t.objectives.push(objective);
        return true;
    }
    if let Some(mut b) = world.get_mut::<Brother>(mob) {
        b.objectives.push(objective);
        return true;
    }
    false
}

/// tg `obj_edit`: the explanation of the objective at `index` of [`panel`]'s list.
pub fn edit_objective(world: &mut World, mob: Entity, index: usize, text: String) -> bool {
    let traitor = world.get::<Traitor>(mob).map_or(0, |t| t.objectives.len());
    if index < traitor {
        world
            .get_mut::<Traitor>(mob)
            .expect("counted above")
            .objectives[index]
            .explanation = text;
        return true;
    }
    match world.get_mut::<Brother>(mob) {
        Some(mut b) if index - traitor < b.objectives.len() => {
            b.objectives[index - traitor].explanation = text;
            true
        }
        _ => false,
    }
}

/// tg `set_assigned_role` from the panel: the job titled `title` (any case). False when no
/// such job is loaded or `mob` has no job slot to set.
pub fn set_job(world: &mut World, mob: Entity, title: &str) -> bool {
    let Some(index) = world.get_resource::<crate::jobs::Jobs>().and_then(|jobs| {
        jobs.data
            .jobs
            .iter()
            .position(|j| j.title.eq_ignore_ascii_case(title))
    }) else {
        // Not a station job: a non-crew one (`/datum/job/space_wizard`, ...).
        let Some(job) = crate::noncrew_jobs::by_title(title) else {
            return false;
        };
        if world.get_entity(mob).is_err() {
            return false;
        }
        crate::noncrew_jobs::set_assigned_role(world, mob, job.path);
        return true;
    };
    let Ok(mut e) = world.get_entity_mut(mob) else {
        return false;
    };
    e.insert(crate::jobs::PlayerJob(index));
    e.remove::<crate::noncrew_jobs::NonCrewJob>();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_moves_the_mind_and_retargets_objectives() {
        let mut world = World::new();
        let old = world.spawn(MindData::new("p1", "Bob")).id();
        let new = world.spawn(MindData::new("", "brainmob")).id();
        crate::addictions::add_points(
            &mut world,
            old,
            crate::addictions::Addiction::Nicotine,
            700.0,
        );
        let hunter = world
            .spawn(Traitor {
                key: "p2".into(),
                employer: String::new(),
                flavor: Default::default(),
                objectives: vec![Objective {
                    kind: ObjectiveKind::Assassinate { target: Some(old) },
                    explanation: String::new(),
                }],
                ending: ObjectiveKind::Escape,
                uplink: None,
                code: None,
                unlock_text: String::new(),
                memory: String::new(),
                failsafe_code: None,
                can_assign_self_objectives: false,
            })
            .id();
        transfer(&mut world, old, new);
        assert!(world.get::<MindData>(old).is_none());
        assert_eq!(world.get::<MindData>(new).unwrap().key, "p1");
        assert_eq!(
            world
                .get::<crate::addictions::Addictions>(new)
                .unwrap()
                .points(crate::addictions::Addiction::Nicotine),
            700.0
        );
        assert!(world.get::<crate::addictions::Addictions>(old).is_none());
        assert_eq!(
            world.get::<Traitor>(hunter).unwrap().objectives[0].kind,
            ObjectiveKind::Assassinate { target: Some(new) }
        );
        // No mind, nothing moves.
        let other = world.spawn_empty().id();
        transfer(&mut world, old, other);
        assert!(world.get::<MindData>(new).is_some());

        // The admin panel: the hunter's objective listed, then removed with the datum.
        let p = panel(&world, hunter);
        assert_eq!(p.antag, vec![crate::antag::ROLE_TRAITOR.to_owned()]);
        assert_eq!(p.objectives.len(), 1);
        assert!(!remove_objective(&mut world, hunter, 1));
        assert!(remove_objective(&mut world, hunter, 0));
        assert!(panel(&world, hunter).objectives.is_empty());
        assert!(remove_antag(&mut world, hunter, AntagRole::Traitor));
        assert!(!remove_antag(&mut world, hunter, AntagRole::Traitor));
        assert!(panel(&world, hunter).antag.is_empty());
    }

    #[test]
    fn other_antags_travel_and_the_panel_edits_them() {
        let mut world = World::new();
        world.init_resource::<RevolutionHandler>();
        world.init_resource::<Time>();
        let old = world
            .spawn((
                MindData::new("p1", "Bob"),
                Vampire {
                    key: "p1".into(),
                    cloaking: true,
                    ..default()
                },
                Revolutionary {
                    head: false,
                    key: "p1".into(),
                },
                AntagHudData::default(),
            ))
            .id();
        world.resource_mut::<RevolutionHandler>().members.push(old);
        let new = world.spawn_empty().id();
        transfer(&mut world, old, new);
        assert!(!has_mind_antag(&world, old));
        assert!(world.get::<AntagHudData>(old).is_none());
        assert!(!world.get::<Vampire>(new).unwrap().cloaking);
        assert!(world.get::<Revolutionary>(new).is_some());
        assert_eq!(world.resource::<RevolutionHandler>().members, vec![new]);
        let p = panel(&world, new);
        assert!(
            p.antag
                .contains(&crate::antag::vampire::ROLE_VAMPIRE.to_owned())
        );
        assert!(p.antag.contains(&REV.to_owned()));
        assert!(p.lines.iter().any(|l| l == "Должность: нет"));

        // The vampire goes; the antag HUD stays for the revolutionary.
        assert!(remove_antag(&mut world, new, AntagRole::Vampire));
        assert!(!remove_antag(&mut world, new, AntagRole::Vampire));
        assert!(world.get::<AntagHudData>(new).is_some());
        // Neither traitor nor brother: no custom objective, no job without the jobs data.
        assert!(!add_objective(&mut world, new, "x".into()));
        assert!(!set_job(&mut world, new, "Assistant"));
        world.entity_mut(new).insert(Brother {
            key: "p1".into(),
            team: Entity::PLACEHOLDER,
            objectives: Vec::new(),
        });
        assert!(add_objective(&mut world, new, "Украсть луну.".into()));
        assert!(edit_objective(&mut world, new, 0, "Вернуть луну.".into()));
        assert!(!edit_objective(&mut world, new, 1, "-".into()));
        assert_eq!(
            panel(&world, new).objectives,
            vec!["Вернуть луну.".to_owned()]
        );
    }

    fn human() -> crate::human::HumanLook {
        crate::human::player_body(Entity::PLACEHOLDER, None)
    }

    #[test]
    fn deaths_desensitize_down_to_the_minimum() {
        let mut world = World::new();
        let viewer = world.spawn((MindData::new("p1", "A"), human())).id();
        let dead = world.spawn(human()).id();
        let monkey = world.spawn_empty().id();
        witnessed_death(&mut world, viewer, monkey);
        assert_eq!(world.get::<MindData>(viewer).unwrap().deaths_witnessed, 0);
        for _ in 0..40 {
            witnessed_death(&mut world, viewer, dead);
        }
        experienced_death(&mut world, viewer);
        let mind = world.get::<MindData>(viewer).unwrap();
        assert_eq!(mind.deaths_witnessed, 41);
        assert_eq!(mind.desensitized_level, DESENSITIZED_MINIMUM);
        assert_eq!(desensitized_level(&world, dead), 1.0);
    }
}
