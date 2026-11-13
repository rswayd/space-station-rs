//! The ghost's windows, shared by the server (`crate::ghost_verbs`) and the client
//! (`crate::ghost_menu_client`): tg's observer HUD buttons ([`GhostHudUi`], headless, open while
//! the client is a ghost), the Orbit menu ([`OrbitUi`], `/datum/orbit_menu`), the ghost menu
//! ([`GhostMenuUi`], `/datum/ghost_menu`) and the spawners menu ([`SpawnersMenuUi`],
//! `/datum/spawners_menu`).

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tg `GLOB.ghost_lightings`: names and `LIGHTING_CUTOFF_*` (percent added to the light).
pub const GHOST_LIGHTINGS: [(&str, u8); 4] = [
    ("Стандартное", 0),
    ("Темнее", 15),
    ("Ночное зрение", 30),
    ("Полное освещение", 100),
];

/// The observer HUD (tg `/datum/hud/ghost` buttons).
pub struct GhostHudUi;

impl UiInterface for GhostHudUi {
    const NAME: &'static str = "GhostHud";
    type Data = GhostHudData;
    type StaticData = ();
}

/// What the client's HUD and lighting need while it is a ghost.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhostHudData {
    /// The ghost's `lighting_cutoff` (one of [`GHOST_LIGHTINGS`]): the client's lighting floor.
    pub lighting_cutoff: u8,
    /// It may re-enter a body (the "Re-enter Corpse" button).
    pub has_body: bool,
}

/// [`GhostHudUi`] acts: the HUD buttons.
pub const HUD_ORBIT: &str = "orbit";
pub const HUD_GHOST_MENU: &str = "ghost_menu";
pub const HUD_SPAWNERS: &str = "spawners";
pub const HUD_REENTER: &str = "reenter_corpse";
pub const HUD_RESPAWN: &str = "respawn";
/// tg `/atom/movable/screen/ghost/minigames_menu`: the minigames menu (`crate::minigames`).
pub const HUD_MINIGAMES: &str = "minigames";

/// tgui `Orbit`.
pub struct OrbitUi;

impl UiInterface for OrbitUi {
    const NAME: &'static str = "Orbit";
    type Data = OrbitData;
    type StaticData = ();
}

/// One point of interest (tg `get_mob_pois` entry).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrbitEntry {
    pub full_name: String,
    /// The thing, as the `orbit` act names it back; the server checks it is still a POI.
    #[serde(rename = "ref")]
    pub key: u64,
}

/// `/datum/orbit_menu/ui_data`, without the antagonist and admin lists.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrbitData {
    pub alive: Vec<OrbitEntry>,
    pub dead: Vec<OrbitEntry>,
    pub ghosts: Vec<OrbitEntry>,
    pub npcs: Vec<OrbitEntry>,
    /// `orbiting`: what the viewer orbits now.
    pub orbiting: Option<u64>,
    /// What the viewer observes now (tg `observetarget`).
    pub observing: Option<u64>,
}

/// `act('orbit', { ref, auto_observe })`.
pub const ORBIT: &str = "orbit";
pub const ORBIT_REF: &str = "ref";
pub const ORBIT_AUTO_OBSERVE: &str = "auto_observe";
/// `act('refresh')`.
pub const ORBIT_REFRESH: &str = "refresh";

/// tgui `GhostMenu`.
pub struct GhostMenuUi;

impl UiInterface for GhostMenuUi {
    const NAME: &'static str = "GhostMenu";
    type Data = GhostMenuData;
    type StaticData = ();
}

/// `/datum/ghost_menu/ui_data`: the settings this port has.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhostMenuData {
    pub can_boo: bool,
    /// tg `fun_verbs`: Boo and Possess.
    pub has_fun: bool,
    /// The body it may re-enter.
    pub body_name: Option<String>,
    /// Index into [`GHOST_LIGHTINGS`].
    pub current_darkness: u8,
    /// `ghost_hud_flags & GHOST_VISION`.
    pub ghost_vision: bool,
    /// `ghost_hud_flags & GHOST_HEALTH`: a click on a mob prints its health scan.
    pub health_scan: bool,
    /// tg `possess`'s list (only with `has_fun`): living mobs without a player.
    pub possessable: Vec<OrbitEntry>,
}

pub const MENU_DNR: &str = "DNR";
pub const MENU_RETURN: &str = "return_to_body";
/// `act('darkness', { darkness_level })`: a name of [`GHOST_LIGHTINGS`].
pub const MENU_DARKNESS: &str = "darkness";
pub const MENU_DARKNESS_LEVEL: &str = "darkness_level";
/// `act('toggle_visibility', { toggling })`.
pub const MENU_TOGGLE: &str = "toggle_visibility";
pub const MENU_TOGGLING: &str = "toggling";
/// tg `GHOST_VISION` and `GHOST_HEALTH` flag names.
pub const GHOST_VISION: &str = "ghost_vision";
pub const GHOST_HEALTH: &str = "health_scan";
/// `act('tray_scan')`: one T-ray pulse around the ghost.
pub const MENU_TRAY: &str = "tray_scan";
pub const MENU_BOO: &str = "boo";
/// `act('possess', { ref })`: a [`GhostMenuData::possessable`] entry.
pub const MENU_POSSESS: &str = "possess";

/// tgui `SpawnersMenu`.
pub struct SpawnersMenuUi;

impl UiInterface for SpawnersMenuUi {
    const NAME: &'static str = "SpawnersMenu";
    type Data = SpawnersData;
    type StaticData = ();
}

/// One group of spawners with the same role name (`/datum/spawners_menu/ui_data`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnerEntry {
    pub name: String,
    pub desc: String,
    pub amount_left: u32,
    /// A spawner of the group with uses left.
    #[serde(rename = "ref")]
    pub key: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnersData {
    pub spawners: Vec<SpawnerEntry>,
}

/// `act('jump' | 'spawn', { name })`: tg names the group; here its spawner's key.
pub const SPAWNER_JUMP: &str = "jump";
pub const SPAWNER_SPAWN: &str = "spawn";
pub const SPAWNER_REF: &str = "ref";
