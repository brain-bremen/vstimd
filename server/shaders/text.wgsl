// Text glyph atlas shader.
//
// Vertex attributes: position (NDC vec2), uv (atlas UV vec2).
// Descriptor set 0, binding 0: sampler
// Descriptor set 0, binding 1: R8_UNORM glyph atlas texture
// Push constant: rgba color tint (16 bytes)
//
// The fragment samples the single-channel coverage value from the atlas and
// multiplies it by the tint alpha; RGB comes from the tint unchanged.

struct PushConstants {
    color: vec4<f32>,
}
var<push_constant> p: PushConstants;

@group(0) @binding(0) var text_sampler: sampler;
@group(0) @binding(1) var text_texture: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv:       vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_pos = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let coverage = textureSample(text_texture, text_sampler, in.uv).r;
    return vec4<f32>(p.color.rgb, p.color.a * coverage);
}
