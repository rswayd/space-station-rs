//! `ChemMaster` (ChemMaster and CondiMaster): `tgui/packages/tgui/interfaces/ChemMaster.tsx` and
//! `/obj/machinery/chem_master/ui_data` (`code/modules/reagents/chemistry/machinery/
//! chem_master.dm`). The server half lives with the game (`ss13_game::chem_master`).
// shortcut: no `AnimatedNumber` tweening of volumes; the container picture buttons have no
// spinner fallback.

use serde::{Deserialize, Serialize};

use crate::UiInterface;
use crate::interfaces::vending::ProductIcon;

pub struct ChemMasterUi;

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ChemMasterStatic {
    /// `MAX_CONTAINER_PRINT_AMOUNT`.
    pub max_printable: u32,
    /// `PILL_MAX_PRINTABLE_LAYERS`.
    pub max_pill_duration: u32,
    pub categories: Vec<ContainerCategory>,
}

/// One `categories` entry (`tubes`, `pills`, `patches`, `condiments`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ContainerCategory {
    pub name: String,
    pub containers: Vec<PrintableContainer>,
}

/// A printable container type: `ref` is its type path, what `selectContainer` sends back.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PrintableContainer {
    pub r#ref: String,
    pub name: String,
    pub icon: Option<ProductIcon>,
    pub volume: f32,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ChemMasterData {
    pub is_printing: bool,
    pub printing_progress: u32,
    pub printing_total: u32,
    pub selected_pill_duration: f32,
    pub has_beaker_in_hand: bool,
    pub custom_transfer_amount: f32,
    pub beaker: Option<AnalyzableBeaker>,
    pub buffer: AnalyzableBeaker,
    pub is_transfering: bool,
    pub suggested_container_ref: String,
    pub selected_container_ref: String,
    pub selected_container_volume: f32,
    pub selected_container_category: Option<String>,
}

/// `beaker` and `buffer`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AnalyzableBeaker {
    pub max_volume: f32,
    pub current_volume: f32,
    pub contents: Vec<AnalyzableReagent>,
}

/// One reagent: `ref` is its type path, what `transfer` sends back.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AnalyzableReagent {
    pub r#ref: String,
    pub name: String,
    pub volume: f32,
    pub ph: f32,
    pub color: String,
    pub description: String,
    pub purity: f32,
    pub meta_rate: f32,
    pub overdose: f32,
    pub addiction_types: Vec<String>,
}

impl UiInterface for ChemMasterUi {
    const NAME: &'static str = "ChemMaster";
    type Data = ChemMasterData;
    type StaticData = ChemMasterStatic;
}

/// `ReagentEntry` transfer amounts: 1, 5, 10, the custom one, "All" (1000) and "..." (-1, the
/// server asks).
pub const ALL_AMOUNT: f32 = 1000.0;
pub const ASK_AMOUNT: f32 = -1.0;

/// The `u. each` text: `round(min(volume, buffer / count) * 100) / 100`.
pub fn volume_each(selected_volume: f32, buffer_volume: f32, count: u32) -> f32 {
    (selected_volume.min(buffer_volume / count.max(1) as f32) * 100.0).round() / 100.0
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{
        ALL_AMOUNT, ASK_AMOUNT, AnalyzableReagent, ChemMasterData, ChemMasterStatic, ChemMasterUi,
        PrintableContainer, volume_each,
    };
    use crate::client::{ActWithLocal, ClientUi, LocalOnNumber, SetLocal, UiLocalState, UiSprite};
    use crate::interfaces::chem_dispenser::js_num;
    use crate::interfaces::job_selection::parse_hex;

    /// Local keys: the analysed reagent (`"beaker|ref"`/`"buffer|ref"`), the print count and
    /// the "Suggest" checkbox.
    const ANALYZE: &str = "analyze";
    const COUNT: &str = "itemCount";
    const SUGGEST: &str = "suggest";

    impl ClientUi for ChemMasterUi {
        const WIDTH: f32 = 450.0;
        const HEIGHT: f32 = 620.0;

        fn content(st: &ChemMasterStatic, data: &ChemMasterData) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &ChemMasterStatic,
            data: &ChemMasterData,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }

    fn view(
        st: &ChemMasterStatic,
        data: &ChemMasterData,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        let analyzed = local.get(ANALYZE).split_once('|').and_then(|(list, r)| {
            let contents = match list {
                "beaker" => &data.beaker.as_ref()?.contents,
                _ => &data.buffer.contents,
            };
            contents.iter().find(|c| c.r#ref == r).cloned()
        });
        let body: Box<dyn Scene> = match analyzed {
            Some(reagent) => Box::new(analysis(&reagent)),
            None => Box::new(main(st, data, local)),
        };
        bsn! {
            @WindowContent { @scrollable: true }
            Children [ {EntityScene(body)} ]
        }
    }

    fn main(
        st: &ChemMasterStatic,
        data: &ChemMasterData,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        let beaker_buttons: Box<dyn SceneList> = match &data.beaker {
            Some(b) => {
                let volume = format!(
                    "{} / {} units",
                    js_num(b.current_volume),
                    js_num(b.max_volume)
                );
                let input = NumberInput {
                    value: f64::from(data.custom_transfer_amount),
                    min_value: 0.0,
                    max_value: f64::from(b.max_volume),
                    step: 5.0,
                    step_pixel_size: 3.0,
                    unit: "u".into(),
                    ..Default::default()
                };
                let on_change = crate::client::ActOnChange {
                    action: "setCustomTransfer".into(),
                    param: "target".into(),
                };
                Box::new(EntityScene(bsn! {
                    Node { align_items: AlignItems::Center }
                    Children [
                        Node { margin: UiRect::right(px(12)) }
                        Children [ Text(volume) TextColor({UiColor::Label.base()}) ]
                        --
                        Node { width: px(55), margin: UiRect::right(px(12)) }
                        input on_change
                        --
                        @Button { @icon: "eject", @content: "Eject" }
                        Act("eject")
                    ]
                }))
            }
            None => {
                let tooltip = if data.has_beaker_in_hand {
                    ""
                } else {
                    "You need to hold a container in your hand"
                };
                Box::new(EntityScene(bsn! {
                    @Button { @icon: "eject", @content: "Insert", @tooltip: tooltip }
                    Act("insert")
                }))
            }
        };
        let beaker_body: Box<dyn Scene> = match &data.beaker {
            None => Box::new(label_line("No beaker loaded.")),
            Some(b) if b.current_volume == 0.0 => Box::new(label_line("Beaker is empty.")),
            Some(b) => Box::new(reagent_table(data, &b.contents, "beaker")),
        };
        let buffer_volume = format!(
            "{} / {} units",
            js_num(data.buffer.current_volume),
            js_num(data.buffer.max_volume)
        );
        let (mode_color, mode_icon, mode_text) = if data.is_transfering {
            (UiColor::Good, "exchange-alt", "Moving reagents")
        } else {
            (UiColor::Bad, "trash", "Destroying reagents")
        };
        let buffer_buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            Node { align_items: AlignItems::Center }
            Children [
                Node { margin: UiRect::right(px(6)) }
                Children [ Text(buffer_volume) TextColor({UiColor::Label.base()}) ]
                --
                @Button { @icon: mode_icon, @content: mode_text }
                Button { color: mode_color }
                Act("toggleTransferMode")
            ]
        }));
        let buffer_body: Box<dyn Scene> = if data.buffer.contents.is_empty() {
            Box::new(label_line("Buffer is empty."))
        } else {
            Box::new(reagent_table(data, &data.buffer.contents, "buffer"))
        };
        let bottom: Box<dyn Scene> = if data.is_printing {
            Box::new(printing(data))
        } else {
            Box::new(packaging(st, data, local))
        };
        bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                @Section { @title: "Beaker", @buttons: {Some(beaker_buttons)} }
                Children [ {EntityScene(beaker_body)} ]
                --
                @Section { @title: "Buffer", @buttons: {Some(buffer_buttons)} }
                Children [ {EntityScene(buffer_body)} ]
                --
                {EntityScene(bottom)}
            ]
        }
    }

    fn label_line(text: &'static str) -> impl Scene + use<> {
        bsn! {
            Node { margin: UiRect::vertical(px(4)) }
            Children [ Text(text) TextColor({UiColor::Label.base()}) ]
        }
    }

    /// `ReagentEntry` rows.
    fn reagent_table(
        data: &ChemMasterData,
        contents: &[AnalyzableReagent],
        list: &str,
    ) -> impl Scene + use<> {
        let to = if list == "beaker" { "buffer" } else { "beaker" };
        let rows: Vec<_> = contents
            .iter()
            .map(|c| {
                let label = format!("{} {}u", c.name, js_num(c.volume));
                let amounts = [
                    ("1".to_owned(), 1.0),
                    ("5".to_owned(), 5.0),
                    ("10".to_owned(), 10.0),
                    (
                        js_num(data.custom_transfer_amount),
                        data.custom_transfer_amount,
                    ),
                    ("All".to_owned(), ALL_AMOUNT),
                ];
                let printing = data.is_printing;
                let mut buttons: Vec<Box<dyn Scene>> = amounts
                    .into_iter()
                    .map(|(text, amount)| {
                        let params = transfer_params(&c.r#ref, amount, to);
                        Box::new(bsn! {
                            @Button { @content: text }
                            Button { disabled: printing }
                            Act("transfer")
                            params
                        }) as Box<dyn Scene>
                    })
                    .collect();
                let ask = transfer_params(&c.r#ref, ASK_AMOUNT, to);
                buttons.push(Box::new(bsn! {
                    @Button { @icon: "ellipsis-h", @tooltip: "Custom amount" }
                    Button { disabled: printing }
                    Act("transfer")
                    ask
                }));
                let analyze = SetLocal {
                    key: ANALYZE.into(),
                    value: format!("{list}|{}", c.r#ref),
                    rerender: true,
                };
                buttons.push(Box::new(bsn! {
                    @Button { @icon: "question", @tooltip: "Analyze" }
                    analyze
                }));
                bsn! {
                    @TableRow
                    Children [
                        @TableCell { @content: label, @color: UiColor::Label }
                        --
                        @TableCell { @collapsing: true }
                        Children [ Node { flex_wrap: FlexWrap::NoWrap } Children [ {buttons} ] ]
                    ]
                }
            })
            .collect();
        bsn! { @Table Children [ {rows} ] }
    }

    fn transfer_params(r: &str, amount: f32, to: &str) -> ActParams {
        ActParams(vec![
            ("reagentRef".into(), r.to_owned()),
            ("amount".into(), js_num(amount)),
            ("target".into(), to.to_owned()),
        ])
    }

    fn packaging(
        st: &ChemMasterStatic,
        data: &ChemMasterData,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        let count: u32 = local
            .get(COUNT)
            .parse::<f64>()
            .ok()
            .map_or(1, |n| (n as u32).clamp(1, st.max_printable.max(1)));
        let suggest = local.get(SUGGEST) == "1";
        let buttons: Option<Box<dyn SceneList>> = if data.buffer.contents.is_empty() {
            None
        } else {
            let toggle = SetLocal {
                key: SUGGEST.into(),
                value: if suggest { "0" } else { "1" }.into(),
                rerender: true,
            };
            let check = if suggest {
                "check-square-o"
            } else {
                "square-o"
            };
            let count_input = NumberInput {
                value: f64::from(count),
                min_value: 1.0,
                max_value: f64::from(st.max_printable),
                step: 1.0,
                unit: "items".into(),
                ..Default::default()
            };
            let pills = data.selected_container_category.as_deref() == Some("pills");
            let duration: Box<dyn SceneList> = if pills {
                let input = NumberInput {
                    value: f64::from(data.selected_pill_duration),
                    min_value: 0.0,
                    max_value: f64::from(st.max_pill_duration),
                    step: 1.0,
                    unit: "s".into(),
                    ..Default::default()
                };
                let on_change = crate::client::ActOnChange {
                    action: "setPillDuration".into(),
                    param: "duration".into(),
                };
                Box::new(EntityScene(bsn! { Node { width: px(60) } input on_change }))
            } else {
                Box::new(Vec::<Box<dyn Scene>>::new())
            };
            let each = format!(
                "{} u. each",
                js_num(volume_each(
                    data.selected_container_volume,
                    data.buffer.current_volume,
                    count
                ))
            );
            let count_local = LocalOnNumber(COUNT.to_owned());
            let print_count = ActWithLocal {
                param: "itemCount".into(),
                key: COUNT.into(),
                default: "1".into(),
            };
            Some(Box::new(EntityScene(bsn! {
                Node { align_items: AlignItems::Center }
                Children [
                    @Button { @icon: check, @content: "Suggest" }
                    Button { selected: suggest }
                    toggle
                    --
                    Node { width: px(70) }
                    count_input
                    count_local
                    --
                    {duration}
                    --
                    Node { margin: UiRect::horizontal(px(6)) }
                    Children [ Text(each) ]
                    --
                    @Button { @icon: "flask", @content: "Print" }
                    Act("create")
                    print_count
                ]
            })))
        };
        let groups: Vec<_> = st
            .categories
            .iter()
            .map(|category| {
                let pill_patch = matches!(category.name.as_str(), "pills" | "patches");
                let title = category.name.clone();
                let buttons: Vec<_> = category
                    .containers
                    .iter()
                    .map(|c| container_button(data, c, pill_patch, suggest))
                    .collect();
                bsn! {
                    Node { flex_direction: FlexDirection::Column }
                    Children [
                        Node { align_items: AlignItems::Center, margin: UiRect::vertical(px(6)) }
                        Children [
                            Node { flex_grow: 1.0 } Children [ @Divider ]
                            --
                            Node { margin: UiRect::horizontal(px(6)) }
                            Children [ Text(title) TextColor({UiColor::Grey.base()}) ]
                            --
                            Node { flex_grow: 1.0 } Children [ @Divider ]
                        ]
                        --
                        Node { flex_wrap: FlexWrap::Wrap }
                        Children [ {buttons} ]
                    ]
                }
            })
            .collect();
        bsn! {
            @Section { @title: "Packaging", @buttons: {buttons} }
            Children [ {groups} ]
        }
    }

    /// `ContainerButton`.
    fn container_button(
        data: &ChemMasterData,
        c: &PrintableContainer,
        pill_patch: bool,
        suggest: bool,
    ) -> impl Scene + use<> {
        let size: f32 = if pill_patch { 48.0 } else { 64.0 };
        let image: Box<dyn SceneList> = match c.icon {
            Some(icon) => Box::new(EntityScene(bsn! {
                Node { width: percent(100), height: percent(100) }
                UiSprite { icon_state: {Some(icon.icon_state)}, color: {icon.color} }
            })),
            None => Box::new(EntityScene(bsn! {
                Node {
                    width: percent(100),
                    height: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                }
                Children [ @Icon { @name: "question" } ]
            })),
        };
        let selected = c.r#ref == data.selected_container_ref;
        let suggested = suggest
            && data.selected_container_ref != data.suggested_container_ref
            && c.r#ref == data.suggested_container_ref;
        let color = if suggested {
            UiColor::Blue
        } else {
            UiColor::Transparent
        };
        let tooltip = format!("{}\u{a0}({}u)", capitalize(&c.name), js_num(c.volume));
        let params = ActParams(vec![("ref".into(), c.r#ref.clone())]);
        let disabled = data.is_printing;
        let margin = if pill_patch { 4.0 } else { 2.0 };
        bsn! {
            Node { margin: UiRect::all(px(margin)) }
            Children [
                @ImageButton { @image: image, @image_size: size, @tooltip: tooltip }
                ImageButton { color: color, selected: selected, disabled: disabled }
                Act("selectContainer")
                params
            ]
        }
    }

    fn capitalize(s: &str) -> String {
        let mut chars = s.chars();
        chars
            .next()
            .map(|c| c.to_uppercase().chain(chars).collect())
            .unwrap_or_default()
    }

    fn printing(data: &ChemMasterData) -> impl Scene + use<> {
        let stop: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            @Button { @icon: "times", @content: "Stop" }
            Button { color: UiColor::Bad }
            Act("stopPrinting")
        }));
        let text = format!(
            "Printing {} out of {}",
            data.printing_progress, data.printing_total
        );
        let bar = ProgressBar {
            value: data.printing_progress as f32,
            min_value: 0.0,
            max_value: data.printing_total.max(1) as f32,
            color: UiColor::Good,
            ..Default::default()
        };
        bsn! {
            @Section { @title: "Printing", @buttons: {Some(stop)} }
            Children [
                @ProgressBar { @empty: true }
                bar
                Children [ Text(text) ]
            ]
        }
    }

    /// `AnalysisResults`.
    fn analysis(r: &AnalyzableReagent) -> impl Scene + use<> {
        let back = SetLocal {
            key: ANALYZE.into(),
            value: String::new(),
            rerender: true,
        };
        let back: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            @Button { @icon: "arrow-left", @content: "Back" }
            back
        }));
        let (level, purity_color) = if r.purity <= 0.5 {
            ("Bad", UiColor::Bad)
        } else if r.purity <= 0.75 {
            ("Average", UiColor::Average)
        } else {
            ("Good", UiColor::Good)
        };
        let color = parse_hex(&r.color).map_or(UiColor::Default, UiColor::Css);
        let color_text = r.color.clone();
        let meta = format!("{} units/second", js_num(r.meta_rate));
        let overdose = if r.overdose > 0.0 {
            format!("{} units", js_num(r.overdose))
        } else {
            "N/A".to_owned()
        };
        let addiction = if r.addiction_types.is_empty() {
            "N/A".to_owned()
        } else {
            r.addiction_types.join(",")
        };
        let (name, ph, description) = (r.name.clone(), js_num(r.ph), r.description.clone());
        bsn! {
            @Section { @title: "Analysis Results", @buttons: {Some(back)} }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem { @label: "Name", @content: name }
                    --
                    @LabeledListItem { @label: "Purity", @content: level, @color: purity_color }
                    --
                    @LabeledListItem { @label: "pH", @content: ph }
                    --
                    @LabeledListItem { @label: "Color" }
                    Children [
                        Node { align_items: AlignItems::Center }
                        Children [
                            Node { width: px(12), height: px(12), margin: UiRect::right(px(6)) }
                            BackgroundColor({color.base()})
                            --
                            Text(color_text)
                        ]
                    ]
                    --
                    @LabeledListItem { @label: "Description", @content: description }
                    --
                    @LabeledListItem { @label: "Metabolization Rate", @content: meta }
                    --
                    @LabeledListItem { @label: "Overdose Threshold", @content: overdose }
                    --
                    @LabeledListItem { @label: "Addiction Types", @content: addiction }
                ]
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips_and_each_volume_rounds() {
        let data = ChemMasterData {
            buffer: AnalyzableBeaker {
                max_volume: 100.0,
                current_volume: 10.0,
                contents: vec![AnalyzableReagent {
                    r#ref: "/datum/reagent/water".into(),
                    name: "Water".into(),
                    volume: 10.0,
                    ..Default::default()
                }],
            },
            selected_container_category: Some("pills".into()),
            ..Default::default()
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<ChemMasterData>(&bytes).unwrap(), data);
        assert_eq!(volume_each(50.0, 10.0, 3), 3.33);
        assert_eq!(volume_each(5.0, 100.0, 2), 5.0);
    }
}
