//! Exploration sites and their events, after tg `exploration_site.dm`,
//! `exploration_events/*.dm`, `loot.dm` and the scan conditions of `scanner_array.dm`: the
//! site kinds and their generation (`generate_exploration_sites`, `/datum/exploration_site/New`),
//! the event definitions (fluff, resources, dangers, traders, the adventure wrapper), the loot
//! generators (`/datum/adventure_loot_generator`) and the round's exploration state.
//!
//! Sites and events are data in [`Exploration`]; their ids (`ref`s in the windows) are stable
//! for the round.
// shortcut: the `maint` loot generator picks from a short list of common maintenance items
// (tg's `GLOB.maintenance_loot` tables are not converted); the `pets` generator gives nothing
// (pets in biopod carriers are not ported).

use std::collections::HashMap;

use bevy::prelude::*;
use serde_json::Value;
use ss13_mapload::spawner::Rng;

use super::adventure::{Adventure, AdventureEntry};
use super::{BANDS, Tool};

pub const PLASMA: &str = BANDS[0];
pub const LIFE: &str = BANDS[1];
pub const TECH: &str = BANDS[2];
pub const RADIATION: &str = BANDS[3];
pub const DENSITY: &str = BANDS[4];

// `EXPLORATION_SITE_*`.
pub const RUINS: &str = "ruins";
pub const TECHNOLOGY: &str = "technology present";
pub const STATION: &str = "space station";
pub const ALIEN: &str = "alien";
pub const HABITABLE: &str = "habitable";
pub const SPACE: &str = "in space";
pub const SURFACE: &str = "on surface";
pub const SHIP: &str = "spaceship";
pub const CIVILIZED: &str = "civilized";

/// `MAX_SCAN_DISTANCE`: the last band a wide scan reaches.
pub const MAX_SCAN_DISTANCE: u32 = 10;

/// `strings/exodrone.json`.
#[derive(Clone, Debug, Default)]
pub struct Strings {
    pub probe_names: Vec<String>,
    pub trading_station_names: Vec<String>,
    /// `(name, description, habitable, civilized, tech)`.
    pub planet_types: Vec<(String, String, bool, bool, bool)>,
    pub alien_fauna: Vec<String>,
    /// `fluff_*` key -> lines.
    pub fluff: HashMap<String, Vec<String>>,
}

impl Strings {
    pub fn parse(text: &str) -> Self {
        let Ok(raw) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        let list = |key: &str| -> Vec<String> {
            raw.get(key)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let flag = |p: &Value, key: &str| p.get(key).and_then(Value::as_bool).unwrap_or(false);
        let planet_types = raw
            .get("planet_types")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|p| {
                        let text = |key: &str| {
                            p.get(key)
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned()
                        };
                        (
                            text("name"),
                            text("description"),
                            flag(p, "habitable"),
                            flag(p, "civilized"),
                            flag(p, "tech"),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let fluff = raw
            .as_object()
            .map(|o| {
                o.keys()
                    .filter(|k| k.starts_with("fluff_"))
                    .map(|k| (k.clone(), list(k)))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            probe_names: list("probe_names"),
            trading_station_names: list("trading_station_names"),
            planet_types,
            alien_fauna: list("alien_fauna"),
            fluff,
        }
    }
}

/// `/datum/scan_condition` subtypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanCondition {
    Nebula,
    Pulsar,
    AsteroidBelt,
    BlackHole,
    Easy,
}

impl ScanCondition {
    pub const ALL: [Self; 5] = [
        Self::Nebula,
        Self::Pulsar,
        Self::AsteroidBelt,
        Self::BlackHole,
        Self::Easy,
    ];

    /// The `/datum/scan_condition` subtype.
    pub fn type_path(self) -> &'static str {
        match self {
            Self::Nebula => "/datum/scan_condition/nebula",
            Self::Pulsar => "/datum/scan_condition/pulsar",
            Self::AsteroidBelt => "/datum/scan_condition/asteroid_belt",
            Self::BlackHole => "/datum/scan_condition/black_hole",
            Self::Easy => "/datum/scan_condition/easy",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Nebula => "Nebula",
            Self::Pulsar => "Pulsar",
            Self::AsteroidBelt => "Asteroid Belt",
            Self::BlackHole => "Black Hole",
            Self::Easy => "Easy Scan",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Nebula => {
                "Site is within an unusually dense nebula. To reduce scanner noise, position \
                 dishes at least 15 tiles apart."
            }
            Self::Pulsar => {
                "A pulsar near the site requires dishes to be shielded from electomagnetic \
                 noise. Ensure no other machines are working near the dish."
            }
            Self::AsteroidBelt => {
                "An asteroid belt is obscuring the direct line of sight from the station to the \
                 site. Ensure the dishes are placed outside of the station z level."
            }
            Self::BlackHole => {
                "A background black hole requires you to focus the scan point precisely. Ensure \
                 the dishes are isolated from rest of the station with at least 6 walls around \
                 them."
            }
            Self::Easy => "This site is very easy to scan, all dish power is doubled.",
        }
    }
}

/// What a resource or loot generator gives: a type (a stack with its amount).
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    pub path: &'static str,
    /// Stacks: the amount (tg `new sheet_type(amount)`); 0 = the type's own.
    pub amount: u32,
}

const fn spawn(path: &'static str) -> Spawn {
    Spawn { path, amount: 0 }
}

/// A resource event's loot: a type, or a loot generator by its `id`.
#[derive(Clone, Debug, PartialEq)]
pub enum Loot {
    Item(Spawn),
    Generator(&'static str),
}

/// The mineral deposit's materials (`possible_materials`): name and sheet.
pub const DEPOSIT_MATERIALS: [(&str, &str); 3] = [
    ("silver", "/obj/item/stack/sheet/mineral/silver"),
    ("bananium", "/obj/item/stack/sheet/mineral/bananium"),
    ("pizza", "/obj/item/stack/sheet/pizza"),
];

/// `/datum/supply_pack/exploration/*` the trade contract chip unlocks.
pub const TRADE_CONTRACT_PACKS: [&str; 3] = [
    "/datum/supply_pack/exploration/scrapyard",
    "/datum/supply_pack/exploration/catering",
    "/datum/supply_pack/exploration/shrubbery",
];
pub const TRADE_CHIP: &str = "/obj/item/trade_chip";

const MAINT_LOOT: &[&str] = &[
    "/obj/item/trash/can",
    "/obj/item/trash/chips",
    "/obj/item/trash/candy",
    "/obj/item/crowbar",
    "/obj/item/screwdriver",
    "/obj/item/wirecutters",
    "/obj/item/weldingtool",
    "/obj/item/analyzer",
    "/obj/item/t_scanner",
    "/obj/item/flashlight",
    "/obj/item/stack/cable_coil",
];
const UNIQUE_LOOT: &[&str] = &[
    "/obj/item/clothing/glasses/geist_gazers",
    "/obj/item/clothing/glasses/psych",
    "/obj/item/firelance",
];
const CASH_LOOT: &[&str] = &[
    "/obj/item/storage/bag/money",
    "/obj/item/antique",
    "/obj/item/stack/spacecash/c1000",
    "/obj/item/holochip/thousand",
];
const DRUGS_LOOT: &[&str] = &[
    "/obj/item/storage/pill_bottle/happy",
    "/obj/item/storage/pill_bottle/lsd",
    "/obj/item/storage/pill_bottle/penacid",
    "/obj/item/storage/pill_bottle/stimulant",
];
const MATERIALS_LOOT: &[&str] = &[
    "/obj/item/stack/sheet/iron/fifty",
    "/obj/item/stack/sheet/plasteel/twenty",
];
const WEAPONS_LOOT: &[&str] = &[
    "/obj/item/gun/energy/laser",
    "/obj/item/melee/baton/security/loaded",
];
const SYNDICATE_FISH_LOOT: &[&str] = &["/obj/item/storage/fish_case/syndicate"];

/// Every type exploration spawns (the `ss13_mapload::runtime` registry checks it).
pub fn runtime_types() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = [
        MAINT_LOOT,
        UNIQUE_LOOT,
        CASH_LOOT,
        DRUGS_LOOT,
        MATERIALS_LOOT,
        WEAPONS_LOOT,
        SYNDICATE_FISH_LOOT,
    ]
    .concat();
    out.extend(DEPOSIT_MATERIALS.iter().map(|m| m.1));
    out.push(TRADE_CHIP);
    for def in event_defs() {
        match def.kind {
            EventKind::Resource(Resource {
                loot: Loot::Item(s),
                ..
            }) => out.push(s.path),
            EventKind::Trader(t) => {
                out.extend(t.traded.iter().copied());
            }
            _ => {}
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// `/datum/adventure_loot_generator/<id>/generate`: what it gives (cargo first checks room).
pub fn generate_loot(id: &str, rng: &mut Rng) -> Vec<Spawn> {
    let pick = |list: &[&'static str], rng: &mut Rng| {
        let i = rng.range(0, list.len() as i64 - 1) as usize;
        vec![spawn(list[i])]
    };
    match id {
        "maint" => pick(MAINT_LOOT, rng),
        // `/obj/item/trade_chip(null, pack)`: which pack is `crate::science::exodrone::server`'s
        // to record (it picks one of [`TRADE_CONTRACT_PACKS`]).
        "trade_contract" => vec![spawn(TRADE_CHIP)],
        "unique" => pick(UNIQUE_LOOT, rng),
        "cash" => pick(CASH_LOOT, rng),
        "drugs" => pick(DRUGS_LOOT, rng),
        "materials" => pick(MATERIALS_LOOT, rng),
        "weapons" => pick(WEAPONS_LOOT, rng),
        "syndicate_fish" => pick(SYNDICATE_FISH_LOOT, rng),
        _ => Vec::new(),
    }
}

/// A resource event (`/datum/exploration_event/simple/resource`).
#[derive(Clone, Debug, PartialEq)]
pub struct Resource {
    pub required_tool: Option<Tool>,
    pub loot: Loot,
    pub success_log: String,
    pub no_tool_description: String,
    pub has_tool_description: String,
    /// Seconds (`delay`, 30 s).
    pub delay: f64,
    pub delay_message: String,
    pub amount: u32,
}

/// A danger event (`/datum/exploration_event/simple/danger`).
#[derive(Clone, Debug, PartialEq)]
pub struct Danger {
    pub required_tool: Option<Tool>,
    pub has_tool_action_text: String,
    pub no_tool_action_text: String,
    pub has_tool_description: String,
    pub no_tool_description: String,
    pub avoid_log: String,
    pub damage: f32,
}

/// A trader event (`/datum/exploration_event/simple/trader`). The lists are picked from at
/// creation (`New`).
#[derive(Clone, Debug, PartialEq)]
pub struct Trader {
    pub wanted: Vec<&'static str>,
    pub traded: Vec<&'static str>,
    pub amount: u32,
    pub requires_translator: bool,
    /// `flame_card`: restyles an ID card instead of trading it.
    pub id_artisan: bool,
}

#[derive(Clone, Debug)]
pub enum EventKind {
    Fluff,
    Adventure(Box<Adventure>),
    Resource(Resource),
    Danger(Danger),
    Trader(Trader),
}

/// `/datum/exploration_event` with the `simple` fields (`description`, `action_text`, ...).
#[derive(Clone, Debug)]
pub struct Event {
    pub id: u32,
    pub name: String,
    pub visited: bool,
    pub band_values: Vec<(String, f64)>,
    pub discovery_log: Option<String>,
    pub point_scan_description: Option<String>,
    pub deep_scan_description: Option<String>,
    pub description: String,
    pub action_text: String,
    pub kind: EventKind,
}

impl Event {
    /// `is_targetable`: resources and traders with uses left can be revisited.
    pub fn is_targetable(&self) -> bool {
        match &self.kind {
            EventKind::Resource(r) => self.visited && r.amount > 0,
            EventKind::Trader(t) => self.visited && t.amount > 0,
            _ => false,
        }
    }

    /// A simple event (one screen with act/ignore).
    pub fn is_simple(&self) -> bool {
        matches!(
            self.kind,
            EventKind::Resource(_) | EventKind::Danger(_) | EventKind::Trader(_)
        )
    }

    /// `skippable`: dangers cannot be ignored.
    pub fn skippable(&self) -> bool {
        !matches!(self.kind, EventKind::Danger(_))
    }
}

/// The event families of `generate_events`' weights.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Fluff,
    Danger,
    Trader,
    Resource,
}

/// An event type: its requirements and its fresh instance.
pub struct EventDef {
    pub required: &'static [&'static str],
    pub blacklisted: &'static [&'static str],
    pub category: Category,
    pub kind: EventKind,
    pub name: &'static str,
    pub band_values: &'static [(&'static str, f64)],
    pub discovery_log: Option<&'static str>,
    pub deep_scan_description: Option<&'static str>,
    pub description: &'static str,
    pub action_text: &'static str,
}

#[allow(clippy::too_many_arguments)]
fn resource(
    name: &'static str,
    required: &'static [&'static str],
    band_values: &'static [(&'static str, f64)],
    tool: Tool,
    discovery_log: &'static str,
    description: &'static str,
    no_tool: &'static str,
    has_tool: &'static str,
    action_text: &'static str,
    delay_message: &'static str,
    success_log: &'static str,
    loot: Loot,
    amount: u32,
) -> EventDef {
    EventDef {
        required,
        blacklisted: &[],
        category: Category::Resource,
        kind: EventKind::Resource(Resource {
            required_tool: Some(tool),
            loot,
            success_log: success_log.to_owned(),
            no_tool_description: no_tool.to_owned(),
            has_tool_description: has_tool.to_owned(),
            delay: 30.0,
            delay_message: delay_message.to_owned(),
            amount,
        }),
        name,
        band_values,
        discovery_log: Some(discovery_log),
        deep_scan_description: None,
        description,
        action_text,
    }
}

#[allow(clippy::too_many_arguments)]
fn danger(
    name: &'static str,
    required: &'static [&'static str],
    blacklisted: &'static [&'static str],
    tool: Tool,
    deep_scan: &'static str,
    description: &'static str,
    has_tool_action: &'static str,
    no_tool_action: &'static str,
    has_tool: &'static str,
    no_tool: &'static str,
    avoid_log: &'static str,
) -> EventDef {
    EventDef {
        required,
        blacklisted,
        category: Category::Danger,
        kind: EventKind::Danger(Danger {
            required_tool: Some(tool),
            has_tool_action_text: has_tool_action.to_owned(),
            no_tool_action_text: no_tool_action.to_owned(),
            has_tool_description: has_tool.to_owned(),
            no_tool_description: no_tool.to_owned(),
            avoid_log: avoid_log.to_owned(),
            damage: 30.0,
        }),
        name,
        band_values: &[],
        discovery_log: None,
        deep_scan_description: Some(deep_scan),
        description,
        action_text: "encounter",
    }
}

#[allow(clippy::too_many_arguments)]
fn trader(
    name: &'static str,
    required: &'static [&'static str],
    band_values: &'static [(&'static str, f64)],
    deep_scan: Option<&'static str>,
    wanted: &[&'static str],
    traded: &[&'static str],
    amount: u32,
    requires_translator: bool,
) -> EventDef {
    EventDef {
        required,
        blacklisted: &[],
        category: Category::Trader,
        kind: EventKind::Trader(Trader {
            wanted: wanted.to_vec(),
            traded: traded.to_vec(),
            amount,
            requires_translator,
            id_artisan: false,
        }),
        name,
        band_values,
        discovery_log: None,
        deep_scan_description: deep_scan,
        description: "",
        action_text: "Trade",
    }
}

/// `valid_subtypesof(/datum/exploration_event)` but the adventure wrapper.
pub fn event_defs() -> Vec<EventDef> {
    use Tool::*;
    let item = |path| Loot::Item(spawn(path));
    let mut defs = vec![EventDef {
        required: &[],
        blacklisted: &[],
        category: Category::Fluff,
        kind: EventKind::Fluff,
        name: "fluff event",
        band_values: &[],
        discovery_log: None,
        deep_scan_description: None,
        description: "",
        action_text: "",
    }];
    defs.extend([
        resource(
            "concealed cache",
            &[],
            &[(DENSITY, 1.0)],
            Welder,
            "Discovered a concealed, locked cache.",
            "You spot a cleverly hidden metal container.",
            "You see no way to open it without a welder.",
            "You can try and open it with your welder.",
            "Weld open",
            "Welding open the cache...",
            "Retrieved something",
            Loot::Generator("maint"),
            1,
        ),
        resource(
            "desiccated corpse",
            &[RUINS],
            &[],
            Multitool,
            "Discovered a corpse of a humanoid.",
            "You find a dessicated corpose of a humanoid, though it's too damaged to identify. \
             A locked briefcase is lying nearby.",
            "You can't open it without a multiool.",
            "You can try to hack it open with your multitool!",
            "Hack open",
            "Hacking...",
            "Retrieved something",
            Loot::Generator("cash"),
            1,
        ),
        resource(
            "gunfight leftovers",
            &[RUINS],
            &[],
            Drill,
            "Discovered a site of a past gunfight.",
            "You find a site full of gun casings and scorched with laser marks. You notice \
             something under some nearby rubble.",
            "You can't get to it without a drill.",
            "You can remove the rubble with your drill!",
            "Remove rubble",
            "Drilling...",
            "Retrieved something",
            Loot::Generator("weapons"),
            1,
        ),
        resource(
            "locked maintenance room",
            &[TECHNOLOGY, STATION],
            &[],
            Multitool,
            "Discovered a locked maintenance room.",
            "You discover a locked maintenance room. You can see marks from frequent movement \
             nearby.",
            "You can't open it without a multitool.",
            "You can try to open it with your multitool!",
            "Hack",
            "Hacking...",
            "Retrieved the contents of a locked maintenance room.",
            Loot::Generator("maint"),
            3,
        ),
        resource(
            "storage room",
            &[TECHNOLOGY, STATION],
            &[],
            Translator,
            "Discovered a storage room full of crates.",
            "You find a storage room full of unidentified crates. There's a manifest in an \
             obscure language pinned near the entrance.",
            "All the crates around are devoid of useful contents, and the manifest is \
             unreadable without a translator.",
            "You can translate the manifest with your translator!",
            "Translate",
            "Translating manifest...",
            "Used a translated manifest to find a crate of medication.",
            Loot::Generator("drugs"),
            1,
        ),
        resource(
            "alien sarcophagus",
            &[ALIEN],
            &[(TECH, 1.0), (RADIATION, 1.0)],
            Translator,
            "Discovered an alien sarcophagus covered in unknown glyphs.",
            "You find a giant sarcophagus of alien origin, covered in unknown script.",
            "You see no way to open the sarcophagus nor translate the glyphs without a \
             multitool.",
            "You translate the glyphs and find a description of a hidden mechanism for \
             unlocking the tomb.",
            "Open",
            "Opening...",
            "Retrieved contents of an alien sarcophagus.",
            item("/obj/item/scalpel/alien"),
            1,
        ),
        resource(
            "alien biopod",
            &[ALIEN],
            &[(LIFE, 1.0)],
            Laser,
            "Discovered an alien pod.",
            "You encounter an alien biopod full of strange sacks containing abducted lifeforms.",
            "You can't breach the biopod without a precise laser.",
            "You can try to cut one open with your laser!",
            "Open",
            "Opening...",
            "Retrieved contents of the alien pod.",
            Loot::Generator("pets"),
            1,
        ),
        resource(
            "fuel storage",
            &[SHIP],
            &[(PLASMA, 1.0)],
            Multitool,
            "Discovered ship fuel storage.",
            "You find the ship's fuel storage. Unfortunately, it has an electronic lock.",
            "You can't breach the lock without a multitool.",
            "You can try and short circuit the lock with your multitool!",
            "Open",
            "Opening...",
            "Retrieved fuel from storage.",
            item(super::FUEL_EXOTIC),
            1,
        ),
        resource(
            "navigation systems",
            &[SHIP],
            &[],
            Translator,
            "Discovered ship navigation systems.",
            "You find the ship's navigation systems encoded in a strange language. You'll be \
             able to use the data with a translator.",
            "You'll need a translator to decipher the data.",
            "You can try and translate the navigation data with your multitool!",
            "Retrieve data",
            "Retrieving data...",
            "Retrieved shipping data from navigation systems.",
            Loot::Generator("trade_contract"),
            1,
        ),
        resource(
            "unknown microbiome",
            &[HABITABLE],
            &[],
            Translator,
            "Discovered a isolated microbiome.",
            "You discover a giant fungus colony.",
            "With a precise laser, you could slice off a sample for study.",
            "You can carefully cut a sample from the colony with your laser!",
            "Take sample",
            "Taking samples...",
            "Retrieved samples of the fungus for future study.",
            item("/obj/item/petri_dish/random"),
            1,
        ),
        resource(
            "creepy stranger",
            &[HABITABLE],
            &[(LIFE, 1.0)],
            Translator,
            "Met a creepy stranger.",
            "You meet an inhabitant of this site, looking ragged and clearly agitated about \
             something.",
            "You can't tell what it's trying to convey without a translator.",
            "Your best translation dictates that it would like to share its hobby with you!",
            "Accept gift.",
            "Enduring...",
            "Received a gift from a stranger.",
            item("/obj/item/cardpack/series_one"),
            1,
        ),
        resource(
            "derelict comms satellite",
            &[SPACE],
            &[],
            Multitool,
            "Discovered a derelict communication satellite.",
            "You discover a derelict communication satellite. Its encryption key is intact, but \
             has a complicated electronic lock.",
            "You'll need a multiool to retrieve the encryption key.",
            "You can disable the lock to retrieve the key with your multitool!",
            "Hack lock",
            "Hacking...",
            "Retrieved an encryption key from a derelict satellite.",
            item("/obj/item/encryptionkey/heads/captain"),
            1,
        ),
        // `welded_locker` gives a skeleton's head (tg makes a skeleton and cuts its head off).
        resource(
            "welded locker",
            &[SPACE],
            &[],
            Welder,
            "Discovered a hastily welded locker.",
            "You discover a welded locker floating through space. What could be inside...?",
            "You'll need a welding tool to take the contents of the locker.",
            "You can get it out with your drill!",
            "Weld open",
            "Welding open...",
            "Retrieved... a severed head.",
            item("/obj/item/bodypart/head/skeleton"),
            1,
        ),
        resource(
            "Raw Plasma Deposit",
            &[SURFACE],
            &[(PLASMA, 3.0)],
            Drill,
            "Discovered a sizeable plasma deposit.",
            "You locate a rich surface deposit of plasma.",
            "You'll need a drill to take anything from the deposit.",
            "Your drill will allow you to extract the deposit!",
            "Mine",
            "Mining...",
            "Extracted the plasma from the deposit.",
            item("/obj/item/stack/sheet/mineral/plasma/thirty"),
            1,
        ),
        // `mineral_deposit`: the material is picked in [`instance`].
        resource(
            "MATERIAL Deposit",
            &[SURFACE],
            &[(DENSITY, 3.0)],
            Drill,
            "Discovered a sizeable MATERIAL deposit",
            "You locate a rich surface deposit of MATERIAL.",
            "You'll need a drill to take anything from the deposit.",
            "Your drill will allow you to extract the deposit!",
            "Mine",
            "Mining...",
            "Extracted MATERIAL.",
            Loot::Item(Spawn {
                path: DEPOSIT_MATERIALS[0].1,
                amount: 30,
            }),
            1,
        ),
    ]);
    let carp = |required: &'static [&'static str]| {
        danger(
            "space carp attack",
            required,
            &[CIVILIZED],
            Laser,
            "You detect damage patterns to the site indicating the presence of space carp.",
            "You are ambushed by a solitary space carp!",
            "Fight",
            "Escape!",
            "You charge your laser to fend it off.",
            "Without any weaponry, you can only attempt a hasty escape!",
            "Defeated a space carp.",
        )
    };
    defs.extend([
        carp(&[SPACE]),
        carp(&[SURFACE]),
        danger(
            "assistant attack",
            &[STATION],
            &[],
            Laser,
            "Detected mask usage coefficent suggests a sizeable crowd of undersirables on the \
             site.",
            "You encounter a shaggy creature dressed in gray! It's a deranged assistant!",
            "Fight",
            "Escape!",
            "You charge your laser to fend it off.",
            "Without any weaponry, you can only attempt a hasty escape!",
            "Defeated an assistant.",
        ),
        danger(
            "collapse",
            &[RUINS],
            &[],
            Drill,
            "The scan indicates that the site's structure is decrepit; caution is advised.",
            "A damaged ceiling gives out as you search an unexplored passage! You're trapped by \
             the debris.",
            "Dig out",
            "Squeeze.",
            "You can use your drill to get out.",
            "You'll have to scrape a few parts to get out without any tools.",
            "Dug out of collapsed passage.",
        ),
        danger(
            "loose wires",
            &[TECHNOLOGY],
            &[],
            Multitool,
            "Scans indicate a huge quantity of damaged wiring detected on site.",
            "You hear a loud snap behind you! A stack of sparking high-voltage wires is blocking \
             you way out.",
            "Disable power",
            "Get fried.",
            "You can try to use your multitool to shut down power to escape.",
            "You'll have to risk frying your electronics getting out.",
            "Escaped loose wire.",
        ),
        danger(
            "cosmic ray burst",
            &[SURFACE],
            &[],
            Multitool,
            "Site is exposed to space radiation. Using self-diagnostic multiool attachment \
             advised.",
            "The drone feed suddenly goes haywire! It seems that the drone got hit by a burst \
             of cosmic rays! You'll have to wait for the signal to be restored.",
            "Wait",
            "Wait",
            "Your multitool should allow a significant amount of the damage to be repaired on \
             its own.",
            "Nothing more to be done than to wait and assess the damage.",
            "Prevented cosmic ray damage with multitool.",
        ),
        danger(
            "alien security measure",
            &[ALIEN],
            &[],
            Translator,
            "Automated security measures of unknown origin detected on site.",
            "A dangerous looking machine slides out the floor and start flashing strange glyphs \
             while emitting a high-pitched sound!",
            "Identify yourself",
            "Escape",
            "Your translator recognizes the glyphs as security hail and suggests identyfing \
             yourself as a guest.",
            "The machine start shooting soon after.",
            "Avoided alien security.",
        ),
        // `beast`: `BEAST` in the texts is a random `alien_fauna` name ([`instance`]).
        danger(
            "alien encounter",
            &[HABITABLE],
            &[CIVILIZED],
            Laser,
            "Dangerous fauna detected on site.",
            "You encounter an unusual beast! It prepares to strike.",
            "Fight",
            "Escape",
            "You ready your laser.",
            "Without any weaponry, you can only attempt a hasty escape!",
            "Defeated a beast.",
        ),
        danger(
            "irradiated section",
            &[SHIP],
            &[],
            Multitool,
            "Scans indicate dangerous radioactive presence.",
            "You enter a nondescript ship section.",
            "Detour",
            "Escape and mitigate damage.",
            "Your multitool suddenly flashes in warning! The section ahead is irradiated, \
             you'll have to go around to avoid damage.",
            "Suddenly the drone reports significant damage, it seems that this section was \
             heavily irradiated.",
            "Avoided irradiated section.",
        ),
    ]);
    const CELL: &str = "/obj/item/stock_parts/power_store/cell/high";
    defs.extend([
        trader(
            "sentient drug vending machine",
            &[TECHNOLOGY],
            &[(TECH, 2.0)],
            None,
            &[CELL],
            &["/obj/item/storage/pill_bottle/happy"],
            3,
            false,
        ),
        trader(
            "farmer's market",
            &[HABITABLE, SURFACE],
            &[(LIFE, 2.0)],
            Some("You detect an area with an unusually high concentration of edibles on site."),
            &["/obj/item/stock_parts/servo/nano"],
            &[
                "/obj/item/seeds/tomato/killer",
                "/obj/item/seeds/orange_3d",
                "/obj/item/seeds/firelemon",
                "/obj/item/seeds/gatfruit",
            ],
            1,
            true,
        ),
        trader(
            "interstellar fish trader",
            &[HABITABLE, SURFACE],
            &[(LIFE, 2.0)],
            Some("You spot a giant \"FRESH FISH\" sign on site."),
            &[CELL],
            &["/obj/item/storage/fish_case/random"],
            3,
            false,
        ),
        trader(
            "shady merchant",
            &[HABITABLE, CIVILIZED],
            &[(LIFE, 1.0)],
            None,
            &[
                "/obj/item/organ/heart",
                "/obj/item/organ/liver",
                "/obj/item/organ/stomach",
                "/obj/item/organ/eyes",
            ],
            &["/obj/item/implanter/explosive"],
            1,
            false,
        ),
        trader(
            "military surplus trader",
            &[HABITABLE, CIVILIZED],
            &[(LIFE, 1.0)],
            Some("You decrypt a transmission advertising military surplus for sale on site."),
            &[
                "/obj/item/clothing/suit/armor",
                "/obj/item/clothing/shoes/jackboots",
            ],
            &["/obj/item/gun/energy/laser/retro/old"],
            3,
            true,
        ),
    ]);
    let mut artisan = trader(
        "id card artisan",
        &[HABITABLE, CIVILIZED],
        &[(TECH, 1.0)],
        Some("You spy an advertisment for an ID card customisation workshop."),
        &["/obj/item/card/id"],
        &[],
        1,
        false,
    );
    if let EventKind::Trader(t) = &mut artisan.kind {
        t.id_artisan = true;
    }
    defs.push(artisan);
    defs
}

/// `flame_card`'s `possible_card_states`.
pub const ARTISAN_CARD_STATES: [&str; 3] = ["card_flames", "card_carp", "card_rainbow"];

fn pick_index(len: usize, rng: &mut Rng) -> usize {
    rng.range(0, len as i64 - 1).max(0) as usize
}

/// `pick_weight` over `(item, weight)`.
fn pick_weighted<T: Copy>(items: &[(T, u32)], rng: &mut Rng) -> Option<T> {
    let total: u32 = items.iter().map(|i| i.1).sum();
    if total == 0 {
        return None;
    }
    let mut r = rng.range(1, i64::from(total));
    items.iter().find_map(|&(t, w)| {
        r -= i64::from(w);
        (r <= 0).then_some(t)
    })
}

/// A fresh event of `def` (`new event_type`, with the subtypes' `New` randomization).
fn instance(def: &EventDef, id: u32, strings: &Strings, rng: &mut Rng) -> Event {
    let mut event = Event {
        id,
        name: def.name.to_owned(),
        visited: false,
        band_values: def
            .band_values
            .iter()
            .map(|&(b, v)| (b.to_owned(), v))
            .collect(),
        discovery_log: def.discovery_log.map(str::to_owned),
        point_scan_description: None,
        deep_scan_description: def.deep_scan_description.map(str::to_owned),
        description: def.description.to_owned(),
        action_text: def.action_text.to_owned(),
        kind: def.kind.clone(),
    };
    match &mut event.kind {
        EventKind::Trader(t) => {
            // `New`: one of each list.
            if !t.wanted.is_empty() {
                t.wanted = vec![t.wanted[pick_index(t.wanted.len(), rng)]];
            }
            if !t.traded.is_empty() {
                t.traded = vec![t.traded[pick_index(t.traded.len(), rng)]];
            }
        }
        EventKind::Resource(r) if event.name == "MATERIAL Deposit" => {
            let (mat, sheet) = DEPOSIT_MATERIALS[pick_index(DEPOSIT_MATERIALS.len(), rng)];
            event.name = format!("{mat} Deposit");
            event.discovery_log = Some(format!("Discovered a sizeable {mat} deposit"));
            event.description = format!("You locate a rich surface deposit of {mat}.");
            r.success_log = format!("Extracted {mat}.");
            r.loot = Loot::Item(Spawn {
                path: sheet,
                amount: 30,
            });
        }
        EventKind::Danger(d)
            if event.name == "alien encounter" && !strings.alien_fauna.is_empty() =>
        {
            let beast = &strings.alien_fauna[pick_index(strings.alien_fauna.len(), rng)];
            event.description = event.description.replace("BEAST", beast);
            d.avoid_log = d.avoid_log.replace("BEAST", beast);
        }
        _ => {}
    }
    event
}

/// `/datum/exploration_site` subtypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteKind {
    AbandonedRefuelingStation,
    TraderPost,
    CargoWreck,
    AlienSpaceship,
    UnchartedPlanet,
    AlienRuins,
    AsteroidBelt,
    SpaceMine,
    Junkyard,
}

impl SiteKind {
    pub const ALL: [Self; 9] = [
        Self::AbandonedRefuelingStation,
        Self::TraderPost,
        Self::CargoWreck,
        Self::AlienSpaceship,
        Self::UnchartedPlanet,
        Self::AlienRuins,
        Self::AsteroidBelt,
        Self::SpaceMine,
        Self::Junkyard,
    ];

    /// The `/datum/exploration_site` subtype.
    pub fn type_path(self) -> &'static str {
        match self {
            Self::AbandonedRefuelingStation => {
                "/datum/exploration_site/abandoned_refueling_station"
            }
            Self::TraderPost => "/datum/exploration_site/trader_post",
            Self::CargoWreck => "/datum/exploration_site/cargo_wreck",
            Self::AlienSpaceship => "/datum/exploration_site/alien_spaceship",
            Self::UnchartedPlanet => "/datum/exploration_site/uncharted_planet",
            Self::AlienRuins => "/datum/exploration_site/alien_ruins",
            Self::AsteroidBelt => "/datum/exploration_site/asteroid_belt",
            Self::SpaceMine => "/datum/exploration_site/spacemine",
            Self::Junkyard => "/datum/exploration_site/junkyard",
        }
    }

    /// The type's `initial(name)`.
    pub fn name(self) -> &'static str {
        self.def().0
    }

    /// `(name, description, band_info, site_traits, fluff_type)`.
    #[allow(clippy::type_complexity)]
    fn def(
        self,
    ) -> (
        &'static str,
        &'static str,
        &'static [(&'static str, f64)],
        &'static [&'static str],
        &'static str,
    ) {
        match self {
            Self::AbandonedRefuelingStation => (
                "abandoned refueling station",
                "An old shuttle refueling station drifting through the void.",
                &[(TECH, 1.0)],
                &[RUINS, TECHNOLOGY, STATION],
                "fluff_generic",
            ),
            Self::TraderPost => (
                "unregistered trading station",
                "A weak radio transmission advertises this place as RANDOMIZED_NAME",
                &[(TECH, 1.0), (LIFE, 1.0)],
                &[TECHNOLOGY, STATION, HABITABLE, CIVILIZED],
                "fluff_trading",
            ),
            Self::CargoWreck => (
                "interstellar cargo ship wreckage",
                "Wreckage of a long-range cargo shuttle.",
                &[(TECH, 1.0), (DENSITY, 1.0)],
                &[SHIP, TECHNOLOGY],
                "fluff_generic",
            ),
            Self::AlienSpaceship => (
                "ancient alien spaceship",
                "A gigantic spaceship of unknown origin. It doesn't respond to your hails but \
                 does not prevent you boarding, either.",
                &[(TECH, 1.0), (RADIATION, 1.0)],
                &[SHIP, HABITABLE, ALIEN],
                "fluff_generic",
            ),
            Self::UnchartedPlanet => (
                "uncharted planet",
                "A planet missing from Nanotrasen starcharts.",
                &[(LIFE, 3.0)],
                &[SURFACE],
                "fluff_generic",
            ),
            Self::AlienRuins => (
                "alien ruins",
                "Alien ruins on a small moon's surface.",
                &[],
                &[HABITABLE, SURFACE, ALIEN, RUINS],
                "fluff_ruins",
            ),
            Self::AsteroidBelt => (
                "asteroid belt",
                "A dense asteroid belt.",
                &[],
                &[SURFACE],
                "fluff_space",
            ),
            Self::SpaceMine => (
                "mining facility",
                "An abandoned mining facility attached to an ore-rich asteroid.",
                &[(PLASMA, 3.0)],
                &[RUINS, HABITABLE, SURFACE],
                "fluff_ruins",
            ),
            Self::Junkyard => (
                "space junk field",
                "A giant cluster of space junk.",
                &[(DENSITY, 3.0)],
                &[TECHNOLOGY, SPACE],
                "fluff_space",
            ),
        }
    }
}

/// `/datum/exploration_site`.
#[derive(Clone, Debug)]
pub struct Site {
    pub id: u32,
    pub kind: SiteKind,
    pub name: String,
    pub description: String,
    pub distance: u32,
    pub coordinates: String,
    pub revealed: bool,
    pub point_scan_complete: bool,
    pub deep_scan_complete: bool,
    /// Band -> value, site's own then its events' (`add_event`).
    pub band_info: Vec<(String, f64)>,
    pub events: Vec<Event>,
    pub traits: Vec<&'static str>,
    pub fluff_type: &'static str,
    pub conditions: Vec<ScanCondition>,
}

impl Site {
    /// `/datum/experiment/exploration_scan/is_complete()` for this site: scanned with
    /// `scan_type` (`point`/`deep`; anything else: any), of `site_type` and with `condition`
    /// (empty: any).
    pub fn fits_exploration(&self, site_type: &str, condition: &str, scan_type: &str) -> bool {
        let scanned = match scan_type {
            "deep" => self.deep_scan_complete,
            "point" => self.point_scan_complete,
            _ => true,
        };
        scanned
            && (site_type.is_empty()
                || crate::obj_damage::is_under(self.kind.type_path(), site_type))
            && (condition.is_empty()
                || self
                    .conditions
                    .iter()
                    .any(|c| crate::obj_damage::is_under(c.type_path(), condition)))
    }

    /// `display_name`.
    pub fn display_name(&self) -> &str {
        if self.revealed { &self.name } else { "Anomaly" }
    }

    /// `display_description`.
    pub fn display_description(&self) -> String {
        if !self.revealed {
            return "No Data".to_owned();
        }
        let mut parts = vec![self.description.clone()];
        for event in &self.events {
            if self.deep_scan_complete
                && let Some(d) = &event.deep_scan_description
            {
                parts.push(d.clone());
            } else if self.point_scan_complete
                && let Some(d) = &event.point_scan_description
            {
                parts.push(d.clone());
            }
        }
        parts.join("\n")
    }

    pub fn event(&self, id: u32) -> Option<&Event> {
        self.events.iter().find(|e| e.id == id)
    }

    pub fn event_mut(&mut self, id: u32) -> Option<&mut Event> {
        self.events.iter_mut().find(|e| e.id == id)
    }

    /// `add_event`.
    fn add_event(&mut self, event: Event) {
        for (band, value) in &event.band_values {
            match self.band_info.iter_mut().find(|(b, _)| b == band) {
                Some(slot) => slot.1 += value,
                None => self.band_info.push((band.clone(), *value)),
            }
        }
        self.events.push(event);
    }
}

/// An exoscan in progress (`/datum/exoscan`).
#[derive(Clone, Debug, PartialEq)]
pub struct Scan {
    pub kind: ScanKind,
    pub power: f64,
    pub target: Option<u32>,
    /// `Time::elapsed_secs_f64` at its end.
    pub ends_at: f64,
    /// The console that started it gets the failure popup (`COMSIG_EXOSCAN_INTERRUPTED`).
    pub console: Option<Entity>,
}

/// `EXOSCAN_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanKind {
    Wide,
    Point,
    Deep,
}

/// `BASE_POINT_SCAN_TIME`, seconds.
pub const BASE_POINT_SCAN_TIME: f64 = 120.0;
/// `BASE_DEEP_SCAN_TIME`, seconds.
pub const BASE_DEEP_SCAN_TIME: f64 = 180.0;

/// `WIDE_SCAN_COST(band, power)`, seconds.
pub fn wide_scan_cost(band: u32, power: f64) -> f64 {
    let band = f64::from(band);
    ((band * band) / power * 2.0 * 60.0).min(600.0)
}

/// The round's exploration: `GLOB.exploration_sites`, `GLOB.exoscanner_controller`,
/// `GLOB.explorer_drone_adventure_db_entries` and the drone name counter.
#[derive(Resource)]
pub struct Exploration {
    pub sites: Vec<Site>,
    next_id: u32,
    /// `wide_scan_band`.
    pub wide_scan_band: u32,
    pub scan: Option<Scan>,
    pub adventures: Vec<AdventureEntry>,
    pub strings: Strings,
    pub rng: Rng,
    /// `/obj/item/exodrone::name_counter`.
    pub name_counter: HashMap<String, u32>,
}

impl Exploration {
    pub fn new(strings: Strings, adventures: Vec<AdventureEntry>, seed: u64) -> Self {
        Self {
            sites: Vec::new(),
            next_id: 1,
            wide_scan_band: 1,
            scan: None,
            adventures,
            strings,
            rng: Rng::new(seed),
            name_counter: HashMap::new(),
        }
    }

    pub fn site(&self, id: u32) -> Option<&Site> {
        self.sites.iter().find(|s| s.id == id)
    }

    pub fn site_mut(&mut self, id: u32) -> Option<&mut Site> {
        self.sites.iter_mut().find(|s| s.id == id)
    }

    fn id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// `generate_exploration_sites`: `1 + rand(band - 1, band + 1)` sites of the band, then the
    /// next band.
    pub fn generate_sites(&mut self) {
        let band = self.wide_scan_band;
        let count = 1 + self
            .rng
            .range(i64::from(band) - 1, i64::from(band) + 1)
            .max(0);
        for _ in 0..count {
            let kind = SiteKind::ALL[pick_index(SiteKind::ALL.len(), &mut self.rng)];
            let site = self.new_site(kind, band);
            self.sites.push(site);
        }
        self.wide_scan_band += 1;
    }

    /// `/datum/exploration_site/New(band)`.
    pub fn new_site(&mut self, kind: SiteKind, band: u32) -> Site {
        let (name, description, bands, traits, fluff_type) = kind.def();
        let mut site = Site {
            id: self.id(),
            kind,
            name: name.to_owned(),
            description: description.to_owned(),
            distance: 1,
            coordinates: String::new(),
            revealed: false,
            point_scan_complete: false,
            deep_scan_complete: false,
            band_info: bands.iter().map(|&(b, v)| (b.to_owned(), v)).collect(),
            events: Vec::new(),
            traits: traits.to_vec(),
            fluff_type,
            conditions: Vec::new(),
        };
        if kind == SiteKind::UnchartedPlanet && !self.strings.planet_types.is_empty() {
            let i = pick_index(self.strings.planet_types.len(), &mut self.rng);
            let (n, d, habitable, civilized, tech) = self.strings.planet_types[i].clone();
            site.name = n;
            site.description = d;
            for (flag, t) in [
                (habitable, HABITABLE),
                (civilized, CIVILIZED),
                (tech, TECHNOLOGY),
            ] {
                if flag {
                    site.traits.push(t);
                }
            }
        }
        let offset = [-1, 0, 1, 2][pick_index(4, &mut self.rng)];
        site.distance = (i64::from(band) + offset).max(1) as u32;
        let (l, b) = (self.rng.range(0, 360), self.rng.range(0, 90));
        site.coordinates = format!("ℓ:{l}°,𝑏:{b}°");
        self.generate_events(&mut site);
        // `generate_scan_conditions`: none (3), one (2) or two (1).
        let count = pick_weighted(&[(0, 3), (1, 2), (2, 1)], &mut self.rng).unwrap_or(0);
        let mut pool = ScanCondition::ALL.to_vec();
        for _ in 0..count {
            let i = pick_index(pool.len(), &mut self.rng);
            site.conditions.push(pool.remove(i));
        }
        if kind == SiteKind::TraderPost && !self.strings.trading_station_names.is_empty() {
            let i = pick_index(self.strings.trading_station_names.len(), &mut self.rng);
            let chosen = self.strings.trading_station_names[i].clone();
            site.name = format!("\"{chosen}\" trading station");
            site.description = site.description.replace("RANDOMIZED_NAME", &chosen);
        }
        site
    }

    /// `generate_events`: an adventure first, then weighted random events.
    fn generate_events(&mut self, site: &mut Site) {
        if let Some(event) = self.generate_adventure(&site.traits) {
            site.add_event(event);
        }
        let distance = site.distance;
        let weights = [
            (Category::Fluff, 20),
            (Category::Danger, 20),
            (Category::Trader, 10 + 3 * distance),
            (Category::Resource, 10 + 3 * distance),
        ];
        let min = (0.4 * f64::from(distance) + 0.2).ceil() as i64;
        let count = self.rng.range(min, min + 2);
        let defs = event_defs();
        for _ in 0..count {
            let Some(category) = pick_weighted(&weights, &mut self.rng) else {
                continue;
            };
            // Not tg: tg compares the event's blacklist with its own requirements instead of
            // the site's traits, so blacklisted events still show up.
            let viable: Vec<&EventDef> = defs
                .iter()
                .filter(|d| d.category == category)
                .filter(|d| d.required.iter().all(|t| site.traits.contains(t)))
                .filter(|d| !d.blacklisted.iter().any(|t| site.traits.contains(t)))
                .collect();
            if viable.is_empty() {
                continue;
            }
            let def = viable[pick_index(viable.len(), &mut self.rng)];
            let id = self.id();
            let event = instance(def, id, &self.strings, &mut self.rng);
            site.add_event(event);
        }
    }

    /// `generate_adventure`: a random unused adventure that fits the traits.
    fn generate_adventure(&mut self, traits: &[&'static str]) -> Option<Event> {
        let possible: Vec<usize> = (0..self.adventures.len())
            .filter(|&i| self.adventures[i].valid_for_use(traits))
            .collect();
        if possible.is_empty() {
            return None;
        }
        let chosen = possible[pick_index(possible.len(), &mut self.rng)];
        let adventure = self.adventures[chosen].create()?;
        self.adventures[chosen].placed = true;
        let id = self.id();
        Some(Event {
            id,
            name: "Something interesting".to_owned(),
            visited: false,
            band_values: adventure.band_modifiers.clone(),
            discovery_log: Some("Encountered something unexpected".to_owned()),
            point_scan_description: None,
            deep_scan_description: adventure.deep_scan_description.clone(),
            description: String::new(),
            action_text: String::new(),
            kind: EventKind::Adventure(Box::new(adventure)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_generation() {
        let mut ex = Exploration::new(Strings::default(), Vec::new(), 7);
        ex.generate_sites();
        assert_eq!(ex.wide_scan_band, 2);
        // Band 1: 1 + rand(0, 2) sites at distance 1..=3.
        assert!((1..=3).contains(&ex.sites.len()));
        for site in &ex.sites {
            assert!((1..=3).contains(&site.distance));
            assert!(!site.events.is_empty());
            assert_eq!(site.display_name(), "Anomaly");
            assert!(site.conditions.len() <= 2);
            for event in &site.events {
                if let EventKind::Danger(_) = event.kind {
                    assert!(event.name != "space carp attack" || !site.traits.contains(&CIVILIZED));
                }
            }
        }
    }

    #[test]
    fn events_respect_traits_and_bands() {
        let mut ex = Exploration::new(Strings::default(), Vec::new(), 9);
        for _ in 0..40 {
            let site = ex.new_site(SiteKind::Junkyard, 3);
            for event in &site.events {
                let def = event_defs()
                    .into_iter()
                    .find(|d| d.name == event.name)
                    .expect("defined event");
                assert!(def.required.iter().all(|t| site.traits.contains(t)));
            }
            let density: f64 = site
                .band_info
                .iter()
                .filter(|(b, _)| b == DENSITY)
                .map(|(_, v)| v)
                .sum();
            assert!(density >= 3.0);
        }
    }

    #[test]
    fn adventures_are_placed_once() {
        let json = r#"{"adventure_name": "x", "version": 1, "starting_node": "a",
            "required_site_traits": ["in space"],
            "nodes": [{"name": "a", "description": "", "choices": [
                {"key": "k", "name": "go", "exit_node": "WIN"}]}]}"#;
        let entry = AdventureEntry::parse("x", json).unwrap();
        let mut ex = Exploration::new(Strings::default(), vec![entry], 1);
        let first = ex.new_site(SiteKind::Junkyard, 1);
        assert!(matches!(first.events[0].kind, EventKind::Adventure(_)));
        let second = ex.new_site(SiteKind::Junkyard, 1);
        assert!(
            !second
                .events
                .iter()
                .any(|e| matches!(e.kind, EventKind::Adventure(_)))
        );
    }

    #[test]
    fn loot_is_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        let missing: Vec<&str> = runtime_types()
            .into_iter()
            .filter(|p| !registry.contains(p))
            .collect();
        assert!(
            missing.is_empty(),
            "not in ss13_mapload::runtime: {missing:?}"
        );
    }

    #[test]
    fn scan_costs_and_strings() {
        assert_eq!(wide_scan_cost(1, 1.0), 120.0);
        assert_eq!(wide_scan_cost(5, 1.0), 600.0);
        let s = Strings::parse(
            r#"{"probe_names": ["Juno"], "fluff_space": ["a"],
                "planet_types": [{"name": "p", "description": "d", "habitable": true}]}"#,
        );
        assert_eq!(s.probe_names, ["Juno"]);
        assert_eq!(s.fluff["fluff_space"], ["a"]);
        assert!(s.planet_types[0].2);
    }

    #[test]
    fn exploration_experiments_check_kind_condition_and_scan() {
        let mut ex = Exploration::new(Strings::default(), Vec::new(), 7);
        let mut site = ex.new_site(SiteKind::AsteroidBelt, 1);
        site.conditions = vec![ScanCondition::BlackHole];
        let belt = "/datum/exploration_site/asteroid_belt";
        let hole = "/datum/scan_condition/black_hole";
        assert!(!site.fits_exploration(belt, "", "point"));
        site.point_scan_complete = true;
        assert!(site.fits_exploration(belt, "", "point"));
        assert!(!site.fits_exploration("/datum/exploration_site/junkyard", "", "point"));
        assert!(!site.fits_exploration("", hole, "deep"));
        site.deep_scan_complete = true;
        assert!(site.fits_exploration("", hole, "deep"));
        assert!(!site.fits_exploration("", "/datum/scan_condition/nebula", "deep"));
    }
}
