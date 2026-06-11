/// Serializable photodiode configuration (live values only).
#[derive(Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct PhotoDiodeConfig {
    pub enabled:  bool,
    pub lit:      bool,
    pub flicker:  bool,
    pub position: u32, // 0 = bottom-left, 1 = bottom-right
}

/// Full photodiode state: serializable config + deferred copies.
///
/// Always rendered last (on top of all stimuli) when `enabled`.
/// Deref/DerefMut give transparent access to the config fields.
#[derive(Clone)]
#[derive(Default)]
pub struct PhotoDiodeState {
    pub config:        PhotoDiodeConfig,
    pub enabled_copy:  bool,
    pub lit_copy:      bool,
    pub flicker_copy:  bool,
    pub position_copy: u32,
}


impl std::ops::Deref for PhotoDiodeState {
    type Target = PhotoDiodeConfig;
    fn deref(&self) -> &PhotoDiodeConfig { &self.config }
}

impl std::ops::DerefMut for PhotoDiodeState {
    fn deref_mut(&mut self) -> &mut PhotoDiodeConfig { &mut self.config }
}

/// Serde serializes only the live config; copy fields are set to live on deserialize.
impl serde::Serialize for PhotoDiodeState {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.config.serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for PhotoDiodeState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let config = PhotoDiodeConfig::deserialize(d)?;
        Ok(Self {
            enabled_copy:  config.enabled,
            lit_copy:      config.lit,
            flicker_copy:  config.flicker,
            position_copy: config.position,
            config,
        })
    }
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
