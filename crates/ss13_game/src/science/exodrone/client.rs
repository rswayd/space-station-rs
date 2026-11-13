//! The exploration windows (client): tgui `ExodroneConsole.tsx` and `ExoscannerConsole.tsx`.
//! The data is in [`super`], the server half in [`super::server`].
//!
//! Client-only state (`useState`): the tool selection modal and the exploration screen's
//! travel list ([`UiLocalState`] keys).
// shortcut: no event and adventure images, no `Button.Confirm` (self-destruct and stopping a
// scan act on the first click), no NT logo on the signal-lost modal.

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::FontPx;

use super::*;

/// `choosingTools`.
const TOOLS: &str = "exodrone_tools";
/// `TravelDimmerShown`.
const TRAVEL: &str = "exodrone_travel";

pub struct ExodroneClientPlugin;

impl Plugin for ExodroneClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<ExodroneConsoleUi>()
            .register_ui_view::<ExoscannerConsoleUi>();
    }
}

/// tgui-core `formatTime(deciseconds, format)`: `HH:MM:SS`, or `1h2m3s` when `short`.
pub fn format_time(secs: f64, short: bool) -> String {
    let total = secs.max(0.0).floor() as u64;
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if short {
        let mut out = String::new();
        if h > 0 {
            out.push_str(&format!("{h}h"));
        }
        if m > 0 {
            out.push_str(&format!("{m}m"));
        }
        if s > 0 {
            out.push_str(&format!("{s}s"));
        }
        out
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

use ss13_chat::capitalize;

fn text_row(text: String) -> impl Scene {
    bsn! { Node { margin: UiRect::bottom(px(4)) } Children [ Text(text) ] }
}

fn numeric(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n:.2}")
    }
}

/// A big centered icon with a caption (the dimmers' screens).
fn dimmer_screen(icon: &'static str, color: UiColor, caption: String) -> impl Scene + use<> {
    let text = color.text().unwrap_or_default();
    bsn! {
        @Dimmer
        Children [
            Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center }
            Children [
                @Icon { @name: icon, @size: 6.0, @color: color }
                --
                Node { margin: UiRect::top(px(8)) }
                Propagate::<FontPx>(FontPx(18.0))
                Propagate::<TextColor>(TextColor(text))
                Children [ Text(caption) ]
            ]
        ]
    }
}

// --- Drone console --------------------------------------------------------------------------

impl ClientUi for ExodroneConsoleUi {
    const WIDTH: f32 = 750.0;
    const HEIGHT: f32 = 600.0;

    fn content(_: &(), data: &ExodroneData) -> impl Scene {
        exodrone_view(data, &UiLocalState::default())
    }

    fn content_local(_: &(), data: &ExodroneData, local: &UiLocalState) -> impl Scene {
        exodrone_view(data, local)
    }
}

fn exodrone_view(data: &ExodroneData, local: &UiLocalState) -> impl Scene + use<> {
    let body: Box<dyn SceneList> = match &data.drone {
        None => Box::new(EntityScene(drone_listing(&data.all_drones))),
        Some(drone) => Box::new(EntityScene(drone_content(drone, local))),
    };
    let signal_lost = data.signal_lost.then(|| EntityScene(signal_lost_modal()));
    let tools = (data.drone.is_some() && local.get(TOOLS) == "1")
        .then(|| EntityScene(tool_selection_modal()));
    bsn! { @WindowContent Children [ {body} -- {signal_lost} -- {tools} ] }
}

/// `SignalLostModal`.
fn signal_lost_modal() -> impl Scene {
    let red = UiColor::Bad.text().unwrap_or_default();
    bsn! {
        @Modal
        Children [
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                max_width: px(360),
            }
            Children [
                Node Propagate::<FontPx>(FontPx(24.0)) Propagate::<TextColor>(TextColor(red))
                Children [ Text("CONNECTION LOST") ]
                --
                Node { margin: UiRect::all(px(12)) }
                Children [ Text(
                    "Connection to exploration drone interrupted. Please contact nearest \
                     Nanotrasen Exploration Division representative for further instructions."
                ) ]
                --
                @Icon { @name: "exclamation-triangle", @size: 5.0 }
                --
                @ss13_ui_kit::Button { @content: "Confirm" }
                ss13_ui_kit::Button { color: UiColor::Average }
                Act({CONFIRM_SIGNAL_LOST.to_owned()})
                Name("exodrone: confirm signal lost")
            ]
        ]
    }
}

/// `DroneSelectionSection`.
fn drone_listing(drones: &[DroneListing]) -> impl Scene + use<> {
    let rows: Vec<_> = drones
        .iter()
        .map(|drone| {
            let control: Box<dyn SceneList> = if drone.controlled {
                Box::new(EntityScene(bsn! { Text("Controlled by another console.") }))
            } else {
                let params = ActParams(vec![(DRONE_REF.to_owned(), drone.id.to_string())]);
                Box::new(EntityScene(bsn! {
                    @ss13_ui_kit::Button { @icon: "plug", @content: "Assume Control" }
                    Act({SELECT_DRONE.to_owned()})
                    params
                    Name("exodrone: select drone")
                }))
            };
            let (name, description) = (drone.name.clone(), drone.description.clone());
            bsn! {
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    margin: UiRect::bottom(px(6)),
                }
                Children [
                    Node { width: px(140) } Propagate::<FontPx>(FontPx(18.0))
                    Children [ Text(name) ]
                    --
                    Node { flex_grow: 1.0 } Children [ Text(description) ]
                    --
                    {control}
                ]
            }
        })
        .collect();
    bsn! {
        @Section { @title: "Exploration Drone Listing", @fill: true, @scrollable: true }
        Children [ {rows} ]
    }
}

/// `ToolSelectionModal`.
fn tool_selection_modal() -> impl Scene {
    let close: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @icon: "xmark" }
        SetLocal { key: {TOOLS.to_owned()}, value: {String::new()}, rerender: true }
    }));
    let tools: Vec<_> = Tool::ALL
        .iter()
        .map(|&tool| {
            let params = ActParams(vec![(TOOL_TYPE.to_owned(), tool.name().to_owned())]);
            let label = capitalize(tool.name());
            let (icon, tooltip) = (tool.icon(), tool.description());
            let name = Name::new(format!("exodrone: add {}", tool.name()));
            bsn! {
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    margin: UiRect::horizontal(px(6)),
                }
                Children [
                    @ss13_ui_kit::Button { @icon: icon, @tooltip: tooltip }
                    Act({ADD_TOOL.to_owned()})
                    params
                    SetLocal { key: {TOOLS.to_owned()}, value: {String::new()}, rerender: true }
                    name
                    --
                    Text(label)
                ]
            }
        })
        .collect();
    bsn! {
        @Modal
        Children [
            @Section { @title: "Tool Selection", @buttons: close }
            Children [ Node { flex_direction: FlexDirection::Row } Children [ {tools} ] ]
        ]
    }
}

fn drone_content(drone: &ActiveDrone, local: &UiLocalState) -> impl Scene + use<> {
    let log: Vec<_> = drone
        .log
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let (label, line) = (format!("#{}", i + 1), line.clone());
            bsn! { @LabeledListItem { @label: label, @content: line } }
        })
        .collect();
    let screen = drone_screen(drone, local);
    bsn! {
        Node { flex_direction: FlexDirection::Row, flex_grow: 1.0, height: percent(100) }
        Children [
            Node { flex_direction: FlexDirection::Column, width: px(230), margin: UiRect::right(px(6)) }
            Children [ {EntityScene(equipment_grid(drone))} ]
            --
            Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0) }
            Children [
                Node { flex_grow: 1.0, flex_basis: px(0), margin: UiRect::bottom(px(6)) }
                Children [ {screen} ]
                --
                Node { flex_grow: 1.0, flex_basis: px(0) }
                Children [
                    @Section { @title: "Drone Log", @fill: true, @scrollable: true }
                    Children [ @LabeledList Children [ {log} ] ]
                ]
            ]
        ]
    }
}

/// `EquipmentGrid` with its `EquipmentBox`es.
fn equipment_grid(drone: &ActiveDrone) -> impl Scene + use<> {
    let fraction = if drone.max_integrity > 0.0 {
        drone.integrity / drone.max_integrity
    } else {
        0.0
    };
    let ranges = vec![
        (UiColor::Good, 0.7, f32::INFINITY),
        (UiColor::Average, 0.4, 0.7),
        (UiColor::Bad, f32::NEG_INFINITY, 0.4),
    ];
    let integrity = format!("{:.0}/{:.0}", drone.integrity, drone.max_integrity);
    let (name, status) = (drone.name.clone(), drone.screen.status().to_owned());
    let configure: Option<Box<dyn SceneList>> = drone.configurable.then(|| {
        Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "wrench" }
            ss13_ui_kit::Button { color: UiColor::Average }
            SetLocal { key: {TOOLS.to_owned()}, value: {"1".to_owned()}, rerender: true }
            Name("exodrone: choose tools")
        })) as Box<dyn SceneList>
    });
    let slots: Vec<_> = drone
        .cargo
        .iter()
        .map(|slot| cargo_box(slot, drone.configurable))
        .collect();
    bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [
            @Section { @title: "Master Controls" }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem { @label: "Drone ID", @content: name }
                    --
                    @LabeledListItem { @label: "Activity", @content: status }
                    --
                    @LabeledListItem { @label: "Integrity" }
                    Children [
                        @ProgressBar { @empty: true }
                        ProgressBar { value: fraction, ranges: ranges }
                        Children [ Text(integrity) ]
                    ]
                ]
                --
                Node { margin: UiRect::vertical(px(4)) }
                Children [
                    @ss13_ui_kit::Button { @icon: "plug", @content: "Disconnect", @fluid: true }
                    Act({END_CONTROL.to_owned()})
                    Name("exodrone: disconnect")
                ]
                --
                @ss13_ui_kit::Button { @icon: "bomb", @content: "Self-Destruct", @fluid: true }
                ss13_ui_kit::Button { color: UiColor::Bad }
                Act({SELF_DESTRUCT.to_owned()})
                Name("exodrone: self-destruct")
            ]
            --
            @Section { @title: "Equipment", @buttons: configure }
            Children [ {slots} ]
        ]
    }
}

fn cargo_box(slot: &CargoSlot, configurable: bool) -> impl Scene + use<> {
    let background = Color::srgba(0.0, 0.0, 0.0, 0.3);
    let inner: Box<dyn SceneList> = match slot {
        CargoSlot::Tool(tool) => {
            let (icon, label) = (tool.icon(), capitalize(tool.name()));
            let eject = configurable.then(|| {
                let params = ActParams(vec![(TOOL_TYPE.to_owned(), tool.name().to_owned())]);
                EntityScene(bsn! {
                    @ss13_ui_kit::Button { @icon: "eject", @tooltip: "Eject" }
                    Act({REMOVE_TOOL.to_owned()})
                    params
                })
            });
            Box::new(EntityScene(bsn! {
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, flex_grow: 1.0 }
                Children [
                    Node { width: px(26) } Children [ @Icon { @name: icon, @size: 1.5 } ]
                    --
                    Node { flex_grow: 1.0 } Propagate::<FontPx>(FontPx(16.0)) Children [ Text(label) ]
                    --
                    {eject}
                ]
            }))
        }
        CargoSlot::Cargo { name, id } => {
            let params = ActParams(vec![(TARGET_REF.to_owned(), id.to_string())]);
            let (label, tooltip) = (capitalize(name), format!("Jettison {name}"));
            Box::new(EntityScene(bsn! {
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, flex_grow: 1.0 }
                Children [
                    Node { width: px(26) } Children [ @Icon { @name: "box", @size: 1.5 } ]
                    --
                    Node { flex_grow: 1.0 } Propagate::<FontPx>(FontPx(16.0)) Children [ Text(label) ]
                    --
                    @ss13_ui_kit::Button { @icon: "eject", @tooltip: tooltip }
                    Act({JETTISON.to_owned()})
                    params
                ]
            }))
        }
        CargoSlot::Empty => {
            let faint = Color::srgba(1.0, 1.0, 1.0, 0.2);
            Box::new(EntityScene(bsn! {
                Node {
                    flex_grow: 1.0,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                }
                Propagate::<TextColor>(TextColor(faint))
                Children [ Text("Empty slot") ]
            }))
        }
    };
    bsn! {
        Node {
            height: px(36),
            padding: UiRect::all(px(5)),
            margin: UiRect::bottom(px(4)),
            flex_direction: FlexDirection::Row,
        }
        BackgroundColor(background)
        Children [ {inner} ]
    }
}

/// `DroneScreen`.
fn drone_screen(drone: &ActiveDrone, local: &UiLocalState) -> Box<dyn SceneList> {
    match &drone.screen {
        DroneScreen::Busy {
            wait_time_left,
            wait_message,
        } => {
            let caption = format!("{wait_message} ({})", format_time(*wait_time_left, false));
            Box::new(EntityScene(bsn! {
                @Section { @fill: true }
                Children [ {EntityScene(dimmer_screen("cog", UiColor::Good, caption))} ]
            }))
        }
        DroneScreen::Travel {
            travel_time_left, ..
        } => {
            let caption = format!("Travel Time: {}", format_time(*travel_time_left, false));
            Box::new(EntityScene(bsn! {
                @Section { @fill: true }
                Children [ {EntityScene(dimmer_screen("route", UiColor::Average, caption))} ]
            }))
        }
        DroneScreen::Idle { sites } => {
            Box::new(EntityScene(travel_screen(drone, sites, None, false)))
        }
        DroneScreen::Adventure(adventure) => Box::new(EntityScene(adventure_view(adventure))),
        DroneScreen::Exploration { sites, site, event } => match event {
            Some(event) => Box::new(EntityScene(event_view(event))),
            None if local.get(TRAVEL) == "1" => Box::new(EntityScene(travel_screen(
                drone,
                sites,
                Some(&**site),
                true,
            ))),
            None => Box::new(EntityScene(exploration_view(site))),
        },
    }
}

/// `TravelTargetSelectionScreen`.
fn travel_screen(
    drone: &ActiveDrone,
    sites: &[SiteInfo],
    here: Option<&SiteInfo>,
    cancel: bool,
) -> impl Scene + use<> {
    let coeff = drone.travel_coefficient;
    let travel_cost = |target: &SiteInfo| match here {
        Some(here) => f64::from(here.distance.abs_diff(target.distance).max(1)) * coeff,
        None => f64::from(target.distance) * coeff,
    };
    let launch = |target: Option<u32>| {
        let (icon, label) = if drone.can_travel {
            ("rocket", "Launch".to_owned())
        } else {
            ("triangle-exclamation", drone.travel_error.clone())
        };
        let params = ActParams(
            target
                .map(|t| vec![(TARGET_SITE.to_owned(), t.to_string())])
                .unwrap_or_default(),
        );
        let disabled = !drone.can_travel;
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: icon, @content: label }
            ss13_ui_kit::Button { disabled: disabled }
            Act({START_TRAVEL.to_owned()})
            params
            SetLocal { key: {TRAVEL.to_owned()}, value: {String::new()}, rerender: true }
        })
    };
    let buttons = cancel.then(|| {
        Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button { @content: "Cancel" }
            SetLocal { key: {TRAVEL.to_owned()}, value: {String::new()}, rerender: true }
        })) as Box<dyn SceneList>
    });
    let home = here.map(|here| {
        let eta = format!(
            "ETA: {} ",
            format_time(f64::from(here.distance) * coeff, true)
        );
        let button = launch(None);
        let header: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
            Children [ Text(eta) -- {button} ]
        }));
        EntityScene(bsn! { @Section { @title: "Home", @buttons: header } })
    });
    let destinations: Vec<_> = sites
        .iter()
        .filter(|s| here.is_none_or(|h| h.id != s.id))
        .map(|site| {
            let eta = format!("ETA: {} ", format_time(travel_cost(site), true));
            let button = launch(Some(site.id));
            let header: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                Children [ Text(eta) -- {button} ]
            }));
            let bands: Vec<_> = BANDS
                .iter()
                .filter_map(|band| {
                    let value = site
                        .band_info
                        .iter()
                        .find(|(b, _)| b == band)
                        .map(|(_, v)| *v)
                        .filter(|v| *v != 0.0)?;
                    let (label, value) = ((*band).to_owned(), numeric(value));
                    Some(bsn! { @LabeledListItem { @label: label, @content: value } })
                })
                .collect();
            let (title, coordinates, description) = (
                site.name.clone(),
                site.coordinates.clone(),
                site.description.clone(),
            );
            bsn! {
                @Section { @title: title, @buttons: header }
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem { @label: "Location", @content: coordinates }
                        --
                        @LabeledListItem { @label: "Description", @content: description }
                        --
                        @LabeledListDivider
                        --
                        {bands}
                    ]
                ]
            }
        })
        .collect();
    let no_sites = (sites.is_empty() && here.is_none()).then(|| {
        EntityScene(bsn! {
            @Dimmer
            Children [
                Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center }
                Children [
                    @Icon { @name: "map", @size: 8.0, @color: UiColor::Bad }
                    --
                    Node Propagate::<FontPx>(FontPx(18.0))
                    Propagate::<TextColor>(TextColor({UiColor::Bad.text().unwrap_or_default()}))
                    Children [ Text("No Destinations.") ]
                    --
                    Node Propagate::<TextColor>(TextColor({UiColor::Bad.text().unwrap_or_default()}))
                    Children [ Text("(Use the Scanner Array Console to find new locations.)") ]
                ]
            ]
        })
    });
    bsn! {
        @Section { @title: "Travel Destinations", @fill: true, @scrollable: true, @buttons: buttons }
        Children [ {no_sites} -- {home} -- {destinations} ]
    }
}

/// `ExplorationScreen`.
fn exploration_view(site: &SiteInfo) -> impl Scene + use<> {
    let (name, coordinates, description) = (
        site.name.clone(),
        site.coordinates.clone(),
        site.description.clone(),
    );
    let events: Vec<_> = site
        .events
        .iter()
        .map(|(name, id)| {
            let params = ActParams(vec![(TARGET_EVENT.to_owned(), id.to_string())]);
            let label = capitalize(name);
            bsn! {
                Node { justify_content: JustifyContent::Center, margin: UiRect::top(px(6)) }
                Children [
                    @ss13_ui_kit::Button { @content: label }
                    Act({EXPLORE_EVENT.to_owned()})
                    params
                ]
            }
        })
        .collect();
    bsn! {
        @Section { @title: "Exploration", @fill: true }
        Children [
            @LabeledList
            Children [
                @LabeledListItem { @label: "Site", @content: name }
                --
                @LabeledListItem { @label: "Location", @content: coordinates }
                --
                @LabeledListItem { @label: "Description", @content: description }
            ]
            --
            Node { justify_content: JustifyContent::Center, margin: UiRect::top(px(6)) }
            Children [
                @ss13_ui_kit::Button { @content: "Explore" }
                Act({EXPLORE.to_owned()})
                Name("exodrone: explore")
            ]
            --
            {events}
            --
            Node { justify_content: JustifyContent::Center, margin: UiRect::top(px(6)) }
            Children [
                @ss13_ui_kit::Button { @content: "Travel" }
                SetLocal { key: {TRAVEL.to_owned()}, value: {"1".to_owned()}, rerender: true }
                Name("exodrone: travel")
            ]
        ]
    }
}

/// `EventScreen`.
fn event_view(event: &EventScreen) -> impl Scene + use<> {
    let (description, action) = (event.description.clone(), event.action_text.clone());
    let disabled = !event.action_enabled;
    let skip = event.skippable.then(|| {
        let ignore = event.ignore_text.clone();
        EntityScene(bsn! {
            Node { justify_content: JustifyContent::Center, margin: UiRect::top(px(12)) }
            Children [
                @ss13_ui_kit::Button { @content: ignore }
                Act({SKIP_EVENT.to_owned()})
                Name("exodrone: skip event")
            ]
        })
    });
    bsn! {
        @Section { @title: "Exploration", @fill: true }
        Children [
            Node { padding: UiRect::all(px(8)), margin: UiRect::bottom(px(12)) }
            BackgroundColor({Color::srgba(1.0, 1.0, 1.0, 0.05)})
            Children [ Text(description) ]
            --
            Node { justify_content: JustifyContent::Center }
            Children [
                @ss13_ui_kit::Button { @content: action }
                ss13_ui_kit::Button { disabled: disabled }
                Act({START_EVENT.to_owned()})
                Name("exodrone: start event")
            ]
            --
            {skip}
        ]
    }
}

/// `AdventureScreen`.
fn adventure_view(adventure: &AdventureScreen) -> impl Scene + use<> {
    let description = adventure.description.clone();
    let choices: Vec<_> = adventure
        .choices
        .iter()
        .map(|(key, text)| {
            let params = ActParams(vec![(CHOICE.to_owned(), key.clone())]);
            let text = text.clone();
            bsn! {
                Node { margin: UiRect::bottom(px(4)) }
                Children [
                    @ss13_ui_kit::Button { @content: text, @fluid: true }
                    Act({ADVENTURE_CHOICE.to_owned()})
                    params
                ]
            }
        })
        .collect();
    bsn! {
        @Section { @title: "Exploration", @fill: true, @scrollable: true }
        Children [
            Node { padding: UiRect::all(px(8)), margin: UiRect::bottom(px(12)) }
            BackgroundColor({Color::srgba(1.0, 1.0, 1.0, 0.05)})
            Children [ Text(description) ]
            --
            Node { flex_direction: FlexDirection::Column } Children [ {choices} ]
        ]
    }
}

// --- Scanner console ------------------------------------------------------------------------

impl ClientUi for ExoscannerConsoleUi {
    const WIDTH: f32 = 550.0;
    const HEIGHT: f32 = 600.0;

    fn content(_: &(), data: &ExoscannerData) -> impl Scene {
        exoscanner_view(data)
    }
}

fn exoscanner_view(data: &ExoscannerData) -> impl Scene + use<> {
    let power: Box<dyn SceneList> = if data.scan_power > 0.0 {
        let power = numeric(data.scan_power);
        Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
            Children [
                Node { margin: UiRect::right(px(6)) } Propagate::<FontPx>(FontPx(24.0))
                Children [ Text(power) ]
                --
                @Icon { @name: "satellite-dish", @size: 3.0 }
            ]
        }))
    } else {
        Box::new(EntityScene(text_row(
            "No properly configured scanner arrays detected.".to_owned(),
        )))
    };
    let conditions: Vec<_> = data
        .scan_conditions
        .iter()
        .map(|c| {
            let c = c.clone();
            bsn! { @NoticeBox Children [ Text(c) ] }
        })
        .collect();
    let body: Box<dyn SceneList> = match &data.selected_site {
        Some(site) => Box::new(EntityScene(site_scans(data, site))),
        None => Box::new(EntityScene(scan_configuration(data))),
    };
    let progress = data.scan.as_ref().map(|s| EntityScene(scan_progress(s)));
    let failed = data.failed.then(|| {
        EntityScene(bsn! {
            @Modal
            Children [
                Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center }
                Children [
                    Node Propagate::<TextColor>(TextColor({UiColor::Bad.text().unwrap_or_default()}))
                    Children [ Text("SCAN FAILURE!") ]
                    --
                    @ss13_ui_kit::Button { @content: "Confirm" }
                    Act({CONFIRM_FAIL.to_owned()})
                    Name("exoscanner: confirm fail")
                ]
            ]
        })
    });
    bsn! {
        @WindowContent
        Children [
            Node { flex_direction: FlexDirection::Column, height: percent(100) }
            Children [
                @Section { @title: "Available array power" }
                Children [
                    {power}
                    --
                    @Section { @title: "Special Scan Condtions" } Children [ {conditions} ]
                ]
                --
                Node { flex_grow: 1.0, flex_direction: FlexDirection::Column }
                Children [ {body} ]
            ]
            --
            {progress}
            --
            {failed}
        ]
    }
}

/// The site list and the wide scan (no site selected).
fn scan_configuration(data: &ExoscannerData) -> impl Scene + use<> {
    let can_start = data.scan_power > 0.0;
    let cost = if can_start {
        format!("Cost estimate: {}", format_time(data.wide_scan_eta, true))
    } else {
        "Cost estimate: ∞ minutes".to_owned()
    };
    let scan: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @icon: "search", @content: "Scan" }
        ss13_ui_kit::Button { disabled: {!can_start} }
        Act({START_WIDE_SCAN.to_owned()})
        Name("exoscanner: wide scan")
    }));
    let sites: Vec<_> = data
        .possible_sites
        .iter()
        .map(|(id, name)| {
            let params = ActParams(vec![(SITE_REF.to_owned(), id.to_string())]);
            let name = name.clone();
            bsn! {
                Node { margin: UiRect::bottom(px(4)) }
                Children [
                    @ss13_ui_kit::Button { @content: name }
                    Act({SELECT_SITE.to_owned()})
                    params
                ]
            }
        })
        .collect();
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 }
        Children [
            @Section { @title: "Configure Wide Scan", @buttons: scan }
            Children [
                {EntityScene(text_row(
                    "Broad spectrum scan looking for anything not matching known start charts."
                        .to_owned(),
                ))}
                --
                Text(cost)
            ]
            --
            @Section {
                @title: "Configure Targeted Scans",
                @fill: true,
                @scrollable: true,
                @buttons: bsn! {
                    @ss13_ui_kit::Button { @icon: "tasks", @content: "View Experiments" }
                    Act({OPEN_EXPERIMENTS.to_owned()})
                    Name("exoscanner: experiments")
                },
            }
            Children [ {sites} ]
        ]
    }
}

/// `ScanSelectionSection`.
fn site_scans(data: &ExoscannerData, site: &SiteInfo) -> impl Scene + use<> {
    let back: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @content: "Back" }
        Act({SELECT_SITE.to_owned()})
        Name("exoscanner: back")
    }));
    let description = if site.revealed {
        site.description.clone()
    } else {
        "No Data".to_owned()
    };
    let (name, distance) = (site.name.clone(), site.distance.to_string());
    let bands: Vec<_> = site
        .band_info
        .iter()
        .map(|(band, value)| {
            let (band, value) = (band.clone(), numeric(*value));
            bsn! { @LabeledListItem { @label: band, @content: value } }
        })
        .collect();
    let powered = data.scan_power > 0.0;
    let scan_block = |title: &'static str, blurb: &'static str, eta: f64, act: &'static str| {
        let eta = if powered {
            format_time(eta, true)
        } else {
            "∞".to_owned()
        };
        let line = format!("Estimated Time: {eta}.");
        EntityScene(bsn! {
            @Section { @title: title }
            Children [
                {EntityScene(text_row(blurb.to_owned()))}
                --
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                Children [
                    @ss13_ui_kit::Button { @content: "Scan" }
                    ss13_ui_kit::Button { disabled: {!powered} }
                    Act({act.to_owned()})
                    --
                    Node { margin: UiRect::left(px(12)) } Children [ Text(line) ]
                ]
            ]
        })
    };
    let point = (!site.point_scan_complete).then(|| {
        scan_block(
            "Point Scan",
            "Point scan performs rudimentary scan of the site, revealing its general \
             characteristics.",
            data.point_scan_eta,
            START_POINT_SCAN,
        )
    });
    let deep = (!site.deep_scan_complete).then(|| {
        scan_block(
            "Deep Scan",
            "Deep scan performs full scan of the site, revealing all details.",
            data.deep_scan_eta,
            START_DEEP_SCAN,
        )
    });
    let scans = (!site.point_scan_complete || !site.deep_scan_complete)
        .then(|| EntityScene(bsn! { @Section { @title: "Scans" } Children [ {point} -- {deep} ] }));
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 }
        Children [
            @Section { @title: "Site Data", @fill: true, @buttons: back }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem { @label: "Name", @content: name }
                    --
                    @LabeledListItem { @label: "Description", @content: description }
                    --
                    @LabeledListItem { @label: "Distance", @content: distance }
                    --
                    @LabeledListDivider
                    --
                    @LabeledListItem { @label: "Spectrography Data" }
                    --
                    @LabeledListDivider
                    --
                    {bands}
                ]
            ]
            --
            {scans}
        ]
    }
}

/// `ScanInProgressModal`.
fn scan_progress(scan: &ScanProgress) -> impl Scene + use<> {
    let (description, time, power) = (
        scan.description.clone(),
        format_time(scan.time_left, false),
        numeric(scan.power),
    );
    bsn! {
        @Modal
        Children [
            @NoticeBox Children [ Text("Scan in Progress!") ]
            --
            @LabeledList
            Children [
                @LabeledListItem { @label: "Scan summary", @content: description }
                --
                @LabeledListItem { @label: "Time left", @content: time }
                --
                @LabeledListItem { @label: "Scanning array power", @content: power }
                --
                @LabeledListItem { @label: "Emergency Stop" }
                Children [
                    @ss13_ui_kit::Button { @icon: "times", @content: "STOP SCAN" }
                    ss13_ui_kit::Button { color: UiColor::Bad }
                    Act({STOP_SCAN.to_owned()})
                    Name("exoscanner: stop scan")
                ]
            ]
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(format_time(125.0, true), "2m5s");
        assert_eq!(format_time(3725.0, false), "01:02:05");
        assert_eq!(format_time(0.0, true), "");
        assert_eq!(capitalize("drill"), "Drill");
    }
}
