//! Emags (`ss13_game::emag`) on the power fixture room (`super::power`) without clients: the
//! plain emag refuses the airlock, the doorjack breaks it (0.6 s of sparks, then open, bolted,
//! unpowered, for good stuck) with one of its charges, which comes back; the doorjack refuses an
//! APC, the emag unlocks it (not with its panel open) for good; a secure closet's lock breaks
//! open and no longer locks; an air alarm offers its emag modes only once emagged.

use bevy::prelude::*;
use ss13_game::atmos_machinery::AirAlarm;
use ss13_game::atmos_machinery::alarm::{AlarmMode, select_mode};
use ss13_game::closets::Closet;
use ss13_game::construction::PanelOpen;
use ss13_game::doors::{Door, DoorState};
use ss13_game::emag::{self, DOORJACK, DoorjackCharges, EMAG};
use ss13_game::power::{Apc, Unpowered};
use ss13_game::testing::{self, fixture_map, steps};
use ss13_game::tools::Actor;
use ss13_mapload::{DmAtmosDevice, DmAtom, DmCloset, DmItem};
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

const APC: TilePos = TilePos::new(7, 5, 1);
const DOOR: TilePos = TilePos::new(5, 3, 1);
const FLOOR: TilePos = TilePos::new(8, 3, 1);

fn server(test: &str) -> App {
    let assets = super::power::fixture_assets(&format!("emag_{test}"));
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    app.update();
    app
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn find<T: Component>(app: &mut App, pos: TilePos) -> Entity {
    let world = app.world();
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<T>(e).is_some())
        .unwrap()
}

/// A user (no client) holding a card of `path`.
fn user_with(app: &mut App, path: &str) -> Actor {
    let world = app.world_mut();
    let mob = world.spawn(FLOOR).id();
    let card = world
        .spawn(DmItem {
            path: path.into(),
            ..default()
        })
        .id();
    Actor {
        client: Entity::PLACEHOLDER,
        mob,
        item: Some(card),
    }
}

fn swipe(app: &mut App, actor: Actor, target: Entity) -> bool {
    emag::interact_with_atom(app.world_mut(), actor, target)
}

fn door(app: &App, e: Entity) -> &Door {
    app.world().get::<Door>(e).unwrap()
}

#[test]
fn airlock_needs_the_doorjack_and_stays_broken() {
    let mut app = server("airlock");
    run(&mut app, 3.0);
    let airlock = find::<Door>(&mut app, DOOR);
    assert!(app.world().get::<Unpowered>(airlock).is_none(), "powered");
    let emag_user = user_with(&mut app, EMAG);
    let jack_user = user_with(&mut app, DOORJACK);
    let jack = jack_user.item.unwrap();

    // The plain emag is the airlock's, but refused (`type_blacklist`).
    assert!(
        swipe(&mut app, emag_user, airlock),
        "the click is the emag's"
    );
    run(&mut app, 1.0);
    assert!(!door(&app, airlock).emagged);
    assert_eq!(door(&app, airlock).state, DoorState::Closed);

    // The doorjack: sparks first, a charge used.
    assert!(swipe(&mut app, jack_user, airlock));
    assert!(door(&app, airlock).emagging);
    assert!(door(&app, airlock).is_operating());
    let used = app.world().get::<DoorjackCharges>(jack).unwrap();
    assert_eq!(used.charges(), 2);

    // 0.6 s later: open, bolted, lights off, no power; it never closes again.
    run(&mut app, 3.0);
    let d = door(&app, airlock);
    assert!(d.emagged && !d.emagging);
    assert!(d.locked, "bolted");
    assert!(!d.feedback, "lights off");
    assert!(
        d.main_outage > 0.0 && d.backup_outage > 0.0,
        "both powers lost"
    );
    assert_eq!(d.state, DoorState::Open, "the autoclose cannot close it");

    // A second swipe does nothing and costs no charge.
    assert!(swipe(&mut app, jack_user, airlock));
    let used = app.world().get::<DoorjackCharges>(jack).unwrap();
    assert_eq!(used.charges(), 2);

    // The charge comes back at its time.
    let now = app.world().resource::<Time>().elapsed_secs_f64();
    app.world_mut()
        .get_mut::<DoorjackCharges>(jack)
        .unwrap()
        .recharge_at[0] = now + 0.5;
    run(&mut app, 1.0);
    assert!(
        app.world().get::<DoorjackCharges>(jack).is_none(),
        "all 3 back"
    );
}

#[test]
fn apc_unlocks_for_good() {
    let mut app = server("apc");
    let apc = find::<Apc>(&mut app, APC);
    assert!(app.world().get::<Apc>(apc).unwrap().locked);

    // The doorjack only fits airlock electronics.
    let jack_user = user_with(&mut app, DOORJACK);
    assert!(swipe(&mut app, jack_user, apc));
    assert!(!app.world().get::<Apc>(apc).unwrap().emagged);
    assert!(
        app.world()
            .get::<DoorjackCharges>(jack_user.item.unwrap())
            .is_none(),
        "no charge used"
    );

    // Not with the panel open.
    let user = user_with(&mut app, EMAG);
    app.world_mut().entity_mut(apc).insert(PanelOpen(true));
    assert!(swipe(&mut app, user, apc));
    assert!(!app.world().get::<Apc>(apc).unwrap().emagged);

    app.world_mut().entity_mut(apc).insert(PanelOpen(false));
    assert!(swipe(&mut app, user, apc));
    let a = app.world().get::<Apc>(apc).unwrap();
    assert!(a.emagged && !a.locked);
    assert!(!emag::emag_act(
        app.world_mut(),
        user,
        user.item.unwrap(),
        apc
    ));
}

#[test]
fn secure_closet_lock_breaks_open() {
    let mut app = server("closet");
    let dm = DmCloset {
        secure: true,
        locked: true,
        ..default()
    };
    let state = Closet::from_dm(&dm, &DmAtom::default(), None);
    let closet = app.world_mut().spawn((state, FLOOR)).id();
    let plain = app
        .world_mut()
        .spawn((
            Closet::from_dm(&DmCloset::default(), &DmAtom::default(), None),
            FLOOR,
        ))
        .id();
    let user = user_with(&mut app, EMAG);

    assert!(swipe(&mut app, user, closet));
    let c = app.world().get::<Closet>(closet).unwrap();
    assert!(c.broken && !c.locked);
    assert!(
        !c.may_toggle_lock(Some(&["captain".to_owned()][..])),
        "a broken lock does not lock"
    );
    // Once only; a closet without a lock is not affected.
    assert!(!emag::emag_act(
        app.world_mut(),
        user,
        user.item.unwrap(),
        closet
    ));
    assert!(!emag::emag_act(
        app.world_mut(),
        user,
        user.item.unwrap(),
        plain
    ));
    assert!(!app.world().get::<Closet>(plain).unwrap().broken);
}

#[test]
fn air_alarm_emag_modes() {
    let mut app = server("alarm");
    let alarm = app
        .world_mut()
        .spawn((AirAlarm::new(None, &DmAtmosDevice::default()), FLOOR))
        .id();
    let mode = |app: &App| app.world().get::<AirAlarm>(alarm).unwrap().mode;

    select_mode(app.world_mut(), alarm, AlarmMode::Flood);
    assert_eq!(mode(&app), AlarmMode::Filtering, "flood needs an emag");

    let user = user_with(&mut app, EMAG);
    assert!(swipe(&mut app, user, alarm));
    assert!(app.world().get::<AirAlarm>(alarm).unwrap().emagged);
    select_mode(app.world_mut(), alarm, AlarmMode::Flood);
    assert_eq!(mode(&app), AlarmMode::Flood);
}

/// Not a family with an `emag_act`: the click goes on (a table takes the card, a bag stores it).
#[test]
fn other_targets_are_left_alone() {
    let mut app = server("other");
    let user = user_with(&mut app, EMAG);
    let thing = app.world_mut().spawn(FLOOR).id();
    assert!(!swipe(&mut app, user, thing));
}
