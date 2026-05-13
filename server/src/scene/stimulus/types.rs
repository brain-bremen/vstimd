// When 3-D stimulus types arrive (see docs/3D_ROADMAP.md), split this file
// along natural boundaries: shapes_2d.rs, bitmap.rs, shader.rs, shapes_3d.rs,
// environments.rs (corridor/maze), mesh.rs, etc. For now a single file suffices.

use super::super::deferred::Deferred;
use super::common::{ShapeAppearance, StimulusFlags, Transform2D};
pub use super::grating::{GratingMask, GratingParams, GratingStimulus, Waveform};

// ── Shape stimuli ─────────────────────────────────────────────────────────────

pub struct RectStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub appearance: Deferred<ShapeAppearance>,
    pub size: Deferred<[f32; 2]>, // [half_width, half_height]
}

pub struct EllipseStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub appearance: Deferred<ShapeAppearance>,
    pub radii: Deferred<[f32; 2]>, // [rx, ry]
}

/// Shape parameters for the petal stimulus (arc + quadratic Bézier outline).
#[derive(Clone, Copy, Default)]
pub struct PetalParams {
    pub r: f32,     // inner arc radius
    pub big_r: f32, // outer arc radius
    pub d: f32,     // tip distance
    pub q: f32,     // split ratio (golden ratio default ≈ 0.618)
}

pub struct PetalStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub appearance: Deferred<ShapeAppearance>,
    pub params: Deferred<PetalParams>,
    pub rebuild: bool, // set when params change; cleared after tessellation
}

pub struct WedgeStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub appearance: Deferred<ShapeAppearance>,
    pub half_angle: Deferred<f32>, // degrees
    pub rebuild: bool,
}

pub struct DiscStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub appearance: Deferred<ShapeAppearance>,
    pub radius: Deferred<f32>,
}

// ── Bitmap stimuli ────────────────────────────────────────────────────────────

pub struct BitmapStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub alpha: Deferred<f32>,
    pub phi_inc: Deferred<f32>, // continuous rotation rate (deg/frame)
    pub phi_accum: f32,         // accumulated rotation (not deferred)
    pub texture_id: u32,        // index into RenderState texture store
    pub size: [f32; 2],         // half-extents, set at load time
}

pub struct BitmapSeqStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub alpha: Deferred<f32>,
    pub texture_ids: Vec<u32>, // one per frame
    pub frame_index: u32,
    pub rate_num: u32, // fps numerator
    pub rate_den: u32, // fps denominator (≈ display rate)
    pub frac_counter: u32,
    pub size: [f32; 2],
}

// ── Shader stimuli ────────────────────────────────────────────────────────────

/// Uniform parameters for a custom WGSL pixel-shader stimulus.
#[derive(Clone, Copy, Default)]
pub struct ShaderParams {
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub params: [f32; 8],
    pub phase: f32,
    pub phase_inc: f32,
}

pub struct WgslShaderStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub params: Deferred<ShaderParams>,
    pub pipeline_id: u32, // index into RenderState pipeline store
}

// ── Particle stimuli ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Default)]
pub struct ParticleParams {
    pub color: [f32; 4],
    pub size: f32,
    pub angle: f32,
    pub velocity: f32,
    pub patch_radius: f32,
    pub gauss_radius: f32,
}

pub struct ParticleStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub params: Deferred<ParticleParams>,
    pub shift: Deferred<f32>,
    pub vbuffer_id: u32, // index into RenderState vertex buffer store
    pub n_particles: u32,
}

// ── Pixel stimulus ────────────────────────────────────────────────────────────

pub struct PixelStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub color: Deferred<[f32; 4]>,
}
