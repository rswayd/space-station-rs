//! Golem shells (server), after tg `code/modules/mob_spawn/ghost_roles/golem_roles.dm` and
//! `/obj/item/golem_shell` (`code/modules/mapfluff/ruins/lavaland_ruin_code.dm`).
//!
//! - An incomplete shell (`/obj/item/golem_shell`; the servant one from xenobiology) takes ten
//!   of a mineral golems eat (`golem_stack_food_directory`) in 4 s and becomes the ghost role
//!   spawner of its `shell_type`, made by whoever fed it; the ghosts are told. A crowbar breaks
//!   it into one adamantine sheet in 1 s.
//! - A completed shell (`/obj/effect/mob_spawn/ghost_role/human/golem/*`, also placed on maps)
//!   is a ghost role of `crate::ghost_roles`: a human of the golem species. A crowbar takes it
//!   apart in 4 s: an adamantine sheet and five of what it was made of (iron when map placed).
//! - Taking it (`special`, `give_directive`, `name_mob`): a servant golem whose creator still
//!   lives is "Golem (N)", is told to serve the creator and joins the creator's factions; a free
//!   golem away from the station is told to build more shells.
// shortcut: no golem food buff from the shell's mineral (no golem mineral diet), no adamantine
// vocal cords (the golem radio), no `hazard_area` slowing free golems on the station, no
// enslavement into the creator's cult, revolution or nuclear team, no `set_cloned_appearance`.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mob_spawn::GOLEM_STACK_FOOD;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::click_handlers::{AddClickHandler, ClickCtx, ClickPhase};
use crate::construction::{Job, spawn_obj, spawn_stack};
use crate::ghost_verbs::GhostRoleSpawner;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, Stack, ToolKind, do_after, tell, tell_mob, tool_of, use_stack};

const SHELL_ITEM: &str = "/obj/item/golem_shell";
const SERVANT_SHELL_ITEM: &str = "/obj/item/golem_shell/servant";
pub const SHELL: &str = "/obj/effect/mob_spawn/ghost_role/human/golem";
const SERVANT_SHELL: &str = "/obj/effect/mob_spawn/ghost_role/human/golem/servant";
const ADAMANTINE: &str = "/obj/item/stack/sheet/mineral/adamantine";
const IRON: &str = "/obj/item/stack/sheet/iron";
const CROWBAR_SOUND: &str = "sound/items/tools/crowbar.ogg";
const BREAK_SOUND: &str = "sound/effects/rock/rock_break.ogg";

/// `required_stacks`.
const REQUIRED_STACKS: u32 = 10;
/// Feeding the shell, `do_after` deciseconds.
const FEED_DS: f32 = 40.0;
/// Breaking an incomplete shell.
const BREAK_DS: f32 = 10.0;
/// `deconstruct_time` of a completed shell.
const DECONSTRUCT_DS: f32 = 40.0;

/// On a completed shell made from an incomplete one: who fed it (`owner_ref` of the servant
/// shell) and with what (`initial_type`).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct GolemShell {
    pub creator: Option<Entity>,
    pub made_of: Option<&'static str>,
}

/// The timed steps of this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GolemJob {
    /// Minerals into an incomplete shell.
    Feed { shell: Entity, food: Entity },
    /// A crowbar on an incomplete shell.
    Break { shell: Entity },
    /// A crowbar on a completed shell.
    Deconstruct,
}

pub struct GolemsPlugin;

impl Plugin for GolemsPlugin {
    fn build(&self, app: &mut App) {
        app.add_click_handler(ClickPhase::HeldOnTarget, 4475, item_interaction)
            .add_click_handler(ClickPhase::OtherHand, 1405, item_interaction);
    }
}

/// The `golem_stack_food_directory` key a stack type falls under (`is_path_in_list` of its
/// `merge_type`), the most specific one.
pub fn stack_food(path: &str) -> Option<&'static str> {
    GOLEM_STACK_FOOD
        .into_iter()
        .filter(|food| is_under(path, food))
        .max_by_key(|food| food.len())
}

fn item_path(world: &World, e: Entity) -> Option<String> {
    world.get::<DmItem>(e).map(|i| i.path.clone())
}

fn spawner_path(world: &World, e: Entity) -> Option<String> {
    world.get::<GhostRoleSpawner>(e).and_then(|s| s.def.clone())
}

/// `golem_shell/item_interaction`, `golem_shell/crowbar_act`, `golem/crowbar_act`.
fn item_interaction(world: &mut World, ctx: &ClickCtx) -> bool {
    let Some(held) = ctx.held else {
        return false;
    };
    let actor = Actor {
        client: ctx.client,
        mob: ctx.mob,
        item: Some(held),
    };
    let crowbar = tool_of(world, held).is_some_and(|t| t.0 == ToolKind::Crowbar);
    // A completed shell (the effect is never held).
    if spawner_path(world, ctx.target).is_some_and(|p| is_under(&p, SHELL)) {
        // `if(user.combat_mode) return`.
        if !crowbar
            || world
                .get::<crate::combat::CombatMode>(ctx.mob)
                .is_some_and(|c| c.0)
        {
            return false;
        }
        if world.get::<crate::tools::DoAfter>(ctx.mob).is_some() {
            return true;
        }
        tell(
            world,
            ctx.client,
            Span::Notice,
            "Вы начинаете выламывать несущие куски из готовой оболочки.",
        );
        playsound(world, PlaySound::new(ctx.pos, CROWBAR_SOUND, 70.0, false));
        do_after(
            world,
            actor,
            ctx.target,
            DECONSTRUCT_DS,
            0,
            Job::Golem(GolemJob::Deconstruct),
        );
        return true;
    }
    if !item_path(world, ctx.target).is_some_and(|p| is_under(&p, SHELL_ITEM)) {
        return false;
    }
    let shell = ctx.target;
    // A shell in a hand has no tile: the timer follows the user instead.
    let timer_target = if world.get::<TilePos>(shell).is_some() {
        shell
    } else {
        ctx.mob
    };
    if crowbar {
        tell(
            world,
            ctx.client,
            Span::Notice,
            "Вы начинаете выбивать структурно важные куски.",
        );
        playsound(world, PlaySound::new(ctx.pos, CROWBAR_SOUND, 70.0, false));
        do_after(
            world,
            actor,
            timer_target,
            BREAK_DS,
            0,
            Job::Golem(GolemJob::Break { shell }),
        );
        return true;
    }
    if world.get::<Stack>(held).is_none() {
        return false;
    }
    let food = item_path(world, held).and_then(|p| stack_food(&p));
    if food.is_none() {
        crate::balloon::balloon_alert(world, ctx.mob, shell, "неподходящий минерал!");
        return true;
    }
    if crate::tools::stack_amount(world, held) < REQUIRED_STACKS {
        crate::balloon::balloon_alert(world, ctx.mob, shell, "недостаточно минералов!");
        return true;
    }
    do_after(
        world,
        actor,
        timer_target,
        FEED_DS,
        0,
        Job::Golem(GolemJob::Feed { shell, food: held }),
    );
    true
}

/// End of a [`GolemJob`]; the shell and the minerals are checked again here.
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: GolemJob) {
    let client = action.client;
    match job {
        GolemJob::Feed { shell, food } => {
            let Some(shell_path) = item_path(world, shell).filter(|p| is_under(p, SHELL_ITEM))
            else {
                return;
            };
            let Some(made_of) = item_path(world, food).and_then(|p| stack_food(&p)) else {
                return;
            };
            let Some(pos) = turf_of(world, shell, mob) else {
                return;
            };
            if !use_stack(world, food, REQUIRED_STACKS) {
                crate::balloon::balloon_alert(world, mob, shell, "недостаточно минералов!");
                return;
            }
            let shell_type = if is_under(&shell_path, SERVANT_SHELL_ITEM) {
                SERVANT_SHELL
            } else {
                SHELL
            };
            world.despawn(shell);
            complete_shell(world, shell_type, pos, Some(mob), Some(made_of));
        }
        GolemJob::Break { shell } => {
            if !item_path(world, shell).is_some_and(|p| is_under(&p, SHELL_ITEM)) {
                return;
            }
            let Some(pos) = turf_of(world, shell, mob) else {
                return;
            };
            world.despawn(shell);
            spawn_stack(world, ADAMANTINE, pos, 1);
            tell(world, client, Span::Notice, "Оболочка разваливается!");
            playsound(world, PlaySound::new(pos, BREAK_SOUND, 40.0, false));
        }
        GolemJob::Deconstruct => {
            let shell = action.target;
            if !spawner_path(world, shell).is_some_and(|p| is_under(&p, SHELL)) {
                return;
            }
            let pos = action.target_pos;
            let made_of = world.get::<GolemShell>(shell).and_then(|s| s.made_of);
            world.despawn(shell);
            spawn_stack(world, ADAMANTINE, pos, 1);
            spawn_stack(world, made_of.unwrap_or(IRON), pos, 5);
            tell(world, client, Span::Notice, "Голем рассыпается!");
            playsound(world, PlaySound::new(pos, BREAK_SOUND, 60.0, false));
        }
    }
}

/// `get_turf(src)` of a shell on the floor or in `mob`'s hands.
fn turf_of(world: &World, shell: Entity, mob: Entity) -> Option<TilePos> {
    world
        .get::<TilePos>(shell)
        .or_else(|| world.get::<TilePos>(mob))
        .copied()
}

/// `new shell_type(loc, creator, made_of)`: the ghost role spawner, the ghosts told
/// (`notify_ghosts`, not at map load).
pub fn complete_shell(
    world: &mut World,
    shell_type: &str,
    pos: TilePos,
    creator: Option<Entity>,
    made_of: Option<&'static str>,
) -> Option<Entity> {
    let def = crate::ghost_roles::def(world, shell_type)?;
    let shell = spawn_obj(world, shell_type, pos)?;
    world.entity_mut(shell).insert((
        GhostRoleSpawner {
            name: def.name.clone(),
            desc: crate::ghost_roles::role_text(&def),
            uses: def.uses,
            def: Some(shell_type.to_owned()),
        },
        GolemShell { creator, made_of },
    ));
    let area = crate::bots::area_name(world, pos);
    let alert = format!("В зоне {area} была завершена оболочка голема.");
    for ghost in crate::ghost_poll::ghosts(world) {
        tell_mob(world, ghost, Span::Notice, alert.clone());
    }
    Some(shell)
}

/// `golem/servant/name_mob`: "Golem (N)" while its creator lives.
pub fn forced_name(world: &mut World, spawner: Entity) -> Option<String> {
    if !spawner_path(world, spawner).is_some_and(|p| is_under(&p, SERVANT_SHELL)) {
        return None;
    }
    living_creator(world, spawner)?;
    let n = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(1, 999);
    Some(format!("Golem ({n})"))
}

fn living_creator(world: &World, spawner: Entity) -> Option<Entity> {
    let creator = world.get::<GolemShell>(spawner)?.creator?;
    (world.get::<crate::health::Stat>(creator)? != &crate::health::Stat::Dead).then_some(creator)
}

/// `golem/special` → `give_directive` for the golem `mob` taken from `spawner`.
pub fn special(world: &mut World, mob: Entity, spawner: Entity) {
    let Some(path) = spawner_path(world, spawner).filter(|p| is_under(p, SHELL)) else {
        return;
    };
    if is_under(&path, SERVANT_SHELL) {
        // Without a living creator: "Guess you're free now".
        let Some(creator) = living_creator(world, spawner) else {
            info!("{mob:?} possessed a servant golem shell with no owner");
            return;
        };
        // `enslave_mind_to_creator`: the creator's factions.
        if let Some(factions) = world.get::<crate::mobs::Faction>(creator).cloned() {
            let mut own = world
                .get::<crate::mobs::Faction>(mob)
                .cloned()
                .unwrap_or_else(|| {
                    crate::mobs::Faction(vec![crate::mobs::Faction::NEUTRAL.to_owned()])
                });
            for f in factions.0 {
                if !own.0.contains(&f) {
                    own.0.push(f);
                }
            }
            world.entity_mut(mob).insert(own);
        }
        let master = crate::security::name_of(world, creator);
        tell_mob(
            world,
            mob,
            Span::BoldWarning,
            format!("Служите {master} и помогайте в достижении его целей любой ценой."),
        );
        info!("{mob:?} has been enslaved to {creator:?}");
        return;
    }
    info!("{mob:?} possessed a free golem shell");
    let on_station = world
        .get::<TilePos>(mob)
        .is_some_and(|p| crate::events::controller::station_levels(world).contains(&p.z));
    if !on_station {
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Создавайте оболочки големов в автолате и скармливайте им очищенные листы минералов, \
             чтобы оживить их! В целом вы мирная группа, если вас не провоцировать.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shells_eat_their_minerals() {
        assert_eq!(
            stack_food("/obj/item/stack/sheet/iron/fifty"),
            Some("/obj/item/stack/sheet/iron")
        );
        assert_eq!(
            stack_food("/obj/item/stack/ore/bluespace_crystal/refined"),
            Some("/obj/item/stack/ore/bluespace_crystal/refined")
        );
        assert_eq!(stack_food("/obj/item/stack/sheet/mineral/wood"), None);
        assert_eq!(stack_food("/obj/item/stack/sheet/ironing"), None);
    }
}
