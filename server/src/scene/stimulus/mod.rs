mod common;
mod types;

pub use common::{DrawMode, ShapeAppearance, StimulusFlags, Transform2D};
pub use types::{
    BitmapSeqStimulus, BitmapStimulus, DiscStimulus, EllipseStimulus, ParticleParams,
    ParticleStimulus, PetalParams, PetalStimulus, PixelStimulus, RectStimulus, ShaderParams,
    WedgeStimulus, WgslShaderStimulus,
};

use super::deferred::Deferred;

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
        self.flags_mut().mark_dirty();
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
        if !deferred {
            self.flags_mut().mark_dirty();
        }
    }

    pub fn set_angle(&mut self, deferred: bool, degrees: f32) {
        if let Some(t) = self.transform_mut() {
            let pos = t.live.pos;
            t.set(deferred, Transform2D { pos, angle: degrees });
        }
        if !deferred {
            self.flags_mut().mark_dirty();
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

    // ── Display name ──────────────────────────────────────────────────────────

    pub fn type_name(&self) -> &'static str {
        match self {
            Stimulus::Rect(_)      => "Rect",
            Stimulus::Ellipse(_)   => "Ellipse",
            Stimulus::Petal(_)     => "Petal",
            Stimulus::Wedge(_)     => "Wedge",
            Stimulus::Disc(_)      => "Disc",
            Stimulus::Bitmap(_)    => "Bitmap",
            Stimulus::BitmapSeq(_) => "BitmapSeq",
            Stimulus::WgslShader(_)=> "WgslShader",
            Stimulus::Particle(_)  => "Particle",
            Stimulus::Pixel(_)     => "Pixel",
        }
    }

    // ── Animation parameter target ────────────────────────────────────────────

    /// Set a type-specific animatable parameter by index.
    /// Returns `false` if the index is unsupported for this stimulus type.
    pub fn set_anim_param(&mut self, index: u8, value: f32) -> bool {
        let changed = match self {
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
        };
        if changed {
            self.flags_mut().mark_dirty();
        }
        changed
    }
}
