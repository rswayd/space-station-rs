//! The fish analyzer and the fish catalog (server), after tg `/obj/item/fish_analyzer`
//! (`code/modules/fishing/aquarium/fish_analyzer.dm`) and `/obj/item/book/manual/fish_catalog`
//! (`code/modules/fishing/fish_catalog.dm`), with the fish traits' names and catalog lines
//! (`code/modules/fishing/fish/fish_traits.dm`).
//!
//! - The analyzer used on a fish or an aquarium remembers it and opens `FishAnalyzer`: health,
//!   size, weight, hunger, food, temperature and fluid needs, traits. The window closes when
//!   the specimen is gone or out of view (7 tiles).
//! - The catalog used in hand opens `FishCatalog`: every catalogued fish type with its needs,
//!   where it is caught (the ported sources' tables), traits and difficulty.
// shortcut: no evolutions or breeding timers (not ported), no fish scanning experiments
// (experiment handler), no greyscale case colour; out of an aquarium a fish's temperature
// counts as suitable. The catalog has no favourite or disliked baits (no bait preferences in
// the fish data: "Unknown"), no beauty score, and lists only the ported fish sources' spots.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::fishing::{
    AQUARIUM_SOURCE, DmFish, FISH_DEFAULT_FOOD, FISH_FLAG_SHOW_IN_CATALOG, FISH_ROOT, FISH_SOURCES,
};
use ss13_mapload::{DmItem, DmName};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::ui::UiStatus;
use ss13_tgui::UiInterface as _;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiSystems, UiWindow};

use super::aquarium::{Aquarium, fluid_compatible};
use super::fish::{Fish, MAX_HEALTH, hunger, proper_environment};
use super::{
    AnalyzedFish, CatalogFish, FishAnalyzerData, FishAnalyzerUi, FishCatalogStatic, FishCatalogUi,
    FishTraitInfo,
};
use crate::storage::{Stored, StoredIn};

const ANALYZER: &str = "/obj/item/fish_analyzer";
const CATALOG: &str = "/obj/item/book/manual/fish_catalog";
/// `AQUARIUM_COMPANY`.
pub const AQUARIUM_COMPANY: &str = "Aquatech Ltd.";
/// The analyzer's view check (`view(7, get_turf(src))`).
const VIEW_RANGE: u32 = 7;

/// One `/datum/fish_trait`: `name`, `catalog_description`, `inheritability`,
/// `added_difficulty`.
pub struct FishTraitDef {
    pub path: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub inheritability: u32,
    pub added_difficulty: f32,
}

const TRAITS: [FishTraitDef; 33] = [
    FishTraitDef {
        path: "/datum/fish_trait/wary",
        name: "Wary",
        desc: "This fish will avoid visible fish lines, cloaked line recommended.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/shiny_lover",
        name: "Shiny Lover",
        desc: "This fish loves shiny things and money, shiny lure recommended.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/picky_eater",
        name: "Picky Eater",
        desc: "This fish is very picky and will ignore low quality bait (unless it's amongst its favorites).",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/nocturnal",
        name: "Nocturnal",
        desc: "This fish avoids bright lights, fishing and storing in darkness recommended.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/heavy",
        name: "Demersal",
        desc: "This fish tends to stay near the waterbed.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/carnivore",
        name: "Carnivore",
        desc: "This fish can only be baited with meat.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/vegan",
        name: "Herbivore",
        desc: "This fish can only be baited with fresh produce.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/emulsijack",
        name: "Emulsifier",
        desc: "This fish emits an invisible toxin that emulsifies other fish for it to feed on.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/emulsijack/psychic",
        name: "Psychic Aura",
        desc: "This fish emits an almost unblockable psychic aura that assaults minds, slowly killing all nearby fish and making humanoids have a bad time.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/necrophage",
        name: "Necrophage",
        desc: "This fish will eat carcasses of dead fish when hungry.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/parthenogenesis",
        name: "Parthenogenesis",
        desc: "This fish can reproduce asexually, without the need of a mate.",
        inheritability: 40,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/no_mating",
        name: "Mateless",
        desc: "This fish cannot reproduce with other fishes.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/recessive",
        name: "Recessive",
        desc: "If crossbred, offsprings will always be of the mate species, unless it also possess the trait.",
        inheritability: 0,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/revival",
        name: "Self-Revival",
        desc: "This fish shows a peculiar ability of reviving itself a minute or two after death.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/predator",
        name: "Predator",
        desc: "It's a predatory fish. It'll hunt down and eat live fishes of smaller size when hungry.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/yucky",
        name: "Yucky",
        desc: "This fish tastes so repulsive, other fishes won't try to eat it.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/toxic",
        name: "Toxic",
        desc: "This fish contains toxins. Feeding it to predatory fishes or people is not recommended.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/toxic/carpotoxin",
        name: "Carpotoxic",
        desc: "This fish contains carpotoxin. Definitely not safe for consumption.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/toxin_immunity",
        name: "Toxin Immunity",
        desc: "This fish has developed an ample-spected immunity to toxins.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/crossbreeder",
        name: "Crossbreeder",
        desc: "This fish's adaptive genetics allows it to crossbreed with other fish species.",
        inheritability: 40,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/territorial",
        name: "Territorial",
        desc: "This fish will start attacking other fish if the aquarium has five or more.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/lubed",
        name: "Lubed",
        desc: "This fish exudes a viscous, slippery lubrificant. It's recommended not to step on it.",
        inheritability: 50,
        added_difficulty: 5.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/amphibious",
        name: "Amphibious",
        desc: "This fish has developed a primitive adaptation to life on both land and water.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/mixotroph",
        name: "Mixotroph",
        desc: "This fish is capable of substaining itself by producing its own sources of energy (food).",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/antigrav",
        name: "Anti-Gravity",
        desc: "This fish will invert the gravity of the bait at random. May fall upward outside after being caught.",
        inheritability: 50,
        added_difficulty: 20.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/anxiety",
        name: "Anxiety",
        desc: "This fish tends to die of stress when forced to be around too many other fish.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/electrogenesis",
        name: "Electrogenesis",
        desc: "This fish is electroreceptive, and will generate electric fields. Can be harnessed inside a bioelectric generator.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/stunted",
        name: "Stunted Growth",
        desc: "This chrab's development is stunted, and will not properly reach adulthood.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/stinger",
        name: "Stinger",
        desc: "This fish is equipped with a sharp stringer or bill capable of delivering damage and toxins.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/toxic_barbs",
        name: "Toxic Barbs",
        desc: "The stinger or bill of this fish is coated in a simple, yet effective venom.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/hallucinogenic",
        name: "Hallucinogenic",
        desc: "This fish is coated with hallucinogenic neurotoxin. We advise cooking it before consumption.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/ink",
        name: "Ink Production",
        desc: "This fish possess a sac that produces ink.",
        inheritability: 50,
        added_difficulty: 0.0,
    },
    FishTraitDef {
        path: "/datum/fish_trait/camouflage",
        name: "Camouflage",
        desc: "This fish possess the ability to blend with its surroundings.",
        inheritability: 50,
        added_difficulty: 5.0,
    },
];

/// The trait `path` (`GLOB.fish_traits[path]`).
pub fn trait_def(path: &str) -> Option<&'static FishTraitDef> {
    TRAITS.iter().find(|t| t.path == path)
}

/// `FISH_FLAG_SHOW_IN_CATALOG`.
pub fn in_catalog(def: &DmFish) -> bool {
    def.fish_flags & FISH_FLAG_SHOW_IN_CATALOG != 0
}

/// On an analyzer: the scanned fish or aquarium and the last data.
#[derive(Component, Clone, Debug, Default)]
pub struct FishAnalyzer {
    pub scanned: Option<Entity>,
    pub data: FishAnalyzerData,
}

/// On a catalog that was opened: its static data (`var/static/fish_info`).
#[derive(Component, Clone, Debug, Default)]
pub struct FishCatalogBook(pub FishCatalogStatic);

pub(super) fn plugin(app: &mut App) {
    app.register_ui::<FishAnalyzerUi>()
        .register_ui::<FishCatalogUi>()
        .add_systems(Update, analyzer_windows.in_set(UiSystems::StatusHook));
}

fn item_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

fn reagent_name(world: &World, path: &str) -> String {
    world
        .get_resource::<crate::chem::Chem>()
        .and_then(|c| c.0.id(path).map(|id| c.0.def(id).name.clone()))
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).replace('_', " "))
}

/// `food`'s name for the analyzer and the catalog.
fn feed_name(world: &World, food: &str) -> String {
    if food == FISH_DEFAULT_FOOD {
        format!("{AQUARIUM_COMPANY} Fish Feed")
    } else {
        reagent_name(world, food)
    }
}

/// `interact_with_atom`: the analyzer on a fish or an aquarium.
pub fn analyze(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !item_path(world, held).is_some_and(|p| is_under(p, ANALYZER)) {
        return false;
    }
    if world.get::<Fish>(target).is_none() && world.get::<Aquarium>(target).is_none() {
        return false;
    }
    let data = analyzer_data(world, target);
    world.entity_mut(held).insert(FishAnalyzer {
        scanned: Some(target),
        data,
    });
    world.write_message(OpenUi::new::<FishAnalyzerUi>(
        client,
        mob,
        held,
        "fish analyzer",
    ));
    true
}

/// `ui_data`: the fish, or every fish in the aquarium.
pub fn analyzer_data(world: &World, scanned: Entity) -> FishAnalyzerData {
    if world.get::<Fish>(scanned).is_some() {
        return FishAnalyzerData {
            fish_scanned: true,
            fish_list: fish_info(world, scanned).into_iter().collect(),
        };
    }
    FishAnalyzerData {
        fish_scanned: false,
        fish_list: world
            .get::<Stored>(scanned)
            .map(|s| s.items().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| fish_info(world, e))
            .collect(),
    }
}

/// `extract_fish_info`.
fn fish_info(world: &World, e: Entity) -> Option<AnalyzedFish> {
    let fish = world.get::<Fish>(e)?;
    let def = world.get::<DmFish>(e)?;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let aquarium = world
        .get::<StoredIn>(e)
        .and_then(|s| world.get::<Aquarium>(s.0));
    let (fluid_ok, temp_ok) = match aquarium {
        Some(a) => (
            fluid_compatible(&def.required_fluid_type, &a.fluid),
            (def.required_temperature_min..=def.required_temperature_max).contains(&a.temperature),
        ),
        None => (proper_environment(def, None), true),
    };
    Some(AnalyzedFish {
        fish_name: crate::chem::name_of(world, e),
        fish_food: feed_name(world, &def.food),
        fish_min_temp: def.required_temperature_min,
        fish_max_temp: def.required_temperature_max,
        fish_fluid_compatible: fluid_ok,
        fish_fluid_type: def.required_fluid_type.clone(),
        fish_traits: def
            .fish_traits
            .iter()
            .filter_map(|t| trait_def(t))
            .map(|t| FishTraitInfo {
                trait_name: t.name.to_owned(),
                trait_desc: t.desc.to_owned(),
                trait_inherit: t.inheritability,
            })
            .collect(),
        fish_suitable_temp: temp_ok,
        fish_health: if fish.alive {
            (fish.health / MAX_HEALTH * 100.0).round()
        } else {
            0.0
        },
        fish_size: fish.size,
        fish_weight: fish.weight,
        fish_hunger: (1.0 - hunger(fish, def, now, 1.0)) as f32,
    })
}

/// `ui_status` + the autoupdate: the window closes when the specimen is gone or out of view
/// of the user; otherwise its data follows the specimen.
fn analyzer_windows(world: &mut World) {
    let windows: Vec<(Entity, Entity, Entity)> = world
        .query::<(Entity, &UiWindow)>()
        .iter(world)
        .filter(|(_, w)| w.interface == FishAnalyzerUi::NAME)
        .map(|(e, w)| (e, w.actor, w.source))
        .collect();
    for (window, actor, analyzer) in windows {
        let scanned = world.get::<FishAnalyzer>(analyzer).and_then(|a| a.scanned);
        let in_view = scanned.is_some_and(|s| {
            world.get_entity(s).is_ok()
                && match (
                    world.get::<TilePos>(actor).copied(),
                    crate::chem::map_pos(world, s),
                ) {
                    (Some(a), Some(b)) => a.chebyshev(b).is_some_and(|d| d <= VIEW_RANGE),
                    _ => false,
                }
        });
        if !in_view {
            if let Some(mut a) = world.get_mut::<FishAnalyzer>(analyzer) {
                a.scanned = None;
            }
            if let Some(mut w) = world.get_mut::<UiWindow>(window) {
                w.cap_status(UiStatus::Close);
            }
            if let Some(client) = world.get::<UiWindow>(window).map(|w| w.client) {
                crate::tools::tell(world, client, Span::Warning, "specimen data lost!");
            }
            continue;
        }
        let data = analyzer_data(world, scanned.expect("in view"));
        if let Some(mut a) = world.get_mut::<FishAnalyzer>(analyzer)
            && a.data != data
        {
            a.data = data;
        }
    }
}

impl ServerUi for FishAnalyzerUi {
    type Source = &'static FishAnalyzer;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &FishAnalyzer) {}

    fn data(analyzer: &FishAnalyzer, _: Entity, _: &()) -> FishAnalyzerData {
        analyzer.data.clone()
    }
}

impl ServerUi for FishCatalogUi {
    type Source = &'static FishCatalogBook;
    type UserParam = ();
    type Act = ();

    fn static_data(book: &FishCatalogBook) -> FishCatalogStatic {
        book.0.clone()
    }

    fn data(_: &FishCatalogBook, _: Entity, _: &()) {}
}

/// The catalog used in hand opens its window.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !item_path(world, item).is_some_and(|p| is_under(p, CATALOG)) {
        return false;
    }
    if world.get::<FishCatalogBook>(item).is_none() {
        let data = catalog_data(world);
        world.entity_mut(item).insert(FishCatalogBook(data));
    }
    let title = world
        .get::<DmName>(item)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<FishCatalogUi>(client, mob, item, title));
    true
}

/// `ui_static_data` of the catalog: every catalogued fish type the server knows.
pub fn catalog_data(world: &World) -> FishCatalogStatic {
    let Some(protos) = world.get_resource::<ss13_proto::Prototypes>() else {
        return FishCatalogStatic::default();
    };
    let mut paths: Vec<&str> = protos
        .ids()
        .filter(|id| is_under(id, FISH_ROOT) && *id != FISH_ROOT)
        .collect();
    paths.sort_unstable();
    let fish_info = paths
        .into_iter()
        .filter_map(|path| {
            let proto = protos.get(path)?;
            let def = proto.get::<DmFish>()?;
            if !in_catalog(def) {
                return None;
            }
            let name = proto.get::<DmName>();
            Some(CatalogFish {
                name: name.map(|n| n.name.clone()).unwrap_or_default(),
                desc: name.map(|n| n.desc.clone()).unwrap_or_default(),
                fluid: def.required_fluid_type.clone(),
                temp_min: def.required_temperature_min,
                temp_max: def.required_temperature_max,
                source: if def.random_case_rarity > 0 {
                    format!("{AQUARIUM_COMPANY} Fish Packs")
                } else {
                    "Unknown".to_owned()
                },
                size: def.average_size,
                weight: def.average_weight,
                feed: feed_name(world, &def.food),
                spots: spots_of(path),
                favorite_bait: "Unknown".to_owned(),
                disliked_bait: "Unknown".to_owned(),
                traits: trait_lines(&def.fish_traits),
                difficulty: difficulty_word(catalog_difficulty(def)).to_owned(),
            })
        })
        .collect();
    FishCatalogStatic {
        fish_info,
        sponsored_by: AQUARIUM_COMPANY.to_owned(),
    }
}

/// `catalog_description` of the ported sources.
fn source_description(path: &str) -> Option<&'static str> {
    Some(match path {
        "/datum/fish_source/ocean/beach" => "Beach shore water",
        "/datum/fish_source/ice_fishing" => "Ice-covered water",
        "/datum/fish_source/river" => "River water",
        "/datum/fish_source/hot_spring" => "Hot Springs",
        "/datum/fish_source/tizira" => "Tiziran Sea",
        AQUARIUM_SOURCE => "Aquariums",
        "/datum/fish_source/portal" => "Aquarium dimension (Fishing portal generator)",
        "/datum/fish_source/portal/syndicate" => "Syndicate dimension (Fishing portal generator)",
        _ => return None,
    })
}

/// `build_fishing_tips`' `spots`: `english_list` with "Unknown" for none.
fn spots_of(fish: &str) -> String {
    let spots: Vec<&str> = FISH_SOURCES
        .iter()
        .filter(|s| s.table.iter().any(|&(p, _)| p == fish))
        .filter_map(|s| source_description(s.path))
        .collect();
    english_list(&spots, "Unknown")
}

fn english_list(items: &[&str], nothing: &str) -> String {
    match items {
        [] => nothing.to_owned(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn trait_lines(traits: &[String]) -> Vec<String> {
    let lines: Vec<String> = traits
        .iter()
        .filter_map(|t| trait_def(t))
        .map(|t| t.desc.to_owned())
        .collect();
    if lines.is_empty() {
        vec!["This fish exhibits no special behavior.".to_owned()]
    } else {
        lines
    }
}

/// The fish's `fishing_difficulty_modifier` plus its traits' `added_difficulty`.
pub fn catalog_difficulty(def: &DmFish) -> f32 {
    def.fishing_difficulty_modifier
        + def
            .fish_traits
            .iter()
            .filter_map(|t| trait_def(t))
            .map(|t| t.added_difficulty)
            .sum::<f32>()
}

/// The catalog's difficulty word: below 10 easy, below 20 medium.
pub fn difficulty_word(difficulty: f32) -> &'static str {
    if difficulty < 10.0 {
        "Easy"
    } else if difficulty < 20.0 {
        "Medium"
    } else {
        "Hard"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_words_and_traits() {
        assert_eq!(difficulty_word(9.0), "Easy");
        assert_eq!(difficulty_word(10.0), "Medium");
        assert_eq!(difficulty_word(25.0), "Hard");
        let def = DmFish {
            fishing_difficulty_modifier: 5.0,
            fish_traits: vec!["/datum/fish_trait/heavy".to_owned()],
            ..default()
        };
        assert_eq!(
            trait_def("/datum/fish_trait/heavy").unwrap().name,
            "Demersal"
        );
        assert_eq!(
            trait_lines(&[]),
            vec!["This fish exhibits no special behavior."]
        );
        assert!(catalog_difficulty(&def) >= 5.0);
        assert_eq!(spots_of("/obj/item/fish/pike"), "River water");
        assert_eq!(english_list(&["a", "b", "c"], "x"), "a, b and c");
    }
}
