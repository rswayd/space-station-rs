//! `Cryo`: `tgui/packages/tgui/interfaces/Cryo.tsx` and `/obj/machinery/cryo_cell/ui_data`
//! (`code/modules/atmospherics/machinery/components/unary_devices/cryo.dm`). The server half
//! lives with the game (`ss13_game::medical::cryo`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct CryoUi;

/// tg `T0C` (`ui_static_data`): a body under it heals with cryoxadone.
pub const T0C: f32 = 273.15;

/// `occupant`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CryoOccupant {
    pub name: String,
    /// `Dead`, `Unconscious` or `Stable`.
    pub stat: String,
    /// Kelvin, rounded.
    pub body_temperature: f32,
    pub health: f32,
    pub max_health: f32,
    pub brute_loss: f32,
    pub oxy_loss: f32,
    pub tox_loss: f32,
    pub fire_loss: f32,
}

/// `beaker` (`BeakerSectionDisplay`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CryoBeaker {
    pub max_volume: f32,
    pub current_volume: f32,
    /// (name, volume).
    pub contents: Vec<(String, f32)>,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CryoData {
    pub is_operating: bool,
    pub is_open: bool,
    pub auto_eject: bool,
    pub occupant: Option<CryoOccupant>,
    /// Kelvin.
    pub cell_temperature: f32,
    pub beaker: Option<CryoBeaker>,
}

impl UiInterface for CryoUi {
    const NAME: &'static str = "Cryo";
    type Data = CryoData;
    type StaticData = ();
}

/// `stat_to_color` (an unlisted state, `Stable`, has no color).
pub fn stat_color_name(stat: &str) -> Option<&'static str> {
    match stat {
        "Dead" | "Conscious" => Some("bad"),
        "Unconscious" => Some("good"),
        _ => None,
    }
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::UiColor;
    use ss13_ui_kit::prelude::*;

    use super::{CryoBeaker, CryoData, CryoOccupant, CryoUi, T0C, stat_color_name};
    use crate::client::ClientUi;
    use crate::interfaces::chem_dispenser::js_num;

    impl ClientUi for CryoUi {
        const WIDTH: f32 = 400.0;
        const HEIGHT: f32 = 550.0;

        fn content(_: &(), data: &CryoData) -> impl Scene {
            let name = data
                .occupant
                .as_ref()
                .map_or_else(|| "No Occupant".to_owned(), |o| o.name.clone());
            let occupant: Box<dyn Scene> = match &data.occupant {
                Some(o) => Box::new(occupant_rows(o)),
                None => Box::new(bsn! { Node }),
            };
            let (power_icon, power_text) = if data.is_operating {
                ("power-off", "On")
            } else {
                ("times", "Off")
            };
            let power_color = if data.is_operating {
                UiColor::Green
            } else {
                UiColor::Default
            };
            let open = data.is_open;
            let (door_icon, door_text) = if open {
                ("unlock", "Open")
            } else {
                ("lock", "Closed")
            };
            let (eject_icon, eject_text) = if data.auto_eject {
                ("sign-out-alt", "Auto")
            } else {
                ("sign-in-alt", "Manual")
            };
            let cell_temperature = format!("{} K", data.cell_temperature.round());
            let beaker = beaker_section(data.beaker.as_ref());
            bsn! {
                @WindowContent { @scrollable: true }
                Children [
                    @Section { @title: "Occupant" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Occupant", @content: name }
                            --
                            {EntityScene(occupant)}
                        ]
                    ]
                    --
                    @Section { @title: "Cell" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Power" }
                            Children [
                                @Button { @icon: power_icon, @content: power_text }
                                Button { disabled: open, color: power_color }
                                Act("power")
                            ]
                            --
                            @LabeledListItem { @label: "Temperature", @content: cell_temperature }
                            --
                            @LabeledListItem { @label: "Door" }
                            Children [
                                Node
                                Children [
                                    @Button { @icon: door_icon, @content: door_text }
                                    Act("door")
                                    --
                                    @Button { @icon: eject_icon, @content: eject_text }
                                    Act("autoeject")
                                ]
                            ]
                        ]
                    ]
                    --
                    {EntityScene(beaker)}
                ]
            }
        }
    }

    fn color_of(name: Option<&str>) -> UiColor {
        match name {
            Some("good") => UiColor::Good,
            Some("bad") => UiColor::Bad,
            _ => UiColor::Default,
        }
    }

    /// The state, temperature, health and damage rows.
    fn occupant_rows(o: &CryoOccupant) -> impl Scene + use<> {
        let stat_color = color_of(stat_color_name(&o.stat));
        let stat = o.stat.clone();
        // Green if cryoxadone can heal the body.
        let temperature_color = if o.body_temperature < T0C {
            UiColor::Good
        } else {
            UiColor::Bad
        };
        let temperature = format!("{} K", o.body_temperature.round());
        let max = if o.max_health > 0.0 {
            o.max_health
        } else {
            1.0
        };
        let health_value = ((o.health / max) * 100.0).round() / 100.0;
        let health_color = if o.health > 0.0 {
            UiColor::Good
        } else {
            UiColor::Average
        };
        let health_text = js_num(o.health.round());
        let damage: Vec<_> = [
            ("Brute", o.brute_loss),
            ("Respiratory", o.oxy_loss),
            ("Toxin", o.tox_loss),
            ("Burn", o.fire_loss),
        ]
        .into_iter()
        .map(|(label, value)| {
            let bar = ((value / 100.0) * 100.0).round() / 100.0;
            let text = js_num(value.round());
            bsn! {
                @LabeledListItem { @label: label }
                Children [
                    @ProgressBar { @empty: true }
                    ProgressBar { value: bar }
                    Node { flex_grow: 1.0, width: percent(100) }
                    Children [ Text(text) ]
                ]
            }
        })
        .collect();
        bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                @LabeledListItem { @label: "State", @color: stat_color, @content: stat }
                --
                @LabeledListItem {
                    @label: "Temperature",
                    @color: temperature_color,
                    @content: temperature,
                }
                --
                @LabeledListItem { @label: "Health" }
                Children [
                    @ProgressBar { @empty: true }
                    ProgressBar { value: health_value, color: health_color }
                    Node { flex_grow: 1.0, width: percent(100) }
                    Children [ Text(health_text) ]
                ]
                --
                {damage}
            ]
        }
    }

    /// `BeakerSectionDisplay` without pH or insert button.
    fn beaker_section(beaker: Option<&CryoBeaker>) -> Box<dyn Scene> {
        let Some(b) = beaker else {
            return Box::new(bsn! {
                @Section { @title: "Beaker" }
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
    fn data_round_trips_and_states_color() {
        let data = CryoData {
            is_operating: true,
            is_open: false,
            auto_eject: true,
            occupant: Some(CryoOccupant {
                name: "John".into(),
                stat: "Unconscious".into(),
                body_temperature: 120.0,
                health: 40.0,
                max_health: 100.0,
                brute_loss: 60.0,
                ..Default::default()
            }),
            cell_temperature: 73.0,
            beaker: Some(CryoBeaker {
                max_volume: 50.0,
                current_volume: 30.0,
                contents: vec![("Cryoxadone".into(), 30.0)],
            }),
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<CryoData>(&bytes).unwrap(), data);
        assert_eq!(stat_color_name("Unconscious"), Some("good"));
        assert_eq!(stat_color_name("Stable"), None);
    }
}
