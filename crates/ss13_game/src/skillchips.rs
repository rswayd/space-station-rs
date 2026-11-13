//! Skillchips (server), after tg `/obj/item/skillchip` (`code/modules/library/skill_learning/
//! skillchip.dm` and its generic and job chips), the brain's chip procs
//! (`code/modules/mob/living/brain/skillchip.dm`, `brain_item.dm`) and the carbon's
//! (`code/modules/mob/living/carbon/skillchip.dm`).
//!
//! - A chip is an item of a [`CHIPS`] type with [`Skillchip`]. Implanted, it is [`ChipIn`] its
//!   brain organ: off the map and not replicated; it leaves the body with the brain and is
//!   despawned with it. A brain holds chips up to [`MAX_SLOTS`] slots (`slot_use`); a chip is
//!   refused as a duplicate (without `SKILLCHIP_ALLOWS_MULTIPLE`) or as a second job chip; the
//!   active chips use at most [`MAX_COMPLEXITY`].
//! - Activating ([`try_activate`]) checks the cooldown and the complexity; an active chip's
//!   traits belong to its body ([`has_trait`]). Activating and deactivating show the chip's
//!   message and start its 5 minute cooldown; removal deactivates it and resets the cooldown.
//! - A job outfit's `skillchips` go in and activate silently at spawn ([`implant_outfit`]).
//! - A brain leaving its body deactivates its chips ("emergency power saving mode",
//!   [`brain_removed`]); a sharp-edged tool on a brain lying around excises its chips in 15 s
//!   (the removable ones drop, the rest are destroyed).
//! - Read here: the RCD's deconstruction, airlock access and full speed need
//!   [`ADVANCED_RCD_KNOWLEDGE`] (BandaStation, the engineer chip; `crate::devices::rcd`); the
//!   angler chip gives the fishing insights (`crate::skills::fishing_insight`); the suspicious
//!   chip hurts the brain by 20 when it activates.
//! - The skill station is `crate::skill_station`; the CNS connector implant and the Chip
//!   Connector and Chipped quirks use [`connector_click`], [`connector_emp`] and [`implant`].
// shortcut: the other chips' traits are granted but nothing reads them yet (balloon animals,
// detective's taste, cleanbots, minebots, supermatter, rod suplex, cyborg and engineering wire
// labels, self surgery, wine, hedges, bulbs, disk and ID appraisal, entrails, sabrage, brain
// washing, chef's kiss, remote tasting, drunken brawling, throwing arm); no chip actions (fishing
// tips, pointer colour), no acrobatics, taunt dodge, musical or pointer effects, no chef CQC
// (`under_siege`); no `skillchip_complexity_modifier` (the biotech compatibility mutation is not
// ported), no failsafe on a brain type change, no changeling chip cloning, no year in the
// musical chip's name; excision only of a brain on the floor.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::inventory::{CarriedBy, Inventory};
use crate::medical::organs::{self, Organ, OrganOf, Slot};
use crate::obj_damage::is_under;
use crate::storage::StoredIn;
use crate::tools::{Actor, now, tell_mob};

pub const ROOT: &str = "/obj/item/skillchip";
/// `/obj/item/organ/brain::max_skillchip_complexity`, `max_skillchip_slots`.
pub const MAX_COMPLEXITY: u32 = 3;
pub const MAX_SLOTS: u32 = 5;
/// `/obj/item/skillchip::cooldown` (5 minutes).
pub const COOLDOWN_SECS: f64 = 300.0;
/// The brain's excision `do_after` (15 seconds), deciseconds.
const EXCISE_DS: f32 = 150.0;
/// The connector's `do_after` (1.5 seconds), deciseconds.
const CONNECTOR_DS: f32 = 15.0;
/// `/obj/item/organ/cyberimp/brain/connector`.
pub const CONNECTOR: &str = "/obj/item/organ/cyberimp/brain/connector";
const BASKETWEAVING: &str = "/obj/item/skillchip/basketweaving";
const BRAINWASHING: &str = "/obj/item/skillchip/brainwashing";

/// BandaStation `TRAIT_ADVANCED_RCD_KNOWLEDGE` (the engineer chip).
pub const ADVANCED_RCD_KNOWLEDGE: &str = "advanced_rcd_knowledge";
/// The angler chip's `TRAIT_REVEAL_FISH`, `TRAIT_EXAMINE_FISHING_SPOT`, `TRAIT_EXAMINE_FISH`,
/// `TRAIT_EXAMINE_DEEPER_FISH`.
pub const REVEAL_FISH: &str = "reveal_fish";
pub const EXAMINE_FISHING_SPOT: &str = "examine_fishing_spot";
pub const EXAMINE_FISH: &str = "examine_fish";
pub const EXAMINE_DEEPER_FISH: &str = "examine_deeper_fish";

/// The static part of a chip type (tg vars).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChipDef {
    pub path: &'static str,
    /// `skill_name`, `skill_description`, `skill_icon` (Font Awesome name).
    pub skill_name: &'static str,
    pub desc: &'static str,
    pub icon: &'static str,
    /// `auto_traits`.
    pub traits: &'static [&'static str],
    /// `chip_category = SKILLCHIP_CATEGORY_JOB` with `SKILLCHIP_RESTRICTED_CATEGORIES` and the
    /// job category incompatible: one job chip per brain.
    pub job: bool,
    /// `SKILLCHIP_ALLOWS_MULTIPLE`.
    pub multiple: bool,
    pub complexity: u32,
    pub slot_use: u32,
    pub activate: (Span, &'static str),
    pub deactivate: (Span, &'static str),
}

const fn chip(
    path: &'static str,
    skill_name: &'static str,
    desc: &'static str,
    icon: &'static str,
    traits: &'static [&'static str],
    activate: &'static str,
    deactivate: &'static str,
) -> ChipDef {
    ChipDef {
        path,
        skill_name,
        desc,
        icon,
        traits,
        job: false,
        multiple: false,
        complexity: 1,
        slot_use: 1,
        activate: (Span::Notice, activate),
        deactivate: (Span::Notice, deactivate),
    }
}

/// `/obj/item/skillchip/job`: slot use 2, one per brain.
const fn job_chip(
    path: &'static str,
    skill_name: &'static str,
    desc: &'static str,
    icon: &'static str,
    traits: &'static [&'static str],
    activate: &'static str,
    deactivate: &'static str,
) -> ChipDef {
    ChipDef {
        job: true,
        slot_use: 2,
        ..chip(path, skill_name, desc, icon, traits, activate, deactivate)
    }
}

/// Every concrete chip type.
pub const CHIPS: [ChipDef; 30] = [
    chip(
        "/obj/item/skillchip/acrobatics",
        "Spinesthetics",
        "Allows you to flip and spin at an illegal and dangerous rate.",
        "wheelchair-alt",
        &[],
        "You suddenly have an extremely advanced and complex sense of how to spin and flip with grace.",
        "Your divine grasp of Spinesthesics disappears entirely.",
    ),
    ChipDef {
        activate: (Span::BoldWarning, "This was a mistake."),
        deactivate: (Span::BoldWarning, "The mistake is over."),
        ..chip(
            "/obj/item/skillchip/acrobatics/kiss",
            "ERROERERROROROEROEORROER",
            "NULL DESCRIPTION NOT FOUND",
            "kiss-beam",
            &[],
            "",
            "",
        )
    },
    chip(
        "/obj/item/skillchip/appraiser",
        "ID Appraisal",
        "Appraise an ID and see if it's issued from centcom, or just a cruddy station-printed one.",
        "magnifying-glass",
        &["id_appraiser"],
        "You feel that you can recognize special, minute details on ID cards.",
        "Was there something special about certain IDs?",
    ),
    chip(
        BASKETWEAVING,
        "Underwater Basketweaving",
        "Master intricate art of using twine to create perfect baskets while submerged.",
        "shopping-basket",
        &[],
        "You're one with the twine and the sea.",
        "Higher mysteries of underwater basketweaving leave your mind.",
    ),
    chip(
        "/obj/item/skillchip/big_pointer",
        "Enhanced pointing",
        "Learn to point at things in a more noticeable way.",
        "arrow-down",
        &[],
        "From \"The Definitive Compendium of Body Language for the Aspiring Leader\", page 164, \
         paragraph 3...",
        "So, uh, yeah, how do I point at things again?",
    ),
    chip(
        "/obj/item/skillchip/bonsai",
        "Hedgetrimming",
        "Trim hedges and potted plants into marvelous new shapes with any old knife. Not \
         applicable to plastic plants.",
        "spa",
        &["bonsai"],
        "Your mind is filled with plant arrangments.",
        "You can't remember what a hedge looks like anymore.",
    ),
    ChipDef {
        deactivate: (
            Span::Warning,
            "All knowledge of the secret brainwashing technique is GONE.",
        ),
        ..chip(
            BRAINWASHING,
            "Brainwashing",
            "WARNING: The integrity of this chip is compromised. Please discard this skillchip.",
            "soap",
            &["brainwashing"],
            "...But all at once it comes to you... something involving putting a brain in a \
             washing machine?",
            "",
        )
    },
    chip(
        "/obj/item/skillchip/chefs_kiss",
        "Chef's Kiss",
        "Allows you to kiss food you've created to make them with love.",
        "cookie",
        &["chefs_kiss"],
        "You recall learning from your grandmother how they baked their cookies with love.",
        "You forget all memories imparted upon you by your grandmother. Were they even your real \
         grandma?",
    ),
    chip(
        "/obj/item/skillchip/disk_verifier",
        "Nuclear Disk Verification",
        "Nuclear authentication disks have an extremely long serial number for verification. \
         This skillchip stores that number, which allows the user to automatically spot \
         forgeries.",
        "save",
        &["disk-verifier"],
        "You feel your mind automatically verifying long serial numbers on disk shaped objects.",
        "The innate recognition of absurdly long disk-related serial numbers fades from your mind.",
    ),
    chip(
        "/obj/item/skillchip/disposals",
        "Dauntless Disposaler",
        "You have an uncanny ability to perfectly land every toss into disposal units.",
        "trash-can",
        &["throwing_arm"],
        "You seem laser focused on the nearby disposal unit.",
        "The nearby disposal unit fades into the background of your vision.",
    ),
    chip(
        "/obj/item/skillchip/drunken_brawler",
        "Drunken Unarmed Proficiency",
        "When intoxicated, you gain increased unarmed effectiveness.",
        "wine-bottle",
        &["drunken brawler"],
        "You honestly could do with a drink. Never know when someone might try and jump you \
         around here.",
        "You suddenly feel a lot safer going around the station sober... ",
    ),
    chip(
        "/obj/item/skillchip/entrails_reader",
        "Entrails Reader",
        "Be able to learn about a person's life, by looking at their internal organs. Not to be \
         confused with looking into the future.",
        "lungs",
        &["entrails_reader"],
        "You feel that you know a lot about interpreting organs.",
        "Knowledge of liver damage, heart strain and lung scars fades from your mind.",
    ),
    chip(
        "/obj/item/skillchip/intj",
        "Mental Flavour Calculus",
        "When examining food, you can experience the flavours just as well as if you were eating \
         it.",
        "drumstick-bite",
        &["remote_tasting"],
        "You think of your favourite food and realise that you can rotate its flavour in your \
         mind.",
        "You feel your food-based mind palace crumbling...",
    ),
    job_chip(
        "/obj/item/skillchip/job/chef",
        "Close Quarters Cooking",
        "A specialised form of self defence, developed by skilled sous-chef de cuisines. No man \
         fights harder than a chef to defend his kitchen.",
        "utensils",
        &[],
        "You can visualize how to defend your kitchen with martial arts.",
        "You forget how to control your muscles to execute kicks, slams and restraints while in a \
         kitchen environment.",
    ),
    job_chip(
        "/obj/item/skillchip/job/clown",
        "Balloon Sutra",
        "Learn the ancient Honkmotherian arts of balloon-sutra.",
        "face-grin-tears",
        &["balloon_sutra"],
        "Blessed wisdom of Honkmother enwraps you, and with it, governship upon form of \
         balloonkind.",
        "'Remember, then, that true clownery requires freedom and willingness to bend, like ones \
         of a floating balloon.'... Whatever that meant?",
    ),
    job_chip(
        "/obj/item/skillchip/job/detectives_taste",
        "Detective's Taste",
        "Deduce the minute chemical compositions of any liquid substance just by swishing it \
         around your mouth for a bit.",
        "vial",
        &["detectives_taste"],
        "An explosion of flavors hit your mouth as you remember the secret tastebuds long \
         forgotten.",
        "Your mouth dulls to the hidden tastes of the world.",
    ),
    // BandaStation `skillchip_rcd`: `TRAIT_ADVANCED_RCD_KNOWLEDGE` added to the chip's traits.
    job_chip(
        "/obj/item/skillchip/job/engineer",
        "Engineering Circuitry",
        "Recognise airlock and APC wire layouts and understand their functionality at a glance.",
        "sitemap",
        &["know_engi_wires", ADVANCED_RCD_KNOWLEDGE],
        "You suddenly comprehend the secrets behind airlock and APC circuitry.",
        "Airlock and APC circuitry stops making sense as images of coloured wires fade from your \
         mind.",
    ),
    job_chip(
        "/obj/item/skillchip/job/janitor",
        "Voice Of The Voiceless",
        "Gain the affection of all thankless, hardworking cleanbots on the station.",
        "broom",
        &["cleanbot_whisperer"],
        "You start feeling empathetic towards all the cleanbots on the station.",
        "You forget why you felt any sympathy towards the cleanbots, they are just robots after \
         all.",
    ),
    job_chip(
        "/obj/item/skillchip/job/miner",
        "Battlebot enthusiast",
        "Lead minebots into war.",
        "brain",
        &["rock_stoner"],
        "A newfound obsession with battlebots fosters within you.",
        "You finally get over your battlebots phase, now go get a job.",
    ),
    job_chip(
        "/obj/item/skillchip/job/psychology",
        "Supermatter Cognition Theory",
        "Understand the correct mental patterns to keep in mind around matter in a hyperfractal \
         state, causing immunity to visions and making the matter in question \"calmer\".",
        "spa",
        &["supermatter_soother", "supermatter_madness_immune"],
        "You start thinking in patterns that will render you immune to visions from, and act as \
         a calming influence for, matter in a hyperfractal state.",
        "Your thoughts become more disordered and jumbled. You are no longer immune to the abyss.",
    ),
    job_chip(
        "/obj/item/skillchip/job/roboticist",
        "Cyborg Circuitry",
        "Recognise cyborg wire layouts and understand their functionality at a glance.",
        "sitemap",
        &["know_robo_wires"],
        "You suddenly comprehend the secrets behind cyborg circuitry.",
        "Cyborg circuitry stops making sense as images of coloured wires fade from your mind.",
    ),
    chip(
        "/obj/item/skillchip/light_remover",
        "Lightbulb Removing",
        "Stop failing taking out lightbulbs today, no gloves needed!",
        "lightbulb",
        &["lightbulb_remover"],
        "Your feel like your pain receptors are less sensitive to hot objects.",
        "You feel like hot objects could stop you again...",
    ),
    chip(
        "/obj/item/skillchip/master_angler",
        "Fisherman's Discernment",
        "Lists fishes when examining a fishing spot, gives a hint of whatever thing's biting the \
         hook and more.",
        "fish",
        &[
            REVEAL_FISH,
            EXAMINE_FISHING_SPOT,
            EXAMINE_FISH,
            EXAMINE_DEEPER_FISH,
        ],
        "You feel the knowledge and passion of several sunbaked, seasoned fishermen burn within \
         you.",
        "You no longer feel like casting a fishing rod by the sunny riverside.",
    ),
    chip(
        "/obj/item/skillchip/matrix_taunt",
        "Taunt 2 Dodge",
        "At the cost of stamina, your taunts can also be used to dodge incoming projectiles.",
        "spinner",
        &[],
        "You feel the urge to taunt scenically as if you are the 'Chosen One'.",
        "The urge to taunt goes away.",
    ),
    chip(
        "/obj/item/skillchip/musical",
        "Memory of a Musical",
        "Allows you to hit that high note, like those that came a century before us.",
        "music",
        &[],
        "You feel like you could \u{2669} sing a soooong! \u{266B}",
        "The musical fades from your mind, leaving you with a sense of nostalgia.",
    ),
    chip(
        "/obj/item/skillchip/research_director",
        "True Strength",
        "The knowledge and strength to resolve the most ancient conundrum: what happens when an \
         unstoppable force meets an immovable object.",
        "dumbbell",
        &["rod_suplex", "strength"],
        "You realise if you apply the correct force, at the correct angle, it is possible to make \
         the immovable permanently movable. And... damn, you look huge.",
        "You forget how to permanently anchor a paradoxical object. Also, you should really hit \
         the gym...",
    ),
    chip(
        "/obj/item/skillchip/sabrage",
        "Sabrage Proficiency",
        "Grants the user knowledge of the intricate structure of a champagne bottle's structural \
         weakness at the neck, improving their proficiency at being a show-off at officer \
         parties.",
        "bottle-droplet",
        &["sabrage_pro"],
        "You feel a new understanding of champagne bottles and methods on how to remove their \
         corks.",
        "The knowledge of the subtle physics residing inside champagne bottles fades from your \
         mind.",
    ),
    chip(
        "/obj/item/skillchip/self_surgery",
        "Self Surgery",
        "Allows you to perform surgery on yourself.",
        "user-doctor",
        &["self_surgery"],
        "You realize there's nothing stopping you from performing surgery on yourself.",
        "You suddenly feel like you should never perform surgery on yourself.",
    ),
    ChipDef {
        multiple: true,
        complexity: 0,
        slot_use: 0,
        ..chip(
            "/obj/item/skillchip/useless_adapter",
            "Useless adapter",
            "Allows you to insert another skillchip into this adapter after it has been inserted \
             into your brain...",
            "plug",
            &[],
            "You can now activate another chip through this adapter, but you're not sure why you \
             did this...",
            "You no longer have the useless skillchip adapter.",
        )
    },
    chip(
        "/obj/item/skillchip/wine_taster",
        "Wine Tasting",
        "Recognize wine vintage from taste alone. Never again lack an opinion when presented with \
         an unknown drink.",
        "wine-bottle",
        &["wine_taster"],
        "You recall wine taste.",
        "Your memories of wine evaporate.",
    ),
];

/// `GLOB.quirk_chipped_choice`: the chips the Chipped quirk may install.
pub const CHIPPED_CHOICES: [&str; 12] = [
    "/obj/item/skillchip/basketweaving",
    "/obj/item/skillchip/wine_taster",
    "/obj/item/skillchip/bonsai",
    "/obj/item/skillchip/useless_adapter",
    "/obj/item/skillchip/light_remover",
    "/obj/item/skillchip/entrails_reader",
    "/obj/item/skillchip/appraiser",
    "/obj/item/skillchip/sabrage",
    "/obj/item/skillchip/intj",
    "/obj/item/skillchip/musical",
    "/obj/item/skillchip/master_angler",
    "/obj/item/skillchip/big_pointer",
];

/// The chip type of `path` (exactly, or its nearest listed parent).
pub fn def_of(path: &str) -> Option<&'static ChipDef> {
    CHIPS
        .iter()
        .filter(|d| is_under(path, d.path))
        .max_by_key(|d| d.path.len())
}

/// A chip item (tg `/obj/item/skillchip` state).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Skillchip {
    pub def: &'static ChipDef,
    pub active: bool,
    /// `removable`: extracting it gives it back instead of destroying it.
    pub removable: bool,
    /// `chip_cooldown`: game seconds it ends at.
    pub cooldown_until: f64,
}

impl Skillchip {
    pub fn new(def: &'static ChipDef) -> Self {
        Self {
            def,
            active: false,
            removable: true,
            cooldown_until: 0.0,
        }
    }

    /// `is_on_cooldown`.
    pub fn on_cooldown(&self, now: f64) -> bool {
        now < self.cooldown_until
    }

    /// `COOLDOWN_TIMELEFT`, seconds.
    pub fn cooldown_left(&self, now: f64) -> f64 {
        (self.cooldown_until - now).max(0.0)
    }
}

/// On a chip inside a brain (tg `holding_brain`).
#[derive(Component, Debug)]
#[relationship(relationship_target = BrainChips)]
pub struct ChipIn(pub Entity);

/// A brain's chips (tg `skillchips`), in the order they went in. Despawning the brain despawns
/// them.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = ChipIn, linked_spawn)]
pub struct BrainChips(Vec<Entity>);

impl BrainChips {
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.iter().copied()
    }
}

pub struct SkillchipsPlugin;

impl Plugin for SkillchipsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `/obj/item/organ/brain/item_interaction`: a sharp-edged tool on a brain with chips.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2631, |w, c| {
            let Some(held) = c.held else {
                return false;
            };
            let actor = Actor {
                client: c.client,
                mob: c.mob,
                item: Some(held),
            };
            start_excision(w, actor, held, c.target)
        });
        app.add_observer(attach)
            .add_systems(Update, connector_answers);
    }
}

fn attach(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if !is_under(&item.path, ROOT) {
        return;
    }
    if let Some(def) = def_of(&item.path) {
        commands
            .entity(add.entity)
            .insert_if_new(Skillchip::new(def));
    }
}

/// The brain's chips.
pub fn chips(world: &World, brain: Entity) -> Vec<Entity> {
    world
        .get::<BrainChips>(brain)
        .map(|c| c.iter().collect())
        .unwrap_or_default()
}

/// The mob's brain (`get_organ_slot(ORGAN_SLOT_BRAIN)`).
pub fn brain_of(world: &World, mob: Entity) -> Option<Entity> {
    organs::organ(world, mob, Slot::Brain)
}

/// `get_used_skillchip_slots`.
pub fn used_slots(world: &World, brain: Entity) -> u32 {
    chips(world, brain)
        .into_iter()
        .filter_map(|c| world.get::<Skillchip>(c))
        .map(|c| c.def.slot_use)
        .sum()
}

/// `get_used_skillchip_complexity`: the active chips'.
pub fn used_complexity(world: &World, brain: Entity) -> u32 {
    chips(world, brain)
        .into_iter()
        .filter_map(|c| world.get::<Skillchip>(c))
        .filter(|c| c.active)
        .map(|c| c.def.complexity)
        .sum()
}

/// `get_max_skillchip_complexity`.
pub fn max_complexity(_world: &World, _brain: Entity) -> u32 {
    MAX_COMPLEXITY
}

/// `HAS_TRAIT(mob, trait)` from the active chips in its brain (`SKILLCHIP_TRAIT`).
pub fn has_trait(world: &World, mob: Entity, name: &str) -> bool {
    brain_of(world, mob).is_some_and(|brain| {
        chips(world, brain).into_iter().any(|c| {
            world
                .get::<Skillchip>(c)
                .is_some_and(|s| s.active && s.def.traits.contains(&name))
        })
    })
}

/// The body a chip's brain is in.
fn owner_of(world: &World, chip: Entity) -> Option<Entity> {
    let brain = world.get::<ChipIn>(chip)?.0;
    world.get::<OrganOf>(brain).map(|o| o.0)
}

/// `has_activate_incompatibility(brain)`: why `chip` cannot activate in `brain`.
pub fn activate_incompatibility(
    world: &World,
    brain: Option<Entity>,
    chip: Entity,
) -> Option<String> {
    let Some(brain) = brain.filter(|&b| world.get::<Organ>(b).is_some()) else {
        return Some("No brain detected.".to_owned());
    };
    let complexity = world.get::<Skillchip>(chip).map_or(0, |c| c.def.complexity);
    let max = max_complexity(world, brain);
    let new = used_complexity(world, brain) + complexity;
    (new > max).then(|| {
        format!("Skillchip is too complex to activate: {new} total out of {max} max complexity.")
    })
}

/// `has_skillchip_incompatibility(other)`: `def` against a chip already in the brain.
pub fn chip_incompatibility(def: &ChipDef, other: &ChipDef, other_name: &str) -> Option<String> {
    if !def.multiple && def.path == other.path {
        return Some(format!("Duplicate chip detected: {other_name}"));
    }
    if def.job && other.job {
        return Some(format!(
            "Incompatible with implanted job chip {other_name}."
        ));
    }
    None
}

/// `has_brain_incompatibility(brain)`: the free slots, then every chip already in.
pub fn brain_incompatibility(world: &World, brain: Entity, chip: Entity) -> Option<String> {
    let def = world.get::<Skillchip>(chip)?.def;
    let used = used_slots(world, brain);
    if used + def.slot_use > MAX_SLOTS {
        return Some(format!(
            "Not enough free slots. You have {} free and need {}.",
            MAX_SLOTS.saturating_sub(used),
            def.slot_use
        ));
    }
    chips(world, brain).into_iter().find_map(|other| {
        let other_def = world.get::<Skillchip>(other)?.def;
        chip_incompatibility(def, other_def, &crate::examine::name_of(world, other))
    })
}

/// `has_mob_incompatibility(target)`: a carbon with a brain the chip fits.
pub fn mob_incompatibility(world: &World, mob: Option<Entity>, chip: Entity) -> Option<String> {
    let Some(mob) = mob.filter(|&m| world.get::<organs::OrganHost>(m).is_some()) else {
        return Some("Incompatible lifeform detected.".to_owned());
    };
    let Some(brain) = brain_of(world, mob) else {
        return Some("No brain detected.".to_owned());
    };
    if let Some(reason) = brain_incompatibility(world, brain, chip) {
        return Some(reason);
    }
    // `/obj/item/skillchip/basketweaving/has_mob_incompatibility`.
    let path = world.get::<Skillchip>(chip).map_or("", |c| c.def.path);
    if path == BASKETWEAVING && world.get::<crate::mind::MindData>(mob).is_none() {
        return Some("Target incapable of learning recipe.".to_owned());
    }
    None
}

/// Takes `item` off wherever it is (a hand, a container, the floor) without placing it.
fn lift(world: &mut World, item: Entity) {
    if let Some(holder) = world.get::<CarriedBy>(item).map(|c| c.0) {
        if let Some(mut inventory) = world.get_mut::<Inventory>(holder) {
            for hand in inventory.hands.iter_mut() {
                if *hand == Some(item) {
                    *hand = None;
                }
            }
        }
        crate::inventory::refresh_look(world, holder);
    }
    world
        .entity_mut(item)
        .remove::<(CarriedBy, StoredIn, TilePos, Networked)>();
}

/// tg `implant_skillchip(chip, force)` of the carbon: into its brain (inactive). The reason it
/// was refused.
pub fn implant(world: &mut World, mob: Entity, chip: Entity, force: bool) -> Result<(), String> {
    let Some(brain) = brain_of(world, mob) else {
        return Err("Мозг не обнаружен.".to_owned());
    };
    if world.get::<Skillchip>(chip).is_none() {
        return Err("Incompatible lifeform detected.".to_owned());
    }
    if !force && let Some(reason) = mob_incompatibility(world, Some(mob), chip) {
        return Err(reason);
    }
    lift(world, chip);
    world.entity_mut(chip).insert(ChipIn(brain));
    Ok(())
}

/// `try_activate_skillchip(silent, force)`: the reason it did not activate.
pub fn try_activate(world: &mut World, chip: Entity, silent: bool, force: bool) -> Option<String> {
    let brain = world.get::<ChipIn>(chip).map(|c| c.0);
    if brain.is_none() {
        return Some("Skillchip cannot detect viable brain.".to_owned());
    }
    let Some(mob) = owner_of(world, chip) else {
        return Some("Skillchip cannot detect viable body.".to_owned());
    };
    let state = world.get::<Skillchip>(chip)?.clone();
    if !force {
        let t = now(world);
        if state.on_cooldown(t) {
            return Some(recharging(state.cooldown_left(t)));
        }
        if let Some(reason) = activate_incompatibility(world, brain, chip) {
            return Some(reason);
        }
    }
    on_activate(world, chip, mob, silent);
    None
}

fn recharging(secs: f64) -> String {
    format!(
        "Skillchip is still recharging for {}s",
        (secs * 10.0).round() / 10.0
    )
}

/// `on_activate`: the message, the traits (read live), the cooldown.
fn on_activate(world: &mut World, chip: Entity, mob: Entity, silent: bool) {
    let Some(def) = world.get::<Skillchip>(chip).map(|c| c.def) else {
        return;
    };
    // `/obj/item/skillchip/brainwashing/on_activate`, before the parent.
    if def.path == BRAINWASHING {
        tell_mob(
            world,
            mob,
            Span::Danger,
            "You get a pounding headache as the chip sends corrupt memories into your head!",
        );
        organs::adjust_organ_loss(world, mob, Slot::Brain, 20.0);
    }
    if !silent && !def.activate.1.is_empty() {
        tell_mob(world, mob, def.activate.0, def.activate.1);
    }
    let t = now(world);
    if let Some(mut c) = world.get_mut::<Skillchip>(chip) {
        c.active = true;
        c.cooldown_until = t + COOLDOWN_SECS;
    }
}

/// `try_deactivate_skillchip(silent, force, brain_owner)`: the reason it did not deactivate.
pub fn try_deactivate(
    world: &mut World,
    chip: Entity,
    silent: bool,
    force: bool,
    owner: Option<Entity>,
) -> Option<String> {
    let state = world.get::<Skillchip>(chip)?.clone();
    if !state.active {
        return Some("Skillchip is not active.".to_owned());
    }
    if world.get::<ChipIn>(chip).is_none() {
        return Some("Skillchip cannot detect viable brain.".to_owned());
    }
    let Some(mob) = owner.or_else(|| owner_of(world, chip)) else {
        if let Some(mut c) = world.get_mut::<Skillchip>(chip) {
            c.active = false;
        }
        return Some("Skillchip cannot detect viable body.".to_owned());
    };
    if !force {
        let t = now(world);
        if state.on_cooldown(t) {
            return Some(recharging(state.cooldown_left(t)));
        }
    }
    if !silent && !state.def.deactivate.1.is_empty() {
        tell_mob(world, mob, state.def.deactivate.0, state.def.deactivate.1);
    }
    let t = now(world);
    if let Some(mut c) = world.get_mut::<Skillchip>(chip) {
        c.active = false;
        c.cooldown_until = t + COOLDOWN_SECS;
    }
    None
}

/// tg `remove_skillchip` -> `on_removal`: out of its brain (nowhere yet, the caller places or
/// despawns it), deactivated, the cooldown reset. False when it was in no brain.
pub fn remove(world: &mut World, chip: Entity, silent: bool) -> bool {
    if world.get::<ChipIn>(chip).is_none() {
        return false;
    }
    if world.get::<Skillchip>(chip).is_some_and(|c| c.active) {
        try_deactivate(world, chip, silent, true, None);
    }
    if let Some(mut c) = world.get_mut::<Skillchip>(chip) {
        c.cooldown_until = 0.0;
    }
    world.entity_mut(chip).remove::<ChipIn>();
    true
}

/// A chip out of a brain into `mob`'s hands, else at its feet (`put_in_hands`).
pub fn give_to_hands(world: &mut World, mob: Entity, chip: Entity) {
    let hand = world.get::<Inventory>(mob).and_then(|i| {
        [i.active, 1 - i.active]
            .into_iter()
            .find(|&h| i.hands[h].is_none())
    });
    match hand {
        Some(h) => {
            world
                .entity_mut(chip)
                .remove::<(StoredIn, TilePos)>()
                .insert((Networked, CarriedBy(mob)));
            if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
                inventory.hands[h] = Some(chip);
            }
            crate::inventory::refresh_look(world, mob);
        }
        None => drop_at(world, chip, mob),
    }
}

/// A chip onto the tile of `at` (`forceMove(drop_location())`).
fn drop_at(world: &mut World, chip: Entity, at: Entity) {
    if let Some(pos) = crate::explosions::turf_of(world, at) {
        crate::storage::take_out(world, chip, pos);
    }
}

/// The end of tg `/datum/outfit/equip`: the outfit's chips go in and activate silently.
pub fn implant_outfit(world: &mut World, mob: Entity, chips: &[String]) {
    for path in chips {
        let Some(chip) = crate::construction::spawn_detached(world, path) else {
            warn!("no skillchip prototype {path}");
            continue;
        };
        if world.get::<Skillchip>(chip).is_none()
            && let Some(def) = def_of(path)
        {
            world.entity_mut(chip).insert(Skillchip::new(def));
        }
        if let Err(reason) = implant(world, mob, chip, false) {
            warn!("{mob}: skillchip {path} refused: {reason}");
            world.despawn(chip);
            continue;
        }
        try_activate(world, chip, true, true);
    }
}

/// `/obj/item/organ/brain/on_mob_remove`: not `special`, the chips deactivate.
pub fn brain_removed(world: &mut World, brain: Entity, mob: Entity, special: bool) {
    let chips = chips(world, brain);
    if special || chips.is_empty() {
        return;
    }
    tell_mob(
        world,
        mob,
        Span::Notice,
        "You feel your skillchips enable emergency power saving mode, deactivating as your brain \
         leaves your body...",
    );
    for chip in chips {
        try_deactivate(world, chip, false, true, Some(mob));
    }
}

/// The examine line of a brain with chips.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if world
        .get::<Organ>(target)
        .is_some_and(|o| o.slot == Slot::Brain)
        && !chips(world, target).is_empty()
    {
        lines.push((Span::Info, "It has a skillchip embedded in it.".to_owned()));
    }
}

/// `/obj/item/organ/brain/item_interaction`'s excision: a sharp-edged tool on a brain lying
/// around with chips. True when it started.
fn start_excision(world: &mut World, actor: Actor, held: Entity, target: Entity) -> bool {
    if world
        .get::<Organ>(target)
        .is_none_or(|o| o.slot != Slot::Brain)
        || world.get::<OrganOf>(target).is_some()
        || chips(world, target).is_empty()
    {
        return false;
    }
    let sharp = world
        .get::<ss13_mapload::DmWounding>(held)
        .is_some_and(|w| w.sharpness & crate::medical::wounds::SHARP_EDGED != 0);
    if !sharp {
        return false;
    }
    let name = crate::examine::name_of(world, target);
    tell_mob(
        world,
        actor.mob,
        Span::Notice,
        format!("You begin to excise skillchips from {name}."),
    );
    crate::tools::do_after(
        world,
        actor,
        target,
        EXCISE_DS,
        0,
        crate::construction::Job::Medical(crate::medical::MedJob::ExciseSkillchips),
    );
    true
}

/// The end of the excision: every chip out, the removable ones on the brain's tile.
pub fn finish_excision(world: &mut World, brain: Entity) {
    if world.get::<OrganOf>(brain).is_some() {
        return;
    }
    for chip in chips(world, brain) {
        remove(world, chip, false);
        if world.get::<Skillchip>(chip).is_some_and(|c| c.removable) {
            drop_at(world, chip, brain);
        } else {
            world.despawn(chip);
        }
    }
}

// --- The CNS skillchip connector (`/obj/item/organ/cyberimp/brain/connector`) ---

/// `/obj/item/organ/cyberimp/brain/connector/ui_action_click`: fiddling for 1.5 s.
pub fn connector_click(world: &mut World, client: Entity, mob: Entity, implant: Entity) {
    let name = crate::examine::name_of(world, implant);
    tell_mob(
        world,
        mob,
        Span::Warning,
        format!("You start fiddling around with {name}..."),
    );
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/items/taperecorder/tape_flip.ogg", 20.0, true),
        );
    }
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    if !crate::tools::do_after(
        world,
        actor,
        mob,
        CONNECTOR_DS,
        0,
        crate::construction::Job::Medical(crate::medical::MedJob::ChipConnector { implant }),
    ) {
        tell_mob(world, mob, Span::Warning, "You were interrupted!");
    }
}

/// The end of the connector's fiddling, checked again: the implant still in `mob`. A failing
/// connector drops a bit of brain; a held chip goes in and activates; an empty hand picks a
/// chip to take out.
pub fn finish_connector(world: &mut World, client: Entity, mob: Entity, implant: Entity) {
    if world.get::<OrganOf>(implant).is_none_or(|o| o.0 != mob) {
        return;
    }
    let t = now(world);
    if crate::medical::cyberimps::still_failing(world, implant, t) {
        let dropped = connector_remove_brain(world, mob, 1.0);
        if let Some(brain) = dropped {
            let (brain_name, name) = (
                crate::examine::name_of(world, brain),
                crate::examine::name_of(world, implant),
            );
            tell_mob(
                world,
                mob,
                Span::Warning,
                format!(
                    "You take {brain_name} out of {name}. You stare at it for a moment in \
                     confusion."
                ),
            );
        }
        return;
    }
    let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    match held {
        Some(item) if world.get::<Skillchip>(item).is_some() => connector_insert(world, mob, item),
        Some(item) => {
            let (what, name) = (
                crate::examine::name_of(world, item),
                crate::examine::name_of(world, implant),
            );
            tell_mob(
                world,
                mob,
                Span::Warning,
                format!("You try to insert {what} into {name}, but it won't fit!"),
            );
        }
        None => connector_pick(world, client, mob, implant),
    }
}

fn connector_sound(world: &mut World, mob: Entity, sound: &str, vol: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, vol, true));
    }
}

/// `insert_skillchip`: implanted and activated, or the reason with a buzz.
fn connector_insert(world: &mut World, mob: Entity, chip: Entity) {
    if let Err(reason) = implant(world, mob, chip, false) {
        tell_mob(world, mob, Span::Warning, reason);
        connector_sound(world, mob, "sound/machines/buzz/buzz-sigh.ogg", 10.0);
        return;
    }
    if let Some(reason) = try_activate(world, chip, false, false) {
        tell_mob(world, mob, Span::Warning, reason);
        connector_sound(world, mob, "sound/machines/buzz/buzz-two.ogg", 10.0);
        return;
    }
    connector_sound(world, mob, "sound/machines/chime.ogg", 10.0);
}

/// The chips offered by the connector's removal list (tg's radial).
#[derive(Component, Clone, Debug)]
struct ConnectorPrompt {
    implant: Entity,
    chips: Vec<Entity>,
}

/// `remove_skillchip(brain)`: a list of the brain's chips; empty, "Your brain is empty!".
fn connector_pick(world: &mut World, client: Entity, mob: Entity, implant: Entity) {
    let chips = brain_of(world, mob)
        .map(|b| chips(world, b))
        .unwrap_or_default();
    if chips.is_empty() {
        tell_mob(world, mob, Span::Warning, "Your brain is empty!");
        return;
    }
    let items = chips
        .iter()
        .map(|&c| crate::examine::name_of(world, c))
        .collect();
    let title = crate::examine::name_of(world, implant);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "Извлечь скиллчип",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(ConnectorPrompt { implant, chips });
}

/// The chip picked, checked again: the connector still in a conscious `mob`, the chip still in
/// its brain.
fn connector_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, mob, answer) in answers {
        let Some(ConnectorPrompt { implant, chips }) =
            world.get::<ConnectorPrompt>(prompt).cloned()
        else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        let Some(&chip) = chips.get(index) else {
            continue;
        };
        let able = world.get::<crate::health::Stat>(mob) == Some(&crate::health::Stat::Conscious);
        let brain = brain_of(world, mob);
        if !able
            || world.get::<OrganOf>(implant).is_none_or(|o| o.0 != mob)
            || brain.is_none()
            || world.get::<ChipIn>(chip).map(|c| c.0) != brain
        {
            continue;
        }
        remove(world, chip, false);
        give_to_hands(world, mob, chip);
        connector_sound(world, mob, "sound/machines/click.ogg", 10.0);
        let (what, name) = (
            crate::examine::name_of(world, chip),
            crate::examine::name_of(world, implant),
        );
        tell_mob(
            world,
            mob,
            Span::Warning,
            format!("You take {what} out of {name}."),
        );
    }
}

/// `/obj/item/organ/cyberimp/brain/connector/emp_act`: it fails until a reboot `90 / severity`
/// seconds later; once (light) or twice (heavy) a chip falls out behind the head, or without
/// chips a bit of brain. The caller marks it failing; this drops what falls.
pub fn connector_emp(world: &mut World, mob: Entity, severity: f64) {
    let loops = if severity >= 2.0 { 1 } else { 2 };
    for _ in 0..loops {
        let who = crate::examine::name_of(world, mob);
        crate::medical::visible(
            world,
            mob,
            "You feel something fall off from behind your head.",
            &format!("Something falls to the ground from behind {who}'s head."),
            Span::Warning,
        );
        let first = brain_of(world, mob).and_then(|b| chips(world, b).first().copied());
        match first {
            Some(chip) => {
                remove(world, chip, true);
                drop_at(world, chip, mob);
                connector_sound(
                    world,
                    mob,
                    "sound/machines/terminal/terminal_eject.ogg",
                    25.0,
                );
            }
            None => {
                connector_remove_brain(world, mob, if severity >= 2.0 { 1.0 } else { 2.0 });
            }
        }
    }
}

/// `remove_brain(brain, severity)`: 20 × severity brain damage; a brain that fails falls out
/// of the head. The brain when it fell.
// shortcut: no lowered brain `maxHealth` and no gibs on the floor.
fn connector_remove_brain(world: &mut World, mob: Entity, severity: f32) -> Option<Entity> {
    connector_sound(world, mob, "sound/effects/meatslap.ogg", 25.0);
    let brain = brain_of(world, mob)?;
    organs::adjust_organ_loss(world, mob, Slot::Brain, 20.0 * severity);
    if !world.get::<Organ>(brain).is_some_and(|o| o.failing) {
        return None;
    }
    organs::remove(world, brain, false);
    drop_at(world, brain, mob);
    let who = crate::examine::name_of(world, mob);
    crate::medical::visible(
        world,
        mob,
        "You feel like you're missing something.",
        &format!("{who}'s brain falls off the back of their head!!!"),
        Span::BoldWarning,
    );
    Some(brain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chip_has_a_runtime_type_and_resolves() {
        let runtime = ss13_mapload::runtime::runtime_types();
        for def in &CHIPS {
            assert!(
                runtime.contains(def.path),
                "{} is no runtime type",
                def.path
            );
            assert_eq!(def_of(def.path).map(|d| d.path), Some(def.path));
        }
        for path in CHIPPED_CHOICES {
            assert!(def_of(path).is_some_and(|d| d.path == path), "{path}");
        }
        assert_eq!(
            def_of("/obj/item/skillchip/acrobatics/kiss").map(|d| d.skill_name),
            Some("ERROERERROROROEROEORROER")
        );
        assert!(def_of(ROOT).is_none());
        assert!(def_of("/obj/item/skillchip/job").is_none());
    }

    #[test]
    fn chips_refuse_duplicates_and_second_job_chips() {
        let engineer = def_of("/obj/item/skillchip/job/engineer").unwrap();
        let clown = def_of("/obj/item/skillchip/job/clown").unwrap();
        let adapter = def_of("/obj/item/skillchip/useless_adapter").unwrap();
        let wine = def_of("/obj/item/skillchip/wine_taster").unwrap();
        assert!(
            chip_incompatibility(engineer, engineer, "chip")
                .unwrap()
                .starts_with("Duplicate")
        );
        assert!(
            chip_incompatibility(engineer, clown, "B@L00NY skillchip")
                .unwrap()
                .contains("job chip B@L00NY")
        );
        assert_eq!(chip_incompatibility(adapter, adapter, "a"), None);
        assert_eq!(chip_incompatibility(wine, engineer, "e"), None);
        assert_eq!((engineer.slot_use, engineer.complexity), (2, 1));
        assert_eq!((adapter.slot_use, adapter.complexity), (0, 0));
        assert!(engineer.traits.contains(&ADVANCED_RCD_KNOWLEDGE));
    }

    #[test]
    fn cooldown_counts_down() {
        let def = def_of("/obj/item/skillchip/bonsai").unwrap();
        let mut chip = Skillchip::new(def);
        assert!(!chip.on_cooldown(10.0));
        chip.cooldown_until = 10.0 + COOLDOWN_SECS;
        assert!(chip.on_cooldown(10.0));
        assert_eq!(chip.cooldown_left(110.0), 200.0);
        assert_eq!(recharging(12.34), "Skillchip is still recharging for 12.3s");
    }
}
