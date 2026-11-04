//! Client half of the fullscreen overlays (tg `/atom/movable/screen/fullscreen`, `FULLSCREEN_PLANE`)
//! and the world filters, from [`FullscreenHudData`] through the headless tgui interface
//! [`FullscreenHudUi`].
//!
//! Each shown category is a UI image stretched over the map viewport, as tg's `update_for_view`
//! stretches the 480x480 icon (15x15 tiles) to the view; the flash state is a whole-view tile.
//! They sit under every other HUD element, ordered by tg layer, animate by their `.dmi` delays
//! (the flickering blind overlay) and fade out when cleared over the time tg animates
//! (`clear_fullscreen(category, animated)`). Client colours and the eye blur become the world
//! camera's [`ScreenFilter`]; colours fade in and out over 2 s (`fade_in`, `fade_out`).

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::filter::{IDENTITY, compose, lerp};
use ss13_render::{IconAtlas, MapInsets, ScreenFilter, WorldCamera};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, IconStateId};
use ss13_tgui::client::AppTguiClientExt;
use ss13_tgui::{UiInterface, decode};

use crate::hud::{
    ClientColour, FullscreenHudData, FullscreenHudUi, FullscreenScreen, ScreenCategory,
};

/// Below every other HUD root (they use the default 0 or above).
const FULLSCREEN_Z: i32 = -10;

pub struct FullscreenPlugin;

impl Plugin for FullscreenPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<FullscreenHudUi>()
            .init_resource::<FullscreenHud>()
            .init_resource::<FilterFade>()
            .add_systems(Startup, spawn_root)
            .add_systems(
                Update,
                (
                    receive_fullscreen,
                    (draw_screens, apply_filter),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                )
                    .chain(),
            );
    }
}

/// The server's fullscreen window and its last data.
#[derive(Resource, Default, Debug)]
pub struct FullscreenHud {
    pub window: Option<u32>,
    pub data: FullscreenHudData,
}

#[derive(Component)]
struct FullscreenRoot;

/// One drawn overlay.
#[derive(Component, Debug)]
pub struct ScreenNode {
    pub screen: FullscreenScreen,
    /// When it appeared (its animation's start), seconds.
    shown_at: f32,
    /// When it started fading out, while it does.
    fading_since: Option<f32>,
}

/// Every icon state the overlays can use, looked up once.
#[derive(Resource, Default)]
struct ScreenIcons(HashMap<(String, String), IconStateId>);

/// Smooth darkening for [`ScreenCategory::Tint`] and [`ScreenCategory::Nearsighted`], severities
/// 1 and 2+. tg's `impairedoverlay` is a dithered 480x480 sprite drawn 1:1 in BYOND; stretched
/// over a large window its noise turns into coarse blocks, so the client draws its own gradient.
#[derive(Resource, Default)]
struct Vignettes(Vec<Handle<Image>>);

/// Side of a generated vignette; linear sampling smooths it at any window size.
const VIGNETTE_SIDE: u32 = 128;

/// Alpha of the darkening at `r` (0 at the view centre, 1 at the middle of an edge).
pub fn vignette_alpha(r: f32, severity: u8) -> f32 {
    // (alpha at the centre, alpha at the edge, radius where the rise starts)
    let (centre, edge, start) = if severity >= 2 {
        (0.55, 0.95, 0.1)
    } else {
        (0.3, 0.8, 0.25)
    };
    let t = ((r - start) / (1.2 - start)).clamp(0.0, 1.0);
    let smooth = t * t * (3.0 - 2.0 * t);
    centre + (edge - centre) * smooth
}

fn vignette_image(severity: u8) -> Image {
    let side = VIGNETTE_SIDE;
    let mut data = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let fx = (x as f32 + 0.5) / side as f32 * 2.0 - 1.0;
            let fy = (y as f32 + 0.5) / side as f32 * 2.0 - 1.0;
            let a = vignette_alpha((fx * fx + fy * fy).sqrt(), severity);
            data.extend_from_slice(&[0, 0, 0, (a * 255.0).round() as u8]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

fn spawn_root(
    atlas: Option<Res<IconAtlas>>,
    images: Option<ResMut<Assets<Image>>>,
    mut commands: Commands,
) {
    if let Some(mut images) = images {
        let vignettes = [1, 2].map(|s| images.add(vignette_image(s)));
        commands.insert_resource(Vignettes(vignettes.to_vec()));
    }
    let mut icons = HashMap::new();
    if let Some(atlas) = atlas {
        let index = atlas.manifest.index();
        for (dmi, state) in all_icons() {
            if let Some(&id) = index.get(&(dmi, state.as_str())) {
                icons.insert((dmi.to_owned(), state), id);
            } else {
                warn!("fullscreen icon {dmi} {state} missing from the atlas");
            }
        }
    }
    commands.insert_resource(ScreenIcons(icons));
    commands.spawn((
        FullscreenRoot,
        Name::new("fullscreen"),
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            bottom: px(0),
            left: px(0),
            right: px(0),
            ..default()
        },
        GlobalZIndex(FULLSCREEN_Z),
        Pickable::IGNORE,
    ));
}

/// The states [`FullscreenScreen::icon`] can name (severities 0-10).
fn all_icons() -> Vec<(&'static str, String)> {
    let categories = [
        ScreenCategory::Tint,
        ScreenCategory::Nearsighted,
        ScreenCategory::Flash,
        ScreenCategory::Brute,
        ScreenCategory::Oxy,
        ScreenCategory::CritVision,
        ScreenCategory::Crit,
    ];
    let mut out: Vec<(&'static str, String)> = categories
        .into_iter()
        .flat_map(|c| {
            let max = match c {
                ScreenCategory::Tint | ScreenCategory::Nearsighted => 3,
                ScreenCategory::Brute => 6,
                ScreenCategory::Flash => 0,
                _ => 10,
            };
            (0..=max).map(move |s| FullscreenScreen::new(c, Some(s)).icon())
        })
        .collect();
    for flicker in [false, true] {
        let blind = FullscreenScreen {
            flicker,
            ..FullscreenScreen::new(ScreenCategory::Blind, None)
        };
        out.push(blind.icon());
    }
    out.sort();
    out.dedup();
    out
}

fn receive_fullscreen(
    mut received: MessageReader<UiMessageReceived>,
    mut hud: ResMut<FullscreenHud>,
) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == FullscreenHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = FullscreenHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<FullscreenHudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable fullscreen data: {e}"),
            }
        }
    }
}

/// What to do with the drawn overlays for the screens the server shows now: `(update, spawn,
/// fade)` — nodes to point at a new screen, screens to add, nodes to start fading.
pub fn diff_screens(
    drawn: &[(Entity, FullscreenScreen, bool)],
    shown: &[FullscreenScreen],
) -> (
    Vec<(Entity, FullscreenScreen)>,
    Vec<FullscreenScreen>,
    Vec<Entity>,
) {
    let (mut update, mut spawn, mut fade) = (Vec::new(), Vec::new(), Vec::new());
    for screen in shown {
        match drawn
            .iter()
            .find(|(_, d, fading)| !fading && d.category == screen.category)
        {
            Some(&(e, d, _)) if d != *screen => update.push((e, *screen)),
            Some(_) => {}
            None => spawn.push(*screen),
        }
    }
    for &(e, d, fading) in drawn {
        if !fading && !shown.iter().any(|s| s.category == d.category) {
            fade.push(e);
        }
    }
    (update, spawn, fade)
}

/// Alpha of a fading overlay `elapsed` seconds into a fade of `secs` (gone at 0).
pub fn fade_alpha(elapsed: f32, secs: f32) -> f32 {
    if secs <= 0.0 {
        return 0.0;
    }
    (1.0 - elapsed / secs).clamp(0.0, 1.0)
}

#[allow(clippy::too_many_arguments)]
fn draw_screens(
    hud: Res<FullscreenHud>,
    icons: Res<ScreenIcons>,
    vignettes: Option<Res<Vignettes>>,
    time: Res<Time>,
    mut atlas: Option<ResMut<IconAtlas>>,
    root: Single<Entity, With<FullscreenRoot>>,
    mut nodes: Query<(
        Entity,
        &mut ScreenNode,
        &mut ImageNode,
        &mut ZIndex,
        &mut Visibility,
    )>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    let drawn: Vec<(Entity, FullscreenScreen, bool)> = nodes
        .iter()
        .map(|(e, n, ..)| (e, n.screen, n.fading_since.is_some()))
        .collect();
    let (update, spawn, fade) = diff_screens(&drawn, &hud.data.screens);
    for (e, screen) in update {
        if let Ok((_, mut node, ..)) = nodes.get_mut(e) {
            node.screen = screen;
        }
    }
    for e in fade {
        if let Ok((_, mut node, ..)) = nodes.get_mut(e) {
            node.fading_since = Some(now);
        }
    }
    for screen in spawn {
        commands.spawn((
            ScreenNode {
                screen,
                shown_at: now,
                fading_since: None,
            },
            Name::new(format!("fullscreen {:?}", screen.category)),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            // Stretched over the view, as `update_for_view` scales it.
            ImageNode::default().with_mode(NodeImageMode::Stretch),
            ZIndex(screen.category as i32),
            Visibility::Hidden,
            Pickable::IGNORE,
            ChildOf(*root),
        ));
    }
    for (e, node, mut image, mut z, mut visibility) in &mut nodes {
        let alpha = match node.fading_since {
            Some(since) => fade_alpha(now - since, node.screen.fade),
            None => 1.0,
        };
        if alpha <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let smooth = matches!(
            node.screen.category,
            ScreenCategory::Tint | ScreenCategory::Nearsighted
        )
        .then(|| {
            let i = usize::from(node.screen.severity.unwrap_or(1).clamp(1, 2) - 1);
            vignettes.as_deref()?.0.get(i).cloned()
        })
        .flatten();
        let (handle, rect) = if let Some(handle) = smooth {
            (handle, None)
        } else {
            let (dmi, state) = node.screen.icon();
            let Some((info, handle)) = icons
                .0
                .get(&(dmi.to_owned(), state))
                .and_then(|&id| atlas.as_deref_mut()?.state(id))
            else {
                continue;
            };
            let (frame, _) = ss13_render::anim::frame_at(info, now - node.shown_at);
            (handle, frame_rect(info, Dir::South, frame))
        };
        if image.image != handle || image.rect != rect {
            image.image = handle;
            image.rect = rect;
        }
        let color = Color::WHITE.with_alpha(alpha);
        if image.color != color {
            image.color = color;
        }
        z.set_if_neq(ZIndex(node.screen.category as i32));
        visibility.set_if_neq(Visibility::Inherited);
    }
}

/// The colour matrix of `colours` applied in order (`IDENTITY` for none).
pub fn colour_matrix(colours: &[ClientColour]) -> [f32; 20] {
    colours
        .iter()
        .fold(IDENTITY, |m, c| compose(&m, &c.matrix()))
}

/// The colour fade in progress: from what was shown to the target, since when.
#[derive(Resource)]
struct FilterFade {
    from: [f32; 20],
    to: [f32; 20],
    since: f32,
}

impl Default for FilterFade {
    fn default() -> Self {
        Self {
            from: IDENTITY,
            to: IDENTITY,
            since: f32::NEG_INFINITY,
        }
    }
}

impl FilterFade {
    fn at(&self, now: f32) -> [f32; 20] {
        lerp(
            &self.from,
            &self.to,
            (now - self.since) / ClientColour::FADE,
        )
    }
}

/// The world camera's filter follows the colours (fading) and the blur.
fn apply_filter(
    hud: Res<FullscreenHud>,
    time: Res<Time>,
    mut fade: ResMut<FilterFade>,
    cameras: Query<(Entity, &WorldCamera, &Camera, Option<&ScreenFilter>)>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    let target = colour_matrix(&hud.data.colours);
    if target != fade.to {
        *fade = FilterFade {
            from: fade.at(now),
            to: target,
            since: now,
        };
    }
    let matrix = fade.at(now);
    for (entity, world, camera, current) in &cameras {
        let scale = camera.target_scaling_factor().unwrap_or(1.0);
        // BYOND's blur size is in icon pixels; the camera draws `zoom` screen pixels each.
        let blur = hud.data.blur * world.zoom as f32 * scale;
        let filter = ScreenFilter::from_matrix(matrix, blur);
        if filter.is_identity() {
            if current.is_some() {
                commands.entity(entity).remove::<ScreenFilter>();
            }
        } else if current != Some(&filter) {
            commands.entity(entity).insert(filter);
        }
    }
}

/// The overlays cover the map viewport, left of the docked chat.
fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<FullscreenRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vignette_darkens_towards_the_edges_and_more_at_severity_two() {
        for severity in [1, 2] {
            let mut last = 0.0;
            for step in 0..=12 {
                let a = vignette_alpha(step as f32 * 0.1, severity);
                assert!(a >= last && a <= 1.0, "{severity} {step} {a}");
                last = a;
            }
        }
        assert!(vignette_alpha(0.0, 2) > vignette_alpha(0.0, 1));
        assert!(vignette_alpha(1.0, 2) > vignette_alpha(1.0, 1));
    }

    fn screen(category: ScreenCategory, severity: u8) -> FullscreenScreen {
        FullscreenScreen::new(category, Some(severity))
    }

    #[test]
    fn overlays_update_spawn_and_fade_by_category() {
        let (a, b, c) = (
            Entity::from_raw_u32(1).unwrap(),
            Entity::from_raw_u32(2).unwrap(),
            Entity::from_raw_u32(3).unwrap(),
        );
        let drawn = [
            (a, screen(ScreenCategory::Brute, 1), false),
            (b, screen(ScreenCategory::Oxy, 2), false),
            // An oxygen overlay still fading out does not count as shown.
            (c, screen(ScreenCategory::Oxy, 5), true),
        ];
        let shown = [
            screen(ScreenCategory::Brute, 2),
            screen(ScreenCategory::Crit, 0),
        ];
        let (update, spawn, fade) = diff_screens(&drawn, &shown);
        assert_eq!(update, [(a, screen(ScreenCategory::Brute, 2))]);
        assert_eq!(spawn, [screen(ScreenCategory::Crit, 0)]);
        assert_eq!(fade, [b]);
        // Unchanged: nothing to do.
        let (update, spawn, fade) = diff_screens(&drawn[..1], &[screen(ScreenCategory::Brute, 1)]);
        assert!(update.is_empty() && spawn.is_empty() && fade.is_empty());
    }

    #[test]
    fn fades_follow_clear_fullscreen() {
        assert_eq!(fade_alpha(0.0, 1.0), 1.0);
        assert_eq!(fade_alpha(0.5, 1.0), 0.5);
        assert_eq!(fade_alpha(2.5, 2.5), 0.0);
        // `clear_fullscreen("tint", 0)`: gone at once.
        assert_eq!(fade_alpha(0.0, 0.0), 0.0);
    }

    #[test]
    fn every_overlay_state_is_named() {
        let icons = all_icons();
        assert!(icons.contains(&("icons/hud/screen_full.dmi", "passage10".to_owned())));
        assert!(icons.contains(&("icons/hud/screen_full.dmi", "blackimageoverlay".to_owned())));
        assert!(icons.contains(&("icons/hud/screen_gen.dmi", "flash".to_owned())));
        assert!(icons.contains(&("icons/hud/screen_full.dmi", "impairedoverlay2".to_owned())));
    }

    #[test]
    fn colours_compose_by_priority() {
        assert_eq!(colour_matrix(&[]), IDENTITY);
        assert_eq!(
            colour_matrix(&[ClientColour::Monochrome]),
            ClientColour::Monochrome.matrix()
        );
        // Grey after the dim blind colour: every channel equal.
        let m = colour_matrix(&[ClientColour::Blindness, ClientColour::Monochrome]);
        assert!((m[0] - m[1]).abs() < 1e-6 && (m[1] - m[2]).abs() < 1e-6);
    }
}
