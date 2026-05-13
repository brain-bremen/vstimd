// ── StimulusFlags ─────────────────────────────────────────────────────────────

/// Lifecycle and visibility flags. Identical across all stimulus types.
#[derive(Clone, Copy)]
pub struct StimulusFlags {
    pub enabled: bool,
    pub enabled_copy: bool,
    pub protected: bool, // survives RemoveAll
    /// Set on creation, mutation, or flip. Cleared by the render thread after
    /// tessellation+upload. Prevents redundant vkAllocateMemory every frame.
    pub dirty: bool,
}

impl Default for StimulusFlags {
    fn default() -> Self {
        Self {
            enabled: false,
            enabled_copy: false,
            protected: false,
            dirty: true,
        }
    }
}

impl StimulusFlags {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn make_copy(&mut self) {
        self.enabled_copy = self.enabled;
    }
    pub fn get_copy(&mut self) {
        self.enabled = self.enabled_copy;
    }

    pub fn is_visible(&self) -> bool {
        self.enabled
    }
}

// ── Transform2D ───────────────────────────────────────────────────────────────

/// 2-D placement. Used by every positional stimulus.
/// Position is in stimulus-space pixels with origin at screen centre, Y-up.
/// Rotation angle is counter-clockwise degrees.
#[derive(Clone, Copy)]
pub struct Transform2D {
    pub pos: [f32; 2],
    pub angle: f32, // ccw degrees, 0 = right, 90 = up
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
