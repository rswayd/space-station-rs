//! UI gallery: every tgui window the client knows, opened from a list with sample data
//! (`ss13_tgui::samples`), without a server. Buttons in the opened windows do nothing but are
//! shown in the list window's last-action line (and logged).
//!
//! `cargo run -p ss13_client --bin ui_gallery` opens the list;
//! `-- Canister` also opens that window at once;
//! `-- Canister --screenshot out.png` saves a PNG after it settles and exits.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use serde::{Deserialize, Serialize};
use ss13_net::client::{NetClientState, SendIntent};
use ss13_net::ui::client::{SendUi, UiMessageReceived};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::{UiServerMessage, UiStatus};
use ss13_tgui::client::{AppTguiClientExt, ClientUi, TguiClientPlugin, view_names};
use ss13_tgui::samples::sample;
use ss13_tgui::{UiInterface, encode};
use ss13_ui_kit::UiKitPlugin;

/// The list window's id; opened windows count up from it.
const LIST_WINDOW: u32 = 1;
const OPEN: &str = "open";
const CLOSE_ALL: &str = "close_all";

/// The list window.
struct GalleryUi;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
struct GalleryData {
    names: Vec<String>,
    /// Views without a sample (they open with nothing to show).
    missing: Vec<String>,
    last_act: String,
}

impl UiInterface for GalleryUi {
    const NAME: &'static str = "UiGallery";
    type Data = GalleryData;
    type StaticData = ();
}

impl ClientUi for GalleryUi {
    const WIDTH: f32 = 330.0;
    const HEIGHT: f32 = 640.0;

    fn content(_: &(), data: &GalleryData) -> impl Scene {
        use ss13_ui_kit::prelude::*;

        let buttons: Vec<_> = data
            .names
            .iter()
            .map(|name| {
                let params = ActParams(vec![("name".to_owned(), name.clone())]);
                let content = if data.missing.contains(name) {
                    format!("{name} (нет образца)")
                } else {
                    name.clone()
                };
                bsn! {
                    @ss13_ui_kit::Button { @content: content, @fluid: true }
                    Act(OPEN)
                    params
                }
            })
            .collect();
        let last = if data.last_act.is_empty() {
            "—".to_owned()
        } else {
            data.last_act.clone()
        };
        bsn! {
            @WindowContent { @scrollable: true }
            Children [
                @Section {
                    @title: "Окна",
                    @buttons: bsn! {
                        @ss13_ui_kit::Button { @icon: "times", @content: "Закрыть все" }
                        Act(CLOSE_ALL)
                    },
                }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
                    Children [ {buttons} ]
                ]
                --
                @Section { @title: "Последнее действие" }
                Children [ Text(last) ]
            ]
        }
    }
}

#[derive(Resource)]
struct Gallery {
    data: GalleryData,
    next_window: u32,
    opened: Vec<u32>,
    /// Interface name of each opened window, for the act log.
    names: Vec<(u32, String)>,
}

#[derive(Resource)]
struct Shot {
    path: Option<String>,
    frames: u32,
}

fn open_list(gallery: &Gallery, out: &mut MessageWriter<UiMessageReceived>) {
    out.write(UiMessageReceived(UiServerMessage::Open {
        window: LIST_WINDOW,
        interface: GalleryUi::NAME.into(),
        title: "UI gallery".into(),
        status: UiStatus::Interactive,
        static_data: encode(&()),
        data: encode(&gallery.data),
    }));
}

fn open_view(gallery: &mut Gallery, name: &str, out: &mut MessageWriter<UiMessageReceived>) {
    let window = gallery.next_window;
    gallery.next_window += 1;
    let (title, static_data, data) = match sample(name) {
        Some(s) => (s.title, s.static_data, s.data),
        None => (name.to_owned(), Vec::new(), Vec::new()),
    };
    gallery.opened.push(window);
    gallery.names.push((window, name.to_owned()));
    out.write(UiMessageReceived(UiServerMessage::Open {
        window,
        interface: name.into(),
        title,
        status: UiStatus::Interactive,
        static_data,
        data,
    }));
}

fn setup(
    mut commands: Commands,
    world_names: Res<ViewNames>,
    initial: Res<Initial>,
    mut positions: ResMut<ss13_tgui::client::WindowPositions>,
    mut out: MessageWriter<UiMessageReceived>,
) {
    commands.spawn(Camera2d);
    // The list sits at the left edge; the windows it opens are centered (tgui's default).
    positions
        .0
        .insert(GalleryUi::NAME.to_owned(), Vec2::new(16.0, 16.0));
    let names: Vec<String> = world_names
        .0
        .iter()
        .filter(|n| **n != GalleryUi::NAME)
        .map(|n| (*n).to_owned())
        .collect();
    let missing = names
        .iter()
        .filter(|n| sample(n).is_none())
        .cloned()
        .collect();
    let mut gallery = Gallery {
        data: GalleryData {
            names,
            missing,
            last_act: String::new(),
        },
        next_window: LIST_WINDOW + 1,
        opened: Vec::new(),
        names: Vec::new(),
    };
    open_list(&gallery, &mut out);
    if let Some(name) = &initial.0 {
        open_view(&mut gallery, name, &mut out);
    }
    commands.insert_resource(gallery);
}

/// The acts of every window: the list's open views, the rest are only shown.
fn handle_acts(
    mut sent: MessageReader<SendIntent>,
    mut gallery: ResMut<Gallery>,
    mut out: MessageWriter<UiMessageReceived>,
) {
    let mut changed = false;
    for SendIntent(intent) in sent.read() {
        let ClientIntent::UiAct {
            window,
            action,
            params,
        } = intent
        else {
            continue;
        };
        if *window == LIST_WINDOW {
            match action.as_str() {
                OPEN => {
                    if let Some((_, name)) = params.iter().find(|(k, _)| k == "name") {
                        let name = name.clone();
                        open_view(&mut gallery, &name, &mut out);
                    }
                }
                CLOSE_ALL => {
                    for window in std::mem::take(&mut gallery.opened) {
                        out.write(UiMessageReceived(UiServerMessage::Close { window }));
                    }
                    gallery.names.clear();
                }
                _ => {}
            }
            continue;
        }
        let name = gallery
            .names
            .iter()
            .find(|(w, _)| w == window)
            .map_or("?", |(_, n)| n.as_str());
        let text = if params.is_empty() {
            format!("{name}: {action}")
        } else {
            let params: Vec<String> = params.iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!("{name}: {action} ({})", params.join(", "))
        };
        info!("act: {text}");
        gallery.data.last_act = text;
        changed = true;
    }
    if changed {
        out.write(UiMessageReceived(UiServerMessage::Update {
            window: LIST_WINDOW,
            status: UiStatus::Interactive,
            data: Some(encode(&gallery.data)),
        }));
    }
}

/// A window closed by its close button is no longer "opened".
fn forget_closed(mut sent: MessageReader<SendUi>, mut gallery: ResMut<Gallery>) {
    for SendUi(message) in sent.read() {
        let ss13_shared::ui::UiClientMessage::Close { window } = message;
        gallery.opened.retain(|w| w != window);
        gallery.names.retain(|(w, _)| w != window);
    }
}

fn screenshot(mut shot: ResMut<Shot>, mut commands: Commands, mut exit: MessageWriter<AppExit>) {
    let Some(path) = shot.path.clone() else {
        return;
    };
    shot.frames += 1;
    if shot.frames == 30 {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    if shot.frames == 40 {
        exit.write(AppExit::Success);
    }
}

#[derive(Resource)]
struct ViewNames(Vec<&'static str>);

#[derive(Resource)]
struct Initial(Option<String>);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args
        .iter()
        .skip_while(|a| *a != "--screenshot")
        .nth(1)
        .cloned();
    let initial = args
        .iter()
        .take_while(|a| *a != "--screenshot")
        .find(|a| !a.starts_with("--"))
        .cloned();
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(bevy::window::Window {
                title: "ss13: UI gallery".into(),
                resolution: (1280, 800).into(),
                ..default()
            }),
            ..default()
        }),
        UiKitPlugin,
        TguiClientPlugin,
    ))
    .register_ui_view::<GalleryUi>();
    let names = view_names(app.world());
    app.insert_resource(ViewNames(names))
        .insert_resource(Initial(initial))
        .insert_resource(NetClientState::Connected)
        .insert_resource(Shot { path, frames: 0 })
        .add_message::<UiMessageReceived>()
        .add_message::<SendUi>()
        .add_message::<SendIntent>()
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_acts, forget_closed, screenshot));
    app.run();
}
