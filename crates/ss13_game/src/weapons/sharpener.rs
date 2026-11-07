//! Whetstones (`/obj/item/sharpener`, `code/game/objects/items/sharpener.dm`): a sharp item
//! used on one gains `increment` force (up to `max`), wound bonus and throw force, becomes
//! edged and takes the stone's prefix; a two-handed item gains only wielded force
//! (`crate::weapons::two_handed::on_sharpen`) and throw force. An item is sharpened once, energy
//! weapons never, and the stone wears out after its `uses`.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName, DmWounding};
use ss13_shared::chat::Span;

use crate::tools::tell;
use crate::weapons::two_handed::{self, Sharpen};

/// `SHARP_EDGED`.
const SHARP_EDGED: u32 = 1;
const UNSHEATH: &str = "sound/items/unsheath.ogg";

/// A whetstone type's vars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stone {
    pub root: &'static str,
    pub uses: i32,
    pub increment: f32,
    pub max: f32,
    pub prefix: &'static str,
    pub requires_sharpness: bool,
}

/// `/obj/item/sharpener`, `/super`, `/cult` (`cult_items.dm`).
pub const STONES: [Stone; 3] = [
    Stone {
        root: "/obj/item/sharpener",
        uses: 1,
        increment: 4.0,
        max: 30.0,
        prefix: "sharpened",
        requires_sharpness: true,
    },
    Stone {
        root: "/obj/item/sharpener/super",
        uses: 1,
        increment: 200.0,
        max: 200.0,
        prefix: "super-sharpened",
        requires_sharpness: false,
    },
    Stone {
        root: "/obj/item/sharpener/cult",
        uses: 1,
        increment: 5.0,
        max: 40.0,
        prefix: "darkened",
        requires_sharpness: true,
    },
];

/// Uses a whetstone has left (`uses`, counted down from its type's).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoneUses(pub i32);

/// The item was sharpened (tg `force > initial(force)`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sharpened;

pub fn stone_of(path: &str) -> Option<&'static Stone> {
    STONES
        .iter()
        .filter(|s| is_under(path, s.root))
        .max_by_key(|s| s.root.len())
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map_or_else(|| "item".to_owned(), |n| n.name.clone())
}

/// tg `/obj/item/sharpener/item_interaction`: `tool` used on the whetstone `stone`. False when
/// `stone` is not one.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tool: Entity,
    stone: Entity,
) -> bool {
    let Some(spec) = world
        .get::<DmItem>(stone)
        .and_then(|d| stone_of(&d.path))
        .copied()
    else {
        return false;
    };
    let Some(dm) = world.get::<DmItem>(tool).cloned() else {
        return false;
    };
    let uses = world.get::<StoneUses>(stone).map_or(spec.uses, |u| u.0);
    let tool_name = name_of(world, tool);
    let sharpness = world.get::<DmWounding>(tool).map_or(0, |w| w.sharpness);
    let energy = is_under(&dm.path, ss13_mapload::melee::ENERGY_ROOT)
        || is_under(&dm.path, crate::weapons::energy::DUALSABER);
    let refusal = if uses == 0 {
        Some("The sharpening block is too worn to use again!".to_owned())
    } else if dm.force >= spec.max || dm.throwforce >= spec.max {
        Some(format!(
            "{tool_name} is much too powerful to sharpen further!"
        ))
    } else if spec.requires_sharpness && sharpness == 0 {
        Some("You can only sharpen items that are already sharp, such as knives!".to_owned())
    } else if energy {
        let stone_name = name_of(world, stone);
        Some(format!(
            "You don't think the {tool_name} will be the thing getting modified if you use it on the {stone_name}!"
        ))
    } else {
        None
    };
    if let Some(text) = refusal {
        tell(world, client, Span::Warning, text);
        return true;
    }
    // `COMSIG_ITEM_SHARPEN_ACT`: a two-handed item takes the bonus on its wielded force.
    let two_handed = match two_handed::on_sharpen(world, tool, spec.increment, spec.max) {
        Ok(()) => true,
        Err(Sharpen::NotTwoHanded) => false,
        Err(Sharpen::Maxed) => {
            tell(
                world,
                client,
                Span::Warning,
                format!("{tool_name} is much too powerful to sharpen further!"),
            );
            return true;
        }
        Err(Sharpen::Already) => {
            tell(
                world,
                client,
                Span::Warning,
                format!(
                    "{tool_name} has already been refined before. It cannot be sharpened further!"
                ),
            );
            return true;
        }
    };
    if !two_handed && world.get::<Sharpened>(tool).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            format!("{tool_name} has already been refined before. It cannot be sharpened further!"),
        );
        return true;
    }
    if let Some(mut item) = world.get_mut::<DmItem>(tool) {
        if !two_handed {
            item.force = (item.force + spec.increment).clamp(0.0, spec.max);
        }
        item.throwforce = (item.throwforce + spec.increment).clamp(0.0, spec.max);
    }
    let mut wounding = world.get::<DmWounding>(tool).cloned().unwrap_or_default();
    if !two_handed {
        wounding.wound_bonus += spec.increment;
    }
    wounding.sharpness = SHARP_EDGED;
    world.entity_mut(tool).insert((wounding, Sharpened));
    if let Some(mut name) = world.get_mut::<DmName>(tool) {
        name.name = format!("{} {}", spec.prefix, name.name);
    }
    let user = world
        .get::<Name>(mob)
        .map_or_else(|| "Someone".to_owned(), |n| n.as_str().to_owned());
    crate::chem::visible(
        world,
        mob,
        &format!("You sharpen {tool_name}, making it much more deadly than before."),
        &format!(
            "{user} sharpens {tool_name} with {}!",
            name_of(world, stone)
        ),
        Span::Notice,
    );
    if let Some(pos) = crate::explosions::turf_of(world, stone) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, UNSHEATH, 25.0, true),
        );
    }
    let left = uses - 1;
    world.entity_mut(stone).insert(StoneUses(left));
    if left == 0
        && let Some(mut name) = world.get_mut::<DmName>(stone)
    {
        // `update_name`, `desc = "[desc] At least, it used to."`.
        name.name = format!("worn out {}", name.name);
        name.desc = format!("{} At least, it used to.", name.desc);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_net::chat::server::SendChat;
    use ss13_shared::TilePos;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<crate::sound::PlaySound>>();
        world
    }

    /// A knife gains 4 force, 4 throw force and wound bonus once; the stone is then worn out.
    #[test]
    fn whetstone_sharpens_once() {
        let mut world = world();
        let client = world.spawn_empty().id();
        let mob = world.spawn(TilePos::new(1, 1, 1)).id();
        let stone = world
            .spawn((
                DmItem {
                    path: "/obj/item/sharpener".into(),
                    ..default()
                },
                DmName {
                    name: "whetstone".into(),
                    desc: "A block that makes things sharp.".into(),
                },
                TilePos::new(1, 1, 1),
            ))
            .id();
        let knife = world
            .spawn((
                DmItem {
                    path: "/obj/item/knife/kitchen".into(),
                    force: 10.0,
                    throwforce: 10.0,
                    ..default()
                },
                DmName {
                    name: "kitchen knife".into(),
                    desc: String::new(),
                },
                DmWounding {
                    sharpness: SHARP_EDGED,
                    ..default()
                },
            ))
            .id();
        let toolbox = world
            .spawn(DmItem {
                path: "/obj/item/storage/toolbox".into(),
                force: 12.0,
                ..default()
            })
            .id();
        assert!(item_interaction(&mut world, client, mob, toolbox, stone));
        assert_eq!(world.get::<DmItem>(toolbox).unwrap().force, 12.0);
        assert!(item_interaction(&mut world, client, mob, knife, stone));
        let dm = world.get::<DmItem>(knife).unwrap();
        assert_eq!((dm.force, dm.throwforce), (14.0, 14.0));
        assert_eq!(world.get::<DmWounding>(knife).unwrap().wound_bonus, 4.0);
        assert_eq!(
            world.get::<DmName>(knife).unwrap().name,
            "sharpened kitchen knife"
        );
        assert_eq!(world.get::<StoneUses>(stone), Some(&StoneUses(0)));
        assert_eq!(
            world.get::<DmName>(stone).unwrap().name,
            "worn out whetstone"
        );
        // Worn out: nothing more.
        assert!(item_interaction(&mut world, client, mob, knife, stone));
        assert_eq!(world.get::<DmItem>(knife).unwrap().force, 14.0);
    }
}
