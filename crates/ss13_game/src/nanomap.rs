//! BandaStation's nanomap (`modular_bandastation/nanomap`): the map data the camera console and
//! the crew monitor send in their static data (`SSmapping.get_map_ui_data`) and tgui
//! `common/NanoMap`'s pieces both windows draw: the station image of the chosen level under the
//! markers, at the original's default zoom, and the level selector. `ss13-mapload convert`
//! copies the images to the assets' `nanomap/` (`ss13_mapload::nanomap`).
// shortcut: no minimap (tgui's corner overview) and no `stairs` in the data (only the
// navigator uses them, as a list).

use bevy::math::Vec2;
use serde::{Deserialize, Serialize};

/// `<assets>/nanomap/` (`ss13_mapload::nanomap::NANOMAP_DIR`; the client does not link the
/// converter).
pub const NANOMAP_DIR: &str = "nanomap";
/// Tiles on a side of a nanomap image (BYOND `world.maxx` / `maxy`).
pub const TILES: i32 = 255;
/// Window pixels per tile: tgui's 16 px tile at its default scale 0.125.
pub const SCALE: f32 = 2.0;
/// Zoom over the map fitted to its viewport: 1 shows the whole map, up to 8 (tgui `NanoMap`
/// zooms `defaultScale` to `defaultScale * 8`).
pub const MIN_ZOOM: f32 = 1.0;
pub const MAX_ZOOM: f32 = 8.0;
/// One wheel notch multiplies the zoom by this (a smooth zoom; tgui's `smoothStep` 0.0015/px).
pub const WHEEL_ZOOM: f32 = 1.15;
/// A zoom button multiplies or divides it by this.
pub const BUTTON_ZOOM: f32 = 1.5;
/// The window's local state key of the zoom factor.
pub const LOCAL_ZOOM: &str = "nanomap_zoom";

/// The zoom stored under [`LOCAL_ZOOM`] (`value`), 1 when unset or invalid.
pub fn zoom_value(value: Option<&str>) -> f32 {
    value
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|z| z.is_finite())
        .map_or(MIN_ZOOM, |z| z.clamp(MIN_ZOOM, MAX_ZOOM))
}

/// `get_map_ui_data()`: `name` (the map json's `map_name`), the station's levels and the mining
/// level.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapUiData {
    pub name: String,
    pub min_floor: u16,
    pub main_floor: u16,
    pub max_floor: u16,
    pub lavaland_level: Option<u16>,
}

impl MapUiData {
    /// The levels `NanoMapLevelSelector` steps through, lowest first: the station floors, then
    /// the mining level when it lies above them.
    pub fn levels(&self) -> Vec<u16> {
        let mut levels: Vec<u16> = (self.min_floor..=self.max_floor).collect();
        levels.extend(self.lavaland_level.filter(|&z| z > self.max_floor));
        levels
    }

    /// `getMapImage(level)`: the image's file under [`NANOMAP_DIR`]. BYOND's station starts on
    /// z2 (z1 is CentCom), so the original's `z{level - 1}` is the floor counted from 1; the
    /// port's station starts on z1.
    pub fn image(&self, level: u16) -> Option<String> {
        if self.lavaland_level == Some(level) && level != self.min_floor {
            return Some("Lavaland_nanomap_z1.qoi".to_owned());
        }
        (!self.name.is_empty() && (self.min_floor..=self.max_floor).contains(&level))
            .then(|| format!("{}_nanomap_z{}.qoi", self.name, level - self.min_floor + 1))
    }

    /// The selector's label of `level`.
    pub fn level_label(&self, level: u16) -> String {
        if self.lavaland_level == Some(level) && level > self.max_floor {
            "Лаваленд".to_owned()
        } else {
            format!("Этаж {}", level.saturating_sub(self.min_floor) + 1)
        }
    }
}

/// The top-left corner of tile `(x, y)` in a nanomap of [`TILES`] drawn `side` pixels wide
/// (tgui `posToPx`: x from the left, y from the top).
pub fn tile_px(x: i32, y: i32, side: f32) -> (f32, f32) {
    let scale = side / TILES as f32;
    (
        (x.clamp(1, TILES) - 1) as f32 * scale,
        (TILES - y.clamp(1, TILES)) as f32 * scale,
    )
}

/// The map's side at `zoom` in a viewport of `viewport` (logical px): the whole map fits the
/// viewport's shorter side at zoom 1; before the first layout, tgui's default scale.
pub fn map_side(viewport: Vec2, zoom: f32) -> f32 {
    let fit = viewport.min_element();
    let base = if fit > 0.0 { fit } else { TILES as f32 * SCALE };
    base * zoom.clamp(MIN_ZOOM, MAX_ZOOM)
}

#[cfg(feature = "server")]
pub use server::map_ui_data;

#[cfg(feature = "server")]
mod server {
    use bevy::prelude::*;

    use super::MapUiData;

    /// `get_map_ui_data`'s `static map_ui_data`.
    #[derive(Resource)]
    struct MapUiCache(MapUiData);

    /// `SSmapping.get_map_ui_data()`, computed once the station is loaded.
    pub fn map_ui_data(world: &mut World) -> MapUiData {
        if let Some(cache) = world.get_resource::<MapUiCache>() {
            return cache.0.clone();
        }
        let station = crate::events::controller::station_levels(world);
        let (min, max) = (*station.start(), *station.end());
        // The map json of the first map (a name, or a `.dmm` path: its stem names it).
        let json = world
            .get_resource::<ss13_mapload::MapLoadPlugin>()
            .and_then(|p| {
                let map = p.maps.first()?;
                let stem = std::path::Path::new(map).file_stem()?.to_string_lossy();
                let stem = stem.into_owned();
                let json = ss13_mapload::dock::map_json(&p.source, &stem);
                Some((stem, json))
            });
        let name = json.as_ref().map_or_else(String::new, |(stem, json)| {
            json.as_ref()
                .and_then(|j| j.get("map_name")?.as_str().map(str::to_owned))
                .unwrap_or_else(|| stem.clone())
        });
        let main_floor = json
            .as_ref()
            .and_then(|(_, j)| j.as_ref()?.get("main_floor")?.as_u64())
            .and_then(|f| u16::try_from(f).ok())
            .map_or(min, |f| {
                min.saturating_add(f)
                    .saturating_sub(1)
                    .clamp(min, max.max(min))
            });
        let data = MapUiData {
            name,
            min_floor: min,
            main_floor,
            max_floor: max,
            lavaland_level: ss13_mapload::mining_z(world),
        };
        world.insert_resource(MapUiCache(data.clone()));
        data
    }
}

#[cfg(feature = "client")]
pub use client::*;

#[cfg(feature = "client")]
mod client {
    use std::collections::HashMap;

    use bevy::image::ImageSampler;
    use bevy::prelude::*;
    use ss13_render::IconAtlas;
    use ss13_tgui::client::SetLocal;
    use ss13_ui_kit::prelude::*;

    use bevy::picking::events::{PointerDrag, PointerScroll};
    use bevy::ui::{ComputedNode, ScrollPosition, UiGlobalTransform};
    use ss13_tgui::client::{ClientWindow, UiLocalState};

    use super::{
        BUTTON_ZOOM, LOCAL_ZOOM, MAX_ZOOM, MIN_ZOOM, MapUiData, NANOMAP_DIR, WHEEL_ZOOM, map_side,
        tile_px,
    };

    /// The scrolling viewport of a map (`NanoMap`'s `TransformWrapper`): the map fits it at zoom 1
    /// ([`fit_maps`] follows its size), the wheel zooms smoothly around the cursor
    /// ([`wheel_zoom`]), a drag pans ([`drag_pan`]).
    #[derive(Component, Clone, Copy, Debug)]
    #[require(ScrollPosition, ss13_ui_kit::scroll::ScrollContent)]
    pub struct NanomapViewport {
        /// The zoom over the fitted map, [`MIN_ZOOM`]..=[`MAX_ZOOM`].
        pub zoom: f32,
    }

    impl Default for NanomapViewport {
        fn default() -> Self {
            Self { zoom: MIN_ZOOM }
        }
    }

    /// The map square inside a [`NanomapViewport`]: its size follows the viewport and the zoom.
    #[derive(Component, Clone, Copy, Debug, Default)]
    pub struct NanomapSquare;

    /// A marker on the map: its tile and half its size, placed again when the map is resized.
    #[derive(Component, Clone, Copy, Debug, Default)]
    pub struct NanomapMarker {
        pub x: i32,
        pub y: i32,
        pub half: f32,
    }

    type Squares<'w, 's> =
        Query<'w, 's, (&'static mut Node, Option<&'static Children>), With<NanomapSquare>>;
    type Markers<'w, 's> =
        Query<'w, 's, (&'static mut Node, &'static NanomapMarker), Without<NanomapSquare>>;

    /// Sizes the squares under `children` to `side` and places their markers; writes only on a
    /// difference, so a settled map is not laid out again.
    fn place(side: f32, children: &Children, squares: &mut Squares, markers: &mut Markers) {
        for child in children.iter() {
            let Ok((mut node, items)) = squares.get_mut(child) else {
                continue;
            };
            if node.width != px(side) {
                (node.width, node.height) = (px(side), px(side));
                (node.min_width, node.min_height) = (px(side), px(side));
            }
            for item in items.into_iter().flatten() {
                if let Ok((mut node, marker)) = markers.get_mut(*item) {
                    let (x, y) = tile_px(marker.x, marker.y, side);
                    let (left, top) = (px(x - marker.half), px(y - marker.half));
                    if node.left != left || node.top != top {
                        (node.left, node.top) = (left, top);
                    }
                }
            }
        }
    }

    fn logical_size(computed: &ComputedNode) -> Vec2 {
        computed.size * computed.inverse_scale_factor
    }

    /// Fits each map to its viewport's last laid out size (the window resized, a new render).
    pub fn fit_maps(
        viewports: Query<(&NanomapViewport, &ComputedNode, &Children)>,
        mut squares: Squares,
        mut markers: Markers,
    ) {
        for (viewport, computed, children) in &viewports {
            let side = map_side(logical_size(computed), viewport.zoom);
            place(side, children, &mut squares, &mut markers);
        }
    }

    /// The wheel over a map: a smooth zoom ([`WHEEL_ZOOM`] a notch), the point under the cursor
    /// kept in place. Nodes are resized in place (no re-render); the zoom is kept in the window's
    /// local state for the renders that follow.
    #[allow(clippy::too_many_arguments)]
    pub fn wheel_zoom(
        mut scroll: On<PointerScroll>,
        mut viewports: Query<(
            &mut NanomapViewport,
            &mut ScrollPosition,
            &ComputedNode,
            &UiGlobalTransform,
            &Children,
        )>,
        mut squares: Squares,
        mut markers: Markers,
        parents: Query<&ChildOf>,
        mut windows: Query<&mut UiLocalState, With<ClientWindow>>,
    ) {
        let Ok((mut viewport, mut position, computed, transform, children)) =
            viewports.get_mut(scroll.entity)
        else {
            return;
        };
        scroll.propagate(false);
        // Notches of a wheel; a touchpad's pixels count as fractions of one.
        let notches = match scroll.unit {
            bevy::input::mouse::MouseScrollUnit::Line => scroll.y,
            bevy::input::mouse::MouseScrollUnit::Pixel => scroll.y / 40.0,
        };
        if notches == 0.0 {
            return;
        }
        let from = viewport.zoom;
        let to = (from * WHEEL_ZOOM.powf(notches)).clamp(MIN_ZOOM, MAX_ZOOM);
        if (to - from).abs() < f32::EPSILON {
            return;
        }
        // The cursor in the viewport's logical pixels.
        let inverse = computed.inverse_scale_factor;
        let size = computed.size * inverse;
        let center = transform.translation * inverse;
        let cursor = (scroll.pointer.position - (center - size / 2.0)).clamp(Vec2::ZERO, size);
        let factor = to / from;
        position.0 = ((position.0 + cursor) * factor - cursor).max(Vec2::ZERO);
        viewport.zoom = to;
        place(map_side(size, to), children, &mut squares, &mut markers);
        if let Some(window) = parents
            .iter_ancestors(scroll.entity)
            .find(|e| windows.contains(*e))
            && let Ok(mut local) = windows.get_mut(window)
        {
            local.set(LOCAL_ZOOM, &to.to_string());
        }
    }

    /// Dragging a map pans it (the markers' clicks still go through: a click is no drag).
    pub fn drag_pan(
        mut drag: On<PointerDrag>,
        mut viewports: Query<&mut ScrollPosition, With<NanomapViewport>>,
        scale: Res<UiScale>,
    ) {
        let Ok(mut position) = viewports.get_mut(drag.entity) else {
            return;
        };
        drag.propagate(false);
        position.0 = (position.0 - drag.delta / scale.0).max(Vec2::ZERO);
    }

    /// On a map's node: the client gives it the image `file` under [`NANOMAP_DIR`] as its
    /// `ImageNode`; nothing when the assets have no such image (a map without one).
    #[derive(Component, Clone, Debug, Default, PartialEq)]
    #[require(Node)]
    pub struct NanomapImage(pub String);

    /// Decoded images by file; `None`: missing or unreadable.
    #[derive(Resource, Default)]
    pub struct NanomapImages(HashMap<String, Option<Handle<Image>>>);

    /// Gives each new [`NanomapImage`] its image, read and decoded on first use.
    // shortcut: the first decode (a 2040 px QOI, a few ms) blocks the frame once per level and
    // session; move it to the IO task pool if it shows in frame times.
    pub fn resolve_images(
        maps: Query<(Entity, &NanomapImage), Changed<NanomapImage>>,
        atlas: Option<Res<IconAtlas>>,
        mut cache: ResMut<NanomapImages>,
        mut images: ResMut<Assets<Image>>,
        mut commands: Commands,
    ) {
        let Some(atlas) = atlas else {
            return;
        };
        for (entity, map) in &maps {
            if map.0.is_empty() {
                continue;
            }
            let handle = cache
                .0
                .entry(map.0.clone())
                .or_insert_with(|| {
                    let path = atlas.root().join(NANOMAP_DIR).join(&map.0);
                    let bytes = ss13_data::read(&path).ok()?;
                    let image = ss13_render::atlas::qoi_image(&bytes, ImageSampler::linear())
                        .inspect_err(|e| warn!("nanomap {}: {e}", path.display()))
                        .ok()?;
                    Some(images.add(image))
                })
                .clone();
            if let Some(image) = handle {
                commands
                    .entity(entity)
                    .insert(ImageNode { image, ..default() });
            }
        }
    }

    /// `NanoMapLevelSelector`: a tab per level of `map` (the chosen one selected) storing it under
    /// `key` of the window's local state.
    pub fn level_tabs(map: &MapUiData, level: Option<u16>, key: &str) -> Vec<impl Scene + use<>> {
        map.levels()
            .into_iter()
            .map(|z| {
                let pick = SetLocal {
                    key: key.to_owned(),
                    value: z.to_string(),
                    rerender: true,
                };
                let selected = level == Some(z);
                let label = map.level_label(z);
                bsn! {
                    @TabsTab { @content: label }
                    TabsTab { selected: selected }
                    pick
                }
            })
            .collect()
    }

    /// The zoom buttons (tgui `NanoMap`'s zoom in/out): the zoom divided or multiplied by
    /// [`BUTTON_ZOOM`], kept under [`LOCAL_ZOOM`]; markers keep their size (`KeepScale`).
    pub fn zoom_buttons(zoom: f32) -> Vec<impl Scene + use<>> {
        [("minus", zoom / BUTTON_ZOOM), ("plus", zoom * BUTTON_ZOOM)]
            .into_iter()
            .map(|(icon, to)| {
                let to = to.clamp(MIN_ZOOM, MAX_ZOOM);
                let disabled = (to - zoom).abs() < 1e-3;
                let pick = SetLocal {
                    key: LOCAL_ZOOM.to_owned(),
                    value: to.to_string(),
                    rerender: true,
                };
                bsn! {
                    @ss13_ui_kit::Button { @icon: icon }
                    ss13_ui_kit::Button { disabled: disabled }
                    pick
                }
            })
            .collect()
    }

    /// The map: a [`NanomapViewport`] filling its parent around the square of the level's image
    /// (when the assets have it) under `markers`, which carry a [`NanomapMarker`] and are placed
    /// by [`fit_maps`] once the viewport has a size. The square never shrinks, so the image and
    /// the markers keep one scale.
    pub fn map_area<L: SceneList>(
        map: &MapUiData,
        level: Option<u16>,
        zoom: f32,
        markers: L,
    ) -> impl Scene + use<L> {
        let side = map_side(Vec2::ZERO, zoom);
        let viewport = NanomapViewport {
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
        };
        let image = NanomapImage(level.and_then(|z| map.image(z)).unwrap_or_default());
        let backdrop = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35));
        let name = Name::new("nanomap");
        bsn! {
            Node {
                width: percent(100),
                height: percent(100),
                min_height: px(0),
                overflow: Overflow::scroll(),
            }
            viewport
            Children [
                Node {
                    width: px(side),
                    height: px(side),
                    min_width: px(side),
                    min_height: px(side),
                    flex_shrink: 0.0,
                }
                NanomapSquare
                backdrop
                image
                name
                Children [ {markers} ]
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> MapUiData {
        MapUiData {
            name: "MetaStation".into(),
            min_floor: 1,
            main_floor: 1,
            max_floor: 1,
            lavaland_level: Some(2),
        }
    }

    #[test]
    fn images_follow_the_original_keys() {
        let map = meta();
        assert_eq!(map.levels(), [1, 2]);
        assert_eq!(map.image(1).as_deref(), Some("MetaStation_nanomap_z1.qoi"));
        assert_eq!(map.image(2).as_deref(), Some("Lavaland_nanomap_z1.qoi"));
        assert_eq!(map.image(3), None);
        assert_eq!(map.level_label(2), "Лаваленд");
        let icebox = MapUiData {
            name: "Ice Box Station".into(),
            min_floor: 1,
            main_floor: 3,
            max_floor: 3,
            lavaland_level: None,
        };
        assert_eq!(icebox.levels(), [1, 2, 3]);
        assert_eq!(
            icebox.image(3).as_deref(),
            Some("Ice Box Station_nanomap_z3.qoi")
        );
        assert_eq!(icebox.level_label(2), "Этаж 2");
        // No map data (a fixture without a station): no image.
        assert_eq!(MapUiData::default().image(0), None);
        // A 510 px map: 2 px a tile.
        assert_eq!(tile_px(1, 255, 510.0), (0.0, 0.0));
        assert_eq!(tile_px(11, 245, 510.0), (20.0, 20.0));
        // The map fits the viewport's shorter side, times the zoom.
        assert_eq!(map_side(Vec2::new(600.0, 400.0), 1.0), 400.0);
        assert_eq!(map_side(Vec2::new(600.0, 400.0), 2.5), 1000.0);
        assert_eq!(map_side(Vec2::new(600.0, 400.0), 20.0), 3200.0);
        assert_eq!(zoom_value(Some("2.5")), 2.5);
        assert_eq!(zoom_value(Some("40")), MAX_ZOOM);
        assert_eq!(zoom_value(Some("x")), MIN_ZOOM);
        assert_eq!(zoom_value(None), MIN_ZOOM);
    }
}
