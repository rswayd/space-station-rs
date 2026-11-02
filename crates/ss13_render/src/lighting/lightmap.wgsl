// tg lighting plane: multiplies the world drawn below it (blend state set in `LightmapMaterial`).
//
// `corners`: one texel per tile corner, texel (i, j) = the corner at region tile offset (i, j)
// (south-west corner of the region's tile (i, j)); sampled bilinearly, like tg's lighting object
// blends its four corner colours across the tile. `tiles`: one texel per tile, rgb = ambient
// (area base lighting / starlight), a = 1 when sources light the tile.
// Values are sRGB-space factors, as BYOND multiplies them.

struct Region {
    // Tile coordinates of the region's south-west tile, and its size in tiles.
    origin: vec2<f32>,
    size: vec2<f32>,
}

@group(2) @binding(0) var<uniform> region: Region;
@group(2) @binding(1) var corners: texture_2d<f32>;
@group(2) @binding(2) var corners_sampler: sampler;
@group(2) @binding(3) var tiles: texture_2d<f32>;
// Emissive mask in target pixels (alpha = coverage), added to the light like tg's emissive plate.
@group(2) @binding(4) var emissive: texture_2d<f32>;
// One texel per tile: how much of it is in view (r). Sampled linearly between tile centres, so
// the edge of the view is soft; what is out of view is black, as BYOND draws turfs out of view().
@group(2) @binding(5) var view: texture_2d<f32>;

const TILE_SIZE: f32 = 32.0;

fn light_at(world: vec2<f32>, frag: vec2<f32>) -> vec3<f32> {
    let p = world / TILE_SIZE - region.origin;
    let uv = (p + 0.5) / (region.size + 1.0);
    let corner = textureSampleLevel(corners, corners_sampler, uv, 0.0).rgb;
    let tile_xy = clamp(vec2<i32>(floor(p)), vec2<i32>(0), vec2<i32>(region.size) - 1);
    let tile = textureLoad(tiles, tile_xy, 0);
    let size = vec2<i32>(textureDimensions(emissive));
    let glow = textureLoad(emissive, clamp(vec2<i32>(frag), vec2<i32>(0), size - 1), 0).a;
    let seen = textureSampleLevel(view, corners_sampler, p / region.size, 0.0).r;
    return clamp(corner * tile.a + tile.rgb + glow, vec3<f32>(0.0), vec3<f32>(1.0)) * seen;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

// Linear compositing (Bevy's default): the target stores linear values.
@fragment
fn fragment(
    @builtin(position) frag: vec4<f32>,
    @location(0) world_position: vec4<f32>,
) -> @location(0) vec4<f32> {
    return vec4<f32>(srgb_to_linear(light_at(world_position.xy, frag.xy)), 1.0);
}

// sRGB compositing: blending happens on sRGB values, as in BYOND.
@fragment
fn fragment_srgb(
    @builtin(position) frag: vec4<f32>,
    @location(0) world_position: vec4<f32>,
) -> @location(0) vec4<f32> {
    return vec4<f32>(light_at(world_position.xy, frag.xy), 1.0);
}
