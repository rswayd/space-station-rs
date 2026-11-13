//! Cyborg upgrade boards (server), after tg `/obj/item/borg/upgrade`
//! (`code/game/objects/items/robot/robot_upgrades.dm`) and the robot's `apply_upgrade`,
//! `add_to_upgrades`:
//!
//! - A board clicked on an open chassis (`crate::silicon::chassis`) is checked (a model when
//!   it needs one, not `locked`, alive, a mounting point of its `model_type`, no duplicate),
//!   then its `items_to_add` go into the model and its `items_to_remove` leave it. A one-use
//!   board is used up, the others stay inside the cyborg.
//! - `transform` boards change the model to their `new_model` (when it is one of the pickable
//!   models); the illegal equipment module (`syndicate`) emags the cyborg.
// shortcut: boards whose own `action` does more than move items are refused with "Upgrade
// error." (they stay in hand): rename, disabler cooler, ion thrusters, lavaproof chassis,
// self-repair, the hypospray, piercing and omnitool upgrades, the crew pinpointer's monitor,
// B.O.R.I.S., the expander and the kinetic accelerator modkits. No `deactivate`: boards come
// out only with the model reset, which is not ported.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::silicon::{BorgUpgrade, UPGRADE_ROOT};
use ss13_shared::chat::Span;

use super::Silicons;
use super::chassis::{Chassis, name_of, tell_mob};
use super::cyborg::{Cyborg, RobotModule};
use crate::health::Stat;
use crate::tools::Actor;

/// Boards with an unported `action` override.
const UNPORTED: [&str; 13] = [
    "/obj/item/borg/upgrade/rename",
    "/obj/item/borg/upgrade/disablercooler",
    "/obj/item/borg/upgrade/thrusters",
    "/obj/item/borg/upgrade/lavaproof",
    "/obj/item/borg/upgrade/selfrepair",
    "/obj/item/borg/upgrade/hypospray",
    "/obj/item/borg/upgrade/piercing_hypospray",
    "/obj/item/borg/upgrade/surgery_omnitool",
    "/obj/item/borg/upgrade/engineering_omnitool",
    "/obj/item/borg/upgrade/pinpointer",
    "/obj/item/borg/upgrade/ai",
    "/obj/item/borg/upgrade/expand",
    "/obj/item/borg/upgrade/modkit",
];
const TRANSFORM: &str = "/obj/item/borg/upgrade/transform";
const SYNDICATE: &str = "/obj/item/borg/upgrade/syndicate";

fn path_of(world: &World, item: Entity) -> Option<String> {
    world.get::<DmItem>(item).map(|i| i.path.clone())
}

/// `istype(tool, /obj/item/borg/upgrade)`.
pub fn is_upgrade(world: &World, item: Entity) -> bool {
    path_of(world, item).is_some_and(|p| is_under(&p, UPGRADE_ROOT))
}

/// Why `action` refuses before doing anything (its messages to the user and to the cyborg).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// "choose a model first!"
    NoModel,
    /// "upgrade locked!"
    Locked,
    Dead,
    NoMountingPoint,
    Occupied,
    /// "Upgrade error."
    Error,
}

/// The checks of tg `item_interaction` and `action` for `def` on a cyborg of `model` (its
/// path, `None`: the default model) with `installed` boards (paths).
pub fn check(
    def: &BorgUpgrade,
    model: Option<&str>,
    dead: bool,
    installed: &[String],
) -> Result<(), Refusal> {
    if model.is_none() && def.require_model {
        return Err(Refusal::NoModel);
    }
    if def.locked {
        return Err(Refusal::Locked);
    }
    if dead {
        return Err(Refusal::Dead);
    }
    if !def.model_type.is_empty()
        && !model.is_some_and(|m| def.model_type.iter().any(|t| is_under(m, t)))
    {
        return Err(Refusal::NoMountingPoint);
    }
    if !def.allow_duplicates && installed.contains(&def.path) {
        return Err(Refusal::Occupied);
    }
    if UNPORTED.iter().any(|root| is_under(&def.path, root)) {
        return Err(Refusal::Error);
    }
    Ok(())
}

/// tg robot `item_interaction` with an upgrade, then `apply_upgrade`.
pub fn try_install(world: &mut World, actor: Actor, item: Entity, borg: Entity) {
    let user = actor.client;
    let Some(chassis) = world.get::<Chassis>(borg).cloned() else {
        return;
    };
    if !chassis.opened {
        crate::tools::tell(world, user, Span::Warning, "chassis cover is closed!");
        return;
    }
    let Some(path) = path_of(world, item) else {
        return;
    };
    // A board missing from `silicon.ron` (an old conversion) has no known rules.
    let Some(def) = world.resource::<Silicons>().0.upgrade(&path).cloned() else {
        crate::tools::tell(world, user, Span::Danger, "Upgrade error.");
        return;
    };
    let Some(cyborg) = world.get::<Cyborg>(borg).cloned() else {
        return;
    };
    let model_path = cyborg
        .model
        .and_then(|i| world.resource::<Silicons>().0.models.get(i))
        .map(|m| m.path.clone());
    let installed: Vec<String> = chassis
        .upgrades
        .iter()
        .filter_map(|&u| path_of(world, u))
        .collect();
    let dead = world.get::<Stat>(borg) == Some(&Stat::Dead);
    let borg_name = name_of(world, borg);
    let refusal = check(&def, model_path.as_deref(), dead, &installed)
        .err()
        .or_else(|| custom_refusal(world, &def, &cyborg, &chassis));
    if let Some(refusal) = refusal {
        let item_name = name_of(world, item);
        match refusal {
            Refusal::NoModel => {
                crate::tools::tell(world, user, Span::Warning, "choose a model first!");
            }
            Refusal::Locked => crate::tools::tell(world, user, Span::Warning, "upgrade locked!"),
            Refusal::Dead => crate::tools::tell(
                world,
                user,
                Span::Warning,
                format!("{item_name} will not function on a deceased cyborg!"),
            ),
            Refusal::NoMountingPoint => {
                tell_mob(
                    world,
                    borg,
                    Span::Warning,
                    "Upgrade mounting error! No suitable hardpoint detected.",
                );
                crate::tools::tell(
                    world,
                    user,
                    Span::Warning,
                    "There's no mounting point for the module!",
                );
            }
            Refusal::Occupied => {
                tell_mob(
                    world,
                    borg,
                    Span::Warning,
                    "Upgrade mounting error! Hardpoint already occupied!",
                );
                crate::tools::tell(
                    world,
                    user,
                    Span::Warning,
                    "The mounting point for the module is already occupied!",
                );
            }
            Refusal::Error => crate::tools::tell(world, user, Span::Danger, "Upgrade error."),
        }
        return;
    }
    action(world, &def, borg);
    crate::tools::tell(
        world,
        user,
        Span::Notice,
        format!("You apply the upgrade to {borg_name}."),
    );
    add_to_upgrades(world, borg, item, def.one_use);
    crate::tools::tell(world, user, Span::Notice, "upgrade installed");
}

/// What the ported `action` overrides refuse.
fn custom_refusal(
    world: &World,
    def: &BorgUpgrade,
    cyborg: &Cyborg,
    chassis: &Chassis,
) -> Option<Refusal> {
    if is_under(&def.path, SYNDICATE) && chassis.emagged {
        return Some(Refusal::Error);
    }
    if is_under(&def.path, TRANSFORM)
        && def
            .new_model
            .as_ref()
            .is_some_and(|m| model_index(world, m).is_none())
    {
        return Some(Refusal::Error);
    }
    // The items need a model to go into.
    let moves_items = !def.items_to_add.is_empty() || !def.items_to_remove.is_empty();
    (moves_items && cyborg.model_item.is_none()).then_some(Refusal::Error)
}

fn model_index(world: &World, path: &str) -> Option<usize> {
    world
        .resource::<Silicons>()
        .0
        .models
        .iter()
        .position(|m| m.path == path)
}

/// tg `action` after its checks: the items, then the ported overrides.
fn action(world: &mut World, def: &BorgUpgrade, borg: Entity) {
    if !def.items_to_add.is_empty() {
        install_items(world, borg, &def.items_to_add);
    }
    if !def.items_to_remove.is_empty() {
        remove_items(world, borg, &def.items_to_remove);
    }
    if is_under(&def.path, SYNDICATE) {
        super::chassis::set_emagged(world, borg, true);
    }
    if is_under(&def.path, TRANSFORM)
        && let Some(index) = def.new_model.as_deref().and_then(|m| model_index(world, m))
    {
        let model = world.resource::<Silicons>().0.models[index].clone();
        super::cyborg::transform_to(world, borg, index, &model);
    }
}

/// tg `install_items`: new modules into the model.
fn install_items(world: &mut World, borg: Entity, items: &[String]) {
    let Some(model) = world.get::<Cyborg>(borg).and_then(|c| c.model_item) else {
        return;
    };
    let Some(&pos) = world.get::<ss13_shared::TilePos>(borg) else {
        return;
    };
    for path in items {
        if let Some(module) = crate::construction::spawn_obj(world, path, pos) {
            world.entity_mut(module).insert(RobotModule);
            crate::storage::put_inside(world, module, model);
        }
    }
}

/// tg `remove_items`: `locate(type) in model.modules` (subtypes too), held ones included.
fn remove_items(world: &mut World, borg: Entity, items: &[String]) {
    let Some(model) = world.get::<Cyborg>(borg).and_then(|c| c.model_item) else {
        return;
    };
    super::cyborg::stow_held(world, borg, false);
    for root in items {
        let found = world.get::<crate::storage::Stored>(model).and_then(|s| {
            s.items().iter().copied().find(|&m| {
                world
                    .get::<DmItem>(m)
                    .is_some_and(|i| is_under(&i.path, root))
            })
        });
        if let Some(module) = found {
            world.entity_mut(module).despawn();
        }
    }
}

/// tg `add_to_upgrades`: a one-use board is used up, the others go inside the cyborg.
fn add_to_upgrades(world: &mut World, borg: Entity, item: Entity, one_use: bool) {
    let item_name = name_of(world, item);
    tell_mob(
        world,
        borg,
        Span::Notice,
        format!(
            "----------------\nNew hardware detected...Identified as \"{item_name}\"...Setup \
             complete.\n----------------"
        ),
    );
    if one_use {
        crate::modpc::silicon::logevent(
            world,
            borg,
            &format!("Firmware {item_name} run successfully."),
        );
        world.entity_mut(item).despawn();
        return;
    }
    crate::storage::put_inside(world, item, borg);
    if let Some(mut chassis) = world.get_mut::<Chassis>(borg) {
        chassis.upgrades.push(item);
    }
    crate::modpc::silicon::logevent(
        world,
        borg,
        &format!("Hardware {item_name} installed successfully."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(path: &str) -> BorgUpgrade {
        BorgUpgrade {
            path: path.into(),
            ..default()
        }
    }

    #[test]
    fn mounting_checks_follow_action() {
        const MINER: &str = "/obj/item/robot_model/miner";
        let drill = BorgUpgrade {
            require_model: true,
            model_type: vec![MINER.into()],
            ..board("/obj/item/borg/upgrade/diamond_drill")
        };
        assert_eq!(check(&drill, None, false, &[]), Err(Refusal::NoModel));
        assert_eq!(
            check(&drill, Some("/obj/item/robot_model/janitor"), false, &[]),
            Err(Refusal::NoMountingPoint)
        );
        assert_eq!(check(&drill, Some(MINER), true, &[]), Err(Refusal::Dead));
        assert_eq!(check(&drill, Some(MINER), false, &[]), Ok(()));
        assert_eq!(
            check(
                &drill,
                Some(MINER),
                false,
                std::slice::from_ref(&drill.path)
            ),
            Err(Refusal::Occupied)
        );
        let dup = BorgUpgrade {
            allow_duplicates: true,
            ..drill.clone()
        };
        assert_eq!(
            check(&dup, Some(MINER), false, std::slice::from_ref(&dup.path)),
            Ok(())
        );
        let locked = BorgUpgrade {
            locked: true,
            ..board("/obj/item/borg/upgrade/amop")
        };
        assert_eq!(check(&locked, None, false, &[]), Err(Refusal::Locked));
        assert_eq!(
            check(&board("/obj/item/borg/upgrade/rename"), None, false, &[]),
            Err(Refusal::Error)
        );
        assert_eq!(
            check(
                &board("/obj/item/borg/upgrade/hypospray/expanded"),
                None,
                false,
                &[]
            ),
            Err(Refusal::Error)
        );
    }
}
