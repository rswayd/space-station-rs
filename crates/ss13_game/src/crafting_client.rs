//! Client half of the crafting menu ([`crate::crafting`]): tg's `craft` button
//! (`icons/hud/screen_midnight.dmi`) at `ui_human_crafting` (`EAST-3:24,SOUTH+1:7`), whose click
//! sends the button window's [`OPEN`] act, and the `PersonalCrafting` window.
// shortcut: the window is a list by tab (tgui's search, icons, material tab and food filters are
// not drawn); the button is a text label when the atlas lacks `craft`.

use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::scene::{EntityScene, SceneList};
use ss13_net::client::SendIntent;
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, IconStateId};
use ss13_tgui::UiInterface;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use crate::crafting::*;
use crate::inventory_client::{ScreenAxis, screen_loc};

const STYLE_DMI: &str = "icons/hud/screen_midnight.dmi";
const BUTTON_STATE: &str = "craft";
/// The picked tab (tgui `useState(activeCategory)`).
const LOCAL_TAB: &str = "tab";

pub struct CraftingClientPlugin;

impl Plugin for CraftingClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<CraftButtonUi>()
            .register_ui_view::<PersonalCraftingUi>()
            .init_resource::<CraftButtonHud>()
            .add_systems(Startup, spawn_button)
            .add_systems(
                Update,
                (
                    receive,
                    draw_button.run_if(resource_changed::<CraftButtonHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's button window.
#[derive(Resource, Default, Debug)]
pub struct CraftButtonHud {
    pub window: Option<u32>,
}

#[derive(Component)]
struct ButtonRoot;

#[derive(Component)]
struct CraftButtonNode;

/// The `craft` state, looked up once.
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
            Name::new("crafting menu"),
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
            CraftButtonNode,
            Name::new("crafting menu button"),
            screen_loc(ScreenAxis(1, -3, 24), ScreenAxis(-1, 1, 7)),
            ImageNode::default(),
            Visibility::Hidden,
            ChildOf(root),
        ))
        .observe(click_button);
}

fn receive(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<CraftButtonHud>) {
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window, interface, ..
            } if interface == CraftButtonUi::NAME => hud.window = Some(*window),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                hud.window = None;
            }
            _ => {}
        }
    }
}

fn draw_button(
    hud: Res<CraftButtonHud>,
    state: Res<ButtonState>,
    mut atlas: Option<ResMut<IconAtlas>>,
    button: Single<(Entity, &mut Visibility, &mut ImageNode), With<CraftButtonNode>>,
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
                .with_child((Text::new("Крафт"), Pickable::IGNORE));
        }
    }
}

/// tg `/atom/movable/screen/craft` click (`component_ui_interact`).
fn click_button(
    click: On<PointerClick>,
    hud: Res<CraftButtonHud>,
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

impl ClientUi for PersonalCraftingUi {
    const WIDTH: f32 = 700.0;
    const HEIGHT: f32 = 720.0;

    fn content(st: &CraftingStatic, data: &CraftingState) -> impl Scene {
        view(st, data, &UiLocalState::default())
    }

    fn content_local(
        st: &CraftingStatic,
        data: &CraftingState,
        local: &UiLocalState,
    ) -> impl Scene {
        view(st, data, local)
    }
}

fn toggle(label: &str, on: bool, action: &str) -> impl Scene + use<> {
    let icon = if on { "check-square-o" } else { "square-o" };
    let (label, action) = (label.to_owned(), action.to_owned());
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @content: label }
        ss13_ui_kit::Button { selected: on }
        Act(action)
    }
}

fn mode_button(label: &str, icon: &str, selected: bool) -> impl Scene + use<> {
    let (label, icon) = (label.to_owned(), icon.to_owned());
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @content: label }
        ss13_ui_kit::Button { selected: selected, disabled: selected }
        Act({TOGGLE_MODE.to_owned()})
    }
}

fn line(text: String) -> impl Scene + use<> {
    bsn! { Text(text) TextColor({UiColor::Label.base()}) }
}

fn recipe_row(r: &RecipeView, data: &CraftingState) -> impl Scene + use<> {
    let can = data.can_make(&r.path);
    let disabled = !can || data.busy;
    let make_params = ActParams(vec![(RECIPE.to_owned(), r.path.clone())]);
    let mass_params = ActParams(vec![(RECIPE.to_owned(), r.path.clone())]);
    let make = (!r.non_craftable).then(|| {
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "hammer", @content: "Создать" }
            ss13_ui_kit::Button { disabled: disabled }
            Act({MAKE.to_owned()})
            make_params
        })
    });
    let mass = (!r.non_craftable && r.mass_craftable).then(|| {
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "layer-group", @content: "Создать все" }
            ss13_ui_kit::Button { disabled: disabled }
            Act({MAKE_MASS.to_owned()})
            mass_params
        })
    });
    let details: Vec<_> = details(r, data.display_compact)
        .into_iter()
        .map(line)
        .collect();
    let name = r.name.clone();
    let color = if can { UiColor::Good } else { UiColor::White };
    bsn! {
        Node { flex_direction: FlexDirection::Column, margin: UiRect::bottom(px(8)) }
        Children [
            Node { align_items: AlignItems::Center, column_gap: px(4) }
            Children [
                Node { flex_grow: 1.0 } Children [ Text(name) TextColor({color.base()}) ]
                --
                {make}
                --
                {mass}
            ]
            --
            Node { flex_direction: FlexDirection::Column, margin: UiRect::left(px(8)) }
            Children [ {details} ]
        ]
    }
}

fn view(st: &CraftingStatic, data: &CraftingState, local: &UiLocalState) -> impl Scene + use<> {
    let picked = local.get(LOCAL_TAB);
    let tab = if st.tabs.iter().any(|t| t == picked) {
        picked.to_owned()
    } else {
        CAN_MAKE_TAB.to_owned()
    };
    let tabs: Vec<_> = std::iter::once(CAN_MAKE_TAB.to_owned())
        .chain(st.tabs.iter().cloned())
        .map(|name| {
            let pick = SetLocal {
                key: LOCAL_TAB.into(),
                value: name.clone(),
                rerender: true,
            };
            let selected = name == tab;
            bsn! {
                @TabsTab { @content: name }
                TabsTab { selected: selected }
                pick
            }
        })
        .collect();
    let rows: Vec<_> = shown(st, data, &tab)
        .into_iter()
        .map(|r| recipe_row(r, data))
        .collect();
    let empty = rows
        .is_empty()
        .then(|| EntityScene(bsn! { @NoticeBox Children [ Text("Не найдены рецепты.") ] }));
    let crafting = EntityScene(mode_button("Создание", "hammer", !data.cooking));
    let cooking = EntityScene(mode_button("Готовка", "utensils", data.cooking));
    let modes = (!st.forced_mode).then(|| {
        EntityScene(bsn! {
            Node { column_gap: px(4), margin: UiRect::bottom(px(6)) }
            Children [ {crafting} -- {cooking} ]
        })
    });
    let title = if data.cooking {
        "Готовка"
    } else {
        "Создание"
    };
    let only = EntityScene(toggle(
        "Можно сделать сейчас",
        data.display_craftable_only,
        TOGGLE_RECIPES,
    ));
    let compact = EntityScene(toggle(
        "Компактный список",
        data.display_compact,
        TOGGLE_COMPACT,
    ));
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { column_gap: px(4) }
        Children [ {only} -- {compact} ]
    }));
    let busy: Box<dyn Scene> = if data.busy {
        Box::new(bsn! {
            @Dimmer
            Children [
                Node { align_items: AlignItems::Center }
                Children [
                    @Icon { @name: "cog", @spin: true, @size: 2.0 }
                    --
                    Text(" Создание...")
                ]
            ]
        })
    } else {
        Box::new(bsn! { Node })
    };
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 }
        Children [
            @WindowContent { @scrollable: true }
            Children [
                {modes}
                --
                @Section { @title: title, @buttons: {Some(buttons)} }
                Children [
                    Node { flex_direction: FlexDirection::Row, column_gap: px(6) }
                    Children [
                        Node { min_width: px(170) }
                        Children [ @Tabs { @vertical: true } Children [ {tabs} ] ]
                        --
                        Node { flex_grow: 1.0, flex_direction: FlexDirection::Column }
                        Children [ {empty} -- {rows} ]
                    ]
                ]
            ]
            --
            {EntityScene(busy)}
        ]
    }
}
