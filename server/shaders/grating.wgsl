// Grating stimulus fragment shader.
//
// The vertex shader emits NDC positions for a simple axis-aligned quad that
// covers the grating patch.  All pattern logic lives in the fragment shader,
// which reconstructs pixel-space coordinates from gl_FragCoord and the push
// constants, so no UV attributes are needed on the vertices.
//
// Push constant layout (96 bytes, std430):
//   screen_half     vec2<f32>   half screen dimensions in pixels (for coord conversion)
//   center_px       vec2<f32>   grating centre in pixel-space (Y-up)
//   half_size       vec2<f32>   patch half-extents [hw, hh] in pixels
//   sf              f32         spatial frequency in cycles/pixel
//   phase           f32         total phase (static + accumulated drift) in [0,1]
//   ori_rad         f32         stripe orientation in radians (CCW from X axis)
//   contrast        f32         [0, 1]
//   global_opacity  f32         global alpha multiplier [0, 1]
//   (4 bytes padding — vec4 requires 16-byte alignment)
//   fore_color      vec4<f32>   peak colour rgba
//   back_color      vec4<f32>   trough colour rgba
//   waveform        u32         0=sin  1=sqr  2=saw  3=tri
//   mask_type       u32         0=none  1=circle  2=gauss  3=hann  4=raisedCos
//   mask_param      f32         mask-specific param (0=default): SD for gauss; fringe for raisedCos
//   _pad            u32         alignment padding

struct PushConstants {
    screen_half    : vec2<f32>,
    center_px      : vec2<f32>,
    half_size      : vec2<f32>,
    sf             : f32,
    phase          : f32,
    ori_rad        : f32,
    contrast       : f32,
    global_opacity : f32,
    // 4 bytes implicit padding here (vec4 alignment)
    fore_color     : vec4<f32>,
    back_color     : vec4<f32>,
    waveform       : u32,
    mask_type      : u32,
    mask_param     : f32,
    _pad           : u32,
}

var<push_constant> p: PushConstants;

// ── Vertex stage ──────────────────────────────────────────────────────────────

struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) uv       : vec2<f32>,
    @location(3) color    : vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos : vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    // in.position.xy is a unit-quad corner in [-1, 1] local space.
    // Transform to pixel-space then to NDC using push constants.
    let pixel_pos = p.center_px + in.position.xy * p.half_size;
    let ndc = pixel_pos / p.screen_half;
    // Clip space is Y-up (top of screen = +1), matching the text path; do NOT
    // negate Y here or the grating renders vertically flipped.
    var out: VertexOutput;
    out.clip_pos = vec4<f32>(ndc.x, ndc.y, 0.0, 1.0);
    return out;
}

// ── Fragment stage ────────────────────────────────────────────────────────────

const TAU: f32 = 6.283185307179586;

// Carrier waveforms — all return a value in [-1, 1] for argument t in cycles.
fn waveform_sin(t: f32) -> f32 { return sin(t * TAU); }
fn waveform_sqr(t: f32) -> f32 { return sign(sin(t * TAU)); }
fn waveform_saw(t: f32) -> f32 { return 2.0 * fract(t + 0.5) - 1.0; }
fn waveform_tri(t: f32) -> f32 { return 1.0 - 4.0 * abs(fract(t + 0.75) - 0.5); }

fn eval_waveform(t: f32, waveform: u32) -> f32 {
    switch waveform {
        case 1u:       { return waveform_sqr(t); }
        case 2u:       { return waveform_saw(t); }
        case 3u:       { return waveform_tri(t); }
        default:       { return waveform_sin(t); }
    }
}

// Aperture masks — return alpha in [0, 1].
fn mask_circle(d: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let r = min(half_size.x, half_size.y);
    return select(0.0, 1.0, length(d) <= r);
}

// Gaussian envelope. mask_param = SD in normalized units (patch radius = 1); default 1/3.
// At the default SD the value at the patch edge is exp(-4.5) ≈ 0.011.
fn mask_gauss(d: vec2<f32>, half_size: vec2<f32>, sd: f32) -> f32 {
    let s     = select(0.33333, sd, sd > 0.0);
    let sigma = min(half_size.x, half_size.y) * s;
    return exp(-dot(d, d) / (2.0 * sigma * sigma));
}

// Cosine bell (Hanning window): exactly 0 at the circular border, 1 at centre.
fn mask_hann(d: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let r = min(half_size.x, half_size.y);
    let dist = length(d);
    if dist >= r { return 0.0; }
    return 0.5 * (1.0 + cos(3.14159265358979 * dist / r));
}

// Tukey window (PsychoPy raisedCos): flat at 1 in the interior, raised-cosine taper
// in the outer fringe.  mask_param = fringe proportion [0, 1]; default 0.2.
fn mask_raised_cos(d: vec2<f32>, half_size: vec2<f32>, fringe_prop: f32) -> f32 {
    let fp    = select(0.2, fringe_prop, fringe_prop > 0.0);
    let r     = min(half_size.x, half_size.y);
    let dist  = length(d);
    if dist >= r { return 0.0; }
    let fringe = fp * r;
    let inner  = r - fringe;
    if dist <= inner { return 1.0; }
    return 0.5 * (1.0 + cos(3.14159265358979 * (dist - inner) / fringe));
}

@fragment
fn fs_main(@builtin(position) frag_pos: vec4<f32>) -> @location(0) vec4<f32> {
    // Convert viewport coordinates (origin top-left, Y-down) to pixel-space
    // (origin screen-centre, Y-up) to match the stimulus coordinate system.
    let px = vec2<f32>(
        frag_pos.x - p.screen_half.x,
        p.screen_half.y - frag_pos.y,
    );

    // Offset relative to grating centre.
    let d = px - p.center_px;

    // Project onto the grating axis (perpendicular to stripes).
    let cos_a = cos(p.ori_rad);
    let sin_a = sin(p.ori_rad);
    let u = cos_a * d.x + sin_a * d.y;

    // Evaluate carrier: t in cycles, phase in [0, 1] (wraps naturally via sin/fract).
    let t = u * p.sf + p.phase;
    let carrier = eval_waveform(t, p.waveform);  // [-1, 1]

    // Map carrier through contrast: blend parameter in [0, 1] (clamped for sqr/saw edges).
    let blend = clamp(0.5 + 0.5 * carrier * p.contrast, 0.0, 1.0);

    // Interpolate between trough and peak colour.
    let rgb = mix(p.back_color.rgb, p.fore_color.rgb, blend);

    // Aperture mask.
    var alpha: f32;
    switch p.mask_type {
        case 1u:  { alpha = mask_circle(d, p.half_size); }
        case 2u:  { alpha = mask_gauss(d, p.half_size, p.mask_param); }
        case 3u:  { alpha = mask_hann(d, p.half_size); }
        case 4u:  { alpha = mask_raised_cos(d, p.half_size, p.mask_param); }
        default:  { alpha = 1.0; }
    }
    let alpha_mix = mix(p.back_color.a, p.fore_color.a, blend);
    alpha *= alpha_mix * p.global_opacity;

    return vec4<f32>(rgb, alpha);
}
