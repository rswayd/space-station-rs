//! Client half of the clothing: the crew monitor window (tgui `CrewConsole.tsx`).
// shortcut: no sort cycling, sort direction or name search; the server sends the list sorted by
// job (the window's default sort). No "Track" column (no AI link).

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;

use super::*;

pub struct ClothingClientPlugin;

impl Plugin for ClothingClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<CrewConsoleUi>();
    }
}

/// `HEALTH_COLOR_BY_LEVEL`.
const HEALTH_COLORS: [[u8; 3]; 6] = [
    [0x17, 0xd5, 0x68],
    [0xc4, 0xcf, 0x2d],
    [0xe6, 0x7e, 0x22],
    [0xed, 0x51, 0x00],
    [0xe7, 0x4c, 0x3c],
    [0x80, 0x13, 0x08],
];
/// `COLORS.damageType`: oxy, toxin, burn, brute.
const DAMAGE_COLORS: [[u8; 3]; 4] = [
    [0x34, 0x98, 0xdb],
    [0x2e, 0xcc, 0x71],
    [0xe6, 0x7e, 0x22],
    [0xe7, 0x4c, 0x3c],
];
const STAT_DEAD: u8 = 4;

/// `jobToColor` (`COLORS.department`).
fn job_color(ijob: u32) -> [u8; 3] {
    match ijob {
        0 => [0xc0, 0x66, 0x16],
        10..20 => [0xe7, 0x4c, 0x3c],
        20..30 => [0x34, 0x98, 0xdb],
        30..40 => [0x9b, 0x59, 0xb6],
        40..50 => [0xf1, 0xc4, 0x0f],
        50..60 => [0xf3, 0x9c, 0x12],
        60..200 => [0x7c, 0xc4, 0x6a],
        200..230 => [0x00, 0xc1, 0x00],
        _ => [0xc3, 0x83, 0x12],
    }
}

/// `healthToAttribute`: the color of the summed damage, one step per 25.
fn health_color(v: &CrewVitals) -> [u8; 3] {
    let sum = v.oxy + v.tox + v.burn + v.brute;
    let level = ((sum as f32 / 25.0).ceil() as i32).clamp(0, 5) as usize;
    HEALTH_COLORS[level]
}

/// `statToIcon`.
fn stat_icon(life_status: u8) -> &'static str {
    match life_status {
        0 => "heart",
        STAT_DEAD => "skull",
        _ => "heartbeat",
    }
}

fn rgb([r, g, b]: [u8; 3]) -> Color {
    Color::srgb_u8(r, g, b)
}

/// `HealthStat`: one damage number in its color.
fn damage_text(value: i32, color: [u8; 3]) -> impl Scene + use<> {
    let text = value.to_string();
    let color = rgb(color);
    bsn! { Text(text) TextColor(color) }
}

fn vitals_cell(sensor: &CrewSensor) -> Box<dyn Scene> {
    match sensor.vitals {
        Some(v) => {
            let [oxy, tox, burn, brute] = DAMAGE_COLORS;
            let oxy = EntityScene(damage_text(v.oxy, oxy));
            let tox = EntityScene(damage_text(v.tox, tox));
            let burn = EntityScene(damage_text(v.burn, burn));
            let brute = EntityScene(damage_text(v.brute, brute));
            Box::new(bsn! {
                Node { flex_direction: FlexDirection::Row }
                Children [
                    {oxy} -- Text("/") -- {tox} -- Text("/") -- {burn} -- Text("/") -- {brute}
                ]
            })
        }
        None => {
            let text = if sensor.life_status == STAT_DEAD {
                "Dead"
            } else {
                "Alive"
            };
            Box::new(bsn! { Text(text) })
        }
    }
}

fn row(sensor: &CrewSensor) -> impl Scene + use<> {
    let label = match &sensor.assignment {
        Some(a) => format!("{} ({a})", sensor.name),
        None => sensor.name.clone(),
    };
    let head = sensor.ijob % 10 == 0;
    let name_color = UiColor::Css(job_color(sensor.ijob));
    let (icon, icon_color) = match &sensor.vitals {
        Some(v) => (stat_icon(sensor.life_status), health_color(v)),
        None if sensor.life_status != STAT_DEAD => ("heart", HEALTH_COLORS[0]),
        None => ("skull", HEALTH_COLORS[5]),
    };
    let icon_color = UiColor::Css(icon_color);
    let vitals = vitals_cell(sensor);
    let area: Box<dyn Scene> = match sensor.area.clone().filter(|a| a != "~") {
        Some(area) => Box::new(bsn! { Text(area) }),
        None => Box::new(bsn! { @Icon { @name: "question" } }),
    };
    bsn! {
        @TableRow
        Children [
            @TableCell { @content: label, @color: name_color, @bold: head }
            --
            @TableCell { @collapsing: true }
            Children [ @Icon { @name: icon, @color: icon_color } ]
            --
            @TableCell { @collapsing: true }
            Children [ {EntityScene(vitals)} ]
            --
            @TableCell
            Children [ {EntityScene(area)} ]
        ]
    }
}

impl ClientUi for CrewConsoleUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 600.0;

    fn content(_: &(), data: &CrewConsoleData) -> impl Scene {
        let rows: Vec<_> = data.sensors.iter().map(row).collect();
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Crew Monitor", @scrollable: true }
                Children [
                    @Table
                    Children [
                        @TableRow
                        Children [
                            @TableCell { @header: true, @content: "Name" }
                            --
                            @TableCell { @header: true, @collapsing: true }
                            --
                            @TableCell { @header: true, @collapsing: true, @content: "Vitals" }
                            --
                            @TableCell { @header: true, @content: "Position" }
                        ]
                        --
                        {rows}
                    ]
                ]
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_follow_the_window() {
        assert_eq!(job_color(0), [0xc0, 0x66, 0x16]);
        assert_eq!(job_color(22), [0x34, 0x98, 0xdb]);
        assert_eq!(job_color(999), [0xc3, 0x83, 0x12]);
        let hurt = CrewVitals {
            brute: 30,
            ..default()
        };
        assert_eq!(health_color(&hurt), HEALTH_COLORS[2]);
        assert_eq!(health_color(&CrewVitals::default()), HEALTH_COLORS[0]);
        assert_eq!(stat_icon(1), "heartbeat");
    }
}
