//! `RapidPipeDispenser` (`tgui/packages/tgui/interfaces/RapidPipeDispenser.tsx`, the RPD's
//! `ui_data` in `code/game/objects/items/rcd/RPD.dm`). The server half is
//! `ss13_game::atmos_machinery::rpd`; the color, layer and smart pipe parts are shared with
//! [`super::pipe_dispenser`].
//!
//! The recipe list and its previews never change, so they travel as static data; which preview
//! is selected follows from the data. A preview click is one `pipe_type` act carrying the
//! direction (tgui sends `pipe_type`, then `setdir`).
// shortcut: Disposals and Transit Tubes are shown disabled (their construction is not ported);
// the recipe title is a plain right-aligned row, not tgui's `StyleableSection`.

use serde::{Deserialize, Serialize};
use ss13_shared::IconStateId;

use super::pipe_dispenser::InitDirections;
use crate::UiInterface;

/// `get_preview` row.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RpdPreview {
    /// BYOND dir.
    pub dir: u8,
    pub dir_name: String,
    pub flipped: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RpdRecipe {
    /// 1-based.
    pub pipe_index: u32,
    pub pipe_name: String,
    /// Lines split at `<br>`; empty for none.
    pub pipe_desc: String,
    /// `pipe_item.dmi`'s state for the previews.
    pub icon: Option<IconStateId>,
    pub previews: Vec<RpdPreview>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RpdCategory {
    pub cat_name: String,
    pub recipes: Vec<RpdRecipe>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RpdStatic {
    pub paint_colors: Vec<(String, String)>,
    pub max_pipe_layers: u8,
    pub categories: Vec<RpdCategory>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RpdData {
    /// 0 atmos, 1 disposals, 2 transit tubes.
    pub category: u8,
    /// `BUILD_MODE` 1, `WRENCH_MODE` 2, `DESTROY_MODE` 4, `REPROGRAM_MODE` 8.
    pub mode: u8,
    pub multi_layer: bool,
    /// `PIPE_LAYER(n)` = `1 << (n - 1)` per active layer.
    pub pipe_layers: u8,
    pub selected_category: String,
    /// 1-based index in `selected_category`.
    pub selected_recipe: u32,
    pub p_dir: u8,
    pub selected_color: String,
    pub init_directions: InitDirections,
}

pub struct RapidPipeDispenserUi;

impl UiInterface for RapidPipeDispenserUi {
    const NAME: &'static str = "RapidPipeDispenser";
    type Data = RpdData;
    type StaticData = RpdStatic;
}

/// `ROOT_CATEGORIES`.
pub const ROOT_CATEGORIES: [&str; 3] = ["Atmospherics", "Disposals", "Transit Tubes"];

/// `TOOLS`: name and mode bit.
pub const TOOLS: [(&str, u8); 4] = [
    ("Dispense", 1),
    ("Connect", 2),
    ("Destroy", 4),
    ("Reprogram", 8),
];

/// BYOND `dir2text`.
pub fn dir_text(dir: u8) -> &'static str {
    match dir {
        1 => "north",
        2 => "south",
        4 => "east",
        8 => "west",
        5 => "northeast",
        9 => "northwest",
        6 => "southeast",
        10 => "southwest",
        _ => "",
    }
}

/// BYOND `text2dir`.
pub fn text_dir(text: &str) -> Option<u8> {
    [1, 2, 4, 8, 5, 9, 6, 10]
        .into_iter()
        .find(|&d| dir_text(d) == text)
}

/// `preview.selected`: the chosen recipe, in the chosen direction (one-direction pipes: any).
pub fn preview_selected(data: &RpdData, category: &str, recipe: &RpdRecipe, dir: u8) -> bool {
    data.selected_category == category
        && data.selected_recipe == recipe.pipe_index
        && (recipe.previews.len() == 1 || dir == data.p_dir)
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::{EntityScene, SceneList};
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, ImageButton};

    use super::super::pipe_dispenser::client_side::{category_tabs, color_item, smart_pipe_block};
    use super::super::pipe_dispenser::{PipeCategory, PipeRecipeEntry};
    use super::{
        ROOT_CATEGORIES, RapidPipeDispenserUi, RpdData, RpdRecipe, RpdStatic, TOOLS, dir_text,
        preview_selected,
    };
    use crate::client::{ClientUi, UiLocalState, UiSprite};

    fn params(pairs: Vec<(&str, String)>) -> ActParams {
        ActParams(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    /// `CategoryItem`.
    fn category_item(selected: u8) -> impl Scene + use<> {
        let buttons: Vec<_> = ROOT_CATEGORIES
            .iter()
            .enumerate()
            .map(|(i, &name)| {
                let p = params(vec![("category", i.to_string())]);
                let icon = super::super::pipe_dispenser::category_icon(name);
                // Only atmos piping is ported.
                let disabled = i != 0;
                let color = if i as u8 == selected {
                    UiColor::Default
                } else {
                    UiColor::Transparent
                };
                bsn! {
                    @ss13_ui_kit::Button { @content: name, @icon: icon }
                    ss13_ui_kit::Button { color: color, disabled: disabled }
                    Act("category")
                    p
                }
            })
            .collect();
        bsn! {
            @LabeledListItem { @label: "Category" }
            Children [ Node { flex_wrap: FlexWrap::Wrap } Children [ {buttons} ] ]
        }
    }

    /// `Button.Checkbox` sending `act` with `p`.
    fn checkbox(
        text: String,
        checked: bool,
        act: &'static str,
        p: ActParams,
        tooltip: &'static str,
    ) -> impl Scene + use<> {
        let icon = if checked {
            "check-square-o"
        } else {
            "square-o"
        };
        bsn! {
            @ss13_ui_kit::Button { @content: text, @icon: icon, @tooltip: tooltip }
            ss13_ui_kit::Button { selected: checked }
            Act(act)
            p
        }
    }

    /// `ModeItem`.
    fn mode_item(mode: u8) -> impl Scene + use<> {
        let boxes: Vec<_> = TOOLS
            .iter()
            .map(|(name, bit)| {
                let p = params(vec![("mode", bit.to_string())]);
                checkbox((*name).to_owned(), mode & bit != 0, "mode", p, "")
            })
            .collect();
        bsn! {
            @LabeledListItem { @label: "Modes" }
            Children [ Node { flex_wrap: FlexWrap::Wrap } Children [ {boxes} ] ]
        }
    }

    /// `LayerSelect` (the RPD's: a bitmask, and `Multi`).
    fn layer_select(data: &RpdData, max: u8) -> impl Scene + use<> {
        let mut boxes: Vec<Box<dyn SceneList>> = (0..max)
            .map(|layer| {
                let bit = 1u8 << layer;
                let checked = if data.multi_layer {
                    data.pipe_layers & bit != 0
                } else {
                    data.pipe_layers == bit
                };
                let p = params(vec![("pipe_layers", bit.to_string())]);
                let b = checkbox((layer + 1).to_string(), checked, "pipe_layers", p, "");
                Box::new(EntityScene(b)) as Box<dyn SceneList>
            })
            .collect();
        boxes.push(Box::new(EntityScene(checkbox(
            "Multi".to_owned(),
            data.multi_layer,
            "toggle_multi_layer",
            ActParams::default(),
            "Build on multiple pipe layers simultaneously",
        ))));
        bsn! {
            @LabeledListItem { @label: "Layer" }
            Children [ Node { flex_wrap: FlexWrap::Wrap } Children [ {boxes} ] ]
        }
    }

    /// `RecipeRow`.
    fn recipe_row(data: &RpdData, category: &str, recipe: &RpdRecipe) -> impl Scene + use<> {
        let tiles: Vec<_> = recipe
            .previews
            .iter()
            .map(|preview| {
                let p = params(vec![
                    ("pipe_type", recipe.pipe_index.to_string()),
                    ("category", category.to_owned()),
                    ("dir", dir_text(preview.dir).to_owned()),
                    ("flipped", u8::from(preview.flipped).to_string()),
                ]);
                let dir = ss13_shared::Dir::from_bits(preview.dir);
                let icon = recipe.icon;
                let image: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                    Node { width: percent(100), height: percent(100) }
                    UiSprite { icon_state: icon, color: None, dir: dir }
                }));
                let selected = preview_selected(data, category, recipe, preview.dir);
                let tip = preview.dir_name.clone();
                bsn! {
                    @ImageButton { @image: image, @image_size: 58.0, @tooltip: tip }
                    ImageButton { color: UiColor::Blue, selected: selected }
                    Act("pipe_type")
                    p
                }
            })
            .collect();
        let name = recipe.pipe_name.clone();
        let info: Option<EntityScene<_>> = (!recipe.pipe_desc.is_empty()).then(|| {
            let tip = recipe.pipe_desc.replace("<br>", "\n");
            EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "info-circle", @tooltip: tip }
                ss13_ui_kit::Button { color: UiColor::Transparent }
            })
        });
        bsn! {
            Node { flex_direction: FlexDirection::Column, margin: UiRect::bottom(px(6)) }
            Children [
                Node {
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::Center,
                    border: UiRect::bottom(px(1)),
                }
                Children [
                    {info}
                    --
                    @UiBox { @color: UiColor::Label, @inline: true } Children [ Text(name) ]
                ]
                --
                Node { flex_wrap: FlexWrap::Wrap } Children [ {tiles} ]
            ]
        }
    }

    /// `PipeTypeSection`.
    fn pipe_type_section(
        st: &RpdStatic,
        data: &RpdData,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        // The tabs are the dispenser's (names only).
        let as_tabs: Vec<PipeCategory> = st
            .categories
            .iter()
            .map(|c| PipeCategory {
                cat_name: c.cat_name.clone(),
                recipes: Vec::<PipeRecipeEntry>::new(),
            })
            .collect();
        let picked = local.get(super::super::pipe_dispenser::client_side::LOCAL_CATEGORY);
        let shown = st
            .categories
            .iter()
            .find(|c| c.cat_name == picked)
            .or_else(|| {
                st.categories
                    .iter()
                    .find(|c| c.cat_name == data.selected_category)
            })
            .or_else(|| st.categories.first());
        let shown_name = shown.map_or(String::new(), |c| c.cat_name.clone());
        let tabs = category_tabs(&as_tabs, &shown_name);
        let rows: Vec<_> = shown
            .map(|c| c.recipes.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|r| recipe_row(data, &shown_name, r))
            .collect();
        bsn! {
            @Stack { @fill: true, @vertical: true }
            Children [
                @StackItem Children [ {EntityScene(tabs)} ]
                --
                @StackItem { @grow: 1.0 }
                Children [
                    @Section { @fill: true, @scrollable: true }
                    Children [ {rows} ]
                ]
            ]
        }
    }

    fn view(st: &RpdStatic, data: &RpdData, local: &UiLocalState) -> impl Scene + use<> {
        let atmos = data.category == 0;
        let category = category_item(data.category);
        let modes = mode_item(data.mode);
        let colors = atmos.then(|| EntityScene(color_item(&st.paint_colors, &data.selected_color)));
        let layers = atmos.then(|| EntityScene(layer_select(data, st.max_pipe_layers)));
        let block = atmos.then(|| {
            let block = smart_pipe_block(data.init_directions);
            EntityScene(bsn! {
                @StackItem Node { width: px(90) } Children [ {EntityScene(block)} ]
            })
        });
        let types = pipe_type_section(st, data, local);
        bsn! {
            @WindowContent
            Children [
                @Stack { @fill: true, @vertical: true }
                Children [
                    @StackItem
                    Children [
                        @Stack { @fill: true }
                        Children [
                            @StackItem { @grow: 1.0 }
                            Children [
                                @Section { @fill: true }
                                Children [
                                    @LabeledList
                                    Children [
                                        {EntityScene(category)}
                                        --
                                        {EntityScene(modes)}
                                        --
                                        {colors}
                                        --
                                        {layers}
                                    ]
                                ]
                            ]
                            --
                            {block}
                        ]
                    ]
                    --
                    @StackItem { @grow: 1.0 }
                    Children [ {EntityScene(types)} ]
                ]
            ]
        }
    }

    impl ClientUi for RapidPipeDispenserUi {
        const WIDTH: f32 = 550.0;
        const HEIGHT: f32 = 580.0;

        fn content(st: &RpdStatic, data: &RpdData) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(st: &RpdStatic, data: &RpdData, local: &UiLocalState) -> impl Scene {
            view(st, data, local)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_round_trip_and_selection_follows_the_dir() {
        for d in [1, 2, 4, 8, 5, 6, 9, 10] {
            assert_eq!(text_dir(dir_text(d)), Some(d));
        }
        assert_eq!(text_dir("up"), None);
        let recipe = |n: usize| RpdRecipe {
            pipe_index: 2,
            previews: (0..n)
                .map(|i| RpdPreview {
                    dir: [1, 4][i],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let data = RpdData {
            selected_category: "Binary".into(),
            selected_recipe: 2,
            p_dir: 4,
            ..Default::default()
        };
        assert!(preview_selected(&data, "Binary", &recipe(2), 4));
        assert!(!preview_selected(&data, "Binary", &recipe(2), 1));
        // One direction only: selected whatever the dir.
        assert!(preview_selected(&data, "Binary", &recipe(1), 1));
        assert!(!preview_selected(&data, "Pipes", &recipe(2), 4));
    }
}
