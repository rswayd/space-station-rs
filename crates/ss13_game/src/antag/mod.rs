//! Antagonists after tg/BandaStation: roundstart selection by `SSdynamic` ([`dynamic`]), the
//! traitor (`/datum/antagonist/traitor`, [`traitor`]), blood brothers (`/datum/antagonist/brother`
//! and `/datum/team/brother_team`, [`brother`]), their objectives (`/datum/objective/*`,
//! [`objectives`]), the antag HUD ([`hud`]) and the syndicate uplink (`/datum/component/uplink` +
//! `/datum/uplink_handler`, [`uplink`]). This module holds the interfaces both halves share;
//! [`client`] draws them.
//!
//! - `AntagInfoTraitor`: the traitor's info panel (tg `ui_name`), opened on gain and from the
//!   antag info button: introduction, objectives, employer, uplink code. It doubles as the notes
//!   (tg `antag_memory`).
//! - `Uplink`: tg's `Uplink` interface (`GenericUplink`, `PrimaryObjectiveMenu`).
//! - `AntagInfoBrother`: the blood brother's info panel (name and the team's objectives).
//! - `AntagInfoBrainwashed`: the brainwashed victim's directives (`brainwashed`).
//! - `AntagHud` (headless): the antag info action button and the antag HUD icon the client draws
//!   over its own mob and its team mates (tg `antag_hud` and the team's `has_antagonist` HUD).

use serde::{Deserialize, Serialize};
use ss13_shared::IconStateId;
use ss13_shared::protocol::NetId;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::vending::ProductIcon;

pub mod abductor;
#[cfg(feature = "server")]
pub mod blob;
#[cfg(feature = "server")]
pub mod brainwashed;
#[cfg(feature = "server")]
pub mod brother;
pub mod changeling;
#[cfg(feature = "client")]
pub mod client;
pub mod cult;
#[cfg(feature = "server")]
pub mod dynamic;
#[cfg(feature = "server")]
pub mod ert;
pub mod heretic;
#[cfg(feature = "server")]
pub mod hud;
#[cfg(feature = "server")]
pub mod midround;
#[cfg(feature = "server")]
pub mod morph;
#[cfg(feature = "server")]
pub mod nightmare;
pub mod nukeop;
#[cfg(feature = "server")]
pub mod objectives;
#[cfg(feature = "server")]
pub mod paradox_clone;
#[cfg(feature = "server")]
pub mod revenant;
pub mod revolution;
#[cfg(feature = "server")]
pub mod slaughter;
#[cfg(feature = "server")]
pub mod space_dragon;
#[cfg(feature = "server")]
pub mod survivalist;
#[cfg(feature = "server")]
pub mod syndie_items;
#[cfg(feature = "server")]
pub mod traitor;
#[cfg(feature = "server")]
pub mod uplink;
#[cfg(feature = "server")]
pub mod valentine;
pub mod vampire;
pub mod wizard;
#[cfg(feature = "server")]
pub mod zombie;

/// Antagonists on the server: dynamic's state, the traitor and the uplink.
#[cfg(feature = "server")]
pub struct AntagServerPlugin;

#[cfg(feature = "server")]
impl bevy::prelude::Plugin for AntagServerPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        use bevy::prelude::*;
        if !app.world().contains_resource::<dynamic::DynamicConfig>() {
            app.init_resource::<dynamic::DynamicConfig>();
        }
        app.add_plugins((
            hud::AntagHudPlugin,
            uplink::UplinkPlugin,
            syndie_items::SyndieItemsPlugin,
            traitor::TraitorPlugin,
            brother::BrotherPlugin,
            revolution::server::RevolutionPlugin,
            paradox_clone::ParadoxClonePlugin,
            brainwashed::BrainwashedPlugin,
            nightmare::NightmarePlugin,
            morph::MorphPlugin,
            valentine::ValentinePlugin,
            survivalist::SurvivalistPlugin,
            blob::BlobPlugin,
        ))
        .add_plugins((
            space_dragon::SpaceDragonPlugin,
            revenant::RevenantPlugin,
            slaughter::SlaughterPlugin,
        ))
        .add_plugins((zombie::ZombiePlugin, ert::ErtPlugin))
        .add_systems(Startup, dynamic::init)
        .add_plugins(midround::MidroundPlugin);
    }
}

/// tg `ROLE_TRAITOR`: the traitor's `be_special` key.
pub const ROLE_TRAITOR: &str = "Traitor";
/// tg `ROLE_BROTHER`: the blood brother's `be_special` key.
pub const ROLE_BROTHER: &str = "Blood Brother";
/// tg `ROLE_SYNDICATE_INFILTRATOR`: the latejoin traitor's `be_special` key.
pub const ROLE_SYNDICATE_INFILTRATOR: &str = "Syndicate Infiltrator";
/// tg `ROLE_SLEEPER_AGENT`: the midround traitor's `be_special` key.
pub const ROLE_SLEEPER_AGENT: &str = "Syndicate Sleeper Agent";
/// tg `ROLE_STOWAWAY_CHANGELING`: the latejoin changeling's `be_special` key.
pub const ROLE_STOWAWAY_CHANGELING: &str = "Stowaway Changeling";
/// tg `ROLE_OPERATIVE_MIDROUND`: the midround nuclear operative's `be_special` key.
pub const ROLE_OPERATIVE_MIDROUND: &str = "Operative (Midround)";
/// tg `ROLE_MALF`: the malfunctioning AI's `be_special` key (`crate::silicon::malf`).
pub const ROLE_MALF: &str = "Malf AI";
/// tg `ROLE_PARADOX_CLONE`: the paradox clone's `be_special` key (`paradox_clone`).
pub const ROLE_PARADOX_CLONE: &str = "Paradox Clone";
/// tg `ROLE_NIGHTMARE`: the nightmare's `be_special` key (`nightmare`).
pub const ROLE_NIGHTMARE: &str = "Nightmare";
/// tg `ROLE_MORPH` (`morph`); as in tg the morph's poll asks every ghost (no `pref_flag`).
pub const ROLE_MORPH: &str = "Morph";
/// The slaughter demon midround's role (`slaughter`): as in tg, no `pref_flag`, every ghost is
/// polled.
pub const ROLE_SLAUGHTER_DEMON: &str = "Slaughter Demon";
/// tg `ROLE_REVENANT` (`revenant`).
pub const ROLE_REVENANT: &str = "Revenant";
/// tg `ROLE_SPACE_DRAGON` (`space_dragon`).
pub const ROLE_SPACE_DRAGON: &str = "Space Dragon";
/// tg `ROLE_BLOB`: the ghost midround's overmind (`blob`).
pub const ROLE_BLOB: &str = "Blob";
/// tg `ROLE_BLOB_INFECTION`: the living crew member who becomes a blob host (`blob`).
pub const ROLE_BLOB_INFECTION: &str = "Blob Infection";

/// The blood brother's info panel.
pub struct AntagInfoBrotherUi;

impl UiInterface for AntagInfoBrotherUi {
    const NAME: &'static str = "AntagInfoBrother";
    type Data = ();
    type StaticData = BrotherInfo;
}

/// `/datum/antagonist/brother/ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrotherInfo {
    pub antag_name: String,
    pub objectives: Vec<ObjectiveInfo>,
}

/// The brainwashed victim's info panel (`AntagInfoBrainwashed`): its directives.
pub struct AntagInfoBrainwashedUi;

impl UiInterface for AntagInfoBrainwashedUi {
    const NAME: &'static str = "AntagInfoBrainwashed";
    type Data = ();
    /// `objectives`; `antag_name` is the datum's name.
    type StaticData = BrotherInfo;
}

/// The traitor's info panel.
pub struct AntagInfoTraitorUi;

impl UiInterface for AntagInfoTraitorUi {
    const NAME: &'static str = "AntagInfoTraitor";
    type Data = ();
    type StaticData = TraitorInfo;
}

/// `/datum/antagonist/get_objectives()` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveInfo {
    pub count: u32,
    pub explanation: String,
    pub complete: bool,
}

/// `/datum/antagonist/traitor/ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraitorInfo {
    pub has_codewords: bool,
    /// The employer's `ui_theme` (`syndicate`, `ntos`, ...).
    pub theme: String,
    pub intro: String,
    pub allies: String,
    pub goal: String,
    /// The uplink's unlock code.
    pub code: Option<String>,
    /// The uplink's bought failsafe code.
    #[serde(default)]
    pub failsafe_code: Option<String>,
    pub has_uplink: bool,
    pub given_uplink: bool,
    pub uplink_intro: String,
    pub uplink_unlock_info: String,
    pub objectives: Vec<ObjectiveInfo>,
}

/// The syndicate uplink.
pub struct UplinkUi;

impl UiInterface for UplinkUi {
    const NAME: &'static str = "Uplink";
    type Data = UplinkData;
    type StaticData = UplinkStatic;
}

/// An item of the catalog (`/datum/asset/json/uplink` `items` entry, already filtered by the
/// uplink's `uplink_flag` as `Uplink.populateServerData` does).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UplinkItemRecord {
    /// The `/datum/uplink_item` type: the `buy` act's `path`.
    pub id: String,
    pub name: String,
    pub desc: String,
    pub cost: u32,
    pub category: String,
    pub icon: Option<ProductIcon>,
    /// `-1`: unlimited.
    pub limited_stock: i32,
    pub lock_other_purchases: bool,
    pub population_minimum: u32,
}

/// `ui_static_data` plus the catalog asset.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UplinkStatic {
    /// Categories that have items, by weight.
    pub categories: Vec<String>,
    pub items: Vec<UplinkItemRecord>,
    pub lockable: bool,
}

/// `/datum/component/uplink/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UplinkData {
    pub telecrystals: u32,
    pub joined_population: u32,
    /// `primary_objectives`: (`task_name`, `task_text`).
    pub primary_objectives: Vec<(String, String)>,
    pub purchased_items: u32,
    pub shop_locked: bool,
    /// `current_stock`: stock left of limited items, by item id.
    pub current_stock: Vec<(String, i32)>,
    /// `can_renegotiate`: the owner may replace the objectives with a custom one.
    #[serde(default)]
    pub can_renegotiate: bool,
}

impl UplinkData {
    /// `Uplink.render`: `canBuy && hasEnoughPop && !(lock_other_purchases && purchased)`.
    pub fn can_buy(&self, item: &UplinkItemRecord) -> bool {
        let stock = self
            .current_stock
            .iter()
            .find(|(id, _)| *id == item.id)
            .map(|(_, n)| *n);
        let enough_pop =
            self.joined_population == 0 || self.joined_population >= item.population_minimum;
        self.telecrystals >= item.cost
            && stock.is_none_or(|n| n > 0)
            && enough_pop
            && !(item.lock_other_purchases && self.purchased_items > 0)
    }
}

/// `act('buy', { path })`.
pub const BUY: &str = "buy";
/// `act('lock')`.
pub const LOCK: &str = "lock";
/// `act('buy_raw_tc')`: withdraw raw telecrystals (the amount is asked).
pub const BUY_RAW_TC: &str = "buy_raw_tc";
/// `act('renegotiate_objectives')`: pick a custom objective.
pub const RENEGOTIATE_OBJECTIVES: &str = "renegotiate_objectives";

/// The antag HUD of one antagonist mob (no window).
pub struct AntagHudUi;

impl UiInterface for AntagHudUi {
    const NAME: &'static str = "AntagHud";
    type Data = AntagHudData;
    type StaticData = ();
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct AntagHudData {
    /// `icons/mob/huds/antag_hud.dmi` `antag_hud_name`, drawn over the own mob.
    pub hud_icon: Option<IconStateId>,
    /// The team mates this client sees (its ids for them), drawn with `hud_icon` too.
    pub team: Vec<NetId>,
    /// Team mates drawn with their own icon (the revolution's `rev` and `rev_head`).
    #[serde(default)]
    pub team_icons: Vec<(NetId, IconStateId)>,
}

/// The antag info button (tg `/datum/action/antag_info`): opens the info panel.
pub const OPEN_INFO: &str = "open_info";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_buy_follows_the_uplink_view() {
        let item = |id: &str, cost, lock| UplinkItemRecord {
            id: id.into(),
            cost,
            lock_other_purchases: lock,
            limited_stock: -1,
            ..default_record()
        };
        let mut data = UplinkData {
            telecrystals: 5,
            ..Default::default()
        };
        assert!(data.can_buy(&item("a", 5, false)));
        assert!(!data.can_buy(&item("a", 6, false)));
        data.current_stock = vec![("a".into(), 0)];
        assert!(!data.can_buy(&item("a", 1, false)));
        assert!(data.can_buy(&item("b", 1, true)));
        data.purchased_items = 1;
        assert!(!data.can_buy(&item("b", 1, true)));
        let mut rare = item("c", 1, false);
        rare.population_minimum = 30;
        data.joined_population = 10;
        assert!(!data.can_buy(&rare));
    }

    fn default_record() -> UplinkItemRecord {
        UplinkItemRecord::default()
    }
}
