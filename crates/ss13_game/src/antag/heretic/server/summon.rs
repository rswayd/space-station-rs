//! Summoning rituals (`/datum/heretic_knowledge/summon`, `summon_ritual_mob` in
//! `heretic_knowledge.dm`) and the summoned (`/datum/antagonist/heretic_monster`,
//! `heretic_monsters.dm`; the mobs are `code/modules/mob/living/basic/heretic/`).
//!
//! The ritual spawns its mob on the rune half-transparent and polls ghosts for 10 s
//! ([`crate::ghost_poll`]). With a volunteer the mob becomes theirs and serves the heretic
//! ([`HereticMonster`]); the ritual's atoms are consumed then. Without one the mob goes and the
//! atoms stay. The rune is busy during the poll.
// shortcut: the summons are their basic mob definitions (stats, attacks, speed) without their
// abilities: no raw prophet telepathic link and far sight, no flesh stalker disguise, no maid's
// mirror walk or void chill, no rust walker rusting, no ash spirit jaunt/cascade, no fire shark
// phlogiston, no Armsy segments (the Flesh ascension) or Star Gazer (the Cosmic ascension). No
// `heretic_summon` objective, no fade-in animation, moodlet or suicide cry.

use super::*;
use crate::ghost_poll::{self, PollSpec};

/// `summon_ritual_mob`'s `poll_time`.
pub const SUMMON_POLL_SECS: f64 = 10.0;

/// `/datum/antagonist/heretic_monster` on the summoned mob: its master's body.
#[derive(Component, Clone, Copy, Debug)]
pub struct HereticMonster {
    pub master: Entity,
}

/// The summoned mob waiting for its ghost (`TRAIT_NO_TRANSFORM`, alpha 155).
#[derive(Component, Clone, Copy, Debug)]
pub struct PendingSummon;

/// `IS_HERETIC_OR_MONSTER`.
pub fn is_heretic_or_monster(world: &World, mob: Entity) -> bool {
    is_heretic(world, mob) || world.get::<HereticMonster>(mob).is_some()
}

/// `mob_to_summon` and `poll_ignore_define` of each summon knowledge.
pub fn summon_of(knowledge: &str) -> Option<(&'static str, &'static str)> {
    Some(match knowledge {
        "/datum/heretic_knowledge/summon/raw_prophet" => (
            "/mob/living/basic/heretic_summon/raw_prophet",
            "raw_prophet",
        ),
        "/datum/heretic_knowledge/summon/stalker" => {
            ("/mob/living/basic/heretic_summon/stalker", "stalker")
        }
        "/datum/heretic_knowledge/summon/rusty" => (
            "/mob/living/basic/heretic_summon/rust_walker",
            "rust_spirit",
        ),
        "/datum/heretic_knowledge/summon/maid_in_mirror" => (
            "/mob/living/basic/heretic_summon/maid_in_the_mirror",
            "maid_in_mirror",
        ),
        "/datum/heretic_knowledge/summon/ashy" => {
            ("/mob/living/basic/heretic_summon/ash_spirit", "ash_spirit")
        }
        "/datum/heretic_knowledge/summon/fire_shark" => {
            ("/mob/living/basic/heretic_summon/fire_shark", "fire_shark")
        }
        _ => return None,
    })
}

/// `summon/on_finished_recipe` -> `summon_ritual_mob`. The atoms are consumed when a ghost
/// takes the mob; false when the ritual fails at once.
#[allow(clippy::too_many_arguments)]
pub(super) fn begin_summon(
    world: &mut World,
    client: Entity,
    user: Entity,
    rune: Entity,
    def: &'static KnowledgeDef,
    atoms: Vec<Entity>,
    stacks: Vec<(Entity, u32)>,
    loc: TilePos,
) -> bool {
    let Some((mob_path, ignore)) = summon_of(def.path) else {
        return false;
    };
    if !world.contains_resource::<ghost_poll::Polls>() {
        tell(
            world,
            client,
            Span::Warning,
            "ритуал провален, нет призраков!",
        );
        return false;
    }
    let Some(summoned) = crate::mobs::spawn::spawn_mob(world, mob_path, loc) else {
        warn!("heretic: no prototype for {mob_path}");
        tell(
            world,
            client,
            Span::Warning,
            "ритуал провален, нет призраков!",
        );
        return false;
    };
    world.entity_mut(summoned).insert(PendingSummon);
    if let Some(mut appearance) = world.get_mut::<ss13_shared::Appearance>(summoned) {
        let [r, g, b, _] = appearance.color.unwrap_or([255; 4]);
        appearance.color = Some([r, g, b, 155]);
    }
    if let Some(mut r) = world.get_mut::<TransmutationRune>(rune) {
        r.in_use = true;
    }
    let name = name_of(world, summoned);
    info!(
        "heretic: a {name} is being summoned by {} at {loc:?}",
        name_of(world, user)
    );
    let spec = PollSpec {
        question: format!("Вы хотите сыграть за {name}?"),
        role_name: name,
        secs: SUMMON_POLL_SECS,
        ignore: Some(ignore),
        target: Some(summoned),
        ..default()
    };
    let group = ghost_poll::ghosts(world);
    ghost_poll::poll_candidates(world, spec, group, move |world, mut signed_up| {
        if let Some(mut r) = world.get_mut::<TransmutationRune>(rune) {
            r.in_use = false;
        }
        let chosen = ghost_poll::pick_candidate(world, &mut signed_up);
        finish_summon(world, client, user, summoned, chosen, &atoms, &stacks);
    });
    true
}

/// After the poll: the chosen ghost takes the mob and serves `user`, or the mob goes.
fn finish_summon(
    world: &mut World,
    client: Entity,
    user: Entity,
    summoned: Entity,
    chosen: Option<ghost_poll::Candidate>,
    atoms: &[Entity],
    stacks: &[(Entity, u32)],
) {
    if world.get_entity(summoned).is_err() {
        return;
    }
    let taken = chosen.is_some_and(|c| {
        world.get_entity(user).is_ok()
            && crate::medical::brainmob::possess(world, c.client, c.mob, summoned)
    });
    if !taken {
        tell(
            world,
            client,
            Span::Warning,
            "ритуал провален, нет призраков!",
        );
        despawn(world, summoned);
        return;
    }
    if let Some(c) = chosen
        && world.get_entity(c.mob).is_ok()
    {
        world.despawn(c.mob);
    }
    // `antag_hud_name = "heretic_beast"`.
    let hud_icon = icon(world, ANTAG_HUD_DMI, "heretic_beast");
    world
        .entity_mut(summoned)
        .remove::<PendingSummon>()
        .insert((
            HereticMonster { master: user },
            AntagHudData {
                hud_icon,
                ..default()
            },
        ));
    if let Some(mut appearance) = world.get_mut::<ss13_shared::Appearance>(summoned) {
        if let Some(color) = appearance.color.as_mut() {
            color[3] = 255;
        }
        if appearance.color == Some([255; 4]) {
            appearance.color = None;
        }
    }
    // `cleanup_atoms` of the ritual, now that it succeeded.
    for &e in atoms {
        if world.get_entity(e).is_ok() && !is_living(world, e) {
            despawn(world, e);
        }
    }
    for &(stack, amount) in stacks {
        if world.get_entity(stack).is_ok() {
            crate::tools::use_stack(world, stack, amount);
        }
    }
    let master = name_of(world, user);
    info!("heretic: {master} created a {}", name_of(world, summoned));
    // `set_owner`: the objective and the greeting.
    chat_to(world, summoned, Span::Bold, "Ваши текущие задачи:");
    chat_to(
        world,
        summoned,
        Span::Plain,
        "Задача #1: Помогайте своему хозяину.",
    );
    let what = if world.get::<HumanLook>(summoned).is_some() {
        "возвращённый труп"
    } else {
        "ужасное создание, принесённое"
    };
    chat_to(
        world,
        summoned,
        Span::BoldNotice,
        format!("Вы - {what} в этот мир через врата Мансуса."),
    );
    chat_to(
        world,
        summoned,
        Span::Notice,
        format!("Твой хозяин - {master}. Помогай ему во всех деяниях."),
    );
    tell(world, client, Span::Notice, "ритуал завершен");
}
