//! Trees, grass, bushes and rocks (server), after tg `code/game/objects/structures/flora.dm`.
//!
//! - Harvesting (`can_harvest`, `harvest`): wooden flora (trees) with a saw, stone flora (rocks)
//!   with a mining tool, after `harvest_time × toolspeed` (6 s); grass by hand after 6 s. The
//!   products (`get_potential_products`: logs, grass and its seeds, basalt or siderite sand) are
//!   `rand(harvest_amount_low, harvest_amount_high)` weighted picks on the flora's tile, with the
//!   low/medium/high message; a tree falls (the meteor impact, five tiles further) and leaves a
//!   stump; trees, stumps and rocks are gone, grass regrows after 8 to 16 minutes.
//! - A shovel (6 s) uproots flora that `can_uproot` (unanchored, turned 90° either way) and
//!   replants it; an uprooted stump is gone.
//! - Destroyed (`atom_deconstruct`), unharvested flora leaves 60 % of a harvest.
//! - Out of combat mode only; the hit sounds of `play_attack_sound` (bush whack, tree chop, rock
//!   tap).
// shortcut: lavaland ash flora (`/obj/structure/flora/ash`, its own harvest and regrowth sprites)
// and the volcanic pore wait for lavaland; no burn ×4 against flora (`run_atom_armor`), no
// gravity check on the falling tree, no holograms, no Christmas tree presents, the stump keeps
// its own name ("[tree] stump").

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmObj, DmTool};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use crate::combat::CombatRng;
use crate::construction::Job;
use crate::medical::{name_of, visible};
use crate::obj_damage::set_anchored;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, tell};

pub const FLORA: &str = "/obj/structure/flora";
const TREE: &str = "/obj/structure/flora/tree";
const STUMP: &str = "/obj/structure/flora/tree/stump";
const XMAS_PRESENTS: &str = "/obj/structure/flora/tree/pine/xmas/presents";
const ASH: &str = "/obj/structure/flora/ash";
const VOLCANO: &str = "/obj/structure/flora/rock/volcano";
const LOG: &str = "/obj/item/grown/log/tree";
const GRASS: &str = "/obj/item/food/grown/grass";
const GRASS_SEEDS: &str = "/obj/item/seeds/grass";
const BASALT: &str = "/obj/item/stack/ore/glass/basalt";
const SIDERITE: &str = "/obj/item/stack/ore/glass/siderite";
const TREE_FALL: &str = "sound/effects/meteorimpact.ogg";
const SHOVEL_DIG: &str = "sound/effects/shovel_dig.ogg";
/// `TREE_FALL_EXTRARANGE`.
const TREE_FALL_EXTRARANGE: f32 = 5.0;
/// `SFX_CRUNCHY_BUSH_WHACK`, `SFX_TREE_CHOP`, `SFX_ROCK_TAP`.
const BUSH_WHACK: &str = "crunchy_bush_whack";
const TREE_CHOP: &str = "tree_chop";
const ROCK_TAP: &str = "rock_tap";
/// `harvest_time`: 6 s, deciseconds.
const HARVEST_DS: f32 = 60.0;
/// `regrowth_time_low`, `regrowth_time_high`: 8 and 16 minutes, deciseconds.
const REGROW_LOW_DS: u32 = 4800;
const REGROW_HIGH_DS: u32 = 9600;
/// `atom_deconstruct`: `harvest(product_amount_multiplier = 0.6)`.
const DESTROYED_MULTIPLIER: f32 = 0.6;

/// The vars of one kind of flora (tg's per-subtype `harvest_*`, `flora_flags`, `can_uproot`,
/// `delete_on_harvest`, `get_potential_products`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloraKind {
    /// `(path, weight)`; empty: nothing to harvest.
    pub products: &'static [(&'static str, u32)],
    pub low: u32,
    pub high: u32,
    pub with_hands: bool,
    /// `FLORA_HERBAL`, `FLORA_WOODEN`, `FLORA_STONE`.
    pub herbal: bool,
    pub wooden: bool,
    pub stone: bool,
    pub can_uproot: bool,
    pub delete_on_harvest: bool,
    /// `harvest_verb`: to start, done by you, done by someone (with `harvest_verb_suffix`).
    pub verb: (&'static str, &'static str, &'static str),
    /// `harvest_message_low`, `_med`, `_high`.
    pub messages: [Option<&'static str>; 3],
}

const BASE: FloraKind = FloraKind {
    products: &[],
    low: 1,
    high: 3,
    with_hands: false,
    herbal: false,
    wooden: false,
    stone: false,
    can_uproot: true,
    delete_on_harvest: false,
    verb: ("собирать", "собрали", "собирает"),
    messages: [None; 3],
};

const TREE_KIND: FloraKind = FloraKind {
    products: &[(LOG, 1)],
    low: 6,
    high: 10,
    herbal: true,
    wooden: true,
    delete_on_harvest: true,
    verb: ("рубить", "срубили", "срубает"),
    messages: [
        Some("Вам удаётся получить с дерева несколько брёвен."),
        Some("Вам удаётся получить с дерева немного брёвен."),
        Some("Вам удаётся получить с дерева почти всю древесину."),
    ],
    ..BASE
};

const ROCK_KIND: FloraKind = FloraKind {
    products: &[(BASALT, 1)],
    low: 10,
    high: 20,
    stone: true,
    can_uproot: false,
    delete_on_harvest: true,
    verb: ("раскапывать", "раскопали", "раскапывает"),
    messages: [None, Some("Вы заканчиваете раскапывать камень."), None],
    ..BASE
};

/// The flora's vars by subtype, most specific first.
const KINDS: [(&str, FloraKind); 9] = [
    (
        "/obj/structure/flora/rock/pile/jungle/large",
        FloraKind {
            low: 9,
            high: 13,
            messages: [
                None,
                Some("Вы заканчиваете раскапывать груду камней."),
                None,
            ],
            ..ROCK_KIND
        },
    ),
    (
        "/obj/structure/flora/rock/pile/siderite",
        FloraKind {
            products: &[(SIDERITE, 1)],
            low: 5,
            high: 10,
            messages: [
                None,
                Some("Вы заканчиваете раскапывать груду камней."),
                None,
            ],
            ..ROCK_KIND
        },
    ),
    (
        "/obj/structure/flora/rock/pile",
        FloraKind {
            low: 5,
            high: 10,
            messages: [
                None,
                Some("Вы заканчиваете раскапывать груду камней."),
                None,
            ],
            ..ROCK_KIND
        },
    ),
    (
        "/obj/structure/flora/rock/siderite_growth",
        FloraKind {
            products: &[(SIDERITE, 1)],
            messages: [None, Some("Вы заканчиваете раскапывать нарост."), None],
            ..ROCK_KIND
        },
    ),
    ("/obj/structure/flora/rock", ROCK_KIND),
    (
        "/obj/structure/flora/tree/dead",
        FloraKind {
            low: 2,
            high: 6,
            herbal: false,
            ..TREE_KIND
        },
    ),
    (TREE, TREE_KIND),
    (
        "/obj/structure/flora/grass",
        FloraKind {
            products: &[(GRASS, 10), (GRASS_SEEDS, 1)],
            low: 0,
            high: 2,
            with_hands: true,
            herbal: true,
            messages: [
                Some("Вы вырываете траву из земли просто ради забавы."),
                Some("Вы собираете немного травы."),
                Some("Вы собираете пригоршню травы."),
            ],
            ..BASE
        },
    ),
    (
        "/obj/structure/flora/bush",
        FloraKind {
            herbal: true,
            ..BASE
        },
    ),
];

/// The vars of the flora at `path`; `None` for non-flora and the flora left to other groups.
pub fn kind_of(path: &str) -> Option<FloraKind> {
    if !is_under(path, FLORA) || is_under(path, ASH) || is_under(path, VOLCANO) {
        return None;
    }
    Some(
        KINDS
            .iter()
            .find(|(root, _)| is_under(path, root))
            .map_or(BASE, |(_, kind)| *kind),
    )
}

/// tg `harvested`, with the moment `regrow` runs (seconds of `Time::elapsed_secs_f64`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Harvested {
    pub regrow_at: f64,
}

/// tg `uprooted` and `previous_rotation` (degrees clockwise).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uprooted {
    pub rotation: u16,
}

/// The timed actions of flora.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloraJob {
    /// `item_interaction`/`attack_hand` -> `harvest`.
    Harvest,
    /// A shovel's `uproot`/`replant`.
    Uproot,
}

pub struct FloraPlugin;

impl Plugin for FloraPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `flora/attack_hand`.
        app.add_click_handler(ClickPhase::EmptyHand, 3160, |w, c| {
            crate::flora::attack_hand(w, c.client, c.mob, c.target)
        });
        // tg `flora/item_interaction`, ahead of mining and botany tools on other things.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2960, |w, c| {
            c.held
                .is_some_and(|h| crate::flora::item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_systems(Update, regrow);
    }
}

fn flora_kind(world: &World, e: Entity) -> Option<(String, FloraKind)> {
    let path = world.get::<DmObj>(e)?.path.clone();
    let kind = kind_of(&path)?;
    Some((path, kind))
}

fn harvested(world: &World, e: Entity) -> bool {
    world.get::<Harvested>(e).is_some()
}

/// tg `can_harvest(user, tool)` with `tool_behaviour`: `None` when nothing can be harvested.
fn can_harvest(kind: &FloraKind, behaviour: Option<&str>) -> bool {
    match behaviour {
        Some(b) => (kind.wooden && b == "saw") || (kind.stone && b == "mining"),
        None => kind.with_hands,
    }
}

/// tg `play_attack_sound` of flora (no default sound otherwise: the hit has its own).
fn attack_sound(world: &mut World, e: Entity, kind: &FloraKind) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    for (on, sfx) in [
        (kind.herbal, BUSH_WHACK),
        (kind.wooden, TREE_CHOP),
        (kind.stone, ROCK_TAP),
    ] {
        if on {
            playsound(world, PlaySound::new(pos, sfx, 50.0, false));
        }
    }
}

/// tg `flora/attack_hand`. True when handled.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some((_, kind)) = flora_kind(world, target) else {
        return false;
    };
    if harvested(world, target) || !can_harvest(&kind, None) {
        return false;
    }
    let name = name_of(world, target);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        &format!("Вы начинаете {} {name}...", kind.verb.0),
        &format!("{user} начинает {} {name}...", kind.verb.0),
        Span::Notice,
    );
    attack_sound(world, target, &kind);
    crate::tools::do_after(
        world,
        Actor {
            client,
            mob,
            item: None,
        },
        target,
        HARVEST_DS,
        0,
        Job::Flora(FloraJob::Harvest),
    );
    true
}

/// tg `flora/item_interaction`. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some((_, kind)) = flora_kind(world, target) else {
        return false;
    };
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    let (behaviour, speed) = world
        .get::<DmTool>(held)
        .map_or((String::new(), 1.0), |t| (t.behaviour.clone(), t.speed));
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let name = name_of(world, target);
    let user = name_of(world, mob);
    if kind.can_uproot && behaviour == "shovel" {
        let (own, others) = if world.get::<Uprooted>(target).is_some() {
            ("Вы начинаете пересаживать", "начинает пересаживать")
        } else {
            ("Вы начинаете выкапывать", "начинает выкапывать")
        };
        visible(
            world,
            mob,
            &format!("{own} {name}..."),
            &format!("{user} {others} {name}..."),
            Span::Notice,
        );
        tool_sound(world, target);
        crate::tools::do_after(
            world,
            actor,
            target,
            HARVEST_DS,
            0,
            Job::Flora(FloraJob::Uproot),
        );
        return true;
    }
    if harvested(world, target) || !can_harvest(&kind, Some(behaviour.as_str())) {
        return false;
    }
    visible(
        world,
        mob,
        &format!("Вы начинаете {} {name}...", kind.verb.0),
        &format!("{user} начинает {} {name}...", kind.verb.0),
        Span::Notice,
    );
    attack_sound(world, target, &kind);
    crate::tools::do_after(
        world,
        actor,
        target,
        HARVEST_DS * speed,
        0,
        Job::Flora(FloraJob::Harvest),
    );
    true
}

/// `tool.play_tool_sound(src, 50)` of a shovel (its `usesound`).
fn tool_sound(world: &mut World, target: Entity) {
    if let Some(&pos) = world.get::<TilePos>(target) {
        playsound(world, PlaySound::new(pos, SHOVEL_DIG, 50.0, true));
    }
}

/// End of a [`FloraJob`] (`crate::tools` checked nobody moved), the state checked again.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: FloraJob) {
    let target = action.target;
    let client = action.client;
    let Some((path, kind)) = flora_kind(world, target) else {
        return;
    };
    let name = name_of(world, target);
    let user = name_of(world, mob);
    match job {
        FloraJob::Uproot => {
            if !kind.can_uproot {
                return;
            }
            if let Some(uprooted) = world.get::<Uprooted>(target).copied() {
                visible(
                    world,
                    mob,
                    &format!("Вы пересаживаете {name}."),
                    &format!("{user} пересаживает {name}."),
                    Span::Notice,
                );
                // `replant`: `anchored = initial(anchored)`, turned back.
                let anchored = world.get::<DmObj>(target).is_some_and(|o| o.anchored);
                set_anchored(world, target, anchored);
                world.entity_mut(target).remove::<Uprooted>();
                rotate(world, target, (360 - uprooted.rotation) % 360);
            } else {
                visible(
                    world,
                    mob,
                    &format!("Вы выкапываете {name}."),
                    &format!("{user} выкапывает {name}."),
                    Span::Notice,
                );
                uproot(world, client, target, &path);
            }
            if world.get_entity(target).is_ok() {
                tool_sound(world, target);
            }
        }
        FloraJob::Harvest => {
            if harvested(world, target) {
                return;
            }
            let behaviour = action
                .item
                .and_then(|i| world.get::<DmTool>(i))
                .map(|t| t.behaviour.clone());
            if !can_harvest(&kind, behaviour.as_deref()) {
                return;
            }
            visible(
                world,
                mob,
                &format!("Вы {} {name}.", kind.verb.1),
                &format!("{user} {} {name}.", kind.verb.2),
                Span::Notice,
            );
            attack_sound(world, target, &kind);
            if harvest(world, Some(client), target, &path, &kind, 1.0) && kind.delete_on_harvest {
                remove(world, target);
            }
        }
    }
}

/// `uproot`: unanchored and turned `pick(-90, 90)`; a tree's fall; a stump is just removed.
fn uproot(world: &mut World, client: Entity, target: Entity, path: &str) {
    if is_under(path, STUMP) {
        tell(world, client, Span::Notice, "Вам удаётся убрать пень.");
        remove(world, target);
        return;
    }
    set_anchored(world, target, false);
    let rotation = if world
        .get_resource_mut::<CombatRng>()
        .is_none_or(|mut rng| rng.chance(50.0))
    {
        270
    } else {
        90
    };
    world.entity_mut(target).insert(Uprooted { rotation });
    rotate(world, target, rotation);
    if is_under(path, TREE) {
        tree_fall(world, target);
    }
}

fn rotate(world: &mut World, target: Entity, by: u16) {
    if let Some(mut appearance) = world.get_mut::<Appearance>(target) {
        let rotation = (appearance.rotation + by) % 360;
        if appearance.rotation != rotation {
            appearance.rotation = rotation;
        }
    }
}

fn tree_fall(world: &mut World, target: Entity) {
    if let Some(&pos) = world.get::<TilePos>(target) {
        playsound(
            world,
            PlaySound::new(pos, TREE_FALL, 100.0, false).extrarange(TREE_FALL_EXTRARANGE),
        );
    }
}

/// `qdel(src)` of flora.
fn remove(world: &mut World, target: Entity) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    world.despawn(target);
    crate::construction::tile_changed(world, pos);
}

/// `get_products_list`: `rand(low, high)` weighted picks.
pub fn roll_products(kind: &FloraKind, rng: &mut CombatRng) -> Vec<(&'static str, u32)> {
    let mut out: Vec<(&'static str, u32)> = Vec::new();
    let total: u32 = kind.products.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return out;
    }
    let amount = rng.range(kind.low, kind.high);
    for _ in 0..amount {
        let mut roll = rng.range(1, total);
        let Some(&(product, _)) = kind.products.iter().find(|(_, w)| {
            if roll <= *w {
                return true;
            }
            roll -= w;
            false
        }) else {
            continue;
        };
        match out.iter_mut().find(|(p, _)| *p == product) {
            Some((_, n)) => *n += 1,
            None => out.push((product, 1)),
        }
    }
    out
}

/// `harvest`'s message choice (the "three regions" method; `harvest_message_true_thresholds`
/// is never set by the station's flora).
pub fn harvest_message(kind: &FloraKind, created: u32) -> Option<&'static str> {
    let [low, med, high] = kind.messages;
    let mut message = med.or(high).or(low)?;
    let (lo, hi, n) = (kind.low as f32, kind.high as f32, created as f32);
    let comparison = n - lo;
    // DM `round(x)` with one argument floors.
    let middle = ((hi - lo) / 2.0).floor() + lo;
    if let Some(low) = low
        && comparison < (middle - lo) / 2.0
    {
        message = low;
    }
    // tg compares `products - low` with `(high - middle) / 2`, a threshold measured from
    // `middle`: a middling harvest got the high message. Measured from `low` here (deviation).
    if let Some(high) = high
        && comparison > (middle - lo) + (hi - middle) / 2.0
    {
        message = high;
    }
    Some(message)
}

/// tg `harvest(user, product_amount_multiplier)`: the products on the flora's tile, the
/// message, `harvested` and the regrowth timer; a tree falls and leaves a stump, a stump is
/// removed instead. False when nothing was made.
fn harvest(
    world: &mut World,
    client: Option<Entity>,
    target: Entity,
    path: &str,
    kind: &FloraKind,
    multiplier: f32,
) -> bool {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    if is_under(path, STUMP) {
        // `stump/harvest`.
        if let Some(client) = client {
            tell(world, client, Span::Notice, "Вам удаётся убрать пень.");
        }
        remove(world, target);
        return false;
    }
    let products = world
        .get_resource_mut::<CombatRng>()
        .map(|mut rng| roll_products(kind, &mut rng))
        .unwrap_or_default();
    if products.is_empty() {
        return false;
    }
    let mut created = 0;
    for (product, amount) in products {
        // `round(amount * product_amount_multiplier, 1)`.
        let amount = (amount as f32 * multiplier).round() as u32;
        created += amount;
        if is_under(product, "/obj/item/stack") {
            crate::construction::spawn_stack(world, product, pos, amount);
        } else {
            for _ in 0..amount {
                crate::construction::spawn_obj(world, product, pos);
            }
        }
    }
    if let Some(client) = client
        && let Some(message) = harvest_message(kind, created)
    {
        tell(world, client, Span::Notice, message);
    }
    if !kind.delete_on_harvest {
        let ds = world
            .get_resource_mut::<CombatRng>()
            .map_or(REGROW_LOW_DS, |mut rng| {
                rng.range(REGROW_LOW_DS, REGROW_HIGH_DS)
            });
        let now = world.resource::<Time>().elapsed_secs_f64();
        world.entity_mut(target).insert(Harvested {
            regrow_at: now + f64::from(ds) / 10.0,
        });
    } else {
        world.entity_mut(target).insert(Harvested {
            regrow_at: f64::INFINITY,
        });
    }
    if is_under(path, TREE) {
        // `tree/harvest`: it falls, a stump stays.
        tree_fall(world, target);
        crate::construction::spawn_obj(world, STUMP, pos);
    }
    true
}

/// `regrow`. Runs over harvested flora only (a few grass patches per round).
fn regrow(time: Res<Time>, harvested: Query<(Entity, &Harvested)>, mut commands: Commands) {
    let now = time.elapsed_secs_f64();
    for (e, h) in &harvested {
        if h.regrow_at <= now {
            commands.entity(e).remove::<Harvested>();
        }
    }
}

/// `flora/atom_deconstruct`: unharvested flora leaves 60 % of a harvest. True when `dm` was
/// flora.
pub(crate) fn deconstruct(world: &mut World, target: Entity, dm: &DmObj) -> bool {
    let Some(kind) = kind_of(&dm.path) else {
        return false;
    };
    if is_under(&dm.path, XMAS_PRESENTS) || harvested(world, target) {
        return true;
    }
    if is_under(&dm.path, STUMP) {
        return true;
    }
    harvest(world, None, target, &dm.path, &kind, DESTROYED_MULTIPLIER);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_the_subtypes() {
        assert_eq!(
            kind_of("/obj/structure/flora/tree/pine/style_2"),
            Some(TREE_KIND)
        );
        let dead = kind_of("/obj/structure/flora/tree/dead/style_3").unwrap();
        assert_eq!((dead.low, dead.high, dead.herbal), (2, 6, false));
        let pile = kind_of("/obj/structure/flora/rock/pile/style_2").unwrap();
        assert_eq!((pile.low, pile.high), (5, 10));
        assert_eq!(
            kind_of("/obj/structure/flora/rock/pile/siderite")
                .unwrap()
                .products,
            &[(SIDERITE, 1)]
        );
        let grass = kind_of("/obj/structure/flora/grass/green/style_2").unwrap();
        assert!(grass.with_hands && !grass.delete_on_harvest);
        let bush = kind_of("/obj/structure/flora/bush/reed").unwrap();
        assert!(bush.products.is_empty() && bush.can_uproot);
        assert!(kind_of("/obj/structure/flora/ash/cacti").is_none());
        assert!(kind_of("/obj/structure/table").is_none());
        assert!(!kind_of("/obj/structure/flora/rock").unwrap().can_uproot);
    }

    #[test]
    fn only_the_right_tool_harvests() {
        let tree = kind_of(TREE).unwrap();
        assert!(can_harvest(&tree, Some("saw")));
        assert!(!can_harvest(&tree, Some("mining")));
        assert!(!can_harvest(&tree, None));
        let rock = kind_of("/obj/structure/flora/rock").unwrap();
        assert!(can_harvest(&rock, Some("mining")));
        let grass = kind_of("/obj/structure/flora/grass").unwrap();
        assert!(can_harvest(&grass, None));
        assert!(!can_harvest(&grass, Some("saw")));
    }

    #[test]
    fn products_stay_in_range() {
        let mut rng = CombatRng(7);
        for _ in 0..50 {
            let logs = roll_products(&TREE_KIND, &mut rng);
            let n: u32 = logs.iter().map(|(_, n)| n).sum();
            assert!((6..=10).contains(&n), "{logs:?}");
            assert!(logs.iter().all(|(p, _)| *p == LOG));
        }
        let grass = kind_of("/obj/structure/flora/grass").unwrap();
        for _ in 0..50 {
            let n: u32 = roll_products(&grass, &mut rng).iter().map(|(_, n)| n).sum();
            assert!(n <= 2);
        }
        assert!(roll_products(&BASE, &mut rng).is_empty());
    }

    #[test]
    fn harvest_messages_split_in_three() {
        // Tree: 6..10, middle 8: below 7 low, above 9 high.
        let [low, med, high] = TREE_KIND.messages;
        assert_eq!(harvest_message(&TREE_KIND, 6), low);
        assert_eq!(harvest_message(&TREE_KIND, 8), med);
        assert_eq!(harvest_message(&TREE_KIND, 10), high);
        // Rocks have only the medium one.
        let rock = kind_of("/obj/structure/flora/rock").unwrap();
        assert_eq!(harvest_message(&rock, 10), rock.messages[1]);
        assert_eq!(harvest_message(&BASE, 1), None);
    }
}
