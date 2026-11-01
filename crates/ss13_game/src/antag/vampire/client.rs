//! Client half of the vampire: the `AntagInfoVampire` panel (`AntagInfoVampire.tsx`: the
//! information and specializations tabs, the blood drunk, each specialization's powers with
//! their blood thresholds and the pick button) and the usable blood counter (`/atom/movable/
//! screen/vampire_blood`: `icons/hud/screen_gen.dmi` `blood_display` at `WEST:6,CENTER-1:15`,
//! the number in `#ce0202`).
// shortcut: no `Vampire` window theme and no blood requirement tooltips.

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::Dir;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_tgui::{UiInterface, decode};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use super::{
    AntagInfoVampireUi, PowerInfo, SubclassInfo, VampireHudData, VampireHudUi, VampireInfoData,
    VampireInfoStatic, blood_text,
};
use crate::inventory_client::{ScreenAxis, screen_loc};

const GEN_DMI: &str = "icons/hud/screen_gen.dmi";
/// `useState` keys: the tab (`"1"` specializations) and the specialization shown.
const TAB: &str = "tab";
const SPEC: &str = "spec";
const RED: Color = Color::srgb(0.878, 0.357, 0.396);

pub struct VampireClientPlugin;

impl Plugin for VampireClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<AntagInfoVampireUi>()
            .register_ui_headless::<VampireHudUi>()
            .init_resource::<VampireHud>()
            .add_systems(Startup, spawn_counter)
            .add_systems(
                Update,
                (
                    receive,
                    draw_counter.run_if(resource_changed::<VampireHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's blood counter window and its last data.
#[derive(Resource, Default, Debug)]
pub struct VampireHud {
    pub window: Option<u32>,
    pub data: VampireHudData,
}

#[derive(Component)]
struct CounterRoot;

#[derive(Component)]
struct Counter;

#[derive(Component)]
struct CounterText;

fn spawn_counter(mut commands: Commands) {
    let root = commands
        .spawn((
            CounterRoot,
            Name::new("vampire blood"),
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
    let slot = commands
        .spawn((
            Counter,
            Name::new("usable blood"),
            screen_loc(ScreenAxis(-1, 0, 6), ScreenAxis(0, -1, 15)),
            ImageNode::default(),
            Visibility::Hidden,
            Pickable::IGNORE,
            ChildOf(root),
        ))
        .id();
    // `maptext_x = 1, maptext_y = 8`, centered.
    commands.spawn((
        CounterText,
        Node {
            position_type: PositionType::Absolute,
            left: px(2),
            right: px(0),
            bottom: px(16),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(slot),
        children![(
            Text::default(),
            TextColor(Color::srgb_u8(0xce, 0x02, 0x02)),
            FontPx(14.0),
            Bold(true),
        )],
    ));
}

fn receive(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<VampireHud>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == VampireHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = VampireHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<VampireHudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable vampire HUD data: {e}"),
            }
        }
    }
}

fn draw_counter(
    hud: Res<VampireHud>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut slot: Single<(&mut ImageNode, &mut Visibility), With<Counter>>,
    holders: Query<&Children, With<CounterText>>,
    mut texts: Query<&mut Text>,
) {
    let (image, visibility) = &mut *slot;
    **visibility = if hud.window.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let sprite = atlas.as_deref_mut().and_then(|atlas| {
        let id = atlas
            .manifest
            .index()
            .get(&(GEN_DMI, "blood_display"))
            .copied()?;
        let (info, handle) = atlas.state(id)?;
        Some(ImageNode {
            image: handle,
            rect: frame_rect(info, Dir::South, 0),
            ..default()
        })
    });
    if let Some(sprite) = sprite {
        **image = sprite;
    }
    for children in &holders {
        for &child in children {
            if let Ok(mut text) = texts.get_mut(child) {
                text.0 = blood_text(hud.data.usable);
            }
        }
    }
}

fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<CounterRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}

impl ClientUi for AntagInfoVampireUi {
    const WIDTH: f32 = 750.0;
    const HEIGHT: f32 = 635.0;

    fn content(st: &VampireInfoStatic, data: &VampireInfoData) -> impl Scene {
        info_view(st, data, &UiLocalState::default())
    }

    fn content_local(
        st: &VampireInfoStatic,
        data: &VampireInfoData,
        local: &UiLocalState,
    ) -> impl Scene {
        info_view(st, data, local)
    }
}

fn info_view(
    st: &VampireInfoStatic,
    data: &VampireInfoData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let specializations = local.get(TAB) == "1";
    let total = format!("Всего выпито крови: {}", blood_text(data.total_blood));
    let body: Box<dyn SceneList> = if specializations {
        Box::new(EntityScene(specializations_view(st, data, local)))
    } else {
        Box::new(EntityScene(intro_view(data)))
    };
    bsn! {
        @WindowContent
        Children [
            @Stack { @vertical: true, @fill: true }
            Children [
                @StackItem
                Children [
                    @Tabs { @fluid: true }
                    Children [
                        @TabsTab { @content: "Информация", @icon: "info" }
                        TabsTab { selected: {!specializations} }
                        SetLocal { key: {TAB.to_owned()}, value: {"0".to_owned()}, rerender: true }
                        --
                        @TabsTab { @content: "Специализации", @icon: "book" }
                        TabsTab { selected: specializations }
                        SetLocal { key: {TAB.to_owned()}, value: {"1".to_owned()}, rerender: true }
                    ]
                ]
                --
                @StackItem
                Children [
                    Node {
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(8)),
                        margin: UiRect::vertical(px(6)),
                    }
                    BackgroundColor(Color::srgba(0.5, 0.05, 0.08, 0.35))
                    Children [
                        Node Propagate::<FontPx>(FontPx(14.0)) Name("vampire info: total")
                        Children [ Text(total) ]
                        --
                        Node
                        Propagate::<FontPx>(FontPx(12.0))
                        Propagate::<TextColor>(TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)))
                        Children [ Text("Накопленная кровь открывает новые способности.") ]
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

/// `IntroductionSection`.
fn intro_view(data: &VampireInfoData) -> impl Scene + use<> {
    let path = format!(
        "Достигнув {} единиц крови, откройте раздел «Специализации» и выберите специализацию. \
         Изучите способности каждого пути: они определяют, как будет развиваться ваша сила.",
        data.specialization_blood_required
    );
    let mut lines: Vec<String> = data
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
    bsn! {
        @Section { @title: "Вы - вампир!", @fill: true, @scrollable: true }
        Children [
            Node { flex_direction: FlexDirection::Column, row_gap: px(8) }
            Propagate::<FontPx>(FontPx(14.0))
            Children [
                Node { justify_content: JustifyContent::Center }
                Children [ Text("Пейте кровь живых, чтобы усиливать свои вампирские способности.") ]
                --
                Node { flex_direction: FlexDirection::Column }
                Propagate::<FontPx>(FontPx(12.0))
                Children [
                    Node Propagate::<Bold>(Bold(true)) Propagate::<TextColor>(TextColor(RED))
                    Children [ Text("Выберите свой путь") ]
                    --
                    Node Children [ Text(path) ]
                ]
                --
                Node Propagate::<Bold>(Bold(true))
                Children [ Text("Ваши текущие задачи:") ]
                --
                Node { flex_direction: FlexDirection::Column }
                Name("vampire info: objectives")
                Children [ {objectives} ]
            ]
        ]
    }
}

/// `SpecializationInfo`: the vertical tabs and the shown specialization.
fn specializations_view(
    st: &VampireInfoStatic,
    data: &VampireInfoData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let selected_index = data
        .selected_subclass
        .as_ref()
        .and_then(|id| st.subclasses.iter().position(|s| s.id == *id))
        .unwrap_or(0);
    let shown = local.get(SPEC).parse().unwrap_or(selected_index);
    let tabs: Vec<_> = st
        .subclasses
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mine = data.selected_subclass.as_deref() == Some(s.id.as_str());
            let name = s.name.clone();
            bsn! {
                @TabsTab { @content: name, @icon: {if mine { "check" } else { "info" }} }
                TabsTab { selected: {i == shown} }
                SetLocal { key: {SPEC.to_owned()}, value: {i.to_string()}, rerender: true }
            }
        })
        .collect();
    let content: Box<dyn SceneList> = match st.subclasses.get(shown) {
        Some(spec) => Box::new(EntityScene(spec_view(spec, data))),
        None => Box::new(EntityScene(bsn! { Node })),
    };
    bsn! {
        @Stack { @fill: true }
        Children [
            @StackItem
            Node { width: px(125), flex_shrink: 0.0 }
            Children [
                @Tabs { @vertical: true, @fluid: true }
                Children [ {tabs} ]
            ]
            --
            @StackItem { @grow: 1.0 }
            Node { min_width: px(0) }
            Children [ {content} ]
        ]
    }
}

/// `SpecializationContent`.
fn spec_view(spec: &SubclassInfo, data: &VampireInfoData) -> impl Scene + use<> {
    let selected = data.selected_subclass.as_deref() == Some(spec.id.as_str());
    let can_select = data.can_select_subclass && data.selected_subclass.is_none();
    let label = if selected {
        "Ваша специализация"
    } else if can_select {
        "Выбрать специализацию"
    } else {
        "Недоступно"
    };
    let icon = if selected {
        "check"
    } else if can_select {
        "plus"
    } else {
        "lock"
    };
    let tooltip = if selected {
        "Это ваша специализация.".to_owned()
    } else if data.selected_subclass.is_some() {
        "Вы уже выбрали другую специализацию.".to_owned()
    } else if !can_select && data.total_blood < data.specialization_blood_required as f32 {
        format!(
            "Для выбора специализации нужно выпить ещё {} крови (всего {}).",
            blood_text(data.specialization_blood_required as f32 - data.total_blood),
            data.specialization_blood_required
        )
    } else if can_select {
        "Выбрать этот путь развития.".to_owned()
    } else {
        "Выбор специализации недоступен.".to_owned()
    };
    let name = spec.name.clone();
    let description = spec.description.clone();
    let color = if selected {
        UiColor::Green
    } else {
        UiColor::Red
    };
    let button_name = Name::new(format!("vampire spec: {}", spec.id));
    let powers = EntityScene(power_group(
        "Способности пути",
        &spec.powers,
        data.total_blood,
    ));
    let full = EntityScene(power_group(
        "Полная сила",
        &spec.full_powers,
        data.total_blood,
    ));
    bsn! {
        @Section { @fill: true, @scrollable: true }
        Children [
            Node { flex_direction: FlexDirection::Column, row_gap: px(8) }
            Children [
                Node { justify_content: JustifyContent::Center }
                Propagate::<FontPx>(FontPx(24.0))
                Propagate::<TextColor>(TextColor(RED))
                Children [ Text(name) ]
                --
                Node { justify_content: JustifyContent::Center }
                Children [ Text(description) ]
                --
                Node { justify_content: JustifyContent::Center }
                Children [
                    @ss13_ui_kit::Button { @content: label, @icon: icon, @tooltip: tooltip }
                    ss13_ui_kit::Button { color: color, disabled: {!can_select || selected} }
                    Act({spec.id.clone()})
                    button_name
                ]
                --
                {powers}
                --
                {full}
            ]
        ]
    }
}

/// `PowerGroup` + `PowerDescription`.
fn power_group(title: &str, powers: &[PowerInfo], total: f32) -> impl Scene + use<> {
    let title = title.to_owned();
    let rows: Vec<_> = powers
        .iter()
        .map(|power| {
            let header = power.name.is_some() || power.blood_required.is_some();
            let name = power
                .name
                .clone()
                .unwrap_or_else(|| "Пассивная способность".to_owned());
            let badge = power.blood_required.map(|blood| {
                let reached = total >= blood as f32;
                let color = if reached {
                    Color::srgb(0.45, 0.75, 0.3)
                } else {
                    Color::srgba(1.0, 1.0, 1.0, 0.5)
                };
                let icon = if reached { "check" } else { "lock" };
                let text = format!("{blood} крови");
                EntityScene(bsn! {
                    Node { margin: UiRect::left(px(8)), column_gap: px(4) }
                    Propagate::<TextColor>(TextColor(color))
                    Children [ @Icon { @name: icon } -- Node Children [ Text(text) ] ]
                })
            });
            let head = header.then(|| {
                EntityScene(bsn! {
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::bottom(px(4)) }
                    Children [
                        Node Propagate::<Bold>(Bold(true)) Children [ Text(name) ]
                        --
                        {badge}
                    ]
                })
            });
            let description = power.description.clone();
            bsn! {
                Node { flex_direction: FlexDirection::Column }
                Children [
                    {head}
                    --
                    Node {
                        padding: UiRect::left(px(8)),
                        border: UiRect::left(px(2)),
                    }
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.25))
                    Children [ Text(description) ]
                ]
            }
        })
        .collect();
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: px(6) }
        Children [
            Node Propagate::<Bold>(Bold(true)) Propagate::<FontPx>(FontPx(14.0))
            Children [ Text(title) ]
            --
            Node { flex_direction: FlexDirection::Column, row_gap: px(6) }
            Children [ {rows} ]
        ]
    }
}
