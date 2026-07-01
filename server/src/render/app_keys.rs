use crate::render::overlay_ui::OverlayGroup;

/// Application-level key actions, shared between the DRM and winit backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKey {
    /// Close the open dialog, else hide the overlay. Never quits.
    Escape,
    /// Toggle the whole overlay on/off (backtick).
    ToggleOverlay,
    /// Show (make visible + focus) one overlay group (F1–F7).
    ShowGroup(OverlayGroup),
    /// Hide one overlay group (Shift+F1–F7).
    HideGroup(OverlayGroup),
    /// Spawn demo stimuli (only acted on when the overlay is hidden).
    D,
    /// Ctrl+Alt+Fn — forward to the kernel as a VT switch.
    SwitchVt(u16),
    /// Ctrl+Q — quit the process (DRM mode has no window manager to send a
    /// close request, so this is the only in-session quit hotkey).
    Quit,
}
