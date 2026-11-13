//! Minebots and NODE drones (server), after tg `/mob/living/basic/mining_drone`
//! (`code/modules/mob/living/basic/minebots/`) and `/mob/living/basic/node_drone`
//! (`lavaland/node_drone/`); their trees are in `crate::mobs::ai::trees::lavaland`, the leaves in
//! [`super::ai`].
//!
//! - A minebot out of combat mode collects loose ore (`minebot_mining`: the nearest ore stack
//!   it sees within 7 tiles, walked to; `UnarmedAttack` moves every ore stack of that tile into
//!   it) and dumps it all when it dies (`drop_ore`), leaving old robot debris
//!   (`death_drops`).
//! - It repairs a hurt NODE drone next to it (`repair_node_drone`: 6 s holding still next to
//!   it, then 50 brute healed), and fights back with its drill.
//! - Alt-click (not in combat mode) toggles its combat mode (`click_alt`).
//! - A welder used on it out of combat mode (neither in combat mode) mends 15 brute
//!   (`welder_act`).
//! - A NODE drone fights nobody, remembers who hurts it (`ai_retaliate`) and blows up lightly
//!   when it dies (`explosion(light_impact_range = 1)`).
// shortcut: no minebot gun (the kinetic accelerator shots and keeping its shooting distance;
// its modkits are `crate::mining::modkits`), missiles, landmines, light and meson toggles, the
// `MineBot` UI, pet commands, colours, mining walls, the
// SOS for downed miners, sentience; the drone does not seek out a vent by itself (vents land
// theirs, `crate::mining::vents`), no fly-in/escape animation.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

use super::abilities::{alive, now};
use crate::combat::CombatMode;
use crate::health::{DamageType, Health, Stat};

pub const MINEBOT: &str = "/mob/living/basic/mining_drone";
pub const NODE_DRONE: &str = "/mob/living/basic/node_drone";
const ORE: &str = "/obj/item/stack/ore";
/// The minebot's `death_drops`.
pub const ROBOT_DEBRIS: &str = "/obj/effect/decal/cleanable/blood/gibs/robot_debris/old";
/// `repair_node_drone`: the `do_after` and the brute it heals.
const REPAIR_TIME: f64 = 6.0;
const REPAIR_AMOUNT: f32 = 50.0;
/// `welder_act`: `adjust_brute_loss(-15)`, `use_tool` at volume 40.
const WELD_AMOUNT: f32 = 15.0;
const WELD_SOUND: &str = "sound/items/tools/welder.ogg";
/// How far a minebot looks for ore and drones (`oview` of its controller's finders).
pub const SEARCH_RANGE: u32 = 7;

/// A minebot: the ore it holds (path, amount), a repair under way. Its `combat_mode` is its
/// [`CombatMode`].
#[derive(Component, Clone, Debug, Default)]
pub struct Minebot {
    pub ore: Vec<(String, u32)>,
    /// `do_after` on a NODE drone: done at this time.
    pub repair: Option<(f64, Entity)>,
}

/// `/mob/living/basic/node_drone`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NodeDrone;

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // A welder mends a minebot.
    app.add_click_handler(ClickPhase::HeldOnTarget, 1511, |w, c| {
        c.held.is_some_and(|h| weld_repair(w, c.mob, h, c.target))
    });
    app.add_systems(Update, repairs.before(crate::health::HealthSystems));
}

/// `mining_drone/click_alt`: out of combat mode, the user flips the bot between collecting ore
/// and attacking wildlife. True on a minebot (`CLICK_ACTION_BLOCKING` in combat mode).
pub fn click_alt(world: &mut World, user: Entity, bot: Entity) -> bool {
    if world.get::<Minebot>(bot).is_none() {
        return false;
    }
    let user_combat = world
        .get::<crate::combat::CombatMode>(user)
        .is_some_and(|c| c.0);
    if user_combat {
        return true;
    }
    let combat = !in_combat(world, bot);
    world.entity_mut(bot).insert(CombatMode(combat));
    let text = if combat {
        "Теперь атакует фауну."
    } else {
        "Теперь собирает рассыпанную руду."
    };
    super::abilities::tell(
        world,
        user,
        ss13_shared::chat::Span::Notice,
        text.to_owned(),
    );
    true
}

/// `mining_drone/welder_act`: 15 brute mended, not in either's combat mode, not at full health.
/// False when the click goes on (not a welder, not a minebot, the user in combat mode).
pub fn weld_repair(world: &mut World, user: Entity, welder: Entity, bot: Entity) -> bool {
    if world.get::<Minebot>(bot).is_none() {
        return false;
    }
    let combat = in_combat(world, bot);
    let user_combat = world
        .get::<crate::combat::CombatMode>(user)
        .is_some_and(|c| c.0);
    let is_welder =
        crate::tools::tool_of(world, welder).is_some_and(|t| t.0 == crate::tools::ToolKind::Welder);
    if !is_welder || user_combat {
        return false;
    }
    let full = world.get::<Health>(bot).is_none_or(|h| h.health() >= h.max);
    let text = if combat {
        "Нельзя чинить в боевом режиме!".to_owned()
    } else if full {
        "Повреждений нет!".to_owned()
    } else if !crate::tools::tool_usable(world, welder, 0, 0.0) {
        crate::tools::unusable_message(world, welder, 0)
    } else {
        if let Some(mut health) = world.get_mut::<Health>(bot) {
            health.adjust(DamageType::Brute, -WELD_AMOUNT);
        }
        if let Some(&at) = world.get::<TilePos>(bot) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(at, WELD_SOUND, 40.0, true),
            );
        }
        "Успешно отремонтирован!".to_owned()
    };
    super::abilities::tell(world, user, ss13_shared::chat::Span::Notice, text);
    true
}

fn in_combat(world: &World, bot: Entity) -> bool {
    world.get::<CombatMode>(bot).is_some_and(|c| c.0)
}

pub(super) fn attach(world: &mut World, mob: Entity, path: &str) {
    if is_under(path, MINEBOT) {
        world.entity_mut(mob).insert(Minebot::default());
    } else if is_under(path, NODE_DRONE) {
        world.entity_mut(mob).insert(NodeDrone);
    }
}

/// `mining_drone/death` (`drop_ore`, `death_drops`) and `node_drone/death`.
pub(super) fn on_death(world: &mut World, mob: Entity, path: &str, pos: TilePos) {
    if is_under(path, MINEBOT) {
        let ore = world
            .get_mut::<Minebot>(mob)
            .map(|mut b| std::mem::take(&mut b.ore))
            .unwrap_or_default();
        for (ore, amount) in ore {
            crate::construction::spawn_stack(world, &ore, pos, amount);
        }
        crate::construction::spawn_obj(world, ROBOT_DEBRIS, pos);
    } else if is_under(path, NODE_DRONE) {
        crate::explosions::explosion(
            world,
            pos,
            crate::explosions::Explosion::new(0, 0, 1).cause("NODE drone"),
        );
    }
}

/// The repair holds the minebot still.
pub(super) fn channeling(world: &World, mob: Entity) -> bool {
    world
        .get::<Minebot>(mob)
        .is_some_and(|b| b.repair.is_some())
}

fn next_to(world: &World, a: Entity, b: Entity) -> bool {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(a), world.get::<TilePos>(b)) else {
        return false;
    };
    world
        .resource::<SpatialIndex>()
        .adjacent(from, to, |e| e == b)
}

/// The loose ore stacks on the floor (items with a tile).
fn loose_ore(world: &mut World) -> Vec<(Entity, TilePos)> {
    let mut q = world.query::<(Entity, &DmItem, &TilePos)>();
    q.iter(world)
        .filter(|(_, item, _)| is_under(&item.path, ORE))
        .map(|(e, _, p)| (e, *p))
        .collect()
}

/// `find_and_set` of ore for a minebot collecting: the nearest loose ore it sees.
pub fn ore_to_collect(world: &mut World, bot: Entity, range: u32) -> Option<(Entity, TilePos)> {
    if world.get::<Minebot>(bot).is_none() || in_combat(world, bot) {
        return None;
    }
    let from = *world.get::<TilePos>(bot)?;
    let mut ore: Vec<(u32, Entity, TilePos)> = loose_ore(world)
        .into_iter()
        .filter_map(|(e, p)| from.chebyshev(p).filter(|d| *d <= range).map(|d| (d, e, p)))
        .collect();
    ore.sort_by_key(|o| o.0);
    ore.into_iter()
        .find(|o| crate::mobs::ai::behaviors::can_see(world, from, o.2, range))
        .map(|o| (o.1, o.2))
}

/// `mining_drone/UnarmedAttack` on ore at `at`, out of combat mode, next to it: every ore stack
/// of that tile goes into the bot. The amount taken.
pub fn collect_ore(world: &mut World, bot: Entity, at: TilePos) -> u32 {
    if world.get::<Minebot>(bot).is_none() || in_combat(world, bot) || !alive(world, bot) {
        return 0;
    }
    let Some(&from) = world.get::<TilePos>(bot) else {
        return 0;
    };
    if !world
        .resource::<SpatialIndex>()
        .adjacent(from, at, |_| true)
    {
        return 0;
    }
    let mut taken = 0;
    for (ore, pos) in loose_ore(world) {
        if pos != at {
            continue;
        }
        let Some(path) = world.get::<DmItem>(ore).map(|i| i.path.clone()) else {
            continue;
        };
        let amount = crate::tools::stack_amount(world, ore);
        world.despawn(ore);
        taken += amount;
        if let Some(mut b) = world.get_mut::<Minebot>(bot) {
            match b.ore.iter_mut().find(|(p, _)| *p == path) {
                Some(held) => held.1 += amount,
                None => b.ore.push((path, amount)),
            }
        }
    }
    taken
}

/// A hurt living NODE drone the minebot sees within `range`, nearest first.
pub fn drone_to_repair(world: &mut World, bot: Entity, range: u32) -> Option<Entity> {
    world.get::<Minebot>(bot)?;
    let from = *world.get::<TilePos>(bot)?;
    let mut q = world.query::<(Entity, &NodeDrone, &TilePos, &Health, &Stat)>();
    let mut hurt: Vec<(u32, Entity, TilePos)> = q
        .iter(world)
        .filter(|(_, _, _, h, s)| **s != Stat::Dead && h.brute > 0.0)
        .filter_map(|(e, _, p, _, _)| {
            from.chebyshev(*p)
                .filter(|d| *d <= range)
                .map(|d| (d, e, *p))
        })
        .collect();
    hurt.sort_by_key(|h| h.0);
    hurt.into_iter()
        .find(|h| crate::mobs::ai::behaviors::can_see(world, from, h.2, range))
        .map(|h| h.1)
}

/// `early_melee_attack` on a NODE drone -> `repair_node_drone`: the 6 s `do_after` starts.
pub fn start_repair(world: &mut World, bot: Entity, drone: Entity) -> bool {
    let now = now(world);
    if world.get::<NodeDrone>(drone).is_none()
        || !alive(world, bot)
        || channeling(world, bot)
        || !next_to(world, bot, drone)
    {
        return false;
    }
    let Some(mut b) = world.get_mut::<Minebot>(bot) else {
        return false;
    };
    b.repair = Some((now + REPAIR_TIME, drone));
    true
}

fn repairs(world: &mut World) {
    let now = now(world);
    let busy: Vec<(Entity, f64, Entity)> = world
        .query::<(Entity, &Minebot)>()
        .iter(world)
        .filter_map(|(e, b)| b.repair.map(|(at, d)| (e, at, d)))
        .collect();
    for (bot, done_at, drone) in busy {
        // The `do_after` breaks when either moves away or the bot is down.
        let holds =
            alive(world, bot) && world.get_entity(drone).is_ok() && next_to(world, bot, drone);
        if holds && now < done_at {
            continue;
        }
        if let Some(mut b) = world.get_mut::<Minebot>(bot) {
            b.repair = None;
        }
        let living = world.get::<Stat>(drone).is_some_and(|s| *s != Stat::Dead);
        if holds
            && living
            && let Some(mut health) = world.get_mut::<Health>(drone)
        {
            health.adjust(DamageType::Brute, -REPAIR_AMOUNT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(SpatialIndex::default());
        world.insert_resource(Time::<()>::default());
        world
    }

    #[test]
    fn a_minebot_repairs_a_hurt_drone_after_six_seconds() {
        let mut world = world();
        let bot = world
            .spawn((TilePos::new(5, 5, 1), Stat::Conscious, Minebot::default()))
            .id();
        let mut hurt = Health::basic(300.0, [1.0; 5]);
        hurt.adjust(DamageType::Brute, 80.0);
        let drone = world
            .spawn((TilePos::new(6, 5, 1), Stat::Conscious, hurt, NodeDrone))
            .id();
        assert_eq!(drone_to_repair(&mut world, bot, SEARCH_RANGE), Some(drone));
        assert!(start_repair(&mut world, bot, drone));
        assert!(channeling(&world, bot));
        // Not yet done: nothing healed.
        world.run_system_once(repairs).unwrap();
        assert!((world.get::<Health>(drone).unwrap().brute - 80.0).abs() < 1e-3);
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f64(REPAIR_TIME));
        world.run_system_once(repairs).unwrap();
        assert!((world.get::<Health>(drone).unwrap().brute - 30.0).abs() < 1e-3);
        assert!(!channeling(&world, bot));
    }

    #[test]
    fn a_welder_needs_a_minebot_out_of_combat_mode() {
        let mut world = world();
        let bot = world
            .spawn((TilePos::new(5, 5, 1), Stat::Conscious, Minebot::default()))
            .id();
        let user = world.spawn(TilePos::new(6, 5, 1)).id();
        // Not a welder: the click goes on.
        let rock = world.spawn_empty().id();
        assert!(!weld_repair(&mut world, user, rock, bot));
        // Not a minebot either.
        assert!(!weld_repair(&mut world, user, rock, user));
        // Alt-click flips its mode, back and forth.
        assert!(click_alt(&mut world, user, bot));
        assert!(in_combat(&world, bot));
        assert!(click_alt(&mut world, user, bot));
        assert!(!in_combat(&world, bot));
        assert!(!click_alt(&mut world, user, user));
    }

    #[test]
    fn a_collecting_minebot_takes_the_ore_next_to_it_and_not_in_combat_mode() {
        let mut world = world();
        let bot = world
            .spawn((TilePos::new(5, 5, 1), Stat::Conscious, Minebot::default()))
            .id();
        let at = TilePos::new(6, 5, 1);
        world.spawn((
            DmItem {
                path: "/obj/item/stack/ore/iron".into(),
                ..Default::default()
            },
            at,
            crate::tools::Stack { amount: 5 },
        ));
        world.entity_mut(bot).insert(CombatMode(true));
        assert_eq!(ore_to_collect(&mut world, bot, SEARCH_RANGE), None);
        assert_eq!(collect_ore(&mut world, bot, at), 0);
        world.entity_mut(bot).insert(CombatMode(false));
        assert!(ore_to_collect(&mut world, bot, SEARCH_RANGE).is_some());
        assert_eq!(collect_ore(&mut world, bot, at), 5);
        assert_eq!(
            world.get::<Minebot>(bot).unwrap().ore,
            vec![("/obj/item/stack/ore/iron".to_owned(), 5)]
        );
        assert!(ore_to_collect(&mut world, bot, SEARCH_RANGE).is_none());
    }
}
