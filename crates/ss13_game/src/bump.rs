//! A living mob walking into something (server), after tg `living.dm` `/mob/living/Bump`,
//! `MobBump`, `can_mobswap_with` and `PushAM`.
//!
//! A player's mob whose step is refused (`ss13_world::Bumped`) by:
//! - a living mob: their skin-contact diseases go both ways (`crate::medical::diseases::touch`);
//!   then the two swap tiles ([`can_mobswap_with`]) unless the bumper walks (move intent), either
//!   is in combat mode (a restrained one excepted), the target is buckled or carries a buckled
//!   mob, is a restrained mob pulled by someone else or pulls a restrained mob (with tg's
//!   warning), or the step is half of a diagonal. The swap lets both through other mobs (tg
//!   `PASSMOB`) and puts both back if either cannot move.
//! - when there is no swap, the target is pushed one tile and the bumper steps after it, unless it
//!   is push immune (a Hulk, the security, medical, peacekeeper and syndicate cyborg models), a
//!   human or living cyborg in combat mode, or an item in its hands blocks (`block_chance * 2`).
//! - an object: pushed the same way when unanchored and not too heavy (`move_resist`), a full tile
//!   window never into a tile with a window. Pushing what the bumper pulls stops the pull; a
//!   pushed mob keeps facing where it faced. No pushes on a diagonal step.
// shortcut: no `spreadFire` (burning mobs do not light each other: the port has no fire spread
// between mobs yet), no `move_crush`/`force_push` (no mob here has three or two times the default
// `move_force`), and no "the faster one passes first" when both mobs press the same direction
// (`client.intended_direction` is not tracked); add them with strong mobs and held-key state.
// shortcut: only player mobs bump this way (NPC mobs have no tracked `combat_mode` here and stay
// blocked), and a buckled mob is never pushed with its chair; add with NPC combat mode and
// moving buckled furniture.

use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmMelee, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Facing, TilePos, capitalize};
use ss13_world::{Bumped, Moved, SpatialIndex, WorldSystems};

use crate::combat::{CombatMode, CombatRng};
use crate::fire::{MOVE_FORCE_DEFAULT, MOVE_FORCE_PUSH_RATIO, MOVE_RESIST_DEFAULT};
use crate::health::{Health, Stat};
use crate::inventory::Inventory;
use crate::medical::surgery::Buckled;
use crate::pull::{PulledBy, Pulling, dir_between, step_once, step_passing};
use crate::server::PlayerMob;
use crate::status_effects::Incapacitation;

/// `MOVE_FORCE_EXTREMELY_STRONG` (mechs), `MOVE_FORCE_OVERPOWERING` (megafauna).
pub const MOVE_FORCE_EXTREMELY_STRONG: f64 = MOVE_FORCE_DEFAULT * 3.0 * 3.0;
pub const MOVE_FORCE_OVERPOWERING: f64 = MOVE_FORCE_DEFAULT * 3.0 * 10.0;
/// The cyborg models with `TRAIT_PUSHIMMUNE` in `model_traits`.
const PUSH_IMMUNE_MODELS: [&str; 6] = [
    "/obj/item/robot_model/medical",
    "/obj/item/robot_model/peacekeeper",
    "/obj/item/robot_model/security",
    "/obj/item/robot_model/syndicate",
    "/obj/item/robot_model/syndicate_medical",
    "/obj/item/robot_model/saboteur",
];
const WINDOW: &str = "/obj/structure/window";
const CLOTHING: &str = "/obj/item/clothing";
/// Seconds between two "cannot push past" warnings to one mob (tg: `!(world.time % 5)`).
const WARN_EVERY_SECS: f64 = 0.5;

/// When the mob was last told it cannot push past a restrained mob.
#[derive(Component, Clone, Copy, Debug, Default)]
struct WarnedAt(f64);

pub struct BumpPlugin;

impl Plugin for BumpPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, mob_bumps.after(WorldSystems::Movement));
    }
}

/// One side of [`can_mobswap_with`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SwapSide {
    pub no_mobswap: bool,
    pub combat_mode: bool,
    pub restrained: bool,
    /// tg `mobility_flags & MOBILITY_MOVE`.
    pub can_move: bool,
    pub move_force: f64,
    pub move_resist: f64,
}

/// tg `can_mobswap_with` of `me` bumping `other`; `pulled_by_me`: `other.pulledby == src`.
pub fn can_mobswap_with(me: SwapSide, other: SwapSide, pulled_by_me: bool) -> bool {
    if me.no_mobswap || other.no_mobswap {
        return false;
    }
    let too_strong = other.move_resist > me.move_force;
    if !other.can_move {
        return !too_strong;
    }
    if pulled_by_me && !too_strong {
        return true;
    }
    if me.combat_mode && !me.restrained {
        return false;
    }
    !(other.combat_mode && (!other.restrained || too_strong))
}

/// The first refused step of each player mob this tick (BYOND bumps only the first refusing
/// object; tg `now_pushing` keeps one bump at a time).
// Cost: per tick, per bump of a player mob a few lookups; nothing when nobody bumps.
fn mob_bumps(world: &mut World, mut cursor: Local<MessageCursor<Bumped>>) {
    let mut seen = EntityHashSet::default();
    let bumps: Vec<Bumped> = cursor
        .read(world.resource::<Messages<Bumped>>())
        .filter(|b| world.get::<PlayerMob>(b.entity).is_some())
        .filter(|b| seen.insert(b.entity))
        .copied()
        .collect();
    // A bump that moved its pair (a swap, a push) voids the later bumps of either this tick: two
    // mobs walking into each other swap once, not twice.
    let mut moved = EntityHashSet::default();
    for bump in bumps {
        if moved.contains(&bump.entity) || moved.contains(&bump.obstacle) {
            continue;
        }
        let at = |w: &World| {
            (
                w.get::<TilePos>(bump.entity).copied(),
                w.get::<TilePos>(bump.obstacle).copied(),
            )
        };
        let before = at(world);
        living_bump(world, bump);
        if at(world) != before {
            moved.extend([bump.entity, bump.obstacle]);
        }
    }
}

/// tg `/mob/living/Bump`.
fn living_bump(world: &mut World, bump: Bumped) {
    let Bumped {
        entity: mob,
        obstacle,
        diagonal,
    } = bump;
    if world.get::<Health>(mob).is_none() || buckled(world, mob) {
        return;
    }
    // Both still where the bump happened (another bump this tick may have moved them).
    let (Some(&at), Some(&target_at)) = (world.get::<TilePos>(mob), world.get::<TilePos>(obstacle))
    else {
        return;
    };
    if at.chebyshev(target_at).is_none_or(|d| d > 1) {
        return;
    }
    if world.get::<Health>(obstacle).is_some() && mob_bump(world, mob, obstacle, diagonal) {
        return;
    }
    push_am(world, mob, obstacle, diagonal);
}

/// tg `MobBump`: true when the bump ends here (a swap, or no push allowed).
fn mob_bump(world: &mut World, mob: Entity, other: Entity, diagonal: bool) -> bool {
    if world
        .get::<crate::janitor::Walking>(mob)
        .is_some_and(|w| w.0)
    {
        return true;
    }
    // shortcut: no `spreadFire` here (see the module docs).
    crate::medical::diseases::touch(world, mob, other);

    if restrained(world, other)
        && let Some(puller) = world.get::<PulledBy>(other).map(PulledBy::puller)
        && puller != mob
    {
        let text = format!(
            "{} сдерживается, вы не можете протолкнуться сквозь.",
            capitalize(&crate::security::name_of(world, other))
        );
        warn(world, mob, text);
        return true;
    }
    if let Some(&Pulling(pulled)) = world.get::<Pulling>(other)
        && world.get::<Health>(pulled).is_some()
        && restrained(world, pulled)
    {
        let text = format!(
            "{} сдерживает {}, вы не можете протолкнуться сквозь.",
            capitalize(&crate::security::name_of(world, other)),
            crate::security::name_of(world, pulled)
        );
        warn(world, mob, text);
        return true;
    }
    if diagonal {
        return true;
    }
    if !buckled(world, other) && !has_buckled_mobs(world, other) {
        let pulled_by_me = world
            .get::<PulledBy>(other)
            .is_some_and(|p| p.puller() == mob);
        if can_mobswap_with(swap_side(world, mob), swap_side(world, other), pulled_by_me)
            && swap(world, mob, other)
        {
            return true;
        }
    }
    // No swap: should we push? Not a push immune mob, a human or a living cyborg in combat mode,
    // nor one whose held item blocks (anti-riot gear is anti-push).
    if push_immune(world, other) {
        return true;
    }
    let combat = world.get::<CombatMode>(other).is_some_and(|c| c.0);
    let human = world.get::<crate::human::HumanLook>(other).is_some();
    let cyborg = world.get::<crate::silicon::cyborg::Cyborg>(other).is_some()
        && world.get::<Stat>(other) != Some(&Stat::Dead);
    if combat && (human || cyborg) {
        return true;
    }
    held_item_blocks(world, other)
}

/// The two mobs trade tiles, each passing other mobs; both go back when either cannot move.
fn swap(world: &mut World, mob: Entity, other: Entity) -> bool {
    let (Some(&from), Some(&other_from)) = (world.get::<TilePos>(mob), world.get::<TilePos>(other))
    else {
        return false;
    };
    let Some(dir) = dir_between(from, other_from) else {
        return false;
    };
    let pass_mobs = |w: &World, e: Entity| w.get::<Health>(e).is_some();
    if step_passing(world, other, dir.reverse(), pass_mobs).is_err() {
        return false;
    }
    if step_passing(world, mob, dir, pass_mobs).is_ok() {
        return true;
    }
    // tg `forceMove` back.
    world.entity_mut(other).insert(other_from);
    world.write_message(Moved {
        entity: other,
        from,
        to: other_from,
        dir,
    });
    false
}

/// tg `PushAM` with the mob's `move_force`.
fn push_am(world: &mut World, mob: Entity, target: Entity, diagonal: bool) {
    if diagonal {
        return;
    }
    let (Some(&from), Some(&target_at)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target))
    else {
        return;
    };
    // On the same tile (a directional window the mob stands on): the way the mob faces.
    let Some(dir) = dir_between(from, target_at).or(world.get::<Facing>(mob).map(|f| f.0)) else {
        return;
    };
    if !pushable(world, mob, target) {
        return;
    }
    if is_fulltile_window(world, target) {
        let to = target_at.step(dir);
        let window = world
            .resource::<SpatialIndex>()
            .on_tile(to)
            .iter()
            .any(|&e| crate::construction::path_of(world, e).is_some_and(|p| is_under(&p, WINDOW)));
        if window {
            return;
        }
    }
    if world.get::<Pulling>(mob).is_some_and(|p| p.0 == target) {
        world.entity_mut(mob).remove::<Pulling>();
    }
    let facing = world
        .get::<Health>(target)
        .and(world.get::<Facing>(target).copied());
    if step_once(world, target, dir, None).is_ok() {
        crate::forensics::server::add_fingerprint(world, target, mob, false);
        step_once(world, mob, dir, None).ok();
    }
    if let Some(facing) = facing
        && let Some(mut now) = world.get_mut::<Facing>(target)
    {
        now.set_if_neq(facing);
    }
}

/// `anchored` and `force < move_resist * MOVE_FORCE_PUSH_RATIO`, and a mob buckled to something
/// is not pushed (see the module docs).
fn pushable(world: &World, mob: Entity, target: Entity) -> bool {
    if world
        .get::<crate::obj_damage::Anchored>(target)
        .is_some_and(|a| a.0)
    {
        return false;
    }
    if world.get::<Health>(target).is_some() && buckled(world, target) {
        return false;
    }
    if MOVE_FORCE_DEFAULT < move_resist(world, target) * MOVE_FORCE_PUSH_RATIO {
        return false;
    }
    // Items, closets, structures and machines by their `anchored`, living mobs.
    crate::pull::can_be_pulled(world, mob, target)
}

/// tg `move_resist`: `MOVE_RESIST_DEFAULT` for everything here but mechs and megafauna.
pub fn move_resist(world: &World, e: Entity) -> f64 {
    if world.get::<crate::mobs::megafauna::Megafauna>(e).is_some() {
        MOVE_FORCE_OVERPOWERING
    } else if world.get::<crate::mecha::Mech>(e).is_some() {
        MOVE_FORCE_EXTREMELY_STRONG
    } else {
        MOVE_RESIST_DEFAULT
    }
}

fn swap_side(world: &World, mob: Entity) -> SwapSide {
    let traits = world.get::<Incapacitation>(mob);
    let can_move = match (world.get::<Health>(mob), world.get::<Stat>(mob)) {
        (Some(health), Some(&stat)) => health.can_move(stat),
        _ => true,
    } && !traits.is_some_and(|t| t.immobilized);
    SwapSide {
        // tg `TRAIT_NOMOBSWAP`: MULEbots and robot customers.
        no_mobswap: world.get::<crate::bots::mule::Mule>(mob).is_some()
            || world.get::<crate::restaurant::Customer>(mob).is_some(),
        combat_mode: world.get::<CombatMode>(mob).is_some_and(|c| c.0),
        restrained: restrained(world, mob),
        can_move,
        move_force: MOVE_FORCE_DEFAULT,
        move_resist: move_resist(world, mob),
    }
}

/// tg `TRAIT_RESTRAINED`.
fn restrained(world: &World, mob: Entity) -> bool {
    world
        .get::<Incapacitation>(mob)
        .is_some_and(|t| t.restrained)
}

/// tg `buckled`: to furniture, a MULEbot or a ridden vehicle.
fn buckled(world: &World, mob: Entity) -> bool {
    world.get::<Buckled>(mob).is_some()
        || world.get::<crate::bots::mule::Riding>(mob).is_some()
        || world.get::<crate::vehicles::Rider>(mob).is_some()
}

/// tg `has_buckled_mobs` of a mob: someone rides it (a MULEbot's rider).
fn has_buckled_mobs(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::bots::mule::Mule>(mob)
        .and_then(|m| m.load)
        .is_some_and(|load| world.get::<crate::bots::mule::Riding>(load).is_some())
}

/// tg `TRAIT_PUSHIMMUNE`: the Hulk and some cyborg models.
fn push_immune(world: &World, mob: Entity) -> bool {
    if crate::genetics::effects::is_hulk(world, mob) {
        return true;
    }
    let Some(model) = world
        .get::<crate::silicon::cyborg::Cyborg>(mob)
        .and_then(|c| c.model)
    else {
        return false;
    };
    world
        .get_resource::<crate::silicon::Silicons>()
        .and_then(|s| s.0.models.get(model))
        .is_some_and(|m| PUSH_IMMUNE_MODELS.contains(&m.path.as_str()))
}

/// `prob(I.block_chance * 2)` of each held item that is not clothing.
fn held_item_blocks(world: &mut World, mob: Entity) -> bool {
    let hands = world
        .get::<Inventory>(mob)
        .map(|i| i.hands)
        .unwrap_or_default();
    for item in hands.into_iter().flatten() {
        let clothing = world
            .get::<DmItem>(item)
            .is_some_and(|i| is_under(&i.path, CLOTHING));
        let chance = world.get::<DmMelee>(item).map_or(0.0, |m| m.block_chance);
        if !clothing && chance > 0.0 && world.resource_mut::<CombatRng>().chance(chance * 2.0) {
            return true;
        }
    }
    false
}

fn is_fulltile_window(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| o.fulltile && is_under(&o.path, WINDOW))
}

fn warn(world: &mut World, mob: Entity, text: String) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world
        .get::<WarnedAt>(mob)
        .is_some_and(|w| now < w.0 + WARN_EVERY_SECS)
    {
        return;
    }
    world.entity_mut(mob).insert(WarnedAt(now));
    if let Some(&PlayerMob { client }) = world.get::<PlayerMob>(mob) {
        crate::tools::tell(world, client, Span::Warning, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side() -> SwapSide {
        SwapSide {
            can_move: true,
            move_force: MOVE_FORCE_DEFAULT,
            move_resist: MOVE_RESIST_DEFAULT,
            ..default()
        }
    }

    #[test]
    fn peaceful_mobs_swap() {
        assert!(can_mobswap_with(side(), side(), false));
        let walled = SwapSide {
            no_mobswap: true,
            ..side()
        };
        assert!(!can_mobswap_with(walled, side(), false));
        assert!(!can_mobswap_with(side(), walled, false));
    }

    #[test]
    fn combat_mode_blocks_unless_restrained() {
        let fighter = SwapSide {
            combat_mode: true,
            ..side()
        };
        assert!(!can_mobswap_with(fighter, side(), false));
        assert!(!can_mobswap_with(side(), fighter, false));
        let cuffed_fighter = SwapSide {
            restrained: true,
            ..fighter
        };
        assert!(can_mobswap_with(cuffed_fighter, side(), false));
        assert!(can_mobswap_with(side(), cuffed_fighter, false));
        // A puller passes through the fighter it pulls.
        assert!(can_mobswap_with(side(), fighter, true));
    }

    #[test]
    fn immobile_mobs_are_passed_unless_too_strong() {
        let limp = SwapSide {
            can_move: false,
            combat_mode: true,
            ..side()
        };
        assert!(can_mobswap_with(side(), limp, false));
        let heavy = SwapSide {
            move_resist: MOVE_FORCE_OVERPOWERING,
            ..limp
        };
        assert!(!can_mobswap_with(side(), heavy, false));
        // Too strong and restrained in combat mode: still no pass.
        let heavy_cuffed = SwapSide {
            can_move: true,
            restrained: true,
            ..heavy
        };
        assert!(!can_mobswap_with(side(), heavy_cuffed, true));
    }
}
