//! Gas flow meters: tg `/obj/machinery/meter` (`machinery/other/meter.dm`). A meter reads the
//! pipeline of the pipe on its tile and layer every fire: a dial for the pressure, a color for
//! the temperature (its GAGS `meter` config, drawn here from the raw `meter.dmi` layers:
//! `meter_base`, a tinted `pressure*`, `buttons*`) and glowing buttons. Examining or clicking
//! it reads the numbers out.
// shortcut: no USB port. Wrenching off and on, and falling off with its pipe:
// `super::fitting`.

use bevy::prelude::*;
use ss13_atmos::gas::{ONE_ATMOSPHERE, T0C};
use ss13_atmos::pipenet::{DeviceId, Pipenet};
use ss13_mapload::{DmAtmosDevice, IconIndex};
use ss13_shared::{Appearance, EMISSIVE_PLANE, Overlay, TilePos};

use crate::obj_damage::is_under;
use crate::power::Unpowered;

pub const METER: &str = "/obj/machinery/meter";
const DMI: &str = "icons/obj/pipes_n_cables/meter.dmi";
const PIPING_LAYER_DEFAULT: i16 = 3;
/// `PIPING_LAYER_P_X`, `PIPING_LAYER_P_Y`.
const PIPING_LAYER_P: i16 = 5;

/// `BODYTEMP_*_WARNING_*`.
const HEAT_3: f64 = 340.15 + 360.0;
const HEAT_2: f64 = 340.15 + 120.0;
const HEAT_1: f64 = 340.15;
const COLD_1: f64 = 270.15;
const COLD_2: f64 = 270.15 - 70.0;
const COLD_3: f64 = 270.15 - 150.0;

/// One meter.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Meter {
    /// `target`: the pipe it reads.
    pub target: Option<DeviceId>,
    /// `meter_value`, `button_value` and the GAGS color last drawn.
    pub look: Option<MeterLook>,
}

/// What a meter shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeterLook {
    /// `meter_value`: `"0"`, `"1_3"`, `"4"`...; `None` while not operational (`meter`).
    pub value: Option<String>,
    pub button: u8,
    pub color: [u8; 3],
}

/// `process_atmos`: the dial and button for `pressure` kPa, the color for `temperature`.
pub fn meter_look(pressure: Option<f64>, temperature: f64) -> MeterLook {
    let Some(p) = pressure else {
        return MeterLook {
            value: Some("0".into()),
            button: 0,
            color: GRAY,
        };
    };
    let (value, button) = if p <= 0.15 * ONE_ATMOSPHERE {
        ("0".to_owned(), 0)
    } else if p <= 1.8 * ONE_ATMOSPHERE {
        let val = (p / (ONE_ATMOSPHERE * 0.3) + 0.5).floor();
        (format!("1_{val}"), 1)
    } else if p <= 30.0 * ONE_ATMOSPHERE {
        let val = (p / (ONE_ATMOSPHERE * 5.0) - 0.35).floor() + 1.0;
        (format!("2_{val}"), 2)
    } else if p <= 59.0 * ONE_ATMOSPHERE {
        let val = (p / (ONE_ATMOSPHERE * 5.0) - 6.0).floor() + 1.0;
        (format!("3_{val}"), 3)
    } else {
        ("4".to_owned(), 4)
    };
    let color = if p == 0.0 || temperature == 0.0 {
        GRAY
    } else if temperature >= HEAT_3 {
        [0xFF, 0, 0]
    } else if temperature >= HEAT_2 {
        [0xFF, 0x99, 0]
    } else if temperature >= HEAT_1 {
        [0xFF, 0xFF, 0]
    } else if temperature >= COLD_1 {
        [0, 0xFF, 0]
    } else if temperature >= COLD_2 {
        [0, 0xFF, 0xFF]
    } else if temperature >= COLD_3 {
        [0, 0, 0xFF]
    } else {
        [0xB9, 0, 0xF7]
    };
    MeterLook {
        value: Some(value),
        button,
        color,
    }
}

/// `COLOR_GRAY`.
const GRAY: [u8; 3] = [0x80; 3];

/// Meters from the map: each finds the pipe on its tile and `target_layer` (tg keeps the last
/// one) and shifts by its layer (`PIPING_LAYER_DOUBLE_SHIFT`).
pub fn init_meters(world: &mut World) {
    let mut query = world.query::<(Entity, &DmAtmosDevice, &TilePos)>();
    let found: Vec<(Entity, u8, TilePos)> = query
        .iter(world)
        .filter(|(_, dm, _)| is_under(&dm.path, METER))
        .map(|(e, dm, &pos)| {
            let layer = if dm.piping_layer > 0.0 {
                dm.piping_layer as u8
            } else {
                3
            };
            (e, layer, pos)
        })
        .collect();
    for (e, layer, pos) in found {
        let target = world.get_resource::<Pipenet>().and_then(|net| {
            net.devices_on(pos).iter().copied().rfind(|&id| {
                net.device(id)
                    .is_some_and(|d| d.is_pipe() && d.layer == layer)
            })
        });
        if target.is_some()
            && let Some(mut appearance) = world.get_mut::<Appearance>(e)
        {
            let shift = (i16::from(layer) - PIPING_LAYER_DEFAULT) * PIPING_LAYER_P;
            appearance.pixel_offset = [shift, shift];
        }
        world.entity_mut(e).insert(Meter { target, look: None });
    }
}

/// The pressure and temperature a meter reads (`target.return_air()`).
fn reading(net: &Pipenet, meter: &Meter) -> Option<(f64, f64)> {
    let air = net.pipeline_air(meter.target?, 0)?;
    Some((air.pressure(), air.temperature()))
}

/// Every fire: a changed reading redraws the meter.
pub fn process_meters(
    net: Res<Pipenet>,
    icons: Option<Res<IconIndex>>,
    mut meters: Query<(&mut Meter, &mut Appearance, Has<Unpowered>)>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (mut meter, mut appearance, unpowered) in &mut meters {
        let look = if unpowered {
            // `on_set_is_operational`: plain `meter`, no glow.
            MeterLook {
                value: None,
                button: 0,
                color: GRAY,
            }
        } else {
            match reading(&net, &meter) {
                Some((p, t)) => meter_look(Some(p), t),
                None => meter_look(None, 0.0),
            }
        };
        if meter.look.as_ref() == Some(&look) {
            continue;
        }
        draw(&icons, &mut appearance, &look);
        meter.look = Some(look);
    }
}

/// The GAGS layers and `update_overlays` emissives of a look.
fn draw(icons: &IconIndex, appearance: &mut Appearance, look: &MeterLook) {
    let Some(base) = icons.get(DMI, "meter_base") else {
        return;
    };
    appearance.icon_state = base;
    appearance.overlays.clear();
    let [r, g, b] = look.color;
    let pressure = look
        .value
        .as_ref()
        .map_or_else(|| "pressure_off".to_owned(), |v| format!("pressure{v}"));
    if let Some(id) = icons.get(DMI, &pressure) {
        let color = look.value.is_some().then_some([r, g, b, 255]);
        appearance.overlays.push(Overlay {
            color,
            ..Overlay::new(id)
        });
    }
    let Some(value) = &look.value else {
        return;
    };
    if let Some(id) = icons.get(DMI, &format!("buttons{}", look.button)) {
        appearance.overlays.push(Overlay::new(id));
    }
    let mut emissive = vec![format!("buttons{}-emissive", look.button)];
    if value != "0" {
        emissive.push(format!("pressure{value}"));
    }
    for state in emissive {
        if let Some(id) = icons.get(DMI, &state) {
            appearance.overlays.push(Overlay {
                plane: Some(EMISSIVE_PLANE),
                ..Overlay::new(id)
            });
        }
    }
}

/// `status()`: what examining or clicking a meter says.
pub fn status(world: &World, meter: Entity) -> Option<String> {
    let meter = world.get::<Meter>(meter)?;
    let net = world.get_resource::<Pipenet>()?;
    Some(match (meter.target, reading(net, meter)) {
        (None, _) => "Мигает индикатор ошибки подключения.".to_owned(),
        (Some(_), None) => "Мигает индикатор ошибки датчика.".to_owned(),
        (Some(_), Some((p, t))) => format!(
            "Манометр показывает {} кПа; {} K ({} °C).",
            round2(p),
            round2(t),
            round2(t - T0C)
        ),
    })
}

/// BYOND `round(x, 0.01)` as text.
fn round2(x: f64) -> String {
    ss13_atmos::scan::num(ss13_atmos::gas::byond_round(x, 0.01))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// tg's dial steps and temperature colors.
    #[test]
    fn readings_follow_tg() {
        let at = |p: f64, t: f64| meter_look(Some(p), t);
        assert_eq!(at(10.0, 293.0).value.as_deref(), Some("0"));
        // 101.325 kPa: round(1/0.3 + 0.5) = round(3.83) = 3.
        let room = at(ONE_ATMOSPHERE, 293.15);
        assert_eq!((room.value.as_deref(), room.button), (Some("1_3"), 1));
        assert_eq!(room.color, [0, 0xFF, 0]);
        // 10 atm: round(2 - 0.35) + 1 = 2.
        assert_eq!(
            at(10.0 * ONE_ATMOSPHERE, 500.0).value.as_deref(),
            Some("2_2")
        );
        // 500 K is between HEAT_WARNING_2 (460) and _3 (700): orange.
        assert_eq!(at(10.0 * ONE_ATMOSPHERE, 500.0).color, [0xFF, 0x99, 0]);
        assert_eq!(at(10.0 * ONE_ATMOSPHERE, 400.0).color, [0xFF, 0xFF, 0]);
        assert_eq!(
            at(40.0 * ONE_ATMOSPHERE, 100.0).value.as_deref(),
            Some("3_3")
        );
        assert_eq!(at(40.0 * ONE_ATMOSPHERE, 100.0).color, [0xB9, 0, 0xF7]);
        assert_eq!(
            at(100.0 * ONE_ATMOSPHERE, 1000.0).value.as_deref(),
            Some("4")
        );
        assert_eq!(meter_look(None, 0.0).color, GRAY);
    }
}
