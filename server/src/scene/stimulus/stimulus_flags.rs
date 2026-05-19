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
