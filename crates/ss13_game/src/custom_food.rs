//! Custom food (server), after tg `/datum/component/ingredients_holder`
//! (`code/datums/components/ingredients_holder.dm`) on the bases tg gives it in `Initialize`:
//! bread, buns, pizza dough, pie and cake bases and their slices, the seaweed sheet, onigiri,
//! taco shells, boiled spaghetti, a few rice and noodle dishes and the bowl.
//!
//! - Any food (not another holder) used on a base goes into it, up to its
//!   `max_ingredients` ("too full!"): the base takes the ingredient's reagents (its volume grows
//!   by the ingredient's), its weight class if bigger, the name "<size> <first ingredient>
//!   <base>" and a filling overlay (`<icon_state>_filling`) by the base's fill type (one mixed
//!   fill, scattered, stacked, stacked with a top, or lines).
//! - A base with a `replacement` (a bread loaf, a bowl, a bun, ...) becomes that type first:
//!   it is spawned where the base was, takes the base's reagents and the holder, and the base
//!   is gone.
//! - Sliced, the slices keep the ingredients, name and fillings (`on_processed`).
//! - Examined: what it contains and its size.
// shortcut: the filling colour is the ingredient's `color`, else the mix of its reagents' colours
// (tg averages the ingredient's icon pixels, which the server does not load), until icon colours
// are converted; no custom materials, no grenades as odd ingredients, no meat `subjectname`, no
// pizza slices taken from a box keeping them, an ingredient leaving the holder keeps its name.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmFood, DmItem, DmName, DmWeight, IconIndex};
use ss13_reagents::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay};

use crate::chem::{Chem, Reagents, name_of};
use crate::combat::CombatRng;
use crate::storage::put_inside;

/// tg `MAX_ATOM_OVERLAYS - 3`, the default `max_ingredients`.
const DEFAULT_MAX: usize = 97;

/// tg `CUSTOM_INGREDIENT_ICON_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    NoChange,
    Fill,
    Scatter,
    Stack,
    Line,
    StackPlusTop,
}

/// A base: the type that adds the holder (with subtypes), its `replacement`, fill type and
/// `max_ingredients`. Most specific first.
struct HolderDef {
    root: &'static str,
    replacement: Option<&'static str>,
    fill: Fill,
    max: usize,
}

const fn def(
    root: &'static str,
    replacement: Option<&'static str>,
    fill: Fill,
    max: usize,
) -> HolderDef {
    HolderDef {
        root,
        replacement,
        fill,
        max,
    }
}

const HOLDERS: [HolderDef; 22] = [
    def(
        "/obj/item/food/seaweedsheet",
        Some("/obj/item/food/sushi/empty"),
        Fill::Fill,
        6,
    ),
    def(
        "/obj/item/food/onigiri",
        Some("/obj/item/food/onigiri/empty"),
        Fill::NoChange,
        4,
    ),
    def(
        "/obj/item/food/spaghetti/boiledspaghetti",
        None,
        Fill::Scatter,
        6,
    ),
    def(
        "/obj/item/food/salad/egg_fried_rice",
        None,
        Fill::Stack,
        DEFAULT_MAX,
    ),
    def(
        "/obj/item/food/salad/bibimbap",
        None,
        Fill::Stack,
        DEFAULT_MAX,
    ),
    def(
        "/obj/item/food/salad/simple_fried_noodles",
        None,
        Fill::Stack,
        DEFAULT_MAX,
    ),
    def("/obj/item/food/okonomiyaki", None, Fill::Stack, DEFAULT_MAX),
    def("/obj/item/food/pizzaslice/margherita", None, Fill::Fill, 12),
    def(
        "/obj/item/food/hard_taco_shell",
        Some("/obj/item/food/hard_taco_shell/empty"),
        Fill::Fill,
        6,
    ),
    // `cake_holder`.
    def(
        "/obj/item/food/cake/plain/vegan",
        Some("/obj/item/food/cake/empty/vegan"),
        Fill::Fill,
        16,
    ),
    def(
        "/obj/item/food/cake/plain",
        Some("/obj/item/food/cake/empty"),
        Fill::Fill,
        16,
    ),
    def("/obj/item/food/cakeslice/empty", None, Fill::Fill, 16),
    def(
        "/obj/item/food/bread/plain",
        Some("/obj/item/food/bread/empty"),
        Fill::Fill,
        8,
    ),
    def(
        "/obj/item/food/breadslice/plain",
        None,
        Fill::Stack,
        DEFAULT_MAX,
    ),
    def("/obj/item/food/breadslice/empty", None, Fill::Fill, 8),
    def(
        "/obj/item/food/bread/root",
        Some("/obj/item/food/bread/empty"),
        Fill::Fill,
        8,
    ),
    def(
        "/obj/item/food/breadslice/root",
        None,
        Fill::Stack,
        DEFAULT_MAX,
    ),
    def(
        "/obj/item/reagent_containers/cup/bowl",
        Some("/obj/item/food/salad/empty"),
        Fill::Fill,
        6,
    ),
    def(
        "/obj/item/food/pizzabread",
        Some("/obj/item/food/pizza/custom"),
        Fill::Scatter,
        12,
    ),
    def(
        "/obj/item/food/bun",
        Some("/obj/item/food/burger/empty"),
        Fill::StackPlusTop,
        DEFAULT_MAX,
    ),
    def(
        "/obj/item/food/pie/plain",
        Some("/obj/item/food/pie/empty"),
        Fill::Fill,
        8,
    ),
    def("/obj/item/food/pieslice/empty", None, Fill::Fill, 8),
];

/// The replacement types the game spawns (`ss13_mapload::runtime`).
pub const REPLACEMENTS: [&str; 10] = [
    "/obj/item/food/sushi/empty",
    "/obj/item/food/onigiri/empty",
    "/obj/item/food/hard_taco_shell/empty",
    "/obj/item/food/cake/empty/vegan",
    "/obj/item/food/cake/empty",
    "/obj/item/food/bread/empty",
    "/obj/item/food/salad/empty",
    "/obj/item/food/pizza/custom",
    "/obj/item/food/burger/empty",
    "/obj/item/food/pie/empty",
];

/// tg `/datum/component/ingredients_holder`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct CustomFood {
    pub replacement: Option<&'static str>,
    pub fill: Fill,
    pub max: usize,
    /// `ingredient_names`.
    pub names: Vec<String>,
    /// `filling_colors`.
    pub colors: Vec<[u8; 4]>,
    pub custom_name: Option<String>,
    /// `top_overlay`.
    top: Option<Overlay>,
}

impl CustomFood {
    fn new(replacement: Option<&'static str>, fill: Fill, max: usize) -> Self {
        Self {
            replacement,
            fill,
            max,
            names: Vec::new(),
            colors: Vec::new(),
            custom_name: None,
            top: None,
        }
    }
}

pub struct CustomFoodPlugin;

impl Plugin for CustomFoodPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // After `food::item_interaction` (dunking, eating, tool processing come first).
        app.add_click_handler(ClickPhase::HeldOnTarget, 3309, |w, c| {
            !c.modifiers.right
                && c.held
                    .is_some_and(|h| customizable_attack(w, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::OtherHand, 459, |w, c| {
            c.held
                .is_some_and(|h| customizable_attack(w, c.mob, h, c.target))
        });
        app.add_observer(attach);
    }
}

fn holder_def(path: &str) -> Option<&'static HolderDef> {
    HOLDERS.iter().find(|d| is_under(path, d.root))
}

/// The bases' `Initialize`: `AddComponent(/datum/component/ingredients_holder, ...)`.
fn attach(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let Some(d) = holder_def(&item.path) else {
        return;
    };
    commands
        .entity(add.entity)
        .try_insert(CustomFood::new(d.replacement, d.fill, d.max));
}

/// tg `custom_adjective`.
pub fn custom_adjective(count: usize) -> &'static str {
    match count {
        0..=2 => "small",
        3..=5 => "standard",
        6..=8 => "big",
        9..=11 => "ridiculous",
        _ => "monstrous",
    }
}

fn base_name(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map_or_else(|| name_of(world, e), |n| n.name.clone())
}

/// `valid_ingredient` for `CUSTOM_INGREDIENT_TYPE_EDIBLE`: edible and not itself a holder.
fn valid_ingredient(world: &World, ingredient: Entity) -> bool {
    world.get::<CustomFood>(ingredient).is_none() && world.get::<DmFood>(ingredient).is_some()
}

/// `customizable_attack`. True when the click was taken.
fn customizable_attack(world: &mut World, mob: Entity, ingredient: Entity, holder: Entity) -> bool {
    let Some(state) = world.get::<CustomFood>(holder) else {
        return false;
    };
    let full = state.names.len() >= state.max;
    // An invalid ingredient (a flour sack, a knife) goes on down the chain (pouring, hitting).
    if !valid_ingredient(world, ingredient) {
        return false;
    }
    if full {
        crate::balloon::balloon_alert(world, mob, holder, "too full!");
        return true;
    }
    if crate::inventory::no_drop(world, ingredient) {
        return true;
    }
    add_ingredient(world, holder, ingredient);
    true
}

/// `add_ingredient`.
pub fn add_ingredient(world: &mut World, mut holder: Entity, ingredient: Entity) {
    let Some(state) = world.get::<CustomFood>(holder).cloned() else {
        return;
    };
    if let Some(replacement) = state.replacement {
        let Some(pos) = crate::explosions::turf_of(world, holder) else {
            return;
        };
        let Some(made) = crate::construction::spawn_obj(world, replacement, pos) else {
            return;
        };
        // Its own `Initialize` (food reagents, a holder of its own) before the base's holder.
        world.flush();
        world.entity_mut(made).insert(CustomFood {
            replacement: None,
            ..state
        });
        handle_reagents(world, made, holder);
        crate::food_storage::release(world, holder);
        world.despawn(holder);
        holder = made;
    }
    handle_reagents(world, holder, ingredient);
    let ingredient_name = name_of(world, ingredient);
    let ingredient_weight = world.get::<DmWeight>(ingredient).map(|w| w.w_class);
    if let (Some(w), Some(mut own)) = (ingredient_weight, world.get_mut::<DmWeight>(holder))
        && w > own.w_class
    {
        own.w_class = w;
    }
    let (count, custom) = {
        let mut state = world.get_mut::<CustomFood>(holder).expect("checked");
        state.names.push(ingredient_name.clone());
        let custom = state
            .custom_name
            .get_or_insert_with(|| ingredient_name.clone())
            .clone();
        (state.names.len(), custom)
    };
    let name = format!(
        "{} {custom} {}",
        custom_adjective(count),
        base_name(world, holder)
    );
    world.entity_mut(holder).insert(Name::new(name));
    let color = fill_color(world, ingredient);
    if let Some(mut state) = world.get_mut::<CustomFood>(holder) {
        state.colors.push(color);
    }
    apply_fill(world, holder, color);
    put_inside(world, ingredient, holder);
}

/// `handle_reagents`: the holder's volume grows by the ingredient's, which gives it everything.
fn handle_reagents(world: &mut World, holder: Entity, ingredient: Entity) {
    let Some(volume) = world
        .get::<Reagents>(ingredient)
        .map(|r| r.0.maximum_volume)
    else {
        return;
    };
    let Some(total) = world.get::<Reagents>(ingredient).map(|r| r.0.total_volume) else {
        return;
    };
    {
        let Some(mut own) = world.get_mut::<Reagents>(holder) else {
            return;
        };
        own.0.maximum_volume += volume;
    }
    crate::chem::transfer(world, ingredient, holder, total);
}

/// The filling colour of `ingredient` (see the module shortcut).
fn fill_color(world: &World, ingredient: Entity) -> [u8; 4] {
    if let Some(color) = world.get::<DmAtom>(ingredient).and_then(|a| a.color) {
        return [color[0], color[1], color[2], 255];
    }
    let mixed = match (
        world.get_resource::<Chem>(),
        world.get::<Reagents>(ingredient),
    ) {
        (Some(chem), Some(r)) => ss13_reagents::mix_color(&chem.0, &r.0.reagents),
        _ => None,
    };
    mixed.map_or([255, 255, 255, 255], |c| [c[0], c[1], c[2], 255])
}

/// `mix_color`: the average with the old top fill.
fn mix(a: [u8; 4], b: [u8; 4]) -> [u8; 4] {
    std::array::from_fn(|i| ((u16::from(a[i]) + u16::from(b[i])) / 2) as u8)
}

fn rand_offset(world: &mut World, low: i16, high: i16) -> i16 {
    let span = (high - low) as u32;
    low + world.resource_mut::<CombatRng>().range(0, span) as i16
}

/// `apply_fill`: the filling overlay for this many ingredients.
fn apply_fill(world: &mut World, holder: Entity, color: [u8; 4]) {
    let Some(state) = world.get::<CustomFood>(holder).cloned() else {
        return;
    };
    if state.fill == Fill::NoChange {
        return;
    }
    let Some(atom) = world.get::<DmAtom>(holder) else {
        return;
    };
    let (icon, base_state) = (atom.icon.clone(), atom.icon_state.clone());
    let id = |world: &World, s: &str| world.get_resource::<IconIndex>()?.get(&icon, s);
    let Some(filling_id) = id(world, &format!("{base_state}_filling")) else {
        return;
    };
    let n = state.names.len() as i16;
    let mut filling = Overlay {
        color: Some(color),
        ..Overlay::new(filling_id)
    };
    let mut remove: Option<Overlay> = None;
    let mut top: Option<Overlay> = state.top.clone();
    let mut extra: Option<Overlay> = None;
    match state.fill {
        Fill::NoChange => {}
        Fill::Scatter => {
            filling.pixel_offset = [rand_offset(world, -1, 1), rand_offset(world, -1, 1)];
        }
        Fill::Stack => {
            filling.pixel_offset = [rand_offset(world, -1, 1), 2 * n - 1];
        }
        Fill::StackPlusTop => {
            filling.pixel_offset = [rand_offset(world, -1, 1), 2 * n - 1];
            remove = state.top.clone();
            let new_top = id(world, &format!("{base_state}_top")).map(|t| Overlay {
                pixel_offset: [0, 2 * n + 3],
                ..Overlay::new(t)
            });
            top = new_top.clone();
            extra = new_top;
        }
        Fill::Fill => {
            if let Some(old) = &state.top {
                if state.colors.len() != 1
                    && let Some(old_color) = old.color
                {
                    filling.color = Some(mix(color, old_color));
                }
                remove = Some(old.clone());
            }
            top = Some(filling.clone());
        }
        Fill::Line => {
            let o = rand_offset(world, -8, 3);
            filling.pixel_offset = [o, o];
        }
    }
    if let Some(mut look) = world.get_mut::<Appearance>(holder) {
        if let Some(old) = &remove
            && let Some(i) = look.overlays.iter().position(|o| o == old)
        {
            look.overlays.remove(i);
        }
        look.overlays.push(filling);
        look.overlays.extend(extra);
    }
    if let Some(mut state) = world.get_mut::<CustomFood>(holder) {
        state.top = top;
    }
}

/// `on_processed`: each result gets a holder with the base's fill type, ingredients and fillings.
pub fn on_processed(world: &mut World, source: Entity, results: &[Entity]) {
    let Some(state) = world.get::<CustomFood>(source).cloned() else {
        return;
    };
    for &result in results {
        if world.get_entity(result).is_err() {
            continue;
        }
        let mut made = CustomFood::new(None, state.fill, state.max);
        if state.names.is_empty() {
            world.entity_mut(result).insert(made);
            continue;
        }
        made.names = state.names.clone();
        made.custom_name = state.custom_name.clone();
        let name = format!(
            "{} {} {}",
            custom_adjective(made.names.len()),
            made.custom_name.clone().unwrap_or_default(),
            name_of(world, result)
        );
        world.entity_mut(result).insert((made, Name::new(name)));
        for &color in &state.colors {
            if let Some(mut s) = world.get_mut::<CustomFood>(result) {
                s.colors.push(color);
            }
            apply_fill(world, result, color);
        }
    }
}

/// `on_examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(state) = world.get::<CustomFood>(target) else {
        return;
    };
    let text = if state.names.is_empty() {
        "It does not contain any ingredients.".to_owned()
    } else {
        format!(
            "It contains {} making a {}-sized {}.",
            crate::clothing::accessories::english_list(&state.names),
            custom_adjective(state.names.len()),
            base_name(world, target)
        )
    };
    lines.push((Span::Plain, text));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_adjectives_follow_tg_ranges() {
        assert_eq!(custom_adjective(0), "small");
        assert_eq!(custom_adjective(2), "small");
        assert_eq!(custom_adjective(3), "standard");
        assert_eq!(custom_adjective(8), "big");
        assert_eq!(custom_adjective(11), "ridiculous");
        assert_eq!(custom_adjective(12), "monstrous");
    }

    #[test]
    fn the_most_specific_base_wins() {
        let vegan = holder_def("/obj/item/food/cake/plain/vegan").unwrap();
        assert_eq!(vegan.replacement, Some("/obj/item/food/cake/empty/vegan"));
        let plain = holder_def("/obj/item/food/cake/plain/birthday").unwrap();
        assert_eq!(plain.replacement, Some("/obj/item/food/cake/empty"));
        let bowl = holder_def("/obj/item/reagent_containers/cup/bowl/soup").unwrap();
        assert_eq!(bowl.fill, Fill::Fill);
        assert_eq!(bowl.max, 6);
        assert!(holder_def("/obj/item/food/bread/empty").is_none());
        assert_eq!(
            holder_def("/obj/item/food/bun").unwrap().fill,
            Fill::StackPlusTop
        );
    }

    #[test]
    fn fill_mixes_with_the_previous_top() {
        assert_eq!(
            mix([200, 100, 0, 255], [100, 50, 50, 255]),
            [150, 75, 25, 255]
        );
    }

    #[test]
    fn ingredients_fill_a_base_and_carry_into_slices() {
        let mut world = World::new();
        world.init_resource::<CombatRng>();
        let holder = world
            .spawn((
                CustomFood::new(None, Fill::Stack, 2),
                DmName {
                    name: "bread slice".into(),
                    desc: String::new(),
                },
                ss13_shared::Appearance {
                    icon_state: ss13_shared::IconStateId(0),
                    layer: ss13_shared::Layer(3.0),
                    plane: -6,
                    color: None,
                    pixel_offset: [0, 0],
                    rotation: 0,
                    mouse_opacity: Default::default(),
                    glide: 0,
                    overlays: Vec::new(),
                },
            ))
            .id();
        let tomato = world
            .spawn(DmName {
                name: "tomato".into(),
                desc: String::new(),
            })
            .id();
        add_ingredient(&mut world, holder, tomato);
        let state = world.get::<CustomFood>(holder).unwrap();
        assert_eq!(state.names, ["tomato"]);
        assert_eq!(state.custom_name.as_deref(), Some("tomato"));
        assert_eq!(
            world.get::<Name>(holder).unwrap().as_str(),
            "small tomato bread slice"
        );
        assert_eq!(
            world.get::<crate::storage::StoredIn>(tomato).map(|s| s.0),
            Some(holder)
        );
        let slice = world
            .spawn(ss13_shared::Appearance {
                icon_state: ss13_shared::IconStateId(0),
                layer: ss13_shared::Layer(3.0),
                plane: -6,
                color: None,
                pixel_offset: [0, 0],
                rotation: 0,
                mouse_opacity: Default::default(),
                glide: 0,
                overlays: Vec::new(),
            })
            .id();
        on_processed(&mut world, holder, &[slice]);
        let carried = world.get::<CustomFood>(slice).unwrap();
        assert_eq!(carried.names, ["tomato"]);
        assert_eq!(carried.colors.len(), 1);
        assert_eq!(carried.max, 2);
    }
}
