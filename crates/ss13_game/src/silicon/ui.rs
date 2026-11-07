//! Window and HUD data both halves share.

use serde::{Deserialize, Serialize};
use ss13_shared::TilePos;
use ss13_tgui::UiInterface;

/// The security camera console: BandaStation's `CameraConsole220` (tg `CameraConsole` with a
/// nanomap tab).
pub struct CameraConsoleUi;

impl UiInterface for CameraConsoleUi {
    const NAME: &'static str = "CameraConsole220";
    type Data = CameraConsoleData;
    type StaticData = CameraConsoleStatic;
}

/// One entry of `SScameras.get_available_cameras_data` (BandaStation adds x, y, z, status).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraEntry {
    pub name: String,
    /// tg `REF(camera)`: an opaque id the act names.
    pub cam_ref: String,
    pub x: i32,
    pub y: i32,
    pub z: u16,
    pub status: bool,
}

/// `ui_static_data`: `network`, `cameras` (sorted by `c_tag`, tg `cmp_camera_ctag_asc`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraConsoleStatic {
    pub network: Vec<String>,
    pub cameras: Vec<CameraEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActiveCamera {
    pub name: String,
    pub cam_ref: String,
    pub status: bool,
}

/// `ui_data`: `activeCamera`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraConsoleData {
    pub active_camera: Option<ActiveCamera>,
}

/// `act('switch_camera', { camera })`.
pub const SWITCH_CAMERA: &str = "switch_camera";

/// What a camera eye or a console's camera shows: the client's main map view looks from
/// `center` (BYOND `client.eye`) and draws static over the tiles of the box that `visible` does
/// not cover (tg `/datum/camerachunk` static images, or the console's `cam_screen`). A headless
/// tgui interface on the camera (console) or on the AI eye.
pub struct CameraViewUi;

impl UiInterface for CameraViewUi {
    const NAME: &'static str = "CameraView";
    type Data = CameraViewData;
    type StaticData = ();
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraViewData {
    pub center: TilePos,
    /// Half side of the box; the box is `(2 * radius + 1)²` tiles around `center`.
    pub radius: u8,
    /// Bit `i` of the row-major box (from its bottom-left corner): tile `i` is visible.
    pub visible: Vec<u8>,
    /// The camera moves the client's view there (a console); `false`: the view follows the
    /// controlled eye anyway (the AI).
    pub moves_view: bool,
}

impl CameraViewData {
    pub fn side(&self) -> i32 {
        2 * i32::from(self.radius) + 1
    }

    /// Whether `pos` is in the box and visible.
    pub fn sees(&self, pos: TilePos) -> bool {
        let r = i32::from(self.radius);
        let (dx, dy) = (pos.x - self.center.x + r, pos.y - self.center.y + r);
        if pos.z != self.center.z
            || !(0..self.side()).contains(&dx)
            || !(0..self.side()).contains(&dy)
        {
            return false;
        }
        let i = (dy * self.side() + dx) as usize;
        self.visible
            .get(i / 8)
            .is_some_and(|b| b & (1 << (i % 8)) != 0)
    }
}

/// tg `tgui_input_list`: pick one of `items`.
pub struct ListInputUi;

impl UiInterface for ListInputUi {
    const NAME: &'static str = "ListInputModal";
    type Data = ();
    type StaticData = ListInputStatic;
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ListInputStatic {
    pub title: String,
    pub message: String,
    pub items: Vec<String>,
}

/// `act('submit', { entry })`.
pub const SUBMIT: &str = "submit";

/// tg `StationAlertConsole` (`/datum/station_alert`): the alarms a listener tracks, by type.
/// Opened by the station alert console, the AI's and a cyborg's alerts button.
pub struct StationAlertUi;

impl UiInterface for StationAlertUi {
    const NAME: &'static str = "StationAlertConsole";
    type Data = StationAlertData;
    type StaticData = ();
}

/// `ui_data`: `cameraView`, `alarms` (a category per tracked type, nominal ones empty), in the
/// tgui's sorting order (fire, atmosphere, power, burglar, motion, camera).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StationAlertData {
    pub camera_view: bool,
    pub alarms: Vec<AlarmCategory>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AlarmCategory {
    /// tg `ALARM_*` name (`"Fire"`, `"Atmosphere"`, ...).
    pub name: String,
    pub alerts: Vec<AlarmAlert>,
}

/// One alarmed area; the counts and the ref only with `camera_view`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AlarmAlert {
    pub name: String,
    pub cameras: Option<u32>,
    pub sources: Option<u32>,
    /// tg `REF(alert)`: an opaque id the `select_camera` act names.
    pub alert_ref: Option<String>,
}

/// `act('select_camera', { alert })`.
pub const SELECT_CAMERA: &str = "select_camera";

/// tg `RoboticsControlConsole`: the cyborgs the user may lock down.
pub struct RoboticsConsoleUi;

impl UiInterface for RoboticsConsoleUi {
    const NAME: &'static str = "RoboticsControlConsole";
    type Data = RoboticsConsoleData;
    type StaticData = ();
}

/// One entry of `ui_data`'s `cyborgs`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoboticsCyborg {
    pub name: String,
    pub locked_down: bool,
    /// tg `stat`: 0 conscious.
    pub status: u8,
    /// Cell charge, percent; `None`: no cell.
    pub charge: Option<u32>,
    /// `"[model.name] Model"`.
    pub module: String,
    /// The master AI's name.
    pub synchronization: Option<String>,
    pub emagged: bool,
    /// tg `REF(R)`: an opaque id the `stopbot` act names.
    pub borg_ref: String,
}

/// `ui_data`: `cyborgs` (tg's `can_hack`, `can_detonate` and `drones` are not sent: no malf
/// AI, no drones here).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoboticsConsoleData {
    pub cyborgs: Vec<RoboticsCyborg>,
}

/// `act('stopbot', { ref })`.
pub const STOPBOT: &str = "stopbot";

/// tg `AiRestorer`: the AI system integrity restorer (`/obj/machinery/computer/aifixer`).
pub struct AiRestorerUi;

impl UiInterface for AiRestorerUi {
    const NAME: &'static str = "AiRestorer";
    type Data = AiRestorerData;
    type StaticData = ();
}

/// `ui_data` (tg `ejectable` is the NTOS app's, always false on the console: not sent).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AiRestorerData {
    pub ai_present: bool,
    /// "Please transfer an AI unit." without an AI.
    pub error: Option<String>,
    pub name: String,
    pub restoring: bool,
    /// `(health + 100) / 2`, percent.
    pub health: f32,
    pub is_dead: bool,
    pub laws: Vec<String>,
}

/// `act('PRG_beginReconstruction')`.
pub const BEGIN_RECONSTRUCTION: &str = "PRG_beginReconstruction";

/// tg `LawRack`: a module rack's slots and links.
pub struct LawRackUi;

impl UiInterface for LawRackUi {
    const NAME: &'static str = "LawRack";
    type Data = LawRackData;
    type StaticData = ();
}

/// tg `MODULE_UNSECURED`, `MODULE_SCREWED`, `MODULE_WELDED`.
pub const SLOT_UNSECURED: u8 = 0;
pub const SLOT_SCREWED: u8 = 1;
pub const SLOT_WELDED: u8 = 2;

/// One entry of `slots` (the first is the core slot when `has_core_slot`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LawSlotData {
    /// tg `MODULE_NAME`: "Empty", the module's name, or "unidentified welded module".
    pub name: String,
    pub security: u8,
    pub ioned: bool,
    pub empty: bool,
}

/// tg `format_linkable`: a name and an opaque ref the link acts name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Linkable {
    pub name: String,
    pub link_ref: String,
}

/// `ui_data` and `ui_static_data` together (the static lists change only on refresh, link and
/// unlink, when tg updates the static data of every viewer).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LawRackData {
    pub slots: Vec<LawSlotData>,
    pub holding_module: bool,
    pub holding_screwdriver: bool,
    pub holding_welder: bool,
    pub holding_multitool: bool,
    pub has_core_slot: bool,
    pub depowered: bool,
    /// Seconds until `refresh` works again.
    pub refresh_cooldown: u32,
    /// A broadcaster's: the rack it feeds, the racks it may link to.
    pub parent_rack: Option<Linkable>,
    pub linkable_racks: Option<Vec<Linkable>>,
    /// A base rack's: who is linked (names), who may be, how many modules the linked
    /// broadcasters add.
    pub linked_mobs: Option<Vec<String>>,
    pub linkable_silicons: Option<Vec<Linkable>>,
    pub linked_rack_module_count: Option<u32>,
}

/// `act(name, { slot })` with the 1-based slot.
pub const INSERT_MODULE: &str = "insert_module";
pub const REMOVE_MODULE: &str = "remove_module";
pub const SCREW_MODULE: &str = "screw_module";
pub const WELD_MODULE: &str = "weld_module";
pub const MULTITOOL_MODULE: &str = "multitool_module";
/// `act('link_silicon', { silicon_ref })`, `act('unlink_silicon', { silicon_index })` (0-based).
pub const LINK_SILICON: &str = "link_silicon";
pub const UNLINK_SILICON: &str = "unlink_silicon";
/// `act('link_rack', { rack_ref })`, `act('unlink_rack')`.
pub const LINK_RACK: &str = "link_rack";
pub const UNLINK_RACK: &str = "unlink_rack";
pub const REFRESH: &str = "refresh";
