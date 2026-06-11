/// Serializable part of stimulus flags.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[derive(Default)]
pub struct StimulusFlagsConfig {
    /// User-controlled visibility.
    pub enabled: bool,
    pub protected: bool, // survives RemoveAll
}


/// Full stimulus flag state: serializable config + render-thread runtime fields.
#[derive(Clone)]
pub struct StimulusFlags {
    pub config: StimulusFlagsConfig,
    pub enabled_copy: bool,
    /// Animation-controlled visibility. Written by the render thread each frame.
    /// Defaults to true (no animation hold). Animations set this; user commands do not.
    /// Not part of deferred mode — the render thread owns it exclusively.
    pub anim_enabled: bool,
    /// Set on creation, mutation, or flip. Cleared by the render thread after
    /// tessellation+upload. Prevents redundant vkAllocateMemory every frame.
    pub dirty: bool,
}

impl Default for StimulusFlags {
    fn default() -> Self {
        Self {
            config: StimulusFlagsConfig::default(),
            enabled_copy: false,
            anim_enabled: true,
            dirty: true,
        }
    }
}

impl std::ops::Deref for StimulusFlags {
    type Target = StimulusFlagsConfig;
    fn deref(&self) -> &StimulusFlagsConfig { &self.config }
}

impl std::ops::DerefMut for StimulusFlags {
    fn deref_mut(&mut self) -> &mut StimulusFlagsConfig { &mut self.config }
}

/// Serde serializes only the config (enabled + protected); runtime fields are restored
/// with correct defaults on deserialization.
impl serde::Serialize for StimulusFlags {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.config.serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for StimulusFlags {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let config = StimulusFlagsConfig::deserialize(d)?;
        Ok(Self {
            enabled_copy: config.enabled,
            config,
            anim_enabled: true,
            dirty: true,
        })
    }
}

impl StimulusFlags {
    /// Construct with the given enabled state; all other fields take their defaults.
    pub fn enabled(enabled: bool) -> Self {
        Self {
            config: StimulusFlagsConfig { enabled, protected: false },
            enabled_copy: false,
            anim_enabled: true,
            dirty: true,
        }
    }

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
        self.enabled && self.anim_enabled
    }
}
