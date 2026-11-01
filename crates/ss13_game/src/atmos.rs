//! Atmos glue (server): gas overlay entities replicate, and gas analyzers print tg's readout
//! (`atmos_scan`) to their user's chat. The simulation itself is `ss13_atmos`.

use bevy::prelude::*;
use ss13_atmos::{Atmos, GasOverlay};
use ss13_mapload::{DmItem, DmName, Turf};
use ss13_net::chat::server::SendChat;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

/// `/obj/item/analyzer` and subtypes.
const ANALYZER_ROOT: &str = "/obj/item/analyzer";

pub struct AtmosGluePlugin;

impl Plugin for AtmosGluePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(network_gas_overlay);
    }
}

fn network_gas_overlay(add: On<Add<GasOverlay>>, mut commands: Commands) {
    commands.entity(add.entity).insert(Networked);
}

pub(crate) fn is_analyzer(world: &World, item: Entity) -> bool {
    world.get::<DmItem>(item).is_some_and(|i| {
        i.path
            .strip_prefix(ANALYZER_ROOT)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// tg `scan_atom` on the turf at `pos` (attack_self scans the user's own turf; clicking an atom
/// scans its turf): the readout goes to `client`'s chat. Nothing without atmos or in walls.
pub(crate) fn analyze(world: &mut World, client: Entity, pos: TilePos) {
    let name = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .find(|&&e| world.get::<Turf>(e).is_some())
        .and_then(|&e| world.get::<DmName>(e))
        .map_or_else(
            || "the floor".to_owned(),
            |n| strip_text_macros(&n.name).to_owned(),
        );
    let Some(message) = world
        .get_resource::<Atmos>()
        .and_then(|atmos| atmos.scan(pos, &name))
    else {
        return;
    };
    world.write_message(SendChat {
        clients: vec![client],
        message,
    });
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
        analyze(world, client, TilePos::new(1, 1, 1));
        analyze(world, client, TilePos::new(2, 1, 1));
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
