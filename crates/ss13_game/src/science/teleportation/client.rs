//! The teleportation windows (client): tgui `Teleporter.jsx`, `BluespaceLocator.tsx`,
//! `LaunchpadConsole.tsx`, `SatelliteControl.tsx` and `Gateway.jsx`. The data is in
//! [`super`], the server halves in the sibling modules.
// shortcut: the locator is a plain list (tgui draws a compass and a search field); the launchpad
// pads are buttons instead of vertical tabs, the diagonal arrows are not rotated and the name
// applies on Enter (tgui: on blur); the
// gateway has no portal view (`ByondUi`).

use bevy::scene::EntityScene;
use ss13_tgui::client::{ActOnChange, ActOnSubmit, AppTguiClientExt, ClientUi};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;

use super::*;

pub struct TeleportationClientPlugin;

impl Plugin for TeleportationClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<TeleporterUi>()
            .register_ui_view::<BluespaceLocatorUi>()
            .register_ui_view::<LaunchpadConsoleUi>()
            .register_ui_view::<SatelliteControlUi>()
            .register_ui_view::<GatewayUi>();
    }
}

fn text(t: impl Into<String>) -> Box<dyn Scene> {
    let t: String = t.into();
    Box::new(bsn! { Text(t) })
}

fn colored(color: UiColor, t: impl Into<String>) -> Box<dyn Scene> {
    let t: String = t.into();
    Box::new(bsn! {
        @UiBox { @color: color }
        Children [ Text(t) ]
    })
}

fn notice(t: &str) -> Box<dyn Scene> {
    let t = t.to_owned();
    Box::new(bsn! {
        @NoticeBox
        Children [ Text(t) ]
    })
}

fn button(icon: &str, label: &str, action: &str, params: Vec<(String, String)>) -> Box<dyn Scene> {
    let (icon, label, action) = (icon.to_owned(), label.to_owned(), action.to_owned());
    let params = ActParams(params);
    Box::new(bsn! {
        @Button { @icon: icon, @content: label }
        Act(action)
        params
    })
}

fn toggle_button(
    icon: &str,
    label: String,
    action: &str,
    params: Vec<(String, String)>,
    selected: bool,
) -> Box<dyn Scene> {
    let (icon, action) = (icon.to_owned(), action.to_owned());
    let params = ActParams(params);
    Box::new(bsn! {
        @Button { @icon: icon, @content: label }
        Button { selected: selected }
        Act(action)
        params
    })
}

fn row(label: &str, content: Box<dyn Scene>) -> Box<dyn Scene> {
    let label = label.to_owned();
    let content = EntityScene(content);
    Box::new(bsn! {
        @LabeledListItem { @label: label }
        Children [ {content} ]
    })
}

fn section(title: String, children: Vec<Box<dyn Scene>>) -> Box<dyn Scene> {
    Box::new(bsn! {
        @Section { @title: title }
        Children [ {children} ]
    })
}

impl ClientUi for TeleporterUi {
    const WIDTH: f32 = 360.0;
    const HEIGHT: f32 = 130.0;

    fn content(_: &(), data: &TeleporterData) -> impl Scene {
        teleporter(data)
    }
}

fn teleporter(data: &TeleporterData) -> impl Scene + use<> {
    let body: Box<dyn Scene> = if !data.power_station {
        colored(UiColor::Bad, "No power station linked.")
    } else if !data.teleporter_hub {
        colored(UiColor::Bad, "No hub linked.")
    } else {
        let (color, calibration) = if data.calibrating {
            (UiColor::Average, "In Progress")
        } else if data.calibrated {
            (UiColor::Good, "Optimal")
        } else {
            (UiColor::Bad, "Sub-Optimal")
        };
        let rows = vec![
            row(
                "Regime",
                button("", &data.regime_set, ACT_REGIME, Vec::new()),
            ),
            row(
                "Target",
                button("edit", &data.target, ACT_TARGET, Vec::new()),
            ),
            row("Calibration", colored(color, calibration)),
            row("", button("tools", "Calibrate", ACT_CALIBRATE, Vec::new())),
        ];
        Box::new(bsn! {
            @LabeledList
            Children [ {rows} ]
        })
    };
    let body = EntityScene(body);
    bsn! {
        @WindowContent
        Children [
            @Section Children [ {body} ]
        ]
    }
}

impl ClientUi for BluespaceLocatorUi {
    const WIDTH: f32 = 300.0;
    const HEIGHT: f32 = 400.0;

    fn content(_: &(), data: &LocatorData) -> impl Scene {
        locator(data)
    }
}

fn locator(data: &LocatorData) -> impl Scene + use<> {
    let range = data.trackingrange.max(1) as u32;
    let rows: Vec<Box<dyn Scene>> = if data.trackimplants.is_empty() {
        vec![text("No implants detected.")]
    } else {
        let items: Vec<Box<dyn Scene>> = data
            .trackimplants
            .iter()
            .map(|t| {
                // tgui `getArrowColor`: red beyond 2/3 of the range, orange beyond 1/3.
                let color = if t.distance == 0 || t.distance * 3 <= range {
                    UiColor::Good
                } else if t.distance * 3 <= range * 2 {
                    UiColor::Average
                } else {
                    UiColor::Bad
                };
                let where_ = if t.direction.is_empty() {
                    format!("{} m", t.distance)
                } else {
                    format!("{} m, {}", t.distance, t.direction)
                };
                row(&t.name, colored(color, where_))
            })
            .collect();
        let list: Box<dyn Scene> = Box::new(bsn! { @LabeledList Children [ {items} ] });
        vec![list]
    };
    let list = section(format!("Tracking range: {}", data.trackingrange), rows);
    let list = EntityScene(list);
    bsn! {
        @WindowContent
        Children [ {list} ]
    }
}

impl ClientUi for LaunchpadConsoleUi {
    const WIDTH: f32 = 475.0;
    const HEIGHT: f32 = 275.0;

    fn content(_: &(), data: &LaunchpadConsoleData) -> impl Scene {
        launchpad(data)
    }
}

/// `buttonConfigs`: columns of (icon, x, y); the middle is "R" (reset).
const MOVES: [[(&str, i32, i32); 3]; 3] = [
    [
        ("arrow-left", -1, 1),
        ("arrow-left", -1, 0),
        ("arrow-left", -1, -1),
    ],
    [("arrow-up", 0, 1), ("", 0, 0), ("arrow-down", 0, -1)],
    [
        ("arrow-right", 1, 1),
        ("arrow-right", 1, 0),
        ("arrow-right", 1, -1),
    ],
];

fn launchpad(data: &LaunchpadConsoleData) -> impl Scene + use<> {
    let body: Box<dyn Scene> = if data.launchpads.is_empty() {
        notice("No Pads Connected")
    } else {
        let tabs: Vec<Box<dyn Scene>> = data
            .launchpads
            .iter()
            .map(|p| {
                toggle_button(
                    "",
                    p.name.clone(),
                    ACT_SELECT_PAD,
                    vec![("id".to_owned(), p.id.to_string())],
                    data.selected_id == Some(p.id),
                )
            })
            .collect();
        let control: Box<dyn Scene> = match &data.selected {
            None => text("Please select a pad"),
            Some(pad) => pad_control(pad),
        };
        let tabs = section(String::new(), tabs);
        let (tabs, control) = (EntityScene(tabs), EntityScene(control));
        Box::new(bsn! {
            @Stack { @fill: true }
            Children [
                @StackItem { @grow: 1.0 } Children [ {tabs} ]
                --
                @StackItem { @grow: 3.0 } Children [ {control} ]
            ]
        })
    };
    let body = EntityScene(body);
    bsn! {
        @WindowContent
        Children [ {body} ]
    }
}

fn number(axis: &str, value: i32, range: i32) -> Box<dyn Scene> {
    let input = NumberInput {
        value: f64::from(value),
        min_value: f64::from(-range),
        max_value: f64::from(range),
        step: 1.0,
        step_pixel_size: 10.0,
        ..Default::default()
    };
    let on_change = ActOnChange {
        action: ACT_SET_POS.to_owned(),
        param: axis.to_owned(),
    };
    Box::new(bsn! {
        Node { width: px(90) }
        input
        on_change
    })
}

fn pad_control(pad: &SelectedPad) -> Box<dyn Scene> {
    let submit = ActOnSubmit {
        action: ACT_RENAME.to_owned(),
        param: "name".to_owned(),
        params: Vec::new(),
    };
    let name = pad.pad_name.clone();
    let max = Some(42usize);
    let remove = EntityScene(button("times", "Remove", ACT_REMOVE, Vec::new()));
    let columns: Vec<Box<dyn Scene>> = MOVES
        .iter()
        .map(|col| {
            let buttons: Vec<Box<dyn Scene>> = col
                .iter()
                .map(|&(icon, x, y)| {
                    let label = if icon.is_empty() { "R" } else { "" };
                    button(
                        icon,
                        label,
                        ACT_MOVE_POS,
                        vec![
                            ("x".to_owned(), x.to_string()),
                            ("y".to_owned(), y.to_string()),
                        ],
                    )
                })
                .collect();
            let b: Box<dyn Scene> = Box::new(bsn! {
                Node { flex_direction: FlexDirection::Column }
                Children [ {buttons} ]
            });
            b
        })
        .collect();
    let targets = vec![
        row("X", number("x", pad.x, pad.range)),
        row("Y", number("y", pad.y, pad.range)),
    ];
    let launch = EntityScene(button("upload", "Launch", ACT_LAUNCH, Vec::new()));
    let pull = EntityScene(button("download", "Pull", ACT_PULL, Vec::new()));
    let inactive = (!pad.pad_active).then(|| EntityScene(notice("Launchpad unpowered")));
    Box::new(bsn! {
        @Stack { @vertical: true, @fill: true }
        Children [
            @StackItem Children [
                @Section Children [
                    Node Children [
                        Node { width: px(170) }
                        Input { value: name, max_length: max }
                        submit
                        --
                        {remove}
                    ]
                ]
            ]
            --
            @StackItem { @grow: 1.0 } Children [
                @Section Children [
                    Node Children [
                        @Section { @title: "Controls" } Children [ Node Children [ {columns} ] ]
                        --
                        @Section { @title: "Target" } Children [ @LabeledList Children [ {targets} ] ]
                    ]
                ]
            ]
            --
            @StackItem Children [
                @Section Children [ {inactive} -- Node Children [ {launch} -- {pull} ] ]
            ]
        ]
    })
}

impl ClientUi for SatelliteControlUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 305.0;

    fn content(_: &(), data: &SatelliteControlData) -> impl Scene {
        satellites(data)
    }
}

fn satellites(data: &SatelliteControlData) -> impl Scene + use<> {
    let shield = data.meteor_shield.then(|| {
        let value =
            data.meteor_shield_coverage as f32 / data.meteor_shield_coverage_max.max(1) as f32;
        let ranges = vec![
            (UiColor::Good, 1.0, f32::INFINITY),
            (UiColor::Average, 0.3, 1.0),
            (UiColor::Bad, f32::NEG_INFINITY, 0.3),
        ];
        let bar: Box<dyn Scene> = Box::new(bsn! {
            @ProgressBar { @empty: true }
            ProgressBar { value: value, ranges: ranges }
            Node { flex_grow: 1.0, width: percent(100) }
        });
        let rows = vec![row("Coverage", bar)];
        EntityScene(Box::new(bsn! {
            @Section Children [ @LabeledList Children [ {rows} ] ]
        }) as Box<dyn Scene>)
    });
    let buttons: Vec<Box<dyn Scene>> = data
        .satellites
        .iter()
        .map(|s| {
            let icon = if s.active {
                "check-square-o"
            } else {
                "square-o"
            };
            toggle_button(
                icon,
                format!("#{} {}", s.id, s.mode),
                ACT_TOGGLE,
                vec![("id".to_owned(), s.id.to_string())],
                s.active,
            )
        })
        .collect();
    let controls = EntityScene(section("Satellite Controls".to_owned(), buttons));
    bsn! {
        @WindowContent
        Children [ {shield} -- {controls} ]
    }
}

impl ClientUi for GatewayUi {
    const WIDTH: f32 = 350.0;
    const HEIGHT: f32 = 440.0;

    fn content(_: &(), data: &GatewayData) -> impl Scene {
        gateway(data)
    }
}

fn gateway(data: &GatewayData) -> impl Scene + use<> {
    let parts: Vec<Box<dyn Scene>> = if !data.gateway_present {
        vec![section(
            String::new(),
            vec![
                notice("No linked gateway"),
                button("", "Linkup", ACT_LINKUP, Vec::new()),
            ],
        )]
    } else if let Some(target) = &data.current_target {
        vec![section(
            target.name.clone(),
            vec![button("", "Deactivate", ACT_DEACTIVATE, Vec::new())],
        )]
    } else if data.destinations.is_empty() {
        vec![section(
            String::new(),
            vec![text("No gateway nodes detected.")],
        )]
    } else {
        let mut out = Vec::new();
        if !data.gateway_status {
            out.push(notice("Gateway Unpowered"));
        }
        for dest in &data.destinations {
            let inner = if dest.available {
                button(
                    "",
                    "Activate",
                    ACT_ACTIVATE,
                    vec![("destination".to_owned(), dest.r#ref.clone())],
                )
            } else {
                colored(UiColor::Bad, dest.reason.clone())
            };
            out.push(section(dest.name.clone(), vec![inner]));
        }
        out
    };
    bsn! {
        @WindowContent
        Children [ {parts} ]
    }
}
