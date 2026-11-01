//! Every ss13_ui_kit primitive and variant, plus tgui's `Teleporter`, `AuxBaseConsole` and
//! `ShuttleConsole` windows ported 1:1.
//!
//! `cargo run -p ss13_ui_kit --example gallery`
//! `cargo run -p ss13_ui_kit --example gallery -- --screenshot out.png` opens one dropdown, saves a
//! PNG and exits.

use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::scene::SpawnListSystem;
use ss13_ui_kit::UiKitPlugin;
use ss13_ui_kit::prelude::*;

fn main() {
    let screenshot = std::env::args().skip_while(|a| a != "--screenshot").nth(1);
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(bevy::window::Window {
                title: "ss13_ui_kit gallery".into(),
                resolution: (1920, 1080).into(),
                ..default()
            }),
            ..default()
        }),
        UiKitPlugin,
    ))
    .insert_resource(ClearColor(Color::srgb(0.05, 0.06, 0.08)))
    .init_resource::<ActLog>()
    .add_systems(Startup, scene.spawn())
    .add_systems(Update, show_act_log.run_if(resource_changed::<ActLog>))
    .add_systems(Update, tick_numbers)
    .add_observer(log_act)
    .add_observer(switch_tabs)
    .add_observer(apply_selection);
    if let Some(path) = screenshot {
        app.add_systems(
            Update,
            move |mut frames: Local<u32>,
                  mut commands: Commands,
                  dropdowns: Query<(&Act, &Children), With<Dropdown>>,
                  mut exit: MessageWriter<AppExit>| {
                *frames += 1;
                // Show one open menu in the picture: the "Dropdown" section's first control.
                if *frames == 10
                    && let Some((_, children)) = dropdowns.iter().find(|(act, _)| act.0 == "pick")
                {
                    commands.trigger(bevy::ui_widgets::Activate {
                        entity: children[0],
                    });
                }
                // Layout settles in a couple of frames; the label equalizer needs one more.
                if *frames == 30 {
                    commands
                        .spawn(Screenshot::primary_window())
                        .observe(save_to_disk(path.clone()));
                }
                if *frames == 40 {
                    exit.write(AppExit::Success);
                }
            },
        );
    }
    app.run();
}

/// Last `act()` received from any window, shown in the gallery to prove the event path.
#[derive(Resource, Default)]
struct ActLog(String);

#[derive(Component, Default, Clone)]
struct ActLogText;

fn log_act(act: On<UiAct>, names: Query<&Name>, mut log: ResMut<ActLog>) {
    let window = names.get(act.scope).map_or("?", |n| n.as_str());
    log.0 = format!("act(\"{}\", {:?}) из окна {window}", act.action, act.params);
    info!("{}", log.0);
}

/// Local `useState` tabs: a tab's act names its panel; the window shows that panel only.
#[derive(Component, Default, Clone)]
struct TabPanel(&'static str);

fn switch_tabs(
    act: On<UiAct>,
    mut tabs: Query<(&mut TabsTab, &Act)>,
    mut panels: Query<(&TabPanel, &mut Node)>,
) {
    let Some(panel) = act.action.strip_prefix("tab:") else {
        return;
    };
    for (mut tab, tab_act) in &mut tabs {
        if let Some(name) = tab_act.0.strip_prefix("tab:") {
            tab.selected = name == panel;
        }
    }
    for (p, mut node) in &mut panels {
        let display = if p.0 == panel {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
}

/// Plays the server: a picked option becomes the new `selected`.
fn apply_selection(picked: On<DropdownSelected>, mut dropdowns: Query<&mut Dropdown>) {
    if let Ok(mut dropdown) = dropdowns.get_mut(picked.entity) {
        dropdown.selected = picked.value.clone();
    }
}

/// Drives the progress bars and animated numbers, as live data would.
fn tick_numbers(
    time: Res<Time>,
    mut bars: Query<&mut ProgressBar, With<Live>>,
    mut numbers: Query<&mut AnimatedNumber>,
    mut last: Local<f32>,
) {
    let t = time.elapsed_secs();
    if t - *last < 1.5 {
        return;
    }
    *last = t;
    let phase = (t * 0.7).sin() * 0.5 + 0.5;
    for mut bar in &mut bars {
        bar.value = phase;
    }
    for mut number in &mut numbers {
        number.value = (phase as f64 * 1000.0).round() / 10.0;
    }
}

#[derive(Component, Default, Clone)]
struct Live;

fn show_act_log(log: Res<ActLog>, mut text: Single<&mut Text, With<ActLogText>>) {
    text.0 = log.0.clone();
}

fn scene() -> impl SceneList {
    bsn_list! {
        Camera2d
        --
        @teleporter(TeleporterData::default())
        --
        @aux_base_console(AuxData::default())
        --
        @shuttle_console()
        --
        @gallery()
        --
        @gallery_more()
    }
}

/// The data tgui's `Teleporter.jsx` reads from `useBackend()`.
struct TeleporterData {
    calibrated: bool,
    calibrating: bool,
    power_station: bool,
    teleporter_hub: bool,
    regime_set: String,
    target: String,
}

impl Default for TeleporterData {
    fn default() -> Self {
        Self {
            calibrated: false,
            calibrating: false,
            power_station: true,
            teleporter_hub: true,
            regime_set: "Teleporter".into(),
            target: "Мостик".into(),
        }
    }
}

/// Port of `tgui/packages/tgui/interfaces/Teleporter.jsx`.
fn teleporter(data: TeleporterData) -> impl Scene {
    let body: Box<dyn Scene> = if !data.power_station {
        Box::new(bsn! {
            @UiBox { @color: UiColor::Bad, @text_align: Justify::Center }
            Children [ Text("No power station linked.") ]
        })
    } else if !data.teleporter_hub {
        Box::new(bsn! {
            @UiBox { @color: UiColor::Bad, @text_align: Justify::Center }
            Children [ Text("No hub linked.") ]
        })
    } else {
        let (calibration_color, calibration) = if data.calibrating {
            (UiColor::Average, "In Progress")
        } else if data.calibrated {
            (UiColor::Good, "Optimal")
        } else {
            (UiColor::Bad, "Sub-Optimal")
        };
        Box::new(bsn! {
            @LabeledList
            Children [
                @LabeledListItem { @label: "Regime" }
                Children [
                    @Button { @content: {data.regime_set} } Act("regimeset")
                ]
                --
                @LabeledListItem { @label: "Target" }
                Children [
                    @Button { @icon: "edit", @content: {data.target} } Act("settarget")
                ]
                --
                @LabeledListItem {
                    @label: "Calibration",
                    @buttons: bsn! {
                        @Button { @icon: "tools", @content: "Calibrate" } Act("calibrate")
                    },
                }
                Children [
                    @UiBox { @color: calibration_color } Children [ Text(calibration) ]
                ]
            ]
        })
    };
    bsn! {
        #Teleporter
        @Window { @title: "Teleporter Control Console", @width: 360.0, @height: 130.0, @left: 20.0, @top: 20.0 }
        Children [
            @WindowContent
            Children [
                @Section Children [ @{body} ]
            ]
        ]
    }
}

fn gallery() -> impl Scene {
    bsn! {
        #Gallery
        @Window { @title: "ss13_ui_kit — галерея примитивов", @width: 760.0, @height: 800.0, @left: 1150.0, @top: 20.0 }
        Children [
            @WindowContent
            Children [
                @Stack { @fill: true }
                Children [
                    @StackItem { @grow: 1.0 } Children [ @buttons_section() -- @notices_section() -- @boxes_section() ]
                    --
                    @StackItem { @grow: 1.0 } Children [ @layout_section() -- @labeled_list_section() ]
                ]
            ]
        ]
    }
}

fn buttons_section() -> impl Scene {
    bsn! {
        @Section {
            @title: "Button",
            @buttons: bsn! { @Button { @icon: "cog", @content: "Title" } Act("title_button") },
        }
        Children [
            @Stack { @wrap: true }
            Children [
                @Button { @content: "Default" } Act("default")
                --
                @Button { @content: "Selected" } Button { selected: true } Act("selected")
                --
                @Button { @content: "Disabled" } Button { disabled: true } Act("disabled")
                --
                @Button { @content: "Caution" } Button { color: UiColor::Caution } Act("caution")
                --
                @Button { @content: "Danger" } Button { color: UiColor::Danger } Act("danger")
                --
                @Button { @content: "Transparent" } Button { color: UiColor::Transparent } Act("transparent")
                --
                @Button { @content: "Good" } Button { color: UiColor::Good }
                --
                @Button { @content: "Average" } Button { color: UiColor::Average }
                --
                @Button { @content: "Bad" } Button { color: UiColor::Bad }
                --
                @Button { @content: "Violet" } Button { color: UiColor::Violet }
                --
                @Button { @icon: "power-off", @content: "Иконка" } Act("icon")
                --
                @Button { @icon: "times" } Act("icon_only")
                --
                @Button { @content: "Compact", @compact: true }
                --
                @Button { @icon: "plus", @circular: true }
                --
                @Button { @content: "Transparent selected" } Button { color: UiColor::Transparent, selected: true }
            ]
            --
            @Divider
            --
            @Button { @content: "Fluid — на всю ширину", @fluid: true } Act("fluid")
            --
            @Button Act("children") Children [ Text("Содержимое через Children") ]
            --
            @UiBox Node { margin: UiRect::top(half_unit(1.0)) } Children [ Text("Нажмите кнопку…") ActLogText ]
        ]
    }
}

fn notices_section() -> impl Scene {
    bsn! {
        @Section { @title: "NoticeBox" }
        Children [
            @NoticeBox Children [ Text("Обычное уведомление") ]
            --
            @NoticeBox { @info: true } Children [ Text("info") ]
            --
            @NoticeBox { @success: true } Children [ Text("success") ]
            --
            @NoticeBox { @warning: true } Children [ Text("warning") ]
            --
            @NoticeBox { @danger: true } Children [ Text("danger") ]
            --
            @NoticeBox { @color: UiColor::Yellow } Children [ Text("color=\"yellow\": чёрный текст") ]
        ]
    }
}

fn boxes_section() -> impl Scene {
    bsn! {
        @Section { @title: "Box" }
        Children [
            @UiBox { @color: UiColor::Good } Children [ Text("color=\"good\"") ]
            --
            @UiBox { @color: UiColor::Average, @bold: true } Children [ Text("color=\"average\" bold") ]
            --
            @UiBox { @color: UiColor::Bad, @italic: true } Children [ Text("color=\"bad\" italic") ]
            --
            @UiBox { @color: UiColor::Label } Children [ Text("color=\"label\"") ]
            --
            @UiBox { @font_size: 20.0 } Children [ Text("fontSize=\"20px\"") ]
            --
            @UiBox { @text_align: Justify::Center } Children [ Text("textAlign=\"center\"") ]
            --
            @UiBox { @background_color: UiColor::Blue, @inline: true } Node { padding: UiRect::all(half_unit(1.0)) }
            Children [ Text("backgroundColor=\"blue\" inline") ]
            --
            @UiBox Children [ Text("Съешь же ещё этих мягких французских булок, да выпей чаю. ЁЁ ёё №") ]
        ]
    }
}

fn layout_section() -> impl Scene {
    bsn! {
        @Section { @title: "Stack / Flex / Divider" }
        Children [
            @Stack
            Children [
                @StackItem @UiBox { @background_color: UiColor::Teal } Node { padding: UiRect::all(half_unit(1.0)) } Children [ Text("item") ]
                --
                @StackItem { @grow: 1.0 } @UiBox { @background_color: UiColor::Purple } Node { padding: UiRect::all(half_unit(1.0)) } Children [ Text("grow") ]
                --
                @StackDivider
                --
                @StackItem @UiBox { @background_color: UiColor::Brown } Node { padding: UiRect::all(half_unit(1.0)) } Children [ Text("item") ]
            ]
            --
            @Divider
            --
            @Stack { @vertical: true, @zebra: true }
            Children [
                @StackItem Children [ Text("vertical zebra 1") ]
                --
                @StackItem Children [ Text("vertical zebra 2") ]
                --
                @StackItem Children [ Text("vertical zebra 3") ]
            ]
            --
            @Divider
            --
            @Flex { @justify: JustifyContent::SpaceBetween, @align: AlignItems::Center }
            Children [
                @FlexItem Children [ Text("Flex: слева") ]
                --
                @FlexItem Children [ @Button { @content: "по центру" } ]
                --
                @FlexItem Children [ Text("справа") ]
            ]
            --
            @Divider
            --
            @Stack Node { height: px(40) }
            Children [
                @StackItem Children [ Text("vertical") ]
                --
                @StackItem @Divider { @vertical: true }
                --
                @StackItem Children [ Text("divider") ]
            ]
            --
            @Section { @title: "Вложенная секция" }
            Children [
                Text("Section в Section: без фона, заголовок 13px")
                --
                @Section { @title: "Третий уровень" } Children [ Text("заголовок 12px") ]
            ]
        ]
    }
}

fn labeled_list_section() -> impl Scene {
    bsn! {
        @Section { @title: "LabeledList" }
        Children [
            @LabeledList
            Children [
                @LabeledListItem { @label: "Давление", @content: "101.3 kPa" }
                --
                @LabeledListItem { @label: "Температура", @color: UiColor::Good, @content: "293 K" }
                --
                @LabeledListItem { @label: "Длинная метка", @color: UiColor::Bad, @content: "Ошибка" }
                --
                @LabeledListDivider
                --
                @LabeledListItem {
                    @label: "Питание",
                    @buttons: bsn! { @Button { @icon: "power-off", @content: "Вкл" } Button { selected: true } Act("power") },
                }
                Children [ Text("12 кВт") ]
                --
                @LabeledListItem { @label: "Скорость" }
                Children [
                    @Button { @content: "−" } Act("slower")
                    --
                    @Button { @content: "+" } Act("faster")
                    --
                    @UiBox { @color: UiColor::Average } Children [ Text("50 L/s") ]
                ]
            ]
        ]
    }
}

/// The data tgui's `AuxBaseConsole.tsx` and `ShuttleConsole.jsx` read from `useBackend()`.
struct AuxData {
    shuttle: bool,
    blind_drop: bool,
    status: &'static str,
    timer: &'static str,
    docked_location: &'static str,
    destination: &'static str,
    locations: Vec<&'static str>,
    turrets: Vec<Turret>,
}

struct Turret {
    reference: String,
    name: String,
    integrity: u32,
    status: &'static str,
    direction: &'static str,
    distance: u32,
}

impl Default for AuxData {
    fn default() -> Self {
        let statuses = ["All Clear", "Firing", "Disabled", "ERROR"];
        let directions = ["North", "East", "South", "West"];
        let turrets = (0..12)
            .map(|i| Turret {
                reference: format!("[0x20{i:02x}]"),
                name: format!("Base turret #{}", i + 1),
                integrity: 100 - (i * 7) % 60,
                status: statuses[i as usize % 4],
                direction: directions[i as usize % 4],
                distance: 5 + i * 3,
            })
            .collect();
        Self {
            shuttle: false,
            blind_drop: true,
            status: "Idle",
            timer: "00:42",
            docked_location: "Mining Station",
            destination: "Lavaland Base",
            locations: vec!["Lavaland Base", "Mining Station", "Deep Space"],
            turrets,
        }
    }
}

/// `STATUS_COLOR_KEYS` of `ShuttleConsole.jsx`.
fn shuttle_status_color(status: &str) -> UiColor {
    match status {
        "In Transit" => UiColor::Good,
        "Idle" | "Igniting" | "Recharging" => UiColor::Average,
        _ => UiColor::Bad,
    }
}

/// `STATUS_COLOR_KEYS` of `AuxBaseConsole.tsx`.
fn turret_status_color(status: &str) -> UiColor {
    match status {
        "Firing" => UiColor::Average,
        "All Clear" => UiColor::Good,
        _ => UiColor::Bad,
    }
}

/// Port of `ShuttleConsoleContent` (`ShuttleConsole.jsx`).
fn shuttle_console_content(data: &AuxData) -> impl Scene + use<> {
    let status = data.status;
    let status_color = shuttle_status_color(status);
    let timer = data.timer;
    let docked = data.docked_location;
    let controls = if data.shuttle {
        "Shuttle Controls"
    } else {
        "Base Launch Controls"
    };
    let options: Vec<String> = data.locations.iter().map(|l| l.to_string()).collect();
    let destination = data.destination.to_string();
    let blind_drop = (!data.shuttle && data.locations.is_empty() && data.blind_drop).then(|| {
        bsn! {
            @Button { @icon: "exclamation-triangle", @content: "Blind Drop" }
            Button { color: UiColor::Bad }
            Act("random")
        }
    });
    bsn! {
        @Section
        Children [
            @UiBox { @bold: true, @font_size: 26.0, @text_align: Justify::Center }
            Children [ Text(timer) ]
            --
            @UiBox { @text_align: Justify::Center, @font_size: 14.0 }
            Node { margin: UiRect::bottom(half_unit(1.0)) }
            Children [
                @UiBox { @inline: true, @bold: true } Children [ Text("STATUS:") ]
                --
                @UiBox { @inline: true, @color: status_color }
                Node { margin: UiRect::left(half_unit(1.0)) }
                Children [ Text(status) ]
            ]
            --
            @Section { @title: controls }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem { @label: "Location", @content: docked }
                    --
                    @LabeledListItem { @label: "Destination", @buttons: {blind_drop.map(|b| Box::new(bsn_list! { @{b} }) as Box<dyn SceneList>)} }
                    Children [
                        @Dropdown { @over: true, @width: 20.0 }
                        Dropdown { options: options, selected: destination }
                        Act("set_destination")
                        Node { margin: UiRect::bottom(half_unit(1.7)) }
                    ]
                ]
                --
                @Button { @fluid: true, @icon: "arrow-up", @content: "Depart" }
                Act("move")
                ActParams({vec![("shuttle_id".into(), "lavaland_base".into())]})
            ]
        ]
    }
}

/// Port of `tgui/packages/tgui/interfaces/AuxBaseConsole.tsx`, the "Turrets" tab open.
fn aux_base_console(data: AuxData) -> impl Scene {
    let turret_count = data.turrets.len();
    let shuttle_tab = if data.shuttle {
        "Shuttle Launch"
    } else {
        "Base Launch"
    };
    let turrets_tab = format!("Turrets ({turret_count})");
    let shuttle = shuttle_console_content(&data);
    let header = Box::new(bsn! {
        @TableRow { @header: true }
        Children [
            @TableCell { @content: "Unit" } -- @TableCell { @content: "Condition" } --
            @TableCell { @content: "Status" } -- @TableCell { @content: "Direction" } --
            @TableCell { @content: "Distance" } -- @TableCell { @content: "Power" }
        ]
    }) as Box<dyn Scene>;
    let rows: Vec<Box<dyn Scene>> = std::iter::once(header)
        .chain(data.turrets.into_iter().map(|t| {
            let color = turret_status_color(t.status);
            let integrity = format!("{}%", t.integrity);
            let distance = format!("{}m", t.distance);
            Box::new(bsn! {
                @TableRow
                Children [
                    @TableCell { @bold: true, @content: {t.name} }
                    --
                    @TableCell { @content: integrity }
                    --
                    @TableCell { @color: color, @content: {t.status} }
                    --
                    @TableCell { @content: {t.direction} }
                    --
                    @TableCell { @content: distance }
                    --
                    @TableCell
                    Children [
                        @Button { @icon: "power-off", @content: "Toggle" }
                        Act("single_turret_power")
                        ActParams({vec![("single_turret_power".into(), t.reference)]})
                    ]
                ]
            }) as Box<dyn Scene>
        }))
        .collect();
    bsn! {
        #AuxBaseConsole
        @Window { @title: "Aux Base Console", @width: 620.0, @height: 310.0, @left: 20.0, @top: 170.0 }
        Children [
            @WindowContent { @scrollable: true }
            Children [
                @Tabs
                Children [
                    @TabsTab { @icon: "list", @content: shuttle_tab } Act("tab:shuttle")
                    --
                    @TabsTab { @icon: "list", @content: turrets_tab } TabsTab { selected: true } Act("tab:aux")
                ]
                --
                @UiBox Node { display: Display::None } TabPanel("shuttle")
                Children [ @{shuttle} ]
                --
                @UiBox Node { flex_grow: 1.0, min_height: px(0) } TabPanel("aux")
                Children [
                    @Section {
                        @title: "Turret Control",
                        @fill: true,
                        @scrollable: true,
                        @buttons: bsn! { @Button { @icon: "power-off", @content: "Toggle Power" } Act("turrets_power") },
                    }
                    Children [
                        @Table Children [ {rows} ]
                    ]
                ]
            ]
        ]
    }
}

/// Port of `ShuttleConsole.jsx` while `authorization_required`: the `Modal` blocks the console.
fn shuttle_console() -> impl Scene {
    let content = shuttle_console_content(&AuxData {
        shuttle: true,
        status: "Locked",
        ..AuxData::default()
    });
    bsn! {
        #ShuttleConsole
        @Window { @title: "Shuttle Console", @width: 350.0, @height: 260.0, @left: 20.0, @top: 500.0 }
        Children [
            @WindowContent
            Children [
                @{content}
                --
                @Modal
                Children [
                    @UiBox { @font_size: 28.0, @text_align: Justify::Center }
                    Children [
                        @Flex
                        Children [
                            @FlexItem Node { margin: UiRect::top(half_unit(2.0)) }
                            Children [ @Icon { @name: "minus-circle" } ]
                            --
                            @FlexItem Node { margin: {UiRect { top: half_unit(2.0), left: half_unit(2.0), ..UiRect::DEFAULT }} }
                            @UiBox { @color: UiColor::Bad }
                            Children [ Text("SHUTTLE LOCKED") ]
                        ]
                        --
                        @UiBox { @font_size: 18.0 } Node { margin: UiRect::top(half_unit(4.0)) }
                        Children [
                            @Button { @icon: "arrow-circle-right", @content: "Request Authorization" }
                            Button { color: UiColor::Bad }
                            Act("request")
                        ]
                    ]
                ]
            ]
        ]
    }
}

/// Wave-2 primitives: icons, inline flow, tabs, table, progress, dropdown, tooltip, scrolling.
fn gallery_more() -> impl Scene {
    bsn! {
        #Gallery2
        @Window { @title: "ss13_ui_kit — галерея 2", @width: 480.0, @height: 1040.0, @left: 655.0, @top: 20.0 }
        Children [
            @WindowContent { @scrollable: true }
            Children [
                @icons_section() -- @inline_section() -- @tabs_section() -- @table_section()
                -- @progress_section() -- @dropdown_section() -- @tooltip_section()
                -- @virtual_list_section() -- @scroll_section()
            ]
        ]
    }
}

fn icons_section() -> impl Scene {
    bsn! {
        @Section { @title: "Icon (Font Awesome Free 6.7.2)" }
        Children [
            @Icon { @name: "cog" } -- @Icon { @name: "power-off" } -- @Icon { @name: "times" }
            -- @Icon { @name: "check-square-o" } -- @Icon { @name: "square-o" }
            -- @Icon { @name: "radiation", @color: UiColor::Yellow } -- @Icon { @name: "biohazard", @color: UiColor::Green }
            -- @Icon { @name: "arrow-up", @rotation: 45.0 } -- @Icon { @name: "spinner", @spin: true }
            -- @Icon { @name: "heart", @size: 2.0, @color: UiColor::Red } -- @Icon { @name: "user-astronaut", @size: 2.0 }
        ]
    }
}

fn inline_section() -> impl Scene {
    bsn! {
        @Section { @title: "Inline: кнопки в обычном блоке" }
        Children [
            @Button { @content: "Один" } -- @Button { @content: "Два" } -- @Button { @content: "Три" }
            -- @Button { @icon: "plus", @content: "Четыре" } -- @Button { @content: "Пять" }
            -- @Button { @content: "Шесть, длинная" } -- @Button { @icon: "times" } -- @Button { @content: "Семь" }
            --
            @UiBox { @text_align: Justify::Center } Node { margin: UiRect::top(half_unit(1.0)) }
            Children [ @Button { @content: "textAlign center" } -- @Button { @content: "второй" } ]
        ]
    }
}

fn tabs_section() -> impl Scene {
    bsn! {
        @Section { @title: "Tabs" }
        Children [
            @Tabs
            Children [
                @TabsTab { @icon: "list", @content: "Выбрана" } TabsTab { selected: true } Act("tab_a")
                --
                @TabsTab { @icon: "cog", @content: "Вторая" } Act("tab_b")
                --
                @TabsTab { @content: "Цветная" } TabsTab { selected: false, color: UiColor::Red } Act("tab_c")
            ]
            --
            @Stack
            Children [
                @StackItem
                Children [
                    @Tabs { @vertical: true }
                    Children [
                        @TabsTab { @content: "Вертикаль" } TabsTab { selected: true }
                        --
                        @TabsTab { @content: "Вкладка 2" }
                        --
                        @TabsTab { @content: "Вкладка 3" }
                    ]
                ]
                --
                @StackItem { @grow: 1.0 }
                Children [
                    @Tabs { @fluid: true }
                    Children [
                        @TabsTab { @content: "fluid" } TabsTab { selected: true } -- @TabsTab { @content: "fluid 2" }
                    ]
                ]
            ]
        ]
    }
}

fn table_section() -> impl Scene {
    bsn! {
        @Section { @title: "Table" }
        Children [
            @Table
            Children [
                @TableRow { @header: true }
                Children [
                    @TableCell { @content: "Предмет" } -- @TableCell { @content: "Кол-во", @collapsing: true }
                    -- @TableCell { @content: "Цена", @text_align: Justify::Right }
                ]
                --
                @TableRow
                Children [
                    @TableCell { @content: "Отвёртка" } -- @TableCell { @content: "12", @collapsing: true }
                    -- @TableCell { @content: "5 cr", @text_align: Justify::Right }
                ]
                --
                @TableRow
                Children [
                    @TableCell { @content: "Изолированные перчатки" } -- @TableCell { @content: "1", @collapsing: true }
                    -- @TableCell { @content: "250 cr", @text_align: Justify::Right, @color: UiColor::Average }
                ]
                --
                @TableRow
                Children [
                    @TableCell { @content: "Лом" } -- @TableCell { @collapsing: true } Children [ @Button { @icon: "minus" } -- @Button { @icon: "plus" } ]
                    -- @TableCell { @content: "40 cr", @text_align: Justify::Right }
                ]
            ]
        ]
    }
}

fn progress_section() -> impl Scene {
    let ranges = vec![
        (UiColor::Good, 0.5, f32::INFINITY),
        (UiColor::Average, 0.25, 0.5),
        (UiColor::Bad, f32::NEG_INFINITY, 0.25),
    ];
    bsn! {
        @Section { @title: "ProgressBar / AnimatedNumber" }
        Children [
            @ProgressBar ProgressBar { value: 0.75, ranges: {ranges.clone()} } Live
            --
            @UiBox Node { margin: UiRect::top(half_unit(0.5)) }
            Children [ @ProgressBar ProgressBar { value: 0.3, ranges: {ranges.clone()} } ]
            --
            @UiBox Node { margin: UiRect::top(half_unit(0.5)) }
            Children [ @ProgressBar ProgressBar { value: 0.1, ranges: ranges } ]
            --
            @UiBox Node { margin: UiRect::top(half_unit(0.5)) }
            Children [
                @ProgressBar ProgressBar { value: 42.0, max_value: 100.0, color: UiColor::Violet }
                Children [ Text("color=violet, свой текст") ]
            ]
            --
            @UiBox Node { margin: UiRect::top(half_unit(0.5)) }
            Children [ @ProgressBar { @empty: true } ProgressBar { value: 0.6, color: UiColor::Teal } ]
            --
            @LabeledList Node { margin: UiRect::top(half_unit(1.0)) }
            Children [
                @LabeledListItem { @label: "AnimatedNumber" }
                Children [ @AnimatedNumber AnimatedNumber { value: 50.0 } -- Text(" %") ]
            ]
        ]
    }
}

fn dropdown_section() -> impl Scene {
    let options = || -> Vec<String> {
        [
            "Мостик",
            "Медблок",
            "Инженерия",
            "Карго",
            "Научный отдел",
            "Бриг",
            "Кухня",
            "Бар",
            "Морг",
            "Сервис",
            "Шахта",
            "Телепортер",
        ]
        .map(String::from)
        .to_vec()
    };
    bsn! {
        @Section { @title: "Dropdown" }
        Children [
            @Dropdown Dropdown { options: {options()}, selected: "Медблок" } Act("pick")
            --
            @Dropdown { @buttons: true, @width: 10.0, @icon: "map" } Dropdown { options: {options()} } Act("pick_buttons")
            --
            @Dropdown { @width: 10.0 } Dropdown { options: {options()}, selected: "Бар", disabled: true }
            --
            @Dropdown { @fluid: true, @color: UiColor::Caution } Dropdown { options: {options()}, display_text: "displayText вместо выбора" } Act("pick_fluid")
        ]
    }
}

fn tooltip_section() -> impl Scene {
    bsn! {
        @Section { @title: "Tooltip" }
        Children [
            @Button { @content: "снизу", @tooltip: "Подсказка снизу (по умолчанию)" }
            -- @Button { @content: "сверху", @tooltip: "Подсказка сверху", @tooltip_position: Placement::Top }
            -- @Button { @content: "справа", @tooltip: "Справа", @tooltip_position: Placement::Right }
            -- @Button { @icon: "info", @tooltip: "Длинная подсказка переносится по словам, если шире 250 пикселей, как max-width: 250px у tgui.", @tooltip_position: Placement::BottomStart }
        ]
    }
}

fn virtual_list_section() -> impl Scene {
    let row = VirtualRow::new(|i| {
        let label = format!("Строка #{i}: сообщение чата");
        let bg = if i % 2 == 0 {
            Color::NONE
        } else {
            Color::srgba(0.0, 0.0, 0.0, 0.25)
        };
        bsn! {
            Node { height: percent(100), padding: UiRect::horizontal(px(6)), align_items: AlignItems::Center }
            BackgroundColor(bg)
            Children [ Text(label) ]
        }
    });
    bsn! {
        @Section { @title: "VirtualList: 10 000 строк" }
        Children [
            @VirtualList { @item_height: 18.0, @row: row } VirtualList { len: 10_000 } Node { height: px(150) }
        ]
    }
}

fn scroll_section() -> impl Scene {
    bsn! {
        @Section { @title: "Section scrollable", @fill: true, @scrollable: true } Node { height: px(120) }
        Children [
            Text("Прокрутка колесом, перетаскивание ползунка, щелчок по дорожке листает страницу.")
            -- Text("1. Атмосферные датчики") -- Text("2. Двигатели") -- Text("3. Щиты") -- Text("4. Солнечные панели")
            -- Text("5. Суперматерия") -- Text("6. Телекоммуникации") -- Text("7. Конец списка")
        ]
    }
}
