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
/// TODO: Replace this trait + `Box<dyn Animation>` with a plain `enum Animation`
/// following the same pattern as `enum Stimulus`. See `STIMULUS_DATA_MODEL.md §13`
/// for the full design. This trait is a placeholder until concrete animation types
/// are implemented in Phase 7+, at which point it should be removed in favour of:
///
/// ```rust
/// pub enum Animation {
///     Flash(FlashAnim),
///     Flicker(FlickerAnim),
///     Harmonic(HarmonicAnim),
///     // ...
/// }
/// ```
///
/// `advance()` and common accessors become methods on the enum, dispatching via
/// `match`. `AnimCommon` holds the shared `stimulus_handle` and `final_action`
/// fields that every variant embeds.
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
