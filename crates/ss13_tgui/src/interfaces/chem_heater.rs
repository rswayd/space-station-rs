//! `ChemHeater` (the reaction chamber): `tgui/packages/tgui/interfaces/ChemHeater.tsx`,
//! `common/BeakerDisplay.tsx` (`BeakerSectionDisplay`) and `/obj/machinery/chem_heater/ui_data`
//! (`code/modules/reagents/chemistry/machinery/chem_heater.dm`). The server half lives with the
//! game (`ss13_game::chem_heater`).
// shortcut: `RoundGauge` (pH and reaction quality dials) is text: the pH reading turns red while
// `isFlashing`, the quality is a percentage coloured by the gauge's ranges.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct ChemHeaterUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ChemHeaterData {
    pub target_temp: f32,
    pub is_active: bool,
    /// `heater_coefficient * 10`.
    pub upgrade_level: f32,
    pub has_beaker_in_hand: bool,
    pub beaker: Option<HeaterBeaker>,
    pub current_temp: f32,
    pub active_reactions: Vec<ActiveReaction>,
    /// `ENABLE_FLASHING` (true) when the pH is out of some reaction's optimum.
    pub is_flashing: bool,
    pub acidic_buffer_vol: f32,
    pub basic_buffer_vol: f32,
    pub dispense_volume: f32,
}

/// `beaker`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HeaterBeaker {
    pub max_volume: f32,
    pub ph: f32,
    pub current_volume: f32,
    /// (name, volume).
    pub contents: Vec<(String, f32)>,
}

/// One `activeReactions` entry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ActiveReaction {
    pub name: String,
    pub danger: bool,
    pub overheat: bool,
    /// The product's purity is under the reaction's `purity_min`.
    pub purity_alert: bool,
    pub quality: f32,
    pub inverse: f32,
    pub min_pure: f32,
    pub reacted_vol: f32,
    pub target_vol: f32,
}

impl UiInterface for ChemHeaterUi {
    const NAME: &'static str = "ChemHeater";
    type Data = ChemHeaterData;
    type StaticData = ();
}

/// `COLORS.reagent.acidicbuffer`, `basicbuffer`.
pub const ACIDIC_COLOR: [u8; 3] = [0xfb, 0xc3, 0x14];
pub const BASIC_COLOR: [u8; 3] = [0x38, 0x53, 0xa4];

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{ACIDIC_COLOR, ActiveReaction, BASIC_COLOR, ChemHeaterData, ChemHeaterUi};
    use crate::client::{ActOnChange, ClientUi};
    use crate::interfaces::chem_dispenser::js_num;

    impl ClientUi for ChemHeaterUi {
        const WIDTH: f32 = 350.0;
        const HEIGHT: f32 = 350.0;

        fn content(_: &(), data: &ChemHeaterData) -> impl Scene {
            let (power_icon, power_text) = if data.is_active {
                ("power-off", "On")
            } else {
                ("times", "Off")
            };
            let on = data.is_active;
            let power: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                @Button { @icon: power_icon, @content: power_text }
                Button { selected: on }
                Act("power")
            }));
            let volume = NumberInput {
                value: f64::from(data.dispense_volume),
                min_value: 1.0,
                max_value: 10.0,
                step: 1.0,
                step_pixel_size: 3.0,
                unit: "u".into(),
                tick_while_dragging: true,
                ..Default::default()
            };
            let on_volume = ActOnChange {
                action: "disp_vol".into(),
                param: "target".into(),
            };
            let target = NumberInput {
                value: f64::from((data.target_temp * 10.0).round() / 10.0),
                min_value: 0.0,
                max_value: 1000.0,
                step: 10.0,
                step_pixel_size: 3.0,
                unit: "K".into(),
                tick_while_dragging: true,
                ..Default::default()
            };
            let on_target = ActOnChange {
                action: "temperature".into(),
                param: "target".into(),
            };
            let reading = if data.beaker.is_some() {
                format!("{} K", data.current_temp.round())
            } else {
                "—".to_owned()
            };
            let acid = buffer_row(
                "Acidic:",
                "acidBuffer",
                data.acidic_buffer_vol,
                ACIDIC_COLOR,
            );
            let base = buffer_row("Basic:", "basicBuffer", data.basic_buffer_vol, BASIC_COLOR);
            let reactions: Box<dyn Scene> = match &data.beaker {
                Some(b) => Box::new(reaction_display(data, b.ph)),
                None => Box::new(bsn! { Node }),
            };
            let beaker = beaker_section(data);
            bsn! {
                @WindowContent { @scrollable: true }
                Children [
                    @Section { @title: "Controls", @buttons: {Some(power)} }
                    Children [
                        @Table
                        Children [
                            @TableRow
                            Children [
                                @TableCell { @content: "Heat", @bold: true, @collapsing: true, @color: UiColor::Label }
                                --
                                @TableCell
                                --
                                @TableCell { @content: "Buffers", @bold: true, @collapsing: true, @color: UiColor::Label }
                                --
                                @TableCell
                                --
                                @TableCell
                                Children [ Node { width: px(45) } volume on_volume ]
                            ]
                            --
                            @TableRow
                            Children [
                                @TableCell { @content: "Target:", @collapsing: true, @color: UiColor::Label }
                                --
                                @TableCell
                                Children [ Node { width: px(65) } target on_target ]
                                --
                                {acid}
                            ]
                            --
                            @TableRow
                            Children [
                                @TableCell { @content: "Reading:", @collapsing: true, @color: UiColor::Label }
                                --
                                @TableCell { @content: reading, @collapsing: true }
                                --
                                {base}
                            ]
                        ]
                    ]
                    --
                    {EntityScene(reactions)}
                    --
                    {EntityScene(beaker)}
                ]
            }
        }
    }

    /// The `Acidic:`/`Basic:` cells: inject 1 dispense, the buffer volume, draw all.
    fn buffer_row(
        label: &'static str,
        action: &'static str,
        volume: f32,
        color: [u8; 3],
    ) -> Vec<Box<dyn Scene>> {
        let inject = ActParams(vec![("target".into(), "1".into())]);
        let draw = ActParams(vec![("target".into(), "-100".into())]);
        let empty = volume == 0.0;
        let full = volume == 100.0;
        let text = format!("{}u", js_num(volume));
        vec![
            Box::new(
                bsn! { @TableCell { @content: label, @collapsing: true, @color: UiColor::Label } },
            ),
            Box::new(bsn! {
                @TableCell
                Children [
                    @Button { @icon: "syringe", @tooltip: "Inject" }
                    Button { disabled: empty }
                    Act(action)
                    inject
                ]
            }),
            Box::new(bsn! {
                @TableCell { @content: text, @color: {UiColor::Css(color)}, @text_align: {Some(Justify::Center)} }
            }),
            Box::new(bsn! {
                @TableCell
                Children [
                    @Button { @icon: "upload", @tooltip: "Draw all" }
                    Button { disabled: full }
                    Act(action)
                    draw
                ]
            }),
        ]
    }

    /// `ReactionDisplay`.
    fn reaction_display(data: &ChemHeaterData, ph: f32) -> impl Scene + use<> {
        let ph_text = format!("pH: {}", js_num((ph * 1000.0).round() / 1000.0));
        let ph_color = if data.is_flashing {
            UiColor::Bad
        } else {
            UiColor::Label
        };
        let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            Text(ph_text) TextColor({ph_color.base()})
        }));
        let high_quality = data.upgrade_level >= 4.0;
        let high_danger = data.upgrade_level >= 2.0;
        let body: Box<dyn Scene> = if data.active_reactions.is_empty() {
            Box::new(bsn! {
                Node
                Children [ Text("No active reactions.") TextColor({UiColor::Label.base()}) ]
            })
        } else {
            let status = if high_quality {
                "Reaction quality"
            } else {
                "Status"
            };
            let rows: Vec<_> = data
                .active_reactions
                .iter()
                .map(|r| reaction_row(r, high_quality, high_danger))
                .collect();
            Box::new(bsn! {
                @Table
                Children [
                    @TableRow
                    Children [
                        @TableCell { @content: "Reaction", @bold: true, @color: UiColor::Label }
                        --
                        @TableCell { @content: status, @bold: true, @color: UiColor::Label }
                        --
                        @TableCell { @content: "Target", @bold: true, @color: UiColor::Label }
                    ]
                    --
                    {rows}
                ]
            })
        };
        bsn! {
            @Section { @title: "Reactions", @buttons: {Some(buttons)} }
            Children [ {EntityScene(body)} ]
        }
    }

    fn reaction_row(
        r: &ActiveReaction,
        high_quality: bool,
        high_danger: bool,
    ) -> impl Scene + use<> {
        let name_color = if r.danger {
            UiColor::Red
        } else {
            UiColor::Default
        };
        let name = r.name.clone();
        let status: Box<dyn Scene> = if high_quality {
            let color = if r.quality < r.min_pure {
                UiColor::Red
            } else if r.quality < r.inverse {
                UiColor::Orange
            } else if r.quality < 0.8 {
                UiColor::Yellow
            } else {
                UiColor::Green
            };
            let text = format!("{}%", (r.quality * 100.0).round());
            Box::new(bsn! { Text(text) TextColor({color.base()}) })
        } else {
            let (icon, color) = if r.danger {
                ("exclamation-triangle", UiColor::Red)
            } else {
                ("spinner", UiColor::Default)
            };
            Box::new(bsn! { @Icon { @name: icon, @color: color } })
        };
        let target = format!("{}u", js_num(r.target_vol));
        let target_cell: Box<dyn Scene> = if high_danger {
            let bar = ProgressBar {
                value: r.reacted_vol,
                min_value: 0.0,
                max_value: r.target_vol.max(f32::EPSILON),
                color: if r.overheat {
                    UiColor::Red
                } else {
                    UiColor::Label
                },
                ..Default::default()
            };
            let overheat: Box<dyn SceneList> = if r.overheat {
                Box::new(EntityScene(bsn! {
                    Node { margin: UiRect::right(px(5)) }
                    Children [ @Icon { @name: "thermometer-full", @color: UiColor::Red } ]
                }))
            } else {
                Box::new(Vec::<Box<dyn Scene>>::new())
            };
            Box::new(bsn! {
                Node { align_items: AlignItems::Center }
                Children [
                    {overheat}
                    --
                    Node { width: px(96) }
                    Children [ @ProgressBar { @empty: true } bar Children [ Text(target) ] ]
                ]
            })
        } else {
            let color = if r.danger {
                UiColor::Red
            } else {
                UiColor::Default
            };
            Box::new(bsn! { Text(target) TextColor({color.base()}) })
        };
        bsn! {
            @TableRow
            Children [
                @TableCell { @content: name, @color: name_color }
                --
                @TableCell Children [ {EntityScene(status)} ]
                --
                @TableCell Children [ {EntityScene(target_cell)} ]
            ]
        }
    }

    /// `BeakerSectionDisplay` with the insert button and no pH.
    fn beaker_section(data: &ChemHeaterData) -> Box<dyn Scene> {
        let Some(b) = &data.beaker else {
            let tooltip = if data.has_beaker_in_hand {
                ""
            } else {
                "You need to hold a container in your hand"
            };
            let insert: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                @Button { @icon: "eject", @content: "Insert", @tooltip: tooltip }
                Act("insert")
            }));
            return Box::new(bsn! {
                @Section { @title: "Beaker", @buttons: {Some(insert)} }
                Children [ Text("No beaker loaded") TextColor({UiColor::Label.base()}) ]
            });
        };
        let volume = format!(
            "{} / {} units",
            js_num(b.current_volume),
            js_num(b.max_volume)
        );
        let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            Node { align_items: AlignItems::Center }
            Children [
                Node { margin: UiRect::right(px(12)) }
                Children [ Text(volume) TextColor({UiColor::Label.base()}) ]
                --
                @Button { @icon: "eject", @content: "Eject" }
                Act("eject")
            ]
        }));
        let lines: Vec<String> = if b.contents.is_empty() {
            vec!["Nothing".to_owned()]
        } else {
            b.contents
                .iter()
                .map(|(name, v)| format!("{} units of {name}", js_num(*v)))
                .collect()
        };
        let lines: Vec<_> = lines
            .into_iter()
            .map(|t| bsn! { Text(t) TextColor({UiColor::Label.base()}) })
            .collect();
        Box::new(bsn! {
            @Section { @title: "Beaker", @buttons: {Some(buttons)} }
            Children [ Node { flex_direction: FlexDirection::Column } Children [ {lines} ] ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips() {
        let data = ChemHeaterData {
            target_temp: 400.0,
            is_active: true,
            upgrade_level: 1.0,
            beaker: Some(HeaterBeaker {
                max_volume: 50.0,
                ph: 7.0,
                current_volume: 20.0,
                contents: vec![("Water".into(), 20.0)],
            }),
            active_reactions: vec![ActiveReaction {
                name: "Smoke".into(),
                quality: 0.9,
                target_vol: 10.0,
                ..Default::default()
            }],
            dispense_volume: 1.0,
            ..Default::default()
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<ChemHeaterData>(&bytes).unwrap(), data);
    }
}
