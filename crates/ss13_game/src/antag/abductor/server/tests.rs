use ss13_assets::dmm::DmmMap;
use ss13_net::chat::server::SendChat;
use ss13_tgui::server::OpenUi;

use super::*;
use crate::antag::abductor::{AbductorConsoleUi, GlandDispenserUi};
use crate::health::Stat;
use crate::medical::organs::OrganOf;

#[test]
fn glands_have_tg_numbers() {
    assert_eq!(GLANDS.len(), 15);
    let access = gland_def("/obj/item/organ/heart/gland/access").unwrap();
    assert_eq!((access.cooldown, access.uses), ((60.0, 120.0), 1));
    assert_eq!(
        (access.mind_control_uses, access.mind_control_secs),
        (3, 90.0)
    );
    let chem = gland_def("/obj/item/organ/heart/gland/chem").unwrap();
    assert_eq!((chem.cooldown, chem.uses), ((5.0, 5.0), -1));
    let trauma = gland_def("/obj/item/organ/heart/gland/trauma").unwrap();
    assert_eq!(trauma.uses, 5);
    assert!(
        gland_def("/obj/item/organ/heart/gland/heal")
            .unwrap()
            .human_only
    );
    assert!(
        gland_def("/obj/item/organ/heart/gland/transform")
            .unwrap()
            .human_only
    );
    assert!(
        !gland_def("/obj/item/organ/heart/gland/egg")
            .unwrap()
            .human_only
    );
    for g in &GLANDS {
        assert_eq!(Gland::new(g).def().path, g.path);
        assert!(crate::obj_damage::is_under(g.path, GLAND_ROOT));
    }
}

#[test]
fn a_gland_activates_on_cooldown_until_its_uses_run_out() {
    let mut once = Gland::new(gland_def("/obj/item/organ/heart/gland/access").unwrap());
    // Not started: nothing happens.
    assert!(!once.tick(1000.0, |l, _| l));
    once.active = true;
    once.next_at = 10.0;
    assert!(!once.tick(5.0, |l, _| l));
    assert!(once.tick(10.0, |l, _| l));
    assert_eq!(once.uses, 0);
    assert!(!once.active, "a one-use gland stops");
    assert!(!once.tick(1000.0, |l, _| l));

    let mut forever = Gland::new(gland_def("/obj/item/organ/heart/gland/egg").unwrap());
    forever.active = true;
    for i in 0..5 {
        let t = forever.next_at;
        assert!(forever.tick(t, |_, h| h), "activation {i}");
        assert_eq!(forever.next_at, t + 40.0);
    }
    assert!(forever.active && forever.uses == -1);
}

#[test]
fn abductee_objectives_are_tgs() {
    let lines = vec!["Строка из файла.".to_owned()];
    let jobs = vec!["Капитан".to_owned()];
    let mut rng = Rng::new(7);
    let mut from_file = 0;
    let mut forbidden = 0;
    for _ in 0..400 {
        let o = abductee_objective(&lines, &jobs, Some("Иван"), &mut rng);
        assert!(!o.is_empty());
        if o == lines[0] {
            from_file += 1;
        }
        if o.starts_with("Игнорируйте группы предметов из ") {
            forbidden += 1;
            let n: u32 = o
                .trim_start_matches("Игнорируйте группы предметов из ")
                .split(' ')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            assert!((2..=10).contains(&n));
        }
        if o.starts_with("Захватите") {
            assert_eq!(o, "Захватите Капитан.");
        }
    }
    // 75% from the file.
    assert!((250..=350).contains(&from_file), "{from_file}");
    assert!(forbidden > 0);
    // Without lines every objective is generated; without jobs, "кого-нибудь".
    for _ in 0..100 {
        let o = abductee_objective(&[], &[], None, &mut rng);
        assert!(!o.is_empty());
        if o.starts_with("Захватите") {
            assert_eq!(o, "Захватите кого-нибудь.");
        }
    }
}

#[test]
fn baton_modes_cycle() {
    let mut mode = BatonMode::default();
    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.push(mode);
        mode = mode.next();
    }
    assert_eq!(
        seen,
        [
            BatonMode::Stun,
            BatonMode::Sleep,
            BatonMode::Cuff,
            BatonMode::Probe
        ]
    );
    assert_eq!(mode, BatonMode::Stun);
}

#[test]
fn the_shop_lists_tgs_gear_by_category() {
    assert_eq!(
        gear_categories(),
        ["Basic Gear", "Miscellaneous Gear", "Advanced Gear"]
    );
    let st = AbductorConsoleUi::static_data(&ConsoleView::default());
    let basic = &st.categories[0];
    assert_eq!(basic.items.len(), 5);
    assert_eq!(basic.items[4].name, "Advanced Baton");
    assert_eq!(basic.items[4].cost, 2);
    let total: usize = st.categories.iter().map(|c| c.items.len()).sum();
    assert_eq!(total, GEAR.len());
}

#[test]
fn spawned_types_are_runtime_types() {
    let registry = ss13_mapload::runtime::runtime_types();
    let mut paths: Vec<&str> = vec![
        UNIFORM,
        SHOES,
        HEADSET,
        HELMET,
        VEST,
        BATON,
        BELT,
        ALIEN_PISTOL,
        SILENCER,
        GIZMO,
        MIND_DEVICE,
        ENERGY_CUFFS,
        GLAND_EGG,
        SLIME,
        SPIDERLING,
    ];
    paths.extend(GLANDS.iter().map(|g| g.path));
    paths.extend(GEAR.iter().flat_map(|g| g.build.iter().map(|&(p, _)| p)));
    let missing: Vec<&str> = paths
        .into_iter()
        .filter(|p| !registry.contains(p))
        .collect();
    assert!(missing.is_empty(), "{missing:?}");
}

const SHIP: &str = r#""a" = (/obj/effect/landmark/abductor/agent{team_number = 2},/turf/open/floor,/area/space)
"b" = (/obj/machinery/abductor/console{team_number = 2},/turf/open/floor,/area/space)
"c" = (/obj/effect/landmark/abductor/scientist,/turf/open/floor,/area/space)
"d" = (/obj/machinery/abductor/pad,/turf/open/floor,/area/space)

(1,1,1) = {"
abcd
"}
"#;

#[test]
fn the_ship_scan_reads_team_numbers() {
    let map = DmmMap::parse(SHIP).unwrap();
    let (landmarks, machines) = scan_ship(&map, 5);
    assert_eq!(
        landmarks,
        [
            ShipLandmark {
                team: 2,
                agent: true,
                pos: TilePos::new(1, 1, 5)
            },
            // Landmarks default to team 1.
            ShipLandmark {
                team: 1,
                agent: false,
                pos: TilePos::new(3, 1, 5)
            },
        ]
    );
    assert_eq!(
        machines,
        [
            (CONSOLE.to_owned(), 2, TilePos::new(2, 1, 5)),
            // Machines default to team 0 (no team).
            (PAD.to_owned(), 0, TilePos::new(4, 1, 5)),
        ]
    );
}

/// A bare world with what the machines' procs read.
fn world() -> World {
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    let mut index = ss13_world::SpatialIndex::default();
    index.create_level(1, 32, 32).unwrap();
    world.insert_resource(index);
    world.init_resource::<crate::construction::BuildTemplates>();
    world.init_resource::<Messages<SendChat>>();
    world.init_resource::<Messages<OpenUi>>();
    world.init_resource::<Messages<crate::sound::PlaySound>>();
    world.init_resource::<AbductorTeams>();
    world
}

#[test]
fn the_gland_storage_holds_every_gland_and_dispenses_them() {
    let mut rng = Rng::new(3);
    let d = new_dispenser(&mut rng);
    assert_eq!(d.slots.len(), GLANDS.len());
    let mut types: Vec<&str> = d.slots.iter().map(|(t, _, _)| *t).collect();
    types.sort();
    types.dedup();
    assert_eq!(types.len(), GLANDS.len());
    for (_, color, amount) in &d.slots {
        assert!((1..=5).contains(amount));
        assert_eq!(color.len(), 7);
        assert!(color.starts_with('#'));
    }
    let data = GlandDispenserUi::data(&d, Entity::PLACEHOLDER, &());
    assert_eq!(data.glands[0].id, 1);

    let mut world = world();
    let first = d.slots[0].2;
    let storage = world.spawn((TilePos::new(1, 1, 1), d)).id();
    for _ in 0..first {
        dispense_gland(&mut world, storage, 1);
    }
    assert_eq!(world.get::<GlandDispenser>(storage).unwrap().slots[0].2, 0);
    // Empty, out of range, zero: nothing.
    assert!(dispense_gland(&mut world, storage, 1).is_none());
    assert!(dispense_gland(&mut world, storage, 99).is_none());
    assert!(dispense_gland(&mut world, storage, 0).is_none());
    assert_eq!(world.get::<GlandDispenser>(storage).unwrap().slots[0].2, 0);
}

#[test]
fn the_shop_spends_the_experiment_machines_credits() {
    let mut world = world();
    let console = world
        .spawn((
            TilePos::new(1, 1, 1),
            AbductorConsole::default(),
            TeamNumber(1),
        ))
        .id();
    let machine = world
        .spawn((
            TilePos::new(2, 1, 1),
            Experiment {
                credits: 1,
                ..default()
            },
            TeamNumber(1),
        ))
        .id();
    // Another team's machine pays nothing.
    world.spawn((
        Experiment {
            credits: 10,
            ..default()
        },
        TeamNumber(2),
    ));
    let baton = GEAR.iter().find(|g| g.name == "Advanced Baton").unwrap();
    assert!(!dispense_gear(&mut world, console, baton));
    assert_eq!(world.get::<Experiment>(machine).unwrap().credits, 1);
    let helmet = &GEAR[0];
    assert!(dispense_gear(&mut world, console, helmet));
    assert_eq!(world.get::<Experiment>(machine).unwrap().credits, 0);
    assert!(!dispense_gear(&mut world, console, helmet));
    let data = console_data(&mut world, console);
    assert!(data.experiment && !data.pad && !data.vest);
    assert_eq!((data.points, data.credits), (Some(0), Some(0)));
}

#[test]
fn an_experiment_makes_an_abductee_and_scores() {
    let mut world = world();
    world.resource_mut::<AbductorTeams>().0.push(AbductorTeam {
        number: 1,
        name: "Mothership Alpha".into(),
        members: Vec::new(),
        abductees: Vec::new(),
    });
    let machine = world
        .spawn((
            TilePos::new(2, 2, 1),
            Experiment::default(),
            TeamNumber(1),
            crate::medical::pods::Pod { open: false },
        ))
        .id();
    world.spawn((
        TilePos::new(4, 4, 1),
        AbductorPad {
            teleport_target: Some(TilePos::new(10, 10, 1)),
        },
        TeamNumber(1),
    ));
    let client = world.spawn_empty().id();
    let scientist = world
        .spawn(Abductor {
            key: "sci".into(),
            team: 1,
            role: AbductorRole::Scientist,
        })
        .id();
    let subject = world
        .spawn((
            TilePos::new(2, 2, 1),
            crate::human::HumanLook::random_assistant(1),
            PlayerMob { client },
            Stat::Conscious,
        ))
        .id();

    // No gland yet.
    assert_eq!(
        experiment(&mut world, machine, subject, 1, scientist),
        "Железы не обнаружены!"
    );
    // Only abductors may run it.
    let crewman = world.spawn_empty().id();
    assert!(experiment(&mut world, machine, subject, 1, crewman).starts_with("Ошибка"));

    let def = gland_def("/obj/item/organ/heart/gland/egg").unwrap();
    let gland = world.spawn((Gland::new(def), OrganOf(subject))).id();
    let report = experiment(&mut world, machine, subject, 2, scientist);
    assert_eq!(report, "Эксперимент успешен! Собрано новых данных: 1.");
    let e = world.get::<Experiment>(machine).unwrap();
    assert_eq!(
        (e.points, e.credits, e.history.clone()),
        (1, 1, vec![subject])
    );
    assert!(world.get::<Abductee>(subject).is_some());
    assert!(world.get::<Gland>(gland).unwrap().active);
    assert!(
        world
            .get::<crate::medical::pods::Pod>(machine)
            .unwrap()
            .open
    );
    // Sent back to the drop point.
    assert_eq!(
        world.get::<TilePos>(subject),
        Some(&TilePos::new(10, 10, 1))
    );
    assert_eq!(
        world.resource::<AbductorTeams>().get(1).unwrap().abductees,
        [subject]
    );
    // The same specimen counts once.
    assert_eq!(
        experiment(&mut world, machine, subject, 3, scientist),
        "Образец уже в базе данных."
    );
    assert_eq!(world.get::<Experiment>(machine).unwrap().points, 1);
    assert!(!experiment_done(&mut world, 1));
    world.get_mut::<Experiment>(machine).unwrap().points = EXPERIMENT_TARGET;
    assert!(experiment_done(&mut world, 1));
}

#[test]
fn removing_a_gland_stops_it() {
    let mut world = world();
    let mob = world.spawn_empty().id();
    let def = gland_def("/obj/item/organ/heart/gland/ventcrawling").unwrap();
    let gland = world.spawn((Gland::new(def), OrganOf(mob))).id();
    gland_inserted(&mut world, gland, mob);
    assert!(world.get::<Gland>(gland).unwrap().active);
    world.get_mut::<Gland>(gland).unwrap().uses = 0;
    gland_removed(&mut world, gland, mob);
    let g = world.get::<Gland>(gland).unwrap();
    // A one-use gland gets its use back.
    assert!(!g.active && g.uses == 1);
}
