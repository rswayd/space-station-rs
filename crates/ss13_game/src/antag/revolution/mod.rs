//! The revolution (tg `/datum/antagonist/rev`, `/datum/antagonist/rev/head`,
//! `/datum/team/revolution`, `/datum/revolution_handler`,
//! `code/modules/antagonists/revolution/`, BandaStation `revolution_overhaul`). [`server`] is the
//! antagonist, the flash conversion, the mindshield implanter and the outcome; [`client`] draws
//! the info panel.
//!
//! - `AntagInfoRevolution`: tg's `ui_name` of the revolutionary; the same fields as the generic
//!   panel (the name and the team's objectives).
// shortcut: the panel has no head revolutionary's conversion objective input (tg
// `conversion_objective`); add it with that act.

use ss13_tgui::UiInterface;

use super::BrotherInfo;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

/// tg `ROLE_REV_HEAD`: the roundstart head revolutionary's `be_special` key.
pub const ROLE_REV_HEAD: &str = "Head Revolutionary";
/// tg `ROLE_PROVOCATEUR`: the latejoin head revolutionary's `be_special` key.
pub const ROLE_PROVOCATEUR: &str = "Provocateur";

/// The revolutionary's info panel.
pub struct AntagInfoRevolutionUi;

impl UiInterface for AntagInfoRevolutionUi {
    const NAME: &'static str = "AntagInfoRevolution";
    type Data = ();
    type StaticData = BrotherInfo;
}
