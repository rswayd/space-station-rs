//! Automated defenses: portable turrets and the turret control panel (tg
//! `code/game/machinery/porta_turret/`), and building, taking apart and motion-sensing security
//! cameras (`code/game/machinery/camera/`). The window data of tgui `TurretControl` and
//! `PortableTurret` lives here, shared by both halves; [`server`] runs them, [`client`] draws
//! the windows.

#[cfg(feature = "server")]
pub mod cameras;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `TurretControl` (`tgui/packages/tgui/interfaces/TurretControl.tsx`).
pub struct TurretControlUi;

impl UiInterface for TurretControlUi {
    const NAME: &'static str = "TurretControl";
    type Data = TurretControlData;
    type StaticData = ();
}

/// `/obj/machinery/turretid/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TurretControlData {
    pub locked: bool,
    /// `siliconUser` (`HAS_SILICON_ACCESS`).
    pub silicon_user: bool,
    pub enabled: bool,
    pub lethal: bool,
    pub shoot_cyborgs: bool,
}

/// tgui `PortableTurret` (`tgui/packages/tgui/interfaces/PortableTurret.jsx`).
pub struct PortableTurretUi;

impl UiInterface for PortableTurretUi {
    const NAME: &'static str = "PortableTurret";
    type Data = PortableTurretData;
    type StaticData = ();
}

/// `/obj/machinery/porta_turret/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct PortableTurretData {
    pub locked: bool,
    pub on: bool,
    pub check_weapons: bool,
    pub neutralize_criminals: bool,
    pub neutralize_all: bool,
    pub neutralize_unidentified: bool,
    pub neutralize_nonmindshielded: bool,
    pub neutralize_cyborgs: bool,
    pub neutralize_heads: bool,
    pub silicon_user: bool,
    pub lasertag_turret: bool,
}

/// `TurretControl` acts.
pub const LOCK: &str = "lock";
pub const POWER: &str = "power";
pub const MODE: &str = "mode";
pub const SHOOT_SILICONS: &str = "shoot_silicons";
/// `PortableTurret` acts (with [`POWER`]): each toggles one `turret_flags` bit.
pub const AUTH_WEAPON: &str = "authweapon";
pub const SHOOT_CRIMINALS: &str = "shootcriminals";
pub const SHOOT_ALL: &str = "shootall";
pub const CHECK_XENOS: &str = "checkxenos";
pub const CHECK_LOYAL: &str = "checkloyal";
pub const SHOOT_BORGS: &str = "shootborgs";
pub const SHOOT_HEADS: &str = "shootheads";

/// tg `TURRET_FLAG_*`.
pub const TURRET_FLAG_SHOOT_ALL_REACT: u32 = 1 << 0;
pub const TURRET_FLAG_AUTH_WEAPONS: u32 = 1 << 1;
pub const TURRET_FLAG_SHOOT_CRIMINALS: u32 = 1 << 2;
pub const TURRET_FLAG_SHOOT_ALL: u32 = 1 << 3;
pub const TURRET_FLAG_SHOOT_ANOMALOUS: u32 = 1 << 4;
pub const TURRET_FLAG_SHOOT_UNSHIELDED: u32 = 1 << 5;
pub const TURRET_FLAG_SHOOT_BORGS: u32 = 1 << 6;
pub const TURRET_FLAG_SHOOT_HEADS: u32 = 1 << 7;

/// The `turret_flags` bit a `PortableTurret` act toggles.
pub fn act_flag(act: &str) -> Option<u32> {
    Some(match act {
        AUTH_WEAPON => TURRET_FLAG_AUTH_WEAPONS,
        SHOOT_CRIMINALS => TURRET_FLAG_SHOOT_CRIMINALS,
        SHOOT_ALL => TURRET_FLAG_SHOOT_ALL,
        CHECK_XENOS => TURRET_FLAG_SHOOT_ANOMALOUS,
        CHECK_LOYAL => TURRET_FLAG_SHOOT_UNSHIELDED,
        SHOOT_BORGS => TURRET_FLAG_SHOOT_BORGS,
        SHOOT_HEADS => TURRET_FLAG_SHOOT_HEADS,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips() {
        let control = TurretControlData {
            locked: true,
            silicon_user: false,
            enabled: true,
            lethal: true,
            shoot_cyborgs: false,
        };
        let bytes = ss13_tgui::encode(&control);
        assert_eq!(
            ss13_tgui::decode::<TurretControlData>(&bytes).unwrap(),
            control
        );
        let turret = PortableTurretData {
            on: true,
            neutralize_heads: true,
            ..Default::default()
        };
        let bytes = ss13_tgui::encode(&turret);
        assert_eq!(
            ss13_tgui::decode::<PortableTurretData>(&bytes).unwrap(),
            turret
        );
        assert_eq!(act_flag(SHOOT_HEADS), Some(TURRET_FLAG_SHOOT_HEADS));
        assert_eq!(act_flag(POWER), None);
    }
}
