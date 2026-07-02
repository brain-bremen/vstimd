//! Start/final action bitflags applied when an animation transitions state.

bitflags::bitflags! {
    #[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
    pub struct StartAction: u8 {
        /// Enable stimuli when animation transitions Armed → Running.
        const ENABLE                      = 0x02;
        const TOGGLE_PHOTODIODE           = 0x04;
        const START_ACTION_TRIGGER_LINE   = 0x08;
    }
}

bitflags::bitflags! {
    #[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
    pub struct FinalAction: u8 {
        const DISABLE                 = 0x01;
        const TOGGLE_PHOTODIODE       = 0x04;
        const FINAL_ACTION_TRIGGER_LINE = 0x08;
        const RESTART                 = 0x10;
        const REVERSE                 = 0x20;
        const RESTORE_STATE           = 0x40;
        const END_DEFERRED            = 0x80;
    }
}

bitflags::bitflags! {
    /// Actions applied when a cancel trigger fires (edge or software command).
    /// Bit values mirror [`FinalAction`] so the teardown can be shared; `RESTART`
    /// and `REVERSE` are intentionally absent — cancel is always terminal.
    #[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
    pub struct CancelAction: u8 {
        /// Disable stimuli on cancel.
        const DISABLE                    = 0x01;
        const TOGGLE_PHOTODIODE          = 0x04;
        /// Pulse `cancel_action_trigger_line` for one frame on cancel.
        const CANCEL_ACTION_TRIGGER_LINE = 0x08;
        /// Restore `user_enabled` captured at start (Running only; no-op if Armed).
        const RESTORE_STATE              = 0x40;
        const END_DEFERRED               = 0x80;
    }
}

impl serde::Serialize for StartAction {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.bits().serialize(s)
    }
}
impl<'de> serde::Deserialize<'de> for StartAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::from_bits_truncate(u8::deserialize(d)?))
    }
}

impl serde::Serialize for FinalAction {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.bits().serialize(s)
    }
}
impl<'de> serde::Deserialize<'de> for FinalAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::from_bits_truncate(u8::deserialize(d)?))
    }
}

impl serde::Serialize for CancelAction {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.bits().serialize(s)
    }
}
impl<'de> serde::Deserialize<'de> for CancelAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::from_bits_truncate(u8::deserialize(d)?))
    }
}

impl CancelAction {
    /// The cancel-action bits reinterpreted as [`FinalAction`] bits (identical
    /// values) so the shared teardown in `finalize` can apply them. The
    /// trigger-line bit maps to `FINAL_ACTION_TRIGGER_LINE`, driving whichever
    /// trigger line the caller passes.
    pub fn as_final_action(self) -> FinalAction {
        FinalAction::from_bits_truncate(self.bits())
    }
}
