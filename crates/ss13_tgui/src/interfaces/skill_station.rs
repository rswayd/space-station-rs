//! `SkillStation`: `tgui/packages/tgui/interfaces/SkillStation.jsx` and
//! `/obj/machinery/skill_station/ui_data` (`code/modules/library/skill_learning/
//! skill_station.dm`). The server half lives with the game (`ss13_game::skill_station`).
// shortcut: the implanted chips are rows of text and buttons instead of tgui's table with
// header icons; the inserted chip's icon is drawn at the normal size.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct SkillStationUi;

pub const ACT_IMPLANT: &str = "implant";
pub const ACT_EJECT: &str = "eject";
pub const ACT_REMOVE: &str = "remove";
pub const ACT_TOGGLE: &str = "toggle_activate";
/// The param of [`ACT_REMOVE`] and [`ACT_TOGGLE`]: the chip's `ref`.
pub const PARAM_REF: &str = "ref";

/// One `current` entry (`/obj/item/skillchip/get_chip_data`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SkillChipEntry {
    pub name: String,
    /// Font Awesome icon name.
    pub icon: String,
    pub desc: String,
    pub complexity: u32,
    pub slot_use: u32,
    pub removable: bool,
    /// `ref`: what the chip acts send back.
    pub chip_ref: String,
    pub active: bool,
    pub active_error: Option<String>,
    /// `cooldown`, seconds.
    pub cooldown: f32,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SkillStationData {
    pub working: bool,
    /// `timeleft`, seconds.
    pub timeleft: Option<f32>,
    pub skillchip_ready: bool,
    pub implantable: bool,
    pub implantable_reason: Option<String>,
    pub skill_name: String,
    pub skill_desc: String,
    pub skill_icon: String,
    pub complexity: u32,
    pub slot_use: u32,
    /// No occupant or no brain.
    pub error: Option<String>,
    pub current: Vec<SkillChipEntry>,
    pub complexity_used: u32,
    pub complexity_max: u32,
    pub slots_used: u32,
    pub slots_max: u32,
}

impl UiInterface for SkillStationUi {
    const NAME: &'static str = "SkillStation";
    type Data = SkillStationData;
    type StaticData = ();
}

/// `TimeFormat`: `HH:MM:SS` of `seconds`.
pub fn time_format(seconds: f32) -> String {
    let total = seconds.max(0.0) as u32;
    format!(
        "{:02}:{:02}:{:02}",
        (total / 3600) % 24,
        (total / 60) % 60,
        total % 60
    )
}

/// The toggle button's tooltip.
pub fn toggle_tooltip(chip: &SkillChipEntry) -> String {
    match &chip.active_error {
        Some(error) if !chip.active => error.clone(),
        _ if chip.active => "Deactivate".to_owned(),
        _ => "Activate".to_owned(),
    }
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, UiColor};

    use super::{
        ACT_EJECT, ACT_IMPLANT, ACT_REMOVE, ACT_TOGGLE, PARAM_REF, SkillChipEntry,
        SkillStationData, SkillStationUi, time_format, toggle_tooltip,
    };
    use crate::client::ClientUi;

    impl ClientUi for SkillStationUi {
        const WIDTH: f32 = 500.0;
        const HEIGHT: f32 = 500.0;

        fn content(_: &(), data: &SkillStationData) -> impl Scene {
            let error = data.error.clone().map(|e| {
                EntityScene(bsn! {
                    @NoticeBox
                    Children [ Text(e) ]
                })
            });
            let working = data.working.then(|| {
                let left = format!(
                    "Time Left: {}",
                    time_format(data.timeleft.unwrap_or_default())
                );
                EntityScene(bsn! {
                    @NoticeBox { @danger: true }
                    Children [
                        Node { flex_direction: FlexDirection::Column }
                        Children [
                            Text("Operation in progress. Please do not leave the chamber.")
                            --
                            Text(left)
                        ]
                    ]
                })
            });
            let inserted: Box<dyn Scene> = if data.skillchip_ready {
                Box::new(inserted(data))
            } else if data.working {
                Box::new(bsn! { Node })
            } else {
                Box::new(bsn! {
                    @NoticeBox { @info: true }
                    Children [ Text("Please insert a skillchip.") ]
                })
            };
            let implanted: Box<dyn Scene> = Box::new(implanted(data));
            bsn! {
                @WindowContent { @scrollable: true }
                Children [
                    {error}
                    --
                    {working}
                    --
                    {EntityScene(inserted)}
                    --
                    {EntityScene(implanted)}
                ]
            }
        }
    }

    /// `InsertedSkillchip`.
    fn inserted(data: &SkillStationData) -> impl Scene + use<> {
        let implant_disabled = !data.implantable || data.working;
        let implant_color = if data.implantable {
            UiColor::Good
        } else {
            UiColor::Default
        };
        let reason = data.implantable_reason.clone().unwrap_or_default();
        let eject_disabled = data.working;
        let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            Node
            Children [
                @Button { @icon: "syringe", @content: "Implant", @tooltip: reason }
                Button { color: implant_color, disabled: implant_disabled }
                Act(ACT_IMPLANT)
                --
                @Button { @icon: "eject", @content: "Eject" }
                Button { disabled: eject_disabled }
                Act(ACT_EJECT)
            ]
        }));
        let name = data.skill_name.clone();
        let desc = data.skill_desc.clone();
        let icon = data.skill_icon.clone();
        let complexity = data.complexity.to_string();
        let slot = data.slot_use.to_string();
        let slot_color = if data.slots_used + data.slot_use > data.slots_max {
            UiColor::Bad
        } else {
            UiColor::Default
        };
        let error = data.implantable_reason.clone().map(|reason| {
            let color = if data.implantable {
                UiColor::Good
            } else {
                UiColor::Bad
            };
            EntityScene(bsn! {
                @LabeledListItem { @label: "Error", @color: color, @content: reason }
            })
        });
        bsn! {
            @Section { @title: "Inserted Skillchip", @buttons: {Some(buttons)} }
            Children [
                Node { align_items: AlignItems::Center }
                Children [
                    Node { margin: UiRect::all(px(6)) }
                    Children [ @Icon { @name: icon, @size: 3.0 } ]
                    --
                    Node { flex_grow: 1.0, flex_basis: px(0) }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Skillchip", @content: name }
                            --
                            @LabeledListItem { @label: "Description", @content: desc }
                            --
                            @LabeledListItem { @label: "Complexity", @content: complexity }
                            --
                            @LabeledListItem { @label: "Slot Size", @color: slot_color, @content: slot }
                            --
                            {error}
                        ]
                    ]
                ]
            ]
        }
    }

    /// `ImplantedSkillchips`.
    fn implanted(data: &SkillStationData) -> impl Scene + use<> {
        let header = format!(
            "Complexity {}/{} · Slots {}/{}",
            data.complexity_used, data.complexity_max, data.slots_used, data.slots_max
        );
        let rows: Vec<_> = data.current.iter().map(|chip| row(chip, data)).collect();
        let empty = data
            .current
            .is_empty()
            .then_some(EntityScene(bsn! { Text("No skillchips detected.") }));
        bsn! {
            @Section { @title: "Implanted Skillchips" }
            Children [
                Node { flex_direction: FlexDirection::Column }
                Children [
                    {empty}
                    --
                    Text(header)
                    --
                    {rows}
                ]
            ]
        }
    }

    fn row(chip: &SkillChipEntry, data: &SkillStationData) -> impl Scene + use<> {
        let icon = chip.icon.clone();
        let name = chip.name.clone();
        let complexity_color = if chip.active {
            UiColor::Good
        } else if chip.complexity + data.complexity_used > data.complexity_max {
            UiColor::Bad
        } else {
            UiColor::Grey
        };
        let complexity = chip.complexity.to_string();
        let slot = chip.slot_use.to_string();
        let (state_icon, state_color) = if chip.active {
            ("check", UiColor::Good)
        } else {
            ("times", UiColor::Bad)
        };
        let cooldown = format!("{}s", chip.cooldown.max(0.0).ceil() as u32);
        let busy = chip.cooldown > 0.0 || data.working;
        let (remove_icon, remove_tip, remove_color) = if chip.removable {
            ("eject", "Extract", UiColor::Good)
        } else {
            ("trash", "Destroy", UiColor::Bad)
        };
        let toggle_disabled =
            busy || (!chip.active && chip.complexity + data.complexity_used > data.complexity_max);
        let (toggle_icon, toggle_color) = if chip.active {
            ("check-square-o", UiColor::Good)
        } else {
            ("square-o", UiColor::Default)
        };
        let toggle_tip = toggle_tooltip(chip);
        let remove_params = ActParams(vec![(PARAM_REF.into(), chip.chip_ref.clone())]);
        let toggle_params = remove_params.clone();
        bsn! {
            Node { align_items: AlignItems::Center, margin: UiRect::bottom(px(2)) }
            Children [
                Node { width: px(22) }
                Children [ @Icon { @name: icon } ]
                --
                Node { flex_grow: 1.0 }
                Children [ Text(name) ]
                --
                Node { width: px(36) }
                Children [ Text(complexity) TextColor({complexity_color.base()}) ]
                --
                Node { width: px(36) }
                Children [ Text(slot) TextColor({UiColor::Good.base()}) ]
                --
                Node { width: px(28) }
                Children [ @Icon { @name: state_icon, @color: state_color } ]
                --
                Node { width: px(48) }
                Children [ Text(cooldown) ]
                --
                @Button { @icon: remove_icon, @tooltip: remove_tip }
                Button { color: remove_color, disabled: busy }
                Act(ACT_REMOVE)
                remove_params
                --
                @Button { @icon: toggle_icon, @tooltip: toggle_tip }
                Button { color: toggle_color, disabled: toggle_disabled }
                Act(ACT_TOGGLE)
                toggle_params
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips_and_formats() {
        let chip = SkillChipEntry {
            name: "Wine Tasting".into(),
            icon: "wine-bottle".into(),
            complexity: 1,
            slot_use: 1,
            removable: true,
            chip_ref: "42".into(),
            active: false,
            active_error: Some("Skillchip is too complex".into()),
            cooldown: 12.5,
            ..Default::default()
        };
        let data = SkillStationData {
            working: true,
            timeleft: Some(14.0),
            current: vec![chip.clone()],
            complexity_max: 3,
            slots_max: 5,
            ..Default::default()
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<SkillStationData>(&bytes).unwrap(), data);
        assert_eq!(time_format(3725.0), "01:02:05");
        assert_eq!(toggle_tooltip(&chip), "Skillchip is too complex");
        let active = SkillChipEntry {
            active: true,
            ..chip
        };
        assert_eq!(toggle_tooltip(&active), "Deactivate");
    }
}
