//! tg-style static lighting on the client, from replicated light data only.
//!
//! The server sends light sources and opacity per light block ([`LightUpdate`]: 32 bytes of
//! opacity bits and each distinct light value once per block, from a box wider than the
//! replicated entities, so sources beyond the view light the screen edge) and tile lighting with
//! entities ([`Optics::tile`]); entities carrying `Optics` light sources or opacity are drawn
//! too (tests, offline tools). Each client computes tg's per-corner lighting ([`math`]) for what it sees and draws it as
//! a lightmap that multiplies the world planes below tg's `LIGHTING_PLANE` (10), as tg's
//! lighting plate does with `BLEND_MULTIPLY`. Planes above it (UI, `ABOVE_LIGHTING_PLANE`) are
//! not darkened.
//!
//! Emissive overlays (`EMISSIVE_PLANE`: screens, bulbs, indicator lights) are drawn by a second
//! camera into a window-sized mask that the lightmap adds to the light, as tg adds its emissive
//! plate to the lighting plate: what is under a mask shows unlit. tg's emissive blockers (mobs
//! and objects hiding a screen behind them) are not drawn, so a mob in front of a glowing pixel
//! is lit there too.
//!
//! Work is incremental:
//! - a source's corner contributions are cached and recomputed only when it changes or the
//!   opacity of a tile in its reach changes (a door opening);
//! - the lightmap textures (one texel per corner and per tile of the camera's region) are
//!   rebuilt from the caches when a source, a tile or the region changes, which costs one pass
//!   over the cached corners of the sources in the region.

pub mod math;

use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::asset::embedded_asset;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::ecs::entity::EntityHashMap;
use bevy::image::ImageSampler;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::platform::collections::HashMap;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d,
    RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{
    AlphaMode2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin,
    Mesh2dPipelineKey, MeshMaterial2d,
};
use ss13_shared::{
    LIGHT_BLOCK, LightSource, LightUpdate, Optics, TILE_SIZE, TileLighting, TilePos, light_block,
};

use crate::camera::WorldCamera;
use crate::order::DrawOrder;
use crate::sprites::EMISSIVE_LAYER;
use crate::{ActiveZ, RenderSystems};

/// tg `LIGHTING_PLANE`: planes below it are lit, planes from it up are not.
pub const LIGHTING_PLANE: i16 = 10;

/// Tiles of margin around the camera's view, so gliding never shows the region's edge.
const MARGIN: i32 = 2;

/// Client lighting. Needs [`RenderPlugin`](crate::RenderPlugin).
pub struct LightingPlugin;

/// Turns lighting on or off at runtime (off: the world is fullbright and nothing is computed).
#[derive(Resource, Clone, Copy, Debug)]
pub struct LightingSettings {
    pub enabled: bool,
}

impl Default for LightingSettings {
    /// `SS13_LIGHTING=0` disables lighting (for comparisons); default on.
    fn default() -> Self {
        Self {
            enabled: std::env::var("SS13_LIGHTING").map_or(true, |v| v != "0"),
        }
    }
}

/// Cost of the lighting systems, for logs and benchmarks.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct LightingStats {
    /// Sources currently known / on the drawn z-level and in the region.
    pub sources: usize,
    pub sources_in_region: usize,
    /// Wall time of the last frame's lighting systems, and the longest since the last log line.
    pub frame_time: Duration,
    pub frame_time_max: Duration,
    /// Since start: source recomputations and lightmap rebuilds.
    pub source_updates: u64,
    pub rebuilds: u64,
}

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "lightmap.wgsl");
        app.add_plugins(Material2dPlugin::<LightmapMaterial>::default())
            .init_resource::<LightingSettings>()
            .init_resource::<LightingStats>()
            .init_resource::<LightWorld>()
            .add_message::<LightUpdate>()
            .add_systems(Startup, spawn_lightmap)
            .add_systems(
                PostUpdate,
                (
                    track_optics,
                    update_sources,
                    sync_emissive_camera,
                    update_lightmap,
                    log_stats,
                )
                    .chain()
                    .in_set(RenderSystems)
                    .after(crate::camera::follow_target),
            );
    }
}

/// The lighting state the client knows, kept in sync with [`LightUpdate`]s and `Optics` +
/// `TilePos`.
#[derive(Resource, Default)]
pub struct LightWorld {
    /// Opaque entities per tile.
    opaque: HashMap<TilePos, u16>,
    /// Opacity bits of the known light blocks, by origin.
    blocks: HashMap<TilePos, [u8; 32]>,
    /// Number of block light sources per tile (keys `SourceKey::Tile(pos, 0..n)`).
    tile_lights: HashMap<TilePos, u8>,
    /// Non-default tile lighting (turfs).
    tiles: HashMap<TilePos, TileLighting>,
    sources: HashMap<SourceKey, Source>,
    /// What each tracked entity contributed, to undo it on change or removal.
    tracked: EntityHashMap<(TilePos, Optics)>,
    /// Tiles whose opacity changed since the sources were last updated.
    dirty_tiles: Vec<TilePos>,
    /// Something in the lightmap may have changed.
    changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SourceKey {
    Entity(Entity),
    /// The `n`th light source of a light block on a tile.
    Tile(TilePos, u8),
}

struct Source {
    pos: TilePos,
    light: LightSource,
    /// Corner offsets from `pos` and luminosity; `None` = needs recomputing.
    corners: Option<Vec<(i32, i32, [f32; 3])>>,
}

impl LightWorld {
    pub fn is_opaque(&self, pos: TilePos) -> bool {
        opaque_at(&self.opaque, &self.blocks, pos)
    }

    /// Applies one block update; tiles whose opacity flips are marked dirty.
    fn update(&mut self, update: &LightUpdate) {
        match update {
            LightUpdate::Opacity { origin, bits } => self.set_block_opacity(*origin, *bits),
            LightUpdate::Lights { pos, lights } => self.set_tile_lights(*pos, lights),
            LightUpdate::Forget { origin } => {
                self.set_block_opacity(*origin, [0; 32]);
                self.blocks.remove(origin);
                for i in 0..LIGHT_BLOCK * LIGHT_BLOCK {
                    let pos = TilePos::new(
                        origin.x + i % LIGHT_BLOCK,
                        origin.y + i / LIGHT_BLOCK,
                        origin.z,
                    );
                    self.set_tile_lights(pos, &[]);
                }
            }
        }
    }

    fn set_block_opacity(&mut self, origin: TilePos, bits: [u8; 32]) {
        let old = self.blocks.insert(origin, bits).unwrap_or([0; 32]);
        for (byte, (&a, &b)) in old.iter().zip(&bits).enumerate() {
            let mut flipped = a ^ b;
            while flipped != 0 {
                let i = byte as i32 * 8 + flipped.trailing_zeros() as i32;
                flipped &= flipped - 1;
                self.dirty_tiles.push(TilePos::new(
                    origin.x + i % LIGHT_BLOCK,
                    origin.y + i / LIGHT_BLOCK,
                    origin.z,
                ));
            }
        }
    }

    fn set_tile_lights(&mut self, pos: TilePos, lights: &[LightSource]) {
        let old = self.tile_lights.remove(&pos).unwrap_or(0);
        if old == 0 && lights.is_empty() {
            return;
        }
        for n in 0..old {
            self.sources.remove(&SourceKey::Tile(pos, n));
        }
        // A tile holds a few lights at most; u8::MAX is far above any real one.
        let n = lights.len().min(usize::from(u8::MAX));
        for (i, &light) in lights[..n].iter().enumerate() {
            self.sources.insert(
                SourceKey::Tile(pos, i as u8),
                Source {
                    pos,
                    light,
                    corners: None,
                },
            );
        }
        if n > 0 {
            self.tile_lights.insert(pos, n as u8);
        }
        self.changed = true;
    }

    fn apply(&mut self, entity: Entity, pos: TilePos, optics: &Optics, sign: i32) {
        if optics.opaque {
            let count = self.opaque.entry(pos).or_default();
            *count = count.saturating_add_signed(sign as i16);
            if *count == 0 {
                self.opaque.remove(&pos);
            }
            self.dirty_tiles.push(pos);
        }
        if let Some(tile) = optics.tile {
            if sign > 0 {
                self.tiles.insert(pos, tile);
            } else if self.tiles.get(&pos) == Some(&tile) {
                self.tiles.remove(&pos);
            }
            self.changed = true;
        }
        if let Some(light) = optics.light {
            if sign > 0 {
                self.sources.insert(
                    SourceKey::Entity(entity),
                    Source {
                        pos,
                        light,
                        corners: None,
                    },
                );
            } else {
                self.sources.remove(&SourceKey::Entity(entity));
            }
            self.changed = true;
        }
    }

    fn untrack(&mut self, entity: Entity) {
        if let Some((pos, optics)) = self.tracked.remove(&entity) {
            self.apply(entity, pos, &optics, -1);
        }
    }
}

/// An opaque entity or a set block bit.
fn opaque_at(
    opaque: &HashMap<TilePos, u16>,
    blocks: &HashMap<TilePos, [u8; 32]>,
    pos: TilePos,
) -> bool {
    opaque.contains_key(&pos) || {
        let (origin, i) = light_block(pos);
        blocks
            .get(&origin)
            .is_some_and(|bits| bits[i / 8] & (1 << (i % 8)) != 0)
    }
}

/// Follows light block updates, `Optics` and `TilePos` changes, despawns and removals.
fn track_optics(
    settings: Res<LightingSettings>,
    mut world: ResMut<LightWorld>,
    mut updates: MessageReader<LightUpdate>,
    changed: Query<(Entity, &TilePos, &Optics), Or<(Changed<Optics>, Changed<TilePos>)>>,
    mut lost_optics: RemovedComponents<Optics>,
    mut lost_pos: RemovedComponents<TilePos>,
    mut stats: ResMut<LightingStats>,
) {
    let start = Instant::now();
    let world = &mut *world;
    // Tracked even while disabled, so turning lighting on needs no resync.
    for update in updates.read() {
        world.update(update);
    }
    for entity in lost_optics.read().chain(lost_pos.read()) {
        world.untrack(entity);
    }
    for (entity, &pos, optics) in &changed {
        if world.tracked.get(&entity) == Some(&(pos, optics.clone())) {
            continue;
        }
        world.untrack(entity);
        world.apply(entity, pos, optics, 1);
        world.tracked.insert(entity, (pos, optics.clone()));
    }
    if settings.is_changed() {
        world.changed = true;
    }
    stats.frame_time = start.elapsed();
}

/// Recomputes sources in reach of changed opacity, and new or moved ones, on the drawn z-level.
fn update_sources(
    settings: Res<LightingSettings>,
    active_z: Res<ActiveZ>,
    mut world: ResMut<LightWorld>,
    mut stats: ResMut<LightingStats>,
) {
    if !settings.enabled {
        return;
    }
    let start = Instant::now();
    let world = &mut *world;
    let dirty = DirtyTiles::new(std::mem::take(&mut world.dirty_tiles));
    let LightWorld {
        sources,
        opaque,
        blocks,
        changed,
        ..
    } = world;
    for source in sources.values_mut() {
        let reach = math::reach(&source.light);
        if source.corners.is_some() && dirty.near(source.pos, reach + 1) {
            source.corners = None;
        }
        if source.corners.is_some() || source.pos.z != active_z.0 {
            continue;
        }
        let at = source.pos;
        source.corners = Some(math::contribution(&source.light, |dx, dy| {
            opaque_at(opaque, blocks, TilePos::new(at.x + dx, at.y + dy, at.z))
        }));
        stats.source_updates += 1;
        *changed = true;
    }
    stats.sources = sources.len();
    stats.frame_time += start.elapsed();
}

/// Tiles whose opacity changed, bucketed by light block: a new block brings a hundred walls at
/// once, and each of the region's sources checks only the buckets its reach overlaps.
struct DirtyTiles(HashMap<TilePos, Vec<TilePos>>);

impl DirtyTiles {
    fn new(tiles: Vec<TilePos>) -> Self {
        let mut buckets: HashMap<TilePos, Vec<TilePos>> = HashMap::default();
        for t in tiles {
            buckets.entry(light_block(t).0).or_default().push(t);
        }
        Self(buckets)
    }

    /// Whether a dirty tile is within Chebyshev `reach` of `pos`.
    fn near(&self, pos: TilePos, reach: i32) -> bool {
        if self.0.is_empty() {
            return false;
        }
        let lo = light_block(TilePos::new(pos.x - reach, pos.y - reach, pos.z)).0;
        let hi = light_block(TilePos::new(pos.x + reach, pos.y + reach, pos.z)).0;
        let step = LIGHT_BLOCK as usize;
        (lo.y..=hi.y).step_by(step).any(|y| {
            (lo.x..=hi.x).step_by(step).any(|x| {
                self.0.get(&TilePos::new(x, y, pos.z)).is_some_and(|tiles| {
                    tiles
                        .iter()
                        .any(|t| t.chebyshev(pos).is_some_and(|d| d <= reach as u32))
                })
            })
        })
    }
}

/// Tile rect `[min, max)` of the region drawn around the camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct Region {
    min: IVec2,
    size: UVec2,
    z: u16,
}

#[derive(Component)]
struct Lightmap {
    region: Option<Region>,
    corners: Handle<Image>,
    tiles: Handle<Image>,
    /// The emissive camera's target.
    emissive: Handle<Image>,
}

/// Draws the [`EMISSIVE_LAYER`] sprites with the world camera's view into the emissive mask.
#[derive(Component)]
struct EmissiveCamera;

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct LightmapMaterial {
    /// Region origin (tiles) in xy, size (tiles) in zw.
    #[uniform(0)]
    region: Vec4,
    #[texture(1)]
    #[sampler(2)]
    corners: Handle<Image>,
    #[texture(3)]
    tiles: Handle<Image>,
    /// Emissive mask, one texel per target pixel (alpha = coverage).
    #[texture(4)]
    emissive: Handle<Image>,
}

impl Material2d for LightmapMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://ss13_render/lighting/lightmap.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        // Sorted with the sprites by z (transparent phase).
        AlphaMode2d::Blend
    }

    fn specialize(
        _pipeline: &Material2dPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let Some(fragment) = descriptor.fragment.as_mut() else {
            return Ok(());
        };
        // The shader has two entry points, so one must always be named.
        fragment.entry_point = Some(
            if key.mesh_key.contains(Mesh2dPipelineKey::SRGB_COMPOSITING) {
                "fragment_srgb"
            } else {
                "fragment"
            }
            .into(),
        );
        // tg's lighting plate: world * light; the target's alpha stays.
        let multiply = BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::Dst,
                dst_factor: BlendFactor::Zero,
                operation: BlendOperation::Add,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::Zero,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
        };
        for target in fragment.targets.iter_mut().flatten() {
            target.blend = Some(multiply);
        }
        Ok(())
    }
}

fn lightmap_image(size: UVec2, linear: bool) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: size.x.max(1),
            height: size.y.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.sampler = if linear {
        ImageSampler::linear()
    } else {
        ImageSampler::nearest()
    };
    image
}

fn spawn_lightmap(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<LightmapMaterial>>,
) {
    let corners = images.add(lightmap_image(UVec2::ONE, true));
    let tiles = images.add(lightmap_image(UVec2::ONE, false));
    let emissive = images.add(emissive_image(UVec2::ONE));
    commands.spawn((
        Name::new("lightmap"),
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(materials.add(LightmapMaterial {
            region: Vec4::ZERO,
            corners: corners.clone(),
            tiles: tiles.clone(),
            emissive: emissive.clone(),
        })),
        Transform::default(),
        Visibility::Hidden,
        Lightmap {
            region: None,
            corners,
            tiles,
            emissive,
        },
    ));
}

fn emissive_image(size: UVec2) -> Image {
    Image::new_target_texture(
        size.x.max(1),
        size.y.max(1),
        TextureFormat::Rgba8Unorm,
        None,
    )
}

type WorldView<'a> = (&'a Camera, &'a Projection, &'a Transform);

/// Keeps the emissive camera on the world camera's view, viewport and target size; spawns it on
/// first use. Cost per frame: a few component compares; the mask is re-allocated only when the
/// window size changes.
fn sync_emissive_camera(
    settings: Res<LightingSettings>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    lightmaps: Query<&Lightmap>,
    world_cameras: Query<WorldView, (With<WorldCamera>, Without<EmissiveCamera>)>,
    mut emissive: Query<
        (
            Entity,
            &mut Camera,
            &mut Projection,
            &mut Transform,
            &RenderTarget,
        ),
        With<EmissiveCamera>,
    >,
) {
    let (Ok(lightmap), Some((camera, projection, transform))) =
        (lightmaps.single(), world_cameras.iter().next())
    else {
        return;
    };
    let (Some(size), Some(scale)) = (
        camera.physical_target_size(),
        camera.target_scaling_factor(),
    ) else {
        return;
    };
    if images
        .get(&lightmap.emissive)
        .is_some_and(|image| image.size() != size)
        && let Some(mut image) = images.get_mut(&lightmap.emissive)
    {
        *image = emissive_image(size);
    }
    let target = RenderTarget::Image(ImageRenderTarget {
        handle: lightmap.emissive.clone(),
        scale_factor: scale,
    });
    let Ok((entity, mut e_camera, mut e_projection, mut e_transform, e_target)) =
        emissive.single_mut()
    else {
        commands.spawn((
            Name::new("emissive camera"),
            Camera2d,
            Camera {
                order: -1,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                viewport: camera.viewport.clone(),
                is_active: settings.enabled,
                ..default()
            },
            target,
            Msaa::Off,
            RenderLayers::layer(EMISSIVE_LAYER),
            projection.clone(),
            *transform,
            EmissiveCamera,
        ));
        return;
    };
    if e_camera.is_active != settings.enabled {
        e_camera.is_active = settings.enabled;
    }
    let same_viewport = match (&e_camera.viewport, &camera.viewport) {
        (Some(a), Some(b)) => {
            a.physical_position == b.physical_position && a.physical_size == b.physical_size
        }
        (None, None) => true,
        _ => false,
    };
    if !same_viewport {
        e_camera.viewport = camera.viewport.clone();
    }
    if let (Projection::Orthographic(a), Projection::Orthographic(b)) =
        (e_projection.as_ref(), projection)
        && a.scale != b.scale
    {
        *e_projection = projection.clone();
    }
    if *e_transform != *transform {
        *e_transform = *transform;
    }
    let stale = match e_target {
        RenderTarget::Image(t) => t.scale_factor != scale || t.handle != lightmap.emissive,
        _ => true,
    };
    if stale {
        commands.entity(entity).insert(target);
    }
}

/// The tiles the camera shows, plus [`MARGIN`].
fn camera_region(
    cameras: &Query<(&Transform, &Projection), (With<WorldCamera>, Without<Lightmap>)>,
    z: u16,
) -> Option<Region> {
    let (transform, projection) = cameras.iter().next()?;
    let Projection::Orthographic(ortho) = projection else {
        return None;
    };
    let tile = TILE_SIZE as f32;
    let center = transform.translation.xy();
    let min = ((center + ortho.area.min) / tile).floor().as_ivec2() - MARGIN;
    let max = ((center + ortho.area.max) / tile).ceil().as_ivec2() + MARGIN;
    let size = (max - min).max(IVec2::ONE).as_uvec2();
    Some(Region { min, size, z })
}

/// Rebuilds the lightmap textures when something in the region changed or the region moved.
fn update_lightmap(
    settings: Res<LightingSettings>,
    active_z: Res<ActiveZ>,
    order: Res<DrawOrder>,
    mut world: ResMut<LightWorld>,
    mut stats: ResMut<LightingStats>,
    cameras: Query<(&Transform, &Projection), (With<WorldCamera>, Without<Lightmap>)>,
    mut lightmaps: Query<(
        &mut Lightmap,
        &MeshMaterial2d<LightmapMaterial>,
        &mut Transform,
        &mut Visibility,
    )>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<LightmapMaterial>>,
) {
    let start = Instant::now();
    let Ok((mut lightmap, material, mut transform, mut visibility)) = lightmaps.single_mut() else {
        return;
    };
    visibility.set_if_neq(if settings.enabled {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if !settings.enabled {
        return;
    }
    let Some(region) = camera_region(&cameras, active_z.0) else {
        return;
    };
    if order.is_changed() || transform.translation.z == 0.0 {
        transform.translation.z = order.plane_z(LIGHTING_PLANE);
    }
    if !world.changed && lightmap.region == Some(region) {
        stats.frame_time += start.elapsed();
        return;
    }
    world.changed = false;
    stats.rebuilds += 1;

    let (corner_bytes, tile_bytes, in_region) = bake(&world, region);
    stats.sources_in_region = in_region;
    write_image(
        &mut images,
        &lightmap.corners,
        region.size + 1,
        corner_bytes,
        true,
    );
    write_image(&mut images, &lightmap.tiles, region.size, tile_bytes, false);
    // Touching the material rebinds the (possibly reallocated) textures.
    if let Some(mut m) = materials.get_mut(&material.0) {
        m.region = Vec4::new(
            region.min.x as f32,
            region.min.y as f32,
            region.size.x as f32,
            region.size.y as f32,
        );
    }
    let tile = TILE_SIZE as f32;
    let size = region.size.as_vec2() * tile;
    transform.translation.x = region.min.x as f32 * tile + size.x / 2.0;
    transform.translation.y = region.min.y as f32 * tile + size.y / 2.0;
    transform.scale = size.extend(1.0);
    lightmap.region = Some(region);
    stats.frame_time += start.elapsed();
}

/// Lightmap texels for `region`: corners (`size + 1`, RGBA8) and tiles (`size`, RGBA8: ambient
/// premultiplied by its alpha, a = 255 where sources light the tile), and how many sources
/// reach the region.
fn bake(world: &LightWorld, region: Region) -> (Vec<u8>, Vec<u8>, usize) {
    let (w, h) = (region.size.x as i32, region.size.y as i32);
    // Corner (i, j) of the texture is corner (min - 1 + i, min - 1 + j) in tg terms.
    let cw = (w + 1) as usize;
    let mut lum = vec![[0.0f32; 3]; cw * (h + 1) as usize];
    let mut in_region = 0;
    for source in world.sources.values() {
        let (Some(corners), true) = (&source.corners, source.pos.z == region.z) else {
            continue;
        };
        let reach = math::reach(&source.light) + 1;
        let (sx, sy) = (source.pos.x, source.pos.y);
        if sx + reach < region.min.x - 1
            || sy + reach < region.min.y - 1
            || sx - reach > region.min.x + w
            || sy - reach > region.min.y + h
        {
            continue;
        }
        in_region += 1;
        for &(dx, dy, add) in corners {
            let i = sx + dx - (region.min.x - 1);
            let j = sy + dy - (region.min.y - 1);
            if i < 0 || j < 0 || i > w || j > h {
                continue;
            }
            let l = &mut lum[j as usize * cw + i as usize];
            for c in 0..3 {
                l[c] += add[c];
            }
        }
    }
    let unit = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let corner_bytes: Vec<u8> = lum
        .iter()
        .flat_map(|&l| {
            let [r, g, b] = math::corner_color(l);
            [unit(r), unit(g), unit(b), 255]
        })
        .collect();
    let mut tile_bytes = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let pos = TilePos::new(region.min.x + x, region.min.y + y, region.z);
            let tile = world
                .tiles
                .get(&pos)
                .copied()
                .unwrap_or(TileLighting::Area {
                    dynamic: true,
                    ambient: [0; 4],
                });
            let [r, g, b, a] = tile.ambient();
            let premul = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
            tile_bytes.extend([
                premul(r),
                premul(g),
                premul(b),
                if tile.dynamic() { 255 } else { 0 },
            ]);
        }
    }
    (corner_bytes, tile_bytes, in_region)
}

/// Every 10 s, like the server's replication stats.
fn log_stats(mut stats: ResMut<LightingStats>, time: Res<Time<Real>>, mut last: Local<f64>) {
    stats.frame_time_max = stats.frame_time_max.max(stats.frame_time);
    let now = time.elapsed_secs_f64();
    if now - *last < 10.0 {
        return;
    }
    *last = now;
    info!(
        "lighting: {} sources ({} in region), {} source updates, {} rebuilds, \
         frame max {:?}",
        stats.sources,
        stats.sources_in_region,
        stats.source_updates,
        stats.rebuilds,
        stats.frame_time_max
    );
    stats.frame_time_max = Duration::ZERO;
}

fn write_image(
    images: &mut Assets<Image>,
    handle: &Handle<Image>,
    size: UVec2,
    data: Vec<u8>,
    linear: bool,
) {
    let Some(mut image) = images.get_mut(handle) else {
        return;
    };
    if image.size() != size {
        *image = lightmap_image(size, linear);
    }
    image.data = Some(data);
}

#[cfg(test)]
mod tests;
