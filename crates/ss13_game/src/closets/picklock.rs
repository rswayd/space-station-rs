//! Picking a secure closet's lock without an emag (BandaStation
//! `modular_bandastation/closet_picklocking/code/closet_picklocking.dm`).
//!
//! Out of combat mode, on a locked secure closet or crate, three 16 s tool steps:
//! - a screwdriver takes the lock's panel off (95%: the lock is `broken`, it no longer toggles;
//!   else the tool slips: 5 brute to the other hand and a scream); the abandoned crates' panels
//!   are rusted shut;
//! - wirecutters prepare the wiring (80%; else sparks and a shock from the area's power);
//! - a multitool hacks it (80%: the lock breaks open as under an emag; else sparks and a shock).
//!
//! The stage reached shows on examine ([`examine_lines`]).

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::Closet;
use crate::click::ClickResolved;
use crate::combat::{CombatMode, CombatRng};
use crate::construction::Job;
use crate::inventory::Inventory;
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of, use_tool};

/// `/obj/structure/closet/crate/secure/loot`: its panel is rusted shut.
const LOOT_CRATE: &str = "/obj/structure/closet/crate/secure/loot";
/// `use_tool(src, user, 16 SECONDS)`, deciseconds.
const STEP_DS: f32 = 160.0;
/// `prob(95)` of the panel, `prob(80)` of the wiring and the hack.
const PANEL_CHANCE: f32 = 95.0;
const WIRES_CHANCE: f32 = 80.0;
/// `apply_damage(5, BRUTE, user.get_inactive_hand())` of a slipped screwdriver.
const SLIP_DAMAGE: f32 = 5.0;
/// `electrocute_mob(..., 0.5, TRUE)`.
const SHOCK_SIEMENS: f32 = 0.5;

/// `picklocking_stage` (`CLOSET_LOCK_PANEL_OPEN`, `CLOSET_LOCK_WIRES_DISCONNECTED`,
/// `CLOSET_LOCK_HACKED`); no component: `null`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PicklockStage {
    PanelOpen,
    WiresDisconnected,
    Hacked,
}

/// The timed steps ([`Job::Picklock`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PicklockJob {
    Panel,
    Wires,
    Hack,
}

/// `screwdriver_act`, `wirecutter_act`, `multitool_act` of a closet with `held`. True when the
/// click was taken.
pub(super) fn tool_click(world: &mut World, click: &ClickResolved, held: Entity) -> bool {
    let closet = click.target;
    let Some(state) = world.get::<Closet>(closet).cloned() else {
        return false;
    };
    if world.get::<CombatMode>(click.mob).is_some_and(|c| c.0) {
        return false;
    }
    let Some((kind, _)) = tool_of(world, held) else {
        return false;
    };
    let stage = world.get::<PicklockStage>(closet).copied();
    let tool = crate::security::name_of(world, held);
    let (job, text) = match kind {
        ToolKind::Screwdriver => {
            if !state.secure {
                return false;
            }
            let loot = crate::construction::path_of(world, closet)
                .is_some_and(|p| ss13_mapload::fidelity::is_under(&p, LOOT_CRATE));
            if loot {
                tell(
                    world,
                    click.client,
                    Span::Warning,
                    "Панель этого замка приржавела намертво!",
                );
                return true;
            }
            if !state.locked || stage.is_some() {
                return false;
            }
            (
                PicklockJob::Panel,
                format!("Вы начинаете откручивать панель замка {tool}..."),
            )
        }
        ToolKind::Wirecutters if state.locked && stage == Some(PicklockStage::PanelOpen) => (
            PicklockJob::Wires,
            format!("Вы начинаете перерезать провода {tool}..."),
        ),
        ToolKind::Multitool if state.locked && stage == Some(PicklockStage::WiresDisconnected) => (
            PicklockJob::Hack,
            format!("Вы начинаете подключать {tool} к проводам..."),
        ),
        _ => return false,
    };
    if world.get::<DoAfter>(click.mob).is_some() {
        return true;
    }
    tell(world, click.client, Span::Notice, text);
    let actor = Actor {
        client: click.client,
        mob: click.mob,
        item: Some(held),
    };
    use_tool(world, actor, closet, STEP_DS, 0, Job::Picklock(job));
    true
}

/// The end of a [`PicklockJob`]: the lock and the stage are checked again, then the roll.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: PicklockJob) {
    let closet = action.target;
    let Some(locked) = world.get::<Closet>(closet).map(|c| c.locked) else {
        return;
    };
    let stage = world.get::<PicklockStage>(closet).copied();
    if !locked || !step_applies(job, stage) {
        return;
    }
    let client = action.client;
    let chance = if job == PicklockJob::Panel {
        PANEL_CHANCE
    } else {
        WIRES_CHANCE
    };
    let success = world.resource_mut::<CombatRng>().chance(chance);
    match (job, success) {
        (PicklockJob::Panel, true) => {
            if let Some(mut c) = world.get_mut::<Closet>(closet) {
                c.broken = true;
            }
            world.entity_mut(closet).insert(PicklockStage::PanelOpen);
            super::refresh_look(world, closet);
            tell(
                world,
                client,
                Span::Notice,
                "Вы успешно открутили и сняли панель с замка!",
            );
        }
        (PicklockJob::Panel, false) => {
            let inactive = world.get::<Inventory>(mob).map_or(1, |i| 1 - i.active);
            let zone = if inactive == 0 {
                crate::zone::BodyZone::LArm
            } else {
                crate::zone::BodyZone::RArm
            };
            crate::security::damage(
                world,
                mob,
                Some(zone),
                crate::health::DamageType::Brute,
                SLIP_DAMAGE,
            );
            crate::medical::diseases::emote(world, mob, "scream");
            let tool = action
                .item
                .map(|t| ss13_shared::capitalize(&crate::security::name_of(world, t)))
                .unwrap_or_default();
            tell(
                world,
                client,
                Span::Warning,
                format!("Проклятье! {tool} сорвалась и повредила руку!"),
            );
        }
        (PicklockJob::Wires, true) => {
            world
                .entity_mut(closet)
                .insert(PicklockStage::WiresDisconnected);
            tell(
                world,
                client,
                Span::Notice,
                "Вы успешно подготовили проводку для взлома!",
            );
        }
        (PicklockJob::Hack, true) => {
            if let Some(mut c) = world.get_mut::<Closet>(closet) {
                c.broken = false;
            }
            world.entity_mut(closet).insert(PicklockStage::Hacked);
            crate::emag::closet(world, mob, closet);
        }
        (PicklockJob::Wires | PicklockJob::Hack, false) => {
            tell(world, client, Span::Warning, "Черт! Не тот провод!");
            if let Some(&pos) = world.get::<TilePos>(closet) {
                crate::effects::particles::do_sparks(world, 5, true, pos);
            }
            crate::wires::electrocute_from_area(world, mob, closet, SHOCK_SIEMENS);
        }
    }
}

/// Whether `job` is the next step from `stage` (each needs the one before it).
fn step_applies(job: PicklockJob, stage: Option<PicklockStage>) -> bool {
    let before = match job {
        PicklockJob::Panel => None,
        PicklockJob::Wires => Some(PicklockStage::PanelOpen),
        PicklockJob::Hack => Some(PicklockStage::WiresDisconnected),
    };
    stage == before
}

/// The stage on examine.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let text = match world.get::<PicklockStage>(target) {
        Some(PicklockStage::PanelOpen) => "Панель замка снята.",
        Some(PicklockStage::WiresDisconnected) => "Панель замка снята, а провода торчат наружу.",
        Some(PicklockStage::Hacked) => "Замок полностью взломан.",
        None => return,
    };
    lines.push((Span::Notice, text.to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_step_needs_the_stage_before_it() {
        use PicklockJob::*;
        use PicklockStage::*;
        assert!(step_applies(Panel, None));
        assert!(!step_applies(Panel, Some(PanelOpen)));
        assert!(!step_applies(Wires, None));
        assert!(step_applies(Wires, Some(PanelOpen)));
        assert!(!step_applies(Hack, Some(PanelOpen)));
        assert!(step_applies(Hack, Some(WiresDisconnected)));
        assert!(!step_applies(Hack, Some(Hacked)));
    }
}
