//! tg's space parallax (`code/_onclick/hud/parallax/parallax.dm`): space turfs are black
//! (`icons/turf/space.dmi:space`) and the stars are three 480 px layers of
//! `icons/effects/parallax.dmi`, added (`BLEND_ADD`) on top of them, each shifted by `speed` pixels
//! per tile the view moves. One quad over the view, drawn right above `PLANE_SPACE`: floors and
//! everything else cover it, glass floors and empty tiles show it.
// shortcut: the per-round random layer (planet, asteroids, gas) and shuttle-flight scrolling are
// not drawn.

use bevy::asset::embedded_asset;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor,
    ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{
    AlphaMode2d, Material2d, Material2dKey, Material2dPipeline, Material2dPlugin,
    Mesh2dPipelineKey, MeshMaterial2d,
};
use ss13_shared::TILE_SIZE;

use crate::camera::WorldCamera;
use crate::order::DrawOrder;
use crate::{IconAtlas, RenderSystems};

/// tg `PLANE_SPACE`: space turfs.
pub const PLANE_SPACE: i16 = -21;
const PARALLAX_DMI: &str = "icons/effects/parallax.dmi";
/// `/atom/movable/screen/parallax_layer/layer_1..3`: state and speed.
const LAYERS: [(&str, f32); 3] = [("layer1", 0.6), ("layer2", 1.0), ("layer3", 1.4)];

/// Draws the parallax. Needs [`RenderPlugin`](crate::RenderPlugin) and a render app.
pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "parallax.wgsl");
        app.add_plugins(Material2dPlugin::<ParallaxMaterial>::default())
            .add_systems(Startup, spawn_parallax)
            .add_systems(
                PostUpdate,
                update_parallax
                    .in_set(RenderSystems)
                    .after(crate::camera::follow_target),
            );
    }
}

#[derive(ShaderType, Clone, Copy, Debug, Default)]
struct ParallaxParams {
    /// Camera centre in world pixels (xy).
    camera: Vec4,
    /// Per layer: atlas rect `(x, y, w, h)` in texels; `w == 0` = not drawn.
    rects: [Vec4; 3],
    /// Per layer: speed (x).
    speeds: [Vec4; 3],
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct ParallaxMaterial {
    #[uniform(0)]
    params: ParallaxParams,
    #[texture(1)]
    layer0: Handle<Image>,
    #[texture(2)]
    layer1: Handle<Image>,
    #[texture(3)]
    layer2: Handle<Image>,
}

impl Material2d for ParallaxMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://ss13_render/parallax.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
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
        fragment.entry_point = Some(
            if key.mesh_key.contains(Mesh2dPipelineKey::SRGB_COMPOSITING) {
                "fragment_srgb"
            } else {
                "fragment"
            }
            .into(),
        );
        // `BLEND_ADD`; the target's alpha stays.
        let add = BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::Zero,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
        };
        for target in fragment.targets.iter_mut().flatten() {
            target.blend = Some(add);
        }
        Ok(())
    }
}

#[derive(Component)]
struct Parallax;

fn spawn_parallax(
    mut commands: Commands,
    atlas: Option<ResMut<IconAtlas>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ParallaxMaterial>>,
) {
    let Some(mut atlas) = atlas else {
        return;
    };
    let mut params = ParallaxParams::default();
    let mut pages: [Handle<Image>; 3] = Default::default();
    for (i, (state, speed)) in LAYERS.into_iter().enumerate() {
        let id = atlas
            .manifest
            .states
            .iter()
            .position(|s| s.dmi == PARALLAX_DMI && s.state == state);
        let Some((info, page)) = id.and_then(|id| atlas.state(ss13_shared::IconStateId(id as u32)))
        else {
            warn!("parallax: {PARALLAX_DMI}:{state} not in the atlas; layer not drawn");
            continue;
        };
        let [x, y, w, h] = info.rects[0];
        params.rects[i] = Vec4::new(x as f32, y as f32, w as f32, h as f32);
        params.speeds[i] = Vec4::new(speed, 0.0, 0.0, 0.0);
        pages[i] = page;
    }
    let [layer0, layer1, layer2] = pages;
    commands.spawn((
        Name::new("parallax"),
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(materials.add(ParallaxMaterial {
            params,
            layer0,
            layer1,
            layer2,
        })),
        Transform::default(),
        Parallax,
    ));
}

/// Covers the camera's view and follows it. Per frame: one transform write and one uniform
/// update while the camera moves.
fn update_parallax(
    order: Res<DrawOrder>,
    cameras: Query<(&Transform, &Projection), (With<WorldCamera>, Without<Parallax>)>,
    mut quads: Query<(&MeshMaterial2d<ParallaxMaterial>, &mut Transform), With<Parallax>>,
    mut materials: ResMut<Assets<ParallaxMaterial>>,
) {
    let (Some((camera, Projection::Orthographic(ortho))), Ok((material, mut transform))) =
        (cameras.iter().next(), quads.single_mut())
    else {
        return;
    };
    let center = camera.translation.xy();
    // One tile of margin against gaps while the camera glides.
    let size = ortho.area.size() + Vec2::splat(2.0 * TILE_SIZE as f32);
    let z = order.plane_z(PLANE_SPACE + 1);
    let wanted = Transform::from_translation((center + ortho.area.center()).extend(z))
        .with_scale(size.extend(1.0));
    if *transform != wanted {
        *transform = wanted;
    }
    let camera_px = center.extend(0.0).extend(0.0);
    if let Some(m) = materials.get(&material.0)
        && m.params.camera != camera_px
        && let Some(mut m) = materials.get_mut(&material.0)
    {
        m.params.camera = camera_px;
    }
}
