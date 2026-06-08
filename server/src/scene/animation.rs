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

/// Which stimulus parameter a `LinearRange` animation drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimParam {
    PositionX,
    PositionY,
    Alpha,
    GratingPhase,
    GratingContrast,
    GratingSf,
}

#[derive(Clone, Debug)]
pub enum Animation {
    // ── Trigger-reactive ────────────────────────────────────────────────────
    /// Mirror stimulus enabled state to the level of a trigger line.
    CoupleVisibility { trigger: VtlBit, polarity: bool, stimuli: Vec<u32> },
    /// Set stimulus enabled once when a trigger edge fires.
    EdgeSetEnabled   { trigger: VtlBit, edge: Edge, stimuli: Vec<u32>, enabled: bool },
    /// Enable stimuli for `duration_frames` after a trigger edge.
    TriggerFlash     { trigger: VtlBit, edge: Edge, stimuli: Vec<u32>, duration_frames: u32 },
    /// Flicker stimuli on/off after a trigger edge.
    TriggerFlicker   { trigger: VtlBit, edge: Edge, stimuli: Vec<u32>, on_frames: u32, off_frames: u32, total_frames: Option<u32> },

    // ── Free-running (fire immediately when Armed) ───────────────────────────
    /// Enable stimuli for `duration_frames`.
    Flash          { stimuli: Vec<u32>, duration_frames: u32 },
    /// Flicker stimuli on/off.
    Flicker        { stimuli: Vec<u32>, on_frames: u32, off_frames: u32, total_frames: Option<u32> },
    /// Sinusoidal position oscillation.
    Harmonic       { stimuli: Vec<u32>, amplitude: f32, phase_inc: f32, direction_deg: f32 },
    /// Linearly interpolate a parameter over `duration_frames`.
    LinearRange    { stimuli: Vec<u32>, param: AnimParam, start: f32, end: f32, duration_frames: u32 },
    /// Read position from a POSIX shm float array each frame.
    ExternalPosition { stimuli: Vec<u32>, shm_name: String, x_offset: f32, y_offset: f32 },

    // ── Output-driving ───────────────────────────────────────────────────────
    /// Drive an output line HIGH for `pulse_frames` every frame.
    FrameOnsetOutput   { output: VtlBit, pulse_frames: u32 },
    /// Mirror any listed stimulus's visibility to an output line.
    StimulusVisibleOut { output: VtlBit, stimuli: Vec<u32> },
}

impl Animation {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::CoupleVisibility { .. }   => "CoupleVisibility",
            Self::EdgeSetEnabled { .. }     => "EdgeSetEnabled",
            Self::TriggerFlash { .. }       => "TriggerFlash",
            Self::TriggerFlicker { .. }     => "TriggerFlicker",
            Self::Flash { .. }              => "Flash",
            Self::Flicker { .. }            => "Flicker",
            Self::Harmonic { .. }           => "Harmonic",
            Self::LinearRange { .. }        => "LinearRange",
            Self::ExternalPosition { .. }   => "ExternalPosition",
            Self::FrameOnsetOutput { .. }   => "FrameOnsetOutput",
            Self::StimulusVisibleOut { .. } => "StimulusVisibleOut",
        }
    }
}

pub struct AnimationEntry {
    pub name:         String,
    pub state:        AnimState,
    /// Bitflags controlling what happens when the animation completes.
    pub final_action: FinalAction,
    /// Output line to pulse for one frame when `SIGNAL_EVENT` is set.
    pub signal_event: Option<VtlBit>,
    pub animation:    Animation,
}
