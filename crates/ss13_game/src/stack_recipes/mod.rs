//! Stack crafting menus (server): tg `/datum/stack_recipe`, `/datum/stack_recipe_list` and
//! `/obj/item/stack` `ui_interact`/`make_item`/`building_checks`
//! (`code/game/objects/items/stacks/stack.dm`, `stack_recipe.dm`).
//!
//! - Using a stack with recipes in hand (`attack_self`) opens its menu as a list: the recipes
//!   and sublists of every list `get_main_recipes` gives the type ([`data::STACKS`], generated
//!   from the DM lists by `scripts/gen-stack-recipes.py`); a sublist opens its own list.
//! - A recipe that makes a stack (`max_res_amount > 1`) asks how many times to make it when the
//!   stack has enough for more than one (tg's multiplier buttons): at most
//!   `min(amount / req_amount, max_res_amount / res_amount)`.
//! - `building_checks` on the builder's tile before and after the `do_after` (`time`, on the
//!   builder): enough material, `CRAFT_ONE_PER_TURF`, `CRAFT_CHECK_DIRECTION`
//!   (`valid_build_direction`), `CRAFT_ON_SOLID_GROUND`, `CRAFT_CHECK_DENSITY`,
//!   `STACK_CHECK_CARDINALS`, `STACK_CHECK_ADJACENT`, the tram checks.
//! - The result appears on the builder's tile facing the builder's dir (a turf is placed on
//!   top, a stack gets `res_amount × multiplier`), `req_amount × multiplier` is used up, an item
//!   goes into the hands, a new storage is emptied ("BubbleWrap").
// shortcut: the menu is tg's list input instead of the `StackCrafting` window (no icons,
// categories or search); `TRAIT_QUICK_BUILD` (`trait_modifier`) is not ported, the speed-up is
// kept in the data; `CRAFT_MUST_BE_LEARNED` recipes are hidden (no learned recipes yet);
// materials of the result (`set_custom_materials`) are not applied.

pub mod data;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj, DmTurf};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_world::{Density, SpatialIndex};

use crate::construction::Job;
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::tools::{Actor, do_after, stack_amount, tell};

/// `CRAFT_*` (`code/__DEFINES/crafting.dm`).
pub const CRAFT_MUST_BE_LEARNED: u16 = 1 << 0;
pub const CRAFT_ONE_PER_TURF: u16 = 1 << 1;
pub const CRAFT_IS_FULLTILE: u16 = 1 << 2;
pub const CRAFT_CHECK_DIRECTION: u16 = 1 << 3;
pub const CRAFT_ON_SOLID_GROUND: u16 = 1 << 4;
pub const CRAFT_CHECK_DENSITY: u16 = 1 << 5;
/// `STACK_CHECK_*` (`placement_checks`).
pub const STACK_CHECK_CARDINALS: u8 = 1 << 0;
pub const STACK_CHECK_ADJACENT: u8 = 1 << 1;
pub const STACK_CHECK_TRAM_EXCLUSIVE: u8 = 1 << 3;
/// `obj_flags`: `BLOCKS_CONSTRUCTION`, `BLOCKS_CONSTRUCTION_DIR`, `IGNORE_DENSITY`.
const BLOCKS_CONSTRUCTION: u32 = 1 << 9;
const BLOCKS_CONSTRUCTION_DIR: u32 = 1 << 10;
const IGNORE_DENSITY: u32 = 1 << 11;
const TURF: &str = "/turf";
const STORAGE: &str = "/obj/item/storage";

/// One `/datum/stack_recipe`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recipe {
    pub title: &'static str,
    pub result: &'static str,
    pub req: u32,
    pub res: u32,
    pub max_res: u32,
    /// Deciseconds.
    pub time_ds: f32,
    pub flags: u16,
    pub placement: u8,
    /// `trait_modifier` of `TRAIT_QUICK_BUILD` (1: none).
    pub quick_build: f32,
}

/// The generated data's constructor (positional, as `/datum/stack_recipe/New`).
#[allow(clippy::too_many_arguments)]
pub const fn r(
    title: &'static str,
    result: &'static str,
    req: u32,
    res: u32,
    max_res: u32,
    time_ds: f32,
    flags: u16,
    placement: u8,
    quick_build: f32,
) -> Recipe {
    Recipe {
        title,
        result,
        req,
        res,
        max_res,
        time_ds,
        flags,
        placement,
        quick_build,
    }
}

/// A menu line: a recipe or a `/datum/stack_recipe_list` (title, recipes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Entry {
    Recipe(Recipe),
    List(&'static str, &'static [Recipe]),
}

/// What a stack recipe's `do_after` finishes ([`crate::construction::Job::StackRecipe`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackJob {
    pub stack: Entity,
    pub recipe: &'static Recipe,
    pub multiplier: u32,
}

#[derive(Clone, Copy, Debug)]
enum Line {
    Recipe(&'static Recipe),
    List(&'static [Recipe]),
}

/// On a prompt: the stack whose menu it is and its lines.
#[derive(Component, Clone, Debug)]
struct MenuFor(Entity, Vec<Line>);

/// On a prompt: the multiplier question of a recipe.
#[derive(Component, Clone, Copy, Debug)]
struct MultiplierFor(Entity, &'static Recipe);

pub struct StackRecipesPlugin;

impl Plugin for StackRecipesPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 3239, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        app.add_systems(
            Update,
            menu_answers
                .in_set(ss13_tgui::server::UiSystems::Handle)
                .after(ss13_tgui::prompt::PromptSystems::Answer),
        );
    }
}

/// The recipe lists of a stack type: the most specific `get_main_recipes` override.
pub fn recipes_of(path: &str) -> Option<&'static [&'static [Entry]]> {
    data::STACKS
        .iter()
        .filter(|(root, _)| is_under(path, root))
        .max_by_key(|(root, _)| root.len())
        .map(|(_, lists)| *lists)
}

fn stack_path(world: &World, stack: Entity) -> Option<String> {
    world.get::<crate::tools::Stack>(stack)?;
    world.get::<DmItem>(stack).map(|i| i.path.clone())
}

fn shown(recipe: &Recipe) -> bool {
    recipe.flags & CRAFT_MUST_BE_LEARNED == 0
}

fn label(recipe: &Recipe) -> String {
    if recipe.max_res > 1 {
        format!("{} × {} ({} шт.)", recipe.res, recipe.title, recipe.req)
    } else {
        format!("{} ({} шт.)", recipe.title, recipe.req)
    }
}

/// tg `attack_self` -> `interact` of a stack with recipes. True when the stack has a menu.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, stack: Entity) -> bool {
    let Some(lists) = stack_path(world, stack).and_then(|p| recipes_of(&p)) else {
        return false;
    };
    let mut lines = Vec::new();
    for entry in lists.iter().flat_map(|l| l.iter()) {
        match entry {
            Entry::Recipe(recipe) if shown(recipe) => lines.push(Line::Recipe(recipe)),
            Entry::List(_, recipes) if recipes.iter().any(shown) => {
                lines.push(Line::List(*recipes))
            }
            _ => {}
        }
    }
    if lines.is_empty() {
        return false;
    }
    let titles: Vec<String> = lists
        .iter()
        .flat_map(|l| l.iter())
        .filter_map(|entry| match entry {
            Entry::Recipe(recipe) if shown(recipe) => Some(label(recipe)),
            Entry::List(title, recipes) if recipes.iter().any(shown) => Some(format!("{title} ▸")),
            _ => None,
        })
        .collect();
    open_menu(world, client, mob, stack, lines, titles);
    true
}

fn open_menu(
    world: &mut World,
    client: Entity,
    mob: Entity,
    stack: Entity,
    lines: Vec<Line>,
    items: Vec<String>,
) {
    let amount = stack_amount(world, stack);
    let title = crate::examine::name_of(world, stack);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            format!("Количество: {amount}. Что построить?"),
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(MenuFor(stack, lines));
}

/// tg's `ui_state` (hands) of the menu: a conscious, capable builder still holding the stack.
fn can_use(world: &World, mob: Entity, stack: Entity) -> bool {
    world.get::<crate::health::Stat>(mob) == Some(&crate::health::Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
        && world
            .get::<Inventory>(mob)
            .is_some_and(|inv| inv.hands.contains(&Some(stack)))
}

fn menu_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let menu = world.get::<MenuFor>(prompt).cloned();
        let multiplier = world.get::<MultiplierFor>(prompt).copied();
        match (menu, multiplier, answer) {
            (Some(MenuFor(stack, lines)), _, Some(PromptAnswer::Item(i))) => {
                let Some(&line) = lines.get(i) else {
                    continue;
                };
                if !can_use(world, actor, stack) {
                    continue;
                }
                match line {
                    Line::Recipe(recipe) => pick(world, client, actor, stack, recipe),
                    Line::List(recipes) => {
                        let visible: Vec<&'static Recipe> =
                            recipes.iter().filter(|r| shown(r)).collect();
                        let items = visible.iter().map(|r| label(r)).collect();
                        let lines = visible.into_iter().map(Line::Recipe).collect();
                        open_menu(world, client, actor, stack, lines, items);
                    }
                }
            }
            (_, Some(MultiplierFor(stack, recipe)), Some(PromptAnswer::Number(n))) => {
                if can_use(world, actor, stack) && n.is_finite() && n >= 1.0 {
                    make_item(world, client, actor, stack, recipe, n as u32);
                }
            }
            _ => {}
        }
    }
}

/// The most a recipe can be made at once from `amount` (tg's multiplier buttons).
pub fn max_multiplier(recipe: &Recipe, amount: u32) -> u32 {
    if recipe.max_res <= 1 || recipe.req == 0 {
        return 1;
    }
    (amount / recipe.req)
        .min(recipe.max_res / recipe.res.max(1))
        .max(1)
}

fn pick(world: &mut World, client: Entity, mob: Entity, stack: Entity, recipe: &'static Recipe) {
    let max = max_multiplier(recipe, stack_amount(world, stack));
    if max <= 1 {
        make_item(world, client, mob, stack, recipe, 1);
        return;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            recipe.title,
            "Сколько раз сделать?",
            PromptKind::Number {
                default: 1.0,
                min: 1.0,
                max: f64::from(max),
                round: true,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(MultiplierFor(stack, recipe));
}

/// tg `make_item`. True when it built or started building.
pub fn make_item(
    world: &mut World,
    client: Entity,
    mob: Entity,
    stack: Entity,
    recipe: &'static Recipe,
    multiplier: u32,
) -> bool {
    if multiplier < 1 || multiplier > max_multiplier(recipe, u32::MAX) {
        return false;
    }
    if !building_checks(world, client, mob, stack, recipe, multiplier) {
        return false;
    }
    let job = StackJob {
        stack,
        recipe,
        multiplier,
    };
    if recipe.time_ds > 0.0 {
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы начинаете строить: {}...", recipe.title),
        );
        let actor = Actor {
            client,
            mob,
            item: Some(stack),
        };
        return do_after(world, actor, mob, recipe.time_ds, 0, Job::StackRecipe(job));
    }
    build(world, client, mob, job)
}

/// [`Job::StackRecipe`] done: the checks again, then the result.
pub fn complete(world: &mut World, client: Entity, mob: Entity, job: StackJob) {
    if world.get_entity(job.stack).is_err() || !can_use(world, mob, job.stack) {
        return;
    }
    if building_checks(world, client, mob, job.stack, job.recipe, job.multiplier) {
        build(world, client, mob, job);
    }
}

fn any_of(world: &World, pos: TilePos, root: &str) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| crate::construction::path_of(world, e).is_some_and(|p| is_under(&p, root)))
}

/// tg `valid_build_direction`.
fn valid_build_direction(world: &World, pos: TilePos, dir: Dir, fulltile: bool) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter_map(|&e| Some((e, world.get::<DmObj>(e)?)))
        .filter(|(_, o)| o.obj_flags & BLOCKS_CONSTRUCTION_DIR != 0)
        .all(|(e, _)| !fulltile && world.get::<Facing>(e).map_or(Dir::South, |f| f.0) != dir)
}

/// tg `building_checks`, with tg's balloon alerts as chat lines.
pub fn building_checks(
    world: &mut World,
    client: Entity,
    mob: Entity,
    stack: Entity,
    recipe: &Recipe,
    multiplier: u32,
) -> bool {
    let fail = |world: &mut World, text: &str| {
        tell(world, client, Span::Warning, text);
        false
    };
    if stack_amount(world, stack) < recipe.req * multiplier {
        return fail(world, "Недостаточно материала!");
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    if recipe.flags & CRAFT_ONE_PER_TURF != 0 && any_of(world, pos, recipe.result) {
        return fail(world, "Здесь такое уже есть!");
    }
    let dir = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
    if recipe.flags & CRAFT_CHECK_DIRECTION != 0
        && !valid_build_direction(world, pos, dir, recipe.flags & CRAFT_IS_FULLTILE != 0)
    {
        return fail(world, "Сюда не поместится!");
    }
    if recipe.flags & CRAFT_ON_SOLID_GROUND != 0 {
        let (closed, groundless) = crate::construction::turf_at(world, pos)
            .and_then(|t| world.get::<DmTurf>(t))
            .map_or((false, false), |t| (t.closed, t.groundless));
        if closed {
            return fail(world, "Нельзя построить в стене!");
        }
        // shortcut: tram thermoplastic over groundless turfs is not ported.
        if groundless {
            return fail(world, "Нужна твёрдая поверхность!");
        }
    }
    if recipe.flags & CRAFT_CHECK_DENSITY != 0 {
        let blocked = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .filter_map(|&e| Some((e, world.get::<DmObj>(e)?.obj_flags)))
            .any(|(e, flags)| {
                (world.get::<Density>(e).is_some() && flags & IGNORE_DENSITY == 0)
                    || flags & BLOCKS_CONSTRUCTION != 0
            });
        if blocked {
            return fail(world, "Что-то мешает!");
        }
    }
    if recipe.placement & STACK_CHECK_CARDINALS != 0
        && Dir::CARDINALS
            .iter()
            .any(|&d| any_of(world, pos.step(d), recipe.result))
    {
        return fail(world, "Нельзя строить вплотную к другому такому же!");
    }
    if recipe.placement & STACK_CHECK_ADJACENT != 0 {
        let near = (-1..=1).any(|dx| {
            (-1..=1).any(|dy| {
                any_of(
                    world,
                    TilePos::new(pos.x + dx, pos.y + dy, pos.z),
                    recipe.result,
                )
            })
        });
        if near {
            return fail(world, "Нельзя строить рядом с другим таким же!");
        }
    }
    // There are no trams: `STACK_CHECK_TRAM_FORBIDDEN` always passes.
    if recipe.placement & STACK_CHECK_TRAM_EXCLUSIVE != 0 {
        return fail(world, "Это строится только на трамвае!");
    }
    true
}

fn build(world: &mut World, client: Entity, mob: Entity, job: StackJob) -> bool {
    let StackJob {
        stack,
        recipe,
        multiplier,
    } = job;
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    let turf = is_under(recipe.result, TURF);
    let created = if recipe.max_res > 1 {
        crate::construction::spawn_stack(world, recipe.result, pos, recipe.res * multiplier)
    } else if turf {
        crate::construction::place_on_top(world, pos, recipe.result, true)
    } else {
        crate::construction::spawn_obj(world, recipe.result, pos)
    };
    let Some(created) = created else {
        tell(world, client, Span::Warning, "Не получается это построить.");
        return false;
    };
    crate::tools::use_stack(world, stack, recipe.req * multiplier);
    let text = if turf {
        "покрытие уложено"
    } else if recipe.max_res > 1 {
        "предметы сделаны"
    } else {
        "предмет сделан"
    };
    tell(world, client, Span::Notice, text);
    if turf {
        return true;
    }
    if recipe.max_res <= 1 {
        let dir = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
        crate::door_structures::face(world, created, dir);
    }
    if world.get::<DmItem>(created).is_some() {
        if is_under(recipe.result, STORAGE) {
            let inside: Vec<Entity> = world
                .get::<crate::storage::Stored>(created)
                .map(|s| s.items().to_vec())
                .unwrap_or_default();
            for item in inside {
                crate::storage::take_out(world, item, pos);
                world.despawn(item);
            }
        }
        crate::assemblies::server::to_hands(world, mob, created);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iron_has_the_metal_menu_and_subtypes_inherit_it() {
        let lists = recipes_of("/obj/item/stack/sheet/iron/fifty").expect("iron has recipes");
        assert!(std::ptr::eq(lists[0], data::METAL_RECIPES));
        let girder = lists[0]
            .iter()
            .find_map(|e| match e {
                Entry::Recipe(r) if r.result == "/obj/structure/girder" => Some(*r),
                _ => None,
            })
            .expect("iron builds girders");
        assert_eq!((girder.req, girder.time_ds), (2, 40.0));
        assert_ne!(girder.flags & CRAFT_ONE_PER_TURF, 0);
        assert_eq!(girder.quick_build, 0.75);
        assert!(recipes_of("/obj/item/stack/cable_coil").is_none());
    }

    #[test]
    fn every_recipe_list_parsed() {
        for path in [
            "/obj/item/stack/sheet/glass",
            "/obj/item/stack/sheet/mineral/gold",
            "/obj/item/stack/sheet/mineral/wood",
            "/obj/item/stack/sheet/cloth",
            "/obj/item/stack/sheet/cardboard",
            "/obj/item/stack/sheet/plastic",
            "/obj/item/stack/rods",
            "/obj/item/stack/sheet/bluespace_crystal",
        ] {
            let lists = recipes_of(path).unwrap_or_else(|| panic!("{path} has recipes"));
            assert!(lists.iter().all(|l| !l.is_empty()), "{path}");
        }
        // BandaStation's wooden floor sublist replaces the plain wood tile recipe.
        let wood = recipes_of("/obj/item/stack/sheet/mineral/wood").unwrap()[0];
        assert!(
            wood.iter()
                .any(|e| matches!(e, Entry::List("Деревянный пол", r) if r.len() == 10))
        );
        assert!(
            !wood
                .iter()
                .any(|e| matches!(e, Entry::Recipe(r) if r.result == "/obj/item/stack/tile/wood"))
        );
    }

    #[test]
    fn multiplier_is_bounded_by_amount_and_max_result() {
        let tiles = r(
            "floor tile",
            "/obj/item/stack/tile/iron/base",
            1,
            4,
            20,
            0.0,
            0,
            0,
            1.0,
        );
        assert_eq!(max_multiplier(&tiles, 50), 5);
        assert_eq!(max_multiplier(&tiles, 3), 3);
        let bed = r("bed", "/obj/structure/bed", 2, 1, 1, 0.0, 0, 0, 1.0);
        assert_eq!(max_multiplier(&bed, 50), 1);
    }
}
