use indexmap::IndexMap;

use super::animation::AnimationEntry;
use super::deferred::Deferred;
use super::photodiode::PhotoDiodeState;
use super::stimulus::StimulusEntry;

pub const CONFIG_VERSION: u32 = 1;

pub enum LoadMode {
    Replace,
    Additive,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SceneConfig {
    pub version:          u32,
    pub background:       Deferred<[f32; 4]>,
    pub default_fill:     [f32; 4],
    pub default_outline:  [f32; 4],
    pub photodiode:       PhotoDiodeState,
    pub stimuli:          IndexMap<u32, StimulusEntry>,
    pub next_stim_handle: u32,
    pub animations:       IndexMap<u32, AnimationEntry>,
    pub next_anim_handle: u32,
}

impl Default for SceneConfig {
    fn default() -> Self {
        Self {
            version:          CONFIG_VERSION,
            background:       Deferred::new([0.0, 0.0, 0.0, 1.0]),
            default_fill:     [1.0, 1.0, 1.0, 1.0],
            default_outline:  [0.0, 0.0, 0.0, 1.0],
            photodiode:       PhotoDiodeState::default(),
            stimuli:          IndexMap::new(),
            next_stim_handle: 1,
            animations:       IndexMap::new(),
            next_anim_handle: 1,
        }
    }
}

impl SceneConfig {
    pub fn save_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let s = std::fs::read_to_string(path)?;
        let cfg: Self = serde_json::from_str(&s)?;
        anyhow::ensure!(
            cfg.version == CONFIG_VERSION,
            "Unsupported config version {} (expected {})",
            cfg.version,
            CONFIG_VERSION
        );
        Ok(cfg)
    }
}
