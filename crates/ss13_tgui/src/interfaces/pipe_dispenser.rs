//! `PipeDispenser` (`tgui/packages/tgui/interfaces/PipeDispenser.tsx`, `pipedispenser/ui_data`
//! in `code/game/machinery/pipe/pipe_dispenser.dm`) and the parts `RapidPipeDispenser.tsx` shares
//! with it: the color row, the layer row and the smart pipe direction block. The server half
//! lives with the machine (`ss13_game::atmos_machinery::dispenser`).
//!
//! The recipe list never changes, so it travels as static data (tg resends it in `ui_data`).
// shortcut: the color row is buttons tinted with the paint color, not tgui's bordered
// `ColorBox`es; the selected one is named next to them as in tgui.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

/// One `/datum/pipe_info` of a category, as `ui_data` lists it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PipeRecipeEntry {
    pub pipe_name: String,
    /// 1-based, as tg's list index.
    pub pipe_index: u32,
    pub all_layers: bool,
    pub dir: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PipeCategory {
    pub cat_name: String,
    pub recipes: Vec<PipeRecipeEntry>,
}

/// `ui_static_data` (`paint_colors`: name and `#RRGGBB`, in `GLOB.pipe_paint_colors` order)
/// and the categories.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PipeDispenserStatic {
    pub paint_colors: Vec<(String, String)>,
    pub categories: Vec<PipeCategory>,
}

/// `init_directions`: the directions a dispensed smart pipe may connect in.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InitDirections {
    pub north: bool,
    pub south: bool,
    pub east: bool,
    pub west: bool,
}

impl InitDirections {
    /// From BYOND dir bits.
    pub fn from_bits(bits: u8) -> Self {
        Self {
            north: bits & 1 != 0,
            south: bits & 2 != 0,
            east: bits & 4 != 0,
            west: bits & 8 != 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PipeDispenserData {
    pub piping_layer: u8,
    pub selected_color: String,
    pub init_directions: InitDirections,
}

pub struct PipeDispenserUi;

impl UiInterface for PipeDispenserUi {
    const NAME: &'static str = "PipeDispenser";
    type Data = PipeDispenserData;
    type StaticData = PipeDispenserStatic;
}

/// `ICON_BY_CATEGORY_NAME` of `RapidPipeDispenser.tsx`.
pub fn category_icon(name: &str) -> &'static str {
    match name {
        "Atmospherics" => "wrench",
        "Binary" => "arrows-left-right",
        "Devices" => "microchip",
        "Disposal Pipes" | "Pipes" => "grip-lines",
        "Disposals" => "trash-alt",
        "Heat Exchange" => "thermometer-half",
        "Station Equipment" => "microchip",
        "Transit Tubes" => "bus",
        _ => "",
    }
}

/// `#RRGGBB` to bytes; black when malformed.
pub fn hex_rgb(hex: &str) -> [u8; 3] {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return [0; 3];
    }
    let byte = |i: usize| {
        h.get(i..i + 2)
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .unwrap_or(0)
    };
    [byte(0), byte(2), byte(4)]
}

/// lodash `capitalize` per word (`capitalizeAll`).
fn capitalize_all(text: &str) -> String {
    text.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(feature = "client")]
pub(crate) mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, TabsTab};

    use super::{
        InitDirections, PipeCategory, PipeDispenserData, PipeDispenserStatic, PipeDispenserUi,
        capitalize_all, category_icon, hex_rgb,
    };
    use crate::client::{ClientUi, SetLocal, UiLocalState};

    /// The local key of the shown recipe tab (`useState(categories[0].cat_name)`).
    pub(crate) const LOCAL_CATEGORY: &str = "category";

    fn params(pairs: &[(&str, String)]) -> ActParams {
        ActParams(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        )
    }

    /// `ColorItem`.
    pub(crate) fn color_item(colors: &[(String, String)], selected: &str) -> impl Scene + use<> {
        let boxes: Vec<_> = colors
            .iter()
            .map(|(name, hex)| {
                let color = UiColor::Css(hex_rgb(hex));
                let p = params(&[("paint_color", name.clone())]);
                let tip = capitalize_all(name);
                bsn! {
                    @ss13_ui_kit::Button { @tooltip: tip }
                    ss13_ui_kit::Button { color: color }
                    Node { width: px(20), min_height: px(20), height: px(20) }
                    Act("color")
                    p
                }
            })
            .collect();
        let shown = colors
            .iter()
            .find(|(n, _)| n == selected)
            .map_or(UiColor::Default, |(_, hex)| UiColor::Css(hex_rgb(hex)));
        let name = capitalize_all(selected);
        bsn! {
            @LabeledListItem { @label: "Color" }
            Children [
                Node { flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center }
                Children [
                    {boxes}
                    --
                    @UiBox { @color: shown, @bold: true, @inline: true }
                    Node { margin: UiRect::left(px(12)) }
                    Children [ Text(name) ]
                ]
            ]
        }
    }

    /// `LayerSelect` of `RapidPlumbingDevice.tsx`: one checkbox per layer.
    pub(crate) fn layer_select(layer: u8) -> impl Scene + use<> {
        let boxes: Vec<_> = (1u8..=5)
            .map(|l| {
                let p = params(&[("piping_layer", l.to_string())]);
                let content = l.to_string();
                let icon = if l == layer {
                    "check-square-o"
                } else {
                    "square-o"
                };
                bsn! {
                    @ss13_ui_kit::Button { @content: content, @icon: icon }
                    ss13_ui_kit::Button { selected: {l == layer} }
                    Act("piping_layer")
                    p
                }
            })
            .collect();
        bsn! {
            @LabeledListItem { @label: "Layer" }
            Children [ Node Children [ {boxes} ] ]
        }
    }

    fn dir_button(icon: &'static str, on: bool, flag: &'static str) -> impl Scene + use<> {
        let p = params(&[("dir_flag", flag.to_owned())]);
        bsn! {
            @ss13_ui_kit::Button { @icon: icon }
            ss13_ui_kit::Button { selected: on }
            Act("init_dir_setting")
            p
        }
    }

    /// `SmartPipeBlockSection`.
    pub(crate) fn smart_pipe_block(dirs: InitDirections) -> impl Scene + use<> {
        let north = dir_button("arrow-up", dirs.north, "north");
        let west = dir_button("arrow-left", dirs.west, "west");
        let east = dir_button("arrow-right", dirs.east, "east");
        let south = dir_button("arrow-down", dirs.south, "south");
        let tip = "This is a panel for blocking certain connection directions for the smart \
                   pipes. The button in the center resets to default (all directions can \
                   connect)";
        bsn! {
            @Section { @fill: true }
            Children [
                Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center }
                Children [
                    Node Children [
                        @ss13_ui_kit::Button { @icon: "info", @tooltip: tip }
                        ss13_ui_kit::Button { color: UiColor::Transparent }
                        --
                        {EntityScene(north)}
                    ]
                    --
                    Node Children [
                        {EntityScene(west)}
                        --
                        @ss13_ui_kit::Button { @icon: "circle" }
                        Act("init_reset")
                        --
                        {EntityScene(east)}
                    ]
                    --
                    Node Children [ {EntityScene(south)} ]
                ]
            ]
        }
    }

    /// The shown category: the local pick, else the first.
    pub(crate) fn shown_category<'a>(
        categories: &'a [PipeCategory],
        local: &UiLocalState,
    ) -> Option<&'a PipeCategory> {
        let picked = local.get(LOCAL_CATEGORY);
        categories
            .iter()
            .find(|c| c.cat_name == picked)
            .or_else(|| categories.first())
    }

    /// The category tabs (`Tabs` with `useState`).
    pub(crate) fn category_tabs(categories: &[PipeCategory], shown: &str) -> impl Scene + use<> {
        let tabs: Vec<_> = categories
            .iter()
            .map(|c| {
                let pick = SetLocal {
                    key: LOCAL_CATEGORY.into(),
                    value: c.cat_name.clone(),
                    rerender: true,
                };
                let (name, icon, selected) = (
                    c.cat_name.clone(),
                    category_icon(&c.cat_name),
                    c.cat_name == shown,
                );
                bsn! {
                    @TabsTab { @content: name, @icon: icon }
                    TabsTab { selected: selected }
                    pick
                }
            })
            .collect();
        bsn! { @Tabs Children [ {tabs} ] }
    }

    /// `PipeTypeSection`.
    fn pipe_type_section(st: &PipeDispenserStatic, local: &UiLocalState) -> impl Scene + use<> {
        let shown = shown_category(&st.categories, local);
        let shown_name = shown.map_or(String::new(), |c| c.cat_name.clone());
        let tabs = category_tabs(&st.categories, &shown_name);
        let buttons: Vec<_> = shown
            .map(|c| c.recipes.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|r| {
                let p = params(&[
                    ("pipe_type", r.pipe_index.to_string()),
                    ("pipe_dir", r.dir.to_string()),
                    ("category", shown_name.clone()),
                ]);
                let name = r.pipe_name.clone();
                let tip = r.pipe_name.clone();
                bsn! {
                    @ss13_ui_kit::Button {
                        @content: name,
                        @fluid: true,
                        @ellipsis: true,
                        @tooltip: tip,
                    }
                    Act("pipe_type")
                    p
                }
            })
            .collect();
        bsn! {
            @Section { @fill: true, @scrollable: true }
            Children [
                {EntityScene(tabs)}
                --
                Node { flex_direction: FlexDirection::Column }
                Children [ {buttons} ]
            ]
        }
    }

    fn view(
        st: &PipeDispenserStatic,
        data: &PipeDispenserData,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        let color = color_item(&st.paint_colors, &data.selected_color);
        let layer = layer_select(data.piping_layer);
        let block = smart_pipe_block(data.init_directions);
        let types = pipe_type_section(st, local);
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
                                @Section
                                Children [
                                    @LabeledList
                                    Children [
                                        {EntityScene(color)}
                                        --
                                        {EntityScene(layer)}
                                    ]
                                ]
                            ]
                            --
                            @StackItem
                            Node { width: px(90) }
                            Children [ {EntityScene(block)} ]
                        ]
                    ]
                    --
                    @StackItem { @grow: 1.0 }
                    Children [ {EntityScene(types)} ]
                ]
            ]
        }
    }

    impl ClientUi for PipeDispenserUi {
        const WIDTH: f32 = 530.0;
        const HEIGHT: f32 = 530.0;

        fn content(st: &PipeDispenserStatic, data: &PipeDispenserData) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &PipeDispenserStatic,
            data: &PipeDispenserData,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_and_names_parse() {
        assert_eq!(hex_rgb("#FFA62B"), [0xFF, 0xA6, 0x2B]);
        assert_eq!(hex_rgb("bad"), [0, 0, 0]);
        assert_eq!(capitalize_all("omni"), "Omni");
        assert_eq!(
            InitDirections::from_bits(1 | 4),
            InitDirections {
                north: true,
                east: true,
                ..Default::default()
            }
        );
    }
}
