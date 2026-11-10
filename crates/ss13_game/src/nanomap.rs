//! BandaStation's nanomap (`modular_bandastation/nanomap`): the map data the camera console and
//! the crew monitor send in their static data (`SSmapping.get_map_ui_data`) and tgui
//! `common/NanoMap`'s pieces both windows draw: the station image of the chosen level under the
//! markers, at the original's default zoom, and the level selector. `ss13-mapload convert`
//! copies the images to the assets' `nanomap/` (`ss13_mapload::nanomap`).
// shortcut: no zoom, panning or minimap, and no `stairs` in the data (only the navigator uses
// them, as a list); port them with a map view that zooms.

use serde::{Deserialize, Serialize};

/// `<assets>/nanomap/` (`ss13_mapload::nanomap::NANOMAP_DIR`; the client does not link the
/// converter).
pub const NANOMAP_DIR: &str = "nanomap";
/// Tiles on a side of a nanomap image (BYOND `world.maxx` / `maxy`).
pub const TILES: i32 = 255;
/// Window pixels per tile: tgui's 16 px tile at its default scale 0.125.
pub const SCALE: f32 = 2.0;

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

/// The top-left corner of tile `(x, y)` in a nanomap of [`TILES`] at [`SCALE`] (tgui `posToPx`:
/// x from the left, y from the top).
pub fn tile_px(x: i32, y: i32) -> (f32, f32) {
    (
        (x.clamp(1, TILES) - 1) as f32 * SCALE,
        (TILES - y.clamp(1, TILES)) as f32 * SCALE,
    )
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

    use super::{MapUiData, NANOMAP_DIR, SCALE, TILES};

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

    /// The map square: the level's image (when the assets have it) under `markers`, which
    /// position themselves absolutely by [`super::tile_px`].
    pub fn map_area<L: SceneList>(
        map: &MapUiData,
        level: Option<u16>,
        markers: L,
    ) -> impl Scene + use<L> {
        let side = TILES as f32 * SCALE;
        let image = NanomapImage(level.and_then(|z| map.image(z)).unwrap_or_default());
        let backdrop = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35));
        let name = Name::new("nanomap");
        bsn! {
            Node { width: px(side), height: px(side) }
            backdrop
            image
            name
            Children [ {markers} ]
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
        assert_eq!(tile_px(1, 255), (0.0, 0.0));
        assert_eq!(tile_px(11, 245), (20.0, 20.0));
    }
}
