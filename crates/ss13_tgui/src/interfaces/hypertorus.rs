//! `Hypertorus`: `tgui/packages/tgui/interfaces/Hypertorus/*` and
//! `/obj/machinery/hypertorus/interface/ui_static_data`, `ui_data`, `ui_act`
//! (`code/modules/atmospherics/machinery/components/fusion/hfr_parts.dm`). The server half lives
//! with the game (`ss13_game::atmos_machinery::hfr`).
//!
//! Acts: `start_power`, `start_cooling`, `start_fuel`, `start_moderator`, `waste_remove`;
//! `heating_conductor`, `magnetic_constrictor`, `fuel_injection_rate`,
//! `moderator_injection_rate`, `current_damper`, `mod_filtering_rate`, `cooling_volume` with
//! the number under the same key; `filter` and `fuel` with `mode` (a gas id, a fuel id or `""`).
// shortcut: one plain column of sections; tg's knobs, round gauges and temperature bars are
// number inputs, progress bars and text here, the recipe table a list of buttons.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct HypertorusUi;

/// One `selectable_fuel` entry (`id` empty: "Nothing").
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HfrFuelEntry {
    pub name: String,
    pub id: String,
    pub requirements: Vec<String>,
    pub fusion_byproducts: Vec<String>,
    pub product_gases: Vec<String>,
    pub recipe_cooling_multiplier: f32,
    pub recipe_heating_multiplier: f32,
    pub energy_loss_multiplier: f32,
    pub fuel_consumption_multiplier: f32,
    pub gas_production_multiplier: f32,
    pub temperature_multiplier: f32,
}

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HypertorusStatic {
    pub base_max_temperature: f32,
    pub selectable_fuel: Vec<HfrFuelEntry>,
}

/// A gas of the fusion or moderator mix; `name` added for the client.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HfrGas {
    pub id: String,
    pub name: String,
    pub amount: f32,
}

/// One `filter_types` entry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HfrFilter {
    pub gas_id: String,
    pub gas_name: String,
    pub enabled: bool,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HypertorusData {
    /// The fuel id, `""` for none.
    pub selected: String,
    pub fusion_gases: Vec<HfrGas>,
    pub moderator_gases: Vec<HfrGas>,
    pub energy_level: f64,
    pub heat_limiter_modifier: f64,
    pub heat_output_min: f64,
    pub heat_output_max: f64,
    pub heat_output: f64,
    pub instability: f32,
    pub heating_conductor: f32,
    pub magnetic_constrictor: f32,
    pub fuel_injection_rate: f32,
    pub moderator_injection_rate: f32,
    pub current_damper: f32,
    pub power_level: u8,
    pub apc_energy: f32,
    pub iron_content: f32,
    pub integrity: f32,
    pub start_power: bool,
    pub start_cooling: bool,
    pub start_fuel: bool,
    pub start_moderator: bool,
    pub internal_fusion_temperature: f64,
    pub moderator_internal_temperature: f64,
    pub internal_output_temperature: f64,
    pub internal_coolant_temperature: f64,
    pub waste_remove: bool,
    pub filter_types: Vec<HfrFilter>,
    pub cooling_volume: f32,
    pub mod_filtering_rate: f32,
}

impl UiInterface for HypertorusUi {
    const NAME: &'static str = "Hypertorus";
    type Data = HypertorusData;
    type StaticData = HypertorusStatic;
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::ActParams;
    use ss13_ui_kit::prelude::*;

    use super::{HfrGas, HypertorusData, HypertorusStatic, HypertorusUi};
    use crate::client::{ActOnChange, ClientUi};

    fn param(key: &str, value: &str) -> ActParams {
        ActParams(vec![(key.to_owned(), value.to_owned())])
    }

    fn toggle(action: &'static str, on: bool, disabled: bool) -> Box<dyn Scene> {
        let (icon, text) = if on {
            ("power-off", "Вкл")
        } else {
            ("times", "Выкл")
        };
        Box::new(bsn! {
            @Button { @icon: icon, @content: text }
            Button { selected: on, disabled: disabled }
            Act({action.to_owned()})
        })
    }

    fn number(
        action: &'static str,
        value: f32,
        min: f64,
        max: f64,
        step: f64,
        unit: &str,
    ) -> Box<dyn Scene> {
        let input = NumberInput {
            value: f64::from(value),
            min_value: min,
            max_value: max,
            step,
            unit: unit.into(),
            tick_while_dragging: true,
            ..Default::default()
        };
        let on_change = ActOnChange {
            action: action.into(),
            param: action.into(),
        };
        Box::new(bsn! {
            Node { width: px(110) }
            input
            on_change
        })
    }

    fn gas_rows(gases: &[HfrGas]) -> Vec<Box<dyn Scene>> {
        let mut gases: Vec<&HfrGas> = gases.iter().filter(|g| g.amount > 0.0).collect();
        gases.sort_by(|a, b| b.amount.total_cmp(&a.amount));
        gases
            .into_iter()
            .map(|g| {
                let (label, text) = (format!("{}:", g.name), format!("{:.2} moles", g.amount));
                Box::new(bsn! {
                    @LabeledListItem { @label: label }
                    Children [ Text(text) ]
                }) as Box<dyn Scene>
            })
            .collect()
    }

    fn labeled(label: &str, text: String) -> Box<dyn Scene> {
        let label = label.to_owned();
        Box::new(bsn! {
            @LabeledListItem { @label: label }
            Children [ Text(text) ]
        })
    }

    impl ClientUi for HypertorusUi {
        const WIDTH: f32 = 600.0;
        const HEIGHT: f32 = 900.0;

        fn content(statics: &HypertorusStatic, data: &HypertorusData) -> impl Scene {
            let level = data.power_level;
            let power = toggle("start_power", data.start_power, level > 0);
            let cooling = toggle(
                "start_cooling",
                data.start_cooling,
                data.start_fuel
                    || data.start_moderator
                    || !data.start_power
                    || (data.start_cooling && level > 0),
            );
            let recipes: Vec<Box<dyn Scene>> = statics
                .selectable_fuel
                .iter()
                .map(|f| {
                    let (name, selected) = (f.name.clone(), f.id == data.selected);
                    let disabled = level != 0;
                    let mode = param("mode", &f.id);
                    Box::new(bsn! {
                        @Button { @content: name }
                        Button { selected: selected, disabled: disabled }
                        Act("fuel")
                        mode
                    }) as Box<dyn Scene>
                })
                .collect();
            let inputs_disabled = !data.start_power || !data.start_cooling;
            let fuel_toggle = toggle("start_fuel", data.start_fuel, inputs_disabled);
            let fuel_rate = number(
                "fuel_injection_rate",
                data.fuel_injection_rate,
                0.5,
                150.0,
                1.0,
                "mol/s",
            );
            let fusion = gas_rows(&data.fusion_gases);
            let moderator_toggle = toggle("start_moderator", data.start_moderator, inputs_disabled);
            let moderator_rate = number(
                "moderator_injection_rate",
                data.moderator_injection_rate,
                0.5,
                150.0,
                1.0,
                "mol/s",
            );
            let moderator = gas_rows(&data.moderator_gases);
            let no_recipe = data.selected.is_empty().then(|| {
                EntityScene(bsn! {
                    @UiBox { @color: UiColor::Bad }
                    Children [ Text("Рецепт не выбран") ]
                })
            });
            let temperatures = vec![
                labeled(
                    "Синтез",
                    format!("{:.0} K", data.internal_fusion_temperature),
                ),
                labeled(
                    "Модератор",
                    format!("{:.0} K", data.moderator_internal_temperature),
                ),
                labeled(
                    "Хладагент",
                    format!("{:.0} K", data.internal_coolant_temperature),
                ),
                labeled(
                    "Вывод",
                    format!("{:.0} K", data.internal_output_temperature),
                ),
            ];
            let activity = {
                let max = if data.heat_output < 0.0 {
                    data.heat_output_min
                } else {
                    data.heat_output_max
                };
                let a = data.heat_output / max;
                if a.is_finite() && data.start_power {
                    a * 100.0
                } else {
                    0.0
                }
            };
            let instability = if data.start_power {
                f64::from(data.instability.max(0.0)) / 8.0 * 100.0
            } else {
                0.0
            };
            let integrity_ranges = vec![
                (UiColor::Good, 90.0, 100.0),
                (UiColor::Average, 50.0, 90.0),
                (UiColor::Bad, 0.0, 50.0),
            ];
            let (integrity, min, max) = (data.integrity, 0.0_f32, 100.0_f32);
            let parameters = vec![
                labeled(
                    "Содержание железа",
                    format!("{:.1}%", data.iron_content * 100.0),
                ),
                labeled("Питание зоны", format!("{:.0}%", data.apc_energy)),
                labeled("Уровень синтеза", level.to_string()),
                labeled("Тепловыделение", format!("{:.3e} J", data.energy_level)),
                labeled("Активность реакции", format!("{activity:.1}%")),
                labeled("Нестабильность", format!("{instability:.1}%")),
            ];
            let controls = vec![
                (
                    "Нагревательный проводник",
                    number(
                        "heating_conductor",
                        data.heating_conductor,
                        50.0,
                        500.0,
                        5.0,
                        "J/cm",
                    ),
                ),
                (
                    "Объем охлаждения",
                    number(
                        "cooling_volume",
                        data.cooling_volume,
                        50.0,
                        2000.0,
                        25.0,
                        "L",
                    ),
                ),
                (
                    "Магнитный ограничитель",
                    number(
                        "magnetic_constrictor",
                        data.magnetic_constrictor,
                        50.0,
                        1000.0,
                        5.0,
                        "m³/T",
                    ),
                ),
                (
                    "Текущий демпфер",
                    number("current_damper", data.current_damper, 0.0, 1000.0, 5.0, "W"),
                ),
            ];
            let controls: Vec<Box<dyn Scene>> = controls
                .into_iter()
                .map(|(label, input)| {
                    let label = label.to_owned();
                    Box::new(bsn! {
                        @LabeledListItem { @label: label }
                        Children [ {EntityScene(input)} ]
                    }) as Box<dyn Scene>
                })
                .collect();
            let waste = toggle("waste_remove", data.waste_remove, false);
            let filtering = number(
                "mod_filtering_rate",
                data.mod_filtering_rate,
                5.0,
                200.0,
                1.0,
                "mol/s",
            );
            let filters: Vec<Box<dyn Scene>> = data
                .filter_types
                .iter()
                .map(|f| {
                    let (name, on) = (f.gas_name.clone(), f.enabled);
                    let icon = if on { "check-square-o" } else { "square-o" };
                    let mode = param("mode", &f.gas_id);
                    Box::new(bsn! {
                        @Button { @icon: icon, @content: name }
                        Button { selected: on }
                        Act("filter")
                        mode
                    }) as Box<dyn Scene>
                })
                .collect();
            bsn! {
                @WindowContent
                Children [
                    @Section
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Питание" }
                            Children [ {EntityScene(power)} ]
                            --
                            @LabeledListItem { @label: "Запуск охлаждения" }
                            Children [ {EntityScene(cooling)} ]
                        ]
                        --
                        @Section { @title: "Выбор рецепта" }
                        Children [ {recipes} ]
                    ]
                    --
                    @Section { @title: "Газы внутреннего синтеза" }
                    Children [
                        {no_recipe}
                        --
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Управление впрыском:" }
                            Children [ {EntityScene(fuel_toggle)} -- {EntityScene(fuel_rate)} ]
                            --
                            {fusion}
                        ]
                    ]
                    --
                    @Section { @title: "Газы модератора" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Управление впрыском:" }
                            Children [
                                {EntityScene(moderator_toggle)} -- {EntityScene(moderator_rate)}
                            ]
                            --
                            {moderator}
                        ]
                    ]
                    --
                    @Section { @title: "Контроль газов" }
                    Children [ @LabeledList Children [ {temperatures} ] ]
                    --
                    @Section { @title: "Состояние реактора" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Целостность реактора" }
                            Children [
                                @ProgressBar
                                ProgressBar {
                                    value: integrity,
                                    min_value: min,
                                    max_value: max,
                                    ranges: integrity_ranges,
                                }
                                Node { flex_grow: 1.0, width: percent(100) }
                            ]
                            --
                            {parameters}
                        ]
                    ]
                    --
                    @Section { @title: "Управление реактором" }
                    Children [ @LabeledList Children [ {controls} ] ]
                    --
                    @Section { @title: "Управление выводом" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Удаление отходов:" }
                            Children [ {EntityScene(waste)} ]
                            --
                            @LabeledListItem { @label: "Скорость фильтрации модератора:" }
                            Children [ {EntityScene(filtering)} ]
                            --
                            @LabeledListItem { @label: "Фильтр из смеси модератора:" }
                            Children [ {filters} ]
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
    fn data_round_trips() {
        let data = HypertorusData {
            selected: "h2_t2_fuel".into(),
            fusion_gases: vec![HfrGas {
                id: "hydrogen".into(),
                name: "Hydrogen".into(),
                amount: 512.25,
            }],
            energy_level: 4.2e21,
            power_level: 3,
            start_power: true,
            filter_types: vec![HfrFilter {
                gas_id: "helium".into(),
                gas_name: "Helium".into(),
                enabled: true,
            }],
            ..Default::default()
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<HypertorusData>(&bytes).unwrap(), data);
    }
}
