//! Bitrunning, after tg `code/modules/bitrunning/`: the quantum server loads a virtual domain
//! mid-round, bitrunners lie in netpods and play avatars inside it, a delivered cache pays the
//! server's points and a reward crate out of the byteforge.
//!
//! - [`domains`]: the domain catalog (`/datum/lazy_template/virtual_domain` subtypes).
//! - [`server`]: the quantum server (`cold_boot_map`, `reset`, `scrub_vdom`, rewards), the
//!   quantum console (tgui `QuantumConsole`), the byteforge and the domain's caches.
//! - [`avatar`]: netpods, avatars and their connection (`/datum/component/avatar_connection`),
//!   hololadders.
//! - [`client`]: the console window.
//!
//! This module holds the window's data, which both halves share.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "server")]
pub mod avatar;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod domains;
#[cfg(feature = "server")]
pub mod server;

/// tgui `QuantumConsole`.
pub struct QuantumConsoleUi;

impl UiInterface for QuantumConsoleUi {
    const NAME: &'static str = "QuantumConsole";
    type Data = QuantumConsoleData;
    type StaticData = ();
}

/// An `available_domains` entry (`SSbitrunning.get_available_domains`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DomainInfo {
    pub announce_ghosts: bool,
    pub cost: i32,
    pub desc: String,
    pub difficulty: u8,
    pub id: String,
    pub is_modular: bool,
    pub has_secondary_objectives: bool,
    /// [`REDACTED`] when the scanner cannot see it.
    pub name: String,
    /// The reward points, or [`REDACTED`].
    pub reward: String,
    /// `GLOB.bitrunning_grades[best_grade]`: a Font Awesome icon name.
    pub grade_symbol: String,
}

/// An `avatars` entry (`get_avatar_data`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AvatarInfo {
    pub health: f32,
    pub name: String,
    pub pilot: String,
    pub brute: f32,
    pub burn: f32,
    pub tox: f32,
    pub oxy: f32,
}

/// `ui_data` (with tg's `ui_static_data` lists: they follow the points and the avatars).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QuantumConsoleData {
    pub connected: bool,
    pub available_domains: Vec<DomainInfo>,
    pub avatars: Vec<AvatarInfo>,
    /// The loaded domain's key.
    pub generated_domain: Option<String>,
    pub occupants: u32,
    pub points: i32,
    pub randomized: bool,
    pub ready: bool,
    pub retries_left: i32,
    pub scanner_tier: u32,
}

/// `REDACTED`.
pub const REDACTED: &str = "???";

pub const SET_DOMAIN: &str = "set_domain";
pub const DOMAIN_ID: &str = "id";
pub const RANDOM_DOMAIN: &str = "random_domain";
pub const STOP_DOMAIN: &str = "stop_domain";
pub const REFRESH: &str = "refresh";
