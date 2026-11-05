//! The registry of DM types the game spawns at runtime by a constant path (`new path(loc)` in
//! tg): `ss13-mapload convert` emits a prototype for every path in [`runtime_types`] whether or
//! not a converted map places it, so `ss13_game::construction::spawn_obj` and friends never
//! silently return `None` in the real game for a type only the fixtures declare.
//!
//! Data-driven spawns (outfits, uplink, cargo, research, vending, loot, ...) are covered by the
//! converter's closures over the dumped types, not here. Game code that spawns a constant path
//! takes the const from here (or from the module's own `*_RUNTIME_TYPES` table listed in
//! [`runtime_types`]); `ss13_game`'s `runtime_types` tests check that every constant spawn
//! argument and every spawn table of the game is in the registry, and
//! `tests/it/runtime_types.rs` checks the generated prototypes against it (`SS13_ASSETS`).

use std::collections::BTreeSet;

/// `/obj/item/assembly_holder`: two attached assemblies (`ss13_game::assemblies`).
pub const ASSEMBLY_HOLDER: &str = "/obj/item/assembly_holder";
/// The DNA console's printed activator (`ss13_game::genetics::console`).
pub const DNA_ACTIVATOR: &str = "/obj/item/dnainjector/activator";
/// What a clusterbuster splits into (`ss13_game::grenades`).
pub const CLUSTERBUSTER_SEGMENT: &str = "/obj/item/grenade/clusterbuster/segment";
/// The fragmentation grenades' shrapnel (`ss13_game::grenades`).
pub const SHRAPNEL: &str = "/obj/projectile/bullet/shrapnel";
pub const SHRAPNEL_MEGA: &str = "/obj/projectile/bullet/shrapnel/mega";
/// The smoke of smoke grenades (`ss13_game::grenades`).
pub const SMOKE_BAD: &str = "/obj/effect/particle_effect/fluid/smoke/bad";
/// A newscaster's printed newspaper (`ss13_game::newscaster_server`).
pub const NEWSPAPER: &str = "/obj/item/newspaper";
/// The uplink failsafe's item (`ss13_game::antag::uplink`).
pub const LOOT_TABLE_MAKER: &str = "/obj/item/loot_table_maker";
/// The cargo export scanner (`ss13_game::cargo::scanner`).
pub const UNIVERSAL_SCANNER: &str = "/obj/item/universal_scanner";

/// Constant paths `ss13_game` spawns that no other table of this crate lists: leftovers,
/// products and printouts of machines, decals, grenade payloads, quirk items, paper kinds.
pub const GAME_RUNTIME_TYPES: &[&str] = &[
    "/obj/item/food/urinalcake",
    "/obj/item/stack/sheet/cloth",
    "/obj/item/stack/sheet/plastic",
    "/obj/item/wallframe/urinal",
    SHRAPNEL,
    SHRAPNEL_MEGA,
    SMOKE_BAD,
    LOOT_TABLE_MAKER,
    UNIVERSAL_SCANNER,
    "/obj/effect/decal/cleanable/ants",
    "/obj/effect/decal/cleanable/ash",
    "/obj/effect/decal/cleanable/ash/large",
    "/obj/effect/decal/cleanable/blood",
    "/obj/effect/decal/cleanable/blood/drip",
    "/obj/effect/decal/cleanable/dirt",
    "/obj/effect/decal/cleanable/molten_object",
    ASSEMBLY_HOLDER,
    "/obj/item/broken_bottle",
    "/obj/item/circuitboard/machine/vendor",
    "/obj/item/coffee_cartridge",
    "/obj/item/dest_tagger",
    DNA_ACTIVATOR,
    "/obj/item/gibtonite",
    "/obj/item/graft",
    CLUSTERBUSTER_SEGMENT,
    NEWSPAPER,
    "/obj/item/paper/carbon",
    "/obj/item/paper/construction",
    "/obj/item/paper/fluff/jobs/cargo/manifest",
    "/obj/item/paper/natural",
    "/obj/item/plate/oven_tray",
    "/obj/item/rack_parts",
    "/obj/item/reagent_containers/cup/beaker/large",
    "/obj/item/reagent_containers/cup/coffeepot",
    "/obj/item/reagent_containers/cup/soup_pot",
    "/obj/item/stack/sheet/mineral/wood",
    "/obj/item/stack/telecrystal",
    "/obj/item/stock_parts/water_recycler",
    "/obj/item/storage/pill_bottle/mannitol/braintumor",
    "/obj/item/clothing/glasses/blindfold/white",
    "/obj/item/clothing/glasses/regular",
    "/obj/item/trash/can",
    "/obj/machinery/atmospherics/components/tank",
    "/obj/machinery/meter",
    "/obj/structure/closet/crate",
    "/obj/structure/rack",
];

/// `ss13_game::species`: every species' organs (`mutant*`), meat (`meat`) and the plasmaman
/// envirosuit (`pre_equip_species_outfit`).
pub const SPECIES_RUNTIME_TYPES: &[&str] = &[
    "/obj/item/clothing/gloves/color/plasmaman",
    "/obj/item/clothing/head/helmet/space/plasmaman",
    "/obj/item/clothing/under/plasmaman",
    "/obj/item/ectoplasm",
    "/obj/item/food/meat/slab/human",
    "/obj/item/food/meat/slab/human/mutant/ethereal",
    "/obj/item/food/meat/slab/human/mutant/fly",
    "/obj/item/food/meat/slab/human/mutant/golem",
    "/obj/item/food/meat/slab/human/mutant/lizard",
    "/obj/item/food/meat/slab/human/mutant/moth",
    "/obj/item/food/meat/slab/human/mutant/plant",
    "/obj/item/food/meat/slab/human/mutant/shadow",
    "/obj/item/food/meat/slab/human/mutant/skeleton",
    "/obj/item/food/meat/slab/human/mutant/slime",
    "/obj/item/food/meat/slab/monkey",
    "/obj/item/organ/appendix",
    "/obj/item/organ/appendix/fly",
    "/obj/item/organ/appendix/golem",
    "/obj/item/organ/appendix/pod",
    "/obj/item/organ/brain",
    "/obj/item/organ/brain/abductor",
    "/obj/item/organ/brain/cybernetic",
    "/obj/item/organ/brain/dullahan",
    "/obj/item/organ/brain/felinid",
    "/obj/item/organ/brain/ghost",
    "/obj/item/organ/brain/golem",
    "/obj/item/organ/brain/lizard",
    "/obj/item/organ/brain/lustrous",
    "/obj/item/organ/brain/pod",
    "/obj/item/organ/brain/primate",
    "/obj/item/organ/brain/primitive",
    "/obj/item/organ/brain/shadow",
    "/obj/item/organ/brain/skrell",
    "/obj/item/organ/brain/tajaran",
    "/obj/item/organ/brain/vulpkanin",
    "/obj/item/organ/ears",
    "/obj/item/organ/ears/cat",
    "/obj/item/organ/ears/cybernetic",
    "/obj/item/organ/ears/dullahan",
    "/obj/item/organ/ears/ghost",
    "/obj/item/organ/ears/pod",
    "/obj/item/organ/ears/tajaran",
    "/obj/item/organ/ears/vulpkanin",
    "/obj/item/organ/eyes",
    "/obj/item/organ/eyes/dullahan",
    "/obj/item/organ/eyes/felinid",
    "/obj/item/organ/eyes/fly",
    "/obj/item/organ/eyes/ghost",
    "/obj/item/organ/eyes/golem",
    "/obj/item/organ/eyes/jelly",
    "/obj/item/organ/eyes/lizard",
    "/obj/item/organ/eyes/moth",
    "/obj/item/organ/eyes/night_vision/mushroom",
    "/obj/item/organ/eyes/pod",
    "/obj/item/organ/eyes/robotic",
    "/obj/item/organ/eyes/shadow",
    "/obj/item/organ/eyes/skrell",
    "/obj/item/organ/eyes/snail",
    "/obj/item/organ/eyes/tajaran",
    "/obj/item/organ/eyes/vulpkanin",
    "/obj/item/organ/heart",
    "/obj/item/organ/heart/ethereal",
    "/obj/item/organ/heart/fly",
    "/obj/item/organ/heart/pod",
    "/obj/item/organ/heart/skrell",
    "/obj/item/organ/heart/tajaran",
    "/obj/item/organ/heart/vampire",
    "/obj/item/organ/heart/vulpkanin",
    "/obj/item/organ/liver",
    "/obj/item/organ/liver/bone",
    "/obj/item/organ/liver/bone/plasmaman",
    "/obj/item/organ/liver/fish",
    "/obj/item/organ/liver/fly",
    "/obj/item/organ/liver/ghost",
    "/obj/item/organ/liver/golem",
    "/obj/item/organ/liver/lizard",
    "/obj/item/organ/liver/pod",
    "/obj/item/organ/liver/skrell",
    "/obj/item/organ/liver/snail",
    "/obj/item/organ/liver/tajaran",
    "/obj/item/organ/liver/vulpkanin",
    "/obj/item/organ/lungs",
    "/obj/item/organ/lungs/ethereal",
    "/obj/item/organ/lungs/fish",
    "/obj/item/organ/lungs/fly",
    "/obj/item/organ/lungs/ghost",
    "/obj/item/organ/lungs/lavaland",
    "/obj/item/organ/lungs/plasmaman",
    "/obj/item/organ/lungs/pod",
    "/obj/item/organ/lungs/skrell",
    "/obj/item/organ/lungs/slime",
    "/obj/item/organ/lungs/tajaran",
    "/obj/item/organ/lungs/vulpkanin",
    "/obj/item/organ/stomach",
    "/obj/item/organ/stomach/bone",
    "/obj/item/organ/stomach/bone/plasmaman",
    "/obj/item/organ/stomach/ethereal",
    "/obj/item/organ/stomach/fish",
    "/obj/item/organ/stomach/fly",
    "/obj/item/organ/stomach/ghost",
    "/obj/item/organ/stomach/golem",
    "/obj/item/organ/stomach/moth",
    "/obj/item/organ/stomach/pod",
    "/obj/item/organ/stomach/skrell",
    "/obj/item/organ/stomach/tajaran",
    "/obj/item/organ/stomach/vulpkanin",
    "/obj/item/organ/tongue",
    "/obj/item/organ/tongue/abductor",
    "/obj/item/organ/tongue/bone",
    "/obj/item/organ/tongue/bone/plasmaman",
    "/obj/item/organ/tongue/cat",
    "/obj/item/organ/tongue/dullahan",
    "/obj/item/organ/tongue/ethereal",
    "/obj/item/organ/tongue/ethereal/lustrous",
    "/obj/item/organ/tongue/fish",
    "/obj/item/organ/tongue/fly",
    "/obj/item/organ/tongue/ghost",
    "/obj/item/organ/tongue/golem",
    "/obj/item/organ/tongue/jelly",
    "/obj/item/organ/tongue/lizard",
    "/obj/item/organ/tongue/lizard/silver",
    "/obj/item/organ/tongue/monkey",
    "/obj/item/organ/tongue/moth",
    "/obj/item/organ/tongue/mush",
    "/obj/item/organ/tongue/pod",
    "/obj/item/organ/tongue/robot",
    "/obj/item/organ/tongue/shadow",
    "/obj/item/organ/tongue/skrell",
    "/obj/item/organ/tongue/snail",
    "/obj/item/organ/tongue/tajaran",
    "/obj/item/organ/tongue/vulpkanin",
    "/obj/item/tank/internals/plasmaman/belt/full",
];

/// `ss13_game::medical::cyberimps`: arm toolkit tools (`items_to_create`) and the organs
/// autosurgeons start with (`starting_organ`).
pub const CYBERIMP_RUNTIME_TYPES: &[&str] = &[
    "/obj/item/assembly/flash/armimplant",
    "/obj/item/borg/stun",
    "/obj/item/cautery/augment",
    "/obj/item/cautery/cruel/augment",
    "/obj/item/circular_saw/augment",
    "/obj/item/circular_saw/cruel/augment",
    "/obj/item/clipboard",
    "/obj/item/crowbar/cyborg",
    "/obj/item/gun/energy/e_gun/advtaser/mounted",
    "/obj/item/gun/energy/laser/mounted/augment",
    "/obj/item/gun/energy/pulse/pistol/m1911",
    "/obj/item/gun/medbeam",
    "/obj/item/hemostat/augment",
    "/obj/item/hemostat/cruel/augment",
    "/obj/item/knife/combat/cyborg",
    "/obj/item/laser_pointer",
    "/obj/item/lighter",
    "/obj/item/melee/energy/blade/hardlight",
    "/obj/item/multitool/cyborg",
    "/obj/item/organ/cyberimp/arm/toolkit/gun/laser",
    "/obj/item/organ/cyberimp/arm/toolkit/surgery/emagged",
    "/obj/item/organ/cyberimp/eyes/hud/security/syndicate",
    "/obj/item/organ/eyes/robotic/thermals",
    "/obj/item/organ/eyes/robotic/xray",
    "/obj/item/pen/fountain",
    "/obj/item/retractor/augment",
    "/obj/item/retractor/cruel/augment",
    "/obj/item/scalpel/augment",
    "/obj/item/scalpel/cruel/augment",
    "/obj/item/screwdriver/cyborg",
    "/obj/item/stamp/mod/toolkit",
    "/obj/item/surgical_drapes",
    "/obj/item/surgicaldrill/augment",
    "/obj/item/surgicaldrill/cruel/augment",
    "/obj/item/taperecorder",
    "/obj/item/weldingtool/largetank/cyborg",
    "/obj/item/wirecutters/cyborg",
    "/obj/item/wrench/cyborg",
];

/// `ss13_game::events`: the vent clog's mobs and filth (`get_mob()`, `filth_spawn_types`) and
/// what meteors drop (`meteordrop`).
pub const EVENT_RUNTIME_TYPES: &[&str] = &[
    "/mob/living/basic/bear",
    "/mob/living/basic/bee",
    "/mob/living/basic/bee/toxin",
    "/mob/living/basic/butterfly",
    "/mob/living/basic/cockroach/bloodroach",
    "/mob/living/basic/cockroach/glockroach",
    "/mob/living/basic/cockroach/glockroach/mobroach",
    "/mob/living/basic/cockroach/hauberoach",
    "/mob/living/basic/goose",
    "/mob/living/basic/lightgeist",
    "/mob/living/basic/mothroach",
    "/mob/living/basic/mouse/rat",
    "/mob/living/basic/mushroom",
    "/mob/living/basic/pet/gondola",
    "/mob/living/basic/snail",
    "/mob/living/basic/spider/giant",
    "/mob/living/basic/spider/maintenance",
    "/mob/living/basic/viscerator",
    "/obj/effect/decal/cleanable/blood/splatter",
    "/obj/effect/decal/cleanable/blood/xeno",
    "/obj/effect/decal/cleanable/fuel_pool",
    "/obj/effect/decal/cleanable/greenglow",
    "/obj/effect/decal/cleanable/insectguts",
    "/obj/effect/decal/cleanable/vomit",
    "/obj/item/food/meat/slab/xeno",
    "/obj/item/organ/tongue/alien",
    "/obj/item/stack/ore/bananium",
    "/obj/item/stack/ore/bluespace_crystal",
    "/obj/item/stack/ore/glass",
    "/obj/item/stack/ore/iron",
    "/obj/item/stack/ore/plasma",
    "/obj/item/stack/ore/uranium",
];

/// Every DM type the game spawns by a constant path: the tables above and every
/// `*_RUNTIME_TYPES` table of this crate. `ss13-mapload convert` emits all of them.
pub fn runtime_types() -> BTreeSet<&'static str> {
    use crate::{antag, atmos_machine, cargo, mobs, service, silicon};
    let mut out = BTreeSet::new();
    let tables: [&[&'static str]; 24] = [
        &crate::RUNTIME_TYPES,
        &crate::RUNTIME_CONTAINER_TYPES,
        &crate::CONSTRUCTION_TYPES,
        &crate::MEDICAL_RUNTIME_TYPES,
        &crate::DISPOSAL_RUNTIME_TYPES,
        &crate::BOT_RUNTIME_TYPES,
        &crate::WALLMOUNT_RUNTIME_TYPES,
        &crate::FRAME_RUNTIME_TYPES,
        &antag::CHANGELING_RUNTIME_TYPES,
        &antag::CULT_RUNTIME_TYPES,
        &antag::HERETIC_RUNTIME_TYPES,
        &antag::WIZARD_RUNTIME_TYPES,
        &mobs::MOB_RUNTIME_TYPES,
        &silicon::CAMERA_RUNTIME_TYPES,
        &silicon::SILICON_TYPES,
        // The chaplain's gear, the null rod's forms and what rites make (`ss13_game::service`).
        &service::RELIGION_RUNTIME_TYPES,
        &service::NULLROD_VARIANTS,
        &service::HOLY_ARMOR_KITS,
        &service::MACHINE_BLESSINGS,
        // Supply crates' bounty cubes, mail and manifests.
        &cargo::RUNTIME_TYPES,
        GAME_RUNTIME_TYPES,
        SPECIES_RUNTIME_TYPES,
        CYBERIMP_RUNTIME_TYPES,
        EVENT_RUNTIME_TYPES,
    ];
    out.extend(tables.into_iter().flatten().copied());
    // A canister's `relabel` copies another canister type's name and look.
    out.extend(atmos_machine::CANISTER_LABELS.iter().map(|&(_, path)| path));
    // Cutting a stationary tank open leaves its frame; wrenching pipes, devices and meters off
    // leaves fittings; the pipe dispenser and RPD make every recipe's machine.
    out.extend([
        atmos_machine::TANK_FRAME,
        atmos_machine::PIPE_FITTING,
        atmos_machine::PIPE_METER_ITEM,
        atmos_machine::PIPE_DISPENSER,
        "/obj/item/pipe_dispenser",
    ]);
    for (_, recipes) in atmos_machine::ATMOS_PIPE_RECIPES {
        out.extend(recipes.iter().filter_map(|r| r.path));
    }
    // The crystallizer makes its recipes' products.
    for recipe in atmos_machine::CRYSTALLIZER_RECIPES {
        out.extend(recipe.products.iter().map(|&(path, _)| path));
    }
    out
}
