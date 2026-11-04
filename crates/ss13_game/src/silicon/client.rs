//! Client half of silicons and cameras:
//!
//! - the headless `CameraView`: while the server sends one, a console's camera moves the map
//!   view there (BYOND `client.eye`), and the tiles of the box the camera (or the AI's camera
//!   network) does not see are drawn as static over everything below the HUD (tg
//!   `CAMERA_STATIC_PLANE`);
//! - the camera console window (`CameraConsole220`): the camera list with prev/next and the
//!   active camera's name;
//! - `ListInputModal` (tg `tgui_input_list`): one button per choice;
//! - `StationAlertConsole`: a section per alarm type, the alarmed areas, and for the AI each
//!   area's camera button.
// shortcut: the static is plain dark tiles, not tg's animated `cameravis.dmi` static.
// shortcut: the console's camera list has no search field and no nanomap tab; its map pane is
// the main map view.

use bevy::app::Propagate;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::scene::EntityScene;
use bevy::sprite::Anchor;
use ss13_net::client::{LocalControl, NetClientState};
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::order::DrawOrder;
use ss13_render::sprites::tile_translation;
use ss13_render::{ActiveZ, CameraTarget, RenderSystems};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{TILE_SIZE, TilePos};
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_tgui::{UiInterface, decode};
use ss13_ui_kit::prelude::*;

use super::ui::*;

/// tg `CAMERA_STATIC_PLANE`.
const CAMERA_STATIC_PLANE: i16 = 31;

pub struct SiliconClientPlugin;

impl Plugin for SiliconClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DrawOrder>()
            .world_mut()
            .resource_mut::<DrawOrder>()
            .insert_plane(CAMERA_STATIC_PLANE);
        app.register_ui_headless::<CameraViewUi>()
            .register_ui_view::<CameraConsoleUi>()
            .register_ui_view::<ListInputUi>()
            .register_ui_view::<StationAlertUi>()
            .init_resource::<CameraView>()
            .add_systems(Startup, spawn_static)
            .add_systems(Update, receive_view)
            .add_systems(
                PostUpdate,
                (follow_remote_view, draw_static)
                    .chain()
                    .in_set(RenderSystems)
                    .before(ss13_render::camera::follow_target),
            );
    }
}

/// The server's CameraView window and its last data.
#[derive(Resource, Default, Debug)]
pub struct CameraView {
    pub window: Option<u32>,
    pub data: Option<CameraViewData>,
}

fn receive_view(
    mut received: MessageReader<UiMessageReceived>,
    state: Res<NetClientState>,
    mut view: ResMut<CameraView>,
) {
    if *state != NetClientState::Connected && view.window.is_some() {
        *view = CameraView::default();
    }
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == CameraViewUi::NAME => {
                view.window = Some(*window);
                view.data = decode(data).ok();
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if view.window == Some(*window) => view.data = decode(data).ok(),
            UiServerMessage::Close { window } if view.window == Some(*window) => {
                *view = CameraView::default();
            }
            _ => {}
        }
    }
}

/// The point a console's camera moves the view to.
#[derive(Component)]
struct RemoteView;

/// While a console camera is shown the map view looks from it, else from the controlled mob.
fn follow_remote_view(
    view: Res<CameraView>,
    remote: Query<(Entity, &TilePos), With<RemoteView>>,
    controlled: Query<(Entity, Has<CameraTarget>), With<LocalControl>>,
    mut commands: Commands,
) {
    let center = view
        .data
        .as_ref()
        .filter(|d| d.moves_view)
        .map(|d| d.center);
    match (center, remote.single()) {
        (Some(center), Ok((e, &pos))) => {
            if pos != center {
                commands.entity(e).insert(center);
            }
        }
        (Some(center), Err(_)) => {
            commands.spawn((RemoteView, center, CameraTarget));
        }
        (None, Ok((e, _))) => {
            commands.entity(e).despawn();
        }
        (None, Err(_)) => {}
    }
    for (mob, targeted) in &controlled {
        match (center.is_some(), targeted) {
            (true, true) => {
                commands.entity(mob).try_remove::<CameraTarget>();
            }
            (false, false) => {
                commands.entity(mob).try_insert(CameraTarget);
            }
            _ => {}
        }
    }
}

#[derive(Component)]
struct StaticOverlay;

fn spawn_static(mut images: ResMut<Assets<Image>>, mut commands: Commands) {
    let mut image = Image::new_fill(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::nearest();
    commands.spawn((
        StaticOverlay,
        Sprite {
            image: images.add(image),
            ..default()
        },
        Anchor::BOTTOM_LEFT,
        Transform::default(),
        Visibility::Hidden,
    ));
}

/// tg's static images on the box's unseen tiles.
fn draw_static(
    view: Res<CameraView>,
    active_z: Res<ActiveZ>,
    order: Res<DrawOrder>,
    mut images: ResMut<Assets<Image>>,
    mut overlay: Query<(&mut Sprite, &mut Transform, &mut Visibility), With<StaticOverlay>>,
    mut drawn: Local<Option<CameraViewData>>,
) {
    let Ok((mut sprite, mut transform, mut visibility)) = overlay.single_mut() else {
        return;
    };
    let Some(data) = view.data.as_ref().filter(|d| d.center.z == active_z.0) else {
        visibility.set_if_neq(Visibility::Hidden);
        *drawn = None;
        return;
    };
    let z = order.plane_z(CAMERA_STATIC_PLANE);
    let side = data.side();
    let corner = TilePos::new(
        data.center.x - i32::from(data.radius),
        data.center.y - i32::from(data.radius),
        data.center.z,
    );
    let at = tile_translation(corner).extend(z);
    if transform.translation != at {
        transform.translation = at;
    }
    visibility.set_if_neq(Visibility::Visible);
    if drawn.as_ref() == Some(data) {
        return;
    }
    *drawn = Some(data.clone());
    let n = side as u32;
    let mut pixels = vec![0u8; (n * n * 4) as usize];
    for row in 0..side {
        for col in 0..side {
            let pos = TilePos::new(corner.x + col, corner.y + row, corner.z);
            // Image rows go top-down.
            let i = (((side - 1 - row) * side + col) * 4) as usize;
            let alpha = if data.sees(pos) { 0 } else { 255 };
            pixels[i..i + 4].copy_from_slice(&[34, 34, 42, alpha]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::nearest();
    sprite.image = images.add(image);
    sprite.custom_size = Some(Vec2::splat((side * TILE_SIZE as i32) as f32));
}

impl ClientUi for CameraConsoleUi {
    const WIDTH: f32 = 300.0;
    const HEIGHT: f32 = 560.0;

    fn content(st: &CameraConsoleStatic, data: &CameraConsoleData) -> impl Scene {
        console_view(st, data)
    }
}

/// tgui `prevNextCamera`.
pub fn prev_next(
    cameras: &[CameraEntry],
    active: Option<&ActiveCamera>,
) -> Option<(String, String)> {
    let active = active?;
    let n = cameras.len();
    if n < 2 {
        return None;
    }
    let refs = |i: usize| cameras[i].cam_ref.clone();
    Some(
        match cameras.iter().position(|c| c.cam_ref == active.cam_ref) {
            None => (refs(n - 1), refs(0)),
            Some(0) if n == 2 => (refs(1), refs(1)),
            Some(0) => (refs(n - 1), refs(1)),
            Some(i) if i == n - 1 && n == 2 => (refs(0), refs(0)),
            Some(i) if i == n - 1 => (refs(i - 1), refs(0)),
            Some(i) => (refs(i - 1), refs(i + 1)),
        },
    )
}

fn switch(cam_ref: String) -> ActParams {
    ActParams(vec![("camera".to_owned(), cam_ref)])
}

fn console_view(st: &CameraConsoleStatic, data: &CameraConsoleData) -> impl Scene + use<> {
    let active = data.active_camera.clone();
    let rows: Vec<_> = st
        .cameras
        .iter()
        .filter(|c| !c.name.is_empty())
        .map(|c| {
            let selected = active.as_ref().is_some_and(|a| a.cam_ref == c.cam_ref);
            let name = Name::new(format!("camera: {}", c.name));
            let params = switch(c.cam_ref.clone());
            bsn! {
                @ss13_ui_kit::Button {
                    @content: {c.name.clone()},
                    @fluid: true,
                    @ellipsis: true,
                    @tooltip: {c.name.clone()},
                }
                ss13_ui_kit::Button { selected: selected, color: UiColor::Transparent }
                name
                Act({SWITCH_CAMERA.to_owned()})
                params
            }
        })
        .collect();
    let notice = match active.as_ref().filter(|a| a.status) {
        Some(a) => {
            let text = a.name.clone();
            EntityScene(bsn! { @NoticeBox { @info: true } Children [ Text(text) ] })
        }
        None => EntityScene(bsn! {
            @NoticeBox { @danger: true } Children [ Text("No input signal") ]
        }),
    };
    let (prev, next) = prev_next(&st.cameras, active.as_ref()).unzip();
    let arrow = |icon: &'static str, target: Option<String>| {
        let disabled = target.is_none();
        let params = switch(target.unwrap_or_default());
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: icon }
            ss13_ui_kit::Button { disabled: disabled }
            Act({SWITCH_CAMERA.to_owned()})
            params
        })
    };
    let (left, right) = (arrow("chevron-left", prev), arrow("chevron-right", next));
    bsn! {
        @WindowContent
        Children [
            @Stack { @vertical: true, @fill: true }
            Children [
                @StackItem
                Children [
                    @Stack
                    Children [
                        @StackItem { @grow: 1.0 } Children [ {notice} ]
                        --
                        @StackItem Children [ {left} ]
                        --
                        @StackItem Children [ {right} ]
                    ]
                ]
                --
                @StackItem { @grow: 1.0 }
                Node { min_height: px(0) }
                Children [
                    @Section { @fill: true, @scrollable: true }
                    Children [ {rows} ]
                ]
            ]
        ]
    }
}

impl ClientUi for ListInputUi {
    const WIDTH: f32 = 325.0;
    const HEIGHT: f32 = 325.0;

    fn content(st: &ListInputStatic, _: &()) -> impl Scene {
        let rows: Vec<_> = st
            .items
            .iter()
            .map(|item| {
                let name = Name::new(format!("choice: {item}"));
                let params = ActParams(vec![("entry".to_owned(), item.clone())]);
                bsn! {
                    @ss13_ui_kit::Button { @content: {item.clone()}, @fluid: true }
                    name
                    Act({SUBMIT.to_owned()})
                    params
                }
            })
            .collect();
        let message = st.message.clone();
        bsn! {
            @WindowContent
            Children [
                @Section { @fill: true, @scrollable: true, @title: message }
                Children [ {rows} ]
            ]
        }
    }
}

/// tgui `StationAlertConsole`'s camera button text.
pub fn camera_label(cameras: u32) -> String {
    match cameras {
        0 => "No Camera".to_owned(),
        1 => "1 Camera".to_owned(),
        n => format!("{n} Cameras"),
    }
}

fn alert_row(alert: &AlarmAlert, camera_view: bool) -> Box<dyn SceneList> {
    let sources = alert.sources.unwrap_or(0);
    let text = if camera_view && sources > 1 {
        format!("{}  ({sources} sources)", alert.name)
    } else {
        alert.name.clone()
    };
    let average = UiColor::Average.base();
    let label = EntityScene(bsn! {
        Node Propagate::<TextColor>(TextColor(average)) Children [ Text(text) ]
    });
    if !camera_view {
        return Box::new(EntityScene(bsn! {
            Node { margin: UiRect::bottom(px(4)) } Children [ {label} ]
        }));
    }
    let cameras = alert.cameras.unwrap_or(0);
    let params = ActParams(vec![(
        "alert".to_owned(),
        alert.alert_ref.clone().unwrap_or_default(),
    )]);
    let icon = if cameras > 0 { "video" } else { "" };
    Box::new(EntityScene(bsn! {
        @Stack
        Node { margin: UiRect::bottom(px(4)) }
        Children [
            @StackItem { @grow: 1.0 } Children [ {label} ]
            --
            @StackItem
            Children [
                @ss13_ui_kit::Button { @icon: icon, @content: {camera_label(cameras)} }
                ss13_ui_kit::Button { disabled: {cameras == 0} }
                Node { width: px(100) }
                Act({SELECT_CAMERA.to_owned()})
                params
            ]
        ]
    }))
}

impl ClientUi for StationAlertUi {
    const WIDTH: f32 = 390.0;
    const HEIGHT: f32 = 587.0;

    fn content(_: &(), data: &StationAlertData) -> impl Scene {
        let good = UiColor::Good.base();
        let sections: Vec<Box<dyn SceneList>> = data
            .alarms
            .iter()
            .map(|category| -> Box<dyn SceneList> {
                let title = format!("{} Alarms", category.name);
                let rows: Vec<Box<dyn SceneList>> = if category.alerts.is_empty() {
                    vec![Box::new(EntityScene(bsn! {
                        Node Propagate::<TextColor>(TextColor(good))
                        Children [ Text("Systems nominal") ]
                    }))]
                } else {
                    category
                        .alerts
                        .iter()
                        .map(|a| alert_row(a, data.camera_view))
                        .collect()
                };
                Box::new(EntityScene(bsn! {
                    @Section { @title: title } Children [ {rows} ]
                }))
            })
            .collect();
        bsn! {
            @WindowContent
            Children [
                @Section { @fill: true, @scrollable: true } Children [ {sections} ]
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cams(n: usize) -> Vec<CameraEntry> {
        (0..n)
            .map(|i| CameraEntry {
                name: format!("c{i}"),
                cam_ref: i.to_string(),
                ..default()
            })
            .collect()
    }

    fn active(r: &str) -> ActiveCamera {
        ActiveCamera {
            cam_ref: r.into(),
            ..default()
        }
    }

    #[test]
    fn prev_next_wraps_like_tgui() {
        let three = cams(3);
        let pair = |a: &str, b: &str| Some((a.to_owned(), b.to_owned()));
        assert_eq!(prev_next(&three, None), None);
        assert_eq!(prev_next(&three, Some(&active("0"))), pair("2", "1"));
        assert_eq!(prev_next(&three, Some(&active("1"))), pair("0", "2"));
        assert_eq!(prev_next(&three, Some(&active("2"))), pair("1", "0"));
        assert_eq!(prev_next(&three, Some(&active("x"))), pair("2", "0"));
        assert_eq!(prev_next(&cams(2), Some(&active("0"))), pair("1", "1"));
        assert_eq!(prev_next(&cams(1), Some(&active("0"))), None);
    }
}
