//! `MedicalKiosk`: `tgui/packages/tgui/interfaces/MedicalKiosk.jsx` and
//! `/obj/machinery/medical_kiosk/ui_data` (`code/game/machinery/medical_kiosk.dm`). The server
//! half lives with the game (`ss13_game::medical::kiosk`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct MedicalKioskUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MedicalKioskData {
    pub kiosk_cost: f32,
    pub patient_name: String,
    /// Percent of the maximum health.
    pub patient_health: f32,
    pub brute_health: f32,
    pub burn_health: f32,
    pub toxin_health: f32,
    pub suffocation_health: f32,
    pub brain_health: String,
    pub brain_damage: f32,
    pub patient_status: String,
    pub trauma_status: String,
    pub patient_illness: String,
    pub illness_info: String,
    pub bleed_status: String,
    pub blood_name: String,
    /// Percent of the normal volume.
    pub blood_levels: f32,
    pub blood_status: String,
    pub blood_alcohol: f32,
    /// (name, volume).
    pub chemical_list: Vec<(String, f32)>,
    pub overdose_list: Vec<String>,
    pub addict_list: Vec<String>,
    pub hallucinating_status: String,
    /// `active_status_1..4`: the paid scans.
    pub active: [bool; 4],
}

impl UiInterface for MedicalKioskUi {
    const NAME: &'static str = "MedicalKiosk";
    type Data = MedicalKioskData;
    type StaticData = ();
}

/// The four scans: (icon, name, description).
pub const SCANS: [(&str, &str, &str); 4] = [
    (
        "procedures",
        "General Health Scan",
        "Reads back exact values of your general health scan.",
    ),
    (
        "heartbeat",
        "Symptom Based Checkup",
        "Provides information based on various non-obvious symptoms, like blood levels or disease \
         status.",
    ),
    (
        "radiation-alt",
        "Neurological/Radiological Scan",
        "Provides information about brain trauma and radiation.",
    ),
    (
        "mortar-pestle",
        "Chemical and Psychoactive Scan",
        "Provides a list of consumed chemicals, as well as potential side effects.",
    ),
];

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::UiColor;
    use ss13_ui_kit::prelude::*;

    use super::{MedicalKioskData, MedicalKioskUi, SCANS};
    use crate::client::{ClientUi, SetLocal, UiLocalState};
    use crate::interfaces::chem_dispenser::js_num;

    /// `useSharedState('scanIndex')`.
    const LOCAL_SCAN: &str = "scanIndex";

    impl ClientUi for MedicalKioskUi {
        const WIDTH: f32 = 575.0;
        const HEIGHT: f32 = 420.0;

        fn content(_: &(), data: &MedicalKioskData) -> impl Scene {
            view(data, &UiLocalState::default())
        }

        fn content_local(_: &(), data: &MedicalKioskData, local: &UiLocalState) -> impl Scene {
            view(data, local)
        }
    }

    fn view(data: &MedicalKioskData, local: &UiLocalState) -> impl Scene + use<> {
        let shown: usize = local.get(LOCAL_SCAN).parse().unwrap_or(0);
        let buttons: Vec<_> = SCANS
            .iter()
            .enumerate()
            .map(|(i, &(icon, name, description))| {
                let index = i + 1;
                let paid = data.active[i];
                let (mark, mark_color) = if paid {
                    ("check", UiColor::Green)
                } else {
                    ("dollar-sign", UiColor::Grey)
                };
                let pick = SetLocal {
                    key: LOCAL_SCAN.into(),
                    value: index.to_string(),
                    rerender: true,
                };
                let selected = paid && shown == index;
                let action = format!("beginScan_{index}");
                bsn! {
                    Node { align_items: AlignItems::Center, margin: UiRect::bottom(px(2)) }
                    Children [
                        Node { width: px(16), margin: UiRect::right(px(4)) }
                        Children [ @Icon { @name: mark, @color: mark_color } ]
                        --
                        @Button {
                            @icon: icon,
                            @content: name,
                            @tooltip: description,
                            @fluid: true,
                        }
                        Button { selected: selected }
                        Node { flex_grow: 1.0 }
                        Act(action)
                        pick
                    ]
                }
            })
            .collect();
        let cost = format!(
            "Greetings Valued Employee! Please select a desired automatic health check \
             procedure. Diagnosis costs {} credits.",
            js_num(data.kiosk_cost)
        );
        let patient = data.patient_name.clone();
        let results: Box<dyn Scene> = match shown {
            1..=4 if data.active[shown - 1] => results(shown, data),
            _ => Box::new(bsn! { Node }),
        };
        bsn! {
            @WindowContent { @scrollable: true }
            Children [
                Node { margin: UiRect::bottom(px(6)) }
                Children [
                    Node { margin: UiRect::right(px(6)) }
                    Children [
                        @Section
                        Children [ Node { flex_direction: FlexDirection::Column } Children [ {buttons} ] ]
                    ]
                    --
                    Node { flex_grow: 1.0, flex_basis: px(0) }
                    Children [
                        @Section
                        Children [
                            Node { flex_direction: FlexDirection::Column }
                            Children [
                                Text(cost)
                                --
                                @LabeledList
                                Children [ @LabeledListItem { @label: "Patient", @content: patient } ]
                                --
                                @Button {
                                    @icon: "sync",
                                    @content: "Reset Scanner",
                                    @tooltip: "Resets the current scanning target, cancelling current scans.",
                                }
                                Button { color: UiColor::Average }
                                Act("clearTarget")
                            ]
                        ]
                    ]
                ]
                --
                {EntityScene(results)}
            ]
        }
    }

    fn bar_row(label: &'static str, value: f32, color: UiColor, text: String) -> impl Scene {
        let fill = value / 100.0;
        bsn! {
            @LabeledListItem { @label: label }
            Children [
                @ProgressBar { @empty: true }
                ProgressBar { value: fill, color: color }
                Node { flex_grow: 1.0, width: percent(100) }
                Children [ Text(text) ]
            ]
        }
    }

    fn line(label: String, text: String, color: UiColor) -> impl Scene {
        bsn! { @LabeledListItem { @label: label, @color: color, @content: text } }
    }

    fn results(index: usize, d: &MedicalKioskData) -> Box<dyn Scene> {
        match index {
            1 => {
                let rows = vec![
                    bar_row(
                        "Brute Damage",
                        d.brute_health,
                        UiColor::Bad,
                        js_num(d.brute_health),
                    ),
                    bar_row(
                        "Burn Damage",
                        d.burn_health,
                        UiColor::Bad,
                        js_num(d.burn_health),
                    ),
                    bar_row(
                        "Oxygen Damage",
                        d.suffocation_health,
                        UiColor::Bad,
                        js_num(d.suffocation_health),
                    ),
                    bar_row(
                        "Toxin Damage",
                        d.toxin_health,
                        UiColor::Bad,
                        js_num(d.toxin_health),
                    ),
                ];
                let total = bar_row(
                    "Total Health",
                    d.patient_health,
                    UiColor::Default,
                    format!("{}%", js_num(d.patient_health)),
                );
                Box::new(bsn! {
                    @Section { @title: "Patient Health" }
                    Children [ @LabeledList Children [ {EntityScene(total)} -- @LabeledListDivider -- {rows} ] ]
                })
            }
            2 => {
                let blood = bar_row(
                    "Levels",
                    d.blood_levels,
                    UiColor::Bad,
                    js_num(d.blood_levels),
                );
                let rows = vec![
                    line(
                        "Patient Status".into(),
                        d.patient_status.clone(),
                        UiColor::Good,
                    ),
                    line(
                        "Disease Status".into(),
                        d.patient_illness.clone(),
                        UiColor::Default,
                    ),
                    line(
                        "Disease information".into(),
                        d.illness_info.clone(),
                        UiColor::Default,
                    ),
                    line(
                        format!("{} Levels", d.blood_name),
                        format!("{}%", js_num(d.blood_levels)),
                        UiColor::Default,
                    ),
                    line("Bleeding".into(), d.bleed_status.clone(), UiColor::Label),
                    line(
                        format!("{} Information", d.blood_name),
                        d.blood_status.clone(),
                        UiColor::Default,
                    ),
                ];
                Box::new(bsn! {
                    @Section { @title: "Symptom Based Checkup" }
                    Children [ @LabeledList Children [ {EntityScene(blood)} -- {rows} ] ]
                })
            }
            3 => {
                let damage = bar_row(
                    "Brain Damage",
                    d.brain_damage,
                    UiColor::Good,
                    js_num(d.brain_damage),
                );
                let rows = vec![
                    line("Brain Status".into(), d.brain_health.clone(), UiColor::Bad),
                    line(
                        "Brain Trauma Status".into(),
                        d.trauma_status.clone(),
                        UiColor::Default,
                    ),
                ];
                Box::new(bsn! {
                    @Section { @title: "Patient Neurological Health" }
                    Children [ @LabeledList Children [ {EntityScene(damage)} -- {rows} ] ]
                })
            }
            _ => {
                let chems = if d.chemical_list.is_empty() {
                    "No reagents detected.".to_owned()
                } else {
                    d.chemical_list
                        .iter()
                        .map(|(name, v)| format!("{} units of {name}", js_num(*v)))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let overdoses = if d.overdose_list.is_empty() {
                    "Patient is not overdosing.".to_owned()
                } else {
                    d.overdose_list
                        .iter()
                        .map(|n| format!("Overdosing on {n}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let addictions = if d.addict_list.is_empty() {
                    "Patient has no addictions.".to_owned()
                } else {
                    d.addict_list
                        .iter()
                        .map(|n| format!("Addicted to {n}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let alcohol_color = if d.blood_alcohol < 0.23 {
                    UiColor::Blue
                } else {
                    UiColor::Bad
                };
                let alcohol_value = d.blood_alcohol / 0.3 * 100.0;
                let alcohol = bar_row(
                    "Blood Alcohol Content",
                    alcohol_value,
                    alcohol_color,
                    js_num(d.blood_alcohol),
                );
                let rows = vec![
                    line("Chemical Contents".into(), chems, UiColor::Good),
                    line("Overdose Status".into(), overdoses, UiColor::Bad),
                    line("Addiction Status".into(), addictions, UiColor::Bad),
                    line(
                        "Psychoactive Status".into(),
                        d.hallucinating_status.clone(),
                        UiColor::Default,
                    ),
                ];
                Box::new(bsn! {
                    @Section { @title: "Chemical and Psychoactive Analysis" }
                    Children [ @LabeledList Children [ {rows} -- {EntityScene(alcohol)} ] ]
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips() {
        let data = MedicalKioskData {
            kiosk_cost: 10.0,
            patient_name: "John".into(),
            patient_health: 80.0,
            brute_health: 20.0,
            chemical_list: vec![("Water".into(), 5.0)],
            overdose_list: vec!["Water".into()],
            active: [true, false, false, true],
            ..Default::default()
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<MedicalKioskData>(&bytes).unwrap(), data);
    }
}
