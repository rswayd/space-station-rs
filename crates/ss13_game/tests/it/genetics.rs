//! Genetics over real WebTransport on 127.0.0.1 (fixture: a DNA console next to a DNA scanner):
//! the subject opens the scanner and steps in, the geneticist closes it from the `DnaConsole`
//! window, completes the Glowy block base by base with `pulse_gene` (the mutation activates,
//! the subject glows, the station discovers it), prints a mutator, picks it up and injects
//! himself (he glows too), and mutadone in his blood takes the mutation away. The mutation
//! effects: the Hulk, a power button, speech, the monkey and the mood aura.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::chem::{Chem, MobReagents};
use ss13_game::closets::InCloset;
use ss13_game::genetics::console::{DnaInjector, DnaScanner};
use ss13_game::genetics::{DiscoveredMutations, Dna, SharedDb, mutation_id};
use ss13_game::inventory::Inventory;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, Optics, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::dna_console::{DnaConsoleData, DnaConsoleUi};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1,
        "inverse_chem": null, "inverse_chem_val": 0.25, "purge_multiplier": 1 } },
    "/datum/reagent/medicine": { "parent": "/datum/reagent", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent/medicine" } } },
    "/datum/reagent/medicine/mutadone": { "parent": "/datum/reagent/medicine",
        "declared_vars": { "name": "Mutadone", "color": "#5096C8", "ph": 2,
        "chemical_flags": 16 } },
    "/datum/chemical_reaction": { "parent": "/datum", "declared_vars": {
        "H_ion_release": 0.01, "determin_ph_range": 4, "is_cold_recipe": 0,
        "mix_message": "The solution begins to bubble.", "mob_react": 1, "optimal_ph_max": 9,
        "optimal_ph_min": 5, "optimal_temp": 500, "overheat_temp": 900, "ph_exponent_factor": 2,
        "purity_min": 0.15, "rate_up_lim": 30, "reaction_flags": 0,
        "required_catalysts": { "list": [] }, "required_container": null,
        "required_container_accepts_subtypes": 0, "required_reagents": { "list": [] },
        "required_temp": 100, "results": { "list": [] }, "temp_exponent_factor": 2,
        "thermic_constant": 0 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/projectile": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i.dmi" },
        "icon_state": "dnainjector" } },
    "/obj/projectile/temp": { "parent": "/obj/projectile" },
    "/obj/projectile/temp/cryo": { "parent": "/obj/projectile/temp" },
    "/obj/effect/decal": { "parent": "/obj/effect" },
    "/obj/effect/decal/cleanable": { "parent": "/obj/effect/decal" },
    "/obj/effect/decal/cleanable/ash": { "parent": "/obj/effect/decal/cleanable",
        "declared_vars": { "name": "ash", "icon": { "resource": "i.dmi" },
        "icon_state": "dnainjector" } },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "dnainjector", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 1,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/dnainjector": { "parent": "/obj/item", "declared_vars": {
        "name": "DNA injector" } },
    "/obj/item/dnainjector/activator": { "parent": "/obj/item/dnainjector", "declared_vars": {
        "name": "DNA activator" } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "m.dmi" }, "icon_state": "computer", "density": 1 } },
    "/obj/machinery/computer/dna_console": { "parent": "/obj/machinery/computer",
        "declared_vars": { "name": "DNA Console" } },
    "/obj/machinery/dna_scannernew": { "parent": "/obj/machinery", "declared_vars": {
        "name": "DNA scanner", "icon": { "resource": "m.dmi" }, "icon_state": "scanner",
        "density": 1 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// The geneticist spawns west of the console `C` (3,3), the subject east of the scanner `S`
/// (4,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"C" = (/obj/machinery/computer/dna_console,/turf/open/floor,/area/space)
"S" = (/obj/machinery/dna_scannernew,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwww
wffffw
wsCSsw
wffffw
wwwwww
"}
"##;

const CONSOLE: TilePos = TilePos::new(3, 3, 1);
const SCANNER: TilePos = TilePos::new(4, 3, 1);
const WEST_SPAWN: TilePos = TilePos::new(2, 3, 1);

const STATES: [(&str, &str); 10] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "dnainjector"),
    ("i.dmi", "dnainjector0"),
    ("m.dmi", "computer"),
    ("m.dmi", "scanner"),
    ("m.dmi", "scanner_open"),
    ("i.dmi", "card_grey"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("genetics_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/computer/dna_console",
        "/obj/machinery/dna_scannernew",
        "/obj/item/dnainjector/activator",
        "/obj/effect/decal/cleanable/ash",
        "/obj/projectile/temp/cryo",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let dir = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    dir
}

/// The client's last `DnaConsole` window.
#[derive(Resource, Default)]
struct DnaWindow {
    id: Option<u32>,
    data: Option<DnaConsoleData>,
}

fn record_window(mut received: MessageReader<UiMessageReceived>, mut w: ResMut<DnaWindow>) {
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == DnaConsoleUi::NAME => {
                w.id = Some(*window);
                w.data = Some(ss13_tgui::decode(data).unwrap());
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if w.id == Some(*window) => w.data = Some(ss13_tgui::decode(data).unwrap()),
            _ => {}
        }
    }
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(2, |app| {
            app.init_resource::<DnaWindow>()
                .add_systems(Update, record_window);
        });
        Self(s)
    }

    fn at(&self, pos: TilePos, has: impl Fn(&World, Entity) -> bool) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| has(world, e))
            .unwrap()
    }

    fn window(&self, p: usize) -> &DnaWindow {
        self.players[p].app.world().resource::<DnaWindow>()
    }

    fn dna(&self, mob: Entity) -> Dna {
        self.get::<Dna>(mob).clone()
    }
}

#[test]
fn sequence_inject_and_cure() {
    let mut s = Station::start("sequence_inject_and_cure");
    // p `g` is the geneticist (west of the console), `t` the subject.
    let g = if *s.get::<TilePos>(s.players[0].mob) == WEST_SPAWN {
        0
    } else {
        1
    };
    let t = 1 - g;
    let (gmob, tmob) = (s.players[g].mob, s.players[t].mob);
    assert!(s.try_get::<Dna>(tmob).is_some(), "every human has DNA");
    assert_eq!(s.dna(tmob).mutation_index.len(), 8);
    // The index holds 8 of the unlocked mutations: make Glowy one of them, two bases missing.
    {
        let glow = mutation_id("/datum/mutation/glow").unwrap();
        let db = s.0.server.world().resource::<SharedDb>().0.clone();
        let mut seq = db.full_sequences[glow].clone();
        seq.replace_range(0..2, "XX");
        let world = s.0.server.world_mut();
        let mut dna = world.get_mut::<Dna>(tmob).unwrap();
        if !dna.in_sequence(glow) {
            dna.mutation_index[0] = (glow, seq.clone());
            dna.default_mutation_genes[0] = seq;
        }
    }
    // A crew member's record DNA is its unique enzymes.
    let records = s
        .server
        .world()
        .resource::<ss13_game::records::manifest::Manifest>();
    if let Some(r) = records.locked.iter().find(|r| r.mob == tmob) {
        assert_eq!(r.dna_string, s.dna(tmob).unique_enzymes);
    }

    let scanner = s.at(SCANNER, |w, e| w.get::<DnaScanner>(e).is_some());
    let console = s.at(CONSOLE, |w, e| {
        w.get::<ss13_game::genetics::console::DnaConsole>(e)
            .is_some()
    });
    // The subject opens the scanner and steps in.
    s.click(t, scanner);
    s.run_until("scanner open", |s| s.get::<DnaScanner>(scanner).open);
    s.run_until("subject on the scanner", |s| {
        if *s.get::<TilePos>(tmob) == SCANNER {
            return true;
        }
        s.try_step(t, Dir::West);
        false
    });
    // The geneticist opens the console and closes the scanner from it.
    s.click(g, console);
    s.run_until("DnaConsole open", |s| s.window(g).data.is_some());
    let window = s.window(g).id.unwrap();
    s.act(g, window, "toggle_door", &[]);
    s.run_until("subject shut in", |s| {
        s.try_get::<InCloset>(tmob)
            .is_some_and(|c| c.closet == scanner)
    });
    s.run_until("window shows the subject", |s| {
        s.window(g)
            .data
            .as_ref()
            .is_some_and(|d| d.is_viable_subject && d.mutations.len() == 8)
    });

    // Complete the Glowy block one base at a time.
    let glow = mutation_id("/datum/mutation/glow").unwrap();
    let db = s.server.world().resource::<SharedDb>().0.clone();
    let alias = db.aliases[glow].clone();
    let full = db.full_sequences[glow].clone().into_bytes();
    let mut pulses = 0;
    loop {
        let seq = s.dna(tmob).sequence(glow).unwrap().as_bytes().to_vec();
        let Some(pos) = (0..seq.len()).find(|&i| seq[i] != full[i]) else {
            break;
        };
        let before = seq[pos];
        let pos_text = (pos + 1).to_string();
        s.act(
            g,
            window,
            "pulse_gene",
            &[("alias", &alias), ("pos", &pos_text), ("pulseAction", "1")],
        );
        pulses += 1;
        s.run_until("the base changes", |s| {
            s.dna(tmob).sequence(glow).unwrap().as_bytes()[pos] != before
        });
        assert!(pulses < 200);
    }
    s.run_until("Glowy active", |s| s.dna(tmob).get_mutation(glow).is_some());
    assert!(s.get::<Optics>(tmob).light.is_some(), "the subject glows");
    assert!(
        s.server
            .world()
            .resource::<DiscoveredMutations>()
            .0
            .contains(&glow)
    );
    assert_eq!(s.dna(tmob).stability, 100, "an activated positive is free");

    // A mutator of it, picked up and injected by the geneticist into himself.
    s.run_until("window shows it active", |s| {
        s.window(g).data.as_ref().is_some_and(|d| {
            d.mutations
                .iter()
                .any(|m| m.alias == alias && m.active && m.discovered)
        })
    });
    s.act(
        g,
        window,
        "print_injector",
        &[("alias", &alias), ("is_activator", "0")],
    );
    s.run_until("injector printed", |s| {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(CONSOLE)
            .iter()
            .any(|&e| {
                world
                    .get::<DnaInjector>(e)
                    .is_some_and(|i| i.add == vec![glow])
            })
    });
    let injector = s.at(CONSOLE, |w, e| w.get::<DnaInjector>(e).is_some());
    let ready = s
        .get::<ss13_game::genetics::console::DnaConsole>(console)
        .injector_ready;
    assert!(ready - s.now() > 9.0, "mutator cooldown max(10 s, 5 s)");
    s.click(g, injector);
    s.run_until("injector in hand", |s| {
        s.get::<Inventory>(gmob).hands.contains(&Some(injector))
    });
    s.click(g, gmob);
    s.run_until("geneticist glows", |s| {
        s.dna(gmob).get_mutation(glow).is_some()
    });
    assert!(s.get::<DnaInjector>(injector).used);
    assert!(s.get::<Optics>(gmob).light.is_some());

    // Mutadone cures it.
    {
        let world = s.0.server.world_mut();
        let chem = world.resource::<Chem>().0.clone();
        let id = chem.id(ss13_game::chem::MUTADONE).unwrap();
        world
            .get_mut::<MobReagents>(gmob)
            .unwrap()
            .blood
            .add_reagent(&chem, id, 5.0, 310.0, None, None, true, false);
    }
    s.run_until_for(10.0, "mutadone removes Glowy", |s| {
        s.dna(gmob).mutations.is_empty()
    });
    assert!(s.try_get::<Optics>(gmob).is_none(), "the glow is gone");
}

#[test]
fn radiation_storm_mutates_humans() {
    let mut s = Station::start("radiation_storm_mutates_humans");
    let mob = s.players[0].mob;
    let before = s.dna(mob);
    ss13_game::events::radiation_storm::run_weather(s.0.server.world_mut());
    // 40 s of telegraph, then a 40% roll each second in the storm.
    s.run_until_for(120.0, "the storm scrambles or mutates", |s| {
        let now = s.dna(mob);
        now.unique_identity != before.unique_identity || !now.mutations.is_empty()
    });
    let now = s.dna(mob);
    // Random mutations come activated: their block is complete.
    for m in &now.mutations {
        assert!(now.in_sequence(m.id));
    }
}

#[test]
fn mutation_effects() {
    use ss13_chat::accents::{Accent, Accents};
    use ss13_game::actions::ActionButtons;
    use ss13_game::genetics::effects::{self, Power};
    use ss13_game::genetics::{SOURCE_MUTATOR, add_mutation, mutadone, remove_mutation};
    use ss13_game::human::HumanLook;
    use ss13_game::species::{Species, SpeciesId};

    let mut s = Station::start("mutation_effects");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    let id = |path: &str| mutation_id(path).unwrap();

    // The Hulk: stun immune, green, shouting, too big for guns; mutadone takes it all away.
    {
        let world = s.0.server.world_mut();
        assert!(add_mutation(
            world,
            a,
            id("/datum/mutation/hulk"),
            SOURCE_MUTATOR
        ));
        assert!(effects::is_hulk(world, a));
        assert!(effects::chunky_fingers(world, a));
    }
    assert!(
        s.try_get::<ss13_game::status_effects::StunImmunity>(a)
            .is_some()
    );
    assert!(s.get::<HumanLook>(a).hulk);
    assert_eq!(s.get::<Accents>(a).0, vec![Accent::Hulk]);
    mutadone(s.0.server.world_mut(), a);
    assert!(
        s.try_get::<ss13_game::status_effects::StunImmunity>(a)
            .is_none()
    );
    assert!(!s.get::<HumanLook>(a).hulk);
    assert!(s.try_get::<Accents>(a).is_none());

    // A power mutation gives its button; casting conjures ash on the floor.
    let ash_power = id("/datum/mutation/cindikinesis");
    let pos = *s.get::<TilePos>(a);
    let on_tile = |s: &Station| {
        s.0.server
            .world()
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .len()
    };
    let before = on_tile(&s);
    {
        let world = s.0.server.world_mut();
        assert!(add_mutation(world, a, ash_power, SOURCE_MUTATOR));
        effects::cast(world, a, Power::Cindikinesis);
    }
    assert!(
        s.get::<ActionButtons>(a)
            .0
            .iter()
            .any(|(b, _)| b.id == "mutation:Cindikinesis")
    );
    s.run_for(0.2);
    assert_eq!(on_tile(&s), before + 1, "the ash lies there");
    remove_mutation(s.0.server.world_mut(), a, ash_power, SOURCE_MUTATOR);
    assert!(
        !s.get::<ActionButtons>(a)
            .0
            .iter()
            .any(|(b, _)| b.id.starts_with("mutation:"))
    );

    // Speech: an accent, and Unintelligible garbles through the impediments.
    {
        let world = s.0.server.world_mut();
        add_mutation(world, b, id("/datum/mutation/swedish"), SOURCE_MUTATOR);
        add_mutation(
            world,
            b,
            id("/datum/mutation/unintelligible"),
            SOURCE_MUTATOR,
        );
    }
    assert_eq!(s.get::<Accents>(b).0, vec![Accent::Swedish]);
    assert!(s.get::<ss13_chat::speech::Impediments>(b).unintelligible);
    mutadone(s.0.server.world_mut(), b);
    assert!(
        s.try_get::<ss13_chat::speech::Impediments>(b)
            .is_none_or(|i| !i.unintelligible)
    );

    // The monkey and back, name and all.
    let name = s.get::<Name>(b).to_string();
    let race = id("/datum/mutation/race");
    assert!(effects::make_monkey(s.0.server.world_mut(), b));
    assert_eq!(s.get::<Species>(b).0, SpeciesId::Monkey);
    assert_eq!(s.get::<Name>(b).as_str(), "monkey");
    remove_mutation(s.0.server.world_mut(), b, race, STANDARD);
    assert_eq!(s.get::<Species>(b).0, SpeciesId::Human);
    assert_eq!(s.get::<Name>(b).as_str(), name);

    // A cryo beam at oneself hits at once and chills the body (-350 twice, unprotected).
    let cryo = id("/datum/mutation/cryokinesis");
    {
        let world = s.0.server.world_mut();
        assert!(add_mutation(world, a, cryo, SOURCE_MUTATOR));
        effects::fire_beam(world, a, Power::Cryokinesis, Some(a), pos);
    }
    assert!(
        s.get::<ActionButtons>(a)
            .0
            .iter()
            .any(|(b, _)| b.id == "mutation:Cryokinesis")
    );
    s.run_for(0.2);
    assert!(s.get::<ss13_game::breath::Respiration>(a).bodytemperature < 100.0);
    remove_mutation(s.0.server.world_mut(), a, cryo, SOURCE_MUTATOR);

    // Stoner: Beach Bum only; heckacious speech keeps its case.
    {
        use ss13_chat::language::{LanguageHolder, LanguageId, SPOKEN};
        let world = s.0.server.world_mut();
        add_mutation(world, b, id("/datum/mutation/stoner"), SOURCE_MUTATOR);
        add_mutation(world, b, id("/datum/mutation/heckacious"), SOURCE_MUTATOR);
        let holder = world.get::<LanguageHolder>(b).unwrap();
        assert!(holder.has(LanguageId::Beachbum, SPOKEN));
        assert!(!holder.has(LanguageId::Common, SPOKEN));
    }
    assert_eq!(s.get::<Accents>(b).0, vec![Accent::Heckacious]);
    mutadone(s.0.server.world_mut(), b);
    {
        use ss13_chat::language::{LanguageHolder, LanguageId, SPOKEN};
        let holder = s.get::<LanguageHolder>(b);
        assert!(!holder.has(LanguageId::Beachbum, SPOKEN));
    }

    // The comely aura cheers up whoever sees its carrier within 3 tiles.
    add_mutation(
        s.0.server.world_mut(),
        a,
        id("/datum/mutation/aoe_moodlet/positive"),
        SOURCE_MUTATOR,
    );
    s.run_until("b sees a pretty face", |s| {
        s.get::<ss13_game::mood::Mood>(b)
            .events
            .iter()
            .any(|e| e.id == "seen_pretty_mutation" && e.mood_change == 3.0)
    });
}

const STANDARD: u8 = ss13_game::genetics::STANDARD_SOURCES;
