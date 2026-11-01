//! Server half: answers the debug overlay's queries and applies its dev tools.
//!
//! Gate ([`DebugGate`], from the environment):
//! - queries (tile details, server stats, prototype search): open in debug builds, or in any
//!   build with `SS13_DEBUG_QUERIES=1`; `SS13_DEBUG_QUERIES=0` closes them in a debug build;
//! - dev tools (teleport, spawn, god mode, round end, time scale): queries open *and* a debug
//!   build. A release server refuses them whatever the environment says.
//!
//! Tile queries are not limited to what the asking client could see: the gate is the limit. A
//! server that opens queries to untrusted players leaks the whole station (atmos, power, every
//! entity), so `SS13_DEBUG_QUERIES=1` is for trusted test servers only.
//!
//! Requests are rate-limited per client ([`RATE_PER_SEC`], burst [`BURST`]); the excess is
//! dropped without an answer.

use std::collections::VecDeque;
use std::time::Instant;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use ss13_atmos::{Atmos, GASES, GasId};
use ss13_game::health::{Health, HealthSystems, Stat};
use ss13_game::power::{AreaPower, Channel};
use ss13_game::round::Ticker;
use ss13_mapload::{Areas, DmAtom, DmName, Turf};
use ss13_net::debug::server::{DebugRequestReceived, SendDebugResponse};
use ss13_net::server::{
    ClientDisconnected, ClientToken, ControlledEntity, Interest, ReplicationStats,
};
use ss13_proto::Prototypes;
use ss13_shared::TilePos;
use ss13_shared::debug::{
    AirReport, AreaReport, ClientStats, DebugRequest, DebugResponse, EntityReport,
    MAX_PROTOTYPE_MATCHES, ServerStats, TileReport,
};
use ss13_world::SpatialIndex;

/// Sustained requests per second per client. The overlay polls stats once a second and the
/// hovered tile at most four times a second.
pub const RATE_PER_SEC: f32 = 10.0;
pub const BURST: f32 = 20.0;
/// Ticks the tick time percentiles are taken over (10 s at 20 Hz).
const TICK_WINDOW: usize = 200;
pub(crate) const TIME_SCALE_RANGE: (f32, f32) = (0.1, 10.0);

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugGate {
    pub queries: bool,
    /// Effective only in a debug build (see the module docs).
    pub dev_tools: bool,
}

impl DebugGate {
    /// `SS13_DEBUG_QUERIES`: `1` opens, `0` closes, unset follows the build.
    pub fn from_env() -> Self {
        let queries = match std::env::var("SS13_DEBUG_QUERIES").as_deref() {
            Ok("1") => true,
            Ok("0") => false,
            _ => cfg!(debug_assertions),
        };
        Self {
            queries,
            dev_tools: queries,
        }
    }

    fn dev_tools(self) -> bool {
        cfg!(debug_assertions) && self.queries && self.dev_tools
    }
}

/// On a mob: tg `GODMODE`, no damage.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct GodMode;

#[derive(Resource, Default)]
struct TickTimes {
    start: Option<Instant>,
    recent: VecDeque<f32>,
}

#[derive(Resource, Default)]
struct Buckets(HashMap<Entity, (f32, f64)>);

pub struct DebugServerPlugin;

impl Plugin for DebugServerPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<DebugGate>() {
            app.insert_resource(DebugGate::from_env());
        }
        let gate = *app.world().resource::<DebugGate>();
        if gate.queries {
            warn!(
                "debug queries open (dev tools {}): any client can read tile details",
                if gate.dev_tools() { "on" } else { "off" }
            );
        }
        app.init_resource::<TickTimes>()
            .init_resource::<Buckets>()
            .add_systems(First, |mut t: ResMut<TickTimes>| {
                t.start = Some(Instant::now())
            })
            .add_systems(Last, end_tick)
            .add_systems(Update, handle_requests)
            // After every damage system (they run before `HealthSystems`), and again at the end
            // of the tick for damage from elsewhere.
            .add_systems(Update, god_mode.in_set(HealthSystems))
            .add_systems(PostUpdate, god_mode);
    }
}

fn end_tick(mut t: ResMut<TickTimes>) {
    let Some(start) = t.start.take() else {
        return;
    };
    if t.recent.len() == TICK_WINDOW {
        t.recent.pop_front();
    }
    t.recent.push_back(start.elapsed().as_secs_f32() * 1000.0);
}

// shortcut: health is restored after the damage instead of the damage being skipped, so one hit
// over the full crit range (200 damage in a tick) still kills, and crit effects may show for a
// tick; a real `GODMODE` check belongs in `ss13_game::health::apply_damage` once a trait system
// exists there.
fn god_mode(mut mobs: Query<(&mut Health, Option<&mut Stat>), With<GodMode>>) {
    for (mut health, stat) in &mut mobs {
        let healed = health.healed();
        health.set_if_neq(healed);
        if let Some(mut stat) = stat
            && *stat != Stat::Dead
        {
            stat.set_if_neq(Stat::Conscious);
        }
    }
}

/// Exclusive: spawning needs the world, and the tile report reads arbitrary components.
fn handle_requests(world: &mut World) {
    let requests: Vec<DebugRequestReceived> = world
        .resource_mut::<Messages<DebugRequestReceived>>()
        .drain()
        .collect();
    let gone: Vec<Entity> = world
        .resource_mut::<Messages<ClientDisconnected>>()
        .iter_current_update_messages()
        .map(|d| d.client)
        .collect();
    {
        let mut buckets = world.resource_mut::<Buckets>();
        for client in gone {
            buckets.0.remove(&client);
        }
    }
    if requests.is_empty() {
        return;
    }
    let gate = *world.resource::<DebugGate>();
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    for DebugRequestReceived { client, request } in requests {
        // Test hooks poll conditions; they exist only with dev tools (trusted debug servers).
        if let DebugRequest::Test { id, command } = &request
            && gate.queries
            && gate.dev_tools()
        {
            let result = crate::test_hooks::answer(world, client, command);
            let response = DebugResponse::Test { id: *id, result };
            world.write_message(SendDebugResponse { client, response });
            continue;
        }
        if !take_token(&mut world.resource_mut::<Buckets>(), client, now) {
            debug!("debug request from {client} dropped: over the rate limit");
            continue;
        }
        let response = if !gate.queries {
            DebugResponse::Refused("debug queries are disabled on this server".into())
        } else {
            answer(world, gate, client, request)
        };
        world.write_message(SendDebugResponse { client, response });
    }
}

fn take_token(buckets: &mut Buckets, client: Entity, now: f64) -> bool {
    let (tokens, last) = buckets.0.entry(client).or_insert((BURST, now));
    *tokens = (*tokens + (now - *last) as f32 * RATE_PER_SEC).min(BURST);
    *last = now;
    if *tokens < 1.0 {
        return false;
    }
    *tokens -= 1.0;
    true
}

fn answer(
    world: &mut World,
    gate: DebugGate,
    client: Entity,
    request: DebugRequest,
) -> DebugResponse {
    let dev = matches!(
        request,
        DebugRequest::Teleport(_)
            | DebugRequest::Spawn { .. }
            | DebugRequest::GodMode(_)
            | DebugRequest::EndRound
            | DebugRequest::TimeScale(_)
    );
    if dev && !gate.dev_tools() {
        return DebugResponse::Refused("dev tools need a debug build of the server".into());
    }
    match request {
        DebugRequest::Tile(pos) => DebugResponse::Tile(tile_report(world, client, pos)),
        DebugRequest::ServerStats => DebugResponse::ServerStats(server_stats(world)),
        DebugRequest::FindPrototypes(query) => {
            let Some(prototypes) = world.get_resource::<Prototypes>() else {
                return DebugResponse::Refused("no prototypes loaded".into());
            };
            let needle = query.to_lowercase();
            let mut ids: Vec<String> = prototypes
                .ids()
                .filter(|id| id.starts_with("/obj") && id.to_lowercase().contains(&needle))
                .map(str::to_owned)
                .collect();
            ids.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
            ids.truncate(MAX_PROTOTYPE_MATCHES);
            DebugResponse::Prototypes { query, ids }
        }
        DebugRequest::Teleport(pos) => {
            let Some(mob) = world.get::<ControlledEntity>(client).map(|c| c.0) else {
                return DebugResponse::Refused("no mob to teleport".into());
            };
            if !on_map(world, pos) {
                return DebugResponse::Refused(format!("{pos:?} is off the map"));
            }
            if world.get_entity(mob).is_err() {
                return DebugResponse::Refused("no mob to teleport".into());
            }
            world.entity_mut(mob).insert(pos);
            info!("dev: {client} teleported to {pos:?}");
            DebugResponse::Done(format!("teleported to ({}, {}, {})", pos.x, pos.y, pos.z))
        }
        DebugRequest::Spawn { path, pos } => {
            let known = world
                .get_resource::<Prototypes>()
                .is_some_and(|p| p.get(&path).is_some());
            if !known || !path.starts_with("/obj") {
                return DebugResponse::Refused(format!("{path} is not a spawnable /obj prototype"));
            }
            if !on_map(world, pos) {
                return DebugResponse::Refused(format!("{pos:?} is off the map"));
            }
            match ss13_game::construction::spawn_obj(world, &path, pos) {
                Some(_) => {
                    info!("dev: {client} spawned {path} at {pos:?}");
                    DebugResponse::Done(format!(
                        "spawned {path} at ({}, {}, {})",
                        pos.x, pos.y, pos.z
                    ))
                }
                None => DebugResponse::Refused(format!("{path} could not be spawned")),
            }
        }
        DebugRequest::GodMode(on) => {
            let Some(mob) = world.get::<ControlledEntity>(client).map(|c| c.0) else {
                return DebugResponse::Refused("no mob".into());
            };
            let Ok(mut mob) = world.get_entity_mut(mob) else {
                return DebugResponse::Refused("no mob".into());
            };
            if on {
                mob.insert(GodMode);
            } else {
                mob.remove::<GodMode>();
            }
            info!("dev: {client} god mode {on}");
            DebugResponse::Done(format!("god mode {}", if on { "on" } else { "off" }))
        }
        DebugRequest::EndRound => match world.get_resource_mut::<Ticker>() {
            Some(mut ticker) => {
                ticker.force_ending = true;
                info!("dev: {client} ended the round");
                DebugResponse::Done("round ending".into())
            }
            None => DebugResponse::Refused("no round ticker".into()),
        },
        DebugRequest::TimeScale(scale) => {
            if !scale.is_finite() || !(TIME_SCALE_RANGE.0..=TIME_SCALE_RANGE.1).contains(&scale) {
                return DebugResponse::Refused(format!(
                    "time scale must be in {:?}",
                    TIME_SCALE_RANGE
                ));
            }
            world
                .resource_mut::<Time<Virtual>>()
                .set_relative_speed(scale);
            info!("dev: {client} set time scale {scale}");
            DebugResponse::Done(format!("time scale {scale}"))
        }
        // Answered before the rate limit in `handle_requests` when dev tools are on; refused
        // here with its id so the asking bot can tell which request failed.
        DebugRequest::Test { id, .. } => DebugResponse::Test {
            id,
            result: Err("dev tools need a debug build of the server".into()),
        },
    }
}

/// A tile of a loaded level (inside its area grid).
pub(crate) fn on_map(world: &World, pos: TilePos) -> bool {
    world
        .get_resource::<Areas>()
        .is_some_and(|a| a.area_at(pos).is_some())
}

pub(crate) fn tile_report(world: &World, client: Entity, pos: TilePos) -> TileReport {
    let area = world
        .get_resource::<Areas>()
        .and_then(|areas| areas.area_at(pos))
        .map(|area| {
            let name = world
                .get::<DmName>(area)
                .map_or_else(String::new, |n| n.name.clone());
            match world.get::<AreaPower>(area) {
                Some(power) => AreaReport {
                    name,
                    requires_power: power.requires_power,
                    powered: Channel::ALL.map(|c| power.powered(c)),
                    load: [0, 1, 2].map(|i| power.static_power[i] + power.used[i]),
                    has_apc: power.apc.is_some(),
                },
                None => AreaReport { name, ..default() },
            }
        });
    let air = world
        .get_resource::<Atmos>()
        .and_then(|atmos| atmos.air(pos))
        .map(|mix| AirReport {
            pressure_kpa: mix.pressure(),
            temperature_k: mix.temperature,
            total_moles: mix.total_moles(),
            gases: (0..GASES.len())
                .filter_map(|i| {
                    let gas = GasId(i as u8);
                    let moles = mix.get(gas);
                    (moles > 0.0)
                        .then(|| (gas.info().id.to_owned(), moles, mix.partial_pressure(gas)))
                })
                .collect(),
        });
    let interest = world.get_resource::<Interest>();
    let mut turf = String::new();
    let entities = world
        .get_resource::<SpatialIndex>()
        .map(|index| index.on_tile(pos).to_vec())
        .unwrap_or_default()
        .into_iter()
        .map(|e| {
            let name = world
                .get::<DmName>(e)
                .map_or_else(String::new, |n| n.name.clone());
            if world.get::<Turf>(e).is_some() {
                turf.clone_from(&name);
            }
            let (icon, icon_state) = world
                .get::<DmAtom>(e)
                .map_or_else(Default::default, |a| (a.icon.clone(), a.icon_state.clone()));
            let components = world
                .inspect_entity(e)
                .map(|infos| {
                    infos
                        .map(|(_, info)| short_type_name(&info.name().to_string()))
                        .collect()
                })
                .unwrap_or_default();
            EntityReport {
                net_id: interest.and_then(|i| i.net_id(client, e)),
                name,
                icon,
                icon_state,
                components,
            }
        })
        .collect();
    TileReport {
        pos,
        turf,
        area,
        air,
        entities,
    }
}

/// `ss13_mapload::Turf` → `Turf`; generics keep their last segments (`Foo<bar::Baz>` → `Foo<Baz>`).
fn short_type_name(full: &str) -> String {
    let mut out = String::with_capacity(full.len());
    let mut segment = String::new();
    for c in full.chars() {
        match c {
            ':' => segment.clear(),
            '<' | '>' | ',' | ' ' | '(' | ')' | '[' | ']' | ';' | '&' => {
                out.push_str(&segment);
                segment.clear();
                out.push(c);
            }
            _ => segment.push(c),
        }
    }
    out.push_str(&segment);
    out
}

fn server_stats(world: &mut World) -> ServerStats {
    let mut ticks: Vec<f32> = world
        .resource::<TickTimes>()
        .recent
        .iter()
        .copied()
        .collect();
    ticks.sort_by(f32::total_cmp);
    let pct = |p: f32| {
        if ticks.is_empty() {
            0.0
        } else {
            ticks[((ticks.len() - 1) as f32 * p).round() as usize]
        }
    };
    let mut tokens = world.query::<(Entity, &ClientToken)>();
    let traffic = world.get_resource::<ReplicationStats>();
    let interest = world.get_resource::<Interest>();
    let mut clients: Vec<ClientStats> = Vec::new();
    for (client, token) in tokens.iter(world) {
        let t = traffic.and_then(|s| s.clients.get(&client));
        clients.push(ClientStats {
            name: token.0.clone(),
            bytes_per_sec: t.map_or(0.0, |t| t.bytes_per_sec),
            packets_per_sec: t.map_or(0.0, |t| t.packets_per_sec),
            visible: interest.map_or(0, |i| i.visible_count(client) as u32),
        });
    }
    clients.sort_by(|a, b| a.name.cmp(&b.name));
    ServerStats {
        tick_p50_ms: pct(0.5),
        tick_p99_ms: pct(0.99),
        tick_max_ms: ticks.last().copied().unwrap_or(0.0),
        entities: world.entities().count_spawned(),
        time_scale: world.resource::<Time<Virtual>>().relative_speed(),
        round_state: world
            .get_resource::<Ticker>()
            .map_or_else(|| "none".into(), |t| format!("{:?}", t.state())),
        clients,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_allows_a_burst_then_the_rate() {
        let mut buckets = Buckets::default();
        let client = Entity::from_raw_u32(7).unwrap();
        let allowed = (0..30)
            .filter(|_| take_token(&mut buckets, client, 0.0))
            .count();
        assert_eq!(allowed, BURST as usize);
        // One second later: RATE_PER_SEC more.
        let allowed = (0..30)
            .filter(|_| take_token(&mut buckets, client, 1.0))
            .count();
        assert_eq!(allowed, RATE_PER_SEC as usize);
    }

    #[test]
    fn type_names_are_shortened() {
        assert_eq!(short_type_name("ss13_mapload::Turf"), "Turf");
        assert_eq!(
            short_type_name("bevy::Foo<ss13_shared::tile::TilePos>"),
            "Foo<TilePos>"
        );
    }
}
