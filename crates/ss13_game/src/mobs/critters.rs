//! Crabs and turtles (server), after tg `/mob/living/basic/crab` (`/datum/element/sideway_movement`)
//! and `/mob/living/basic/turtle` (`turtle.dm`, `turtle_ability.dm`, `turtle.bt.json`).
//!
//! - A crab walks sideways: moving straight ahead or back it turns 90° (left or right), any
//!   other way it keeps facing where it faced (`sideway_movement/on_move`).
//! - A turtle grows a tree on its shell ([`Turtle`]): every second (`SSprocessing`) the plant
//!   nutrients it drank (a reagent container clicked on it, 1.5 s, 5 u) push their path (E-Z-Nutrient,
//!   Robust Harvest, Enduro Grow: healer; Left 4 Zed, uranium: mutator; pest killer: killer) by
//!   1-3 a second and drain 0.5 u; with nothing drunk its leading path grows 0.5. At 250 the tree
//!   is grown (`turtle_trees.dmi`) and the turtle's AI uses its ability whenever it is ready
//!   (2 min): 6 pulses 3 s apart over trays (healer: +5 plant health; mutator: +1 instability;
//!   killer: -2 weeds) and, for the killer, 20 brute to cockroaches and rats within 5 tiles.
//! - A turtle eats seeds whose plant bears food and spits out the produce 30 s later (random
//!   seeds come back out after 20 s); it headbutts the legs of people around and sniffs trays and
//!   potted plants (each once a minute).
// shortcut: no swimming crabs or speech mod (`crustacean` replacements), no turtle happiness
// (`express_happiness`), mirrored sprite when turning, resting or warp effect; a spat random seed
// is a new packet (its stats are not kept).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, Overlay, TilePos};
use ss13_world::{Moved, SpatialIndex};

use super::BasicMob;
use super::ai::AiCtx;
use super::ai::behaviors::{DELAY_FAIL, DELAY_SUCCEED, Perform, can_see};
use crate::click_handlers::ClickCtx;
use crate::combat::CombatRng;
use crate::health::{DamageType, Stat};
use crate::tools::{Actor, DoAfter};

const CRAB: &str = "/mob/living/basic/crab";
const TURTLE: &str = "/mob/living/basic/turtle";
/// `REQUIRED_TREE_GROWTH`, `UPPER_BOUND_VOLUME`, `LOWER_BOUND_VOLUME`.
const REQUIRED_TREE_GROWTH: f32 = 250.0;
const UPPER_BOUND_VOLUME: f32 = 50.0;
const LOWER_BOUND_VOLUME: f32 = 10.0;
/// The turtle's `create_reagents(150)`.
const TURTLE_VOLUME: f32 = 150.0;
/// `turtle_tree` `cooldown_time`, `maximum_intervals` (0 to 5: six pulses),
/// `time_between_intervals`, `tree_range`.
const TREE_COOLDOWN: f64 = 120.0;
const TREE_PULSES: u32 = 6;
const TREE_PULSE_SECS: f64 = 3.0;
const TREE_RANGE: u32 = 5;
const PETS_DMI: &str = "icons/mob/simple/pets.dmi";
const TREES_DMI: &str = "icons/mob/simple/turtle_trees.dmi";
const RANDOM_SEEDS: &str = "/obj/item/seeds/random";
const CONTAINER_ROOT: &str = "/obj/item/reagent_containers";

/// A turtle's tree path (`PATH_PLANT_HEALER`, `PATH_PLANT_MUTATOR`, `PATH_PEST_KILLER`, in the
/// order of `path_growth_progress`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreePath {
    Healer,
    Mutator,
    Killer,
}

impl TreePath {
    const ALL: [Self; 3] = [Self::Healer, Self::Mutator, Self::Killer];

    /// `path_requirements`.
    fn of_reagent(path: &str) -> Option<Self> {
        match path {
            "/datum/reagent/plantnutriment/eznutriment"
            | "/datum/reagent/plantnutriment/robustharvestnutriment"
            | "/datum/reagent/plantnutriment/endurogrow" => Some(Self::Healer),
            "/datum/reagent/plantnutriment/left4zednutriment" | "/datum/reagent/uranium" => {
                Some(Self::Mutator)
            }
            "/datum/reagent/toxin/pestkiller" => Some(Self::Killer),
            _ => None,
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    /// `evolution_gains` `tree_appearance`.
    fn tree_state(self) -> &'static str {
        match self {
            Self::Healer => "healer_tree",
            Self::Mutator => "mutator_tree",
            Self::Killer => "killer_tree",
        }
    }
}

/// A crab's facing (`sideway_movement`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sideways(pub Dir);

/// The turtle's tree and stomach.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Turtle {
    /// `path_growth_progress`.
    pub growth: [f32; 3],
    /// `developed_path`.
    pub developed: Option<TreePath>,
    /// Its reagents (`create_reagents(150)`): reagent type -> units.
    pub nutrients: Vec<(String, f32)>,
    /// The tree ability's cooldown end.
    pub ability_ready_at: f64,
    /// Pulses of the ability still to come, and the next one's time.
    pub pulses: u32,
    pub next_pulse: f64,
    /// `process_food` timers: when, and what comes out.
    pub spit: Vec<(f64, String)>,
    /// The tree overlay shown: (dead, developed).
    shown: Option<(bool, Option<TreePath>)>,
}

impl Turtle {
    fn new() -> Self {
        Self {
            growth: [0.0; 3],
            developed: None,
            nutrients: Vec::new(),
            ability_ready_at: 0.0,
            pulses: 0,
            next_pulse: 0.0,
            spit: Vec::new(),
            shown: None,
        }
    }

    /// `retrieve_destined_path`: the first path with the most growth, healer without any.
    pub fn destined(&self) -> TreePath {
        let mut best = (0.0, TreePath::Healer);
        for path in TreePath::ALL {
            if self.growth[path.index()] > best.0 {
                best = (self.growth[path.index()], path);
            }
        }
        best.1
    }

    /// One `process`: what it drank grows the tree, else its destined path does. Returns the
    /// path the tree just finished growing into.
    pub fn grow_tree(&mut self) -> Option<TreePath> {
        if self.developed.is_some() {
            return None;
        }
        if self.nutrients.is_empty() {
            let path = self.destined();
            return self.add_growth(path, 0.5);
        }
        let mut evolved = None;
        let drunk = self.nutrients.clone();
        for (reagent, volume) in &drunk {
            let amount = if *volume >= UPPER_BOUND_VOLUME {
                3.0
            } else if *volume >= LOWER_BOUND_VOLUME {
                2.0
            } else if *volume >= 1.0 {
                1.0
            } else {
                0.0
            };
            if let Some(path) = TreePath::of_reagent(reagent)
                && amount > 0.0
                && evolved.is_none()
            {
                evolved = self.add_growth(path, amount);
            }
        }
        for entry in &mut self.nutrients {
            entry.1 -= 0.5;
        }
        self.nutrients.retain(|(_, v)| *v > 0.0);
        evolved
    }

    /// `set_plant_growth`.
    fn add_growth(&mut self, path: TreePath, amount: f32) -> Option<TreePath> {
        self.growth[path.index()] += amount;
        if self.growth[path.index()] >= REQUIRED_TREE_GROWTH {
            self.developed = Some(path);
            return Some(path);
        }
        None
    }

    fn total(&self) -> f32 {
        self.nutrients.iter().map(|(_, v)| v).sum()
    }
}

pub(super) fn build(app: &mut App) {
    app.init_resource::<TurtleClock>().add_systems(
        Update,
        (
            crab_sideways.after(ss13_world::WorldSystems::Movement),
            turtles.after(crate::health::HealthSystems),
        ),
    );
}

#[derive(Resource, Default)]
struct TurtleClock(f64);

pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    if is_under(&def.path, CRAB) {
        let dir = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
        world.entity_mut(mob).insert(Sideways(dir));
    }
    if is_under(&def.path, TURTLE) {
        world.entity_mut(mob).insert(Turtle::new());
    }
}

/// `turn(dir, 90)` (counterclockwise) or `turn(dir, -90)`.
pub fn turn90(dir: Dir, counterclockwise: bool) -> Dir {
    let (dx, dy) = dir.offset();
    let (nx, ny) = if counterclockwise {
        (-dy, dx)
    } else {
        (dy, -dx)
    };
    Dir::DMI_ORDER
        .into_iter()
        .find(|d| d.offset() == (nx, ny))
        .unwrap_or(dir)
}

/// `sideway_movement/on_move`: keep the old facing unless walking straight ahead or back.
pub fn sideways_dir(old: Dir, moved: Dir, counterclockwise: bool) -> Dir {
    if moved == old || moved == old.reverse() {
        turn90(old, counterclockwise)
    } else {
        old
    }
}

fn crab_sideways(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        let Some(Sideways(old)) = world.get::<Sideways>(m.entity).copied() else {
            continue;
        };
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let ccw = rng.chance(50.0);
        world.resource_mut::<CombatRng>().0 = rng.0;
        let new = sideways_dir(old, m.dir, ccw);
        world.entity_mut(m.entity).insert(Sideways(new));
        if let Some(mut facing) = world.get_mut::<Facing>(m.entity) {
            facing.set_if_neq(Facing(new));
        }
    }
}

/// Turtles: `process` every second, the tree ability's pulses, spat food and the tree overlay.
// Cost: per tick a pass over the (few) turtles; trays in 5 tiles at a pulse.
fn turtles(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs_f64();
    let now = world.resource::<Time>().elapsed_secs_f64();
    let fires = {
        let mut clock = world.resource_mut::<TurtleClock>();
        clock.0 += delta;
        let n = clock.0.floor();
        clock.0 -= n;
        n as u32
    };
    let turtles: Vec<Entity> = world
        .query_filtered::<Entity, With<Turtle>>()
        .iter(world)
        .collect();
    for turtle in turtles {
        let alive = world.get::<Stat>(turtle) != Some(&Stat::Dead);
        let Some(mut t) = world.get::<Turtle>(turtle).cloned() else {
            continue;
        };
        if alive {
            for _ in 0..fires {
                t.grow_tree();
            }
        }
        let due: Vec<String> = t
            .spit
            .iter()
            .filter(|(at, _)| *at <= now)
            .map(|(_, p)| p.clone())
            .collect();
        t.spit.retain(|(at, _)| *at > now);
        let pulse = t.pulses > 0 && now >= t.next_pulse;
        if pulse {
            if alive {
                t.pulses -= 1;
                t.next_pulse += TREE_PULSE_SECS;
            } else {
                t.pulses = 0;
            }
        }
        let developed = t.developed;
        let shown = (!alive, developed);
        let refresh = t.shown != Some(shown);
        t.shown = Some(shown);
        world.entity_mut(turtle).insert(t);
        let conscious = world.get::<Stat>(turtle) == Some(&Stat::Conscious);
        if !due.is_empty() && conscious {
            spit_out(world, turtle, &due);
        }
        if pulse
            && alive
            && let Some(path) = developed
        {
            tree_effect(world, turtle, path);
        }
        if refresh {
            tree_overlay(world, turtle, !alive, developed);
        }
    }
}

/// `process_food`: the produce (or the indigestible seed) comes out.
fn spit_out(world: &mut World, turtle: Entity, items: &[String]) {
    let Some(&pos) = world.get::<TilePos>(turtle) else {
        return;
    };
    for path in items {
        crate::construction::spawn_obj(world, path, pos);
    }
    super::defense::emote(world, turtle, "выплёвывает немного еды.");
}

/// `update_overlays`: the growing, grown or withered tree on the shell.
fn tree_overlay(world: &mut World, turtle: Entity, dead: bool, developed: Option<TreePath>) {
    let (dmi, state, dy) = match (dead, developed) {
        (true, Some(_)) => (PETS_DMI, "dead_tree", -2),
        (true, None) => (PETS_DMI, "growing_tree", -2),
        (false, Some(path)) => (TREES_DMI, path.tree_state(), 2),
        (false, None) => (PETS_DMI, "growing_tree", 2),
    };
    let Some(icon_state) = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(dmi, state))
    else {
        return;
    };
    if let Some(mut look) = world.get_mut::<Appearance>(turtle) {
        look.overlays = vec![Overlay {
            icon_state,
            dir: None,
            color: None,
            pixel_offset: [0, dy],
            plane: None,
            only_dirs: 0,
        }];
    }
}

/// `turtle_tree/tree_effect` of each path, over what is within `tree_range` in view.
fn tree_effect(world: &mut World, turtle: Entity, path: TreePath) {
    let Some(&from) = world.get::<TilePos>(turtle) else {
        return;
    };
    let around: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(from, TREE_RANGE)
        .filter(|&e| e != turtle)
        .collect();
    for e in around {
        let Some(&at) = world.get::<TilePos>(e) else {
            continue;
        };
        if !can_see(world, from, at, TREE_RANGE) {
            continue;
        }
        if path == TreePath::Killer
            && world.get::<BasicMob>(e).is_some_and(|b| {
                is_under(&b.def.path, "/mob/living/basic/cockroach")
                    || is_under(&b.def.path, "/mob/living/basic/mouse/rat")
            })
        {
            super::defense::hurt(world, e, DamageType::Brute, 20.0, None);
            continue;
        }
        let Some(mut tray) = world.get_mut::<crate::botany::tray::Tray>(e) else {
            continue;
        };
        if tray.myseed.is_none() {
            continue;
        }
        match path {
            TreePath::Healer => tray.adjust_plant_health(5.0),
            TreePath::Killer => {
                tray.weedlevel =
                    (tray.weedlevel - 2.0).clamp(0.0, crate::botany::tray::MAX_TRAY_WEEDS);
            }
            TreePath::Mutator => {
                if let Some(seed) = tray.myseed.as_mut() {
                    seed.adjust_instability(1.0);
                }
            }
        }
    }
}

/// `ability_available` of the grown tree's ability.
pub fn turtle_ability_ready(world: &World, turtle: Entity, now: f64) -> bool {
    world
        .get::<Turtle>(turtle)
        .is_some_and(|t| t.developed.is_some() && t.pulses == 0 && now >= t.ability_ready_at)
        && world.get::<Stat>(turtle) == Some(&Stat::Conscious)
}

/// `use_mob_ability` -> `turtle_tree/Activate`.
pub fn use_tree_ability(ctx: &mut AiCtx) -> Perform {
    if !turtle_ability_ready(ctx.world, ctx.pawn, ctx.now) {
        return DELAY_FAIL;
    }
    let now = ctx.now;
    let Some(mut t) = ctx.world.get_mut::<Turtle>(ctx.pawn) else {
        return DELAY_FAIL;
    };
    t.ability_ready_at = now + TREE_COOLDOWN;
    t.pulses = TREE_PULSES;
    t.next_pulse = now;
    DELAY_SUCCEED
}

/// `pre_eat_food`: only seeds whose product is grown food.
pub fn turtle_accepts(world: &World, food: Entity) -> bool {
    let Some(path) = world
        .get::<ss13_mapload::DmItem>(food)
        .map(|i| i.path.as_str())
    else {
        return false;
    };
    if is_under(path, RANDOM_SEEDS) {
        return true;
    }
    world
        .get_resource::<crate::botany::Botany>()
        .and_then(|b| b.0.seed_def(path).map(|s| s.product.clone()))
        .is_some_and(|product| is_under(&product, "/obj/item/food/grown"))
}

/// `post_eat`: the produce comes out in 30 s, a random seed in 20 s.
pub fn turtle_ate(world: &mut World, turtle: Entity, food: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(path) = world
        .get::<ss13_mapload::DmItem>(food)
        .map(|i| i.path.clone())
    else {
        return;
    };
    let spit = if is_under(&path, RANDOM_SEEDS) {
        Some((now + 20.0, path))
    } else {
        world
            .get_resource::<crate::botany::Botany>()
            .and_then(|b| b.0.seed_def(&path).map(|s| s.product.clone()))
            .filter(|p| !p.is_empty())
            .map(|p| (now + 30.0, p))
    };
    if let (Some(spit), Some(mut t)) = (spit, world.get_mut::<Turtle>(turtle)) {
        t.spit.push(spit);
    }
}

/// `turtle/item_interaction` with a reagent container: it drinks plant nutrients (1.5 s).
pub fn turtle_drink(world: &mut World, ctx: &ClickCtx) -> bool {
    let (Some(item), turtle) = (ctx.held, ctx.target) else {
        return false;
    };
    if world.get::<Turtle>(turtle).is_none() {
        return false;
    }
    let container = world
        .get::<ss13_mapload::DmItem>(item)
        .is_some_and(|i| is_under(&i.path, CONTAINER_ROOT));
    if !container {
        return false;
    }
    let name = super::defense::name_of(world, turtle);
    let Some(holder) = world
        .get::<crate::chem::Reagents>(item)
        .map(|r| r.0.clone())
    else {
        crate::tools::tell(world, ctx.client, Span::Warning, "Пусто!");
        return true;
    };
    if world.get::<Stat>(turtle) == Some(&Stat::Dead) {
        crate::tools::tell(world, ctx.client, Span::Warning, format!("{name} мертва!"));
        return true;
    }
    let wanted = world
        .get_resource::<crate::chem::Chem>()
        .is_some_and(|chem| {
            holder
                .reagents
                .iter()
                .any(|r| TreePath::of_reagent(&chem.0.def(r.id).path).is_some())
        });
    if !wanted {
        crate::tools::tell(
            world,
            ctx.client,
            Span::Warning,
            format!("{name} отказывается пить!"),
        );
        return true;
    }
    crate::tools::do_after(
        world,
        Actor {
            client: ctx.client,
            mob: ctx.mob,
            item: Some(item),
        },
        turtle,
        15.0,
        0,
        crate::construction::Job::Fauna(super::farm::FarmJob::TurtleDrink),
    );
    true
}

/// The drink's end: `trans_to(reagents, 5)`.
pub fn turtle_drank(world: &mut World, _mob: Entity, action: &DoAfter) {
    let (Some(item), turtle) = (action.item, action.target) else {
        return;
    };
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let (Some(mut holder), Some(t)) = (
        world
            .get::<crate::chem::Reagents>(item)
            .map(|r| r.0.clone()),
        world.get::<Turtle>(turtle).cloned(),
    ) else {
        return;
    };
    let room = (TURTLE_VOLUME - t.total()).max(0.0);
    let mut stomach =
        ss13_reagents::holder::Holder::new(room, 0, ss13_reagents::holder::Owner::default());
    let moved = holder.trans_to(&chem, &mut stomach, 5.0, 1.0, None, true, false);
    if moved <= 0.0 {
        return;
    }
    if let Some(mut r) = world.get_mut::<crate::chem::Reagents>(item) {
        r.0 = holder;
    }
    crate::chem::refresh_container(world, &chem, item);
    if let Some(mut t) = world.get_mut::<Turtle>(turtle) {
        for r in &stomach.reagents {
            let path = chem.def(r.id).path.clone();
            match t.nutrients.iter_mut().find(|(p, _)| *p == path) {
                Some(entry) => entry.1 += r.volume,
                None => t.nutrients.push((path, r.volume)),
            }
        }
    }
    if let Some(&pos) = world.get::<TilePos>(turtle) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/items/drink.ogg", 25.0, true),
        );
    }
    super::defense::emote(world, turtle, "радостно пьёт.");
}

/// `turtle/examine`: what its tree exudes.
pub fn examine_lines(world: &World, mob: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(t) = world.get::<Turtle>(mob) else {
        return;
    };
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        lines.push((
            Span::Notice,
            "Её дерево, кажется, совсем засохло...".to_owned(),
        ));
        return;
    }
    let path = t.destined();
    let kind = match path {
        TreePath::Killer => "убивающие вредителей",
        TreePath::Healer => "лечащие растения",
        TreePath::Mutator => "мутирующие растения",
    };
    let state = if t.growth[path.index()] >= REQUIRED_TREE_GROWTH {
        "дерево, похоже, выросло полностью"
    } else {
        "дерево ещё не развилось"
    };
    lines.push((
        Span::Notice,
        format!("Её дерево источает {kind} свойства... {state}."),
    ));
}

/// `update_interaction_target` of the turtle's headbutt victim (`legged_conscious_human`) or
/// flora (`sniffable_hydro`: a planted tray, or a potted plant).
pub fn acquire_turtle_target(ctx: &mut AiCtx, key: super::ai::Key, flora: bool) -> Perform {
    const VISION: u32 = 9;
    let Some(from) = ctx.pawn_pos() else {
        return DELAY_FAIL;
    };
    let world = &*ctx.world;
    let mut found: Vec<(u32, Entity)> = world
        .resource::<SpatialIndex>()
        .in_radius(from, VISION)
        .filter(|&e| e != ctx.pawn)
        .filter(|&e| {
            if flora {
                world
                    .get::<crate::botany::tray::Tray>(e)
                    .is_some_and(|t| t.myseed.is_some())
                    || world
                        .get::<ss13_mapload::DmItem>(e)
                        .is_some_and(|i| is_under(&i.path, "/obj/item/kirbyplants"))
            } else {
                world
                    .get::<crate::medical::wounds::Limbs>(e)
                    .is_some_and(|l| {
                        !l.limb(crate::zone::BodyZone::LLeg).missing
                            || !l.limb(crate::zone::BodyZone::RLeg).missing
                    })
                    && world.get::<BasicMob>(e).is_none()
                    && world.get::<Stat>(e) == Some(&Stat::Conscious)
            }
        })
        .filter_map(|e| Some((from.chebyshev(*world.get::<TilePos>(e)?)?, e)))
        .collect();
    found.sort_by_key(|&(d, e)| (d, e.to_bits()));
    let best = found.into_iter().map(|(_, e)| e).find(|&e| {
        world
            .get::<TilePos>(e)
            .is_some_and(|&p| can_see(world, from, p, VISION))
    });
    match best {
        Some(e) => {
            ctx.bb.set(key, super::ai::Value::Entity(e));
            DELAY_SUCCEED
        }
        None => {
            ctx.bb.clear(key);
            DELAY_FAIL
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crabs_turn_only_walking_ahead_or_back() {
        assert_eq!(turn90(Dir::North, false), Dir::East);
        assert_eq!(turn90(Dir::North, true), Dir::West);
        assert_eq!(sideways_dir(Dir::North, Dir::North, false), Dir::East);
        assert_eq!(sideways_dir(Dir::North, Dir::South, true), Dir::West);
        assert_eq!(sideways_dir(Dir::North, Dir::East, true), Dir::North);
    }

    #[test]
    fn turtle_trees_grow_by_nutrients() {
        let mut t = Turtle::new();
        assert_eq!(t.destined(), TreePath::Healer);
        // Nothing drunk: the healer path by 0.5 a second.
        t.grow_tree();
        assert_eq!(t.growth, [0.5, 0.0, 0.0]);
        t.nutrients
            .push(("/datum/reagent/toxin/pestkiller".to_owned(), 60.0));
        t.grow_tree();
        assert_eq!(t.growth[TreePath::Killer.index()], 3.0);
        assert_eq!(t.nutrients[0].1, 59.5);
        assert_eq!(t.destined(), TreePath::Killer);
        t.growth[TreePath::Killer.index()] = 249.0;
        assert_eq!(t.grow_tree(), Some(TreePath::Killer));
        assert_eq!(t.developed, Some(TreePath::Killer));
        // A grown tree grows no more.
        assert_eq!(t.grow_tree(), None);
    }
}
