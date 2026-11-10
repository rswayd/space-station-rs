//! MODsuits, after tg `code/modules/mod/`: the control unit (`/obj/item/mod/control`) worn on
//! the back deploys and retracts its parts (helmet, chestplate, gauntlets, boots), seals them one
//! by one when activated (protection, pressure sealing, the theme's slowdown), drains its core's
//! cell every second while on, and carries modules up to its theme's complexity. [`server`] runs
//! it (with the common modules in [`modules`]), [`client`] draws tgui `MODsuit`; the window data
//! lives here, shared by both halves.
//!
//! Themes, control unit types and module types come from `ss13-mapload convert`
//! (`<assets>/modsuits.ron`, [`ss13_mapload::modsuit`]); rerun it after updating.

#[cfg(feature = "server")]
pub mod ai;
#[cfg(feature = "server")]
pub mod anomalock;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod construction;
#[cfg(feature = "server")]
pub mod link;
#[cfg(feature = "server")]
pub mod modules;
#[cfg(feature = "server")]
pub mod ninja;
#[cfg(feature = "server")]
pub mod paint;
#[cfg(feature = "server")]
pub mod pathfinder;
#[cfg(feature = "server")]
pub mod science;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod supply;

/// The timed MOD steps (`crate::construction::Job::ModSuit`): the thing they act on is in the
/// job, the `do_after` target is it on the floor or the user when it is held.
#[cfg(feature = "server")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModJob {
    /// `broken_core/screwdriver_act`: a standard core.
    RepairCore(bevy::prelude::Entity),
    /// `lavalandcore/item_interaction`: two cables make a lavaland plasma core.
    WireFlower(bevy::prelude::Entity),
    /// `transfer_ai(AI_TRANS_TO_CARD)` of a control unit.
    AiToCard {
        control: bevy::prelude::Entity,
        card: bevy::prelude::Entity,
    },
    /// `transfer_ai(AI_TRANS_FROM_CARD)` of a control unit.
    AiFromCard {
        control: bevy::prelude::Entity,
        card: bevy::prelude::Entity,
        ai: bevy::prelude::Entity,
    },
    /// `ai_minicard/transfer_ai`: the dead AI into an empty intelliCard.
    MinicardToCard {
        minicard: bevy::prelude::Entity,
        card: bevy::prelude::Entity,
    },
    /// The hydraulic clamp loading the `do_after` target (`clamp/on_select_use`).
    ClampLoad {
        control: bevy::prelude::Entity,
        module: bevy::prelude::Entity,
    },
    /// The hydraulic clamp setting its last crate down on the target turf.
    ClampDrop {
        control: bevy::prelude::Entity,
        module: bevy::prelude::Entity,
    },
    /// The drill module's `manual_mine` of the target wall (`sphere`: the sphere's
    /// `exp_multiplier` of 0.2).
    Drill { sphere: bool },
    /// The teleporter module's `do_after` before `do_teleport`.
    Teleport {
        control: bevy::prelude::Entity,
        module: bevy::prelude::Entity,
        dest: ss13_shared::TilePos,
    },
    /// The pathfinder module implanting the target (`pathfinder/attack`).
    PathfinderImplant { module: bevy::prelude::Entity },
}

/// The end of a [`ModJob`]'s `do_after`.
#[cfg(feature = "server")]
pub fn complete(
    world: &mut bevy::prelude::World,
    mob: bevy::prelude::Entity,
    action: &crate::tools::DoAfter,
    job: ModJob,
) {
    match job {
        ModJob::RepairCore(item) | ModJob::WireFlower(item) => {
            construction::complete(world, mob, action, item)
        }
        ModJob::AiToCard { .. } | ModJob::AiFromCard { .. } | ModJob::MinicardToCard { .. } => {
            ai::complete(world, mob, action, job)
        }
        ModJob::ClampLoad { .. } | ModJob::ClampDrop { .. } | ModJob::Drill { .. } => {
            supply::complete(world, mob, action, job)
        }
        ModJob::Teleport { .. } => science::complete(world, mob, action, job),
        ModJob::PathfinderImplant { module } => {
            pathfinder::complete_implant(world, mob, action, module)
        }
    }
}

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `MODsuit` (`tgui/packages/tgui/interfaces/MODsuit.tsx`).
pub struct ModSuitUi;

impl UiInterface for ModSuitUi {
    const NAME: &'static str = "MODsuit";
    type Data = ModSuitData;
    type StaticData = ModSuitStatic;
}

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModSuitStatic {
    pub ui_theme: String,
    pub complexity_max: i32,
}

/// `ui_data["suit_status"]`.
// shortcut: no pAI, wires or electrification fields (not ported).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SuitStatus {
    pub core_name: Option<String>,
    /// Joules.
    pub charge_current: f64,
    pub charge_max: f64,
    /// `good`, `average`, `bad`, `teal` or `transparent`.
    pub chargebar_color: String,
    pub chargebar_string: String,
    pub active: bool,
    /// Not tg: the activation sequence is running (tg shows it through the do_after bar).
    pub activating: bool,
    pub open: bool,
    pub malfunctioning: bool,
    pub locked: bool,
    pub complexity: i32,
    /// `ai_name`: the AI assistant's name.
    pub ai_name: Option<String>,
    /// `is_ai`: the window's user is the AI assistant.
    pub is_ai: bool,
    /// `link_id`, `link_freq`, `link_call` (the other link's id while in a call).
    pub link_id: String,
    pub link_freq: Option<String>,
    pub link_call: Option<String>,
}

/// `ui_data["user_status"]`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct UserStatus {
    pub user_name: String,
    pub user_assignment: String,
}

/// One `get_configuration` entry (`add_ui_configuration`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModuleConfig {
    pub key: String,
    pub display_name: String,
    /// `number`, `list` or `bool`.
    pub kind: String,
    pub value: String,
    pub values: Vec<String>,
}

/// A `module_info` entry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModuleInfo {
    pub module_name: String,
    pub description: String,
    /// `MODULE_PASSIVE` 0, `MODULE_USABLE` 1, `MODULE_TOGGLE` 2, `MODULE_ACTIVE` 3.
    pub module_type: u8,
    pub module_active: bool,
    pub pinned: bool,
    /// Watts while idle / active, joules per use.
    pub idle_power: f64,
    pub active_power: f64,
    pub use_energy: f64,
    pub module_complexity: i32,
    /// Deciseconds.
    pub cooldown_time: f32,
    /// Deciseconds left.
    pub cooldown: f32,
    pub id: String,
    /// The module entity's bits (tg `REF(module)`).
    pub reference: u64,
    pub configuration_data: Vec<ModuleConfig>,
}

/// A `parts` entry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PartInfo {
    pub slot: String,
    pub name: String,
    pub deployed: bool,
    pub reference: u64,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModSuitData {
    pub suit_status: SuitStatus,
    pub user_status: UserStatus,
    pub control: String,
    pub module_info: Vec<ModuleInfo>,
    pub parts: Vec<PartInfo>,
}

/// tgui `MODpaint` (`tgui/packages/tgui/interfaces/MODpaint.jsx`): the paint kit's color
/// matrix editor.
pub struct ModPaintUi;

impl UiInterface for ModPaintUi {
    const NAME: &'static str = "MODpaint";
    type Data = ModPaintData;
    type StaticData = ();
}

/// `ui_data`: `currentColor`, a 20-value color matrix (rows rr rg rb ra, gr .., br .., ar ..,
/// then the constants).
// shortcut: no `mapRef` preview of the suit (no map views in windows).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModPaintData {
    pub current_color: Vec<f32>,
}

pub const TRANSITION_COLOR: &str = "transition_color";
pub const CONFIRM: &str = "confirm";
/// The `transition_color` param: the matrix as comma-separated numbers.
pub const COLOR: &str = "color";

/// `COLOR_MATRIX_IDENTITY`.
pub const COLOR_MATRIX_IDENTITY: [f32; 20] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
    0.0,
];

/// `MODpaint.jsx` `colorToMatrix` presets.
pub const PAINT_PRESETS: [(&str, [f32; 20]); 6] = [
    (
        "red",
        [
            1.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.0, 0.0, 0.25, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0,
        ],
    ),
    (
        "yellow",
        [
            0.5, 0.5, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.25, 0.25, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0,
        ],
    ),
    (
        "green",
        [
            0.5, 0.25, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0,
        ],
    ),
    (
        "teal",
        [
            0.25, 0.25, 0.25, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0,
        ],
    ),
    (
        "blue",
        [
            0.25, 0.0, 0.25, 0.0, 0.0, 0.5, 0.25, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0,
        ],
    ),
    (
        "purple",
        [
            0.5, 0.0, 0.5, 0.0, 0.25, 0.5, 0.25, 0.0, 0.5, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0,
        ],
    ),
];

/// `MODPAINT_MAX_COLOR_VALUE`, `MODPAINT_MIN_COLOR_VALUE`, `MODPAINT_MAX_SECTION_COLORS`,
/// `MODPAINT_MIN_SECTION_COLORS`, `MODPAINT_MAX_OVERALL_COLORS`, `MODPAINT_MIN_OVERALL_COLORS`.
const MAX_COLOR: f32 = 1.25;
const MIN_COLOR: f32 = 0.0;
const MAX_SECTION: f32 = 2.0;
const MIN_SECTION: f32 = 0.25;
const MAX_OVERALL: f32 = 4.0;
const MIN_OVERALL: f32 = 1.5;

/// The paint kit's `confirm` checks: why the matrix is refused, if it is.
// Not tg: tg's `Cut(13, length)` keeps the last constant in the overall sum; the comment says
// only the colors count, so here the first twelve values do.
pub fn paint_refusal(m: &[f32]) -> Option<String> {
    if m.len() != 20 || m.iter().any(|v| !v.is_finite()) {
        return Some("неверная матрица!".to_owned());
    }
    let pct = |v: f32| (v * 100.0).round();
    let sections = [
        ("красного", m[0] + m[4] + m[8]),
        ("зелёного", m[1] + m[5] + m[9]),
        ("синего", m[2] + m[6] + m[10]),
    ];
    for (name, value) in sections {
        if value > MAX_SECTION {
            return Some(format!(
                "слишком много {name}! ({}%/{}%)",
                pct(value),
                pct(MAX_SECTION)
            ));
        }
        if value < MIN_SECTION {
            return Some(format!(
                "слишком мало {name}! ({}%/{}%)",
                pct(value),
                pct(MIN_SECTION)
            ));
        }
    }
    let mut total = 0.0;
    for &value in &m[..12] {
        total += value;
        if value > MAX_COLOR {
            return Some(format!(
                "один из цветов слишком ярок! ({}%/{}%)",
                pct(value),
                pct(MAX_COLOR)
            ));
        }
        if value < MIN_COLOR {
            return Some(format!(
                "один из цветов слишком тёмен! ({}%/{}%)",
                pct(value),
                pct(MIN_COLOR)
            ));
        }
    }
    if total > MAX_OVERALL {
        return Some(format!(
            "слишком много цвета! ({}%/{}%)",
            pct(total),
            pct(MAX_OVERALL)
        ));
    }
    if total < MIN_OVERALL {
        return Some(format!(
            "слишком мало цвета! ({}%/{}%)",
            pct(total),
            pct(MIN_OVERALL)
        ));
    }
    None
}

/// What a color matrix makes of white: the tint the item's `color` multiplies its sprite by.
// shortcut: appearances carry one RGBA multiplier, not a color matrix; a white pixel is exact,
// others are tinted instead of channel-mixed.
pub fn matrix_tint(m: &[f32]) -> [u8; 4] {
    let channel = |c: usize| {
        let v = m.get(c).copied().unwrap_or(0.0)
            + m.get(4 + c).copied().unwrap_or(0.0)
            + m.get(8 + c).copied().unwrap_or(0.0)
            + m.get(16 + c).copied().unwrap_or(0.0);
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    [channel(0), channel(1), channel(2), 255]
}

pub const LOCK: &str = "lock";
pub const ACTIVATE: &str = "activate";
pub const SELECT: &str = "select";
pub const CONFIGURE: &str = "configure";
pub const PIN: &str = "pin";
pub const DEPLOY: &str = "deploy";
pub const CALL: &str = "call";
pub const REF: &str = "ref";
pub const KEY: &str = "key";
pub const VALUE: &str = "value";

/// `MODULE_PASSIVE`, `MODULE_USABLE`, `MODULE_TOGGLE`, `MODULE_ACTIVE`.
pub const MODULE_PASSIVE: u8 = 0;
pub const MODULE_USABLE: u8 = 1;
pub const MODULE_TOGGLE: u8 = 2;
pub const MODULE_ACTIVE: u8 = 3;

/// `display_energy`: joules with an SI prefix, as the charge bar shows them.
pub fn display_energy(joules: f64) -> String {
    let (value, unit) = if joules >= 1e9 {
        (joules / 1e9, "GJ")
    } else if joules >= 1e6 {
        (joules / 1e6, "MJ")
    } else if joules >= 1e3 {
        (joules / 1e3, "kJ")
    } else {
        (joules, "J")
    };
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text} {unit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_checks_follow_tg_limits() {
        assert_eq!(paint_refusal(&COLOR_MATRIX_IDENTITY), None);
        for (_, preset) in PAINT_PRESETS {
            assert_eq!(paint_refusal(&preset), None, "{preset:?}");
        }
        let mut dark = COLOR_MATRIX_IDENTITY;
        dark[0] = 0.1;
        assert!(paint_refusal(&dark).unwrap().contains("красного"));
        let mut bright = COLOR_MATRIX_IDENTITY;
        bright[5] = 1.25;
        bright[6] = 1.25;
        bright[1] = 1.0;
        assert!(paint_refusal(&bright).is_some());
        assert!(paint_refusal(&[1.0; 3]).is_some());
        assert_eq!(matrix_tint(&COLOR_MATRIX_IDENTITY), [255, 255, 255, 255]);
        assert_eq!(matrix_tint(&PAINT_PRESETS[0].1), [255, 128, 128, 255]);
    }

    #[test]
    fn energy_reads_with_si_prefixes() {
        assert_eq!(display_energy(50_000.0), "50 kJ");
        assert_eq!(display_energy(1_250_000.0), "1.25 MJ");
        assert_eq!(display_energy(0.0), "0 J");
    }
}
