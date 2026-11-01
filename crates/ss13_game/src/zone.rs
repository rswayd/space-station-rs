//! Body zones, shared by both halves: tg `zone_selected` ([`TargetZone`], `BODY_ZONE_*` and
//! `BODY_ZONE_PRECISE_*`), the bodyparts it lands on ([`BodyZone`]), the HUD selector's hit map
//! (`/atom/movable/screen/zone_sel/get_zone_at`) and the numpad keybinds
//! (`code/datums/keybinding/mob.dm`).

use serde::{Deserialize, Serialize};

/// A human's bodyparts (tg `BODY_ZONE_*`), in the order [`crate::health::Health::parts`] keeps
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BodyZone {
    Head,
    Chest,
    LArm,
    RArm,
    LLeg,
    RLeg,
}

impl BodyZone {
    pub const ALL: [Self; 6] = [
        Self::Head,
        Self::Chest,
        Self::LArm,
        Self::RArm,
        Self::LLeg,
        Self::RLeg,
    ];

    /// tg `get_all_limbs()` order (`GLOB.all_body_zones`), which health scans follow.
    pub const SCAN_ORDER: [Self; 6] = [
        Self::Head,
        Self::Chest,
        Self::RArm,
        Self::LArm,
        Self::RLeg,
        Self::LLeg,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// `max_damage`: `LIMB_MAX_HP_CORE` for the head and chest, `LIMB_MAX_HP_DEFAULT` else.
    pub fn max_damage(self) -> f32 {
        match self {
            Self::Head | Self::Chest => 200.0,
            _ => 50.0,
        }
    }

    /// `plaintext_zone` in BandaStation's translation (nominative, accusative).
    pub fn ru(self) -> (&'static str, &'static str) {
        match self {
            Self::Head => ("голова", "голову"),
            Self::Chest => ("грудь", "грудь"),
            Self::LArm => ("левая рука", "левую руку"),
            Self::RArm => ("правая рука", "правую руку"),
            Self::LLeg => ("левая нога", "левую ногу"),
            Self::RLeg => ("правая нога", "правую ногу"),
        }
    }
}

/// tg `zone_selected` (default `BODY_ZONE_CHEST`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TargetZone {
    Head,
    Eyes,
    Mouth,
    #[default]
    Chest,
    Groin,
    LArm,
    RArm,
    LLeg,
    RLeg,
}

impl TargetZone {
    pub const ALL: [Self; 9] = [
        Self::Head,
        Self::Eyes,
        Self::Mouth,
        Self::Chest,
        Self::Groin,
        Self::LArm,
        Self::RArm,
        Self::LLeg,
        Self::RLeg,
    ];

    /// The DM value, also the `screen_gen.dmi` overlay state of the selector.
    pub fn dm(self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Eyes => "eyes",
            Self::Mouth => "mouth",
            Self::Chest => "chest",
            Self::Groin => "groin",
            Self::LArm => "l_arm",
            Self::RArm => "r_arm",
            Self::LLeg => "l_leg",
            Self::RLeg => "r_leg",
        }
    }

    pub fn from_dm(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|z| z.dm() == value)
    }

    /// tg `check_zone`: precise zones land on their bodypart.
    pub fn check_zone(self) -> BodyZone {
        match self {
            Self::Head | Self::Eyes | Self::Mouth => BodyZone::Head,
            Self::Chest | Self::Groin => BodyZone::Chest,
            Self::LArm => BodyZone::LArm,
            Self::RArm => BodyZone::RArm,
            Self::LLeg => BodyZone::LLeg,
            Self::RLeg => BodyZone::RLeg,
        }
    }

    /// tg `get_zone_at(icon_x, icon_y)`: BYOND icon pixels, 1..=32 from the bottom left.
    pub fn at_icon(x: i32, y: i32) -> Option<Self> {
        match y {
            1..=9 => match x {
                10..=15 => Some(Self::RLeg),
                17..=22 => Some(Self::LLeg),
                _ => None,
            },
            10..=13 => match x {
                8..=11 => Some(Self::RArm),
                12..=20 => Some(Self::Groin),
                21..=24 => Some(Self::LArm),
                _ => None,
            },
            14..=22 => match x {
                8..=11 => Some(Self::RArm),
                12..=20 => Some(Self::Chest),
                21..=24 => Some(Self::LArm),
                _ => None,
            },
            23..=30 if (12..=20).contains(&x) => Some(match y {
                23..=24 if (15..=17).contains(&x) => Self::Mouth,
                26 if (14..=18).contains(&x) => Self::Eyes,
                25..=27 if (15..=17).contains(&x) => Self::Eyes,
                _ => Self::Head,
            }),
            _ => None,
        }
    }

    /// tg `body_toggle_head` (Numpad8): head -> eyes -> mouth -> head.
    pub fn head_cycle(self) -> Self {
        match self {
            Self::Head => Self::Eyes,
            Self::Eyes => Self::Mouth,
            _ => Self::Head,
        }
    }

    /// The zone tg's default numpad keybind `n` selects (8 cycles the head instead).
    pub fn numpad(n: u8, current: Self) -> Option<Self> {
        Some(match n {
            1 => Self::RLeg,
            2 => Self::Groin,
            3 => Self::LLeg,
            4 => Self::RArm,
            5 => Self::Chest,
            6 => Self::LArm,
            7 => Self::Eyes,
            8 => current.head_cycle(),
            9 => Self::Mouth,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_map_like_tg_check_zone() {
        assert_eq!(TargetZone::Eyes.check_zone(), BodyZone::Head);
        assert_eq!(TargetZone::Mouth.check_zone(), BodyZone::Head);
        assert_eq!(TargetZone::Groin.check_zone(), BodyZone::Chest);
        assert_eq!(TargetZone::LLeg.check_zone(), BodyZone::LLeg);
        assert_eq!(TargetZone::default(), TargetZone::Chest);
        for zone in TargetZone::ALL {
            assert_eq!(TargetZone::from_dm(zone.dm()), Some(zone));
        }
        assert_eq!(BodyZone::Chest.max_damage(), 200.0);
        assert_eq!(BodyZone::RArm.max_damage(), 50.0);
    }

    #[test]
    fn selector_pixels_follow_get_zone_at() {
        // The node center (as a scripted click lands) is the chest.
        assert_eq!(TargetZone::at_icon(17, 16), Some(TargetZone::Chest));
        assert_eq!(TargetZone::at_icon(16, 26), Some(TargetZone::Eyes));
        assert_eq!(TargetZone::at_icon(18, 26), Some(TargetZone::Eyes));
        assert_eq!(TargetZone::at_icon(19, 26), Some(TargetZone::Head));
        assert_eq!(TargetZone::at_icon(16, 23), Some(TargetZone::Mouth));
        assert_eq!(TargetZone::at_icon(12, 30), Some(TargetZone::Head));
        assert_eq!(TargetZone::at_icon(10, 12), Some(TargetZone::RArm));
        assert_eq!(TargetZone::at_icon(22, 20), Some(TargetZone::LArm));
        assert_eq!(TargetZone::at_icon(16, 11), Some(TargetZone::Groin));
        assert_eq!(TargetZone::at_icon(12, 5), Some(TargetZone::RLeg));
        assert_eq!(TargetZone::at_icon(20, 5), Some(TargetZone::LLeg));
        assert_eq!(TargetZone::at_icon(16, 5), None, "between the legs");
        assert_eq!(TargetZone::at_icon(2, 16), None);
        assert_eq!(TargetZone::at_icon(16, 31), None);
    }

    #[test]
    fn numpad_follows_tg_keybinds() {
        let c = TargetZone::Chest;
        assert_eq!(TargetZone::numpad(1, c), Some(TargetZone::RLeg));
        assert_eq!(TargetZone::numpad(5, c), Some(TargetZone::Chest));
        assert_eq!(TargetZone::numpad(7, c), Some(TargetZone::Eyes));
        assert_eq!(TargetZone::numpad(9, c), Some(TargetZone::Mouth));
        assert_eq!(TargetZone::numpad(8, c), Some(TargetZone::Head));
        assert_eq!(
            TargetZone::numpad(8, TargetZone::Head),
            Some(TargetZone::Eyes)
        );
        assert_eq!(
            TargetZone::numpad(8, TargetZone::Eyes),
            Some(TargetZone::Mouth)
        );
        assert_eq!(
            TargetZone::numpad(8, TargetZone::Mouth),
            Some(TargetZone::Head)
        );
        assert_eq!(TargetZone::numpad(0, c), None);
    }
}
