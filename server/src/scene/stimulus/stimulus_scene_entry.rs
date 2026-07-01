use super::Stimulus;
use uuid::Uuid;

// ── StimulusEntry ─────────────────────────────────────────────────────────────

/// Metadata + stimulus stored as one unit in `SceneState::stimuli`.
///
/// `id` is stable across sessions (survives serialization round-trips and lets
/// reconnecting clients match server-side stimuli to their in-memory objects).
/// `name` is optional human-readable label for debugging/tooling.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StimulusSceneEntry {
    pub id: Uuid,
    pub name: Option<String>,
    pub stimulus: Stimulus,
}

impl StimulusSceneEntry {
    pub fn new(id: Uuid, name: Option<String>, stimulus: Stimulus) -> Self {
        Self { id, name, stimulus }
    }
}
