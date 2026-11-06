//! Laser tag (server), after tg `code/modules/clothing/suits/lasertag.dm` and
//! `_lasertag_components.dm` (`/datum/component/lasertag`).
//!
//! A human wearing a blue or red laser tag vest as its suit is on that team ([`team_of`]; tg
//! adds the component on `equipped` in `ITEM_SLOT_OCLOTHING` and removes it on `dropped`).
//! - A laser tag firing pin fires only for its own team (`COMSIG_LIVING_FIRING_PIN_CHECK`:
//!   `team_color_match`); without a vest, never (`default_pin_auth = FALSE`).
//! - A laser tag beam of another team (neutral beams included) drains `lasertag_damage` (34)
//!   stamina from a team member it hits (`on_laser_hit`); the beams themselves do no damage.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;

use crate::inventory::Inventory;

pub const BLUE_VEST: &str = "/obj/item/clothing/suit/bluetag";
pub const RED_VEST: &str = "/obj/item/clothing/suit/redtag";
const BEAM: &str = "/obj/projectile/beam/lasertag";
const PIN: &str = "/obj/item/firing_pin/tag";
/// `/obj/projectile/beam/lasertag::lasertag_damage`.
pub const LASERTAG_DAMAGE: f32 = 34.0;

/// `LASERTAG_TEAM_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Team {
    Neutral,
    Red,
    Blue,
}

impl Team {
    /// `LASERTAG_TEAM_*` strings, as the pin's failure line names them.
    pub fn name(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Red => "red",
            Self::Blue => "blue",
        }
    }

    /// The team of a `/obj/projectile/beam/lasertag` (`lasertag_team`) or of a
    /// `/obj/item/firing_pin/tag` (`tagcolor`) under `root`.
    fn of_subtype(path: &str, root: &str) -> Self {
        if is_under(path, &format!("{root}/redtag")) || is_under(path, &format!("{root}/red")) {
            Self::Red
        } else if is_under(path, &format!("{root}/bluetag"))
            || is_under(path, &format!("{root}/blue"))
        {
            Self::Blue
        } else {
            Self::Neutral
        }
    }
}

/// The team of a vest path worn as the suit.
pub fn vest_team(path: &str) -> Option<Team> {
    if is_under(path, BLUE_VEST) {
        Some(Team::Blue)
    } else if is_under(path, RED_VEST) {
        Some(Team::Red)
    } else {
        None
    }
}

/// The mob's laser tag team: its suit's (only humans have one).
pub fn team_of(world: &World, mob: Entity) -> Option<Team> {
    let suit = world.get::<Inventory>(mob)?.suit?;
    vest_team(&world.get::<DmItem>(suit)?.path)
}

/// A laser tag pin's check for `user`: `None` when `pin` is not one, else whether it fires
/// and the line on a failure (`auth_fail`).
pub fn pin_check(world: &World, pin: &str, user: Entity) -> Option<Result<(), String>> {
    if !is_under(pin, PIN) {
        return None;
    }
    let color = Team::of_subtype(pin, PIN);
    Some(if team_of(world, user) == Some(color) {
        Ok(())
    } else {
        Err(format!(
            "Вам нужно иметь лазер-таг броню цвета: {}!",
            color.name()
        ))
    })
}

/// The stamina a laser tag beam (`projectile` path) drains from `target` (`on_laser_hit`).
pub fn hit_stamina(world: &World, projectile: &str, target: Entity) -> Option<f32> {
    if !is_under(projectile, BEAM) {
        return None;
    }
    let team = team_of(world, target)?;
    (Team::of_subtype(projectile, BEAM) != team).then_some(LASERTAG_DAMAGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teams_of_vests_beams_and_pins() {
        assert_eq!(vest_team(BLUE_VEST), Some(Team::Blue));
        assert_eq!(vest_team(RED_VEST), Some(Team::Red));
        assert_eq!(vest_team("/obj/item/clothing/suit/armor/vest"), None);
        assert_eq!(
            Team::of_subtype("/obj/projectile/beam/lasertag/redtag/hitscan", BEAM),
            Team::Red
        );
        assert_eq!(Team::of_subtype(BEAM, BEAM), Team::Neutral);
        assert_eq!(
            Team::of_subtype("/obj/item/firing_pin/tag/blue", PIN),
            Team::Blue
        );
    }
}
