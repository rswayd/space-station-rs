//! `IVDrip`: `tgui/packages/tgui/interfaces/IVDrip.tsx` and `/obj/machinery/iv_drip/ui_data`
//! (`code/game/machinery/iv_drip.dm`). The server half lives with the game
//! (`ss13_game::medical::iv_drip`).
// shortcut: the flow rate `Slider` is a `NumberInput` (same range, step and act); no tooltips on
// the "nothing attached" notices.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct IvDripUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct IvDripData {
    pub has_internal_storage: bool,
    pub has_container: bool,
    pub can_remove_container: bool,
    /// `mode`: injecting (true) or drawing blood.
    pub mode: bool,
    pub can_draw: bool,
    /// Units per second.
    pub transfer_rate: f32,
    pub transfer_step: f32,
    pub min_transfer_rate: f32,
    pub max_transfer_rate: f32,
    pub has_object_attached: bool,
    pub object_name: String,
    /// `mix_color_from_reagents`, RGBA.
    pub container_reagent_color: Option<[u8; 4]>,
    pub container_current_volume: f32,
    pub container_max_volume: f32,
}

impl UiInterface for IvDripUi {
    const NAME: &'static str = "IVDrip";
    type Data = IvDripData;
    type StaticData = ();
}

/// The `Direction` line.
pub fn direction_text(data: &IvDripData) -> &'static str {
    match (data.mode, data.has_internal_storage) {
        (true, true) => "Reagents from network",
        (true, false) => "Reagents from container",
        (false, _) => "Blood into container",
    }
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, UiColor};

    use super::{IvDripData, IvDripUi, direction_text};
    use crate::client::{ActOnChange, ClientUi};
    use crate::interfaces::chem_dispenser::js_num;

    impl ClientUi for IvDripUi {
        const WIDTH: f32 = 400.0;
        const HEIGHT: f32 = 220.0;

        fn content(_: &(), data: &IvDripData) -> impl Scene {
            let min = ActParams(vec![("rate".into(), js_num(data.min_transfer_rate))]);
            let max = ActParams(vec![("rate".into(), js_num(data.max_transfer_rate))]);
            let rate_buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                Node
                Children [
                    @Button { @icon: "angles-left" }
                    Node { width: px(48) }
                    Act("changeRate")
                    min
                    --
                    @Button { @icon: "angles-right" }
                    Node { width: px(48) }
                    Act("changeRate")
                    max
                ]
            }));
            let rate = NumberInput {
                value: f64::from(data.transfer_rate),
                min_value: f64::from(data.min_transfer_rate),
                max_value: f64::from(data.max_transfer_rate),
                step: f64::from(data.transfer_step),
                step_pixel_size: 2.0,
                unit: "units/sec.".into(),
                tick_while_dragging: true,
                ..Default::default()
            };
            let on_rate = ActOnChange {
                action: "changeRate".into(),
                param: "rate".into(),
            };
            let injecting = data.mode;
            let (mode_text, mode_icon) = if injecting {
                ("Injecting", "syringe")
            } else {
                ("Draining", "droplet")
            };
            let no_draw = !data.can_draw;
            let mode_color = if injecting {
                UiColor::Default
            } else {
                UiColor::Bad
            };
            let mode_button: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                @Button { @icon: mode_icon, @content: mode_text }
                Button { disabled: no_draw, color: mode_color }
                Node { width: px(96) }
                Act("changeMode")
            }));
            let direction = direction_text(data);
            let container: Box<dyn Scene> = if data.has_container || data.has_internal_storage {
                let eject: Option<Box<dyn SceneList>> =
                    (!data.has_internal_storage && data.can_remove_container).then(|| {
                        Box::new(EntityScene(bsn! {
                            @Button { @icon: "eject", @content: "Eject" }
                            Node { width: px(96) }
                            Act("eject")
                        })) as Box<dyn SceneList>
                    });
                let color = data
                    .container_reagent_color
                    .map_or(UiColor::Default, |[r, g, b, _]| UiColor::Css([r, g, b]));
                let (value, maximum) = (data.container_current_volume, data.container_max_volume);
                let text = format!("{} of {} units", js_num(value), js_num(maximum));
                Box::new(bsn! {
                    @LabeledListItem { @label: "Container", @buttons: {eject} }
                    Children [
                        @ProgressBar { @empty: true }
                        ProgressBar {
                            value: value,
                            min_value: 0.0,
                            max_value: maximum,
                            color: color,
                        }
                        Node { flex_grow: 1.0, width: percent(100) }
                        Children [ Text(text) ]
                    ]
                })
            } else {
                Box::new(bsn! {
                    @LabeledListItem { @label: "Container" }
                    Children [ @NoticeBox Children [ Text("No container attached.") ] ]
                })
            };
            let object: Box<dyn Scene> = if data.has_object_attached {
                let detach: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                    @Button { @icon: "ban", @content: "Disconnect" }
                    Node { width: px(96) }
                    Act("detach")
                }));
                let name = data.object_name.clone();
                Box::new(bsn! {
                    @LabeledListItem { @label: "Object", @buttons: {Some(detach)}, @content: name }
                })
            } else {
                Box::new(bsn! {
                    @LabeledListItem { @label: "Object" }
                    Children [ @NoticeBox Children [ Text("No object attached.") ] ]
                })
            };
            bsn! {
                @WindowContent
                Children [
                    @Section { @fill: true }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Flow Rate", @buttons: {Some(rate_buttons)} }
                            Children [ Node { width: px(160) } rate on_rate ]
                            --
                            @LabeledListItem {
                                @label: "Direction",
                                @color: mode_color,
                                @buttons: {Some(mode_button)},
                                @content: direction,
                            }
                            --
                            {EntityScene(container)}
                            --
                            {EntityScene(object)}
                        ]
                    ]
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips_and_the_direction_follows_the_mode() {
        let data = IvDripData {
            has_container: true,
            can_remove_container: true,
            mode: true,
            can_draw: true,
            transfer_rate: 5.0,
            transfer_step: 0.01,
            max_transfer_rate: 5.0,
            container_reagent_color: Some([200, 0, 0, 255]),
            container_current_volume: 150.0,
            container_max_volume: 200.0,
            ..Default::default()
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<IvDripData>(&bytes).unwrap(), data);
        assert_eq!(direction_text(&data), "Reagents from container");
        let draining = IvDripData {
            mode: false,
            ..data
        };
        assert_eq!(direction_text(&draining), "Blood into container");
    }
}
