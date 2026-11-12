//! `Supermatter`: `tgui/packages/tgui/interfaces/Supermatter.tsx` and
//! `/obj/machinery/power/supermatter_crystal/sm_ui_data` (`code/modules/power/supermatter/
//! supermatter.dm`). The server half lives with the game (`ss13_game::power::supermatter_ui`).
//! [`SmEntry`] is one crystal's data, for the NTOS monitor program as well.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct SupermatterUi;

/// One line of a `*_factors` list: what a factor added (in the unit of its list).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SmFactor {
    pub name: String,
    pub amount: f32,
}

/// One gas of the absorbed mix.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SmGasShare {
    pub name: String,
    /// `/datum/gas::primary_color`.
    pub color: [u8; 3],
    /// 0..1.
    pub amount: f32,
}

/// `sm_ui_data()`. tg formats the SI units on the server; here the client does.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SmEntry {
    pub uid: u32,
    pub area_name: String,
    /// Percent.
    pub integrity: f32,
    /// Percent of integrity this breath (negative: lost).
    pub integrity_factors: Vec<SmFactor>,
    /// MeV.
    pub internal_energy: f32,
    /// MeV.
    pub internal_energy_factors: Vec<SmFactor>,
    /// Watts.
    pub zap_transmission: f32,
    /// Watts.
    pub zap_transmission_factors: Vec<SmFactor>,
    /// Kelvin.
    pub temp_limit: f32,
    pub temp_limit_factors: Vec<SmFactor>,
    pub waste_multiplier: f32,
    pub waste_multiplier_factors: Vec<SmFactor>,
    pub absorbed_ratio: f32,
    /// Every gas, in `/datum/gas` order.
    pub gas_composition: Vec<SmGasShare>,
    pub gas_temperature: f32,
    pub gas_total_moles: f32,
}

impl UiInterface for SupermatterUi {
    const NAME: &'static str = "Supermatter";
    type Data = SmEntry;
    type StaticData = ();
}

/// `logScale`.
pub fn log_scale(value: f32) -> f32 {
    (16.0 + value.max(0.0)).log2() - 4.0
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{SmEntry, SmFactor, SupermatterUi, log_scale};
    use crate::client::ClientUi;
    use crate::interfaces::solar_control::format_si_unit;

    /// A factor list under its entry (tg's chevron detail, always open here).
    fn factors(
        list: &[SmFactor],
        show: impl Fn(f32) -> String,
        good_up: bool,
        delta: bool,
    ) -> Vec<impl Scene> {
        list.iter()
            .map(|f| {
                let label = if delta {
                    format!("{} (∆)", f.name)
                } else {
                    f.name.clone()
                };
                let good = if good_up {
                    f.amount > 0.0
                } else {
                    f.amount < 0.0
                };
                let color = if good { UiColor::Good } else { UiColor::Bad };
                let text = show(f.amount);
                bsn! {
                    @LabeledListItem { @label: label }
                    Children [ Text(text) TextColor({color.base()}) ]
                }
            })
            .collect()
    }

    fn bar(
        value: f32,
        min: f32,
        max: f32,
        ranges: Vec<(UiColor, f32, f32)>,
        text: String,
    ) -> impl Scene {
        bsn! {
            @ProgressBar { @empty: true }
            ProgressBar { value: value, min_value: min, max_value: max, ranges: ranges }
            Node { flex_grow: 1.0, width: percent(100) }
            Children [ Text(text) ]
        }
    }

    impl ClientUi for SupermatterUi {
        const WIDTH: f32 = 700.0;
        const HEIGHT: f32 = 400.0;

        fn content(_: &(), d: &SmEntry) -> impl Scene {
            let title = format!("{}. {}", d.uid, d.area_name);
            let inf = f32::INFINITY;
            let ninf = f32::NEG_INFINITY;
            let integrity = EntityScene(bar(
                d.integrity / 100.0,
                0.0,
                1.0,
                vec![
                    (UiColor::Good, 0.9, inf),
                    (UiColor::Average, 0.5, 0.9),
                    (UiColor::Bad, ninf, 0.5),
                ],
                format!("{:.2} %", d.integrity),
            ));
            let integrity_factors =
                factors(&d.integrity_factors, |a| format!("{a:.2} %"), true, true);
            let energy = EntityScene(bar(
                d.internal_energy,
                0.0,
                5000.0,
                vec![
                    (UiColor::Good, ninf, 5000.0),
                    (UiColor::Average, 5000.0, 7000.0),
                    (UiColor::Bad, 7000.0, inf),
                ],
                format_si_unit(d.internal_energy * 1e6, "eV"),
            ));
            let energy_factors = factors(
                &d.internal_energy_factors,
                |a| format_si_unit(a * 1e6, "eV"),
                true,
                true,
            );
            let zap = EntityScene(bar(
                d.zap_transmission,
                0.0,
                1e7,
                vec![
                    (UiColor::Teal, 1e7, inf),
                    (UiColor::Good, 2e6, 1e7),
                    (UiColor::Average, 1e6, 2e6),
                    (UiColor::Bad, ninf, 1e6),
                ],
                format_si_unit(d.zap_transmission, "W"),
            ));
            let zap_factors = factors(
                &d.zap_transmission_factors,
                |a| format_si_unit(a, "W"),
                true,
                false,
            );
            let moles = EntityScene(bar(
                d.gas_total_moles,
                0.0,
                2000.0,
                vec![
                    (UiColor::Good, 0.0, 900.0),
                    (UiColor::Average, 900.0, 1800.0),
                    (UiColor::Bad, 1800.0, inf),
                ],
                format!("{:.2} Moles", d.gas_total_moles),
            ));
            let temperature = EntityScene(bar(
                log_scale(d.gas_temperature),
                0.0,
                log_scale(10000.0),
                vec![
                    (UiColor::Teal, ninf, log_scale(100.0)),
                    (UiColor::Good, log_scale(100.0), log_scale(300.0)),
                    (UiColor::Average, log_scale(300.0), log_scale(d.temp_limit)),
                    (UiColor::Bad, log_scale(d.temp_limit), inf),
                ],
                format!("{:.2} K", d.gas_temperature),
            ));
            let temp_limit = format!("{} K", d.temp_limit);
            let temp_factors = factors(&d.temp_limit_factors, |a| format!("{a:.2} K"), true, false);
            let waste = EntityScene(bar(
                d.waste_multiplier,
                0.0,
                20.0,
                vec![
                    (UiColor::Good, ninf, 0.8),
                    (UiColor::Average, 0.8, 2.0),
                    (UiColor::Bad, 2.0, inf),
                ],
                format!("{:.2} x", d.waste_multiplier),
            ));
            let waste_factors = factors(
                &d.waste_multiplier_factors,
                |a| format!("{a:.2} x"),
                false,
                false,
            );
            let absorbed = format!("{}%", d.absorbed_ratio * 100.0);
            let mut gases: Vec<_> = d
                .gas_composition
                .iter()
                .filter(|g| g.amount != 0.0)
                .collect();
            gases.sort_by(|a, b| b.amount.total_cmp(&a.amount));
            let gases: Vec<_> = gases
                .into_iter()
                .map(|g| {
                    let label = g.name.clone();
                    let color = UiColor::Css(g.color);
                    let value = g.amount;
                    let text = format!("{:.2}%", g.amount * 100.0);
                    bsn! {
                        @LabeledListItem { @label: label }
                        Children [
                            @ProgressBar { @empty: true }
                            ProgressBar { value: value, color: color }
                            Node { flex_grow: 1.0, width: percent(100) }
                            Children [ Text(text) ]
                        ]
                    }
                })
                .collect();
            bsn! {
                @WindowContent { @scrollable: true }
                Children [
                    @Section { @title: title }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Целостность" }
                            Children [ {integrity} ]
                            --
                            {integrity_factors}
                            --
                            @LabeledListItem { @label: "Внутренняя энергия" }
                            Children [ {energy} ]
                            --
                            {energy_factors}
                            --
                            @LabeledListItem { @label: "Импульсная передача энергии" }
                            Children [ {zap} ]
                            --
                            {zap_factors}
                            --
                            @LabeledListItem { @label: "Поглощённые моли" }
                            Children [ {moles} ]
                            --
                            @LabeledListItem { @label: "Температура" }
                            Children [ {temperature} ]
                            --
                            @LabeledListItem { @label: "Температурный предел" }
                            Children [ Text(temp_limit) ]
                            --
                            {temp_factors}
                            --
                            @LabeledListItem { @label: "Множитель отходов" }
                            Children [ {waste} ]
                            --
                            {waste_factors}
                            --
                            @LabeledListItem { @label: "Коэффициент поглощения" }
                            Children [ Text(absorbed) ]
                        ]
                    ]
                    --
                    @Section { @title: "Газы" }
                    Children [
                        @LabeledList
                        Children [ {gases} ]
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
    fn data_round_trips() {
        let data = SmEntry {
            uid: 1,
            area_name: "Supermatter Engine".into(),
            integrity: 99.5,
            integrity_factors: vec![SmFactor {
                name: "Термическое повреждение".into(),
                amount: -0.5,
            }],
            internal_energy: 1200.0,
            gas_composition: vec![SmGasShare {
                name: "Nitrogen".into(),
                color: [255, 255, 0],
                amount: 1.0,
            }],
            ..Default::default()
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<SmEntry>(&bytes).unwrap(), data);
        assert!((log_scale(0.0)).abs() < 1e-6);
    }
}
