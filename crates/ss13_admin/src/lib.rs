//! Admin tools after tg `/datum/admins` and BandaStation: ranks and rights from
//! `admin_ranks.txt`/`admins.txt` ([`rights`]), the admin verbs, adminhelp tickets, asay, the
//! permissions panel, admin sounds and atmos debug ([`verbs`]) and the admin log (`server` feature), and the admin panel with the F1 adminhelp window (`client`
//! feature). See `docs/RUNNING.md`, "Admins".

pub mod rights;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod tickets;
#[cfg(feature = "server")]
pub mod verbs;
