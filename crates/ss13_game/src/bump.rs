//! A living mob walking into something (server), after tg `living.dm` `/mob/living/Bump`,
//! `MobBump`, `can_mobswap_with` and `PushAM`.
//!
//! A living mob (a player's or an NPC) whose step is refused (`ss13_world::Bumped`) by:
//! - a living mob: fire spreads between them (`crate::fire::spread_fire`), their skin-contact
//!   diseases go both ways (`crate::medical::diseases::touch`); then the two swap tiles
//!   ([`can_mobswap_with`]) unless the bumper walks (move intent), either is in combat mode (a
//!   restrained one excepted), the target is buckled or carries a buckled mob, is a restrained
//!   mob pulled by someone else or pulls a restrained mob (with tg's warning), or the step is
//!   half of a diagonal. The swap lets both through other mobs (tg
//!   `PASSMOB`) and puts both back if either cannot move.
//! - when there is no swap, the target is pushed one tile and the bumper steps after it, unless it
//!   lacks `CANPUSH` (`MobDef::unpushable`: the alien queen, spiders, megafauna), is push immune
//!   (a Hulk, the security, medical, peacekeeper and syndicate cyborg models), is a human or
//!   living cyborg in combat mode, or an item in its hands blocks (`block_chance * 2`). A mob
//!   buckled to something is pushed with it (tg `/mob/living/Move` -> `buckled.Move`): not when
//!   that is anchored, too heavy or a MULEbot (`buckle_prevents_pull`).
//! - an object: pushed the same way when unanchored and not too heavy (`move_resist` of its DM
//!   type), a full tile window never into a tile with a window. Pushing what the bumper pulls
//!   stops the pull; a pushed mob keeps facing where it faced. No pushes on a diagonal step, and
//!   none by an NPC smaller than `MOB_SIZE_SMALL` (mice).
//!
//! NPCs follow the same rules through their DM vars: basic mobs carry [`CombatMode`] from their
//! type's `combat_mode` (hostile fauna start in it, so they neither swap with nor are swapped by
//! anyone, and they push peaceful mobs out of their way), switched by traders and minebots like
//! tg's `set_combat_mode`.
// shortcut: no `move_crush`/`force_push` and every mob pushes with `MOVE_FORCE_DEFAULT` (the
// megafauna's `move_force` is not carried), and no "the faster one passes first" when two
// players press the same direction (`client.intended_direction` is not tracked); add them with
// strong mobs and held-key state.

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
use crate::mobs::BasicMob;
use crate::pull::{PulledBy, Pulling, dir_between, step_once, step_passing};
use crate::server::PlayerMob;
use crate::status_effects::Incapacitation;

/// `MOVE_FORCE_EXTREMELY_STRONG` (mechs), `MOVE_FORCE_OVERPOWERING` (megafauna).
pub const MOVE_FORCE_EXTREMELY_STRONG: f64 = MOVE_FORCE_DEFAULT * 3.0 * 3.0;
pub const MOVE_FORCE_OVERPOWERING: f64 = MOVE_FORCE_DEFAULT * 3.0 * 10.0;
/// `MOB_SIZE_SMALL`: an NPC below it pushes nothing.
const MOB_SIZE_SMALL: u8 = 1;
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

/// The first refused step of each living mob this tick (BYOND bumps only the first refusing
/// object; tg `now_pushing` keeps one bump at a time).
// Cost: per tick, per bump of a living mob a few lookups; nothing when nobody bumps (AI movement
// sidesteps blockers, so NPCs bump only when they walk into a tile anyway).
fn mob_bumps(world: &mut World, mut cursor: Local<MessageCursor<Bumped>>) {
    let mut seen = EntityHashSet::default();
    let bumps: Vec<Bumped> = cursor
        .read(world.resource::<Messages<Bumped>>())
        .filter(|b| world.get::<Health>(b.entity).is_some())
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
    // Even without a swap or a push they touched.
    crate::fire::spread_fire(world, mob, other);
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
    // No swap: should we push? Not a mob without `CANPUSH`, a push immune mob, a human or a
    // living cyborg in combat mode, nor one whose held item blocks (anti-riot gear is anti-push).
    if unpushable(world, other) || push_immune(world, other) {
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
    if world.get::<PlayerMob>(mob).is_none()
        && crate::mobs::pets::mob_size(world, mob) < MOB_SIZE_SMALL
    {
        return;
    }
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
    if step_target(world, target, dir) {
        crate::forensics::server::add_fingerprint(world, target, mob, false);
        step_once(world, mob, dir, None).ok();
    }
    if let Some(facing) = facing
        && let Some(mut now) = world.get_mut::<Facing>(target)
    {
        now.set_if_neq(facing);
    }
}

/// tg `/mob/living/Move` of a buckled mob: what it is buckled to moves instead when it is not
/// anchored, carrying the mob (`handle_buckled_mob_movement`: both go back when the mob cannot
/// follow). Anything else takes one step.
fn step_target(world: &mut World, target: Entity, dir: ss13_shared::Dir) -> bool {
    let Some(&Buckled(seat)) = world.get::<Buckled>(target) else {
        return step_once(world, target, dir, None).is_ok();
    };
    if crate::obj_damage::anchored(world, seat) {
        return false;
    }
    let Some(&seat_from) = world.get::<TilePos>(seat) else {
        return false;
    };
    if step_once(world, seat, dir, Some(target)).is_err() {
        return false;
    }
    // tg `CanAllowThrough`: a buckled mob passes what it is buckled to.
    if step_passing(world, target, dir, |_, e| e == seat).is_ok() {
        return true;
    }
    world.entity_mut(seat).insert(seat_from);
    world.write_message(Moved {
        entity: seat,
        from: seat_from.step(dir),
        to: seat_from,
        dir: dir.reverse(),
    });
    false
}

/// `anchored` and `force < move_resist * MOVE_FORCE_PUSH_RATIO`; a mob buckled to something only
/// when that is not too heavy and does not prevent pulls (a MULEbot's rider).
fn pushable(world: &World, mob: Entity, target: Entity) -> bool {
    if crate::obj_damage::anchored(world, target) {
        return false;
    }
    if world.get::<Health>(target).is_some() {
        if world.get::<crate::bots::mule::Riding>(target).is_some() {
            return false;
        }
        if let Some(&Buckled(seat)) = world.get::<Buckled>(target)
            && MOVE_FORCE_DEFAULT < move_resist(world, seat) * MOVE_FORCE_PUSH_RATIO
        {
            return false;
        }
    }
    // Items, closets, structures and machines by their `anchored`, living mobs; anything heavier
    // than the default force (`MOVE_FORCE_PUSH_RATIO` and `MOVE_FORCE_PULL_RATIO` are both 1).
    crate::pull::can_be_pulled(world, mob, target)
}

/// tg `move_resist`: of the object's or the basic mob's DM type, `MOVE_FORCE_EXTREMELY_STRONG`
/// for mechs, `MOVE_RESIST_DEFAULT` for everything else (humans, cyborgs, items).
pub fn move_resist(world: &World, e: Entity) -> f64 {
    if world.get::<crate::mecha::Mech>(e).is_some() {
        MOVE_FORCE_EXTREMELY_STRONG
    } else if let Some(obj) = world.get::<DmObj>(e) {
        f64::from(obj.move_resist)
    } else if let Some(basic) = world.get::<BasicMob>(e) {
        basic.def.move_resist.map_or(MOVE_RESIST_DEFAULT, f64::from)
    } else {
        MOVE_RESIST_DEFAULT
    }
}

/// tg `status_flags` without `CANPUSH` (basic mobs; every other mob here has it).
fn unpushable(world: &World, mob: Entity) -> bool {
    world.get::<BasicMob>(mob).is_some_and(|b| b.def.unpushable)
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

    fn world() -> World {
        let mut app = App::new();
        app.add_plugins(ss13_world::WorldPlugin);
        app.init_resource::<Time>();
        app.insert_resource(CombatRng(7));
        app.add_message::<ss13_net::chat::server::SendChat>();
        app.world_mut()
            .resource_mut::<SpatialIndex>()
            .create_level(1, 20, 20)
            .unwrap();
        std::mem::take(app.world_mut())
    }

    fn mob(world: &mut World, x: i32, combat: bool) -> Entity {
        world
            .spawn((
                Health::default(),
                Stat::Conscious,
                CombatMode(combat),
                ss13_world::Density,
                TilePos::new(x, 5, 1),
            ))
            .id()
    }

    /// A basic mob of a type with these DM vars, its `combat_mode` on it like `crate::mobs`.
    fn npc(world: &mut World, x: i32, def: ss13_mapload::mobs::MobDef) -> Entity {
        let e = mob(world, x, def.combat_mode);
        world.entity_mut(e).insert(BasicMob {
            def: std::sync::Arc::new(def),
            icon_living: String::new(),
            icon_dead: String::new(),
            mouth: false,
        });
        e
    }

    fn hostile() -> ss13_mapload::mobs::MobDef {
        ss13_mapload::mobs::MobDef {
            combat_mode: true,
            mob_size: 2,
            ..default()
        }
    }

    fn bump(world: &mut World, entity: Entity, obstacle: Entity) {
        living_bump(
            world,
            Bumped {
                entity,
                obstacle,
                diagonal: false,
            },
        );
    }

    fn x(world: &World, e: Entity) -> i32 {
        world.get::<TilePos>(e).unwrap().x
    }

    #[test]
    fn npcs_swap_and_push_by_their_dm_vars() {
        let mut w = world();
        // Two peaceful NPCs swap (tg `can_mobswap_with`: no client is no obstacle).
        let (pig, cow) = (npc(&mut w, 5, default()), npc(&mut w, 6, default()));
        bump(&mut w, pig, cow);
        assert_eq!((x(&w, pig), x(&w, cow)), (6, 5), "swapped");

        // A hostile NPC (in combat mode) does not swap with a peaceful human: it pushes him and
        // steps after him.
        let mut w = world();
        let (carp, human) = (npc(&mut w, 5, hostile()), mob(&mut w, 6, false));
        w.entity_mut(human)
            .insert(crate::human::AppearancePrefs::random(1).body());
        bump(&mut w, carp, human);
        assert_eq!((x(&w, carp), x(&w, human)), (6, 7), "pushed");
        // A human in combat mode holds.
        w.entity_mut(human).insert(CombatMode(true));
        bump(&mut w, carp, human);
        assert_eq!((x(&w, carp), x(&w, human)), (6, 7), "a fighter holds");

        // The human walks into the hostile NPC: no swap, it is pushed (`CANPUSH`)...
        let mut w = world();
        let (human, carp) = (mob(&mut w, 5, false), npc(&mut w, 6, hostile()));
        bump(&mut w, human, carp);
        assert_eq!((x(&w, human), x(&w, carp)), (6, 7), "the carp is shoved");
        // ...unless it has no `CANPUSH` (a spider).
        let spider = npc(
            &mut w,
            8,
            ss13_mapload::mobs::MobDef {
                unpushable: true,
                ..hostile()
            },
        );
        w.entity_mut(carp).despawn();
        w.entity_mut(human).insert(TilePos::new(7, 5, 1));
        bump(&mut w, human, spider);
        assert_eq!((x(&w, human), x(&w, spider)), (7, 8), "the spider holds");

        // A mouse (`MOB_SIZE_TINY`, no client) pushes nothing, but swaps with the peaceful.
        let mut w = world();
        let tiny = ss13_mapload::mobs::MobDef {
            mob_size: 0,
            ..default()
        };
        let (mouse, human) = (npc(&mut w, 5, tiny), mob(&mut w, 6, true));
        bump(&mut w, mouse, human);
        assert_eq!((x(&w, mouse), x(&w, human)), (5, 6), "no swap, no push");
        w.entity_mut(human).insert(CombatMode(false));
        bump(&mut w, mouse, human);
        assert_eq!((x(&w, mouse), x(&w, human)), (6, 5), "swapped");
    }

    #[test]
    fn a_burning_mob_lights_whoever_it_bumps() {
        let mut w = world();
        let (burning, human) = (npc(&mut w, 5, hostile()), mob(&mut w, 6, true));
        crate::fire::adjust_fire_stacks(&mut w, burning, 10.0);
        crate::fire::ignite_mob(&mut w, burning);
        bump(&mut w, burning, human);
        assert!(crate::fire::on_fire(&w, human));
        let stacks = |w: &World, e| w.get::<crate::fire::FireStacks>(e).unwrap().stacks;
        assert_eq!((stacks(&w, burning), stacks(&w, human)), (5.0, 5.0));
    }

    #[test]
    fn a_buckled_mob_is_pushed_with_its_seat() {
        let mut w = world();
        let pusher = mob(&mut w, 5, true);
        let sitter = mob(&mut w, 6, false);
        let seat = |w: &mut World, anchored: bool, move_resist: f32| {
            w.spawn((
                crate::obj_damage::Anchored(anchored),
                DmObj {
                    move_resist,
                    ..default()
                },
                TilePos::new(6, 5, 1),
            ))
            .id()
        };
        // Bolted down: neither moves.
        let chair = seat(&mut w, true, 1000.0);
        w.entity_mut(sitter).insert(Buckled(chair));
        bump(&mut w, pusher, sitter);
        assert_eq!((x(&w, sitter), x(&w, chair), x(&w, pusher)), (6, 6, 5));
        // Too heavy (`move_resist` 2999): neither moves.
        w.entity_mut(chair).despawn();
        let heavy = seat(&mut w, false, 2999.0);
        w.entity_mut(sitter).insert(Buckled(heavy));
        bump(&mut w, pusher, sitter);
        assert_eq!((x(&w, sitter), x(&w, heavy), x(&w, pusher)), (6, 6, 5));
        // A loose roller bed: it rolls on with its patient, the pusher steps after.
        w.entity_mut(heavy).despawn();
        let bed = seat(&mut w, false, 1000.0);
        w.entity_mut(sitter).insert(Buckled(bed));
        bump(&mut w, pusher, sitter);
        assert_eq!((x(&w, sitter), x(&w, bed), x(&w, pusher)), (7, 7, 6));
        assert_eq!(w.get::<Buckled>(sitter), Some(&Buckled(bed)));
        // A MULEbot's rider (`buckle_prevents_pull`) is not pushed.
        w.entity_mut(sitter)
            .remove::<Buckled>()
            .insert(crate::bots::mule::Riding(pusher));
        bump(&mut w, pusher, sitter);
        assert_eq!((x(&w, sitter), x(&w, pusher)), (7, 6));
    }

    #[test]
    fn heavy_objects_are_not_pushed() {
        let mut w = world();
        let human = mob(&mut w, 5, true);
        let crate_of = |w: &mut World, x: i32, move_resist: f32| {
            w.spawn((
                crate::obj_damage::Anchored(false),
                DmObj {
                    move_resist,
                    ..default()
                },
                ss13_world::Density,
                TilePos::new(x, 5, 1),
            ))
            .id()
        };
        let door = crate_of(&mut w, 6, 2999.0);
        bump(&mut w, human, door);
        assert_eq!(
            (x(&w, human), x(&w, door)),
            (5, 6),
            "an unbolted door holds"
        );
        w.entity_mut(door).despawn();
        let closet = crate_of(&mut w, 6, 1000.0);
        bump(&mut w, human, closet);
        assert_eq!((x(&w, human), x(&w, closet)), (6, 7), "a closet is shoved");
    }
}
