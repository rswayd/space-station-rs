//! Draws replicated `Appearance` + `Facing` + `TilePos`. Knows nothing about the network.
//!
//! Every entity with `Appearance` and `TilePos` gets a `Sprite` (and child sprites for overlays),
//! positioned at its tile, ordered by plane/layer (see [`order`]), animated by `.dmi` delays.

pub mod anim;
pub mod atlas;
pub mod camera;
pub mod filter;
pub mod lighting;
pub mod order;
pub mod parallax;
pub mod sprites;
pub mod synthetic;

use std::path::PathBuf;

use bevy::prelude::*;

pub use atlas::{AlphaMask, IconAtlas};
pub use camera::{CameraTarget, MapInsets, WorldCamera};
pub use filter::ScreenFilter;
pub use lighting::{LightingPlugin, LightingSettings};
pub use order::DrawOrder;
pub use parallax::ParallaxPlugin;
pub use sprites::{Animated, IconSprite, Overlays};

/// Default location of the importer output, relative to the working directory.
pub const DEFAULT_ASSET_ROOT: &str = "assets/generated";

pub struct RenderPlugin {
    /// Directory with `manifest.postcard` and the atlases it lists.
    pub asset_root: PathBuf,
}

impl Default for RenderPlugin {
    fn default() -> Self {
        Self {
            asset_root: DEFAULT_ASSET_ROOT.into(),
        }
    }
}

/// The z-level that is drawn: the `CameraTarget`'s, or set directly when there is no target.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveZ(pub u16);

impl Default for ActiveZ {
    /// BYOND z-levels start at 1.
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Resource)]
struct AssetRoot(PathBuf);

/// Runs in `PostUpdate` before transform propagation, so game code in `Update` is
/// drawn in the same frame.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RenderSystems;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(filter::ScreenFilterPlugin)
            .insert_resource(AssetRoot(self.asset_root.clone()))
            .init_resource::<IconAtlas>()
            .init_resource::<DrawOrder>()
            .init_resource::<ActiveZ>()
            .init_resource::<MapInsets>()
            .init_resource::<camera::CameraOffset>()
            .add_systems(PreStartup, open_atlas)
            .add_systems(Startup, camera::spawn_camera)
            .configure_sets(
                PostUpdate,
                RenderSystems.before(bevy::transform::TransformSystems::Propagate),
            )
            .add_systems(
                PostUpdate,
                (
                    sprites::strip_removed,
                    sprites::sync_appearance,
                    sprites::sync_tile_pos,
                    sprites::animate,
                    atlas_pages,
                    camera::apply_map_viewport,
                    camera::follow_target,
                )
                    .chain()
                    .in_set(RenderSystems),
            );
    }
}

fn open_atlas(mut commands: Commands, root: Res<AssetRoot>, images: Res<Assets<Image>>) {
    let budget = atlas::budget_from_env();
    match IconAtlas::open(&root.0, images.get_handle_provider(), budget) {
        Ok(atlas) => {
            info!(
                "{} icon states in {} atlas pages at {}, loaded on demand, budget {} MiB",
                atlas.manifest.states.len(),
                atlas.manifest.atlases.len(),
                root.0.display(),
                budget >> 20
            );
            commands.insert_resource(atlas);
        }
        // The client still runs; every sprite shows the magenta "missing" tile.
        Err(e) => error!("icon atlas not loaded: {e:#}"),
    }
}

/// Inserts decoded pages and evicts idle ones. Cheap per frame: one pass over the page list
/// (tens of entries).
fn atlas_pages(mut atlas: ResMut<IconAtlas>, mut images: ResMut<Assets<Image>>, time: Res<Time>) {
    let loaded = atlas.finish_loads(|handle, image| {
        if let Err(e) = images.insert(handle.id(), image) {
            error!("atlas page not stored: {e}");
        }
    });
    let evicted = atlas.evict(time.elapsed_secs_f64());
    if loaded.is_empty() && evicted.is_empty() {
        return;
    }
    let stats = atlas.stats();
    for (page, bytes, took) in loaded {
        info!(
            "atlas page {page} loaded ({} MiB) in {took:.0?}",
            bytes >> 20
        );
    }
    if !evicted.is_empty() {
        info!("atlas pages {evicted:?} evicted (idle, over budget)");
    }
    info!(
        "atlas: {}/{} pages resident, {} MiB, alpha masks {} KiB",
        stats.loaded,
        stats.pages,
        stats.resident_bytes >> 20,
        stats.mask_bytes >> 10
    );
}
