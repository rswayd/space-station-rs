//! `SSresearch` (`code/controllers/subsystem/networks/research.dm`): loads the research data and
//! creates the station techweb at startup, and every second (`wait = 1 SECONDS`) gives it
//! `single_server_income` per second while one of its R&D servers (`/obj/machinery/rnd/server`)
//! works (powered, not disabled), then works through the research queue. Researching a node pays
//! the science budget `SSeconomy.techweb_bounty`.
//!
//! Cost: one pass over the servers per second (MetaStation has two).
// shortcut: no server control console (`rdservercontrol`), the master server's HDD theft
// (`income_modifier` stays 1) or the servers' random name suffix.

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_mapload::research::ResearchData;
use ss13_mapload::{DmObj, IconIndex, MapLoadPlugin, Template};
use ss13_proto::Prototypes;
use ss13_tgui::interfaces::vending::ProductIcon;

use super::techweb::{ResearchDb, Techweb};
use crate::cargo::economy::{ACCOUNT_SCI, Economy};
use crate::obj_damage::is_under;
use crate::power::Unpowered;

pub const SERVER_ROOT: &str = "/obj/machinery/rnd/server";
/// `SSresearch` `wait`.
pub const RESEARCH_TICK_SECS: f64 = 1.0;

/// An R&D server of the station techweb.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ResearchServer {
    /// tg `research_disabled`.
    pub research_disabled: bool,
}

/// Before the systems that read [`ResearchDb`] and [`Techweb`] at startup.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResearchLoaded;

pub struct PointsPlugin;

impl Plugin for PointsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (load_research, build_icons, setup_servers)
                .chain()
                .in_set(ResearchLoaded),
        )
        .add_systems(Update, research_tick);
    }
}

/// Reads `<assets>/research.ron` unless a test inserted [`ResearchDb`], then creates the
/// techweb (tg `SSresearch.Initialize`).
pub fn load_research(world: &mut World) {
    if !world.contains_resource::<ResearchDb>() {
        let path = world
            .get_resource::<MapLoadPlugin>()
            .map(|m| m.assets.join(ss13_mapload::research::RESEARCH_FILE));
        let data = match path.as_deref().map(ResearchData::read) {
            Some(Ok(data)) => data,
            Some(Err(e)) => {
                warn!("{e:#}; no techweb (rerun `ss13-mapload convert`)");
                ResearchData::default()
            }
            None => ResearchData::default(),
        };
        world.insert_resource(ResearchDb::new(data));
    }
    let db = world.resource::<ResearchDb>();
    let web = Techweb::science(db);
    info!(
        "research: {} nodes ({} researched at start), {} designs ({} researched)",
        db.nodes.len(),
        web.researched.len(),
        db.designs.len(),
        web.researched_designs.len()
    );
    world.insert_resource(web);
}

/// The pictures of designs (their built type) and materials (their sheet) in the windows, by
/// [`ResearchDb`] index.
#[derive(Resource, Debug, Default)]
pub struct ResearchIcons {
    pub designs: Vec<Option<ProductIcon>>,
    pub materials: Vec<Option<ProductIcon>>,
}

fn build_icons(world: &mut World) {
    let index: HashMap<(&str, &str), ss13_shared::IconStateId> = world
        .get_resource::<IconIndex>()
        .map(|i| {
            i.0.iter()
                .map(|((dmi, state), id)| ((dmi.as_str(), state.as_str()), *id))
                .collect()
        })
        .unwrap_or_default();
    let protos = world.get_resource::<Prototypes>();
    // The south-facing first frame of the prototype.
    let icon = |path: &str| {
        let proto = protos?.get(path)?.clone();
        let appearance = Template::new(proto, &index).appearance?;
        Some(ProductIcon {
            icon_state: appearance.icon_state,
            color: appearance.color,
        })
    };
    let db = world.resource::<ResearchDb>();
    let designs = db
        .designs
        .iter()
        .map(|d| d.build_path.as_deref().and_then(icon))
        .collect();
    let materials = db
        .materials
        .iter()
        .map(|m| m.sheet_type.as_deref().and_then(icon))
        .collect();
    world.insert_resource(ResearchIcons { designs, materials });
}

fn setup_servers(world: &mut World) {
    let mut query = world.query::<(Entity, &DmObj)>();
    let servers: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, SERVER_ROOT))
        .map(|(e, _)| e)
        .collect();
    info!("research: {} R&D servers", servers.len());
    for e in servers {
        world.entity_mut(e).insert(ResearchServer::default());
    }
}

/// tg `SSresearch.fire()`.
fn research_tick(
    time: Res<Time>,
    mut next: Local<f64>,
    db: Option<Res<ResearchDb>>,
    web: Option<ResMut<Techweb>>,
    servers: Query<(&ResearchServer, Has<Unpowered>, Has<crate::emp::Emped>)>,
    economy: Option<ResMut<Economy>>,
) {
    let (Some(db), Some(mut web)) = (db, web) else {
        return;
    };
    let now = time.elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + RESEARCH_TICK_SECS;
    // `refresh_working`: `machine_stat & (NOPOWER|EMPED)` or `research_disabled` stop a server.
    let working = servers
        .iter()
        .any(|(s, unpowered, emped)| !unpowered && !emped && !s.research_disabled);
    web.generate_points(&db, working, now);
    if web.queue.is_empty() {
        return;
    }
    let done = web.process_queue(&db);
    if let Some(mut economy) = economy {
        for _ in &done {
            pay_bounty(&db, &mut economy);
        }
    }
}

/// `get_that_dosh`: the science budget gains the techweb bounty.
pub fn pay_bounty(db: &ResearchDb, economy: &mut Economy) {
    if let Some(account) = economy.department_mut(ACCOUNT_SCI) {
        account.adjust_money(db.techweb_bounty as i64);
    }
}
