//! Animation domain: types split per-file plus the per-frame advance engine.
//! This module file only wires the submodules together — no logic lives here.

mod animation_action;
mod animation_advance;
mod animation_entry;
mod animation_kind;
mod animation_state;

pub use animation_action::{CancelAction, FinalAction, StartAction};
pub(crate) use animation_advance::{advance_one, cancel_one};
pub use animation_entry::{AnimationConfig, AnimationEntry};
pub use animation_kind::Animation;
pub use animation_state::AnimState;

pub use crate::vtl_state::{VtlEdge, VtlBit};
