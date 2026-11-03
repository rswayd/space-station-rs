//! Atmos glue (server): gas overlay entities replicate, and gas analyzers (tg
//! `/obj/item/analyzer`, `gas_analyzer.dm`): `atmos_scan` to their user's chat for whatever
//! holds gas (`return_analyzable_air`: a turf, a canister, a stationary tank, a pipe, a
//! machine's ports, a gas tank), the `GasAnalyzer` window of the last scan with the atmos
//! handbook, and the long-range analyzer's 15-tile reach. The simulation is `ss13_atmos`.
// shortcut: no anomaly scanning, no scan sound or message to onlookers; a mixture's
// "reactions" are its fire reactions only. The barometer finds no weather (none is ported),
// which is what tg's says when no storm is coming.

use std::sync::Arc;

use bevy::prelude::*;
use ss13_atmos::pipenet::Pipenet;
use ss13_atmos::scan::{ScanMix, scan_mixes};
use ss13_atmos::{Atmos, GASES, GasMixture, GasOverlay};
use ss13_mapload::{DmItem, DmName, MapLoadPlugin, Turf};
use ss13_net::chat::server::SendChat;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::gas_analyzer::{AtmosHandbook, GasAnalyzerData, GasAnalyzerUi, Gasmix};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject};
use ss13_world::SpatialIndex;

use crate::atmos_machinery::AtmosDevice;
use crate::atmos_machinery::portable::PortableAtmos;
use crate::atmos_machinery::tank::StationaryTank;
use crate::internals::TankAir;

/// `/obj/item/analyzer` and subtypes.
const ANALYZER_ROOT: &str = "/obj/item/analyzer";
const RANGED_ANALYZER: &str = "/obj/item/analyzer/ranged";
/// `ranged_scan_distance` of the two.
const SCAN_RANGE: u32 = 1;
const RANGED_SCAN_RANGE: u32 = 15;

pub struct AtmosGluePlugin;

impl Plugin for AtmosGluePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(network_gas_overlay)
            .register_ui::<AnalyzerWindow>()
            .add_systems(Startup, load_handbook)
            .add_systems(
                Update,
                (ranged_scans, refresh_analyzers, barometer_pings)
                    .after(crate::click::ResolveClicks)
                    .before(ss13_tgui::server::UiSystems::Send),
            );
    }
}

fn network_gas_overlay(add: On<Add<GasOverlay>>, mut commands: Commands) {
    commands.entity(add.entity).insert(Networked);
}

/// `return_atmos_handbooks()`, read once from `<assets>/atmos_handbook.ron`.
#[derive(Resource, Clone, Default)]
pub struct Handbook(pub Arc<AtmosHandbook>);

pub(crate) fn load_handbook(world: &mut World) {
    if world.contains_resource::<Handbook>() {
        return;
    }
    let book = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| ss13_mapload::atmos_handbook::load(&m.assets))
        .unwrap_or_default();
    world.insert_resource(Handbook(Arc::new(book)));
}

pub(crate) fn is_analyzer(world: &World, item: Entity) -> bool {
    world.get::<DmItem>(item).is_some_and(|i| {
        i.path
            .strip_prefix(ANALYZER_ROOT)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

fn scan_range(world: &World, item: Entity) -> u32 {
    let ranged = world
        .get::<DmItem>(item)
        .is_some_and(|i| crate::obj_damage::is_under(&i.path, RANGED_ANALYZER));
    if ranged {
        RANGED_SCAN_RANGE
    } else {
        SCAN_RANGE
    }
}

/// An analyzer's memory: the last scanned atom and its readings (`last_scanned`,
/// `last_gasmix_data`), and the handbook its window shows.
#[derive(Component, Clone)]
pub struct AnalyzerMemory {
    pub handbook: Arc<AtmosHandbook>,
    pub last: Option<Entity>,
    pub gasmixes: Vec<Gasmix>,
}

/// One mixture an atom holds.
#[derive(Clone, Debug)]
pub struct Held {
    pub gas: GasMixture,
    pub volume: f64,
    pub turf: bool,
}

/// `return_analyzable_air()` of `target`: `None` when it holds no gas.
pub fn analyzable(world: &World, target: Entity) -> Option<Vec<Held>> {
    let net = world.get_resource::<Pipenet>();
    if world.get::<Turf>(target).is_some() {
        let pos = world.get::<TilePos>(target)?;
        let air = world.get_resource::<Atmos>()?.air(*pos)?;
        return Some(vec![Held {
            gas: air.clone(),
            volume: ss13_atmos::gas::CELL_VOLUME,
            turf: true,
        }]);
    }
    let pipe = |air: &ss13_atmos::pipenet::air::PipeAir| Held {
        gas: air.gas.clone(),
        volume: air.volume,
        turf: false,
    };
    if let Some(p) = world.get::<PortableAtmos>(target) {
        return p.air(net?).map(|a| vec![pipe(a)]);
    }
    if let Some(t) = world.get::<StationaryTank>(target) {
        return net?.tank_group(t.group).map(|g| vec![pipe(&g.air)]);
    }
    if let Some(tank) = world.get::<TankAir>(target) {
        return Some(vec![Held {
            gas: tank.air.clone(),
            volume: tank.volume,
            turf: false,
        }]);
    }
    let device = world.get::<AtmosDevice>(target)?;
    let net = net?;
    let d = net.device(device.id)?;
    if d.is_pipe() {
        // `pipe/return_analyzable_air`: its pipeline's air.
        return net.pipeline_air(device.id, 0).map(|a| vec![pipe(a)]);
    }
    (!d.airs.is_empty()).then(|| d.airs.iter().map(pipe).collect())
}

/// `capitalize(LOWER_TEXT(name))`, without BYOND's text macros.
fn display_name(world: &World, e: Entity) -> String {
    world.get::<DmName>(e).map_or_else(
        || "the floor".to_owned(),
        |n| strip_text_macros(&n.name).to_owned(),
    )
}

fn capitalized(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut chars = lower.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// `gas_mixture_parser()` of every mixture (`" - Node N"` for machines with several).
fn gasmixes(world: &World, target: Entity, held: &[Held]) -> Vec<Gasmix> {
    let base = capitalized(&display_name(world, target));
    held.iter()
        .enumerate()
        .map(|(i, h)| {
            let name = if held.len() == 1 {
                base.clone()
            } else {
                format!("{base} - Node {}", i + 1)
            };
            let mix = ScanMix {
                gas: &h.gas,
                volume: h.volume,
                turf: h.turf,
            };
            let reactions = ss13_atmos::reactions::fire_results(&h.gas)
                .into_iter()
                .map(|(id, name, amount)| (id.to_owned(), name.to_owned(), amount as f32))
                .collect();
            Gasmix {
                name,
                gases: GASES
                    .iter()
                    .zip(&h.gas.moles)
                    .filter(|(_, m)| **m != 0.0)
                    .map(|(g, &m)| (g.id.to_owned(), g.name.to_owned(), m as f32))
                    .collect(),
                temperature: h.gas.temperature as f32,
                volume: h.volume as f32,
                pressure: mix.pressure() as f32,
                total_moles: h.gas.total_moles() as f32,
                reactions,
                reference: format!("{}:{i}", target.to_bits()),
            }
        })
        .collect()
}

/// `gas_mixture_parser(air, name)` of one mixture.
pub(crate) fn gasmix_named(name: &str, owner: Entity, held: &Held) -> Option<Gasmix> {
    let mix = ScanMix {
        gas: &held.gas,
        volume: held.volume,
        turf: held.turf,
    };
    Some(Gasmix {
        name: name.to_owned(),
        gases: GASES
            .iter()
            .zip(&held.gas.moles)
            .filter(|(_, m)| **m != 0.0)
            .map(|(g, &m)| (g.id.to_owned(), g.name.to_owned(), m as f32))
            .collect(),
        temperature: held.gas.temperature as f32,
        volume: held.volume as f32,
        pressure: mix.pressure() as f32,
        total_moles: held.gas.total_moles() as f32,
        reactions: ss13_atmos::reactions::fire_results(&held.gas)
            .into_iter()
            .map(|(id, name, amount)| (id.to_owned(), name.to_owned(), amount as f32))
            .collect(),
        reference: format!("{}", owner.to_bits()),
    })
}

/// tg `scan_atom`: the readout of `target` (or, holding no gas, of its turf) to `client`, and
/// the analyzer remembers it.
pub(crate) fn scan_target(world: &mut World, client: Entity, item: Entity, target: Entity) {
    let target = if analyzable(world, target).is_some() {
        target
    } else {
        let Some(&pos) = world.get::<TilePos>(target) else {
            return;
        };
        let Some(turf) = turf_at(world, pos) else {
            return;
        };
        turf
    };
    let Some(held) = analyzable(world, target) else {
        return;
    };
    let name = display_name(world, target);
    let mixes: Vec<ScanMix> = held
        .iter()
        .map(|h| ScanMix {
            gas: &h.gas,
            volume: h.volume,
            turf: h.turf,
        })
        .collect();
    let message = scan_mixes(&name, &mixes);
    world.write_message(SendChat {
        clients: vec![client],
        message,
    });
    let gasmixes = gasmixes(world, target, &held);
    remember(world, item, target, gasmixes);
}

fn remember(world: &mut World, item: Entity, target: Entity, gasmixes: Vec<Gasmix>) {
    let handbook = world
        .get_resource::<Handbook>()
        .map(|h| h.0.clone())
        .unwrap_or_default();
    world.entity_mut(item).insert(AnalyzerMemory {
        handbook,
        last: Some(target),
        gasmixes,
    });
}

fn turf_at(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<Turf>(e).is_some())
}

/// tg `attack_self`: the user's own turf.
pub(crate) fn analyze_self(world: &mut World, client: Entity, item: Entity, pos: TilePos) {
    if let Some(turf) = turf_at(world, pos) {
        scan_target(world, client, item, turf);
    }
}

/// `cooldown_time` of the barometer.
const BAROMETER_COOLDOWN: f64 = 25.0;

/// The barometer's cooldown: when it is ready again (`Time` seconds).
#[derive(Component, Clone, Copy, Debug)]
pub struct BarometerCooldown(pub f64);

/// tg `click_alt` of an analyzer: the barometer. Indoors it refuses; outdoors it looks for
/// weather (`SSweather`), finds none and starts its cooldown; `ping` tells when it is ready.
pub(crate) fn barometer(world: &mut World, client: Entity, mob: Entity, item: Entity) {
    use ss13_chat::Span;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let tell = |world: &mut World, span, text: &str| crate::tools::tell(world, client, span, text);
    if world
        .get::<BarometerCooldown>(item)
        .is_some_and(|c| now < c.0)
    {
        tell(world, Span::Warning, "Функция барометра готовится.");
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/pop.ogg", 100.0, false),
    );
    let outdoors = world
        .get_resource::<ss13_mapload::Areas>()
        .and_then(|a| a.area_at(pos))
        .and_then(|area| world.get::<ss13_mapload::DmArea>(area))
        .is_some_and(|a| a.outdoors);
    if !outdoors {
        tell(
            world,
            Span::Warning,
            "Функция барометра не работает в помещении!",
        );
        return;
    }
    tell(
        world,
        Span::Warning,
        "Функции барометра не удалось отследить погодные явления.",
    );
    world
        .entity_mut(item)
        .insert(BarometerCooldown(now + BAROMETER_COOLDOWN));
}

/// `ping`: the barometer is ready again (a message to the holder, a click).
fn barometer_pings(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &BarometerCooldown)>();
    let ready: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| now >= c.0)
        .map(|(e, _)| e)
        .collect();
    for item in ready {
        world.entity_mut(item).remove::<BarometerCooldown>();
        let holder = world.get::<crate::inventory::CarriedBy>(item).map(|c| c.0);
        let client = holder
            .and_then(|m| world.get::<crate::server::PlayerMob>(m))
            .map(|p| p.client);
        if let Some(client) = client {
            crate::tools::tell(
                world,
                client,
                ss13_chat::Span::Notice,
                "Функция барометра готова!",
            );
        }
        let pos = holder
            .and_then(|m| world.get::<TilePos>(m))
            .or_else(|| world.get::<TilePos>(item))
            .copied();
        if let Some(pos) = pos {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, "sound/machines/click.ogg", 100.0, false),
            );
        }
    }
}

/// tg `attack_self_secondary` -> `ui_interact`: the window of the last scan.
pub(crate) fn open_analyzer(world: &mut World, client: Entity, mob: Entity, item: Entity) {
    if world.get::<AnalyzerMemory>(item).is_none() {
        let handbook = world
            .get_resource::<Handbook>()
            .map(|h| h.0.clone())
            .unwrap_or_default();
        world.entity_mut(item).insert(AnalyzerMemory {
            handbook,
            last: None,
            gasmixes: Vec::new(),
        });
    }
    world.write_message(OpenUi::new::<AnalyzerWindow>(
        client,
        mob,
        item,
        "Gas Analyzer",
    ));
}

/// `ranged_interact_with_atom`: a click past reach scans within the analyzer's range and sight.
fn ranged_scans(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::click::RangedClick>>,
) {
    let clicks: Vec<crate::click::RangedClick> = cursor
        .read(world.resource::<bevy::ecs::message::Messages<crate::click::RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(item) = world
            .get::<crate::inventory::Inventory>(click.mob)
            .and_then(|inv| inv.active_item())
        else {
            continue;
        };
        if !is_analyzer(world, item) {
            continue;
        }
        let range = scan_range(world, item);
        if crate::mobs::ai::behaviors::can_see(world, click.mob_pos, click.target_pos, range) {
            scan_target(world, click.client, item, click.target);
        }
    }
}

/// `ui_data`: while the holder can see the last scanned atom within range, its readings are
/// taken again.
fn refresh_analyzers(world: &mut World) {
    let mut q = world.query::<(Entity, &AnalyzerMemory, &crate::inventory::CarriedBy)>();
    let held: Vec<(Entity, Entity, Entity)> = q
        .iter(world)
        .filter_map(|(e, m, c)| Some((e, m.last?, c.0)))
        .collect();
    for (item, target, holder) in held {
        let (Some(&from), Some(&to)) = (world.get::<TilePos>(holder), world.get::<TilePos>(target))
        else {
            continue;
        };
        if !crate::mobs::ai::behaviors::can_see(world, from, to, scan_range(world, item)) {
            continue;
        }
        let Some(held) = analyzable(world, target) else {
            continue;
        };
        let mixes = gasmixes(world, target, &held);
        if let Some(mut memory) = world.get_mut::<AnalyzerMemory>(item)
            && memory.gasmixes != mixes
        {
            memory.gasmixes = mixes;
        }
    }
}

/// `GasAnalyzer`.
pub struct AnalyzerWindow;

impl UiInterface for AnalyzerWindow {
    const NAME: &'static str = GasAnalyzerUi::NAME;
    type Data = GasAnalyzerData;
    type StaticData = AtmosHandbook;
}

impl ServerUi for AnalyzerWindow {
    type Source = &'static AnalyzerMemory;
    type UserParam = ();
    type Act = ();

    fn static_data(memory: &AnalyzerMemory) -> AtmosHandbook {
        (*memory.handbook).clone()
    }

    fn data(memory: &AnalyzerMemory, _: Entity, _: &()) -> GasAnalyzerData {
        GasAnalyzerData {
            gasmixes: memory.gasmixes.clone(),
        }
    }

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// BYOND `\proper` / `\improper` name prefixes, which text output drops.
fn strip_text_macros(name: &str) -> &str {
    name.trim_start_matches("\\proper ")
        .trim_start_matches("\\improper ")
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::Messages;
    use ss13_atmos::{GasMixture, LevelAir, TileKind};
    use ss13_world::WorldPlugin;

    use super::*;

    #[test]
    fn analyzer_readout_reaches_the_user() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, WorldPlugin))
            .add_message::<SendChat>();
        app.world_mut()
            .resource_mut::<SpatialIndex>()
            .create_level(1, 2, 1)
            .unwrap();
        let mut level = LevelAir::new(2, 1);
        let air = GasMixture::parse("o2=22;n2=82;TEMP=293.15").ok();
        level.set_tile(0, TileKind::Open, air, None);
        level.set_tile(1, TileKind::Blocked, None, None);
        level.setup();
        let mut atmos = Atmos::default();
        atmos.insert_level(1, level);
        app.insert_resource(atmos);
        let world = app.world_mut();
        world.spawn((
            Turf,
            DmName {
                name: "\\improper floor".into(),
                desc: String::new(),
            },
            TilePos::new(1, 1, 1),
        ));
        let analyzer = world
            .spawn(DmItem {
                path: "/obj/item/analyzer/ranged".into(),
                ..default()
            })
            .id();
        let card = world
            .spawn(DmItem {
                path: "/obj/item/analyzer_card".into(),
                ..default()
            })
            .id();
        assert!(is_analyzer(world, analyzer) && !is_analyzer(world, card));
        let client = world.spawn_empty().id();
        analyze_self(world, client, analyzer, TilePos::new(1, 1, 1));
        analyze_self(world, client, analyzer, TilePos::new(2, 1, 1));
        assert!(
            world
                .get::<AnalyzerMemory>(analyzer)
                .is_some_and(|m| m.gasmixes.len() == 1),
            "the analyzer remembers the floor"
        );
        let sent: Vec<String> = world
            .resource_mut::<Messages<SendChat>>()
            .drain()
            .map(|m| {
                assert_eq!(m.clients, [client]);
                m.message.plain_text()
            })
            .collect();
        assert!(
            sent[0].starts_with("Results of analysis of floor.\nMoles: 104 mol"),
            "{}",
            sent[0]
        );
        assert!(sent[0].contains("Pressure: 101.34 kPa"));
        // A wall: no readout.
        assert_eq!(sent.len(), 1);
    }
}
