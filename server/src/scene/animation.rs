bitflags::bitflags! {
    #[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
    pub struct FinalAction: u8 {
        const DISABLE           = 0x01;
        const TOGGLE_PHOTODIODE = 0x04;
        const SIGNAL_EVENT      = 0x08;
        const RESTART           = 0x10;
        const REVERSE           = 0x20;
        const END_DEFERRED      = 0x80;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AnimState {
    Idle,
    Armed,
    Running { frame_counter: u32 },
    Done,
}

pub use crate::vtl_state::VtlBit;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Rising,
    Falling,
}

#[derive(Clone, Debug)]
pub enum Animation {
    // ── Trigger-reactive ────────────────────────────────────────────────────
    /// Mirror stimulus enabled state to the level of an input trigger line.
    CoupleVisibilityToInputTriggerLine { trigger: VtlBit, polarity: bool, stimuli: Vec<u32> },
    /// Set stimulus enabled once when a trigger edge fires.
    EdgeSetEnabled   { trigger: VtlBit, edge: Edge, stimuli: Vec<u32>, enabled: bool },
    /// Enable stimuli for `duration_frames`.
    Flash { stimuli: Vec<u32>, duration_frames: u32 },
    /// Flicker stimuli on/off.
    Flicker { stimuli: Vec<u32>, on_frames: u32, off_frames: u32, total_frames: Option<u32> },
    /// Read 2-D position from a POSIX shm float array each frame.
    ExternalPosition2D { stimuli: Vec<u32>, shm_name: String, x_offset: f32, y_offset: f32 },

}

impl Animation {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::CoupleVisibilityToInputTriggerLine { .. } => "CoupleVisibilityToInputTriggerLine",
            Self::EdgeSetEnabled { .. }     => "EdgeSetEnabled",
            Self::Flash { .. }              => "Flash",
            Self::Flicker { .. }            => "Flicker",
            Self::ExternalPosition2D { .. } => "ExternalPosition2D",
        }
    }
}

pub struct AnimationEntry {
    pub name:          String,
    pub state:         AnimState,
    /// Bitflags controlling what happens when the animation completes.
    pub final_action:  FinalAction,
    /// Output line to pulse for one frame when `SIGNAL_EVENT` is set.
    pub signal_event:  Option<VtlBit>,
    /// If `Some`, the animation waits for this edge before starting.
    pub start_trigger: Option<(VtlBit, Edge)>,
    pub animation:     Animation,
}
