//! The ordnance lab (`ss13_game::science::ordnance_machines`) on the explosions fixture
//! (`crate::explosions::server`, server only):
//! - a doppler array records an explosion in the direction it faces (one per second), with
//!   what reacted in the gas tank that blew up, and prints the record to a data disk with the
//!   experiments it qualifies for;
//! - a tank compressor absorbs its tank's explosion, keeps the gas in its buffer and records it
//!   when the tank is destroyed.

use bevy::prelude::*;
use ss13_atmos::{GasId, GasMixture};
use ss13_game::explosions::sources::{TankResults, item_destruction};
use ss13_game::explosions::{Explosion, ExplosionQueue, explosion};
use ss13_game::internals::TankAir;
use ss13_game::modpc::files::{ComputerDisk, DISK_ROOT, OrdnanceRecord};
use ss13_game::science::ordnance_machines::{
    COMPRESSOR, Compressor, DOPPLER, Doppler, DopplerAct, apply_doppler_act, compressor,
    gaseous_experiments,
};
use ss13_game::storage::StoredIn;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::{Dir, Facing, TilePos};

fn machine(app: &mut App, path: &str, pos: TilePos, dir: Dir) -> Entity {
    let e = app
        .world_mut()
        .spawn((
            pos,
            Facing(dir),
            DmObj {
                path: path.into(),
                anchored: true,
                ..default()
            },
        ))
        .id();
    app.world_mut().flush();
    e
}

fn tank(app: &mut App, pos: Option<TilePos>, gas: GasId, moles: f64) -> Entity {
    let mut air = GasMixture::vacuum();
    air.add(gas, moles);
    air.temperature = 293.15;
    let mut e = app.world_mut().spawn((
        DmItem {
            path: "/obj/item/tank/internals/plasma".into(),
            ..default()
        },
        TankAir {
            air,
            volume: 70.0,
            distribute_pressure: 16.0,
        },
    ));
    if let Some(pos) = pos {
        e.insert(pos);
    }
    e.id()
}

#[test]
fn a_doppler_array_records_explosions_it_faces() {
    let mut app = crate::explosions::server("a_doppler_array_records_explosions_it_faces");
    let east = machine(&mut app, DOPPLER, TilePos::new(3, 3, 1), Dir::East);
    let west = machine(&mut app, DOPPLER, TilePos::new(3, 4, 1), Dir::West);
    assert!(
        app.world().get::<Doppler>(east).is_some(),
        "listens from the start"
    );
    // A hydrogen tank blows up east of both.
    let bomb = tank(&mut app, Some(TilePos::new(12, 3, 1)), GasId::HYDROGEN, 1.0);
    let h2 = ss13_atmos::reactions::reaction_ids()
        .position(|id| id == "h2fire")
        .unwrap();
    app.world_mut().entity_mut(bomb).insert(TankResults {
        reactions: 1 << h2,
        overpressure: false,
    });
    explosion(
        app.world_mut(),
        TilePos::new(12, 3, 1),
        Explosion {
            source: Some(bomb),
            ..Explosion::new(1, 2, 3)
        },
    );
    let records = &app.world().get::<Doppler>(east).unwrap().records;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].factual, [1.0, 2.0, 3.0]);
    assert_eq!(records[0].theory, None, "not capped");
    assert_eq!(records[0].coordinates, "12, 3");
    assert!(records[0].results.is_some());
    assert!(
        app.world().get::<Doppler>(west).unwrap().records.is_empty(),
        "facing away"
    );
    // The cooldown: a second blast in the same second is not recorded.
    explosion(
        app.world_mut(),
        TilePos::new(12, 3, 1),
        Explosion::new(0, 0, 2),
    );
    assert_eq!(app.world().get::<Doppler>(east).unwrap().records.len(), 1);

    // Printed to a disk: the hydrogen fire qualifies for the hydrogen bomb.
    let disk = app
        .world_mut()
        .spawn((
            DmItem {
                path: DISK_ROOT.into(),
                ..default()
            },
            StoredIn(east),
        ))
        .id();
    app.world_mut().flush();
    let mut d = app.world().get::<Doppler>(east).unwrap().clone();
    d.disk = Some(disk);
    app.world_mut().entity_mut(east).insert(d);
    let number = app.world().get::<Doppler>(east).unwrap().records[0].number;
    let actor = app.world_mut().spawn_empty().id();
    apply_doppler_act(app.world_mut(), actor, east, DopplerAct::Save(number));
    let files = &app.world().get::<ComputerDisk>(disk).unwrap().data;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].filename, "Doppler Array Log Recording #1");
    assert!(matches!(files[0].record, OrdnanceRecord::Explosion(_)));
    let possible: Vec<&str> = files[0].possible.iter().map(|(p, _)| p.as_str()).collect();
    assert!(possible.contains(&"/datum/experiment/ordnance/explosive/hydrogenbomb"));
    assert!(files[0].possible.iter().all(|(_, score)| *score == 3.0));
}

#[test]
fn a_tank_compressor_absorbs_and_records_its_tank() {
    let mut app = crate::explosions::server("a_tank_compressor_absorbs_and_records_its_tank");
    let pos = TilePos::new(6, 3, 1);
    let machine = machine(&mut app, COMPRESSOR, pos, Dir::North);
    let mut c = compressor(app.world_mut(), machine).unwrap();
    // 300 moles of plasma in 70 L: far over the fragmentation pressure.
    let t = tank(&mut app, None, GasId::PLASMA, 300.0);
    app.world_mut().entity_mut(t).insert(StoredIn(machine));
    c.tank = Some(t);
    app.world_mut().entity_mut(machine).insert(c);
    let before = app.world().resource::<ExplosionQueue>().count;
    item_destruction(app.world_mut(), t);
    assert_eq!(
        app.world().resource::<ExplosionQueue>().count,
        before,
        "the explosion is absorbed"
    );
    let c = app.world().get::<Compressor>(machine).unwrap();
    assert_eq!(c.tank, None);
    assert!(c.last_pressure > 4000.0);
    assert_eq!(c.buffer.total_moles(), 0.0, "flushed");
    assert_eq!(c.records.len(), 1);
    let record = &c.records[0];
    assert_eq!(record.name, "Log Recording #1");
    let plasma = record
        .gases
        .iter()
        .find(|(g, _)| *g == GasId::PLASMA)
        .map(|(_, m)| *m)
        .unwrap();
    assert!((plasma - 300.0).abs() < 1e-6);
    assert_eq!(
        gaseous_experiments(record),
        vec![(
            "/datum/experiment/ordnance/gaseous/plasma".to_owned(),
            plasma
        )]
    );
}
