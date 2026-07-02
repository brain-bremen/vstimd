//! Serializable animation configuration (`AnimationConfig`) and the full scene
//! entry (`AnimationEntry`) that pairs it with non-serialized runtime state.

use super::{AnimState, Animation, CancelAction, FinalAction, StartAction};
use crate::vtl_state::{VtlEdge, VtlBit};

/// Serializable animation configuration.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AnimationConfig {
    pub name: String,
    pub state: AnimState,
    pub stimuli: Vec<u32>,
    /// Bitflags applied when the animation transitions Armed → Running.
    pub start_action: StartAction,
    /// Output line to pulse for one frame when `START_ACTION_TRIGGER_LINE` is set.
    pub start_action_trigger_line: Option<VtlBit>,
    /// Bitflags controlling what happens when the animation completes.
    pub final_action: FinalAction,
    /// Output line to pulse for one frame when `FINAL_ACTION_TRIGGER_LINE` is set.
    pub final_action_trigger_line: Option<VtlBit>,
    /// If `Some`, the animation waits for this edge before starting.
    pub start_trigger: Option<(VtlBit, VtlEdge)>,
    /// If `Some`, this input edge cancels the animation while it is `Armed` or
    /// `Running`. Same wiring as `start_trigger`; evaluated each frame in
    /// `advance_one`.
    #[serde(default)]
    pub cancel_trigger: Option<(VtlBit, VtlEdge)>,
    /// Bitflags applied when the animation is cancelled (edge or software).
    /// Independent of `final_action`; `empty()` means a hard abort that leaves
    /// visibility as-is.
    #[serde(default)]
    pub cancel_action: CancelAction,
    /// Output line to pulse for one frame when `CANCEL_ACTION_TRIGGER_LINE` is set.
    #[serde(default)]
    pub cancel_action_trigger_line: Option<VtlBit>,
    pub animation: Animation,
}

/// Full animation entry: serializable config + runtime state.
/// Deref/DerefMut give transparent access to the config fields.
#[derive(Clone)]
pub struct AnimationEntry {
    pub config: AnimationConfig,
    /// Snapshot of each stimulus's `user_enabled` taken when the animation first
    /// transitions to Running. Used by `RESTORE_STATE` to undo visibility changes.
    /// Not serialized — always None in saved configs.
    pub captured_user_enabled: Option<Vec<bool>>,
}

impl std::ops::Deref for AnimationEntry {
    type Target = AnimationConfig;
    fn deref(&self) -> &AnimationConfig { &self.config }
}

impl std::ops::DerefMut for AnimationEntry {
    fn deref_mut(&mut self) -> &mut AnimationConfig { &mut self.config }
}

impl serde::Serialize for AnimationEntry {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.config.serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for AnimationEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self { config: AnimationConfig::deserialize(d)?, captured_user_enabled: None })
    }
}

impl AnimationEntry {
    pub fn new(animation: Animation, stimuli: Vec<u32>) -> Self {
        Self {
            config: AnimationConfig {
                name: String::new(),
                state: AnimState::Idle,
                stimuli,
                start_action: StartAction::empty(),
                start_action_trigger_line: None,
                final_action: FinalAction::empty(),
                final_action_trigger_line: None,
                start_trigger: None,
                cancel_trigger: None,
                cancel_action: CancelAction::empty(),
                cancel_action_trigger_line: None,
                animation,
            },
            captured_user_enabled: None,
        }
    }

    pub fn armed(animation: Animation, stimuli: Vec<u32>) -> Self {
        let mut e = Self::new(animation, stimuli);
        e.state = AnimState::Armed;
        e
    }
}
