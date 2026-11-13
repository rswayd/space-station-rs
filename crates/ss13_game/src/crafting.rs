//! The crafting menu, after tg `/datum/component/personal_crafting`
//! (`code/datums/components/crafting/_crafting.dm`), its recipes (`/datum/crafting_recipe`,
//! converted to `ss13_mapload::crafting`), its HUD button `/atom/movable/screen/craft` (`craft` at
//! `ui_human_crafting`) and the tgui `PersonalCrafting` window.
//!
//! - Every human player mob gets the button: the headless interface [`CraftButtonUi`] on the mob
//!   (drawn by `crate::crafting_client`); its [`OPEN`] act opens the menu ([`PersonalCraftingUi`],
//!   on the mob, for its own player: tg `not_incapacitated_turf_state`).
//! - Two modes (tg `CRAFTING`/`COOKING`): the recipes of `GLOB.crafting_category_food` (foods and
//!   drinks) or the rest, without `CRAFT_MUST_BE_LEARNED` ones; the window marks which can be
//!   made now (refreshed with tgui's autoupdate).
//! - What counts (`get_surroundings`): objects on the tiles within one tile of the crafter and
//!   what it holds and wears (BYOND `range` includes the center's contents), without holograms
//!   and abstract items; stacks by amount, drainable reagent containers by their reagents (a
//!   requirement counts its subtypes), tools by `tool_behaviour` (also inside the storage the
//!   crafter carries), machines and structures by type.
//! - `make` / `make_mass` (`construct_item`): ingredients, catalysts, machines, structures, tools
//!   and placement (`crate::stack_recipes::placement_checks`), a `do_after` of the recipe time
//!   with the tool part scaled by tool speed, the checks again, then the ingredients are used
//!   (reagents into a temporary holder, stack amounts, items, consumed machines/structures), the
//!   result appears facing the crafter (a turf on top, a stack of `result_amount`), with
//!   `CRAFT_CLEARS_REAGENTS` / `CRAFT_TRANSFERS_REAGENT_COMPONENTS`, food ingredients' reagents
//!   (`/datum/component/edible/OnCraft`) and `TRAIT_HANDMADE`; an item goes into the hands. Mass
//!   crafting repeats until something fails.
// shortcut: no icons, search, material tab or food type filters in the window (tabs by
// category, dish category for food); no learned recipes (`CRAFT_MUST_BE_LEARNED` hidden);
// recipes with their own `check_requirements`/`check_tools` and the results' own
// `on_craft_completion` (guns, molotovs, clothing) are not ported (the former are never made);
// `parts` are deleted instead of moved into the result; materials of the result
// (`set_custom_materials`) are not applied; no slapcrafting.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// The HUD button (no window).
pub struct CraftButtonUi;

impl UiInterface for CraftButtonUi {
    const NAME: &'static str = "CraftingButton";
    type Data = ();
    type StaticData = ();
}

/// [`CraftButtonUi`]'s act: a click on the button (`COMSIG_SCREEN_ELEMENT_CLICK`).
pub const OPEN: &str = "click";

/// tgui `PersonalCrafting`.
pub struct PersonalCraftingUi;

impl UiInterface for PersonalCraftingUi {
    const NAME: &'static str = "PersonalCrafting";
    type Data = CraftingState;
    type StaticData = CraftingStatic;
}

/// `act('make', { recipe })`, `act('make_mass', { recipe })`: the recipe's type path.
pub const MAKE: &str = "make";
pub const MAKE_MASS: &str = "make_mass";
pub const RECIPE: &str = "recipe";
pub const TOGGLE_RECIPES: &str = "toggle_recipes";
pub const TOGGLE_COMPACT: &str = "toggle_compact";
pub const TOGGLE_MODE: &str = "toggle_mode";
/// The client's tab of what can be made now (tgui `'Can Make'`).
pub const CAN_MAKE_TAB: &str = "Можно сделать";

/// One recipe as the window shows it (`build_crafting_data`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RecipeView {
    /// The recipe's type path (the acts' `recipe`).
    pub path: String,
    pub name: String,
    pub desc: String,
    /// Its tab: the category, the dish category for food.
    pub tab: String,
    /// Ingredient lines (`reqs`).
    pub reqs: Vec<String>,
    pub catalysts: Vec<String>,
    /// Tool behaviours and tool types.
    pub tools: Vec<String>,
    /// Machines and structures.
    pub fixtures: Vec<String>,
    pub steps: Vec<String>,
    pub non_craftable: bool,
    pub mass_craftable: bool,
    pub complexity: u32,
    pub foodtypes: Vec<String>,
}

/// `ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CraftingStatic {
    /// The cooking recipes.
    pub cooking: bool,
    pub forced_mode: bool,
    /// Sorted tab names.
    pub tabs: Vec<String>,
    pub recipes: Vec<RecipeView>,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CraftingState {
    pub busy: bool,
    pub cooking: bool,
    pub display_craftable_only: bool,
    pub display_compact: bool,
    /// `craftability`: the paths of the recipes that can be made now.
    pub craftable: Vec<String>,
}

impl CraftingState {
    pub fn can_make(&self, path: &str) -> bool {
        self.craftable.iter().any(|p| p == path)
    }
}

/// The recipes of `tab` (the [`CAN_MAKE_TAB`]: what can be made), only craftable ones when
/// asked; craftable first, then by name (tgui `sortBy`).
pub fn shown<'a>(st: &'a CraftingStatic, data: &CraftingState, tab: &str) -> Vec<&'a RecipeView> {
    let mut out: Vec<&RecipeView> = st
        .recipes
        .iter()
        .filter(|r| {
            let can = data.can_make(&r.path);
            let in_tab = if tab == CAN_MAKE_TAB {
                can
            } else {
                r.tab == tab
            };
            in_tab && (can || !data.display_craftable_only)
        })
        .collect();
    out.sort_by_cached_key(|r| (!data.can_make(&r.path), r.name.to_lowercase()));
    out
}

/// A recipe's detail lines; `compact` leaves out the description, complexity, food types and
/// steps.
pub fn details(r: &RecipeView, compact: bool) -> Vec<String> {
    let mut out = Vec::new();
    if !compact && !r.desc.is_empty() {
        out.push(r.desc.clone());
    }
    for (label, list) in [
        ("Компоненты", &r.reqs),
        ("Катализаторы", &r.catalysts),
        ("Инструменты", &r.tools),
        ("Оборудование", &r.fixtures),
    ] {
        if !list.is_empty() {
            out.push(format!("{label}: {}", list.join(", ")));
        }
    }
    if !compact {
        if r.complexity > 0 {
            out.push(format!("Сложность: {}", r.complexity));
        }
        if !r.foodtypes.is_empty() {
            out.push(r.foodtypes.join(", "));
        }
        out.extend(
            r.steps
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{}. {s}", i + 1)),
        );
    }
    out
}

#[cfg(test)]
mod view_tests {
    use super::*;

    fn view(path: &str, name: &str, tab: &str) -> RecipeView {
        RecipeView {
            path: path.into(),
            name: name.into(),
            tab: tab.into(),
            reqs: vec!["1 × bun".into()],
            steps: vec!["Mix".into()],
            desc: "Tasty.".into(),
            ..Default::default()
        }
    }

    #[test]
    fn tabs_filter_and_craftable_recipes_come_first() {
        let st = CraftingStatic {
            cooking: true,
            forced_mode: false,
            tabs: vec!["Burger".into(), "Pizza".into()],
            recipes: vec![
                view("/datum/crafting_recipe/food/b", "b burger", "Burger"),
                view("/datum/crafting_recipe/food/a", "A burger", "Burger"),
                view("/datum/crafting_recipe/food/p", "pizza", "Pizza"),
            ],
        };
        let mut data = CraftingState {
            craftable: vec!["/datum/crafting_recipe/food/b".into()],
            ..Default::default()
        };
        let names = |v: Vec<&RecipeView>| v.iter().map(|r| r.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(shown(&st, &data, "Burger")), ["b burger", "A burger"]);
        assert_eq!(names(shown(&st, &data, CAN_MAKE_TAB)), ["b burger"]);
        data.display_craftable_only = true;
        assert_eq!(names(shown(&st, &data, "Burger")), ["b burger"]);
        assert!(shown(&st, &data, "Pizza").is_empty());
        let r = &st.recipes[0];
        assert_eq!(
            details(r, false),
            ["Tasty.", "Компоненты: 1 × bun", "1. Mix"]
        );
        assert_eq!(details(r, true), ["Компоненты: 1 × bun"]);
    }

    #[test]
    fn window_data_round_trips() {
        let st = CraftingStatic {
            tabs: vec!["Burger".into()],
            recipes: vec![view("/datum/crafting_recipe/food/b", "b", "Burger")],
            ..Default::default()
        };
        let bytes = ss13_tgui::encode(&st);
        assert_eq!(ss13_tgui::decode::<CraftingStatic>(&bytes).unwrap(), st);
    }
}

#[cfg(feature = "server")]
pub use server::*;

#[cfg(feature = "server")]
mod server {
    use std::collections::HashMap;
    use std::sync::Arc;

    use bevy::ecs::message::{MessageCursor, Messages};
    use bevy::prelude::*;
    use ss13_mapload::crafting::{
        CAT_FOOD, CRAFT_CLEARS_REAGENTS, CRAFT_MUST_BE_LEARNED, CRAFT_TRANSFERS_REAGENT_COMPONENTS,
        CRAFTING_FILE, CraftingData, FOOD_FLAGS_IC, RecipeDef,
    };
    use ss13_mapload::fidelity::is_under;
    use ss13_mapload::{DmFood, DmItem, DmObj, DmReagents, DmTool, MapLoadPlugin};
    use ss13_reagents::{Holder, Owner, flags};
    use ss13_shared::chat::Span;
    use ss13_shared::ui::UiStatus;
    use ss13_shared::{Dir, Facing, TilePos};
    use ss13_tgui::server::{
        AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
    };
    use ss13_world::SpatialIndex;

    use super::*;
    use crate::chem::{Chem, Reagents};
    use crate::construction::Job;
    use crate::health::Stat;
    use crate::inventory::Inventory;
    use crate::server::PlayerMob;
    use crate::tools::{Actor, DoAfter, Stack, do_after, tell};

    /// `RECIPE_DYNAMIC_TIME_COEFF`: the part of the time that tool speed scales.
    const RECIPE_DYNAMIC_TIME_COEFF: f32 = 0.85;
    /// `ABSTRACT` (`item_flags`).
    const ABSTRACT: u32 = 1 << 7;
    const REAGENT_ROOT: &str = "/datum/reagent";
    const TURF_ROOT: &str = "/turf";
    const STACK_ROOT: &str = "/obj/item/stack";
    const STORAGE_ROOT: &str = "/obj/item/storage";
    const MACHINERY_ROOT: &str = "/obj/machinery";
    const STRUCTURE_ROOT: &str = "/obj/structure";
    /// `global_blacklist`: these are tools, never ingredients.
    const GLOBAL_BLACKLIST: [&str; 18] = [
        "/obj/item/cautery/augment",
        "/obj/item/cautery/cruel/augment",
        "/obj/item/circular_saw/augment",
        "/obj/item/circular_saw/cruel/augment",
        "/obj/item/crowbar/cyborg",
        "/obj/item/hemostat/augment",
        "/obj/item/hemostat/cruel/augment",
        "/obj/item/multitool/cyborg",
        "/obj/item/retractor/augment",
        "/obj/item/retractor/cruel/augment",
        "/obj/item/scalpel/augment",
        "/obj/item/scalpel/cruel/augment",
        "/obj/item/screwdriver/cyborg",
        "/obj/item/surgicaldrill/augment",
        "/obj/item/surgicaldrill/cruel/augment",
        "/obj/item/weldingtool/largetank/cyborg",
        "/obj/item/wirecutters/cyborg",
        "/obj/item/wrench/cyborg",
    ];

    /// The converted recipes and the window's static data of both modes.
    #[derive(Resource, Default)]
    pub struct CraftingDb {
        pub recipes: Vec<RecipeDef>,
        pub by_path: HashMap<String, usize>,
        /// Crafting, cooking.
        statics: [Arc<CraftingStatic>; 2],
    }

    impl CraftingDb {
        pub fn from_data(data: CraftingData) -> Self {
            let names: HashMap<String, String> = data.names.into_iter().collect();
            let by_path = data
                .recipes
                .iter()
                .enumerate()
                .map(|(i, r)| (r.path.clone(), i))
                .collect();
            let statics =
                [false, true].map(|cooking| Arc::new(static_view(&data.recipes, &names, cooking)));
            Self {
                recipes: data.recipes,
                by_path,
                statics,
            }
        }

        /// `get_visible_recipes`: the mode's recipes a crafter knows without learning.
        fn visible(&self, cooking: bool) -> impl Iterator<Item = (usize, &RecipeDef)> {
            self.recipes
                .iter()
                .enumerate()
                .filter(move |(_, r)| visible(r, cooking))
        }
    }

    fn visible(recipe: &RecipeDef, cooking: bool) -> bool {
        recipe.cooking == cooking && recipe.crafting_flags & CRAFT_MUST_BE_LEARNED == 0
    }

    fn static_view(
        recipes: &[RecipeDef],
        names: &HashMap<String, String>,
        cooking: bool,
    ) -> CraftingStatic {
        let name = |p: &str| names.get(p).cloned().unwrap_or_else(|| p.to_owned());
        let views: Vec<RecipeView> = recipes
            .iter()
            .filter(|r| visible(r, cooking))
            .map(|r| {
                let reqs = r
                    .reqs
                    .iter()
                    .map(|q| {
                        if is_under(&q.path, REAGENT_ROOT) {
                            format!("{}: {} ед.", name(q.path.as_str()), q.amount)
                        } else {
                            format!("{} × {}", q.amount, name(q.path.as_str()))
                        }
                    })
                    .collect();
                let catalysts = r
                    .chem_catalysts
                    .iter()
                    .map(|q| format!("{}: {} ед.", name(q.path.as_str()), q.amount))
                    .collect();
                let tools = r
                    .tool_behaviors
                    .iter()
                    .cloned()
                    .chain(r.tool_paths.iter().map(|p| name(p.as_str())))
                    .collect();
                let fixtures = r
                    .machinery
                    .iter()
                    .chain(&r.structures)
                    .map(|f| name(f.path.as_str()))
                    .collect();
                let mut display = r.name.clone();
                if r.result_amount > 1 {
                    if is_under(&r.result, REAGENT_ROOT) {
                        display = format!("{display} {}u", r.result_amount);
                    } else if is_under(&r.result, STACK_ROOT) {
                        display = format!("{display} {}x", r.result_amount);
                    }
                }
                let tab = if r.category == CAT_FOOD && !r.dish_category.is_empty() {
                    r.dish_category.clone()
                } else {
                    r.category.clone()
                };
                RecipeView {
                    path: r.path.clone(),
                    name: display,
                    desc: r.desc.clone(),
                    tab,
                    reqs,
                    catalysts,
                    tools,
                    fixtures,
                    steps: r.steps.clone(),
                    non_craftable: r.non_craftable,
                    mass_craftable: r.mass_craftable,
                    complexity: r.complexity,
                    foodtypes: FOOD_FLAGS_IC
                        .iter()
                        .enumerate()
                        .filter(|(bit, _)| r.foodtypes & (1u32 << *bit) != 0)
                        .map(|(_, n)| (*n).to_owned())
                        .collect(),
                }
            })
            .collect();
        let mut tabs: Vec<String> = views.iter().map(|v| v.tab.clone()).collect();
        tabs.sort();
        tabs.dedup();
        tabs.retain(|t| !t.is_empty());
        CraftingStatic {
            cooking,
            forced_mode: false,
            tabs,
            recipes: views,
        }
    }

    /// The recipes, read from `<assets>/crafting.ron` once (none without it).
    pub fn crafting_db(world: &mut World) -> &CraftingDb {
        if !world.contains_resource::<CraftingDb>() {
            let path = world
                .get_resource::<MapLoadPlugin>()
                .map(|m| m.assets.join(CRAFTING_FILE));
            let data = match path.as_deref().map(CraftingData::read) {
                Some(Ok(data)) => data,
                Some(Err(e)) => {
                    warn!("{e:#} (rerun `ss13-mapload convert`): no crafting recipes");
                    CraftingData::default()
                }
                None => CraftingData::default(),
            };
            world.insert_resource(CraftingDb::from_data(data));
        }
        world.resource::<CraftingDb>()
    }

    /// On a human player's mob: the source of its [`CraftButtonUi`] and its menu.
    #[derive(Component, Clone, Debug, Default)]
    pub struct PersonalCrafting {
        pub cooking: bool,
        pub display_craftable_only: bool,
        pub display_compact: bool,
        static_view: Arc<CraftingStatic>,
        view: CraftingState,
        next_refresh: f64,
    }

    /// A craft in progress ([`Job::Craft`]): the recipe's index in [`CraftingDb`], whether it
    /// repeats, and how many the repetition made so far.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct CraftJob {
        pub recipe: u32,
        pub mass: bool,
        pub made: u32,
    }

    pub struct CraftingServerPlugin;

    impl Plugin for CraftingServerPlugin {
        fn build(&self, app: &mut App) {
            app.register_ui::<CraftButtonUi>()
                .register_ui::<PersonalCraftingUi>()
                .add_observer(give_button)
                .add_observer(take_button)
                .add_systems(
                    Update,
                    (
                        window_status.in_set(UiSystems::StatusHook),
                        (open_menus, menu_acts, refresh_views)
                            .chain()
                            .in_set(UiSystems::Handle),
                    ),
                );
        }
    }

    /// tg `/mob/living/carbon/human/Initialize`: `AddComponent(/datum/component/personal_crafting)`
    /// (the button needs a player to see it).
    fn give_button(
        add: On<Add<PlayerMob>>,
        players: Query<(&PlayerMob, Has<crate::human::HumanLook>)>,
        mut open: MessageWriter<OpenUi>,
        mut commands: Commands,
    ) {
        let Ok((player, true)) = players.get(add.entity) else {
            return;
        };
        commands
            .entity(add.entity)
            .try_insert(PersonalCrafting::default());
        open.write(OpenUi::new::<CraftButtonUi>(
            player.client,
            add.entity,
            add.entity,
            "",
        ));
    }

    /// The mob lost its player: its windows close with their source.
    fn take_button(remove: On<Remove<PlayerMob>>, mut commands: Commands) {
        if let Ok(mut mob) = commands.get_entity(remove.entity) {
            mob.try_remove::<PersonalCrafting>();
        }
    }

    impl ServerUi for CraftButtonUi {
        type Source = &'static PersonalCrafting;
        type UserParam = ();
        type Act = ();

        fn static_data(_: &PersonalCrafting) {}

        fn data(_: &PersonalCrafting, _: Entity, _: &()) {}

        fn parse_act(action: &str, _: &UiParams) -> Result<(), UiReject> {
            if action == OPEN {
                Ok(())
            } else {
                Err(UiReject::UnknownAction)
            }
        }
    }

    /// A parsed menu act.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum CraftAct {
        Make { recipe: String, mass: bool },
        ToggleRecipes,
        ToggleCompact,
        ToggleMode,
    }

    impl ServerUi for PersonalCraftingUi {
        type Source = &'static PersonalCrafting;
        type UserParam = ();
        type Act = CraftAct;

        fn static_data(pc: &PersonalCrafting) -> CraftingStatic {
            (*pc.static_view).clone()
        }

        fn data(pc: &PersonalCrafting, _: Entity, _: &()) -> CraftingState {
            pc.view.clone()
        }

        fn parse_act(action: &str, params: &UiParams) -> Result<CraftAct, UiReject> {
            let recipe = || {
                params
                    .get(RECIPE)
                    .filter(|p| p.starts_with("/datum/crafting_recipe/"))
                    .map(str::to_owned)
                    .ok_or_else(|| UiReject::BadParams(RECIPE.into()))
            };
            Ok(match action {
                MAKE => CraftAct::Make {
                    recipe: recipe()?,
                    mass: false,
                },
                MAKE_MASS => CraftAct::Make {
                    recipe: recipe()?,
                    mass: true,
                },
                TOGGLE_RECIPES => CraftAct::ToggleRecipes,
                TOGGLE_COMPACT => CraftAct::ToggleCompact,
                TOGGLE_MODE => CraftAct::ToggleMode,
                _ => return Err(UiReject::UnknownAction),
            })
        }
    }

    /// The button opens the clicker's own menu.
    fn open_menus(world: &mut World, mut acts: Local<MessageCursor<UiActed<CraftButtonUi>>>) {
        let pending: Vec<(Entity, Entity, Entity)> = acts
            .read(world.resource::<Messages<UiActed<CraftButtonUi>>>())
            .map(|a| (a.client, a.actor, a.source))
            .collect();
        for (client, actor, source) in pending {
            if actor != source || world.get::<PersonalCrafting>(actor).is_none() {
                continue;
            }
            refresh_view(world, actor, true);
            world.write_message(OpenUi::new::<PersonalCraftingUi>(
                client,
                actor,
                actor,
                "Меню создания",
            ));
        }
    }

    fn menu_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<PersonalCraftingUi>>>) {
        let pending: Vec<(Entity, Entity, Entity, Entity, CraftAct)> = acts
            .read(world.resource::<Messages<UiActed<PersonalCraftingUi>>>())
            .map(|a| (a.window, a.client, a.actor, a.source, a.act.clone()))
            .collect();
        for (window, client, actor, source, act) in pending {
            if actor != source {
                continue;
            }
            let Some(mut pc) = world.get_mut::<PersonalCrafting>(actor) else {
                continue;
            };
            let mut mode_changed = false;
            let mut wanted = None;
            match act {
                CraftAct::ToggleRecipes => {
                    pc.display_craftable_only = !pc.display_craftable_only;
                }
                CraftAct::ToggleCompact => pc.display_compact = !pc.display_compact,
                CraftAct::ToggleMode => {
                    pc.cooking = !pc.cooking;
                    mode_changed = true;
                }
                CraftAct::Make { recipe, mass } => wanted = Some((recipe, mass, pc.cooking)),
            }
            if mode_changed && let Some(mut w) = world.get_mut::<UiWindow>(window) {
                w.refresh_static();
            }
            if let Some((recipe, mass, cooking)) = wanted
                && let Some(index) = crafting_db(world).by_path.get(&recipe).copied()
            {
                make(world, client, actor, index, cooking, mass, 0);
            }
            refresh_view(world, actor, true);
        }
    }

    /// `ui_data` of every open menu, at tgui's autoupdate period.
    // Cost: per open menu and refresh, one scan of the 3×3 tiles and every visible recipe's
    // requirements against it (~900 cooking recipes × a few requirements).
    fn refresh_views(world: &mut World) {
        let mut q = world.query_filtered::<&UiWindow, With<UiKind<PersonalCraftingUi>>>();
        let mut mobs: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
        mobs.sort();
        mobs.dedup();
        for mob in mobs {
            refresh_view(world, mob, false);
        }
    }

    /// Recomputes the menu's data (`force`: now, else when the autoupdate is due).
    fn refresh_view(world: &mut World, mob: Entity, force: bool) {
        let now = crate::tools::now(world);
        let Some(pc) = world.get::<PersonalCrafting>(mob) else {
            return;
        };
        if !force && now < pc.next_refresh {
            return;
        }
        let cooking = pc.cooking;
        let busy = world
            .get::<DoAfter>(mob)
            .is_some_and(|d| matches!(d.job, Job::Craft(_)));
        crafting_db(world);
        let around = surroundings(world, mob);
        let db = world.resource::<CraftingDb>();
        let craftable: Vec<String> = db
            .visible(cooking)
            .filter(|(_, r)| !r.non_craftable && !r.custom_checks)
            .filter(|(_, r)| check_contents(r, &around) && check_tools(r, &around))
            .map(|(_, r)| r.path.clone())
            .collect();
        let static_view = db.statics[usize::from(cooking)].clone();
        let Some(mut pc) = world.get_mut::<PersonalCrafting>(mob) else {
            return;
        };
        let view = CraftingState {
            busy,
            cooking,
            display_craftable_only: pc.display_craftable_only,
            display_compact: pc.display_compact,
            craftable,
        };
        pc.next_refresh = now + ss13_tgui::server::REFRESH_SECS;
        pc.static_view = static_view;
        pc.view = view;
    }

    /// tg `not_incapacitated_turf_state`: closed unless conscious, disabled when incapacitated or
    /// not on a turf.
    fn window_status(world: &mut World) {
        let windows: Vec<(Entity, Entity)> = world
            .query_filtered::<(Entity, &UiWindow), With<UiKind<PersonalCraftingUi>>>()
            .iter(world)
            .map(|(w, window)| (w, window.actor))
            .collect();
        for (window, actor) in windows {
            let cap = if world.get::<Stat>(actor) != Some(&Stat::Conscious) {
                UiStatus::Close
            } else if crate::status_effects::incapacitation(world, actor).incapacitated
                || world.get::<TilePos>(actor).is_none()
            {
                UiStatus::Disabled
            } else {
                continue;
            };
            if let Some(mut w) = world.get_mut::<UiWindow>(window) {
                w.cap_status(cap);
            }
        }
    }

    /// One object around the crafter.
    #[derive(Clone, Debug)]
    struct Nearby {
        entity: Entity,
        path: String,
    }

    /// tg `get_surroundings`.
    #[derive(Default, Debug)]
    pub struct Surroundings {
        /// Items usable as ingredients, nearest first (held, then on tiles).
        items: Vec<Nearby>,
        /// Type path -> items, stack amount or reagent units.
        counts: HashMap<String, f32>,
        /// Tool behaviour -> the best (lowest) tool speed.
        behaviours: HashMap<String, f32>,
        /// Every item that can serve as a tool, with its speed.
        tools: Vec<(String, f32)>,
        machinery: Vec<Nearby>,
        structures: Vec<Nearby>,
        /// Drainable reagent containers among the items.
        containers: Vec<Entity>,
    }

    fn path_of(world: &World, e: Entity) -> Option<String> {
        world
            .get::<DmItem>(e)
            .map(|i| i.path.clone())
            .or_else(|| world.get::<DmObj>(e).map(|o| o.path.clone()))
    }

    /// tg `get_environment`: the objects on the tiles within a tile, then what the crafter
    /// holds and wears (the center's contents), without holograms and abstract items.
    fn environment(world: &World, mob: Entity) -> Vec<Entity> {
        let mut out: Vec<Entity> = world
            .get::<Inventory>(mob)
            .map(|inv| {
                inv.hands
                    .iter()
                    .flatten()
                    .copied()
                    .chain(inv.equipped())
                    .collect()
            })
            .unwrap_or_default();
        if let Some(&pos) = world.get::<TilePos>(mob) {
            let mut near: Vec<Entity> = world
                .resource::<SpatialIndex>()
                .in_radius(pos, 1)
                .filter(|&e| e != mob)
                .collect();
            // Own tile first, then by entity for a stable order.
            near.sort_by_key(|&e| (world.get::<TilePos>(e) != Some(&pos), e));
            out.extend(near);
        }
        out.retain(|&e| {
            world.get::<crate::holodeck::Hologram>(e).is_none()
                && world
                    .get::<DmItem>(e)
                    .is_none_or(|i| i.item_flags & ABSTRACT == 0)
        });
        out
    }

    pub fn surroundings(world: &World, mob: Entity) -> Surroundings {
        let mut s = Surroundings::default();
        let chem = world.get_resource::<Chem>();
        for e in environment(world, mob) {
            let Some(path) = path_of(world, e) else {
                continue;
            };
            if world.get::<DmItem>(e).is_none() {
                if is_under(&path, MACHINERY_ROOT) {
                    s.machinery.push(Nearby { entity: e, path });
                } else if is_under(&path, STRUCTURE_ROOT) {
                    s.structures.push(Nearby { entity: e, path });
                }
                continue;
            }
            if let Some(tool) = world.get::<DmTool>(e) {
                note_tool(&mut s, tool);
            }
            s.tools.push((
                path.clone(),
                world.get::<DmTool>(e).map_or(1.0, |t| t.speed),
            ));
            if GLOBAL_BLACKLIST.contains(&path.as_str()) {
                continue;
            }
            let count = world.get::<Stack>(e).map_or(1, |st| st.amount) as f32;
            *s.counts.entry(path.clone()).or_default() += count;
            if world.get::<Stack>(e).is_none()
                && world.get::<DmReagents>(e).is_some()
                && let (Some(r), Some(chem)) = (world.get::<Reagents>(e), chem)
                && r.0.is_drainable()
                && !r.0.reagents.is_empty()
            {
                for reagent in &r.0.reagents {
                    *s.counts
                        .entry(chem.0.def(reagent.id).path.clone())
                        .or_default() += reagent.volume;
                }
                s.containers.push(e);
            }
            s.items.push(Nearby { entity: e, path });
        }
        // Tools in the storage the crafter carries (`within_source`).
        let carried: Vec<Entity> = world
            .get::<Inventory>(mob)
            .map(|inv| {
                inv.hands
                    .iter()
                    .flatten()
                    .copied()
                    .chain(inv.equipped())
                    .collect()
            })
            .unwrap_or_default();
        for bag in carried {
            let Some(stored) = world.get::<crate::storage::Stored>(bag) else {
                continue;
            };
            for &item in stored.items() {
                if let (Some(tool), Some(path)) = (world.get::<DmTool>(item), path_of(world, item))
                {
                    note_tool(&mut s, tool);
                    s.tools.push((path, tool.speed));
                }
            }
        }
        s
    }

    fn note_tool(s: &mut Surroundings, tool: &DmTool) {
        if tool.behaviour.is_empty() {
            return;
        }
        // tg keeps the highest (slowest) speed of a behaviour; the best tool counts here, as
        // for `tool_paths`.
        let speed = s
            .behaviours
            .entry(tool.behaviour.clone())
            .or_insert(tool.speed);
        *speed = speed.min(tool.speed);
    }

    /// The type can be one of the recipe's ingredients (`check_contents`).
    fn usable(recipe: &RecipeDef, path: &str) -> bool {
        !recipe.blacklist.iter().any(|b| b == path)
            && !(recipe.blacklist_result && path == recipe.result)
    }

    /// tg `check_contents`.
    pub fn check_contents(recipe: &RecipeDef, s: &Surroundings) -> bool {
        let reqs = recipe.reqs.iter().all(|req| {
            let have: f32 = s
                .counts
                .iter()
                .filter(|(p, _)| is_under(p, &req.path) && usable(recipe, p))
                .map(|(_, n)| *n)
                .sum();
            have + 0.0001 >= req.amount
        });
        let catalysts = recipe
            .chem_catalysts
            .iter()
            .all(|c| s.counts.get(&c.path).copied().unwrap_or(0.0) + 0.0001 >= c.amount);
        let machinery = recipe
            .machinery
            .iter()
            .all(|f| s.machinery.iter().any(|m| is_under(&m.path, &f.path)));
        let structures = recipe
            .structures
            .iter()
            .all(|f| s.structures.iter().any(|m| is_under(&m.path, &f.path)));
        reqs && catalysts && machinery && structures
    }

    /// tg `check_tools`.
    pub fn check_tools(recipe: &RecipeDef, s: &Surroundings) -> bool {
        recipe
            .tool_behaviors
            .iter()
            .all(|b| s.behaviours.contains_key(b))
            && recipe
                .tool_paths
                .iter()
                .all(|t| s.tools.iter().any(|(p, _)| is_under(p, t)))
    }

    /// `construct_item`'s time: the tool part (`RECIPE_DYNAMIC_TIME_COEFF`) split between the
    /// tools and scaled by their speed. Deciseconds, rounded to 0.1 s.
    pub fn craft_time(recipe: &RecipeDef, s: &Surroundings) -> f32 {
        let tools_used = recipe.tool_behaviors.len() + recipe.tool_paths.len();
        let mut time = recipe.time_ds;
        if tools_used > 0 {
            let dynamic = recipe.time_ds * RECIPE_DYNAMIC_TIME_COEFF;
            time -= dynamic;
            let share = dynamic / tools_used as f32;
            for tool in &recipe.tool_paths {
                let best = s
                    .tools
                    .iter()
                    .filter(|(p, _)| is_under(p, tool))
                    .map(|(_, speed)| *speed)
                    .fold(10.0_f32, f32::min);
                time += share * best;
            }
            for behaviour in &recipe.tool_behaviors {
                time += share * s.behaviours.get(behaviour).copied().unwrap_or(1.0);
            }
        }
        time.round()
    }

    /// `perform_all_checks`; false after telling the crafter why.
    fn all_checks(
        world: &mut World,
        client: Entity,
        mob: Entity,
        recipe: &RecipeDef,
        s: &Surroundings,
    ) -> bool {
        if !check_contents(recipe, s) {
            tell(
                world,
                client,
                Span::Warning,
                "Не удалось создать: не хватает компонентов.",
            );
            return false;
        }
        if !check_tools(recipe, s) {
            tell(
                world,
                client,
                Span::Warning,
                "Не удалось создать: нет нужного инструмента.",
            );
            return false;
        }
        crate::stack_recipes::placement_checks(
            world,
            client,
            mob,
            &recipe.result,
            recipe.crafting_flags as u16,
            recipe.placement_checks as u8,
        )
    }

    fn made_summary(world: &mut World, client: Entity, made: u32) {
        if made > 0 {
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы сделали предметов: {made}."),
            );
        }
    }

    /// `ui_act("make"/"make_mass")` -> `make_action` -> `construct_item` up to the `do_after`.
    /// True when the craft started.
    pub fn make(
        world: &mut World,
        client: Entity,
        mob: Entity,
        index: usize,
        cooking: bool,
        mass: bool,
        made: u32,
    ) -> bool {
        if world.get::<DoAfter>(mob).is_some() {
            return false;
        }
        let Some(recipe) = crafting_db(world).recipes.get(index).cloned() else {
            return false;
        };
        // Only what the menu shows can be made.
        if !visible(&recipe, cooking)
            || recipe.non_craftable
            || recipe.custom_checks
            || (mass && !recipe.mass_craftable)
        {
            return false;
        }
        let s = surroundings(world, mob);
        if !all_checks(world, client, mob, &recipe, &s) {
            made_summary(world, client, made);
            return false;
        }
        let actor = Actor {
            client,
            mob,
            item: world.get::<Inventory>(mob).and_then(Inventory::active_item),
        };
        let job = CraftJob {
            recipe: index as u32,
            mass,
            made,
        };
        do_after(
            world,
            actor,
            mob,
            craft_time(&recipe, &s),
            0,
            Job::Craft(job),
        )
    }

    /// [`Job::Craft`] done: the checks again, the result, and the next one of a mass craft.
    pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: CraftJob) {
        let client = action.client;
        let Some(recipe) = crafting_db(world).recipes.get(job.recipe as usize).cloned() else {
            return;
        };
        let s = surroundings(world, mob);
        if !all_checks(world, client, mob, &recipe, &s) {
            made_summary(world, client, job.made);
            refresh_view(world, mob, true);
            return;
        }
        if !construct(world, client, mob, &recipe) {
            made_summary(world, client, job.made);
            refresh_view(world, mob, true);
            return;
        }
        tell(
            world,
            client,
            Span::Notice,
            format!("{}: готово.", ss13_shared::capitalize(&recipe.name)),
        );
        if job.mass {
            // A failure tells why and the count so far.
            let made = job.made + 1;
            make(
                world,
                client,
                mob,
                job.recipe as usize,
                recipe.cooking,
                true,
                made,
            );
        }
        refresh_view(world, mob, true);
    }

    /// The `do_after` of a craft failed (`construct_item` returns `"."`).
    pub fn interrupted(world: &mut World, mob: Entity, action: &DoAfter, job: CraftJob) {
        tell(world, action.client, Span::Warning, "Создание прервано.");
        made_summary(world, action.client, job.made);
        refresh_view(world, mob, true);
    }

    /// The `construct_item` part after the checks: the result, the used ingredients.
    fn construct(world: &mut World, client: Entity, mob: Entity, recipe: &RecipeDef) -> bool {
        let Some(&pos) = world.get::<TilePos>(mob) else {
            return false;
        };
        let turf = is_under(&recipe.result, TURF_ROOT);
        let created = if turf {
            crate::construction::place_on_top(world, pos, &recipe.result, true)
        } else if is_under(&recipe.result, STACK_ROOT) {
            crate::construction::spawn_stack(
                world,
                &recipe.result,
                pos,
                recipe.result_amount.max(1),
            )
        } else {
            crate::construction::spawn_obj(world, &recipe.result, pos)
        };
        let Some(result) = created else {
            tell(world, client, Span::Warning, "Не получается это сделать.");
            return false;
        };
        // Its own `Initialize` (food and container holders) before the transfers.
        world.flush();
        if !turf {
            if is_under(&recipe.result, STORAGE_ROOT) && recipe.delete_contents {
                let inside: Vec<Entity> = world
                    .get::<crate::storage::Stored>(result)
                    .map(|s| s.items().to_vec())
                    .unwrap_or_default();
                for item in inside {
                    crate::storage::take_out(world, item, pos);
                    world.despawn(item);
                }
            }
            let dir = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
            crate::door_structures::face(world, result, dir);
        }
        let (used, mut holder) = use_reqs(world, mob, recipe, result);
        if recipe.crafting_flags & CRAFT_CLEARS_REAGENTS != 0
            && let Some(mut r) = world.get_mut::<Reagents>(result)
        {
            r.0.clear();
        }
        if let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) {
            if recipe.crafting_flags & CRAFT_TRANSFERS_REAGENT_COMPONENTS != 0
                && let Some(mut r) = world.get_mut::<Reagents>(result)
            {
                let volume = holder.total_volume;
                holder.trans_to(&chem, &mut r.0, volume, 1.0, None, true, false);
            }
            // `/datum/component/edible/OnCraft`: food ingredients' holders join the result's.
            if world.get::<DmFood>(result).is_some() {
                for &part in &used {
                    if world.get::<DmFood>(part).is_none() {
                        continue;
                    }
                    let Some(mut part_holder) = world.get::<Reagents>(part).map(|r| r.0.clone())
                    else {
                        continue;
                    };
                    if let Some(mut r) = world.get_mut::<Reagents>(result) {
                        r.0.maximum_volume += part_holder.maximum_volume;
                        let volume = part_holder.maximum_volume;
                        part_holder.trans_to(&chem, &mut r.0, volume, 1.0, None, true, false);
                    }
                }
                if let Some(mut r) = world.get_mut::<Reagents>(result) {
                    r.0.maximum_volume = r.0.maximum_volume.ceil();
                }
            }
        }
        // `on_craft_completion`: `TRAIT_HANDMADE` when the crafter has a mind.
        if world.get::<DmFood>(result).is_some()
            && world.get::<crate::mind::MindData>(mob).is_some()
        {
            world.entity_mut(result).insert(crate::food::Handmade);
        }
        for part in used {
            if world.get_entity(part).is_err() {
                continue;
            }
            if world.get::<DmFood>(part).is_some() {
                crate::food_storage::release(world, part);
            }
            world.despawn(part);
        }
        crate::construction::tile_changed(world, pos);
        if !turf && world.get::<DmItem>(result).is_some() {
            crate::assemblies::server::to_hands(world, mob, result);
        }
        true
    }

    /// tg `get_used_reqs`: takes the ingredients. Returns the items and objects to delete and
    /// the reagents taken.
    fn use_reqs(
        world: &mut World,
        mob: Entity,
        recipe: &RecipeDef,
        result: Entity,
    ) -> (Vec<Entity>, Holder) {
        let mut holder = Holder::new(f32::INFINITY, flags::NO_REACT, Owner::default());
        let mut used: Vec<Entity> = Vec::new();
        let s = surroundings(world, mob);
        let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
        for req in &recipe.reqs {
            let mut amount = req.amount;
            if is_under(&req.path, REAGENT_ROOT) {
                let Some(chem) = &chem else {
                    continue;
                };
                for &container in &s.containers {
                    if amount <= 0.0 {
                        break;
                    }
                    let Some(mut r) = world.get_mut::<Reagents>(container) else {
                        continue;
                    };
                    let matching: Vec<(u16, f32)> =
                        r.0.reagents
                            .iter()
                            .filter(|g| {
                                let path = &chem.def(g.id).path;
                                is_under(path, &req.path) && usable(recipe, path)
                            })
                            .map(|g| (g.id, g.volume))
                            .collect();
                    for (id, volume) in matching {
                        if amount <= 0.0 {
                            break;
                        }
                        let take = amount.min(volume);
                        r.0.trans_to(chem, &mut holder, take, 1.0, Some(id), true, false);
                        amount -= take;
                    }
                }
                continue;
            }
            for item in &s.items {
                if amount <= 0.0 {
                    break;
                }
                if item.entity == result
                    || used.contains(&item.entity)
                    || !is_under(&item.path, &req.path)
                    || !usable(recipe, &item.path)
                    || world.get_entity(item.entity).is_err()
                {
                    continue;
                }
                match world.get::<Stack>(item.entity).map(|st| st.amount) {
                    Some(have) => {
                        let take = (amount.ceil() as u32).min(have);
                        crate::tools::use_stack(world, item.entity, take);
                        amount -= take as f32;
                    }
                    None => {
                        used.push(item.entity);
                        amount -= 1.0;
                    }
                }
            }
        }
        for fixture in recipe.machinery.iter().chain(&recipe.structures) {
            if !fixture.consume {
                continue;
            }
            if let Some(found) = s
                .machinery
                .iter()
                .chain(&s.structures)
                .find(|m| is_under(&m.path, &fixture.path) && !used.contains(&m.entity))
            {
                used.push(found.entity);
            }
        }
        (used, holder)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use ss13_mapload::crafting::Req;

        fn recipe() -> RecipeDef {
            RecipeDef {
                path: "/datum/crafting_recipe/food/burger".into(),
                name: "Plain Burger".into(),
                result: "/obj/item/food/burger/plain".into(),
                reqs: vec![
                    Req {
                        path: "/datum/reagent/consumable/ethanol".into(),
                        amount: 10.0,
                    },
                    Req {
                        path: "/obj/item/food/bun".into(),
                        amount: 2.0,
                    },
                ],
                blacklist: vec!["/obj/item/food/bun/fake".into()],
                blacklist_result: true,
                tool_behaviors: vec!["knife".into()],
                time_ds: 40.0,
                cooking: true,
                ..Default::default()
            }
        }

        fn around(counts: &[(&str, f32)], knife: Option<f32>) -> Surroundings {
            let mut s = Surroundings::default();
            for (p, n) in counts {
                s.counts.insert((*p).to_owned(), *n);
            }
            if let Some(speed) = knife {
                s.behaviours.insert("knife".into(), speed);
            }
            s
        }

        #[test]
        fn subtypes_count_and_blacklisted_types_do_not() {
            let r = recipe();
            let s = around(
                &[
                    ("/datum/reagent/consumable/ethanol/beer", 10.0),
                    ("/obj/item/food/bun", 1.0),
                    ("/obj/item/food/bun/fake", 1.0),
                ],
                Some(1.0),
            );
            assert!(!check_contents(&r, &s));
            let s = around(
                &[
                    ("/datum/reagent/consumable/ethanol/beer", 4.0),
                    ("/datum/reagent/consumable/ethanol", 6.0),
                    ("/obj/item/food/bun", 1.0),
                    ("/obj/item/food/bun/sesame", 1.0),
                ],
                Some(1.0),
            );
            assert!(check_contents(&r, &s));
            assert!(check_tools(&r, &s));
            assert!(!check_tools(&r, &around(&[], None)));
        }

        #[test]
        fn the_result_type_is_no_ingredient() {
            let mut r = recipe();
            r.reqs = vec![Req {
                path: "/obj/item/food/burger".into(),
                amount: 1.0,
            }];
            let s = around(&[("/obj/item/food/burger/plain", 1.0)], Some(1.0));
            assert!(!check_contents(&r, &s));
            r.blacklist_result = false;
            assert!(check_contents(&r, &s));
        }

        #[test]
        fn tool_speed_scales_the_tool_part_of_the_time() {
            let r = recipe();
            // 15% fixed, 85% times the knife's speed.
            assert_eq!(craft_time(&r, &around(&[], Some(1.0))), 40.0);
            assert_eq!(craft_time(&r, &around(&[], Some(0.5))), 23.0);
            let mut plain = recipe();
            plain.tool_behaviors.clear();
            assert_eq!(craft_time(&plain, &around(&[], Some(0.5))), 40.0);
        }

        #[test]
        fn modes_split_the_recipes_and_tabs_follow_dishes() {
            let mut food = recipe();
            food.category = CAT_FOOD.into();
            food.dish_category = "Burger".into();
            food.foodtypes = 1 | (1 << 4);
            let mut learned = recipe();
            learned.path = "/datum/crafting_recipe/food/secret".into();
            learned.crafting_flags = CRAFT_MUST_BE_LEARNED;
            let mut spear = recipe();
            spear.path = "/datum/crafting_recipe/spear".into();
            spear.cooking = false;
            spear.category = "Weapons Melee".into();
            let db = CraftingDb::from_data(CraftingData {
                recipes: vec![food, learned, spear],
                names: vec![("/obj/item/food/bun".into(), "bun".into())],
            });
            let cooking = &db.statics[1];
            assert_eq!(cooking.recipes.len(), 1);
            assert_eq!(cooking.tabs, ["Burger"]);
            assert_eq!(cooking.recipes[0].reqs[1], "2 × bun");
            assert_eq!(cooking.recipes[0].foodtypes, ["Мясное", "Зерновое"]);
            assert_eq!(db.statics[0].tabs, ["Weapons Melee"]);
            assert_eq!(db.visible(false).count(), 1);
        }
    }
}
