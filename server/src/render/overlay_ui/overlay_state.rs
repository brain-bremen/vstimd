//! Overlay group selection, focus, and owned dialog state.
//!
//! The overlay is a set of floating windows ("groups"), each toggled and focused
//! by a function key. Multiple groups can be visible at once. `OverlayState`
//! holds which groups are shown, which one owns keyboard focus, and the modal
//! dialogs/sub-state the groups drive.

use std::path::PathBuf;

use crate::render::benchmark::BenchmarkState;

use super::animation_dialog::AnimationDialog;
use super::file_browser::FileBrowser;
use super::stimulus_dialog::StimulusDialog;

/// A focusable overlay window. Order matches the F1..F7 key assignment.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum OverlayGroup {
    Stimuli,
    Log,
    Vtl,
    Animations,
    System,
    Config,
    Benchmarks,
}

impl OverlayGroup {
    pub const ALL: [OverlayGroup; 7] = [
        OverlayGroup::Stimuli,
        OverlayGroup::Log,
        OverlayGroup::Vtl,
        OverlayGroup::Animations,
        OverlayGroup::System,
        OverlayGroup::Config,
        OverlayGroup::Benchmarks,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&g| g == self).unwrap()
    }

    pub fn title(self) -> &'static str {
        match self {
            OverlayGroup::Stimuli => "Stimuli",
            OverlayGroup::Log => "Log",
            OverlayGroup::Vtl => "Virtual Trigger",
            OverlayGroup::Animations => "Animations",
            OverlayGroup::System => "System",
            OverlayGroup::Config => "Config",
            OverlayGroup::Benchmarks => "Benchmarks",
        }
    }

    /// Label of the function key that toggles this group (1-based).
    pub fn fkey_label(self) -> &'static str {
        ["F1", "F2", "F3", "F4", "F5", "F6", "F7"][self.index()]
    }

    /// Map a function-key number (1..=7) to a group.
    pub fn from_fkey(n: u8) -> Option<Self> {
        Self::ALL.get((n as usize).checked_sub(1)?).copied()
    }
}

pub struct OverlayState {
    /// Master visibility — the backtick (`` ` ``) toggle. When false the overlay
    /// is not built at all and no keyboard input is routed to egui.
    pub master_visible: bool,
    /// Per-group visibility, indexed by `OverlayGroup::index`.
    pub visible: [bool; 7],
    /// The group that owns keyboard focus.
    pub focused: OverlayGroup,
    /// Set when focus moves to a group so its first widget grabs keyboard focus
    /// on the next frame; consumed by the overlay builder.
    pub pending_focus: bool,
    /// Set by the System group's wireframe toggle; applied (and cleared) by the
    /// render loop, which owns the scene-renderer pipeline state.
    pub wireframe_toggle_requested: bool,

    pub file_browser: FileBrowser,
    pub benchmark: BenchmarkState,
    pub stimulus_dialog: StimulusDialog,
    pub animation_dialog: AnimationDialog,
}

impl OverlayState {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            master_visible: false,
            visible: [false; 7],
            focused: OverlayGroup::Stimuli,
            pending_focus: false,
            wireframe_toggle_requested: false,
            file_browser: FileBrowser::new(config_dir),
            benchmark: BenchmarkState::new(),
            stimulus_dialog: StimulusDialog::default(),
            animation_dialog: AnimationDialog::default(),
        }
    }

    pub fn toggle_master(&mut self) {
        self.master_visible = !self.master_visible;
    }

    /// Show a group (make visible), reveal the overlay, and give it focus.
    pub fn show_group(&mut self, group: OverlayGroup) {
        self.master_visible = true;
        self.visible[group.index()] = true;
        self.focused = group;
        self.pending_focus = true;
    }

    /// Hide a group. If no groups remain visible the master overlay is hidden too.
    pub fn hide_group(&mut self, group: OverlayGroup) {
        self.visible[group.index()] = false;
        if self.focused == group {
            // Move focus to the first remaining visible group, if any.
            if let Some(&next) = OverlayGroup::ALL.iter().find(|&&g| self.visible[g.index()]) {
                self.focused = next;
            }
        }
        if !self.visible.iter().any(|&v| v) {
            self.master_visible = false;
        }
    }

    /// Toggle a group's visibility, reveal the overlay, and give it focus.
    /// Kept for DRM backend parity.
    pub fn select_group(&mut self, group: OverlayGroup) {
        if self.visible[group.index()] {
            self.hide_group(group);
        } else {
            self.show_group(group);
        }
    }

    pub fn is_visible(&self, group: OverlayGroup) -> bool {
        self.visible[group.index()]
    }

    pub fn visible_mut(&mut self, group: OverlayGroup) -> &mut bool {
        &mut self.visible[group.index()]
    }

    pub fn any_dialog_open(&self) -> bool {
        self.file_browser.open || self.stimulus_dialog.open || self.animation_dialog.open
    }

    fn close_dialogs(&mut self) {
        self.file_browser.open = false;
        self.stimulus_dialog.open = false;
        self.animation_dialog.open = false;
    }

    /// Esc handling — never quits. Closes an open dialog first, otherwise hides
    /// the whole overlay.
    pub fn handle_escape(&mut self) {
        if self.any_dialog_open() {
            self.close_dialogs();
        } else if self.master_visible {
            self.master_visible = false;
        }
    }
}
