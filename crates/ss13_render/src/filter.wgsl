// The world filter (`ScreenFilter`): BYOND's colour matrix on sRGB values, then a gaussian blur
// like BYOND's `gauss` filter (`size` is the standard deviation, here in target pixels).

struct FullscreenVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct ScreenFilter {
    red: vec4<f32>,
    green: vec4<f32>,
    blue: vec4<f32>,
    alpha: vec4<f32>,
    constant: vec4<f32>,
    blur: vec4<f32>,
};

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: ScreenFilter;

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3(2.4));
    return select(high, low, c <= vec3(0.04045));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    var color = textureSample(screen, screen_sampler, in.uv);
    let sigma = settings.blur.x;
    if sigma > 0.0 {
        // 9x9 taps over +-2 sigma, spaced sigma / 2 apart; bilinear sampling smooths between.
        let texel = 1.0 / vec2<f32>(textureDimensions(screen));
        let step = sigma * 0.5;
        var sum = vec4(0.0);
        var weights = 0.0;
        for (var y = -4; y <= 4; y++) {
            for (var x = -4; x <= 4; x++) {
                let offset = vec2(f32(x), f32(y)) * step;
                let w = exp(-dot(offset, offset) / (2.0 * sigma * sigma));
                sum += textureSampleLevel(screen, screen_sampler, in.uv + offset * texel, 0.0) * w;
                weights += w;
            }
        }
        color = sum / weights;
    }
    let s = vec4(to_srgb(clamp(color.rgb, vec3(0.0), vec3(1.0))), color.a);
    let o = s.r * settings.red + s.g * settings.green + s.b * settings.blue + s.a * settings.alpha
        + settings.constant;
    return vec4(to_linear(clamp(o.rgb, vec3(0.0), vec3(1.0))), clamp(o.a, 0.0, 1.0));
}
