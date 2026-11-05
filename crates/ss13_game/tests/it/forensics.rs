//! Forensics on a bare world: bare hands leave prints and gloves take them (fingerless ones let
//! them through), players leave hidden prints, washing wipes blood and scrubbing prints and
//! fibers, a bloody hit spreads the victim's DNA, the forensic scanner logs what it finds after
//! its analysis and prints the log, an evidence bag takes one item and empties.

use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use ss13_game::combat::CombatRng;
use ss13_game::forensics::server::{
    DetectiveScanner, EVIDENCE_BAG, EvidenceBag, Forensics, SCAN_SECS, SCANNER, add_blood_dna,
    add_fingerprint, attack_self, blood_stained, fingerprint_of, item_attack, item_interaction,
    print_report, scan, scanner_timers,
};
use ss13_game::forensics::{ScanCategory, ScanLog};
use ss13_game::genetics::Dna;
use ss13_game::health::Health;
use ss13_game::human::HumanLook;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::janitor::{CLEAN_SCRUB, CLEAN_WASH, wash};
use ss13_game::medical::blood::{Blood, BloodType};
use ss13_game::server::PlayerMob;
use ss13_game::sound::PlaySound;
use ss13_game::storage::{Storage, StoredIn};
use ss13_mapload::{DmClothing, DmItem, DmName, DmWeight};
use ss13_net::chat::server::SendChat;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

/// `HANDS` body part bits.
const HANDS: u32 = (1 << 9) | (1 << 10);
const BLACK_GLOVES: &str = "/obj/item/clothing/gloves/color/black";
const FINGERLESS: &str = "/obj/item/clothing/gloves/fingerless";

fn world() -> World {
    let mut world = World::new();
    world.init_resource::<Time>();
    world.insert_resource(CombatRng(7));
    world.init_resource::<SpatialIndex>();
    world.init_resource::<Messages<SendChat>>();
    world.init_resource::<Messages<PlaySound>>();
    world
}

fn human(world: &mut World, identity: &str, enzymes: &str) -> Entity {
    world
        .spawn((
            Name::new("Bob"),
            Health::default(),
            HumanLook::random_assistant(1),
            Dna {
                unique_enzymes: enzymes.into(),
                unique_identity: identity.into(),
                mutation_index: Vec::new(),
                default_mutation_genes: Vec::new(),
                mutations: Vec::new(),
                stability: 100,
                melt_until: None,
            },
            Blood::new(BloodType::APlus),
            Inventory::default(),
        ))
        .id()
}

fn item(world: &mut World, path: &str, name: &str, w_class: u8) -> Entity {
    world
        .spawn((
            DmItem {
                path: path.into(),
                interaction_flags_item: 1,
                ..default()
            },
            DmName {
                name: name.into(),
                desc: String::new(),
            },
            DmWeight { w_class },
        ))
        .id()
}

fn wear_gloves(world: &mut World, mob: Entity, path: &str) -> Entity {
    let gloves = item(world, path, "gloves", 2);
    world.entity_mut(gloves).insert(DmClothing {
        body_parts_covered: HANDS,
        ..default()
    });
    world.get_mut::<Inventory>(mob).unwrap().gloves = Some(gloves);
    gloves
}

fn prints(world: &World, e: Entity) -> Vec<String> {
    world
        .get::<Forensics>(e)
        .map(|f| f.fingerprints.clone())
        .unwrap_or_default()
}

#[test]
fn prints_are_unique_hex_per_identity() {
    let a = fingerprint_of("0A1B2C");
    assert_eq!(a.len(), 32);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(a, fingerprint_of("0A1B2C"));
    assert_ne!(a, fingerprint_of("0A1B2D"));
}

#[test]
fn gloves_take_the_prints_unless_fingerless() {
    let mut world = world();
    let cup = item(&mut world, "/obj/item/reagent_containers/cup", "cup", 2);

    let bare = human(&mut world, "ident-a", "enz-a");
    add_fingerprint(&mut world, cup, bare, false);
    assert_eq!(prints(&world, cup), vec![fingerprint_of("ident-a")]);

    let gloved = human(&mut world, "ident-b", "enz-b");
    let gloves = wear_gloves(&mut world, gloved, BLACK_GLOVES);
    add_fingerprint(&mut world, cup, gloved, false);
    assert_eq!(prints(&world, cup), vec![fingerprint_of("ident-a")]);
    assert_eq!(prints(&world, gloves), vec![fingerprint_of("ident-b")]);

    let fingerless = human(&mut world, "ident-c", "enz-c");
    wear_gloves(&mut world, fingerless, FINGERLESS);
    add_fingerprint(&mut world, cup, fingerless, false);
    assert!(prints(&world, cup).contains(&fingerprint_of("ident-c")));

    // Only living mobs touch things.
    let ghost = world.spawn_empty().id();
    add_fingerprint(&mut world, cup, ghost, false);
    assert_eq!(prints(&world, cup).len(), 2);
}

#[test]
fn players_leave_hidden_prints_first_and_last() {
    let mut world = world();
    let cup = item(&mut world, "/obj/item/reagent_containers/cup", "cup", 2);
    let client = world.spawn(ClientToken("Some.Player".into())).id();
    let mob = human(&mut world, "ident-a", "enz-a");
    world.entity_mut(mob).insert(PlayerMob { client });

    add_fingerprint(&mut world, cup, mob, false);
    let f = world.get::<Forensics>(cup).unwrap().clone();
    assert_eq!(f.hiddenprints.len(), 1);
    assert_eq!(f.hiddenprints[0].0, "someplayer");
    assert!(f.hiddenprints[0].1.starts_with("First: ["));
    assert!(f.hiddenprints[0].1.contains("\"Bob\". Ckey: someplayer"));
    assert_eq!(f.last_ckey.as_deref(), Some("someplayer"));

    add_fingerprint(&mut world, cup, mob, false);
    add_fingerprint(&mut world, cup, mob, false);
    let text = &world.get::<Forensics>(cup).unwrap().hiddenprints[0].1;
    assert_eq!(text.matches("\nLast: ").count(), 1);

    // Scrubbing leaves the admins' record.
    wash(&mut world, cup, CLEAN_SCRUB);
    assert_eq!(world.get::<Forensics>(cup).unwrap().hiddenprints.len(), 1);
}

#[test]
fn washing_wipes_blood_and_scrubbing_wipes_prints_and_fibers() {
    let mut world = world();
    let cup = item(&mut world, "/obj/item/reagent_containers/cup", "cup", 2);
    let mob = human(&mut world, "ident-a", "enz-a");
    add_fingerprint(&mut world, cup, mob, false);
    assert!(add_blood_dna(
        &mut world,
        cup,
        &[("enz-v".into(), BloodType::OMinus)]
    ));
    world
        .get_mut::<Forensics>(cup)
        .unwrap()
        .fibers
        .push("Fibers from a jumpsuit.".into());
    assert!(blood_stained(&world, cup));

    assert!(wash(&mut world, cup, CLEAN_WASH));
    let f = world.get::<Forensics>(cup).unwrap();
    assert!(f.blood_dna.is_empty());
    assert_eq!(f.fingerprints.len(), 1);
    assert_eq!(f.fibers.len(), 1);
    assert!(!blood_stained(&world, cup));

    assert!(wash(&mut world, cup, CLEAN_SCRUB));
    let f = world.get::<Forensics>(cup).unwrap();
    assert!(f.fingerprints.is_empty() && f.fibers.is_empty());
    assert!(!wash(&mut world, cup, CLEAN_SCRUB));
}

#[test]
fn a_bloody_hit_marks_the_weapon_and_the_gloves() {
    let mut world = world();
    let attacker = human(&mut world, "ident-a", "enz-a");
    let gloves = wear_gloves(&mut world, attacker, BLACK_GLOVES);
    let victim = human(&mut world, "ident-v", "enz-v");
    world.entity_mut(attacker).insert(TilePos::new(1, 1, 1));
    world.entity_mut(victim).insert(TilePos::new(1, 2, 1));
    let knife = item(&mut world, "/obj/item/knife", "knife", 2);

    // 25 + 2 × 40 percent: always.
    item_attack(&mut world, attacker, victim, knife, 40.0, true);
    let blood = vec![("enz-v".to_owned(), BloodType::APlus)];
    assert_eq!(world.get::<Forensics>(knife).unwrap().blood_dna, blood);
    assert_eq!(world.get::<Forensics>(gloves).unwrap().blood_dna, blood);
    // Gloved: the knife keeps no print, the gloves do.
    assert!(prints(&world, knife).is_empty());
    assert_eq!(prints(&world, gloves), vec![fingerprint_of("ident-a")]);

    // Burns spread no blood.
    let torch = item(&mut world, "/obj/item/weldingtool", "welder", 2);
    item_attack(&mut world, attacker, victim, torch, 40.0, false);
    assert!(
        world
            .get::<Forensics>(torch)
            .is_none_or(|f| f.blood_dna.is_empty())
    );
}

fn advance(world: &mut World, secs: f64) {
    world
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f64(secs));
    world.run_system_once(scanner_timers).unwrap();
}

fn logs(world: &World, scanner: Entity) -> Vec<ScanLog> {
    world.get::<DetectiveScanner>(scanner).unwrap().logs.clone()
}

#[test]
fn the_scanner_logs_traces_after_analysis_and_prints_them() {
    let mut world = world();
    let client = world.spawn_empty().id();
    let detective = human(&mut world, "ident-d", "enz-d");
    let scanner = item(&mut world, SCANNER, "forensic scanner", 2);
    world
        .entity_mut(scanner)
        .insert((DetectiveScanner::default(), CarriedBy(detective)));
    world.get_mut::<Inventory>(detective).unwrap().hands[0] = Some(scanner);

    let suspect = human(&mut world, "ident-s", "enz-s");
    let knife = item(&mut world, "/obj/item/knife", "knife", 2);
    add_fingerprint(&mut world, knife, suspect, false);
    add_blood_dna(&mut world, knife, &[("enz-v".into(), BloodType::BMinus)]);

    scan(&mut world, client, detective, scanner, knife);
    assert!(world.get::<DetectiveScanner>(scanner).unwrap().busy());
    // Busy: a second scan is refused.
    scan(&mut world, client, detective, scanner, suspect);
    advance(&mut world, SCAN_SECS / 2.0);
    assert!(logs(&world, scanner).is_empty());
    advance(&mut world, SCAN_SECS);
    let found = logs(&world, scanner);
    assert_eq!(found.len(), 1);
    let log = &found[0];
    assert_eq!(log.scan_target, "knife");
    let categories: Vec<ScanCategory> = log.data_entries.iter().map(|e| e.category).collect();
    assert_eq!(categories, vec![ScanCategory::Fingers, ScanCategory::Blood]);
    let print = fingerprint_of("ident-s");
    assert_eq!(
        log.data_entries[0].data,
        vec![(print.clone(), print.clone())]
    );
    assert_eq!(
        log.data_entries[1].data,
        vec![("enz-v".to_owned(), "B-".to_owned())]
    );
    let report = log.report_text();
    assert!(report.starts_with("Knife scan at "));
    assert!(report.contains("Fingerprints") && report.contains(&print));
    assert!(report.contains("enz-v, B-"));

    // A human without gloves shows its own print; with gloves, none.
    scan(&mut world, client, detective, scanner, suspect);
    advance(&mut world, SCAN_SECS + 0.1);
    let own = &logs(&world, scanner)[1];
    assert_eq!(own.data_entries[0].category, ScanCategory::Fingers);
    assert_eq!(own.data_entries[0].data[0].0, print);
    wear_gloves(&mut world, suspect, BLACK_GLOVES);
    scan(&mut world, client, detective, scanner, suspect);
    advance(&mut world, SCAN_SECS + 0.1);
    assert!(logs(&world, scanner)[2].data_entries.is_empty());

    // Not held: no scan.
    world.get_mut::<Inventory>(detective).unwrap().hands[0] = None;
    scan(&mut world, client, detective, scanner, knife);
    assert!(!world.get::<DetectiveScanner>(scanner).unwrap().busy());

    print_report(&mut world, scanner);
    let state = world.get::<DetectiveScanner>(scanner).unwrap();
    assert!(state.logs.is_empty());
    assert_eq!(state.print_count, 1);
}

#[test]
fn scan_entries_merge_and_sort_by_category() {
    let mut log = ScanLog::default();
    log.add(ScanCategory::Fiber, [("a".into(), "a".into())]);
    log.add(ScanCategory::Fingers, [("p".into(), "p".into())]);
    log.add(
        ScanCategory::Fiber,
        [("b".into(), "b".into()), ("a".into(), "a".into())],
    );
    assert_eq!(log.data_entries.len(), 2);
    assert_eq!(log.data_entries[0].category, ScanCategory::Fingers);
    assert_eq!(log.data_entries[1].data.len(), 2);
    assert!(
        ScanLog::default()
            .report_text()
            .contains("No forensic traces found.")
    );
}

#[test]
fn an_evidence_bag_holds_one_item_and_empties() {
    let mut world = world();
    let client = world.spawn_empty().id();
    let mob = human(&mut world, "ident-a", "enz-a");
    let at = TilePos::new(1, 1, 1);
    world.entity_mut(mob).insert(at);
    let bag = item(&mut world, EVIDENCE_BAG, "evidence bag", 1);
    world.entity_mut(bag).insert((
        EvidenceBag::default(),
        Storage {
            max_slots: 1,
            max_specific_storage: 3,
            max_total_storage: 14,
        },
    ));
    let floor = TilePos::new(1, 2, 1);
    let knife = item(&mut world, "/obj/item/knife", "knife", 2);
    let pen = item(&mut world, "/obj/item/pen", "pen", 1);
    let toolbox = item(&mut world, "/obj/item/storage/toolbox", "toolbox", 4);
    for thing in [knife, pen, toolbox] {
        world.entity_mut(thing).insert(floor);
    }

    // Too big for it.
    assert!(item_interaction(&mut world, client, mob, bag, toolbox));
    assert!(world.get::<StoredIn>(toolbox).is_none());
    assert!(item_interaction(&mut world, client, mob, bag, knife));
    assert_eq!(world.get::<StoredIn>(knife).map(|s| s.0), Some(bag));
    assert!(world.get::<TilePos>(knife).is_none());
    // One slot.
    assert!(item_interaction(&mut world, client, mob, bag, pen));
    assert!(world.get::<StoredIn>(pen).is_none());

    assert!(attack_self(&mut world, client, mob, bag));
    assert!(world.get::<StoredIn>(knife).is_none());
    assert_eq!(world.get::<TilePos>(knife), Some(&at));
}
