//! ID swipes on locks (tg `togglelock`) through the real click chain
//! (`ss13_game::click_handlers`) on the power fixture room (`super::power`), without clients:
//! the APC's interface flips for `engine_equip` (a card or a PDA with one in hand, or a worn card
//! on a human's empty-hand right click), refuses other access, an open panel and an emagged
//! interface; the emitter's controls lock only while it runs.

use bevy::prelude::*;
use ss13_game::click_handlers::{self, ClickCtx, ClickPhase};
use ss13_game::construction::PanelOpen;
use ss13_game::human::HumanLook;
use ss13_game::inventory::{IdCard, Inventory};
use ss13_game::modpc::pda::ModularComputer;
use ss13_game::power::Apc;
use ss13_game::power::emitter::Emitter;
use ss13_game::testing::{self, fixture_map};
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Dir, TilePos};
use ss13_world::SpatialIndex;

const APC: TilePos = TilePos::new(7, 5, 1);
const FLOOR: TilePos = TilePos::new(8, 3, 1);

fn server(test: &str) -> App {
    let assets = super::power::fixture_assets(&format!("id_lock_{test}"));
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    app.update();
    app
}

fn apc(app: &App) -> Entity {
    let world = app.world();
    world
        .resource::<SpatialIndex>()
        .on_tile(APC)
        .iter()
        .copied()
        .find(|&e| world.get::<Apc>(e).is_some())
        .unwrap()
}

fn card(world: &mut World, access: &[&str]) -> Entity {
    world
        .spawn(IdCard {
            access: access.iter().map(|a| (*a).to_owned()).collect(),
        })
        .id()
}

/// A mob (no client) with `inventory`.
fn mob(world: &mut World, inventory: Inventory) -> Entity {
    world.spawn((FLOOR, inventory)).id()
}

/// A world click by `mob` on `target` with its active hand's item, or empty-handed.
fn click(app: &mut App, mob: Entity, target: Entity, right: bool) -> bool {
    let world = app.world_mut();
    let inventory = world.get::<Inventory>(mob).unwrap().clone();
    let held = inventory.active_item();
    let ctx = ClickCtx {
        client: Entity::PLACEHOLDER,
        mob,
        mob_pos: FLOOR,
        held,
        target,
        pos: *world.get::<TilePos>(target).unwrap(),
        modifiers: ClickModifiers { right, ..default() },
        active_hand: inventory.active,
    };
    let phase = if held.is_some() {
        ClickPhase::HeldOnTarget
    } else {
        ClickPhase::EmptyHand
    };
    click_handlers::run(world, phase, &ctx)
}

fn locked(app: &App, apc: Entity) -> bool {
    app.world().get::<Apc>(apc).unwrap().locked
}

#[test]
fn apc_interface_unlocks_with_engineering_id() {
    let mut app = server("apc");
    let apc = apc(&app);
    assert!(locked(&app, apc));

    // Another department's card: denied, the click still the APC's.
    let world = app.world_mut();
    let wrong = card(world, &["medical"]);
    let medic = mob(
        world,
        Inventory {
            hands: [Some(wrong), None],
            ..default()
        },
    );
    assert!(click(&mut app, medic, apc, false));
    assert!(locked(&app, apc), "access denied");

    // An engineer's card in hand flips it both ways.
    let world = app.world_mut();
    let good = card(world, &["engine_equip"]);
    let engineer = mob(
        world,
        Inventory {
            hands: [Some(good), None],
            ..default()
        },
    );
    assert!(click(&mut app, engineer, apc, false));
    assert!(!locked(&app, apc), "unlocked");
    assert!(click(&mut app, engineer, apc, false));
    assert!(locked(&app, apc), "locked again");

    // A PDA holding the card works the same (`GetID`).
    let world = app.world_mut();
    let inside = card(world, &["engine_equip"]);
    let mut computer = ModularComputer::pda("/obj/item/modular_computer/pda", "PDA");
    computer.stored_id = Some(inside);
    let pda = world.spawn(computer).id();
    let pda_user = mob(
        world,
        Inventory {
            hands: [Some(pda), None],
            ..default()
        },
    );
    assert!(click(&mut app, pda_user, apc, false));
    assert!(!locked(&app, apc), "unlocked by the PDA's card");

    // A human's empty-hand right click uses the worn ID (`attack_hand_secondary`).
    let world = app.world_mut();
    let worn = card(world, &["engine_equip"]);
    let human = mob(
        world,
        Inventory {
            id: Some(worn),
            ..default()
        },
    );
    world
        .entity_mut(human)
        .insert(HumanLook::random_assistant(1));
    assert!(click(&mut app, human, apc, true));
    assert!(locked(&app, apc), "locked by the worn card");

    // An open panel, then an emagged interface, refuse even the right access.
    app.world_mut().entity_mut(apc).insert(PanelOpen(true));
    assert!(click(&mut app, engineer, apc, false));
    assert!(locked(&app, apc), "panel open");
    app.world_mut().entity_mut(apc).insert(PanelOpen(false));
    app.world_mut().get_mut::<Apc>(apc).unwrap().emagged = true;
    assert!(click(&mut app, engineer, apc, false));
    assert!(locked(&app, apc), "interface broken");
}

#[test]
fn emitter_controls_lock_only_while_on() {
    let mut app = server("emitter");
    let world = app.world_mut();
    let emitter = world
        .spawn((Emitter::new(Dir::North, true, 2), TilePos::new(8, 4, 1)))
        .id();
    let good = card(world, &["engine_equip"]);
    let engineer = mob(
        world,
        Inventory {
            hands: [Some(good), None],
            ..default()
        },
    );
    let wrong = card(world, &["medical"]);
    let medic = mob(
        world,
        Inventory {
            hands: [Some(wrong), None],
            ..default()
        },
    );
    let emitter_locked = |app: &App| app.world().get::<Emitter>(emitter).unwrap().locked;

    assert!(click(&mut app, engineer, emitter, false));
    assert!(!emitter_locked(&app), "off: cannot lock");

    app.world_mut().get_mut::<Emitter>(emitter).unwrap().active = true;
    assert!(click(&mut app, medic, emitter, false));
    assert!(!emitter_locked(&app), "access denied");
    assert!(click(&mut app, engineer, emitter, false));
    assert!(emitter_locked(&app), "locked while on");
    let (_, text) = app
        .world_mut()
        .get_mut::<Emitter>(emitter)
        .unwrap()
        .toggle(true);
    assert_eq!(text, "The controls are locked!");
    assert!(click(&mut app, engineer, emitter, false));
    assert!(!emitter_locked(&app), "unlocked");
}

/// An ethereal's right click on an APC (`ethereal_interact`): out of combat mode the stomach
/// charges the APC's cell a tenth of a standard charge per step; in combat mode it drains the
/// cell down to half and the next step starts at once.
#[test]
fn ethereals_charge_and_drain_apcs() {
    use ss13_game::combat::CombatMode;
    use ss13_game::construction::Job;
    use ss13_game::medical::organs::{Organ, OrganHost, OrganOf, Slot};
    use ss13_game::power::ethereal::{self, APC_POWER_GAIN, ETHEREAL_STOMACH, EtherealStomach};
    use ss13_game::power::{Cell, PowerJob};
    use ss13_game::tools::DoAfter;

    let mut app = server("ethereal");
    let apc = apc(&app);
    let world = app.world_mut();
    world.get_mut::<Apc>(apc).unwrap().cell = Some(Cell {
        charge: 6000.0,
        maxcharge: 10_000.0,
        chargerate: 1000.0,
    });
    let human = mob(world, Inventory::default());
    world
        .entity_mut(human)
        .insert((HumanLook::random_assistant(1), OrganHost));
    let stomach = world
        .spawn((
            Organ::new(Slot::Stomach, ETHEREAL_STOMACH),
            EtherealStomach {
                charge: 5000.0,
                drain_until: f64::NEG_INFINITY,
            },
            OrganOf(human),
        ))
        .id();

    let step = |app: &mut App| {
        let world = app.world_mut();
        let action = world
            .entity_mut(human)
            .take::<DoAfter>()
            .expect("a step started");
        let Job::Power(PowerJob::Ethereal(job)) = action.job else {
            panic!("{:?}", action.job);
        };
        ethereal::complete(world, human, &action, job);
    };
    let cell = |app: &App| app.world().get::<Apc>(apc).unwrap().cell.unwrap().charge;
    let charge = |app: &App| app.world().get::<EtherealStomach>(stomach).unwrap().charge;

    assert!(click(&mut app, human, apc, true));
    step(&mut app);
    assert_eq!(cell(&app), 6000.0 + APC_POWER_GAIN);
    assert_eq!(charge(&app), 5000.0 - APC_POWER_GAIN);
    assert!(app.world().get::<DoAfter>(human).is_none(), "one step only");

    // Combat mode drains, but never under half the APC's cell.
    let world = app.world_mut();
    world.entity_mut(human).insert(CombatMode(true));
    world
        .get_mut::<EtherealStomach>(stomach)
        .unwrap()
        .drain_until = f64::NEG_INFINITY;
    assert!(click(&mut app, human, apc, true));
    step(&mut app);
    assert_eq!(cell(&app), 6000.0);
    assert_eq!(charge(&app), 5000.0);
    step(&mut app);
    assert_eq!(cell(&app), 5000.0);
    // At half nothing more comes out; under it the safety protocol stops the loop.
    app.world_mut()
        .get_mut::<Apc>(apc)
        .unwrap()
        .cell
        .as_mut()
        .unwrap()
        .charge = 4000.0;
    step(&mut app);
    assert_eq!(cell(&app), 4000.0);
    assert!(app.world().get::<DoAfter>(human).is_none());
}
