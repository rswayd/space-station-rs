//! The AI's Status tab lines (server), after tg `/mob/living/silicon/ai/get_status_tab_items`
//! (`code/modules/mob/living/silicon/ai/ai.dm`): system integrity, backup power while in a
//! core, and every connected cyborg with its integrity, cell, model, area and status. The
//! statpanel (`crate::statpanel`) appends them to the Status tab of the client whose mob is an
//! AI's eye. Lockdown of these cyborgs is the robotics console's (`super::robotics`), which the
//! AI opens with a click.
// shortcut: a cyborg's line is text, not the tg `track_cyborg` link.

use bevy::prelude::*;
use ss13_shared::TilePos;

use super::Silicons;
use super::ai::{AiCore, AiEye, AiInside};
use super::ai_power::AiPower;
use super::chassis::{Chassis, name_of};
use super::cyborg::Cyborg;
use crate::health::{Health, Stat};
use crate::server::PlayerMob;

/// A connected cyborg's tg `robot_status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RobotStatus {
    Nominal,
    Offline,
    Depowered,
}

impl RobotStatus {
    /// tg order: unconscious or clientless first, then an empty or missing cell.
    pub fn of(conscious: bool, has_client: bool, has_cell: bool, charge: f64) -> Self {
        if !conscious || !has_client {
            Self::Offline
        } else if !has_cell || charge <= 0.0 {
            Self::Depowered
        } else {
            Self::Nominal
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Self::Nominal => "Nominal",
            Self::Offline => "OFFLINE",
            Self::Depowered => "DEPOWERED",
        }
    }
}

/// tg `display_energy` rounding of the cell line.
fn energy(joules: f64) -> String {
    crate::modsuit::display_energy(joules)
}

/// The AI lines of the Status tab for the client's controlled `mob` (empty unless it is an AI
/// eye).
pub fn status_lines(world: &World, mob: Entity) -> Vec<String> {
    match world.get::<AiEye>(mob) {
        Some(eye) => ai_lines(world, eye.ai),
        None => Vec::new(),
    }
}

/// The Status tab lines of the AI mob `ai` (its core).
pub fn ai_lines(world: &World, ai: Entity) -> Vec<String> {
    let Some(core) = world.get::<AiCore>(ai) else {
        return Vec::new();
    };
    // tg `IS_UNCONSCIOUS_OR_CRIT`.
    if world.get::<Stat>(ai).is_some_and(|s| *s != Stat::Conscious) {
        return vec!["Systems nonfunctional".to_owned()];
    }
    let health = world.get::<Health>(ai).map_or(100.0, Health::health);
    let mut lines = vec![format!(
        "System integrity: {}%",
        trim((health as f64 + 100.0) * 0.5)
    )];
    // "only show if we're in a core": on a turf, not in a card or an APC.
    if world.get::<AiInside>(ai).is_none()
        && let Some(power) = world.get::<AiPower>(ai)
    {
        lines.push(format!("Backup Power: {}%", trim(power.battery * 0.5)));
    }
    let robots: Vec<Entity> = core
        .connected_robots
        .iter()
        .copied()
        .filter(|&r| world.get::<Cyborg>(r).is_some())
        .collect();
    lines.push(format!("Connected cyborgs: {}", robots.len()));
    let models = world.get_resource::<Silicons>();
    for borg in robots {
        let Some(cyborg) = world.get::<Cyborg>(borg) else {
            continue;
        };
        let has_cell = world.get::<Chassis>(borg).is_some_and(|c| c.cell.is_some());
        let conscious = world
            .get::<Stat>(borg)
            .is_none_or(|s| *s == Stat::Conscious);
        let has_client = world
            .get::<PlayerMob>(borg)
            .is_some_and(|p| world.get_entity(p.client).is_ok());
        let shell = world.get::<super::ai_shell::AiShell>(borg).is_some();
        let status = RobotStatus::of(conscious, has_client, has_cell, cyborg.charge);
        let cell = if has_cell {
            format!("{}/{}", energy(cyborg.charge), energy(cyborg.max_charge))
        } else {
            "Empty".to_owned()
        };
        let model = cyborg
            .model
            .and_then(|i| models.and_then(|m| m.0.models.get(i)))
            .map_or("Default", |m| m.name.as_str());
        let area = world
            .get::<TilePos>(borg)
            .map(|&p| crate::bots::area_name(world, p))
            .unwrap_or_default();
        let integrity = world.get::<Health>(borg).map_or(100.0, Health::health);
        lines.push(format!(
            "{}: S.Integrity: {}% | Cell: {cell} | Model: {model} | Loc: {area} | Status: {}",
            name_of(world, borg),
            trim(integrity as f64),
            if shell { "AI SHELL" } else { status.text() }
        ));
    }
    lines.push(format!("AI shell beacons detected: {}", shells(world)));
    lines
}

/// tg `GLOB.available_ai_shells`.
// Cost: one pass over the entities per AI client per statpanel refresh (once a second).
fn shells(world: &World) -> usize {
    world
        .iter_entities()
        .filter(|e| e.contains::<super::ai_shell::AiShell>())
        .count()
}

/// A BYOND number as text: no trailing `.0`.
fn trim(value: f64) -> String {
    let text = format!("{:.1}", value);
    text.strip_suffix(".0").map(str::to_owned).unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robot_status_follows_tg_order() {
        assert_eq!(
            RobotStatus::of(true, true, true, 10.0),
            RobotStatus::Nominal
        );
        assert_eq!(
            RobotStatus::of(false, true, true, 10.0),
            RobotStatus::Offline
        );
        assert_eq!(
            RobotStatus::of(true, false, false, 0.0),
            RobotStatus::Offline
        );
        assert_eq!(
            RobotStatus::of(true, true, false, 10.0),
            RobotStatus::Depowered
        );
        assert_eq!(
            RobotStatus::of(true, true, true, 0.0),
            RobotStatus::Depowered
        );
    }

    #[test]
    fn numbers_read_as_byond() {
        assert_eq!(trim(100.0), "100");
        assert_eq!(trim(75.5), "75.5");
    }
}
