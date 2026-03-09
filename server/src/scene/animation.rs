use indexmap::IndexMap;

use crate::scene::stimulus::Stimulus;

pub type FinalActionMask = u8;

// Final action bits (matching C++ constants):
// bit 0 = disable stimulus on finish
// bit 2 = toggle photodiode on finish
// bit 3 = signal event on finish
// bit 4 = restart animation on finish
// bit 5 = reverse direction on finish
// bit 6 = restore initial state on finish
// bit 7 = end deferred mode on finish

/// Interface for all animation types.
///
/// Animations are stored as `Box<dyn Animation>` because their internal state
/// is highly heterogeneous and the per-frame `advance()` interface is uniform
/// regardless of implementation.
///
/// # `Send + Sync` requirement
///
/// `Animation` requires both `Send` and `Sync` because `SceneState` (which
/// owns a map of `Box<dyn Animation>`) is shared across threads via
/// `Arc<RwLock<SceneState>>`.  For `Arc<RwLock<T>>` to be `Send`, `T` must
/// be `Send + Sync`.  All concrete animation types must therefore be `Send +
/// Sync`; this is trivially satisfied as long as they contain no raw pointers
/// or thread-local state.
///
/// Note: the `command()` method (for handling per-animation protobuf commands)
/// will be added in Phase 3 once the protobuf types are available.
pub trait Animation: Send + Sync + 'static {
    /// Advance the animation by one frame, updating the assigned stimulus.
    fn advance(
        &mut self,
        stimuli:    &mut IndexMap<u32, Stimulus>,
        frame_rate: f32,
        deferred:   bool,
    );

    /// Assign this animation to a stimulus handle.
    fn assign(&mut self, handle: u32);

    /// Remove the assignment. May restore the stimulus to its initial state
    /// depending on `final_action`.
    fn deassign(&mut self, stimuli: &mut IndexMap<u32, Stimulus>);

    fn stimulus_handle(&self) -> Option<u32>;

    fn final_action(&self) -> FinalActionMask;

    fn set_final_action(&mut self, mask: FinalActionMask);
}
