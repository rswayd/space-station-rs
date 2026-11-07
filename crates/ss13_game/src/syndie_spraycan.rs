//! The seditious spray can (server), after tg `code/game/objects/items/syndie_spraycan.dm` and
//! `antagonists/traitor/components/demoraliser.dm` (`/datum/demoralise_moods/graffiti`).
//!
//! - `/obj/item/traitor_spraycan` on a turf with a clear open 3x3 around it draws the Syndicate
//!   rune in three steps of 3 s (outline, colour, final coating), on its rune it continues an
//!   unfinished one; then the can is spent.
//! - The finished rune shines: for 5 minutes cleaning does not take it off, then it is an
//!   ordinary hard decal. From then on it slips whoever moves next to it (6 s, not walking)
//!   and demoralises those who move within sight of it (7 tiles, conscious, not blind, once
//!   per mood): an antagonist feels bold (+2), command and security worried (-3), the crew
//!   uneasy (-2), for 2 minutes.
// shortcut: no tagger trait, examine demoralising, literacy/light check, traitor objective
// (`COMSIG_TRAITOR_GRAFFITI_DRAWN`/`_SLIPPED`, the traitor objectives' group), suicide face
// decal; "authority" is a head of staff or a mindshielded job instead of the department
// bitflags; flying mobs slip too.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmTurf, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_world::Moved;

use crate::construction::{Job, path_of, turf_at};
use crate::crayons::CrayonJob;
use crate::health::{Health, Stat};
use crate::janitor::{CLEAN_TYPE_HARD_DECAL, Cleanable};
use crate::mood::{Mood, MoodEvent};
use crate::obj_damage::is_under;
use crate::tools::{Actor, DoAfter, do_after, tell};

pub const TRAITOR_SPRAYCAN: &str = "/obj/item/traitor_spraycan";
/// The rune it draws (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
pub const TRAITOR_RUNE: &str = "/obj/effect/decal/cleanable/traitor_rune";
const RUNE_DMI: &str = "icons/effects/96x96.dmi";
/// `SYNDIE_DRAW_TIME`, deciseconds.
const DRAW_DS: f32 = 30.0;
/// `protected_timer`.
const PROTECTED_SECS: f64 = 300.0;
/// `slip_time`.
const SLIP_SECS: f32 = 6.0;
/// The demoraliser's range.
const DEMORALISE_RANGE: u32 = 7;
const NO_DRAW_TURFS: [&str; 4] = [
    "/turf/open/space",
    "/turf/open/openspace",
    "/turf/open/lava",
    "/turf/open/chasm",
];
const MOOD_CATEGORY: &str = "evil graffiti";
const ANTAG_MOOD: MoodEvent = MoodEvent {
    id: "traitor_graffiti_antag",
    description: "Логотип Синдиката? Как восхитительно смело.",
    mood_change: 2.0,
    timeout: 120.0,
    hidden: true,
};
const CREW_MOOD: MoodEvent = MoodEvent {
    id: "traitor_graffiti_crew",
    description: "Логотип Синдиката? Я точно здесь в безопасности?",
    mood_change: -2.0,
    timeout: 120.0,
    hidden: true,
};
const AUTH_MOOD: MoodEvent = MoodEvent {
    id: "traitor_graffiti_auth",
    description: "Кто из этих бездельников нарисовал логотип Синдиката?!",
    mood_change: -3.0,
    timeout: 120.0,
    hidden: true,
};

/// `expended` of a seditious spray can.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraitorCan {
    pub expended: bool,
}

/// `RUNE_STAGE_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuneStage {
    Outline,
    Coloured,
    Complete,
    Removable,
}

/// A Syndicate rune: its stage, and when it stops being clean-proof.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct TraitorRune {
    pub stage: RuneStage,
    pub removable_at: f64,
}

pub struct SyndieSpraycanPlugin;

impl Plugin for SyndieSpraycanPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_observer(add_can);
        app.add_click_handler(ClickPhase::HeldOnTarget, 4263, |w, c| {
            c.held
                .is_some_and(|held| interact(w, c.client, c.mob, held, c.target))
        });
        app.add_systems(
            Update,
            (
                protection_ends.run_if(any_with_component::<TraitorRune>),
                near_runes.after(ss13_world::WorldSystems::Movement),
            ),
        );
    }
}

fn add_can(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, TRAITOR_SPRAYCAN))
    {
        commands
            .entity(add.entity)
            .insert_if_new(TraitorCan::default());
    }
}

/// `traitor_spraycan/interact_with_atom`. True when the held item is the can.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(can) = world.get::<TraitorCan>(held).copied() else {
        return false;
    };
    if can.expended {
        tell(world, client, Span::Warning, "Краска закончилась...");
        return true;
    }
    if world.get::<DoAfter>(mob).is_some() {
        tell(world, client, Span::Warning, "Вы уже заняты!");
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if world.get::<TraitorRune>(target).is_some() {
        step(world, actor, target);
    } else if world.get::<ss13_mapload::Turf>(target).is_some()
        && let Some(&pos) = world.get::<TilePos>(target)
    {
        if !clear_area(world, pos) {
            tell(
                world,
                client,
                Span::Warning,
                "Нужна свободная площадка 3x3!",
            );
            return true;
        }
        tell(world, client, Span::Notice, "Вы рисуете контур...");
        let draw = DRAW_DS * crate::quirks::tagger_multiplier(world, mob);
        do_after(
            world,
            actor,
            target,
            draw,
            0,
            Job::Crayon(CrayonJob::Rune(None)),
        );
    }
    // Anything else: `ITEM_INTERACT_BLOCKING`.
    true
}

/// Every turf of the 3x3 is open and drawable on.
fn clear_area(world: &World, pos: TilePos) -> bool {
    (-1..=1).all(|dx| {
        (-1..=1).all(|dy| {
            let at = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            turf_at(world, at).is_some_and(|t| {
                world.get::<DmTurf>(t).is_none_or(|d| !d.closed)
                    && path_of(world, t).is_some_and(|p| {
                        is_under(&p, "/turf/open") && !NO_DRAW_TURFS.iter().any(|r| is_under(&p, r))
                    })
            })
        })
    })
}

/// `try_complete_rune`: the next step's `do_after` on the rune.
fn step(world: &mut World, actor: Actor, rune: Entity) {
    let Some(r) = world.get::<TraitorRune>(rune).copied() else {
        return;
    };
    match r.stage {
        RuneStage::Outline | RuneStage::Coloured => {
            let text = if r.stage == RuneStage::Outline {
                "...вы дорабатываете рисунок..."
            } else {
                "...вы наносите финальное покрытие..."
            };
            tell(world, actor.client, Span::Notice, text);
            // `TRAIT_TAGGER` (`crate::quirks`): twice as fast.
            let draw = DRAW_DS * crate::quirks::tagger_multiplier(world, actor.mob);
            do_after(
                world,
                actor,
                rune,
                draw,
                0,
                Job::Crayon(CrayonJob::Rune(Some(rune))),
            );
        }
        RuneStage::Complete | RuneStage::Removable => {
            tell(world, actor.client, Span::Notice, "Готово!");
        }
    }
}

/// A step done: the outline is drawn, or the rune moves to its next stage; then the next step.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, rune: Option<Entity>) {
    let actor = Actor {
        client: action.client,
        mob,
        item: action.item,
    };
    let Some(can) = actor
        .item
        .filter(|&i| world.get::<TraitorCan>(i).is_some_and(|c| !c.expended))
    else {
        return;
    };
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/spray.ogg", 5.0, true).extrarange(5.0),
        );
    }
    let rune = match rune {
        None => {
            if !clear_area(world, action.target_pos) {
                return;
            }
            let Some(rune) = crate::construction::spawn_obj(world, TRAITOR_RUNE, action.target_pos)
            else {
                return;
            };
            world.entity_mut(rune).insert((
                TraitorRune {
                    stage: RuneStage::Outline,
                    removable_at: 0.0,
                },
                Cleanable {
                    clean_type: CLEAN_TYPE_HARD_DECAL,
                },
            ));
            set_stage(world, rune, RuneStage::Outline);
            step(world, actor, rune);
            return;
        }
        Some(rune) => rune,
    };
    let Some(r) = world.get::<TraitorRune>(rune).copied() else {
        tell(world, actor.client, Span::Warning, "Граффити уничтожено!");
        return;
    };
    match r.stage {
        RuneStage::Outline => {
            set_stage(world, rune, RuneStage::Coloured);
            step(world, actor, rune);
        }
        RuneStage::Coloured => {
            tell(world, actor.client, Span::Notice, "Закончено!");
            set_stage(world, rune, RuneStage::Complete);
            world.entity_mut(can).insert(TraitorCan { expended: true });
            if let Some(mut name) = world.get_mut::<DmName>(can) {
                name.desc = "A suspicious looking spraycan, it's all out of paint.".to_owned();
            }
        }
        _ => {}
    }
}

/// `set_stage`: the sprite, the description, the protection timer.
pub fn set_stage(world: &mut World, rune: Entity, stage: RuneStage) {
    let (state, desc) = match stage {
        RuneStage::Outline => (
            "traitor_rune_outline",
            "It looks like it's going to be... the Syndicate logo?",
        ),
        RuneStage::Coloured | RuneStage::Removable => (
            "traitor_rune_done",
            "A large depiction of the Syndicate logo.",
        ),
        RuneStage::Complete => (
            "traitor_rune_sheen",
            "A large depiction of the Syndicate logo. It looks slippery.",
        ),
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut r) = world.get_mut::<TraitorRune>(rune) {
        r.stage = stage;
        if stage == RuneStage::Complete {
            r.removable_at = now + PROTECTED_SECS;
        }
    }
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|ix| ix.get(RUNE_DMI, state));
    if let Some(id) = id
        && let Some(mut a) = world.get_mut::<Appearance>(rune)
    {
        a.icon_state = id;
    }
    if let Some(mut name) = world.get_mut::<DmName>(rune) {
        name.desc = desc.to_owned();
    }
}

/// `wash()` of the rune: nothing while it is `clean_proof`.
pub fn clean_proof(world: &World, decal: Entity) -> bool {
    world
        .get::<TraitorRune>(decal)
        .is_some_and(|r| r.stage == RuneStage::Complete)
}

/// The 5-minute protection timer.
fn protection_ends(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &TraitorRune)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, r)| r.stage == RuneStage::Complete && now >= r.removable_at)
        .map(|(e, _)| e)
        .collect();
    for rune in due {
        set_stage(world, rune, RuneStage::Removable);
    }
}

/// The finished runes' proximity monitors: a living mob that moved next to one slips
/// (`HasProximity`), one that moved within its sight is demoralised (`field_turf_crossed`).
// Cost: per tick, the moved living mobs × the finished runes (a handful per round).
fn near_runes(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<(Entity, TilePos)> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| (m.entity, m.to))
        .collect();
    if moved.is_empty() {
        return;
    }
    let mut q = world.query::<(&TraitorRune, &TilePos)>();
    let runes: Vec<TilePos> = q
        .iter(world)
        .filter(|(r, _)| matches!(r.stage, RuneStage::Complete | RuneStage::Removable))
        .map(|(_, &p)| p)
        .collect();
    if runes.is_empty() {
        return;
    }
    for (mob, to) in moved {
        if world.get::<Health>(mob).is_none() {
            continue;
        }
        for &rune in &runes {
            let Some(dist) = to.chebyshev(rune) else {
                continue;
            };
            if dist <= 1 {
                crate::janitor::try_slip(world, mob, (SLIP_SECS, true));
            }
            if dist <= DEMORALISE_RANGE
                && crate::mobs::ai::behaviors::can_see(world, to, rune, DEMORALISE_RANGE)
            {
                demoralise(world, mob);
            }
        }
    }
}

/// `demoraliser/on_seen`.
fn demoralise(world: &mut World, viewer: Entity) {
    let Some(client) = world
        .get::<crate::server::PlayerMob>(viewer)
        .map(|p| p.client)
    else {
        return;
    };
    if world.get::<Stat>(viewer) != Some(&Stat::Conscious) || crate::vision::is_blind(world, viewer)
    {
        return;
    }
    if world
        .get::<Mood>(viewer)
        .is_none_or(|m| m.get(MOOD_CATEGORY).is_some())
    {
        return;
    }
    use crate::antag::revolution::server::{is_head_of_staff, mindshielded};
    let (text, event) = if world.get::<crate::antag::AntagHudData>(viewer).is_some() {
        ("Трехглавая змея. Заебись.", &ANTAG_MOOD)
    } else if is_head_of_staff(world, viewer) || mindshielded(world, viewer) {
        ("Трехглавая змея означает только неприятности.", &AUTH_MOOD)
    } else {
        ("Это... трёхголовая змея?", &CREW_MOOD)
    };
    tell(world, client, Span::Notice, text);
    crate::mood::add_event(world, viewer, MOOD_CATEGORY, event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_finished_rune_is_clean_proof() {
        let mut world = World::new();
        let rune = world
            .spawn(TraitorRune {
                stage: RuneStage::Coloured,
                removable_at: 0.0,
            })
            .id();
        assert!(!clean_proof(&world, rune));
        world.get_mut::<TraitorRune>(rune).unwrap().stage = RuneStage::Complete;
        assert!(clean_proof(&world, rune));
        world.get_mut::<TraitorRune>(rune).unwrap().stage = RuneStage::Removable;
        assert!(!clean_proof(&world, rune));
    }
}
