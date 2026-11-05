//! Client half of the language menu ([`crate::language_menu`]): tg's `talk_wheel` button
//! (`icons/hud/screen_midnight.dmi`) at `ui_human_language` (`EAST-3:7,SOUTH+1:24`), whose click
//! sends the button window's [`OPEN`] act, and the `LanguageMenu` window.
// shortcut: no hover tooltips on the names, speak icons and keys of the table; the button is a
// text label when the atlas lacks `talk_wheel`.

use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::scene::EntityScene;
use ss13_net::client::SendIntent;
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, IconStateId};
use ss13_tgui::UiInterface;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;

use crate::inventory_client::{ScreenAxis, screen_loc};
use crate::language_menu::*;

const STYLE_DMI: &str = "icons/hud/screen_midnight.dmi";
const BUTTON_STATE: &str = "talk_wheel";

pub struct LanguageMenuClientPlugin;

impl Plugin for LanguageMenuClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<LanguageButtonUi>()
            .register_ui_view::<LanguageMenuUi>()
            .init_resource::<LanguageButtonHud>()
            .add_systems(Startup, spawn_button)
            .add_systems(
                Update,
                (
                    receive,
                    draw_button.run_if(resource_changed::<LanguageButtonHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's button window.
#[derive(Resource, Default, Debug)]
pub struct LanguageButtonHud {
    pub window: Option<u32>,
}

#[derive(Component)]
struct ButtonRoot;

#[derive(Component)]
struct LanguageButtonNode;

/// The `talk_wheel` state, looked up once.
#[derive(Resource, Default)]
struct ButtonState(Option<IconStateId>);

fn spawn_button(atlas: Option<Res<IconAtlas>>, mut commands: Commands) {
    let state = atlas
        .as_ref()
        .and_then(|a| a.manifest.index().get(&(STYLE_DMI, BUTTON_STATE)).copied());
    commands.insert_resource(ButtonState(state));
    let root = commands
        .spawn((
            ButtonRoot,
            Name::new("language menu"),
            Node {
                position_type: PositionType::Absolute,
                top: px(0),
                bottom: px(0),
                left: px(0),
                right: px(0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    commands
        .spawn((
            LanguageButtonNode,
            Name::new("language menu button"),
            screen_loc(ScreenAxis(1, -3, 7), ScreenAxis(-1, 1, 24)),
            ImageNode::default(),
            Visibility::Hidden,
            ChildOf(root),
        ))
        .observe(click_button);
}

fn receive(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<LanguageButtonHud>) {
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window, interface, ..
            } if interface == LanguageButtonUi::NAME => hud.window = Some(*window),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                hud.window = None;
            }
            _ => {}
        }
    }
}

fn draw_button(
    hud: Res<LanguageButtonHud>,
    state: Res<ButtonState>,
    mut atlas: Option<ResMut<IconAtlas>>,
    button: Single<(Entity, &mut Visibility, &mut ImageNode), With<LanguageButtonNode>>,
    mut commands: Commands,
) {
    let (entity, mut visibility, mut image) = button.into_inner();
    if hud.window.is_none() {
        *visibility = Visibility::Hidden;
        return;
    }
    *visibility = Visibility::Inherited;
    let drawn = state.0.and_then(|id| {
        let (info, handle) = atlas.as_deref_mut()?.state(id)?;
        Some(ImageNode {
            image: handle,
            rect: frame_rect(info, Dir::South, 0),
            ..default()
        })
    });
    match drawn {
        Some(drawn) => *image = drawn,
        None => {
            commands
                .entity(entity)
                .despawn_related::<Children>()
                .with_child((Text::new("Языки"), Pickable::IGNORE));
        }
    }
}

/// tg `/atom/movable/screen/language_menu/Click`.
fn click_button(
    click: On<PointerClick>,
    hud: Res<LanguageButtonHud>,
    mut intents: MessageWriter<SendIntent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    if let Some(window) = hud.window {
        intents.write(SendIntent(ClientIntent::UiAct {
            window,
            action: OPEN.into(),
            params: Vec::new(),
        }));
    }
}

/// The button stays on the map viewport, left of the docked chat.
fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<ButtonRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}

impl ClientUi for LanguageMenuUi {
    const WIDTH: f32 = 500.0;
    const HEIGHT: f32 = 500.0;

    /// `height = min(shown * 25 + 70, 500)`.
    fn size(_: &(), data: &LanguageMenuData) -> (f32, f32) {
        let rows = data.shown().len() as f32;
        (Self::WIDTH, (rows * 25.0 + 70.0).min(Self::HEIGHT))
    }

    fn content(_: &(), data: &LanguageMenuData) -> impl Scene {
        let is_living = data.is_living;
        let rows: Vec<_> = data
            .shown()
            .into_iter()
            .map(|l| {
                // `LangSpeakIcon`: could and can speak, could not, only knows it, neither.
                let speak = match (l.could_speak, l.can_speak) {
                    (true, true) => UiColor::Good,
                    (true, false) => UiColor::Bad,
                    (false, true) => UiColor::Average,
                    (false, false) => UiColor::Grey,
                };
                let understand: Box<dyn bevy::scene::SceneList> =
                    if !l.can_understand && l.partial_understanding > 0 {
                        let text = format!("{}%", l.partial_understanding);
                        Box::new(EntityScene(bsn! { @TableCell { @content: text } }))
                    } else {
                        let color = if l.can_understand {
                            UiColor::Good
                        } else {
                            UiColor::Bad
                        };
                        Box::new(EntityScene(bsn! {
                            @TableCell
                            Children [ @Icon { @name: "brain", @color: color } ]
                        }))
                    };
                let key = l.key.map_or("N/A".to_owned(), |k| format!(",{k}"));
                let default = is_living.then(|| {
                    let color = if l.is_default {
                        UiColor::Good
                    } else if l.could_speak && l.can_speak {
                        UiColor::Grey
                    } else {
                        UiColor::Transparent
                    };
                    let icon = if l.is_default {
                        "check-square-o"
                    } else {
                        "square-o"
                    };
                    let selected = l.is_default;
                    let params = ActParams(vec![(LANGUAGE_NAME.to_owned(), l.name.clone())]);
                    let node = format!("language: {}", l.name);
                    EntityScene(bsn! {
                        @TableCell
                        Children [
                            @ss13_ui_kit::Button { @icon: icon }
                            ss13_ui_kit::Button { color: color, selected: selected }
                            Act({SELECT_DEFAULT.to_owned()})
                            params
                            Name(node)
                        ]
                    })
                });
                let (name, desc) = (l.name.clone(), l.desc.clone());
                bsn! {
                    @TableRow
                    Children [
                        @TableCell
                        Tooltip { content: desc }
                        Children [ Text(name) ]
                        --
                        @TableCell
                        Children [ @Icon { @name: "comment", @color: speak } ]
                        --
                        {understand}
                        --
                        @TableCell { @content: key }
                        --
                        {default}
                    ]
                }
            })
            .collect();
        let default_header =
            is_living.then(|| EntityScene(bsn! { @TableCell { @content: "Default" } }));
        bsn! {
            @WindowContent
            Children [
                @Section { @fill: true, @scrollable: true }
                Children [
                    @Table
                    Children [
                        @TableRow { @header: true }
                        Children [
                            @TableCell { @content: "Name" }
                            --
                            @TableCell { @content: "Speak" }
                            --
                            @TableCell { @content: "Understand" }
                            --
                            @TableCell { @content: "Key" }
                            --
                            {default_header}
                        ]
                        --
                        {rows}
                    ]
                ]
            ]
        }
    }
}
