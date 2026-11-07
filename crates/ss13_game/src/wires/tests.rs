use super::*;

#[test]
fn colors_are_deterministic_per_seed_and_key() {
    let set = airlock_set("/area/station/maintenance/fore");
    assert_eq!(set.key, "/datum/wires/airlock/maint");
    assert_eq!(set.wires.len(), 17);
    assert_eq!(&set.wires[15..], ["__dud1", "__dud0"]);
    let a = colors_for(42, &set);
    assert_eq!(a, colors_for(42, &set), "same seed, same colors");
    assert_ne!(a, colors_for(43, &set), "another round, other colors");
    // Every wire has one color, every color one wire (17 wires, 17 colors).
    let mut wires: Vec<&str> = a.iter().map(|(_, w)| w.as_str()).collect();
    wires.sort_unstable();
    let mut expected: Vec<&str> = set.wires.iter().map(String::as_str).collect();
    expected.sort_unstable();
    assert_eq!(wires, expected);
    let mut colors: Vec<&str> = a.iter().map(|(c, _)| *c).collect();
    colors.sort_unstable();
    colors.dedup();
    assert_eq!(colors.len(), 17);
    // Keys are independent: the generic airlock set differs from the maintenance one.
    let generic = airlock_set("/area/station/hallway");
    assert_eq!(generic.proper_name, "Generic Airlock");
    assert_ne!(colors_for(42, &generic), a);
    // The directory hands every holder of a key the same colors.
    let mut dir = WireColors {
        seed: 42,
        ..default()
    };
    let one = dir.wires_for(WireHolder::Airlock, &set);
    let two = dir.wires_for(WireHolder::Airlock, &set);
    assert_eq!(one.colors, two.colors);
    assert_eq!(one.colors, a);
    assert_eq!(apc_set().wires.len(), 13);
    assert_eq!(vending_set().wires.len(), 6);
}

fn world_with_airlock(req_access: &[&str]) -> (World, Entity) {
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.insert_resource(SpatialIndex::default());
    world.insert_resource(CombatRng(1));
    let mut colors = WireColors {
        seed: 7,
        ..default()
    };
    let wires = colors.wires_for(WireHolder::Airlock, &airlock_set("/area/station/hallway"));
    let door = world
        .spawn((
            crate::doors::test_airlock(req_access),
            wires,
            Appearance {
                icon_state: ss13_shared::IconStateId(0),
                layer: ss13_shared::Layer(3.0),
                plane: 0,
                color: None,
                pixel_offset: [0, 0],
                rotation: 0,
                overlays: vec![],
                mouse_opacity: default(),
            },
            TilePos::new(5, 5, 1),
        ))
        .id();
    (world, door)
}

fn door(world: &World, e: Entity) -> &Door {
    world.get::<Door>(e).unwrap()
}

#[test]
fn airlock_cut_and_pulse_effects() {
    let (mut world, e) = world_with_airlock(&["maint_tunnels"]);
    // Bolts: a pulse drops them, a second raises them (powered).
    pulse(&mut world, e, WIRE_BOLTS, None);
    assert!(door(&world, e).locked);
    pulse(&mut world, e, WIRE_BOLTS, None);
    assert!(!door(&world, e).locked);

    // The open wire does nothing while the door checks IDs.
    pulse(&mut world, e, WIRE_OPEN, None);
    assert_eq!(door(&world, e).state, DoorState::Closed);
    // With the ID scan cut nothing opens it by hand, but the open wire does.
    cut(&mut world, e, WIRE_IDSCAN, None);
    assert!(
        !world
            .get::<Door>(e)
            .unwrap()
            .requires_id(world.get::<Wires>(e))
    );
    pulse(&mut world, e, WIRE_OPEN, None);
    assert!(matches!(door(&world, e).state, DoorState::Opening { .. }));

    // A cut wire cannot be pulsed; mending restores it.
    cut(&mut world, e, WIRE_BOLTS, None);
    assert!(door(&world, e).locked, "cutting the bolts drops them");
    let before = door(&world, e).locked;
    pulse(&mut world, e, WIRE_BOLTS, None);
    assert_eq!(door(&world, e).locked, before);
    cut(&mut world, e, WIRE_BOLTS, None);
    assert!(!world.get::<Wires>(e).unwrap().is_cut(WIRE_BOLTS));

    // Main power: a pulse starts a 60 s outage with 10 s of backup outage; the door
    // still has power from backup after that.
    pulse(&mut world, e, WIRE_POWER1, None);
    assert_eq!(door(&world, e).main_outage, 60.0);
    assert_eq!(door(&world, e).backup_outage, 10.0);
    assert!(!door(&world, e).has_power(true));
    assert!(world.get::<PowerOutage>(e).is_some());
    // Cutting both power wires and a backup wire: all power cut.
    cut(&mut world, e, WIRE_POWER2, None);
    cut(&mut world, e, WIRE_BACKUP1, None);
    assert!(Door::all_power_cut(world.get::<Wires>(e).unwrap()));

    // Shock: cut -> permanently electrified, mend -> not; a pulse electrifies for 30 s.
    cut(&mut world, e, WIRE_SHOCK, None);
    assert_eq!(world.get::<Electrified>(e), Some(&Electrified::Permanent));
    cut(&mut world, e, WIRE_SHOCK, None);
    assert!(world.get::<Electrified>(e).is_none());
    pulse(&mut world, e, WIRE_SHOCK, None);
    assert_eq!(world.get::<Electrified>(e), Some(&Electrified::Timed(30.0)));

    // Safety, timing, feedback, AI.
    cut(&mut world, e, WIRE_SAFETY, None);
    assert!(!door(&world, e).safe);
    pulse(&mut world, e, WIRE_TIMING, None);
    assert!(!door(&world, e).normalspeed);
    cut(&mut world, e, WIRE_FEEDBACK, None);
    assert!(!door(&world, e).feedback);
    cut(&mut world, e, WIRE_AI, None);
    assert_eq!(door(&world, e).ai_control_disabled, AI_WIRE_DISABLED);
}

#[test]
fn idscan_pulse_flashes_and_open_wire_needs_power() {
    let (mut world, e) = world_with_airlock(&[]);
    pulse(&mut world, e, WIRE_IDSCAN, None);
    assert!(door(&world, e).denied(), "the red lights flash");
    // A door without access opens on the open wire even with power out (forced).
    cut(&mut world, e, WIRE_POWER1, None);
    cut(&mut world, e, WIRE_BACKUP1, None);
    pulse(&mut world, e, WIRE_OPEN, None);
    assert!(matches!(door(&world, e).state, DoorState::Opening { .. }));
}

#[test]
fn apc_and_vending_effects() {
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.insert_resource(SpatialIndex::default());
    world.insert_resource(CombatRng(1));
    world.init_resource::<Messages<AreaPowerChanged>>();
    let mut colors = WireColors::default();
    let area = world
        .spawn(AreaPower {
            requires_power: true,
            power: [true; 3],
            ..default()
        })
        .id();
    let apc = world
        .spawn((
            Apc::new(area, None),
            colors.wires_for(WireHolder::Apc, &apc_set()),
            TilePos::new(1, 1, 1),
        ))
        .id();
    // Cutting a channel wire turns the channel and the area's channel off; mending: auto on.
    cut(&mut world, apc, WIRE_LIGHT, None);
    assert_eq!(
        world.get::<Apc>(apc).unwrap().channels[Channel::Light as usize],
        CHANNEL_OFF
    );
    assert!(!world.get::<AreaPower>(area).unwrap().power[Channel::Light as usize]);
    cut(&mut world, apc, WIRE_LIGHT, None);
    assert!(world.get::<AreaPower>(area).unwrap().power[Channel::Light as usize]);
    // Main power: a pulse shorts it (everything off) until the 2 min reset.
    pulse(&mut world, apc, WIRE_POWER1, None);
    assert!(world.get::<Apc>(apc).unwrap().shorted);
    assert_eq!(world.get::<AreaPower>(area).unwrap().power, [false; 3]);
    assert_eq!(world.get::<WireResets>(apc).unwrap().0.len(), 1);
    reset(&mut world, apc, WIRE_POWER1);
    assert!(!world.get::<Apc>(apc).unwrap().shorted);
    // Interface: cut unlocks while cut; AI: cut disables.
    cut(&mut world, apc, WIRE_INTERFACE, None);
    assert!(world.get::<Apc>(apc).unwrap().locked);
    cut(&mut world, apc, WIRE_INTERFACE, None);
    assert!(!world.get::<Apc>(apc).unwrap().locked);
    cut(&mut world, apc, WIRE_AI, None);
    assert!(world.get::<Apc>(apc).unwrap().aidisabled);

    let vendor = world
        .spawn((
            crate::vending::vendor(&default(), &HashMap::new()),
            colors.wires_for(WireHolder::Vending, &vending_set()),
            VendorHack::default(),
            TilePos::new(3, 1, 1),
        ))
        .id();
    pulse(&mut world, vendor, WIRE_CONTRABAND, None);
    assert!(world.get::<Vendor>(vendor).unwrap().extended_inventory);
    // Cut or mended, the contraband wire hides it again.
    cut(&mut world, vendor, WIRE_CONTRABAND, None);
    assert!(!world.get::<Vendor>(vendor).unwrap().extended_inventory);
    cut(&mut world, vendor, WIRE_THROW, None);
    assert!(world.get::<VendorHack>(vendor).unwrap().shoot_inventory);
    assert!(world.get::<ShootsInventory>(vendor).is_some());
    cut(&mut world, vendor, WIRE_IDSCAN, None);
    assert!(!world.get::<VendorHack>(vendor).unwrap().scan_id);
    pulse(&mut world, vendor, WIRE_SHOCK, None);
    assert_eq!(
        world.get::<Electrified>(vendor),
        Some(&Electrified::Timed(30.0))
    );
    cut(&mut world, vendor, WIRE_SHOCK, None);
    assert_eq!(
        world.get::<Electrified>(vendor),
        Some(&Electrified::Permanent)
    );
}

#[test]
fn act_params_need_a_wire() {
    let params = [("wire".to_owned(), String::new())];
    assert!(matches!(
        WiresWindow::parse_act("cut", &UiParams::new(&params)),
        Err(UiReject::BadParams(_))
    ));
    let params = [("wire".to_owned(), "red".to_owned())];
    assert_eq!(
        WiresWindow::parse_act("pulse", &UiParams::new(&params)),
        Ok(WireAct::Pulse("red".into()))
    );
    assert!(matches!(
        WiresWindow::parse_act("explode", &UiParams::new(&params)),
        Err(UiReject::UnknownAction)
    ));
}
