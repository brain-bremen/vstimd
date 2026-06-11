pub mod grating;
pub mod text;
mod primitive_shapes;
mod shape_appearance;
mod shape_stimulus;
mod stimulus_flags;
mod transform2d;

pub use grating::{GratingMask, GratingParams, GratingStimulus, Waveform};
pub use text::{Anchor, LanguageStyle, TextRenderParams, TextStimulus};
pub use primitive_shapes::{CircleStimulus, EllipseStimulus, RectStimulus};
pub use shape_stimulus::ShapeStimulus;
pub use shape_appearance::{DrawMode, ShapeAppearance};
pub use stimulus_flags::StimulusFlags;
pub use transform2d::Transform2D;

use super::deferred::Deferred;
use uuid::Uuid;

// ── StimulusEntry ─────────────────────────────────────────────────────────────

/// Metadata + stimulus stored as one unit in `SceneState::stimuli`.
///
/// `id` is stable across sessions (survives serialization round-trips and lets
/// reconnecting clients match server-side stimuli to their in-memory objects).
/// `name` is optional human-readable label for debugging/tooling.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StimulusEntry {
    pub id: Uuid,
    pub name: Option<String>,
    pub stimulus: Stimulus,
}

impl StimulusEntry {
    pub fn new(id: Uuid, name: Option<String>, stimulus: Stimulus) -> Self {
        Self { id, name, stimulus }
    }
}


// ── Stimulus enum ─────────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum Stimulus {
    Shape(ShapeStimulus),
    Grating(GratingStimulus),
    Text(TextStimulus),
}

impl Stimulus {
    // ── Common field accessors ────────────────────────────────────────────────

    pub fn flags(&self) -> &StimulusFlags {
        match self {
            Stimulus::Shape(s)   => s.flags(),
            Stimulus::Grating(s) => &s.flags,
            Stimulus::Text(s)    => &s.flags,
        }
    }

    pub fn flags_mut(&mut self) -> &mut StimulusFlags {
        match self {
            Stimulus::Shape(s)   => s.flags_mut(),
            Stimulus::Grating(s) => &mut s.flags,
            Stimulus::Text(s)    => &mut s.flags,
        }
    }

    pub fn transform(&self) -> &Deferred<Transform2D> {
        match self {
            Stimulus::Shape(s)   => s.transform(),
            Stimulus::Grating(s) => &s.transform,
            Stimulus::Text(s)    => &s.transform,
        }
    }

    pub fn transform_mut(&mut self) -> &mut Deferred<Transform2D> {
        match self {
            Stimulus::Shape(s)   => s.transform_mut(),
            Stimulus::Grating(s) => &mut s.transform,
            Stimulus::Text(s)    => &mut s.transform,
        }
    }

    // ── Config load ───────────────────────────────────────────────────────────

    /// Reset render-thread runtime state after loading from config.
    pub fn reset_phase_accum(&mut self) {
        if let Stimulus::Grating(s) = self {
            s.reset_phase_accum();
        }
    }

    // ── Deferred mode ─────────────────────────────────────────────────────────

    /// Snapshot all live state into copy fields. Call at the start of deferred mode.
    pub fn make_copy(&mut self) {
        match self {
            Stimulus::Shape(s)   => s.make_copy(),
            Stimulus::Grating(s) => s.make_copy(),
            Stimulus::Text(s)    => s.make_copy(),
        }
    }

    /// Promote all copy fields to live. Call at the frame boundary when `pending_flip` is set.
    pub fn flip(&mut self) {
        match self {
            Stimulus::Shape(s)   => s.flip(),
            Stimulus::Grating(s) => s.flip(),
            Stimulus::Text(s)    => s.flip(),
        }
    }

    // ── Spatial commands ──────────────────────────────────────────────────────

    pub fn move_to(&mut self, deferred: bool, x: f32, y: f32) {
        {
            let t = self.transform_mut();
            let angle = if deferred { t.copy.angle } else { t.live.angle };
            t.set(deferred, Transform2D { pos: [x, y], angle });
        }
        if !deferred {
            self.flags_mut().mark_dirty();
        }
    }

    pub fn set_angle(&mut self, deferred: bool, degrees: f32) {
        {
            let t = self.transform_mut();
            let pos = if deferred { t.copy.pos } else { t.live.pos };
            t.set(deferred, Transform2D { pos, angle: degrees });
        }
        if !deferred {
            self.flags_mut().mark_dirty();
        }
    }

    pub fn get_pos(&self) -> [f32; 2] {
        self.transform().live.pos
    }

    // ── Visibility ────────────────────────────────────────────────────────────

    pub fn is_visible(&self) -> bool {
        self.flags().is_visible()
    }

    // ── Display name ──────────────────────────────────────────────────────────

    pub fn type_name(&self) -> &'static str {
        match self {
            Stimulus::Shape(s)   => s.type_name(),
            Stimulus::Grating(_) => "Grating",
            Stimulus::Text(_)    => "Text",
        }
    }
}
