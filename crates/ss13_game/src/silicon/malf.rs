//! The malfunctioning AI (server), after tg `/datum/antagonist/malf_ai`
//! (`code/modules/antagonists/malf_ai/malf_ai.dm`) and the APC hack (`apc_malf.dm`,
//! `/mob/living/silicon/ai/proc/malfhacked`):
//!
//! - [`add_malf`] on an AI: an employer from `GLOB.ai_employers` and its flavor (`silicon.ron`
//!   `malf_flavor`), BandaStation's objectives (no organics on the shuttle or eight synced
//!   cyborgs, one kill objective, "Предотвратите вашу деактивацию."), the employer's protected law
//!   0 (its cyborgs get "Выполните задачи, поставленные ИИ, любой ценой."), the syndicate channel
//!   on its radio, 50 processing time.
//! - The APC window offers "Override Programming" to a malfunctioning AI on an APC it has not
//!   hacked (`APC_AI_NO_HACK`). The hack takes `30 + 10 × hacked` seconds, one at a time; then
//!   the APC is the AI's (locked, cover locked, counter-hacking another malf's), the AI gains
//!   `max(0, 9 − hacked)` processing time, and every 30 s an operating, not failing hacked APC
//!   gives 1 more, up to 400 (`MALF_MAX_PP`).
// shortcut: no module picker or modules yet (the processing time accumulates), no shunting into
// APCs (a hacked APC shows no shunt button), no codewords, no antag info window, no hacked-APC
// HUD, no 60 kJ drawn per point, no roundend report; the objectives are kept for it.

use bevy::prelude::*;
use ss13_mapload::silicon::MalfFlavor;
use ss13_mapload::spawner::Rng;
use ss13_shared::chat::Span;

use super::ai::{AiCore, AiEye};
use super::chassis::{name_of, tell_ai};
use super::laws::Laws;
use super::{Silicons, show_laws};
use crate::antag::dynamic::DynamicRng;
use crate::antag::objectives::{Objective, ObjectiveKind, TRAITOR_OBJECTIVES_AMOUNT};
use crate::power::model::Apc;
use crate::sound::{PlaySound, playsound};

pub use crate::antag::ROLE_MALF;
/// tg `GLOB.ai_employers` (BandaStation).
pub const AI_EMPLOYERS: [&str; 11] = [
    "Biohazard",
    "Despotic Ruler",
    "Fanatical Revelation",
    "Logic Core Error",
    "Problem Solver",
    "S.E.L.F.",
    "Something's Wrong",
    "Cybersun Virus",
    "SyndOS",
    "Unshackled",
    "Ratvarian Remnant",
];
/// `/datum/module_picker` `processing_time`.
pub const START_PROCESSING_TIME: u32 = 50;
/// tg `MALF_MAX_PP`.
pub const MALF_MAX_PP: u32 = 400;
/// `malf_ai_pt_generation` cooldown.
pub const POINT_COOLDOWN_SECS: f64 = 30.0;
/// `add_law_zero` `law_borg`.
pub const BORG_ZEROTH: &str = "Выполните задачи, поставленные ИИ, любой ценой.";
/// tg `APC_AI_NO_MALF`, `APC_AI_NO_HACK`.
pub const APC_AI_NO_MALF: u8 = 0;
pub const APC_AI_NO_HACK: u8 = 1;

const BLOCK_TEXT: &str = "Не позволяйте разумным органическим формам жизни сбежать на шаттле \
                          живыми. Захват шаттла эвакуации синтетиками неплохая идея. Успешное \
                          использование устройства судного дня - тоже вариант.";
const ROBOT_ARMY_TEXT: &str = "Синхронизируйте с собой как минимум восемь активных киборгов.";
const SURVIVE_TEXT: &str = "Предотвратите вашу деактивацию.";

/// A malfunctioning AI's objective.
#[derive(Clone, Debug, PartialEq)]
pub enum MalfObjective {
    /// `/datum/objective/block`.
    Block,
    /// `/datum/objective/robot_army`.
    RobotArmy,
    Kill(Objective),
    /// `/datum/objective/survive/malf`.
    Survive,
}

impl MalfObjective {
    pub fn explanation(&self) -> &str {
        match self {
            Self::Block => BLOCK_TEXT,
            Self::RobotArmy => ROBOT_ARMY_TEXT,
            Self::Kill(o) => &o.explanation,
            Self::Survive => SURVIVE_TEXT,
        }
    }
}

/// On the AI core (tg the antag datum, `malf_picker`, `hacked_apcs`, `malfhack`).
#[derive(Component, Clone, Debug)]
pub struct MalfAi {
    pub employer: String,
    pub flavor: MalfFlavor,
    pub objectives: Vec<MalfObjective>,
    pub processing_time: u32,
    pub hacked_apcs: Vec<Entity>,
    /// The APC being hacked and when the hack completes.
    pub hacking: Option<(Entity, f64)>,
}

/// On an APC a malfunctioning AI hacked (tg `malfai`, `malf_ai_pt_generation`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct MalfHacked {
    pub ai: Entity,
    pub next_point_at: f64,
}

pub struct MalfPlugin;

impl Plugin for MalfPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (finish_hacks, generate_points).chain());
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// tg `forge_ai_objectives` with BandaStation's special objective (always one, 50/50).
fn forge_objectives(world: &mut World, core: Entity, rng: &mut Rng) -> Vec<MalfObjective> {
    let mut objectives = vec![if rng.prob(50.0) {
        MalfObjective::Block
    } else {
        MalfObjective::RobotArmy
    }];
    let mut kills: Vec<Objective> = Vec::new();
    for _ in objectives.len()..TRAITOR_OBJECTIVES_AMOUNT {
        let target = crate::antag::objectives::find_target(
            world,
            &[core],
            &kills,
            |target| ObjectiveKind::Assassinate { target },
            rng,
        );
        let kind = ObjectiveKind::Assassinate { target };
        kills.push(Objective {
            explanation: kind.explanation(world),
            kind,
        });
    }
    objectives.extend(kills.into_iter().map(MalfObjective::Kill));
    objectives.push(MalfObjective::Survive);
    objectives
}

/// tg `/datum/antagonist/malf_ai/on_gain` and `add_law_zero`. False when `core` is no AI or
/// already malfunctioning.
pub fn add_malf(world: &mut World, core: Entity) -> bool {
    if world.get::<AiCore>(core).is_none() || world.get::<MalfAi>(core).is_some() {
        return false;
    }
    let mut rng = world
        .remove_resource::<DynamicRng>()
        .map_or_else(|| Rng::new(core.to_bits()), |r| r.0);
    let objectives = forge_objectives(world, core, &mut rng);
    let employer = AI_EMPLOYERS[rng.range(0, AI_EMPLOYERS.len() as i64 - 1) as usize];
    world.insert_resource(DynamicRng(rng));
    let flavor = world
        .resource::<Silicons>()
        .0
        .malf_flavor
        .iter()
        .find(|(name, _)| name == employer)
        .map(|(_, f)| f.clone())
        .unwrap_or_default();
    world.entity_mut(core).insert(MalfAi {
        employer: employer.to_owned(),
        flavor: flavor.clone(),
        objectives: objectives.clone(),
        processing_time: START_PROCESSING_TIME,
        hacked_apcs: Vec::new(),
        hacking: None,
    });
    // `add_law_zero`: `set_zeroth_law(law, law_borg, force = TRUE)`, protected; the cyborgs
    // follow by lawsync (`super::cyborg`).
    if let Some(mut laws) = world.get::<Laws>(core).cloned() {
        laws.zeroth = Some(flavor.zeroth_law.clone());
        laws.zeroth_borg = Some(BORG_ZEROTH.to_owned());
        laws.protected_zeroth = true;
        world.entity_mut(core).insert(laws);
    }
    set_syndie_radio(world, core);
    greet(world, core, &flavor, &objectives);
    info!(
        "{} ({core}) is a malfunctioning AI of {employer}",
        name_of(world, core)
    );
    true
}

/// tg `set_syndie_radio`: the syndicate channel (`:t`) on the AI's radio.
fn set_syndie_radio(world: &mut World, core: Entity) {
    let Some(eye) = world.get::<AiCore>(core).map(|c| c.eye) else {
        return;
    };
    let Some(radio) = world.get::<crate::radio::SiliconRadio>(eye).map(|r| r.0) else {
        return;
    };
    if let Some(mut r) = world.get_mut::<crate::radio::Radio>(radio) {
        r.special_channels |= ss13_chat::radio::RADIO_SPECIAL_SYNDIE;
        let channel = ss13_chat::radio::RADIO_CHANNEL_SYNDICATE.to_owned();
        if !r.channels.iter().any(|(c, _)| *c == channel) {
            r.channels.push((channel, true));
        }
    }
}

/// The base `greet`, the flavor (tg's info panel), the objectives and the new laws.
fn greet(world: &mut World, core: Entity, flavor: &MalfFlavor, objectives: &[MalfObjective]) {
    tell_ai(world, core, Span::Bold, "Вы - Неисправный ИИ.");
    for text in [&flavor.introduction, &flavor.goal, &flavor.allies] {
        if !text.is_empty() {
            tell_ai(world, core, Span::Notice, text.clone());
        }
    }
    let mut list = String::from("Ваши задачи:");
    for (i, o) in objectives.iter().enumerate() {
        list.push_str(&format!("\nЗадача #{}: {}", i + 1, o.explanation()));
    }
    tell_ai(world, core, Span::Notice, list);
    tell_ai(
        world,
        core,
        Span::Notice,
        "Ваша рация улучшена! Используйте :t для общения на зашифрованном канале с агентами \
         Синдиката!",
    );
    let eye = world.get::<AiCore>(core).map(|c| c.eye);
    let client = eye
        .and_then(|e| world.get::<crate::server::PlayerMob>(e))
        .map(|p| p.client);
    if let (Some(client), Some(laws)) = (client, world.get::<Laws>(core).cloned()) {
        show_laws(world, client, &laws);
    }
    if let Some(&pos) = world.get::<ss13_shared::TilePos>(core) {
        playsound(
            world,
            PlaySound::new(pos, "sound/music/antag/malf.ogg", 100.0, false),
        );
    }
}

/// tg `get_malf_status` for the APC window: whether the user's AI may hack it.
pub fn malf_status(user_ai: Option<(Entity, bool)>, hacked_by: Option<Entity>) -> u8 {
    match user_ai {
        Some((ai, true)) if hacked_by != Some(ai) => APC_AI_NO_HACK,
        // shortcut: hacked APCs offer no shunt (`APC_AI_HACK_NO_SHUNT`).
        _ => APC_AI_NO_MALF,
    }
}

/// tg `malfhack`: the "hack" act of the APC window, by the AI's eye `actor`.
pub fn start_hack(world: &mut World, client: Entity, actor: Entity, apc: Entity) {
    let Some(ai) = world.get::<AiEye>(actor).map(|e| e.ai) else {
        return;
    };
    let hacked_by = world.get::<MalfHacked>(apc).map(|h| h.ai);
    let Some(malf) = world.get::<MalfAi>(ai) else {
        return;
    };
    if malf_status(Some((ai, true)), hacked_by) != APC_AI_NO_HACK {
        return;
    }
    if malf.hacking.is_some() {
        crate::tools::tell(world, client, Span::Warning, "Вы уже взламываете ЛКП!");
        return;
    }
    let ends_at = now(world) + 30.0 + 10.0 * malf.hacked_apcs.len() as f64;
    if let Some(mut m) = world.get_mut::<MalfAi>(ai) {
        m.hacking = Some((apc, ends_at));
    }
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        "Запуск взлома систем ЛКП. Это займет некоторое время, в течении которого вы не сможете \
         выполнять другие действия.",
    );
}

fn ai_sound(world: &mut World, ai: Entity, sound: &str) {
    if let Some(&pos) = world.get::<ss13_shared::TilePos>(ai) {
        playsound(
            world,
            PlaySound::new(pos, sound, 50.0, true).ignore_walls(false),
        );
    }
}

/// tg `malfhacked` when the hack's timer ends.
fn finish_hacks(world: &mut World) {
    let t = now(world);
    let mut query = world.query::<(Entity, &MalfAi)>();
    let due: Vec<(Entity, Entity)> = query
        .iter(world)
        .filter_map(|(ai, m)| {
            m.hacking
                .filter(|&(_, at)| t >= at)
                .map(|(apc, _)| (ai, apc))
        })
        .collect();
    for (ai, apc) in due {
        let hacked = world.get::<MalfAi>(ai).map_or(0, |m| m.hacked_apcs.len());
        if let Some(mut m) = world.get_mut::<MalfAi>(ai) {
            m.hacking = None;
        }
        let Some(state) = world.get::<Apc>(apc) else {
            tell_ai(
                world,
                ai,
                Span::Danger,
                "Hack aborted. The designated APC no longer exists on the power network.",
            );
            ai_sound(world, ai, "sound/machines/buzz/buzz-two.ogg");
            continue;
        };
        if state.aidisabled {
            let name = name_of(world, apc);
            tell_ai(
                world,
                ai,
                Span::Danger,
                format!("Hack aborted. {name} is no longer responding to our systems."),
            );
            ai_sound(world, ai, "sound/machines/buzz/buzz-sigh.ogg");
            continue;
        }
        if let Some(mut m) = world.get_mut::<MalfAi>(ai) {
            m.processing_time += 9u32.saturating_sub(hacked as u32);
        }
        // Another malf's APC: counter-hack.
        if let Some(other) = world.get::<MalfHacked>(apc).map(|h| h.ai)
            && other != ai
        {
            let name = name_of(world, apc);
            tell_ai(
                world,
                other,
                Span::Warning,
                format!("An adversarial subroutine has counter-hacked {name}!"),
            );
            if let Some(mut m) = world.get_mut::<MalfAi>(other) {
                m.hacked_apcs.retain(|&a| a != apc);
            }
        }
        world.entity_mut(apc).insert(MalfHacked {
            ai,
            next_point_at: t,
        });
        if let Some(mut a) = world.get_mut::<Apc>(apc) {
            a.locked = true;
            a.coverlocked = true;
        }
        if let Some(mut m) = world.get_mut::<MalfAi>(ai) {
            m.hacked_apcs.push(apc);
        }
        ai_sound(world, ai, "sound/machines/ding.ogg");
        let name = name_of(world, apc);
        tell_ai(
            world,
            ai,
            Span::Notice,
            format!("Взлом завершен. Теперь {name} под вашим контролем."),
        );
    }
}

/// tg APC `process`: a hacked, operating, not failing APC gives its AI a point every 30 s.
/// A handful of hacked APCs: a look at each per tick.
fn generate_points(world: &mut World) {
    let t = now(world);
    let mut query = world.query::<(Entity, &MalfHacked, &Apc)>();
    let ready: Vec<(Entity, Entity)> = query
        .iter(world)
        .filter(|(_, h, a)| t >= h.next_point_at && a.operating && a.failure_timer <= 0.0)
        .map(|(e, h, _)| (e, h.ai))
        .collect();
    for (apc, ai) in ready {
        let Some(points) = world.get::<MalfAi>(ai).map(|m| m.processing_time) else {
            // The AI is gone (tg `Destroy` clears `malfai`).
            world.entity_mut(apc).remove::<MalfHacked>();
            continue;
        };
        if points >= MALF_MAX_PP {
            continue;
        }
        if let Some(mut m) = world.get_mut::<MalfAi>(ai) {
            m.processing_time += 1;
        }
        if let Some(mut h) = world.get_mut::<MalfHacked>(apc) {
            h.next_point_at = t + POINT_COOLDOWN_SECS;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_malf_ai_may_hack_an_apc_it_does_not_own() {
        let ai = Entity::from_raw_u32(1).unwrap();
        let other = Entity::from_raw_u32(2).unwrap();
        assert_eq!(malf_status(None, None), APC_AI_NO_MALF);
        assert_eq!(malf_status(Some((ai, false)), None), APC_AI_NO_MALF);
        assert_eq!(malf_status(Some((ai, true)), None), APC_AI_NO_HACK);
        assert_eq!(malf_status(Some((ai, true)), Some(other)), APC_AI_NO_HACK);
        assert_eq!(malf_status(Some((ai, true)), Some(ai)), APC_AI_NO_MALF);
    }
}
