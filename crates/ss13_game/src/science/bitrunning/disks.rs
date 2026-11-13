//! Bitrunning disks (server), after tg `code/modules/bitrunning/objects/disks.dm`,
//! `objects/gimmick_disks/*.dm`, `components/avatar_gear.dm` and `stock_gear`
//! (`server/obj_generation.dm`).
//!
//! - Used in hand, a disk asks once which of its programs to hold (`tgui_input_list`); the
//!   choice is final.
//! - A new avatar is stocked from every disk its bitrunner carries (held, worn or nested in
//!   bags): an item disk puts its item in the avatar's hands, an ability disk teaches its spell,
//!   a gimmick disk puts its loadout in a "digi-case" in the avatar's hands. A disk without a
//!   choice, or a spell the avatar already knows, warns the bitrunner.
// shortcut: no domain forbidding items or abilities (no BandaStation domain has
// `DOMAIN_FORBIDS_*`), no examine lines, no skillchips copied onto the avatar; "Lesser Heal"
// (`spell/basic_heal`) and the wizard loadout's digital spells are not ported, so their choices
// are left out.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName};
use ss13_net::server::Networked;
use ss13_proto::Prototypes;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::spells::Spell;
use crate::storage::Stored;
use crate::tools::tell_mob;

pub const DISK_ROOT: &str = "/obj/item/disk/bitrunning";
const DIGITAL_STORAGE: &str = "/obj/item/storage/briefcase/secure/digital_storage";
const CLICK: &str = "sound/items/click.ogg";

/// What one choice of a disk gives the avatar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Program {
    /// `disk/bitrunning/item`: `put_in_hands(new granted_item())`.
    Item(&'static str),
    /// `disk/bitrunning/ability`: the spell's path.
    Ability(&'static str),
    /// `/datum/bitrunning_gimmick`: its name and `granted_items`.
    Loadout(&'static str, &'static [&'static str]),
}

/// The disk types and their `selectable_*` lists.
pub const DISKS: &[(&str, &[Program])] = &[
    (
        "/obj/item/disk/bitrunning/ability/tier1",
        &[Program::Ability(
            "/datum/action/cooldown/spell/conjure/cheese",
        )],
    ),
    (
        "/obj/item/disk/bitrunning/ability/tier2",
        &[
            Program::Ability("/datum/action/cooldown/spell/pointed/projectile/fireball"),
            Program::Ability("/datum/action/cooldown/spell/pointed/projectile/lightningbolt"),
            Program::Ability("/datum/action/cooldown/spell/forcewall"),
        ],
    ),
    (
        "/obj/item/disk/bitrunning/ability/tier3",
        &[
            Program::Ability("/datum/action/cooldown/spell/shapeshift/dragon"),
            Program::Ability("/datum/action/cooldown/spell/shapeshift/polar_bear"),
        ],
    ),
    (
        "/obj/item/disk/bitrunning/item/tier1",
        &[
            Program::Item("/obj/item/pizzabox/infinite"),
            Program::Item("/obj/item/gun/medbeam"),
            Program::Item("/obj/item/grenade/c4"),
        ],
    ),
    (
        "/obj/item/disk/bitrunning/item/tier2",
        &[
            Program::Item("/obj/item/reagent_containers/hypospray/medipen/survival/luxury"),
            Program::Item("/obj/item/gun/ballistic/automatic/pistol"),
            Program::Item("/obj/item/clothing/suit/armor/vest"),
        ],
    ),
    (
        "/obj/item/disk/bitrunning/item/tier3",
        &[
            Program::Item("/obj/item/gun/energy/e_gun/nuclear"),
            Program::Item("/obj/item/dualsaber/green"),
            Program::Item("/obj/item/grenade/syndieminibomb"),
        ],
    ),
    (
        "/obj/item/disk/bitrunning/item/pka_mods/premium",
        &[
            Program::Item("/obj/item/borg/upgrade/modkit/cooldown/repeater"),
            Program::Item("/obj/item/borg/upgrade/modkit/lifesteal"),
            Program::Item("/obj/item/borg/upgrade/modkit/resonator_blasts"),
            Program::Item("/obj/item/borg/upgrade/modkit/bounty"),
            Program::Item("/obj/item/borg/upgrade/modkit/indoors"),
        ],
    ),
    (
        "/obj/item/disk/bitrunning/item/pka_mods",
        &[
            Program::Item("/obj/item/borg/upgrade/modkit/range"),
            Program::Item("/obj/item/borg/upgrade/modkit/damage"),
            Program::Item("/obj/item/borg/upgrade/modkit/cooldown"),
            Program::Item("/obj/item/borg/upgrade/modkit/cooldown/aoe/mobs"),
            Program::Item("/obj/item/borg/upgrade/modkit/human_passthrough"),
        ],
    ),
    (
        "/obj/item/disk/bitrunning/item/pkc_mods/premium",
        &[
            Program::Item("/obj/item/crusher_trophy/ice_wing"),
            Program::Item("/obj/item/crusher_trophy/blaster_tubes"),
            Program::Item("/obj/item/crusher_trophy/miner_eye"),
            Program::Item("/obj/item/crusher_trophy/tail_spike"),
            Program::Item("/obj/item/crusher_trophy/demon_claws"),
            Program::Item("/obj/item/crusher_trophy/vortex_talisman"),
            Program::Item("/obj/item/crusher_trophy/ice_demon_cube"),
        ],
    ),
    (
        "/obj/item/disk/bitrunning/item/pkc_mods",
        &[
            Program::Item("/obj/item/crusher_trophy/watcher_wing"),
            Program::Item("/obj/item/crusher_trophy/magma_wing"),
            Program::Item("/obj/item/crusher_trophy/legion_skull"),
            Program::Item("/obj/item/crusher_trophy/wolf_ear"),
        ],
    ),
    (
        "/obj/item/disk/bitrunning/item/mini_uzi",
        &[Program::Item("/obj/item/gun/ballistic/automatic/mini_uzi")],
    ),
    (
        "/obj/item/disk/bitrunning/gimmick/sports",
        &[
            Program::Loadout(
                "Boxer",
                &[
                    "/obj/item/clothing/gloves/boxing/evil",
                    "/obj/item/clothing/under/shorts/red",
                    "/obj/item/reagent_containers/condiment/protein",
                    "/obj/item/reagent_containers/cup/glass/drinkingglass/filled/protein_blend",
                ],
            ),
            Program::Loadout(
                "Skater",
                &[
                    "/obj/item/clothing/shoes/wheelys",
                    "/obj/item/melee/skateboard",
                    "/obj/item/clothing/suit/costume/wellworn_shirt/graphic",
                    "/obj/item/clothing/head/soft/black",
                    "/obj/item/clothing/shoes/sneakers/black",
                    "/obj/item/storage/cans/sixenergydrink",
                ],
            ),
            Program::Loadout(
                "Archer",
                &[
                    "/obj/item/clothing/under/costume/kimono",
                    "/obj/item/clothing/shoes/sandal/alt",
                    "/obj/item/storage/bag/quiver/endless",
                    "/obj/item/gun/ballistic/bow/longbow",
                    "/obj/item/ammo_casing/arrow/holy/blazing",
                ],
            ),
            Program::Loadout(
                "Fisher",
                &[
                    "/obj/item/clothing/under/misc/overalls",
                    "/obj/item/clothing/suit/jacket/miljacket",
                    "/obj/item/clothing/head/soft/black",
                    "/obj/item/clothing/shoes/jackboots",
                    "/obj/item/storage/toolbox/fishing/small",
                    "/obj/item/bait_can/worm/premium",
                    "/obj/item/grenade/iedcasing/spawned",
                    "/obj/item/stock_parts/power_store/cell/lead",
                    "/obj/item/reagent_containers/cup/glass/bottle/beer/light",
                ],
            ),
            Program::Loadout(
                "Gamer",
                &[
                    "/obj/item/clothing/under/suit/black_really",
                    "/obj/item/clothing/shoes/laceup",
                    "/obj/item/clothing/gloves/color/black",
                    "/obj/item/clothing/glasses/sunglasses",
                    "/obj/item/modular_computer/laptop/gamer",
                    "/obj/item/storage/cans/sixgamerdrink",
                ],
            ),
        ],
    ),
    (
        "/obj/item/disk/bitrunning/gimmick/dungeon",
        &[
            Program::Loadout(
                "Alchemist",
                &[
                    "/obj/item/clothing/suit/bio_suit/plaguedoctorsuit",
                    "/obj/item/clothing/mask/gas/plaguedoctor",
                    "/obj/item/clothing/head/bio_hood/plague",
                    "/obj/item/storage/box/alchemist_basic_chems",
                    "/obj/item/storage/box/alchemist_basic_chems",
                    "/obj/item/storage/box/alchemist_random_chems",
                    "/obj/item/storage/box/alchemist_chemistry_kit",
                ],
            ),
            Program::Loadout(
                "Rogue",
                &[
                    "/obj/item/clothing/under/color/black",
                    "/obj/item/clothing/shoes/sneakers/black/rogue",
                    "/obj/item/clothing/mask/facescarf/rogue",
                    "/obj/item/clothing/glasses/eyepatch/rogue",
                    "/obj/item/bedsheet/black/rogue_cape",
                    "/obj/item/storage/belt/fannypack/black/rogue",
                    "/obj/item/knife/combat/survival",
                ],
            ),
            Program::Loadout(
                "Healer",
                &[
                    "/obj/item/clothing/under/costume/singer/yellow",
                    "/obj/item/clothing/shoes/singery",
                    "/obj/item/rod_of_asclepius",
                    "/obj/item/storage/medkit/surgery",
                    "/obj/item/emergency_bed",
                    "/obj/item/food/canned/larvae",
                    "/obj/item/reagent_containers/dropper",
                ],
            ),
            Program::Loadout(
                "Wizard",
                &[
                    "/obj/item/clothing/head/wizard/fake",
                    "/obj/item/clothing/suit/wizrobe/fake",
                    "/obj/item/clothing/glasses/eyepatch/medical/chuuni",
                    "/obj/item/staff",
                    "/obj/item/toy/eightball",
                    "/obj/item/storage/fancy/cigarettes/cigpack_cannabis",
                    "/obj/item/storage/box/matches",
                ],
            ),
        ],
    ),
];

/// Every type a disk may spawn, for `ss13_mapload::runtime::runtime_types`.
pub fn spawned_types() -> impl Iterator<Item = &'static str> {
    DISKS
        .iter()
        .flat_map(|(_, programs)| programs.iter())
        .flat_map(|p| match *p {
            Program::Item(path) => vec![path],
            Program::Ability(_) => Vec::new(),
            Program::Loadout(_, items) => items.to_vec(),
        })
        .chain([DIGITAL_STORAGE])
}

/// `choice_made`: the program index of a disk that was set.
#[derive(Component, Clone, Copy, Debug)]
pub struct DiskChoice(pub usize);

/// On a disk's open choice prompt: the disk.
#[derive(Component, Clone, Copy, Debug)]
struct DiskPrompt(Entity);

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::AttackSelf, 3377, |w, c| {
        attack_self(w, c.client, c.mob, c.target)
    })
    .add_systems(
        Update,
        choice_answers
            .in_set(ss13_tgui::server::UiSystems::Handle)
            .after(ss13_tgui::prompt::PromptSystems::Answer),
    );
}

/// The programs of a disk type: its own entry, or the closest parent's.
pub fn programs_of(path: &str) -> Option<&'static [Program]> {
    DISKS
        .iter()
        .find(|(p, _)| is_under(path, p))
        .map(|(_, programs)| *programs)
}

fn disk_programs(world: &World, disk: Entity) -> Option<&'static [Program]> {
    let path = &world.get::<DmItem>(disk)?.path;
    if !is_under(path, DISK_ROOT) {
        return None;
    }
    programs_of(path)
}

/// The choice list's name of a program (`initial(thing.name)`).
fn label(world: &World, program: Program) -> String {
    match program {
        Program::Item(path) => world
            .get_resource::<Prototypes>()
            .and_then(|p| p.get(path)?.get::<DmName>().cloned())
            .map_or_else(
                || path.rsplit('/').next().unwrap_or(path).to_owned(),
                |n| crate::vending::plain_name(&n.name).to_owned(),
            ),
        Program::Ability(path) => Spell::from_path(path)
            .map_or(path, |s| s.def().name)
            .to_owned(),
        Program::Loadout(name, _) => name.to_owned(),
    }
}

/// `disk/bitrunning/*/attack_self`: the choice list, once.
fn attack_self(world: &mut World, client: Entity, mob: Entity, disk: Entity) -> bool {
    let Some(programs) = disk_programs(world, disk) else {
        return false;
    };
    if world.get::<DiskChoice>(disk).is_some() {
        return true;
    }
    let gimmick = programs
        .first()
        .is_some_and(|p| matches!(p, Program::Loadout(..)));
    let items = programs.iter().map(|&p| label(world, p)).collect();
    let (title, message) = if gimmick {
        (
            "Программа битрана",
            "Выберите тематический набор снаряжения",
        )
    } else {
        ("Программа битраннинга", "Выберите способность")
    };
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            message,
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(DiskPrompt(disk));
    true
}

/// The choice, checked when it comes: the user still holds the disk and it is not set yet.
fn choice_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(DiskPrompt(disk)) = world.get::<DiskPrompt>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let held = world
            .get::<Inventory>(actor)
            .is_some_and(|inv| inv.hands.contains(&Some(disk)));
        let Some(programs) = disk_programs(world, disk) else {
            continue;
        };
        if !held || i >= programs.len() || world.get::<DiskChoice>(disk).is_some() {
            continue;
        }
        world.entity_mut(disk).insert(DiskChoice(i));
        let text = if matches!(programs[i], Program::Loadout(..)) {
            "selected"
        } else {
            "выбрано"
        };
        crate::balloon::balloon_alert(world, actor, disk, text);
        if let Some(&pos) = world.get::<TilePos>(actor) {
            crate::sound::playsound(world, crate::sound::PlaySound::new(pos, CLICK, 50.0, true));
        }
    }
}

/// Everything `mob` carries: held, worn and nested in containers (tg `get_nested_locs` from
/// the disk's side).
fn carried(world: &World, mob: Entity) -> Vec<Entity> {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return Vec::new();
    };
    let mut out: Vec<Entity> = inventory
        .hands
        .iter()
        .flatten()
        .copied()
        .chain(inventory.equipped())
        .collect();
    let mut i = 0;
    while i < out.len() {
        if let Some(stored) = world.get::<Stored>(out[i]) {
            out.extend(stored.items().iter().copied());
        }
        i += 1;
    }
    out
}

/// What [`stock_gear`] reports (`BITRUNNER_GEAR_LOAD_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadResult {
    pub failed: bool,
}

/// `stock_gear`'s `COMSIG_BITRUNNER_STOCKING_GEAR`: every disk `neo` carries loads onto the new
/// `avatar`.
pub fn stock_gear(world: &mut World, avatar: Entity, neo: Entity) -> LoadResult {
    let disks: Vec<(Entity, &'static [Program])> = carried(world, neo)
        .into_iter()
        .filter_map(|e| Some((e, disk_programs(world, e)?)))
        .collect();
    let mut result = LoadResult::default();
    for (disk, programs) in disks {
        let Some(program) = world
            .get::<DiskChoice>(disk)
            .and_then(|c| programs.get(c.0))
            .copied()
        else {
            result.failed = true;
            continue;
        };
        if !load(world, avatar, program) {
            result.failed = true;
        }
    }
    if result.failed {
        tell_mob(
            world,
            neo,
            Span::Warning,
            "Как минимум один из ваших внешних хранителей информации получил ошибку при загрузке. \
             Проверьте, нет ли перекрывающихся или неактивных дисков.",
        );
    }
    result
}

/// `load_onto_avatar` of one program; false: `BITRUNNER_GEAR_LOAD_FAILED`.
fn load(world: &mut World, avatar: Entity, program: Program) -> bool {
    match program {
        Program::Item(path) => {
            if let Some(item) = spawn(world, path) {
                to_hands(world, avatar, item);
            }
            true
        }
        Program::Ability(path) => {
            let Some(spell) = Spell::from_path(path) else {
                return false;
            };
            if crate::spells::knows(world, avatar, spell) {
                return false;
            }
            crate::spells::learn(world, avatar, spell);
            true
        }
        Program::Loadout(name, items) => {
            let Some(case) = spawn(world, DIGITAL_STORAGE) else {
                return true;
            };
            // `"[LOWER_TEXT(name)]'s [initial(container_item.name)]"`.
            let base = world
                .get::<DmName>(case)
                .map(|n| crate::vending::plain_name(&n.name).to_owned())
                .unwrap_or_default();
            if let Some(mut n) = world.get_mut::<DmName>(case) {
                n.name = format!("{}'s {base}", name.to_lowercase());
            }
            for &path in items {
                if let Some(item) = spawn(world, path) {
                    crate::storage::put_inside(world, item, case);
                }
            }
            to_hands(world, avatar, case);
            true
        }
    }
}

fn spawn(world: &mut World, path: &str) -> Option<Entity> {
    let item = crate::construction::spawn_detached(world, path)?;
    world.entity_mut(item).insert(Networked);
    Some(item)
}

fn to_hands(world: &mut World, avatar: Entity, item: Entity) {
    let Some(inventory) = world.get::<Inventory>(avatar).cloned() else {
        return;
    };
    crate::storage::put_in_hands(world, avatar, &inventory, item);
    if world.get::<CarriedBy>(item).is_some() {
        crate::inventory::refresh_look(world, avatar);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_types_find_their_own_programs() {
        // The premium mods are subtypes of the plain ones and come first.
        assert_eq!(
            programs_of("/obj/item/disk/bitrunning/item/pka_mods/premium").map(<[Program]>::len),
            Some(5)
        );
        assert_eq!(
            programs_of("/obj/item/disk/bitrunning/item/pka_mods").and_then(|p| p.first().copied()),
            Some(Program::Item("/obj/item/borg/upgrade/modkit/range"))
        );
        assert!(programs_of("/obj/item/disk/bitrunning").is_none());
        // Every ability is a ported spell.
        for (_, programs) in DISKS {
            for p in *programs {
                if let Program::Ability(path) = p {
                    assert!(Spell::from_path(path).is_some(), "{path}");
                }
            }
        }
        let runtime = ss13_mapload::runtime::runtime_types();
        for path in spawned_types() {
            assert!(runtime.contains(path), "{path} missing from runtime_types");
        }
    }
}
