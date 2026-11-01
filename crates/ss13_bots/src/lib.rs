//! Functional test bots (docs/RUNNING.md, "Functional test bots"): headless clients that play
//! scripted scenarios ([`case`] files) against in-process servers over the real transport, a
//! pool of long-lived servers that runs cases in parallel ([`pool`]), and reports ([`report`]).
//!
//! A bot is a real client `App` ([`bot`]): it sends only `ClientIntent`s and sees what a player
//! sees (replication, chat, tgui windows, HUDs). Assertions on server state go through the debug
//! channel's test hooks (`ss13_debug::test_hooks`), open only on debug servers.

pub mod admin;
pub mod bot;
pub mod case;
pub mod cond;
pub mod logcap;
pub mod pool;
pub mod report;
pub mod run;
pub mod server;
