//! Client half of the antagonists: the `Uplink` window (tg `Uplink/index.tsx`, `GenericUplink`,
//! `PrimaryObjectiveMenu`), the `AntagInfoTraitor` and `AntagInfoBrother` panels, and the
//! headless `AntagHud`: the antag info action button at the top left (tg
//! `/datum/action/antag_info`, `round_end` on the HUD's `template` background) and the antag HUD
//! icon drawn over the own mob and the team mates the server lists (tg `antag_hud` and the team's
//! `has_antagonist` HUD: images only the antagonist's client gets).
// shortcut: no window themes (`syndicate`, `ntos`), no search field or compact mode in the
// uplink, and the telecrystal button (`buy_raw_tc`) does nothing (no telecrystal items yet).

use bevy::app::Propagate;
use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::scene::EntityScene;
use ss13_net::client::{NetEntityMap, SendIntent};
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets, RenderSystems};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{ABOVE_LIGHTING_PLANE, Appearance, Dir, IconStateId, Overlay};
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState, UiSprite};
use ss13_tgui::{UiInterface, decode};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use super::{
    AntagHudData, AntagHudUi, AntagInfoBrainwashedUi, AntagInfoBrotherUi, AntagInfoTraitorUi, BUY,
    BUY_RAW_TC, BrotherInfo, LOCK, OPEN_INFO, RENEGOTIATE_OBJECTIVES, TraitorInfo, UplinkData,
    UplinkItemRecord, UplinkStatic, UplinkUi,
};

/// One 32 px HUD tile at the map's zoom 2.
const TILE_PX: f32 = 64.0;
/// tg `get_action_buttons_icons` of the midnight style, and the antag info button's icon.
const BUTTON_BG: (&str, &str) = ("icons/hud/screen_midnight.dmi", "template");
const BUTTON_ICON: (&str, &str) = ("icons/hud/actions.dmi", "round_end");
/// `useState` keys of the uplink: the tab (`"0"` objectives, `"2"` market) and the category.
const TAB: &str = "tab";
const CATEGORY: &str = "category";

pub struct AntagClientPlugin;

impl Plugin for AntagClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<UplinkUi>()
            .register_ui_view::<AntagInfoTraitorUi>()
            .register_ui_view::<AntagInfoBrotherUi>()
            .register_ui_view::<AntagInfoBrainwashedUi>()
            .register_ui_headless::<AntagHudUi>()
            .init_resource::<AntagHud>()
            .add_systems(Startup, spawn_button)
            .add_systems(
                Update,
                (
                    receive_hud,
                    draw_button.run_if(resource_changed::<AntagHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            )
            .add_systems(PostUpdate, hud_overlay.before(RenderSystems));
    }
}

/// The server's antag HUD window and its last data; `window` is `None` for a non-antagonist.
#[derive(Resource, Default, Debug)]
pub struct AntagHud {
    pub window: Option<u32>,
    pub data: AntagHudData,
}

#[derive(Component)]
struct ButtonRoot;

#[derive(Component)]
struct InfoButton;

#[derive(Component)]
struct ButtonIcon;

fn spawn_button(mut commands: Commands) {
    let root = commands
        .spawn((
            ButtonRoot,
            Name::new("antag hud"),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    // tg `ui_action_palette`-adjacent first action slot, "WEST:6,NORTH:-6" at zoom 2.
    let button = commands
        .spawn((
            InfoButton,
            Name::new("antag info"),
            Node {
                position_type: PositionType::Absolute,
                width: px(TILE_PX),
                height: px(TILE_PX),
                left: px(12),
                top: px(12),
                ..default()
            },
            ImageNode::default(),
            Visibility::Hidden,
            ChildOf(root),
        ))
        .observe(click_button)
        .id();
    commands.spawn((
        ButtonIcon,
        Node {
            width: percent(100),
            height: percent(100),
            ..default()
        },
        ImageNode::default(),
        Pickable::IGNORE,
        ChildOf(button),
    ));
}

fn click_button(
    click: On<PointerClick>,
    hud: Res<AntagHud>,
    mut intents: MessageWriter<SendIntent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    if let Some(window) = hud.window {
        intents.write(SendIntent(ClientIntent::UiAct {
            window,
            action: OPEN_INFO.to_owned(),
            params: Vec::new(),
        }));
    }
}

fn receive_hud(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<AntagHud>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == AntagHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = AntagHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<AntagHudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable antag HUD data: {e}"),
            }
        }
    }
}

fn draw_button(
    hud: Res<AntagHud>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut buttons: Query<(&mut ImageNode, &mut Visibility), (With<InfoButton>, Without<ButtonIcon>)>,
    mut icons: Query<&mut ImageNode, With<ButtonIcon>>,
) {
    let mut image = |(dmi, state): (&str, &str)| {
        let atlas = atlas.as_deref_mut()?;
        let id = atlas.manifest.index().get(&(dmi, state)).copied()?;
        let (info, handle) = atlas.state(id)?;
        Some(ImageNode {
            image: handle,
            rect: frame_rect(info, Dir::South, 0),
            ..default()
        })
    };
    let (bg, icon) = match hud.window {
        Some(_) => (image(BUTTON_BG), image(BUTTON_ICON)),
        None => (None, None),
    };
    for (mut node, mut visibility) in &mut buttons {
        *visibility = if hud.window.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some(bg) = &bg {
            *node = bg.clone();
        }
    }
    if let Some(icon) = icon {
        for mut node in &mut icons {
            *node = icon.clone();
        }
    }
}

fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<ButtonRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}

/// The antag HUD icon on the own mob and the listed team mates: an overlay above the lighting,
/// added again whenever a replicated appearance replaced it, and taken off when the server no
/// longer lists it (a deconversion). Cost: the overlay lists of the own mob and the team mates in
/// view (a few) per frame.
fn hud_overlay(
    hud: Res<AntagHud>,
    map: Res<NetEntityMap>,
    mut mobs: Query<&mut Appearance>,
    mut drawn: Local<Vec<(Entity, IconStateId)>>,
) {
    let mut wanted: Vec<(Entity, IconStateId)> = Vec::new();
    if let Some(icon) = hud.data.hud_icon {
        let mates = hud.data.team.iter().filter_map(|&id| map.get(id));
        wanted.extend(map.controlled().into_iter().chain(mates).map(|m| (m, icon)));
    }
    wanted.extend(
        hud.data
            .team_icons
            .iter()
            .filter_map(|&(id, icon)| Some((map.get(id)?, icon))),
    );
    for &(mob, icon) in drawn.iter() {
        if !wanted.contains(&(mob, icon))
            && let Ok(mut appearance) = mobs.get_mut(mob)
        {
            appearance
                .overlays
                .retain(|o| o.icon_state != icon || o.plane != Some(ABOVE_LIGHTING_PLANE));
        }
    }
    for &(mob, icon) in &wanted {
        let Ok(mut appearance) = mobs.get_mut(mob) else {
            continue;
        };
        if !appearance.overlays.iter().any(|o| o.icon_state == icon) {
            appearance.overlays.push(Overlay {
                plane: Some(ABOVE_LIGHTING_PLANE),
                dir: Some(Dir::South),
                ..Overlay::new(icon)
            });
        }
    }
    *drawn = wanted;
}

impl ClientUi for UplinkUi {
    const WIDTH: f32 = 700.0;
    const HEIGHT: f32 = 600.0;

    fn content(st: &UplinkStatic, data: &UplinkData) -> impl Scene {
        uplink_view(st, data, &UiLocalState::default())
    }

    fn content_local(st: &UplinkStatic, data: &UplinkData, local: &UiLocalState) -> impl Scene {
        uplink_view(st, data, local)
    }
}

fn uplink_view(st: &UplinkStatic, data: &UplinkData, local: &UiLocalState) -> impl Scene + use<> {
    let has_objectives = !data.primary_objectives.is_empty();
    let market = !has_objectives || local.get(TAB) == "2";
    let tabs = has_objectives.then(|| {
        EntityScene(bsn! {
            @StackItem { @grow: 1.0 }
            Children [
                @Tabs { @fluid: true }
                Children [
                    @TabsTab { @content: "Основные задачи", @icon: "star" }
                    TabsTab { selected: {!market} }
                    SetLocal { key: {TAB.to_owned()}, value: {"0".to_owned()}, rerender: true }
                    --
                    @TabsTab { @content: "Рынок", @icon: "store" }
                    TabsTab { selected: market }
                    SetLocal { key: {TAB.to_owned()}, value: {"2".to_owned()}, rerender: true }
                ]
            ]
        })
    });
    let lock = st.lockable.then(|| {
        EntityScene(bsn! {
            @StackItem
            Children [
                @ss13_ui_kit::Button { @content: "Закрыть", @icon: "lock" }
                ss13_ui_kit::Button { color: UiColor::Transparent }
                Act({LOCK.to_owned()})
            ]
        })
    });
    let body: Box<dyn SceneList> = if market {
        Box::new(EntityScene(market_view(st, data, local)))
    } else {
        Box::new(EntityScene(objectives_view(data)))
    };
    bsn! {
        @WindowContent
        Children [
            @Stack { @vertical: true, @fill: true }
            Children [
                @StackItem
                Children [
                    @Section { @fitted: true }
                    Children [
                        @Stack
                        Children [ {tabs} -- {lock} ]
                    ]
                ]
                --
                @StackItem { @grow: 1.0 }
                Node { min_height: px(0) }
                Children [ {body} ]
            ]
        ]
    }
}

/// `PrimaryObjectiveMenu`.
fn objectives_view(data: &UplinkData) -> impl Scene + use<> {
    let objectives: Vec<_> = data
        .primary_objectives
        .iter()
        .map(|(name, text)| {
            let (name, text) = (name.clone(), text.clone());
            bsn! {
                Node { flex_direction: FlexDirection::Column, margin: UiRect::bottom(px(8)) }
                Children [
                    Node { padding: UiRect::all(px(4)) }
                    BackgroundColor(Color::srgb(0.35, 0.05, 0.05))
                    Propagate::<Bold>(Bold(true))
                    Children [ Text(name) ]
                    --
                    Node { padding: UiRect::all(px(4)) }
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.4))
                    Children [ Text(text) ]
                ]
            }
        })
        .collect();
    let green = UiColor::Green.text().unwrap_or_default();
    // `can_renegotiate`: "Replace objectives with a custom one".
    let renegotiate = data.can_renegotiate.then(|| {
        EntityScene(bsn! {
            @ss13_ui_kit::Button {
                @content: "Заменить задачи на собственную",
                @icon: "pen",
            }
            Name("uplink: renegotiate")
            Act({RENEGOTIATE_OBJECTIVES.to_owned()})
        })
    });
    bsn! {
        @Section { @fill: true, @scrollable: true }
        Children [
            Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center }
            Children [
                Node { margin: UiRect::vertical(px(24)) }
                Propagate::<Bold>(Bold(true))
                Propagate::<FontPx>(FontPx(14.4))
                Propagate::<TextColor>(TextColor(green))
                Children [ Text("ПРИВЕТСТВУЕМ, АГЕНТ.") ]
                --
                Node { margin: UiRect::bottom(px(24)) }
                Propagate::<Bold>(Bold(true))
                Propagate::<FontPx>(FontPx(14.4))
                Children [ Text("Агент, это ваши основные задачи. Выполните их любой ценой.") ]
                --
                Node { flex_direction: FlexDirection::Column, width: percent(100) }
                Children [ {objectives} ]
                --
                Node Children [ {renegotiate} ]
                --
                Node { margin: UiRect::top(px(24)) }
                Propagate::<FontPx>(FontPx(9.6))
                Children [ Text("SyndOS Version 3.17") ]
                --
                Node
                Propagate::<FontPx>(FontPx(9.6))
                Propagate::<TextColor>(TextColor(green))
                Children [ Text("Безопасное соединение") ]
            ]
        ]
    }
}

/// `GenericUplink`: the currency, the category tabs and the selected category's items.
fn market_view(st: &UplinkStatic, data: &UplinkData, local: &UiLocalState) -> impl Scene + use<> {
    let selected = match local.get(CATEGORY) {
        "" => st.categories.first().cloned().unwrap_or_default(),
        s => s.to_owned(),
    };
    let currency = format!("{} TC", data.telecrystals);
    let categories: Vec<_> = st
        .categories
        .iter()
        .map(|category| {
            let is_selected = *category == selected;
            let category = category.clone();
            bsn! {
                @TabsTab { @content: {category.clone()} }
                TabsTab { selected: is_selected }
                SetLocal { key: {CATEGORY.to_owned()}, value: category, rerender: true }
            }
        })
        .collect();
    let items: Vec<_> = st
        .items
        .iter()
        .filter(|i| i.category == selected)
        .map(|i| item_row(i, data))
        .collect();
    let empty = items
        .is_empty()
        .then(|| EntityScene(bsn! { @NoticeBox Children [ Text("В этой категории пусто.") ] }));
    bsn! {
        @Stack { @fill: true }
        Children [
            @StackItem
            Node { width: px(160) }
            Children [
                @Stack { @vertical: true, @fill: true }
                Children [
                    @StackItem
                    Children [
                        @ss13_ui_kit::Button { @content: currency, @fluid: true }
                        Propagate::<Bold>(Bold(true))
                        Name("uplink: telecrystals")
                        Act({BUY_RAW_TC.to_owned()})
                    ]
                    --
                    @StackItem { @grow: 1.0 }
                    Children [
                        @Tabs { @vertical: true, @fill: true }
                        Children [ {categories} ]
                    ]
                ]
            ]
            --
            @StackItem { @grow: 1.0 }
            Node { min_width: px(0) }
            Children [
                @Section { @fill: true, @scrollable: true }
                Children [ {empty} -- {items} ]
            ]
        ]
    }
}

/// `ItemList` entry, full mode: a 64 px icon, the name as the section title with the cost button,
/// the description.
fn item_row(item: &UplinkItemRecord, data: &UplinkData) -> impl Scene + use<> {
    let disabled = !data.can_buy(item);
    let cost = format!("{} TC", item.cost);
    let name = item.name.clone();
    let desc = item.desc.clone();
    let params = ActParams(vec![("path".to_owned(), item.id.clone())]);
    let button_name = Name::new(format!("buy: {}", item.name));
    let icon = item.icon.map(|i| {
        EntityScene(bsn! {
            Node { width: px(64), height: px(64) }
            UiSprite { icon_state: {Some(i.icon_state)}, color: {i.color} }
        })
    });
    bsn! {
        Node { flex_direction: FlexDirection::Row, margin: UiRect::bottom(px(8)) }
        Children [
            Node { width: px(64), height: px(64), margin: UiRect::right(px(6)), flex_shrink: 0.0 }
            Children [ {icon} ]
            --
            Node { flex_grow: 1.0, min_width: px(0) }
            Children [
                @Section {
                    @title: {name.clone()},
                    @buttons: bsn! {
                        @ss13_ui_kit::Button { @content: cost }
                        ss13_ui_kit::Button { disabled: disabled }
                        Act({BUY.to_owned()})
                        params
                        button_name
                    },
                }
                Children [
                    Node
                    Propagate::<TextColor>(TextColor(Color::srgba(1.0, 1.0, 1.0, 0.75)))
                    Children [ Text(desc) ]
                ]
            ]
        ]
    }
}

impl ClientUi for AntagInfoTraitorUi {
    const WIDTH: f32 = 620.0;
    const HEIGHT: f32 = 580.0;

    fn content(info: &TraitorInfo, _: &()) -> impl Scene {
        let intro = info.intro.clone();
        let mut lines: Vec<String> = info
            .objectives
            .iter()
            .map(|o| format!("#{}: {}", o.count, o.explanation))
            .collect();
        if lines.is_empty() {
            lines.push("Ни одной!".to_owned());
        }
        let objectives: Vec<_> = lines
            .into_iter()
            .map(|line| bsn! { Node Children [ Text(line) ] })
            .collect();
        let yellow = Color::srgb(1.0, 1.0, 0.0);
        let lightblue = Color::srgb(0.68, 0.85, 0.9);
        let (allies, goal) = (info.allies.clone(), info.goal.clone());
        let uplink = info.given_uplink.then(|| {
            let intro = info.uplink_intro.clone();
            let code = info
                .code
                .as_ref()
                .map(|c| format!("Код: {c}"))
                .unwrap_or_default();
            let unlock = info.uplink_unlock_info.clone();
            // `failsafe_code` in `badstyle` (red).
            let failsafe = info.failsafe_code.as_ref().map(|f| {
                let text = format!("Запасной код: {f}");
                EntityScene(bsn! {
                    Node
                    Propagate::<TextColor>(TextColor(Color::srgb(1.0, 0.3, 0.3)))
                    Name("traitor info: failsafe")
                    Children [ Text(text) ]
                })
            });
            EntityScene(bsn! {
                @StackItem
                Children [
                    @Section { @title: "Аплинк" }
                    Children [
                        Node { flex_direction: FlexDirection::Column }
                        Propagate::<Bold>(Bold(true))
                        Children [
                            Node Children [ Text(intro) ]
                            --
                            Node
                            Propagate::<TextColor>(TextColor(lightblue))
                            Name("traitor info: code")
                            Children [ Text(code) ]
                            --
                            Node Children [ {failsafe} ]
                            --
                            Node { margin: UiRect::top(px(6)) }
                            Children [ Text(unlock) ]
                        ]
                    ]
                ]
            })
        });
        let codewords = if info.has_codewords {
            let (phrases, responses) = (info.phrases.clone(), info.responses.clone());
            let blue = Color::hsl(224.1, 1.0, 0.565);
            let red = Color::hsl(0.0, 1.0, 0.5);
            EntityScene(bsn! {
                Node { column_gap: px(8) }
                Children [
                    Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column }
                    Children [
                        Node Children [ Text("Ваш наниматель предоставил вам кодовые слова для идентификации других агентов. Используйте слова во время обычных разговора, если считаете, что говорите с агентом. Но будьте осторожны, так как каждый может быть потенциальным противником.") ]
                        --
                        Node
                        Propagate::<TextColor>(TextColor(Color::srgb(1.0, 0.3, 0.3)))
                        Children [ Text("Вы запомнили кодовые слова, позволяющие распозновать их, когда вы их услышите.") ]
                    ]
                    --
                    Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column }
                    Children [
                        Node Children [ Text("Кодовые фразы:") ]
                        --
                        Node
                        Propagate::<Bold>(Bold(true))
                        Propagate::<TextColor>(TextColor(blue))
                        Name("traitor info: phrases")
                        Children [ Text(phrases) ]
                        --
                        Node Children [ Text("Кодовые ответы:") ]
                        --
                        Node
                        Propagate::<Bold>(Bold(true))
                        Propagate::<TextColor>(TextColor(red))
                        Name("traitor info: responses")
                        Children [ Text(responses) ]
                    ]
                ]
            })
        } else {
            EntityScene(bsn! {
                Node Children [ Text("Вам не были предоставлены кодовые слова. Вы должны использовать альтернативные способы поиска потенциальных союзников. Но будьте осторожны, так как каждый может быть потенциальным противником.") ]
            })
        };
        bsn! {
            @WindowContent
            Children [
                @Stack { @vertical: true, @fill: true }
                Children [
                    @StackItem { @grow: 1.0 }
                    Children [
                        @Stack { @fill: true }
                        Children [
                            @StackItem
                            Node { width: percent(70) }
                            Children [
                                @Section { @title: "Введение", @fill: true, @scrollable: true }
                                Children [
                                    Node { margin: UiRect::bottom(px(8)) }
                                    Propagate::<FontPx>(FontPx(25.0))
                                    Children [ Text(intro) ]
                                    --
                                    Node Propagate::<Bold>(Bold(true))
                                    Children [ Text("Ваши текущие задачи:") ]
                                    --
                                    Node { flex_direction: FlexDirection::Column }
                                    Children [ {objectives} ]
                                ]
                            ]
                            --
                            @StackItem
                            Node { width: percent(30) }
                            Children [
                                @Section { @title: "Наниматель", @fill: true, @scrollable: true }
                                Children [
                                    Node
                                    Propagate::<Bold>(Bold(true))
                                    Propagate::<TextColor>(TextColor(yellow))
                                    Children [ Text("Ваши обязанности:") ]
                                    --
                                    Node { margin: UiRect::bottom(px(8)) }
                                    Children [ Text(allies) ]
                                    --
                                    Node
                                    Propagate::<Bold>(Bold(true))
                                    Propagate::<TextColor>(TextColor(lightblue))
                                    Children [ Text("От нанимателя:") ]
                                    --
                                    Node Children [ Text(goal) ]
                                ]
                            ]
                        ]
                    ]
                    --
                    {uplink}
                    --
                    @StackItem
                    Children [
                        @Section { @title: "Кодовые слова" }
                        Children [ {codewords} ]
                    ]
                ]
            ]
        }
    }
}

/// tg `AntagInfoBrother.tsx`: "You are the {antag_name}!" and `ObjectivePrintout`.
impl ClientUi for AntagInfoBrotherUi {
    const WIDTH: f32 = 620.0;
    const HEIGHT: f32 = 250.0;

    fn content(info: &BrotherInfo, _: &()) -> impl Scene {
        let title = format!("You are the {}!", info.antag_name);
        let mut lines: Vec<String> = info
            .objectives
            .iter()
            .map(|o| format!("#{}: {}", o.count, o.explanation))
            .collect();
        if lines.is_empty() {
            lines.push("Ни одной!".to_owned());
        }
        let objectives: Vec<_> = lines
            .into_iter()
            .map(|line| bsn! { Node Children [ Text(line) ] })
            .collect();
        let red = UiColor::Red.text().unwrap_or_default();
        bsn! {
            @WindowContent
            Children [
                @Section { @fill: true, @scrollable: true }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(6) }
                    Children [
                        Node
                        Propagate::<FontPx>(FontPx(20.0))
                        Propagate::<TextColor>(TextColor(red))
                        Name("brother info: title")
                        Children [ Text(title) ]
                        --
                        Node Propagate::<Bold>(Bold(true))
                        Children [ Text("Ваши текущие задачи:") ]
                        --
                        Node { flex_direction: FlexDirection::Column }
                        Children [ {objectives} ]
                    ]
                ]
            ]
        }
    }
}

// shortcut: drawn like the generic panel (title and objectives), without tg's purple
// `abductor` theme and its "This Directive must be followed." lines.
impl ClientUi for AntagInfoBrainwashedUi {
    const WIDTH: f32 = crate::antag::cult::AntagInfoGenericUi::WIDTH;
    const HEIGHT: f32 = crate::antag::cult::AntagInfoGenericUi::HEIGHT;

    fn content(info: &BrotherInfo, data: &()) -> impl Scene {
        crate::antag::cult::AntagInfoGenericUi::content(info, data)
    }
}
