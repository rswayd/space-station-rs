//! Bot construction (server), after tg `code/modules/mob/living/simple_animal/bot/
//! construction.dm` and the items that start an assembly (`cup/bucket/item_interaction`,
//! `helmet/sec/item_interaction`, `medkit/tool_act`, `extinguisher/item_interaction`).
//!
//! - Cleanbot: a proximity sensor on a bucket gives the assembly (in hand); a robot arm on it
//!   builds the cleanbot.
//! - Medbot: a robot arm on an empty medkit gives the assembly (the kit's skin and healing type);
//!   a health analyzer, then a proximity sensor build the medbot.
//! - Securitron: a signaler on a security helmet gives the assembly; then a welder (a hole), a
//!   proximity sensor, a robot arm and a security baton build it. A screwdriver takes the last
//!   part back off at each step (the signaler and helmet at the first), a welder closes the hole.
//! - Firebot: a robot arm on an extinguisher gives the assembly; a red hardhat, then a proximity
//!   sensor build it.
//!
//! The built bot appears where the assembly was and takes the assembly's name. Each step checks
//! the held item and the assembly's step at the moment of effect; consumed parts are deleted.
// shortcut: no ED-209, honkbot, hygienebot, repairbot or Vim assemblies, no General Beepsky
// swords, no renaming assemblies with a pen.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};

use super::{Bot, Healer};
use crate::health::DamageType;
use crate::inventory::{CarriedBy, Inventory};
use crate::tools::{ToolKind, tell};

pub const CLEANBOT_ASSEMBLY: &str = "/obj/item/bot_assembly/cleanbot";
pub const MEDBOT_ASSEMBLY: &str = "/obj/item/bot_assembly/medbot";
pub const SECBOT_ASSEMBLY: &str = "/obj/item/bot_assembly/secbot";
pub const FIREBOT_ASSEMBLY: &str = "/obj/item/bot_assembly/firebot";
const BUCKET: &str = "/obj/item/reagent_containers/cup/bucket";
const PROX: &str = "/obj/item/assembly/prox_sensor";
const SIGNALER: &str = "/obj/item/assembly/signaler";
const SEC_HELMET: &str = "/obj/item/clothing/head/helmet/sec";
const MEDKIT: &str = "/obj/item/storage/medkit";
const HEALTH_ANALYZER: &str = "/obj/item/healthanalyzer";
const EXTINGUISHER: &str = "/obj/item/extinguisher";
const RED_HARDHAT: &str = "/obj/item/clothing/head/utility/hardhat/red";
const SEC_BATON: &str = "/obj/item/melee/baton/security";
const ARMS: [&str; 2] = [
    "/obj/item/bodypart/arm/left/robot",
    "/obj/item/bodypart/arm/right/robot",
];
const AIBOTS: &str = "icons/mob/silicon/aibots.dmi";

/// An assembly's progress (tg `build_step`, `created_name` and the kind's vars).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct BotAssembly {
    pub step: u8,
    pub created_name: String,
    /// Medbot: the kit's skin and type.
    pub skin: String,
    pub medkit: String,
    pub robot_arm: String,
}

pub(super) fn build(app: &mut App) {
    app.add_observer(add_assembly_state);
}

fn add_assembly_state(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let name = if is_under(&item.path, CLEANBOT_ASSEMBLY) {
        "Cleanbot"
    } else if is_under(&item.path, MEDBOT_ASSEMBLY) {
        "Medibot"
    } else if is_under(&item.path, SECBOT_ASSEMBLY) {
        "Securitron"
    } else if is_under(&item.path, FIREBOT_ASSEMBLY) {
        "Firebot"
    } else {
        return;
    };
    commands.entity(add.entity).insert_if_new(BotAssembly {
        step: 1,
        created_name: name.to_owned(),
        skin: "generic".to_owned(),
        medkit: MEDKIT.to_owned(),
        robot_arm: super::ROBOT_ARM.to_owned(),
    });
}

fn path(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

fn is_arm(path: &str) -> bool {
    ARMS.iter().any(|a| is_under(path, a))
}

/// tg `get_medbot_skin` and `damagetype_healed` of a medkit type.
pub fn medkit_skin(path: &str) -> (&'static str, Healer) {
    const KITS: [(&str, &str, Healer); 8] = [
        ("/surgery_syndie", "bezerk", Healer::All),
        ("/fire", "burn", Healer::Type(DamageType::Burn)),
        ("/toxin", "tox", Healer::Type(DamageType::Tox)),
        ("/o2", "oxy", Healer::Type(DamageType::Oxy)),
        ("/brute", "brute", Healer::Type(DamageType::Brute)),
        ("/advanced", "adv", Healer::All),
        ("/tactical_lite", "bezerk", Healer::All),
        ("/tactical", "bezerk", Healer::All),
    ];
    KITS.iter()
        .find(|(sub, ..)| is_under(path, &format!("{MEDKIT}{sub}")))
        .map_or(
            ("generic", Healer::Type(DamageType::Brute)),
            |&(_, skin, healer)| (skin, healer),
        )
}

/// A held `tool` used on `target` (on the floor or in the other hand). True when it was a
/// bot construction step (or refused as one).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tool: Entity,
    target: Entity,
) -> bool {
    let (tool_path, target_path) = (path(world, tool), path(world, target));
    if target_path.is_empty() || tool == target {
        return false;
    }
    let ctx = Ctx {
        client,
        mob,
        tool,
        target,
    };
    if is_under(&target_path, BUCKET) && is_under(&tool_path, PROX) {
        return start(world, ctx, CLEANBOT_ASSEMBLY, None);
    }
    if is_under(&target_path, SEC_HELMET) && is_under(&tool_path, SIGNALER) {
        return start(world, ctx, SECBOT_ASSEMBLY, None);
    }
    if is_under(&target_path, EXTINGUISHER) && is_arm(&tool_path) {
        return start(world, ctx, FIREBOT_ASSEMBLY, None);
    }
    if is_under(&target_path, MEDKIT) && is_arm(&tool_path) {
        let empty = world
            .get::<crate::storage::Stored>(target)
            .is_none_or(|s| s.items().is_empty());
        if !empty {
            tell(world, client, Span::Warning, "Внутри есть предметы!");
            return true;
        }
        return start(world, ctx, MEDBOT_ASSEMBLY, Some(target_path));
    }
    let Some(assembly) = world.get::<BotAssembly>(target).cloned() else {
        return false;
    };
    if is_under(&target_path, CLEANBOT_ASSEMBLY) {
        return is_arm(&tool_path)
            && finish(world, ctx, "/mob/living/basic/bot/cleanbot", &assembly);
    }
    if is_under(&target_path, MEDBOT_ASSEMBLY) {
        return medbot_step(world, ctx, &assembly, &tool_path);
    }
    if is_under(&target_path, SECBOT_ASSEMBLY) {
        return secbot_step(world, ctx, &assembly, &tool_path);
    }
    if is_under(&target_path, FIREBOT_ASSEMBLY) {
        return firebot_step(world, ctx, &assembly, &tool_path);
    }
    false
}

#[derive(Clone, Copy)]
struct Ctx {
    client: Entity,
    mob: Entity,
    tool: Entity,
    target: Entity,
}

/// Where the target is: its tile, or its holder's and the hand it is in.
fn place_of(world: &World, ctx: Ctx) -> Option<(TilePos, Option<usize>)> {
    if let Some(&pos) = world.get::<TilePos>(ctx.target) {
        return Some((pos, None));
    }
    let holder = world.get::<CarriedBy>(ctx.target)?.0;
    let pos = *world.get::<TilePos>(holder)?;
    let hand = world
        .get::<Inventory>(holder)
        .and_then(|i| i.hands.iter().position(|&h| h == Some(ctx.target)));
    Some((pos, hand.filter(|_| holder == ctx.mob)))
}

/// The held part is used up.
fn consume(world: &mut World, item: Entity) {
    if world.get_entity(item).is_ok() {
        world.despawn(item);
    }
}

/// The first step: `tool` and `target` become a new assembly of `kind` in the target's hand
/// (or on its tile).
fn start(world: &mut World, ctx: Ctx, kind: &str, medkit: Option<String>) -> bool {
    let Some((pos, hand)) = place_of(world, ctx) else {
        return true;
    };
    let tool_name = crate::security::name_of(world, ctx.tool);
    consume(world, ctx.tool);
    consume(world, ctx.target);
    let Some(assembly) = crate::construction::spawn_obj(world, kind, pos) else {
        return true;
    };
    world.flush();
    if let Some(kit) = medkit {
        let (skin, _) = medkit_skin(&kit);
        if let Some(mut a) = world.get_mut::<BotAssembly>(assembly) {
            a.skin = skin.to_owned();
            a.medkit = kit;
        }
        set_icon(world, assembly, &format!("medbot_assembly_{skin}"));
    }
    if let Some(hand) = hand
        && let Some(inventory) = world.get::<Inventory>(ctx.mob).cloned()
        && inventory.hands[hand].is_none()
    {
        world
            .entity_mut(assembly)
            .remove::<TilePos>()
            .insert(CarriedBy(ctx.mob));
        world.get_mut::<Inventory>(ctx.mob).expect("cloned").hands[hand] = Some(assembly);
    }
    tell(
        world,
        ctx.client,
        Span::Notice,
        format!("Вы добавляете {tool_name}."),
    );
    true
}

fn set_icon(world: &mut World, e: Entity, state: &str) {
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(AIBOTS, state))
    else {
        return;
    };
    if let Some(mut look) = world.get_mut::<Appearance>(e) {
        look.icon_state = id;
    }
}

fn overlay(world: &mut World, e: Entity, state: &str, on: bool) {
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(AIBOTS, state))
    else {
        return;
    };
    if let Some(mut look) = world.get_mut::<Appearance>(e) {
        look.overlays.retain(|o| o.icon_state != id);
        if on {
            look.overlays.push(Overlay::new(id));
        }
    }
}

fn set_step(world: &mut World, e: Entity, step: u8) {
    if let Some(mut a) = world.get_mut::<BotAssembly>(e) {
        a.step = step;
    }
}

/// A secbot assembly at `step` with its overlays (the dying secbot's drop).
pub fn set_secbot_step(world: &mut World, assembly: Entity, step: u8, name: &str) {
    world.flush();
    set_step(world, assembly, step);
    if let Some(mut a) = world.get_mut::<BotAssembly>(assembly) {
        a.created_name = name.to_owned();
    }
    overlay(world, assembly, "hs_hole", step >= 2);
}

/// A part added: it is used up, the assembly moves on.
fn add_part(world: &mut World, ctx: Ctx, step: u8) -> bool {
    let tool_name = crate::security::name_of(world, ctx.tool);
    consume(world, ctx.tool);
    set_step(world, ctx.target, step);
    tell(
        world,
        ctx.client,
        Span::Notice,
        format!("Вы добавляете {tool_name}."),
    );
    true
}

/// A part comes back off onto the assembly's tile.
fn drop_part(world: &mut World, ctx: Ctx, part: &str) {
    if let Some((pos, _)) = place_of(world, ctx) {
        crate::construction::spawn_obj(world, part, pos);
    }
}

/// The finished bot where the assembly was, named after it.
fn finish(world: &mut World, ctx: Ctx, bot_path: &str, assembly: &BotAssembly) -> bool {
    let Some((pos, _)) = place_of(world, ctx) else {
        return true;
    };
    consume(world, ctx.tool);
    consume(world, ctx.target);
    let Some(bot) = crate::construction::spawn_obj(world, bot_path, pos) else {
        return true;
    };
    // The basic mob and its bot attach through queued commands.
    world.flush();
    world
        .entity_mut(bot)
        .insert(Name::new(assembly.created_name.clone()));
    if let Some(mut b) = world.get_mut::<Bot>(bot) {
        b.robot_arm = assembly.robot_arm.clone();
        if bot_path.ends_with("/medbot") {
            let (skin, healer) = medkit_skin(&assembly.medkit);
            b.skin = skin.to_owned();
            b.healer = healer;
            b.medkit = assembly.medkit.clone();
        }
    }
    super::refresh_look(world, bot);
    tell(world, ctx.client, Span::Notice, "Бип-буп!");
    true
}

fn tool_kind(world: &World, item: Entity) -> Option<ToolKind> {
    crate::tools::tool_of(world, item).map(|t| t.0)
}

/// `use_tool(src, user, 0)` of a welder: lit, nothing spent.
fn welder_ready(world: &mut World, ctx: Ctx) -> bool {
    if crate::tools::tool_usable(world, ctx.tool, 0, 0.0) {
        return true;
    }
    let text = crate::tools::unusable_message(world, ctx.tool, 0);
    tell(world, ctx.client, Span::Warning, text);
    false
}

fn medbot_step(world: &mut World, ctx: Ctx, a: &BotAssembly, tool: &str) -> bool {
    match a.step {
        1 if is_under(tool, HEALTH_ANALYZER) => {
            add_part(world, ctx, 2);
            overlay(world, ctx.target, "medbot_assembly_analyzer", true);
            true
        }
        2 if is_under(tool, PROX) => finish(world, ctx, "/mob/living/basic/bot/medbot", a),
        _ => false,
    }
}

fn firebot_step(world: &mut World, ctx: Ctx, a: &BotAssembly, tool: &str) -> bool {
    match a.step {
        1 if is_under(tool, RED_HARDHAT) => {
            add_part(world, ctx, 2);
            set_icon(world, ctx.target, "firebot_helmet");
            true
        }
        2 if is_under(tool, PROX) => finish(world, ctx, "/mob/living/basic/bot/firebot", a),
        _ => false,
    }
}

fn secbot_step(world: &mut World, ctx: Ctx, a: &BotAssembly, tool: &str) -> bool {
    let kind = tool_kind(world, ctx.tool);
    match (a.step, kind) {
        (1, Some(ToolKind::Welder)) => {
            if welder_ready(world, ctx) {
                set_step(world, ctx.target, 2);
                overlay(world, ctx.target, "hs_hole", true);
                tell(world, ctx.client, Span::Notice, "Вы прорезаете отверстие!");
            }
            true
        }
        (1, Some(ToolKind::Screwdriver)) => {
            drop_part(world, ctx, SIGNALER);
            drop_part(world, ctx, SEC_HELMET);
            consume(world, ctx.target);
            tell(
                world,
                ctx.client,
                Span::Notice,
                "Вы отсоединяете сигнализатор от шлема.",
            );
            true
        }
        (2, _) if is_under(tool, PROX) => {
            add_part(world, ctx, 3);
            overlay(world, ctx.target, "hs_eye", true);
            true
        }
        (2, Some(ToolKind::Welder)) => {
            if welder_ready(world, ctx) {
                set_step(world, ctx.target, 1);
                overlay(world, ctx.target, "hs_hole", false);
                tell(world, ctx.client, Span::Notice, "Вы заваривате отверстие!");
            }
            true
        }
        (3, _) if is_arm(tool) => {
            if let Some(mut asm) = world.get_mut::<BotAssembly>(ctx.target) {
                asm.robot_arm = tool.to_owned();
            }
            add_part(world, ctx, 4);
            overlay(world, ctx.target, "hs_arm", true);
            true
        }
        (3, Some(ToolKind::Screwdriver)) => {
            drop_part(world, ctx, PROX);
            set_step(world, ctx.target, 2);
            overlay(world, ctx.target, "hs_eye", false);
            true
        }
        (4, _) if is_under(tool, SEC_BATON) => {
            finish(world, ctx, "/mob/living/basic/bot/secbot", a)
        }
        (4, Some(ToolKind::Screwdriver)) => {
            drop_part(world, ctx, &a.robot_arm);
            set_step(world, ctx.target, 3);
            overlay(world, ctx.target, "hs_arm", false);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medkits_give_their_skins_and_healing() {
        assert_eq!(medkit_skin("/obj/item/storage/medkit/fire").0, "burn");
        assert_eq!(
            medkit_skin("/obj/item/storage/medkit/toxin").1,
            Healer::Type(DamageType::Tox)
        );
        assert_eq!(
            medkit_skin("/obj/item/storage/medkit/advanced"),
            ("adv", Healer::All)
        );
        assert_eq!(medkit_skin("/obj/item/storage/medkit/regular").0, "generic");
        // `tactical_lite` is not under `tactical`.
        assert_eq!(
            medkit_skin("/obj/item/storage/medkit/tactical_lite").0,
            "bezerk"
        );
    }
}
