// tg space parallax: three tiling star layers added together (`BLEND_ADD`, set in
// `ParallaxMaterial`). A layer sits `speed` pixels further per tile the camera moves, so screen
// point `s` (world pixels from the camera centre) samples layer texel `s + camera / 32 * speed`.

struct Params {
    camera: vec4<f32>,
    rects: array<vec4<f32>, 3>,
    speeds: array<vec4<f32>, 3>,
}

@group(2) @binding(0) var<uniform> params: Params;
@group(2) @binding(1) var layer0: texture_2d<f32>;
@group(2) @binding(2) var layer1: texture_2d<f32>;
@group(2) @binding(3) var layer2: texture_2d<f32>;

const TILE_SIZE: f32 = 32.0;

fn texel(i: u32, world: vec2<f32>) -> vec2<i32> {
    let rect = params.rects[i];
    let q = world - params.camera.xy + params.camera.xy / TILE_SIZE * params.speeds[i].x;
    // Texture rows run top-down, world y up.
    let uv = fract(vec2<f32>(q.x, -q.y) / rect.zw);
    // `fract` of a tiny negative value rounds to exactly 1.0 in f32: unclamped, that samples the
    // texel past the layer, a neighbour in the atlas, as a 1 px line at the layer's wrap seam.
    let inside = min(floor(uv * rect.zw), rect.zw - vec2<f32>(1.0));
    return vec2<i32>(rect.xy + inside);
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3<f32>(0.0031308));
}

// Atlas pages are sRGB textures: loads return linear values.
fn stars(world: vec2<f32>, srgb: bool) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    if params.rects[0].z > 0.0 {
        let c = textureLoad(layer0, texel(0u, world), 0).rgb;
        sum += select(c, linear_to_srgb(c), srgb);
    }
    if params.rects[1].z > 0.0 {
        let c = textureLoad(layer1, texel(1u, world), 0).rgb;
        sum += select(c, linear_to_srgb(c), srgb);
    }
    if params.rects[2].z > 0.0 {
        let c = textureLoad(layer2, texel(2u, world), 0).rgb;
        sum += select(c, linear_to_srgb(c), srgb);
    }
    return sum;
}

@fragment
fn fragment(@location(0) world_position: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(stars(world_position.xy, false), 1.0);
}

@fragment
fn fragment_srgb(@location(0) world_position: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(stars(world_position.xy, true), 1.0);
}
