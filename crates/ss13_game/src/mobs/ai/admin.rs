//! Admin "Give AI Controller", after tg `ADMIN_VERB(give_ai_controller)` and
//! `/datum/admin_ai_template` (`code/datums/ai/basic_mobs/admin_ai_templates.dm`): a template
//! replaces the controller of the living mobs on the admin's tile, optionally with the hostile
//! faction; "Reset" gives back the mob's own controller, "None" removes it.
// shortcut: no "active while a client controls it" (player mobs stay off), no idle walk chance
// or minimum target stat prompts (the templates keep their trees' values), no ability or goon
// templates (their trees are not ported); basic mobs only (the trees attack through the basic
// mob melee), carbons take the monkey controller via "Monkey".

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

use super::{AiController, Blackboard, Retaliates, bt, trees};
use crate::health::Health;
use crate::mobs::{BasicMob, Faction};

/// `FACTION_HOSTILE`.
const HOSTILE: &str = "hostile";
const SIMPLE: &str = "/datum/ai_controller/basic_controller/simple";

/// tg template `name` -> controller (`None`: "Reset" / "None" / "Monkey").
pub const TEMPLATES: &[(&str, Option<&str>)] = &[
    ("Hostile Melee", Some("simple_hostile_obstacles")),
    ("Hostile Ranged", Some("simple_ranged")),
    ("Hostile Ranged/Melee", Some("simple_skirmisher")),
    ("Passive But Fights Back (Melee)", Some("simple_retaliate")),
    (
        "Passive But Fights Back (Ranged Attacks)",
        Some("simple_ranged_retaliate"),
    ),
    ("Gets Mad Unpredictably", Some("simple_capricious")),
    ("Runs Away", Some("simple_fearful")),
    ("Runs Away From Attackers", Some("simple_skittish")),
    ("Monkey", None),
    ("Reset", None),
    ("None", None),
];

/// `apply_controller` on every living mob on `pos` but `except` (the admin's own body):
/// the number changed, or why nothing was.
pub fn give(
    world: &mut World,
    pos: TilePos,
    except: Entity,
    template: &str,
    make_hostile: bool,
) -> Result<usize, String> {
    let Some(&(name, controller)) = TEMPLATES.iter().find(|(n, _)| *n == template) else {
        return Err(format!("Нет шаблона ИИ «{template}»."));
    };
    let mobs: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| e != except && world.get::<Health>(e).is_some())
        .collect();
    if mobs.is_empty() {
        return Err("На вашем тайле нет живых существ.".into());
    }
    let mut changed = 0;
    for mob in mobs {
        let basic = world.get::<BasicMob>(mob).map(|b| b.def.clone());
        let new = match (name, controller, &basic) {
            ("None", _, _) => None,
            ("Monkey", _, None) => Some(super::monkey::controller(super::monkey::MONKEY)),
            ("Reset", _, Some(def)) => AiController::for_mob(def),
            ("Reset", _, None) => None,
            (_, Some(kind), Some(def)) => {
                let path = format!("{SIMPLE}/{kind}");
                trees::build(&path, def).map(|built| AiController {
                    kind: path,
                    tree: bt::Tree::new(built.roots),
                    bb: Blackboard(built.blackboard.into_iter().collect()),
                    traits: built.traits,
                    retaliates: built.retaliates,
                    ..AiController::default()
                })
            }
            // A basic mob template on a carbon, the monkey on a basic mob.
            _ => continue,
        };
        // `QDEL_NULL(target.ai_controller)`.
        world.entity_mut(mob).remove::<(AiController, Retaliates)>();
        if let Some(controller) = new {
            if controller.retaliates {
                world.entity_mut(mob).insert(Retaliates);
            }
            world.entity_mut(mob).insert(controller);
        }
        if make_hostile && name != "None" && name != "Reset" {
            world
                .entity_mut(mob)
                .insert(Faction(vec![HOSTILE.to_owned()]));
        }
        changed += 1;
    }
    if changed == 0 {
        return Err(format!("«{name}» не подходит существам на тайле."));
    }
    Ok(changed)
}
