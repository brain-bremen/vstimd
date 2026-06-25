// egui overlay shaders for Vulkan
//
// Vertex format matches egui's epaint::Vertex:
//   pos: [f32; 2]      — screen position in pixels
//   uv: [f32; 2]       — texture coordinates (0..1)
//   color: [u8; 4]     — sRGBA color, premultiplied alpha

// Screen size passed as push constants (set dynamically each frame)
struct PushConstants {
    screen_size: vec2<f32>,
}
var<push_constant> push: PushConstants;

struct VertexInput {
    @location(0) pos: vec2<f32>,      // screen position in pixels
    @location(1) uv: vec2<f32>,       // texture coordinates
    @location(2) color: vec4<f32>,    // sRGBA color (0..1), premultiplied
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,  // clip space
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Transform from egui's screen pixels to Vulkan NDC
    // egui: (0, 0) = top-left, x increases right, y increases down
    // Vulkan NDC: (-1, -1) = bottom-left, (+1, +1) = top-right
    let normalized = in.pos / push.screen_size;
    let ndc = vec2<f32>(
        normalized.x * 2.0 - 1.0,  // 0..1 → -1..+1
        1.0 - normalized.y * 2.0   // 0..1 → +1..-1 (flip Y for Vulkan)
    );

    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;

    return out;
}

@group(0) @binding(0)
var tex_sampler: sampler;

@group(0) @binding(1)
var tex: texture_2d<f32>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample texture (font atlas or user image)
    let tex_color = textureSample(tex, tex_sampler, in.uv);

    // egui uses sRGB colors, but our framebuffer is linear
    // Convert sRGB vertex color to linear for correct blending
    let linear_color = srgb_to_linear(in.color.rgb);

    // Multiply texture alpha by vertex alpha, keep premultiplied alpha
    let result = vec4<f32>(
        linear_color * tex_color.rgb,
        in.color.a * tex_color.a
    );

    return result;
}

// sRGB to linear conversion
fn srgb_to_linear(srgb: vec3<f32>) -> vec3<f32> {
    let cutoff = vec3<f32>(0.04045);
    let higher = pow((srgb + 0.055) / 1.055, vec3<f32>(2.4));
    let lower = srgb / 12.92;
    return select(higher, lower, srgb <= cutoff);
}
