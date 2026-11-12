//! Antagonist data from the DM dump, written by `ss13-mapload convert` to [`ANTAG_FILE`]: the
//! uplink catalog (`/datum/uplink_item`, `/datum/uplink_category`) and the steal objective
//! targets (`/datum/objective_item/steal`), plus the traitor flavor texts and the phonetic
//! alphabet from `strings/`. The game (`ss13_game::antag`) reads it at startup.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;

/// `<assets>/antag.ron`.
pub const ANTAG_FILE: &str = "antag.ron";

/// Items a changeling grows at runtime (`/datum/action/changeling/weapon/arm_blade`,
/// `/datum/action/changeling/suit/armor`).
pub const CHANGELING_RUNTIME_TYPES: [&str; 4] = [
    "/obj/item/melee/arm_blade",
    "/obj/item/melee/arm_blade/false",
    "/obj/item/clothing/suit/armor/changeling",
    "/obj/item/clothing/head/helmet/changeling",
];

/// What the blood cult makes at runtime (`ss13_game::antag::cult`): its runes, the ritual dagger
/// and runed metal, the blood magic auras, the shadow shackles, the barrier, Nar'Sie, the
/// structures' items and the pylon's floor.
pub const CULT_RUNTIME_TYPES: [&str; 36] = [
    "/obj/effect/rune/convert",
    "/obj/effect/rune/empower",
    "/obj/effect/rune/teleport",
    "/obj/effect/rune/raise_dead",
    "/obj/effect/rune/wall",
    "/obj/effect/rune/summon",
    "/obj/effect/rune/narsie",
    "/obj/item/melee/cultblade/dagger",
    "/obj/item/stack/sheet/runed_metal",
    "/obj/item/stack/sheet/runed_metal/ten",
    "/obj/item/melee/blood_magic/stun",
    "/obj/item/melee/blood_magic/teleport",
    "/obj/item/melee/blood_magic/shackles",
    "/obj/item/melee/blood_magic/construction",
    "/obj/item/restraints/handcuffs/cult",
    "/obj/structure/emergency_shield/cult/barrier",
    "/obj/narsie",
    // What the altar, forge and archives make, and the pylon's floor.
    "/obj/item/sharpener/cult",
    "/obj/structure/constructshell",
    "/obj/item/reagent_containers/cup/beaker/unholywater",
    "/obj/item/clothing/suit/hooded/cultrobes/hardened",
    "/obj/item/melee/cultblade",
    "/obj/item/clothing/glasses/hud/health/night/cultblind",
    "/obj/item/shuttle_curse",
    "/obj/item/cult_shift",
    "/turf/open/floor/engine/cult",
    // Soulstones' shades and the constructs of shells (`ss13_game::antag::cult::server::soulstone`).
    "/mob/living/basic/shade",
    "/mob/living/basic/construct/juggernaut",
    "/mob/living/basic/construct/juggernaut/angelic",
    "/mob/living/basic/construct/wraith",
    "/mob/living/basic/construct/wraith/angelic",
    "/mob/living/basic/construct/wraith/mystic",
    "/mob/living/basic/construct/artificer",
    "/mob/living/basic/construct/artificer/noncult",
    "/mob/living/basic/construct/artificer/angelic",
    "/mob/living/basic/construct/artificer/mystic",
];

/// What the heretic makes at runtime (`ss13_game::antag::heretic`): influences, the rune, the
/// Mansus grasp's hand and every ritual's results (`result_atoms`).
pub const HERETIC_RUNTIME_TYPES: [&str; 82] = [
    "/obj/effect/heretic_influence",
    "/obj/effect/visible_heretic_influence",
    "/obj/effect/heretic_rune/big",
    "/obj/item/melee/touch_attack/mansus_fist",
    "/obj/item/ammo_box/speedloader/strilka310/lionhunter",
    "/obj/item/canvas",
    "/obj/item/card/id/advanced/heretic",
    "/obj/item/clothing/mask/madness_mask",
    "/obj/item/clothing/neck/eldritch_amulet",
    "/obj/item/clothing/neck/heretic_focus",
    "/obj/item/clothing/neck/moon_amulet",
    "/obj/item/clothing/shoes/greaves_of_the_prophet",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/ash",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/blade",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/cosmic",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/flesh",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/lock",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/moon",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/rust",
    "/obj/item/clothing/suit/hooded/cultrobes/eldritch/void",
    "/obj/item/clothing/suit/hooded/cultrobes/void",
    "/obj/item/codex_cicatrix",
    "/obj/item/codex_cicatrix/morbus",
    "/obj/item/coin/eldritch",
    "/obj/item/ether",
    "/obj/item/flashlight/lantern/heretic",
    "/obj/item/food/grown/flower/poppy/blessed",
    "/obj/item/grenade/chem_grenade/rust_sower",
    "/obj/item/gun/ballistic/rifle/lionhunter",
    "/obj/item/heretic_labyrinth_handbook",
    "/obj/item/knife/butcher/heretic",
    "/obj/item/melee/rune_carver",
    "/obj/item/melee/sickly_blade/ash",
    "/obj/item/melee/sickly_blade/cosmic",
    "/obj/item/melee/sickly_blade/dark",
    "/obj/item/melee/sickly_blade/flesh",
    "/obj/item/melee/sickly_blade/lock",
    "/obj/item/melee/sickly_blade/moon",
    "/obj/item/melee/sickly_blade/rust",
    "/obj/item/melee/sickly_blade/void",
    "/obj/item/reagent_containers/cup/beaker/eldritch",
    "/obj/item/reagent_containers/cup/phylactery",
    "/obj/item/storage/belt/unfathomable_curio",
    "/obj/item/void_prison",
    "/obj/item/wallframe/mirror/heretic",
    "/obj/structure/destructible/eldritch_crucible",
    // Summons, foci, brews, paintings and what paintings give (`ss13_game::antag::heretic`).
    "/mob/living/basic/heretic_summon/raw_prophet",
    "/mob/living/basic/heretic_summon/stalker",
    "/mob/living/basic/heretic_summon/rust_walker",
    "/mob/living/basic/heretic_summon/maid_in_the_mirror",
    "/mob/living/basic/heretic_summon/ash_spirit",
    "/mob/living/basic/heretic_summon/fire_shark",
    "/obj/item/clothing/neck/heretic_focus/crimson_medallion",
    "/obj/item/clothing/neck/eldritch_amulet/piercing",
    "/obj/item/eldritch_potion/crucible_soul",
    "/obj/item/eldritch_potion/duskndawn",
    "/obj/item/eldritch_potion/wounded",
    "/obj/effect/gibspawner/generic",
    "/obj/item/wallframe/painting/eldritch/weeping",
    "/obj/structure/sign/painting/eldritch/weeping",
    "/obj/item/wallframe/painting/eldritch/desire",
    "/obj/structure/sign/painting/eldritch/desire",
    "/obj/item/wallframe/painting/eldritch/vines",
    "/obj/structure/sign/painting/eldritch/vines",
    "/obj/item/wallframe/painting/eldritch/beauty",
    "/obj/structure/sign/painting/eldritch/beauty",
    "/obj/item/wallframe/painting/eldritch/rust",
    "/obj/structure/sign/painting/eldritch/rust",
    "/obj/item/organ/brain",
    "/obj/item/organ/lungs",
    "/obj/item/organ/eyes",
    "/obj/item/organ/ears",
    "/obj/item/organ/heart",
    "/obj/item/organ/liver",
    "/obj/item/organ/stomach",
    "/obj/item/organ/appendix",
    "/obj/item/bodypart/arm/left",
    "/obj/item/bodypart/arm/right",
    "/obj/item/bodypart/leg/left",
    "/obj/item/bodypart/leg/right",
    "/obj/item/food/grown/flower/poppy",
    "/obj/item/food/grown/flower/harebell",
];

/// tg `UPLINK_TRAITORS`.
pub const UPLINK_TRAITORS: u32 = 1 << 0;
/// tg `UPLINK_NUKE_OPS`.
pub const UPLINK_NUKE_OPS: u32 = 1 << 1;
/// tg `UPLINK_CLOWN_OPS`.
pub const UPLINK_CLOWN_OPS: u32 = 1 << 2;
/// tg `UPLINK_LONE_OP`.
pub const UPLINK_LONE_OP: u32 = 1 << 4;
/// tg `UPLINK_ALL_SYNDIE_OPS`.
pub const UPLINK_ALL_SYNDIE_OPS: u32 = UPLINK_NUKE_OPS | UPLINK_LONE_OP | UPLINK_CLOWN_OPS;

/// The nuclear operatives' outfits (`/datum/antagonist/nukeop` `nukeop_outfit`): the
/// operative's, the leader's, the lone operative's, the clown operatives' (+ leader,
/// reinforcement) and the reinforcements' (`/obj/item/antag_spawner/nuke_ops` picks a subtype).
pub const NUKEOP_OUTFITS: [&str; 12] = [
    "/datum/outfit/syndicate",
    "/datum/outfit/syndicate/leader",
    "/datum/outfit/syndicate/full/loneop",
    "/datum/outfit/syndicate/clownop",
    "/datum/outfit/syndicate/clownop/leader",
    "/datum/outfit/syndicate/clownop/no_crystals",
    "/datum/outfit/syndicate/reinforcement/gorlex",
    "/datum/outfit/syndicate/reinforcement/cybersun",
    "/datum/outfit/syndicate/reinforcement/donk",
    "/datum/outfit/syndicate/reinforcement/waffle",
    "/datum/outfit/syndicate/reinforcement/interdyne",
    "/datum/outfit/syndicate/reinforcement/mi13",
];

/// What zombies make at runtime (`ss13_game::antag::zombie`): the infection organ and the claws.
pub const ZOMBIE_RUNTIME_TYPES: [&str; 4] = [
    "/obj/item/organ/zombie_infection",
    "/obj/item/organ/zombie_infection/nodamage",
    "/obj/item/mutant_hand/zombie",
    "/obj/item/mutant_hand/zombie/weak",
];

/// The emergency response teams' outfits (`/datum/antagonist/ert` `outfit`, BandaStation's
/// `ert_outfits.dm`), every role of `ss13_game::antag::ert`.
pub const ERT_OUTFITS: [&str; 56] = [
    "/datum/outfit/centcom/ert/security",
    "/datum/outfit/centcom/ert/security/amber",
    "/datum/outfit/centcom/ert/security/red",
    "/datum/outfit/centcom/ert/security/gamma",
    "/datum/outfit/centcom/ert/security/inquisitor",
    "/datum/outfit/centcom/ert/security/party",
    "/datum/outfit/centcom/ert/engineer",
    "/datum/outfit/centcom/ert/engineer/amber",
    "/datum/outfit/centcom/ert/engineer/red",
    "/datum/outfit/centcom/ert/engineer/gamma",
    "/datum/outfit/centcom/ert/engineer/party",
    "/datum/outfit/centcom/ert/medic",
    "/datum/outfit/centcom/ert/medic/amber",
    "/datum/outfit/centcom/ert/medic/red",
    "/datum/outfit/centcom/ert/medic/gamma",
    "/datum/outfit/centcom/ert/medic/inquisitor",
    "/datum/outfit/centcom/ert/commander",
    "/datum/outfit/centcom/ert/commander/amber",
    "/datum/outfit/centcom/ert/commander/red",
    "/datum/outfit/centcom/ert/commander/gamma",
    "/datum/outfit/centcom/ert/commander/inquisitor",
    "/datum/outfit/centcom/ert/commander/party",
    "/datum/outfit/centcom/ert/janitor",
    "/datum/outfit/centcom/ert/janitor/heavy",
    "/datum/outfit/centcom/ert/janitor/amber",
    "/datum/outfit/centcom/ert/janitor/red",
    "/datum/outfit/centcom/ert/janitor/gamma",
    "/datum/outfit/centcom/ert/janitor/party",
    "/datum/outfit/centcom/ert/chaplain",
    "/datum/outfit/centcom/ert/chaplain/amber",
    "/datum/outfit/centcom/ert/chaplain/red",
    "/datum/outfit/centcom/ert/chaplain/gamma",
    "/datum/outfit/centcom/ert/chaplain/inquisitor",
    "/datum/outfit/centcom/ert/clown",
    "/datum/outfit/centcom/ert/clown/amber",
    "/datum/outfit/centcom/ert/clown/red",
    "/datum/outfit/centcom/ert/clown/gamma",
    "/datum/outfit/centcom/ert/clown/party",
    "/datum/outfit/centcom/death_commando",
    "/datum/outfit/centcom/death_commando/officer",
    "/datum/outfit/centcom/centcom_official",
    "/datum/outfit/centcom/centcom_intern",
    "/datum/outfit/centcom/centcom_intern/leader",
    "/datum/outfit/centcom/centcom_intern/unarmed",
    "/datum/outfit/centcom/centcom_intern/leader/unarmed",
    "/datum/outfit/centcom/ert/marine",
    "/datum/outfit/centcom/ert/marine/security",
    "/datum/outfit/centcom/ert/marine/engineer",
    "/datum/outfit/centcom/ert/marine/medic",
    "/datum/outfit/centcom/militia",
    "/datum/outfit/centcom/militia/general",
    "/datum/outfit/centcom/ert/medical_commander",
    "/datum/outfit/centcom/ert/medical_technician",
    "/datum/outfit/bountyarmor/ert",
    "/datum/outfit/bountyhook/ert",
    "/datum/outfit/bountysynth/ert",
];

/// The space ninja's and the fugitives' outfits (`ss13_game::antag::raiders`; pirates and
/// hunters wear their spawners' outfits, `crate::mob_spawn`).
pub const RAIDER_OUTFITS: [&str; 6] = [
    "/datum/outfit/ninja",
    "/datum/outfit/prisoner",
    "/datum/outfit/yalp_cultist",
    "/datum/outfit/waldo",
    "/datum/outfit/synthetic",
    "/datum/outfit/invisible_man",
];

/// What the raiders make at runtime (`ss13_game::antag::raiders`, `ss13_game::modsuit::ninja`):
/// the ninja's katana (when the sheath came empty), MOD and spider charge, the star
/// dispenser's stars, the energy net, the fugitive leader's toolbox and the synth's augments,
/// the hunters' capture machine.
pub const RAIDER_RUNTIME_TYPES: [&str; 10] = [
    "/obj/item/energy_katana",
    "/obj/item/mod/control/pre_equipped/ninja",
    "/obj/item/grenade/c4/ninja",
    "/obj/item/throwing_star/stamina/ninja",
    "/obj/structure/energy_net",
    "/obj/item/storage/toolbox/mechanical",
    "/obj/item/choice_beacon/augments",
    "/obj/item/autosurgeon",
    "/obj/machinery/fugitive_capture",
    "/obj/machinery/fugitive_capture/psyker",
];

/// Items the nuclear operatives get at runtime: the leader's code paper and war declaration
/// item (`/datum/antagonist/nukeop/leader`, the clowns' `/clownops`), the uplinks
/// (`/datum/outfit/syndicate` `uplink_type`: nuclear, lone operative, clown), the operatives'
/// pinpointer (on the syndicate base in tg), the disk, the telecrystals of a war declaration
/// without an uplink (`distribute_tc`) and what a bananium detonation dresses people in
/// (`make_into_clown`).
pub const NUKEOP_RUNTIME_TYPES: [&str; 13] = [
    "/obj/item/paper",
    "/obj/item/nuclear_challenge",
    "/obj/item/nuclear_challenge/clownops",
    "/obj/item/uplink/nuclear",
    "/obj/item/uplink/loneop",
    "/obj/item/uplink/clownop",
    "/obj/item/pinpointer/nuke/syndicate",
    "/obj/item/disk/nuclear",
    "/obj/item/stack/telecrystal",
    "/obj/item/clothing/under/rank/civilian/clown",
    "/obj/item/clothing/shoes/clown_shoes",
    "/obj/item/clothing/mask/gas/clown_hat",
    "/obj/item/storage/backpack/clown",
];

/// The healing school's staff (`ss13_game::antag::wizard`).
pub const HEALING_STAFF: &str = "/obj/item/gun/magic/staff/healing";

/// What the wizard and its spells make at runtime (`ss13_game::antag::wizard`,
/// `ss13_game::spells`): `/datum/outfit/wizard` (+ `/apprentice`), the spellbook's items, the
/// touch hands, the forcefield, the magic bolts and the door bolt's airlock and plating.
/// What the spells of `ss13_game::spells` and the wizard's Grand Ritual, scepter and demon
/// bottles spawn (`ss13_game::spells::conjure::runtime_paths`, `shapeshift`, `wizard::ritual`).
pub const SPELL_RUNTIME_TYPES: &[&str] = &[
    "/mob/living/basic/bee/toxin",
    "/mob/living/basic/carp",
    "/obj/item/food/cheese/wheel",
    "/obj/structure/constructshell",
    "/mob/living/basic/creature",
    "/mob/living/basic/bot/secbot/ed209",
    "/obj/structure/chair/mime",
    "/obj/effect/forcefield/mime",
    "/obj/structure/spawner/nether",
    "/obj/item/gift",
    "/mob/living/basic/gorilla/lesser",
    "/mob/living/carbon/human/species/monkey/angry",
    "/obj/item/soulstone",
    "/obj/item/soulstone/anybody",
    "/obj/item/soulstone/anybody/purified",
    "/obj/item/soulstone/mystic",
    "/obj/structure/trap/stun",
    "/obj/structure/trap/fire",
    "/obj/structure/trap/chill",
    "/obj/structure/trap/damage",
    "/obj/item/gun/ballistic/rifle/enchanted",
    "/obj/item/gun/magic/wand/arcane_barrage",
    "/obj/item/storage/box/mime",
    "/obj/item/spellpacket/lightningbolt",
    "/obj/item/toy/snowball",
    "/mob/living/simple_animal/hostile/megafauna/dragon/lesser",
    "/mob/living/basic/gorilla",
    "/mob/living/basic/mining/polarbear/lesser",
    "/mob/living/basic/mining/wolf",
    "/mob/living/basic/carp/magic/chaos",
    "/mob/living/basic/construct/juggernaut/mystic",
    "/mob/living/basic/mouse",
    "/mob/living/basic/pet/dog/corgi",
    "/mob/living/basic/spider/giant/viper/wizard",
    "/obj/effect/grand_rune",
    "/obj/effect/grand_rune/finale",
    "/obj/effect/grand_rune/finale/cheesy",
    "/obj/effect/decal/cleanable/grand_remains",
    "/obj/effect/decal/cleanable/grand_remains/cheese",
    "/obj/effect/spectre_of_resurrection",
    "/obj/item/gun/magic/staff/chaos/true_wabbajack",
    "/obj/item/clothing/under/rank/civilian/clown/magic",
    "/obj/item/clothing/mask/gas/clown_hat",
    "/obj/item/food/cheese/wedge",
    "/obj/item/food/grown/banana",
    "/obj/item/food/cookie",
    "/obj/item/food/donut/plain",
    "/obj/item/food/pizzaslice/margherita",
    "/obj/item/food/burger/plain",
    "/mob/living/basic/illusion",
    "/mob/living/basic/killer_tomato",
    "/mob/living/basic/skeleton",
    "/mob/living/basic/wumborian_fugu",
    "/mob/living/simple_animal/hostile/ooze",
    "/obj/item/runic_vendor_scepter",
    "/obj/machinery/vending/runic_vendor",
    "/obj/item/toy/foam_runic_scepter",
    "/obj/item/clothing/head/wizard/red",
    "/obj/item/clothing/suit/wizrobe/red",
    "/obj/item/clothing/head/wizard/yellow",
    "/obj/item/clothing/suit/wizrobe/yellow",
    "/obj/item/antag_spawner/slaughter_demon",
    "/obj/item/antag_spawner/slaughter_demon/laughter",
];

pub const WIZARD_RUNTIME_TYPES: [&str; 40] = [
    "/obj/item/spellbook",
    "/obj/item/teleportation_scroll",
    "/obj/item/teleportation_scroll/apprentice",
    "/obj/item/antag_spawner/contract",
    "/obj/item/clothing/suit/wizrobe",
    "/obj/item/clothing/head/wizard",
    "/obj/item/clothing/shoes/sandal/magic",
    "/obj/item/clothing/under/color/lightpurple",
    "/obj/item/radio/headset",
    "/obj/item/staff",
    "/obj/item/storage/backpack",
    "/obj/item/storage/box/survival",
    "/obj/item/gun/magic/staff/chaos",
    HEALING_STAFF,
    "/obj/item/gun/magic/staff/door",
    "/obj/item/storage/belt/wands/full",
    "/obj/item/gun/magic/wand/death",
    "/obj/item/gun/magic/wand/resurrection",
    "/obj/item/gun/magic/wand/polymorph",
    "/obj/item/gun/magic/wand/teleport",
    "/obj/item/gun/magic/wand/door",
    "/obj/item/gun/magic/wand/fireball",
    "/obj/item/gun/magic/wand/shrink",
    "/obj/item/melee/touch_attack/smite",
    "/obj/effect/forcefield/wizard",
    "/obj/effect/particle_effect/fluid/smoke/bad",
    "/obj/projectile/magic/nothing",
    "/obj/projectile/magic/aoe/magic_missile",
    "/obj/projectile/magic/fireball",
    "/obj/projectile/magic/aoe/lightning",
    "/obj/projectile/magic/death",
    "/obj/projectile/magic/resurrection",
    "/obj/projectile/magic/teleport",
    "/obj/projectile/magic/safety",
    "/obj/projectile/magic/door",
    "/obj/machinery/door/airlock/wood",
    "/turf/open/floor/plating",
    "/obj/effect/landmark/start/wizard",
    "/area/centcom/wizard_station",
    "/obj/item/clothing/suit/wizrobe/red",
];

/// What the abductors make at runtime (`ss13_game::antag::abductor`): their outfits, the
/// console's gear, the gland storage's glands, the glands' eggs, slimes and spiderlings, the
/// baton's energy cuffs.
pub const ABDUCTOR_RUNTIME_TYPES: [&str; 38] = [
    "/obj/item/clothing/under/abductor",
    "/obj/item/clothing/shoes/combat",
    "/obj/item/radio/headset/abductor",
    "/obj/item/clothing/head/helmet/abductor",
    "/obj/item/clothing/suit/armor/abductor/vest",
    "/obj/item/melee/baton/abductor",
    "/obj/item/storage/belt/military/abductor/full",
    "/obj/item/storage/backpack",
    "/obj/item/gun/energy/alien",
    "/obj/item/abductor/silencer",
    "/obj/item/abductor/gizmo",
    "/obj/item/abductor/mind_device",
    "/obj/item/restraints/handcuffs/energy",
    "/obj/item/organ/tongue/abductor",
    "/obj/item/abductor_machine_beacon/chem_dispenser",
    "/obj/item/gun/energy/recharge/shrink_ray",
    "/obj/item/abductor/alien_omnitool",
    "/mob/living/basic/cow",
    "/obj/item/food/grown/wheat",
    "/obj/item/poster/random_abductor",
    "/obj/item/organ/heart/gland/access",
    "/obj/item/organ/heart/gland/blood",
    "/obj/item/organ/heart/gland/chem",
    "/obj/item/organ/heart/gland/egg",
    "/obj/item/organ/heart/gland/electric",
    "/obj/item/organ/heart/gland/heal",
    "/obj/item/organ/heart/gland/mindshock",
    "/obj/item/organ/heart/gland/plasma",
    "/obj/item/organ/heart/gland/quantum",
    "/obj/item/organ/heart/gland/slime",
    "/obj/item/organ/heart/gland/spiderman",
    "/obj/item/organ/heart/gland/transform",
    "/obj/item/organ/heart/gland/trauma",
    "/obj/item/organ/heart/gland/ventcrawling",
    "/obj/item/organ/heart/gland/viral",
    "/obj/item/food/egg/gland",
    "/mob/living/basic/slime",
    "/mob/living/basic/spider/growing/spiderling",
];

/// The uplink items sold here: the ones whose item exists in this codebase. The rest of tg's
/// catalog needs mechanics that are not modelled yet (see `docs/parity/antagonists/uplink.md`).
// shortcut: a curated list; grow it as the items' mechanics land.
pub const UPLINK_ITEMS: [&str; 102] = [
    // Guns that fire (`ss13_game::guns`): the traitors' Makarov case and .357 revolver, the
    // operatives' C-20r case, and their ammunition.
    "/datum/uplink_item/dangerous/pistol",
    "/datum/uplink_item/ammo/pistol",
    "/datum/uplink_item/dangerous/revolver",
    "/datum/uplink_item/ammo/revolver",
    "/datum/uplink_item/weapon_kits/medium_cost/smg",
    "/datum/uplink_item/ammo_nuclear/basic/smg",
    "/datum/uplink_item/device_tools/toolbox",
    "/datum/uplink_item/device_tools/duffelbag",
    "/datum/uplink_item/device_tools/military_belt",
    "/datum/uplink_item/device_tools/tactical_medkit",
    // shortcut: the key cannot be put into a headset yet (no `keyslot` changes).
    "/datum/uplink_item/device_tools/encryptionkey",
    // The emag and the doorjack (`ss13_game::emag`): the emag does not open airlocks, the
    // doorjack only does.
    "/datum/uplink_item/device_tools/emag",
    "/datum/uplink_item/device_tools/doorjack",
    // shortcut: an ID card without the agent card's forging and access copying.
    "/datum/uplink_item/stealthy_tools/agent_card",
    "/datum/uplink_item/stealthy_tools/syndigaloshes",
    // The chameleon kit (`ss13_game::clothing::chameleon`): only its clothing changes looks.
    "/datum/uplink_item/stealthy_tools/chameleon",
    // C-4 (`ss13_game::explosions::sources`): timer, planting, the blast; no wires or
    // assemblies.
    "/datum/uplink_item/explosives/c4",
    // The nuclear operatives' bag of C-4.
    "/datum/uplink_item/explosives/military_grade/c4bag",
    // X-4, the frag grenade, the minibomb and the EMP kit (`ss13_game::grenades`, the EMP
    // implanter of `ss13_game::implants`).
    "/datum/uplink_item/explosives/x4",
    "/datum/uplink_item/explosives/frag",
    "/datum/uplink_item/explosives/syndicate_minibomb",
    "/datum/uplink_item/explosives/emp",
    "/datum/uplink_item/implants/freedom",
    // The sleeping carp scroll (`ss13_game::martial`).
    "/datum/uplink_item/stealthy_weapons/martialarts",
    "/datum/uplink_item/device_tools/stimpack",
    // The energy sword, the energy dagger and the sleepy pen
    // (`ss13_game::antag::syndie_items`).
    "/datum/uplink_item/dangerous/sword",
    "/datum/uplink_item/stealthy_weapons/edagger",
    "/datum/uplink_item/stealthy_weapons/sleepy_pen",
    // The failsafe code (`ss13_game::antag::uplink`; its item is abstract).
    "/datum/uplink_item/device_tools/failsafe",
    // Not sold (`purchasable_from = NONE`): brings the raw telecrystal's prototype for
    // `buy_raw_tc`.
    "/datum/uplink_item/bundles_tc/telecrystal",
    "/datum/uplink_item/badass/balloon",
    // Makarov magazines: armour piercing, hollow point, incendiary (`ss13_game::projectile`).
    "/datum/uplink_item/ammo/pistolap",
    "/datum/uplink_item/ammo/pistolhp",
    "/datum/uplink_item/ammo/pistolfire",
    // MODsuits and modules (`ss13_game::modsuit`).
    // shortcut: the infiltrator's psi-emitter stealth module, the shock absorber, the magnetic
    // harness and the other effectless modules of these suits only install and draw power.
    "/datum/uplink_item/suits/infiltrator_bundle",
    "/datum/uplink_item/suits/space_suit",
    "/datum/uplink_item/suits/modsuit",
    "/datum/uplink_item/suits/noslip",
    "/datum/uplink_item/suits/modsuit/elite",
    "/datum/uplink_item/suits/emp_shield",
    // Role-restricted: the counterfeit mail kit (`ss13_game::cargo::mail`), the DNA syringe gun,
    // the reagent dartgun and the meat hook (`ss13_game::guns_special`), the syndicate rebar
    // crossbow, the surgery disks (`ss13_game::medical::operating`), the pressure modkit
    // (`ss13_game::mining::modkits`), the laser arm autosurgeon
    // (`ss13_game::medical::cyberimps`), BandaStation's breaching sledgehammer
    // (`ss13_game::weapons::rip_and_tear`) and the ancient jumpsuit (plain clothing in tg too).
    "/datum/uplink_item/role_restricted/mail_counterfeit_kit",
    "/datum/uplink_item/role_restricted/ancient_jumpsuit",
    "/datum/uplink_item/role_restricted/modified_syringe_gun",
    "/datum/uplink_item/role_restricted/meathook",
    "/datum/uplink_item/role_restricted/rebarxbowsyndie",
    "/datum/uplink_item/role_restricted/brainwash_disk",
    "/datum/uplink_item/role_restricted/advanced_plastic_surgery",
    "/datum/uplink_item/role_restricted/pressure_mod",
    "/datum/uplink_item/role_restricted/laser_arm",
    "/datum/uplink_item/role_restricted/chemical_gun",
    "/datum/uplink_item/role_restricted/syndiesledge",
    // `purchasable_from = NONE`: tg sells it to traitors only under the cybernetic revolution
    // station trait, which is not ported.
    "/datum/uplink_item/special/autosurgeon",
    // The operatives' guns, their ammunition (`ss13_game::guns`) and kits.
    // shortcut: the M-90gl's underbarrel launcher, the L6 SAW's cover and the sniper scope are
    // not modelled; the guns fire and reload as plain ballistics.
    "/datum/uplink_item/weapon_kits/low_cost/shotgun",
    "/datum/uplink_item/ammo_nuclear/basic/buck",
    "/datum/uplink_item/ammo_nuclear/basic/slug",
    "/datum/uplink_item/ammo_nuclear/basic/flechette",
    "/datum/uplink_item/ammo_nuclear/incendiary/dragon",
    "/datum/uplink_item/weapon_kits/low_cost/clandestine",
    "/datum/uplink_item/ammo_nuclear/basic/m10mm",
    "/datum/uplink_item/ammo_nuclear/ap/m10mm",
    "/datum/uplink_item/ammo_nuclear/hp/m10mm",
    "/datum/uplink_item/ammo_nuclear/incendiary/m10mm",
    "/datum/uplink_item/ammo_nuclear/ap/smg",
    "/datum/uplink_item/ammo_nuclear/hp/smg",
    "/datum/uplink_item/ammo_nuclear/incendiary/smg",
    "/datum/uplink_item/weapon_kits/medium_cost/revolvercase",
    "/datum/uplink_item/ammo_nuclear/basic/revolver",
    "/datum/uplink_item/weapon_kits/high_cost/machinegun",
    "/datum/uplink_item/ammo_nuclear/basic/machinegun",
    "/datum/uplink_item/ammo_nuclear/ap/machinegun",
    "/datum/uplink_item/ammo_nuclear/hp/machinegun",
    "/datum/uplink_item/ammo_nuclear/incendiary/machinegun",
    "/datum/uplink_item/ammo_nuclear/special/machinegun",
    "/datum/uplink_item/weapon_kits/high_cost/carbine",
    "/datum/uplink_item/ammo_nuclear/basic/carbine",
    "/datum/uplink_item/weapon_kits/high_cost/sniper",
    "/datum/uplink_item/ammo_nuclear/basic/sniper",
    "/datum/uplink_item/ammo_nuclear/incendiary/sniper",
    "/datum/uplink_item/weapon_kits/surplus_smg",
    "/datum/uplink_item/ammo_nuclear/surplus_smg",
    "/datum/uplink_item/weapon_kits/medium_cost/syndiesledge",
    "/datum/uplink_item/explosives/military_grade/frag_grenade",
    "/datum/uplink_item/explosives/military_grade/x4",
    "/datum/uplink_item/explosives/military_grade/syndicate_minibomb",
    "/datum/uplink_item/explosives/military_grade/x4bag",
    // Mechs (`ss13_game::mecha`) land at the buyer's feet; their support bags.
    "/datum/uplink_item/mech/gygax",
    "/datum/uplink_item/mech/mauler",
    "/datum/uplink_item/mech/support_bag",
    "/datum/uplink_item/mech/support_bag/mauler",
    "/datum/uplink_item/stealthy_tools/syndigaloshes/nuke",
    // Bomb implants (`ss13_game::implants`), the reviver and CNS rebooter autosurgeons.
    "/datum/uplink_item/implants/nuclear/microbomb",
    "/datum/uplink_item/implants/nuclear/macrobomb",
    "/datum/uplink_item/implants/nuclear/reviver",
    "/datum/uplink_item/implants/nuclear/antistun",
    "/datum/uplink_item/device_tools/medkit",
    // The syndicate jaws of life (`ss13_game::doors`) and BandaStation's tacticool holster
    // (`ss13_game::clothing::holsters`).
    "/datum/uplink_item/device_tools/syndie_jaws_of_life",
    "/datum/uplink_item/device_tools/holster_tacticool",
    // The reinforcement beacons (`ss13_game::antag::nukeop::reinforcements`).
    "/datum/uplink_item/reinforcements/operative_reinforcement",
    "/datum/uplink_item/reinforcement/clown_reinforcement",
    "/datum/uplink_item/reinforcements/assault_borg",
    "/datum/uplink_item/reinforcements/medical_borg",
    "/datum/uplink_item/reinforcements/saboteur_borg",
];

/// tg `/datum/uplink_category/discounts`: the category of an uplink's sales.
pub const DISCOUNT_CATEGORY: &str = "/datum/uplink_category/discounts";

/// A `/datum/uplink_item` (the fields `/datum/asset/json/uplink` sends).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct UplinkItemDef {
    /// The datum type: the `buy` act's `path`.
    pub path: String,
    pub name: String,
    pub desc: String,
    /// The spawned item type.
    pub item: String,
    pub cost: u32,
    /// `category.name`.
    pub category: String,
    pub purchasable_from: u32,
    /// `-1`: unlimited.
    pub limited_stock: i32,
    pub lock_other_purchases: bool,
    pub cant_discount: bool,
    /// `limited_discount_stock`: the stock of this item on sale (`-1`: as `limited_stock`).
    #[serde(default = "no_limit")]
    pub limited_discount_stock: i32,
    pub population_minimum: u32,
    pub restricted_roles: Vec<String>,
    /// `uplink_item_flags` (`SYNDIE_ILLEGAL_TECH`, `SYNDIE_TRIPS_CONTRABAND`).
    #[serde(default = "default_uplink_flags")]
    pub uplink_item_flags: u32,
}

/// `SYNDIE_TRIPS_CONTRABAND`: the item counts as contraband (`is_contraband`).
pub const SYNDIE_TRIPS_CONTRABAND: u32 = 1 << 1;

/// The `/datum/uplink_item` default, `SYNDIE_ILLEGAL_TECH | SYNDIE_TRIPS_CONTRABAND`.
fn default_uplink_flags() -> u32 {
    (1 << 0) | SYNDIE_TRIPS_CONTRABAND
}

/// A `/datum/objective_item/steal` of `objective_type` `normal` (tg `GLOB.possible_items`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct StealItemDef {
    pub path: String,
    /// How the objective names it (`"Украдите [name]"`).
    pub name: String,
    pub targetitem: String,
    /// Jobs that cannot get this objective.
    pub excludefromjob: Vec<String>,
    /// Only a target while one exists on the station.
    pub exists_on_map: bool,
    pub altitems: Vec<String>,
    /// The type has its own `check_special_completion`, `target_exists` or `New` (an AI in the
    /// card, a supermatter engine, extraction kits handed out).
    pub special: bool,
}

/// One employer of `strings/antagonist_flavor/traitor_flavor.json`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TraitorFlavor {
    #[serde(default)]
    pub allies: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub introduction: String,
    #[serde(default)]
    pub roundend_report: String,
    #[serde(default)]
    pub ui_theme: String,
    #[serde(default)]
    pub uplink: String,
}

fn no_limit() -> i32 {
    -1
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AntagData {
    /// Category names by `weight`, highest first (`cmp_uplink_category_desc`).
    pub uplink_categories: Vec<String>,
    /// [`DISCOUNT_CATEGORY`]'s name.
    #[serde(default)]
    pub discount_category: String,
    pub uplink_items: Vec<UplinkItemDef>,
    pub steal_items: Vec<StealItemDef>,
    /// `TRAITOR_FLAVOR_FILE` by employer name, sorted.
    #[serde(default)]
    pub traitor_flavor: Vec<(String, TraitorFlavor)>,
    /// `strings/phonetic_alphabet.txt` (`GLOB.phonetic_alphabet`).
    #[serde(default)]
    pub phonetic_alphabet: Vec<String>,
    /// [`NUKEOP_OUTFITS`] that resolve, in that order.
    #[serde(default)]
    pub nukeop_outfits: Vec<crate::jobs::Outfit>,
    /// [`ERT_OUTFITS`] that resolve, in that order.
    #[serde(default)]
    pub ert_outfits: Vec<crate::jobs::Outfit>,
    /// `strings/names/operative_alias.txt` (`GLOB.operative_aliases`).
    #[serde(default)]
    pub operative_aliases: Vec<String>,
    /// `strings/names/last.txt` (`GLOB.last_names`).
    #[serde(default)]
    pub last_names: Vec<String>,
    /// [`RAIDER_OUTFITS`] that resolve, in that order.
    #[serde(default)]
    pub raider_outfits: Vec<crate::jobs::Outfit>,
    /// `strings/names/ninjatitle.txt`, `ninjaname.txt` (`GLOB.ninja_titles`, `ninja_names`).
    #[serde(default)]
    pub ninja_titles: Vec<String>,
    #[serde(default)]
    pub ninja_names: Vec<String>,
    /// `strings/ninja.json` `lines` (`NINJA_FILE`): the adrenaline boost's battle cries.
    #[serde(default)]
    pub ninja_lines: Vec<String>,
    /// `strings/pirates.json` (`PIRATE_NAMES_FILE`): name pools by key, sorted.
    #[serde(default)]
    pub pirate_names: Vec<(String, Vec<String>)>,
}

/// Reads the traitor flavor and the phonetic alphabet from the BYOND checkout's `strings/`;
/// a missing file leaves its part empty (reported).
pub fn read_strings(source: &Path, data: &mut AntagData) -> Vec<String> {
    let mut problems = Vec::new();
    let flavor = source.join("strings/antagonist_flavor/traitor_flavor.json");
    match std::fs::read(&flavor)
        .map_err(anyhow::Error::from)
        .and_then(|b| {
            Ok(serde_json::from_slice::<
                std::collections::BTreeMap<String, TraitorFlavor>,
            >(&b)?)
        }) {
        Ok(map) => data.traitor_flavor = map.into_iter().collect(),
        Err(e) => problems.push(format!("{}: {e}", flavor.display())),
    }
    let lines = |file: &str, problems: &mut Vec<String>| {
        let path = source.join(file);
        match std::fs::read_to_string(&path) {
            Ok(text) => text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect(),
            Err(e) => {
                problems.push(format!("{}: {e}", path.display()));
                Vec::new()
            }
        }
    };
    data.phonetic_alphabet = lines("strings/phonetic_alphabet.txt", &mut problems);
    data.operative_aliases = lines("strings/names/operative_alias.txt", &mut problems);
    data.last_names = lines("strings/names/last.txt", &mut problems);
    data.ninja_titles = lines("strings/names/ninjatitle.txt", &mut problems);
    data.ninja_names = lines("strings/names/ninjaname.txt", &mut problems);
    let json = |file: &str, problems: &mut Vec<String>| {
        let path = source.join(file);
        match std::fs::read(&path)
            .map_err(anyhow::Error::from)
            .and_then(|b| {
                Ok(serde_json::from_slice::<
                    std::collections::BTreeMap<String, Vec<String>>,
                >(&b)?)
            }) {
            Ok(map) => map,
            Err(e) => {
                problems.push(format!("{}: {e}", path.display()));
                std::collections::BTreeMap::new()
            }
        }
    };
    data.ninja_lines = json("strings/ninja.json", &mut problems)
        .remove("lines")
        .unwrap_or_default();
    data.pirate_names = json("strings/pirates.json", &mut problems)
        .into_iter()
        .collect();
    problems
}

impl AntagData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        crate::read_ron(path)
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("antag data serializes"),
        );
        out.push('\n');
        out
    }

    /// Item types the uplink spawns: the converter emits prototypes for them.
    pub fn item_types(&self) -> BTreeSet<&str> {
        self.uplink_items.iter().map(|i| i.item.as_str()).collect()
    }

    /// Item types the emergency response teams' outfits spawn.
    pub fn ert_types(&self) -> BTreeSet<&str> {
        self.ert_outfits
            .iter()
            .flat_map(|o| {
                o.items
                    .iter()
                    .map(|(_, p)| p.as_str())
                    .chain(o.backpack_contents.iter().map(|(p, _)| p.as_str()))
                    .chain(o.belt_contents.iter().map(|(p, _)| p.as_str()))
            })
            .collect()
    }

    /// Item types the raiders' outfits spawn ([`RAIDER_OUTFITS`]).
    pub fn raider_types(&self) -> BTreeSet<&str> {
        self.raider_outfits
            .iter()
            .flat_map(|o| {
                o.items
                    .iter()
                    .map(|(_, p)| p.as_str())
                    .chain(o.backpack_contents.iter().map(|(p, _)| p.as_str()))
                    .chain(o.belt_contents.iter().map(|(p, _)| p.as_str()))
            })
            .collect()
    }

    /// Item types the nuclear operatives' outfits spawn, and [`NUKEOP_RUNTIME_TYPES`].
    pub fn nukeop_types(&self) -> BTreeSet<&str> {
        self.nukeop_outfits
            .iter()
            .flat_map(|o| {
                o.items
                    .iter()
                    .map(|(_, p)| p.as_str())
                    .chain(o.backpack_contents.iter().map(|(p, _)| p.as_str()))
                    .chain(o.belt_contents.iter().map(|(p, _)| p.as_str()))
            })
            .chain(NUKEOP_RUNTIME_TYPES)
            .collect()
    }
}

fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn path(v: Option<&Value>) -> Option<String> {
    v?.get("path")?.as_str().map(str::to_owned)
}

fn int(v: Option<&Value>) -> i64 {
    v.and_then(Value::as_f64).map_or(0, |f| f as i64)
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(|v| v.get("list"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_owned).or_else(|| path(Some(i))))
                .collect()
        })
        .unwrap_or_default()
}

/// The catalog ([`UPLINK_ITEMS`] that resolve to an item) and every normal steal target.
pub fn resolve(types: &DmTypes) -> AntagData {
    let mut categories: Vec<(i64, String)> = types
        .types
        .keys()
        .filter(|p| p.starts_with("/datum/uplink_category/"))
        .map(|p| {
            (
                int(types.resolved(p, "weight")),
                text(types.resolved(p, "name")),
            )
        })
        .collect();
    // `sortTim(cmp_uplink_category_desc)`: stable by weight; the dump's keys are path-sorted.
    categories.sort_by_key(|(w, _)| -w);
    let uplink_items: Vec<UplinkItemDef> = UPLINK_ITEMS
        .iter()
        .filter(|p| types.types.contains_key(**p))
        .filter_map(|&p| {
            let item = path(types.resolved(p, "item"))?;
            let category = path(types.resolved(p, "category"))
                .map(|c| text(types.resolved(&c, "name")))
                .unwrap_or_default();
            Some(UplinkItemDef {
                path: p.to_owned(),
                name: text(types.resolved(p, "name")),
                desc: text(types.resolved(p, "desc")),
                item,
                cost: int(types.resolved(p, "cost")).max(0) as u32,
                category,
                purchasable_from: int(types.resolved(p, "purchasable_from")) as u32,
                limited_stock: int(types.resolved(p, "limited_stock")) as i32,
                lock_other_purchases: int(types.resolved(p, "lock_other_purchases")) != 0,
                cant_discount: int(types.resolved(p, "cant_discount")) != 0,
                limited_discount_stock: types
                    .resolved(p, "limited_discount_stock")
                    .map_or(-1, |v| int(Some(v)) as i32),
                population_minimum: int(types.resolved(p, "population_minimum")).max(0) as u32,
                restricted_roles: strings(types.resolved(p, "restricted_roles")),
                uplink_item_flags: types
                    .resolved(p, "uplink_item_flags")
                    .map_or_else(default_uplink_flags, |v| int(Some(v)) as u32),
            })
        })
        .collect();
    let mut steal_items: Vec<StealItemDef> = types
        .types
        .iter()
        .filter(|(p, _)| p.starts_with("/datum/objective_item/steal/"))
        .filter(|(p, _)| text(types.resolved(p, "objective_type")) == "normal")
        .filter_map(|(p, ty)| {
            Some(StealItemDef {
                path: p.clone(),
                name: text(types.resolved(p, "name")),
                targetitem: path(types.resolved(p, "targetitem"))?,
                excludefromjob: strings(types.resolved(p, "excludefromjob")),
                exists_on_map: int(types.resolved(p, "exists_on_map")) != 0,
                altitems: strings(types.resolved(p, "altitems")),
                special: ty.procs.iter().any(|f| {
                    ["check_special_completion", "target_exists", "New"].contains(&f.as_str())
                }),
            })
        })
        .collect();
    steal_items.sort_by(|a, b| a.path.cmp(&b.path));
    let nukeop_outfits = NUKEOP_OUTFITS
        .iter()
        .filter(|p| types.types.contains_key(**p))
        .map(|p| crate::jobs::outfit(types, p))
        .collect();
    let ert_outfits = ERT_OUTFITS
        .iter()
        .filter(|p| types.types.contains_key(**p))
        .map(|p| crate::jobs::outfit(types, p))
        .collect();
    let raider_outfits = RAIDER_OUTFITS
        .iter()
        .filter(|p| types.types.contains_key(**p))
        .map(|p| crate::jobs::outfit(types, p))
        .collect();
    AntagData {
        uplink_categories: categories.into_iter().map(|(_, n)| n).collect(),
        discount_category: text(types.resolved(DISCOUNT_CATEGORY, "name")),
        uplink_items,
        steal_items,
        nukeop_outfits,
        ert_outfits,
        raider_outfits,
        ..AntagData::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/datum/uplink_category": { "parent": "/datum", "declared_vars": { "name": null, "weight": 0 } },
        "/datum/uplink_category/device_tools": { "parent": "/datum/uplink_category",
            "declared_vars": { "name": "Misc. Gadgets", "weight": 3 } },
        "/datum/uplink_category/badassery": { "parent": "/datum/uplink_category",
            "declared_vars": { "name": "(Pointless) Badassery", "weight": 0 } },
        "/datum/uplink_category/explosives": { "parent": "/datum/uplink_category",
            "declared_vars": { "name": "Explosives", "weight": 6 } },
        "/datum/uplink_category/discounts": { "parent": "/datum/uplink_category",
            "declared_vars": { "name": "Discounted Gear", "weight": -1 } },
        "/datum/uplink_item": { "parent": "/datum", "declared_vars": { "cost": 0,
            "item": null, "category": null, "purchasable_from": 16777215, "limited_stock": -1,
            "lock_other_purchases": 0, "cant_discount": 0, "population_minimum": 0,
            "restricted_roles": { "list": [] }, "name": "item name", "desc": "item description" } },
        "/datum/uplink_item/device_tools": { "parent": "/datum/uplink_item",
            "declared_vars": { "category": { "path": "/datum/uplink_category/device_tools" } } },
        "/datum/uplink_item/device_tools/toolbox": { "parent": "/datum/uplink_item/device_tools",
            "declared_vars": { "cost": 1, "name": "Full Syndicate Toolbox", "desc": "Tools.",
            "item": { "path": "/obj/item/storage/toolbox/syndicate" } } },
        "/datum/uplink_item/badass": { "parent": "/datum/uplink_item",
            "declared_vars": { "category": { "path": "/datum/uplink_category/badassery" } } },
        "/datum/uplink_item/badass/balloon": { "parent": "/datum/uplink_item/badass",
            "declared_vars": { "cost": 20, "lock_other_purchases": 1, "cant_discount": 1,
            "name": "Syndicate Balloon", "item": { "path": "/obj/item/toy/balloon/syndicate" } } },
        "/datum/objective_item": { "parent": "/datum", "declared_vars": {
            "name": "a silly bike horn! Honk!", "targetitem": { "path": "/obj/item/bikehorn" },
            "objective_type": "normal", "excludefromjob": { "list": [] }, "exists_on_map": 0,
            "altitems": { "list": [] } },
            "procs": ["check_special_completion"] },
        "/datum/objective_item/steal": { "parent": "/datum/objective_item" },
        "/datum/objective_item/steal/caplaser": { "parent": "/datum/objective_item/steal",
            "declared_vars": { "name": "капитанское антикварное лазерное оружие",
            "targetitem": { "path": "/obj/item/gun/energy/laser/captain" },
            "excludefromjob": { "list": ["Captain"] }, "exists_on_map": 1 } },
        "/datum/objective_item/steal/nukedisc": { "parent": "/datum/objective_item/steal",
            "declared_vars": { "name": "диск", "targetitem": { "path": "/obj/item/disk/nuclear" } },
            "procs": ["check_special_completion"] },
        "/datum/objective_item/steal/traitor": { "parent": "/datum/objective_item/steal",
            "declared_vars": { "objective_type": "traitor" } },
        "/datum/objective_item/steal/traitor/fireaxe": { "parent": "/datum/objective_item/steal/traitor",
            "declared_vars": { "name": "пожарный топор", "targetitem": { "path": "/obj/item/fireaxe" } } }
      }
    }"##;

    #[test]
    fn catalog_and_steal_targets_resolve_through_parents() {
        let data = resolve(&DmTypes::parse(TYPES.as_bytes()).unwrap());
        assert_eq!(
            data.uplink_categories,
            [
                "Explosives",
                "Misc. Gadgets",
                "(Pointless) Badassery",
                "Discounted Gear"
            ]
        );
        assert_eq!(data.discount_category, "Discounted Gear");
        let paths: Vec<&str> = data.uplink_items.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/datum/uplink_item/device_tools/toolbox",
                "/datum/uplink_item/badass/balloon"
            ]
        );
        let toolbox = &data.uplink_items[0];
        assert_eq!(
            (
                toolbox.cost,
                toolbox.category.as_str(),
                toolbox.limited_stock
            ),
            (1, "Misc. Gadgets", -1)
        );
        assert_eq!(toolbox.limited_discount_stock, -1);
        assert_eq!(toolbox.purchasable_from & UPLINK_TRAITORS, UPLINK_TRAITORS);
        assert!(data.uplink_items[1].lock_other_purchases);
        let steal: Vec<(&str, bool, bool)> = data
            .steal_items
            .iter()
            .map(|s| (s.name.as_str(), s.exists_on_map, s.special))
            .collect();
        // Traitor-type items are not steal objectives; only types defining the proc are special.
        assert_eq!(
            steal,
            [
                ("капитанское антикварное лазерное оружие", true, false),
                ("диск", false, true)
            ]
        );
        assert_eq!(data.steal_items[0].excludefromjob, ["Captain"]);
        assert_eq!(data.item_types().len(), 2);
    }
}
