/// State for the photodiode sync-flash rectangle drawn in a screen corner.
///
/// Always rendered last (on top of all stimuli) when `enabled`.
#[derive(Clone, Copy, Default)]
pub struct PhotoDiodeState {
    pub enabled:  bool,
    pub lit:      bool,
    pub flicker:  bool,
    pub position: u32,  // 0 = bottom-left, 1 = bottom-right

    // Deferred copies (participate in the same flip mechanism as stimuli)
    pub enabled_copy:  bool,
    pub lit_copy:      bool,
    pub flicker_copy:  bool,
    pub position_copy: u32,
}

impl PhotoDiodeState {
    pub fn make_copy(&mut self) {
        self.enabled_copy  = self.enabled;
        self.lit_copy      = self.lit;
        self.flicker_copy  = self.flicker;
        self.position_copy = self.position;
    }

    pub fn flip(&mut self) {
        self.enabled  = self.enabled_copy;
        self.lit      = self.lit_copy;
        self.flicker  = self.flicker_copy;
        self.position = self.position_copy;
    }

    /// Advance the photodiode state by one frame (handles flicker toggling).
    pub fn advance(&mut self) {
        if self.enabled && self.flicker {
            self.lit = !self.lit;
        }
    }
}
