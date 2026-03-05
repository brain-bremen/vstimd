// ── Deferred<T> ──────────────────────────────────────────────────────────────

/// Holds a live value and a staging copy for deferred mode.
/// The copy is written during deferred mode; `flip()` promotes it to live.
#[derive(Clone, Copy, Default)]
pub struct Deferred<T: Copy + Default> {
    pub live: T,
    pub copy: T,
}

impl<T: Copy + Default> Deferred<T> {
    pub fn new(value: T) -> Self {
        Self {
            live: value,
            copy: value,
        }
    }

    /// Write to the copy slot (deferred=true) or live slot (deferred=false).
    pub fn set(&mut self, deferred: bool, value: T) {
        if deferred {
            self.copy = value;
        } else {
            self.live = value;
        }
    }

    pub fn get(&self) -> &T {
        &self.live
    }

    /// Snapshot live → copy. Call at start of deferred mode.
    pub fn make_copy(&mut self) {
        self.copy = self.live;
    }

    /// Promote copy → live. Call at frame boundary after deferred mode ends.
    pub fn flip(&mut self) {
        self.live = self.copy;
    }
}

// ── StimulusFlags ─────────────────────────────────────────────────────────────

/// Lifecycle and visibility flags. Identical across all stimulus types.
#[derive(Clone, Copy, Default)]
pub struct StimulusFlags {
    pub enabled: bool,
    pub enabled_copy: bool,
    pub suppressed: bool, // set by Flicker animation
    pub protected: bool,  // survives RemoveAll
    pub anim_handle: Option<u32>,
}

impl StimulusFlags {
    pub fn make_copy(&mut self) {
        self.enabled_copy = self.enabled;
    }
    pub fn get_copy(&mut self) {
        self.enabled = self.enabled_copy;
    }

    pub fn is_visible(&self) -> bool {
        self.enabled && !self.suppressed
    }
}

// ── Transform2D ───────────────────────────────────────────────────────────────

/// 2-D placement. Used by every positional stimulus.
/// Position is in stimulus-space pixels with origin at screen centre, Y-up.
/// Rotation angle is counter-clockwise degrees.
#[derive(Clone, Copy)]
pub struct Transform2D {
    pub pos: [f32; 2],
    pub angle: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            pos: [0.0, 0.0],
            angle: 0.0,
        }
    }
}

impl Transform2D {
    /// Convert to a `kurbo::Affine` for tessellation (applies rotation then translation).
    pub fn to_affine(&self) -> kurbo::Affine {
        kurbo::Affine::translate((self.pos[0] as f64, self.pos[1] as f64))
            * kurbo::Affine::rotate(self.angle.to_radians() as f64)
    }
}

// ── Appearance ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DrawMode {
    #[default]
    Fill,
    Stroke,
    FillAndStroke,
}

/// Fill / outline / stroke appearance for coloured shape stimuli.
#[derive(Clone, Copy)]
pub struct ShapeAppearance {
    pub fill_color: [f32; 4],    // RGBA
    pub outline_color: [f32; 4], // RGBA
    pub stroke_width: f32,
    pub draw_mode: DrawMode,
}

impl Default for ShapeAppearance {
    fn default() -> Self {
        Self {
            fill_color: [1.0, 1.0, 1.0, 1.0],
            outline_color: [0.0, 0.0, 0.0, 1.0],
            stroke_width: 2.0,
            draw_mode: DrawMode::Fill,
        }
    }
}

// ── Concrete stimulus structs ─────────────────────────────────────────────────

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

pub struct PixelStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub color: Deferred<[f32; 4]>,
}

// ── stim_field! macro ─────────────────────────────────────────────────────────

/// Apply a field expression to the inner struct of any `Stimulus` variant.
///
/// Usage:
/// ```rust,ignore
/// stim_field!(stimulus, |s| &s.flags)
/// stim_field!(stimulus, |s| &mut s.flags)
/// ```
macro_rules! stim_field {
    ($stim:expr, |$s:ident| $expr:expr) => {
        match $stim {
            Stimulus::Rect($s) => $expr,
            Stimulus::Ellipse($s) => $expr,
            Stimulus::Petal($s) => $expr,
            Stimulus::Wedge($s) => $expr,
            Stimulus::Disc($s) => $expr,
            Stimulus::Bitmap($s) => $expr,
            Stimulus::BitmapSeq($s) => $expr,
            Stimulus::WgslShader($s) => $expr,
            Stimulus::Particle($s) => $expr,
            Stimulus::Pixel($s) => $expr,
        }
    };
}

// ── Stimulus enum ─────────────────────────────────────────────────────────────

pub enum Stimulus {
    Rect(RectStimulus),
    Ellipse(EllipseStimulus),
    Petal(PetalStimulus),
    Wedge(WedgeStimulus),
    Disc(DiscStimulus),
    Bitmap(BitmapStimulus),
    BitmapSeq(BitmapSeqStimulus),
    WgslShader(WgslShaderStimulus),
    Particle(ParticleStimulus),
    Pixel(PixelStimulus),
}

impl Stimulus {
    // ── Common field accessors ────────────────────────────────────────────────

    pub fn flags(&self) -> &StimulusFlags {
        stim_field!(self, |s| &s.flags)
    }

    pub fn flags_mut(&mut self) -> &mut StimulusFlags {
        stim_field!(self, |s| &mut s.flags)
    }

    /// All current variants have a transform; returns `None` only as a
    /// forward-compatibility escape hatch.
    pub fn transform(&self) -> Option<&Deferred<Transform2D>> {
        Some(stim_field!(self, |s| &s.transform))
    }

    pub fn transform_mut(&mut self) -> Option<&mut Deferred<Transform2D>> {
        Some(stim_field!(self, |s| &mut s.transform))
    }

    /// Returns `None` for stimulus types that have no fill/stroke appearance
    /// (bitmaps, shaders, particles, pixels).
    pub fn appearance_mut(&mut self) -> Option<&mut Deferred<ShapeAppearance>> {
        match self {
            Stimulus::Rect(s) => Some(&mut s.appearance),
            Stimulus::Ellipse(s) => Some(&mut s.appearance),
            Stimulus::Petal(s) => Some(&mut s.appearance),
            Stimulus::Wedge(s) => Some(&mut s.appearance),
            Stimulus::Disc(s) => Some(&mut s.appearance),
            Stimulus::Bitmap(_)
            | Stimulus::BitmapSeq(_)
            | Stimulus::WgslShader(_)
            | Stimulus::Particle(_)
            | Stimulus::Pixel(_) => None,
        }
    }

    // ── Deferred mode ─────────────────────────────────────────────────────────

    /// Snapshot all live state into copy fields.
    /// Call at the start of deferred mode.
    pub fn make_copy(&mut self) {
        self.flags_mut().make_copy();
        if let Some(t) = self.transform_mut() {
            t.make_copy();
        }
        if let Some(a) = self.appearance_mut() {
            a.make_copy();
        }
        match self {
            Stimulus::Rect(s) => {
                s.size.make_copy();
            }
            Stimulus::Ellipse(s) => {
                s.radii.make_copy();
            }
            Stimulus::Petal(s) => {
                s.params.make_copy();
            }
            Stimulus::Wedge(s) => {
                s.half_angle.make_copy();
            }
            Stimulus::Disc(s) => {
                s.radius.make_copy();
            }
            Stimulus::Bitmap(s) => {
                s.alpha.make_copy();
                s.phi_inc.make_copy();
            }
            Stimulus::BitmapSeq(s) => {
                s.alpha.make_copy();
            }
            Stimulus::WgslShader(s) => {
                s.params.make_copy();
            }
            Stimulus::Particle(s) => {
                s.params.make_copy();
                s.shift.make_copy();
            }
            Stimulus::Pixel(s) => {
                s.color.make_copy();
            }
        }
    }

    /// Promote all copy fields to live.
    /// Call at the frame boundary when `pending_flip` is set.
    pub fn flip(&mut self) {
        self.flags_mut().get_copy();
        if let Some(t) = self.transform_mut() {
            t.flip();
        }
        if let Some(a) = self.appearance_mut() {
            a.flip();
        }
        match self {
            Stimulus::Rect(s) => {
                s.size.flip();
            }
            Stimulus::Ellipse(s) => {
                s.radii.flip();
            }
            Stimulus::Petal(s) => {
                s.params.flip();
                s.rebuild = true;
            }
            Stimulus::Wedge(s) => {
                s.half_angle.flip();
                s.rebuild = true;
            }
            Stimulus::Disc(s) => {
                s.radius.flip();
            }
            Stimulus::Bitmap(s) => {
                s.alpha.flip();
                s.phi_inc.flip();
            }
            Stimulus::BitmapSeq(s) => {
                s.alpha.flip();
            }
            Stimulus::WgslShader(s) => {
                s.params.flip();
            }
            Stimulus::Particle(s) => {
                s.params.flip();
                s.shift.flip();
            }
            Stimulus::Pixel(s) => {
                s.color.flip();
            }
        }
    }

    // ── Spatial commands ──────────────────────────────────────────────────────

    pub fn move_to(&mut self, deferred: bool, x: f32, y: f32) {
        if let Some(t) = self.transform_mut() {
            let angle = t.live.angle;
            t.set(deferred, Transform2D { pos: [x, y], angle });
        }
    }

    pub fn set_angle(&mut self, deferred: bool, degrees: f32) {
        if let Some(t) = self.transform_mut() {
            let pos = t.live.pos;
            t.set(
                deferred,
                Transform2D {
                    pos,
                    angle: degrees,
                },
            );
        }
    }

    pub fn get_pos(&self) -> [f32; 2] {
        self.transform().map(|t| t.live.pos).unwrap_or([0.0, 0.0])
    }

    // ── Visibility ────────────────────────────────────────────────────────────

    pub fn is_visible(&self) -> bool {
        self.flags().is_visible()
    }

    // ── Geometry dirty flag ───────────────────────────────────────────────────

    /// True when the shape geometry needs re-tessellating.
    /// For types with `rebuild`, cleared by the renderer after re-upload.
    pub fn needs_rebuild(&self) -> bool {
        match self {
            Stimulus::Petal(s) => s.rebuild,
            Stimulus::Wedge(s) => s.rebuild,
            _ => false,
        }
    }

    pub fn clear_rebuild(&mut self) {
        match self {
            Stimulus::Petal(s) => s.rebuild = false,
            Stimulus::Wedge(s) => s.rebuild = false,
            _ => {}
        }
    }

    // ── Animation parameter target ────────────────────────────────────────────

    /// Set a type-specific animatable parameter by index.
    /// Returns `false` if the index is unsupported for this stimulus type.
    pub fn set_anim_param(&mut self, index: u8, value: f32) -> bool {
        match self {
            Stimulus::WgslShader(s) => {
                let i = index as usize;
                if i < 8 {
                    s.params.live.params[i] = value;
                    true
                } else {
                    false
                }
            }
            Stimulus::Wedge(s) if index == 1 => {
                let pos = s.transform.live.pos;
                s.transform.set(false, Transform2D { pos, angle: value });
                true
            }
            Stimulus::Bitmap(s) if index == 1 => {
                s.alpha.live = value;
                true
            }
            _ => false,
        }
    }
}
