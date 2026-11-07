//! Client half of the ghost's windows ([`crate::ghost_menu`]): the observer HUD buttons (tg
//! `/datum/hud/ghost`: Orbit, ghost menu, spawners, re-enter corpse, respawn) along the bottom of
//! the map while the server keeps [`GhostHudUi`] open, the ghost's lighting cutoff (its darkness
//! setting) into the renderer, and the `Orbit`, `GhostMenu` and `SpawnersMenu` windows.
// shortcut: the HUD buttons are text labels, not tg's `icons/hud/screen_ghost.dmi` sprites; the
// windows have no search box, no antagonist/admin sections and no notification list.

use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::scene::EntityScene;
use ss13_net::client::SendIntent;
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::{LightingCutoff, MapInsets};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_tgui::{UiInterface, decode};
use ss13_ui_kit::prelude::*;

use crate::ghost_menu::*;

pub struct GhostMenuClientPlugin;

impl Plugin for GhostMenuClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<GhostHudUi>()
            .register_ui_view::<OrbitUi>()
            .register_ui_view::<GhostMenuUi>()
            .register_ui_view::<SpawnersMenuUi>()
            .init_resource::<GhostHud>()
            .add_systems(Startup, spawn_buttons)
            .add_systems(
                Update,
                (
                    receive,
                    draw_buttons.run_if(resource_changed::<GhostHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's HUD window and its data.
#[derive(Resource, Default, Debug)]
pub struct GhostHud {
    pub window: Option<u32>,
    pub data: GhostHudData,
}

#[derive(Component)]
struct HudRoot;

/// One HUD button: the act it sends; `needs_body`: shown only with a body to re-enter.
#[derive(Component, Clone, Copy)]
struct HudButton {
    act: &'static str,
    needs_body: bool,
}

const BUTTONS: [(&str, &str, bool); 5] = [
    (HUD_ORBIT, "Orbit", false),
    (HUD_GHOST_MENU, "Меню призрака", false),
    (HUD_SPAWNERS, "Спавнеры", false),
    (HUD_REENTER, "В тело", true),
    (HUD_RESPAWN, "Респаун", false),
];

fn spawn_buttons(mut commands: Commands) {
    let root = commands
        .spawn((
            HudRoot,
            Name::new("ghost hud"),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(4),
                left: px(0),
                right: px(0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                column_gap: px(4),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .id();
    for (act, label, needs_body) in BUTTONS {
        commands
            .spawn((
                HudButton { act, needs_body },
                Name::new(format!("ghost hud {act}")),
                Node {
                    padding: UiRect::axes(px(6), px(2)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.1, 0.2, 0.8)),
                ChildOf(root),
            ))
            .with_child((Text::new(label), Pickable::IGNORE))
            .observe(click_button);
    }
}

fn receive(
    mut received: MessageReader<UiMessageReceived>,
    mut hud: ResMut<GhostHud>,
    cutoff: Option<ResMut<LightingCutoff>>,
) {
    let before = hud.data.lighting_cutoff;
    let mut changed = false;
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == GhostHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = GhostHud::default();
                changed = true;
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<GhostHudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable ghost HUD data: {e}"),
            }
            changed = true;
        }
    }
    // Out of a ghost (closed HUD) the cutoff is 0 again: lighting as is.
    if (changed || hud.data.lighting_cutoff != before)
        && let Some(mut cutoff) = cutoff
    {
        let value = f32::from(hud.data.lighting_cutoff) / 100.0;
        if cutoff.0 != value {
            cutoff.0 = value;
        }
    }
}

fn draw_buttons(
    hud: Res<GhostHud>,
    mut roots: Query<&mut Visibility, (With<HudRoot>, Without<HudButton>)>,
    mut buttons: Query<(&HudButton, &mut Node)>,
) {
    for mut visibility in &mut roots {
        *visibility = if hud.window.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (button, mut node) in &mut buttons {
        node.display = if button.needs_body && !hud.data.has_body {
            Display::None
        } else {
            Display::Flex
        };
    }
}

/// tg `/atom/movable/screen/ghost/*/Click`.
fn click_button(
    click: On<PointerClick>,
    buttons: Query<&HudButton>,
    hud: Res<GhostHud>,
    mut intents: MessageWriter<SendIntent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let (Some(window), Ok(button)) = (hud.window, buttons.get(click.entity)) else {
        return;
    };
    intents.write(SendIntent(ClientIntent::UiAct {
        window,
        action: button.act.into(),
        params: Vec::new(),
    }));
}

/// The buttons stay on the map viewport, left of the docked chat.
fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<HudRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}

fn act_button(
    label: String,
    act: &'static str,
    params: Vec<(String, String)>,
    selected: bool,
) -> impl Scene + use<> {
    let params = ActParams(params);
    let act = act.to_owned();
    bsn! {
        @ss13_ui_kit::Button { @content: label, @fluid: true }
        ss13_ui_kit::Button { selected: selected }
        Act(act)
        params
    }
}

fn poi_row(e: &OrbitEntry, orbiting: Option<u64>, observing: Option<u64>) -> impl Scene + use<> {
    let key = e.key.to_string();
    let orbit = act_button(
        e.full_name.clone(),
        ORBIT,
        vec![(ORBIT_REF.to_owned(), key.clone())],
        orbiting == Some(e.key),
    );
    let observe = act_button(
        "Наблюдать".to_owned(),
        ORBIT,
        vec![
            (ORBIT_REF.to_owned(), key),
            (ORBIT_AUTO_OBSERVE.to_owned(), "1".to_owned()),
        ],
        observing == Some(e.key),
    );
    let (orbit, observe) = (EntityScene(orbit), EntityScene(observe));
    bsn! {
        Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
        Children [ {orbit} -- {observe} ]
    }
}

impl ClientUi for OrbitUi {
    const WIDTH: f32 = 500.0;
    const HEIGHT: f32 = 700.0;

    fn content(_: &(), data: &OrbitData) -> impl Scene {
        let (orbiting, observing) = (data.orbiting, data.observing);
        let section = |title: &str, list: &[OrbitEntry]| {
            let title = format!("{title} ({})", list.len());
            let rows: Vec<_> = list
                .iter()
                .map(|e| poi_row(e, orbiting, observing))
                .collect();
            EntityScene(bsn! {
                @Section { @title: title }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
                    Children [ {rows} ]
                ]
            })
        };
        let alive = section("Живые", &data.alive);
        let dead = section("Мёртвые", &data.dead);
        let ghosts = section("Призраки", &data.ghosts);
        let npcs = section("Прочие", &data.npcs);
        let refresh = EntityScene(act_button(
            "Обновить".to_owned(),
            ORBIT_REFRESH,
            Vec::new(),
            false,
        ));
        bsn! {
            @WindowContent
            Children [
                @Section { @fill: true, @scrollable: true }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
                    Children [ {refresh} -- {alive} -- {dead} -- {ghosts} -- {npcs} ]
                ]
            ]
        }
    }
}

impl ClientUi for GhostMenuUi {
    const WIDTH: f32 = 500.0;
    const HEIGHT: f32 = 630.0;

    fn content(_: &(), data: &GhostMenuData) -> impl Scene {
        let body: Vec<_> = data
            .body_name
            .iter()
            .flat_map(|name| {
                [
                    act_button(
                        format!("Вернуться в {name}"),
                        MENU_RETURN,
                        Vec::new(),
                        false,
                    ),
                    act_button(
                        "Не воскрешать (DNR)".to_owned(),
                        MENU_DNR,
                        Vec::new(),
                        false,
                    ),
                ]
            })
            .collect();
        let darkness: Vec<_> = GHOST_LIGHTINGS
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                act_button(
                    (*name).to_owned(),
                    MENU_DARKNESS,
                    vec![(MENU_DARKNESS_LEVEL.to_owned(), (*name).to_owned())],
                    usize::from(data.current_darkness) == i,
                )
            })
            .collect();
        let vision = EntityScene(act_button(
            "Ghost Vision".to_owned(),
            MENU_TOGGLE,
            vec![(MENU_TOGGLING.to_owned(), GHOST_VISION.to_owned())],
            data.ghost_vision,
        ));
        let health = EntityScene(act_button(
            "Health Scanner".to_owned(),
            MENU_TOGGLE,
            vec![(MENU_TOGGLING.to_owned(), GHOST_HEALTH.to_owned())],
            data.health_scan,
        ));
        let tray = EntityScene(act_button(
            "T-ray scan".to_owned(),
            MENU_TRAY,
            Vec::new(),
            false,
        ));
        let mut fun: Vec<_> = Vec::new();
        if data.has_fun {
            let boo = if data.can_boo {
                "Boo!"
            } else {
                "Boo! (перезарядка)"
            };
            fun.push(act_button(boo.to_owned(), MENU_BOO, Vec::new(), false));
            for e in &data.possessable {
                fun.push(act_button(
                    format!("Вселиться: {}", e.full_name),
                    MENU_POSSESS,
                    vec![(ORBIT_REF.to_owned(), e.key.to_string())],
                    false,
                ));
            }
        }
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Тело" }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
                    Children [ {body} ]
                ]
                --
                @Section { @title: "Темнота" }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
                    Children [ {darkness} ]
                ]
                --
                @Section { @title: "HUD" }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
                    Children [ {vision} -- {health} -- {tray} ]
                ]
                --
                @Section { @title: "Fun", @scrollable: true }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
                    Children [ {fun} ]
                ]
            ]
        }
    }
}

impl ClientUi for SpawnersMenuUi {
    const WIDTH: f32 = 700.0;
    const HEIGHT: f32 = 600.0;

    fn content(_: &(), data: &SpawnersData) -> impl Scene {
        let rows: Vec<_> = data
            .spawners
            .iter()
            .map(|s| {
                let key = s.key.to_string();
                let title = format!("{} ({} left)", s.name, s.amount_left);
                let desc = s.desc.clone();
                let jump = EntityScene(act_button(
                    "Jump".to_owned(),
                    SPAWNER_JUMP,
                    vec![(SPAWNER_REF.to_owned(), key.clone())],
                    false,
                ));
                let spawn = EntityScene(act_button(
                    "Spawn".to_owned(),
                    SPAWNER_SPAWN,
                    vec![(SPAWNER_REF.to_owned(), key)],
                    false,
                ));
                bsn! {
                    @Section { @title: title }
                    Children [
                        Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
                        Children [
                            Text(desc)
                            --
                            Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
                            Children [ {jump} -- {spawn} ]
                        ]
                    ]
                }
            })
            .collect();
        bsn! {
            @WindowContent
            Children [
                @Section { @fill: true, @scrollable: true }
                Children [ {rows} ]
            ]
        }
    }
}
