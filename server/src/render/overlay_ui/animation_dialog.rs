//! Keyboard-driven dialog for creating an animation from the overlay.
//!
//! Builds an [`AnimationEntry`] the caller inserts via
//! [`SceneState::add_animation`]. Covers the keyboard-friendly [`Animation`]
//! variants; `MoveAlongPath2D` (bulk coords) and `ExternalPosition2D` (shm) are
//! intentionally omitted from v1.

use crate::scene::animation::{Animation, CancelAction, FinalAction, StartAction};
use crate::scene::{AnimState, AnimationEntry, VtlEdge, VtlBit};
use vtl::VtlKind;

/// A VTL line offered as a trigger choice: display label + resolved address.
#[derive(Clone)]
pub struct TriggerLine {
    pub label: String,
    pub bit: VtlBit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnimationDialogKind {
    Flash,
    Flicker,
    EnableOnEdge,
    CoupleVisibility,
    MoveSegments,
}

pub struct AnimationDialog {
    pub open: bool,
    focus_first: bool,
    kind: AnimationDialogKind,
    name: String,
    /// One bool per stimulus handle offered, parallel to the `stimuli` slice.
    targets: Vec<bool>,
    enable_on_start: bool,
    arm_immediately: bool,
    // Flash
    flash_frames: u32,
    // Flicker
    on_frames: u32,
    off_frames: u32,
    flicker_total: u32,
    flicker_forever: bool,
    start_on_phase: bool,
    // Trigger-based
    line_idx: usize,
    edge_rising: bool,
    enable_to: bool,
    polarity: bool,
    // Move
    waypoints_text: String,
    speed: f32,
    // Start trigger: gate the animation on a VTL edge (after arming).
    start_trig_enabled: bool,
    start_bank: u32,
    start_bit: u32,
    start_trig_rising: bool,
    start_trig_output: bool,
    // Cancel trigger: abort the animation on a VTL edge.
    cancel_trig_enabled: bool,
    cancel_bank: u32,
    cancel_bit: u32,
    cancel_trig_rising: bool,
    cancel_trig_output: bool,
    // Cancel actions: applied on cancel (edge or software).
    cancel_disable: bool,
    cancel_toggle_pd: bool,
    cancel_restore: bool,
    cancel_pulse_enabled: bool,
    cancel_pulse_bank: u32,
    cancel_pulse_bit: u32,
    // Final trigger: pulse a VTL output line when the animation completes.
    final_trig_enabled: bool,
    final_bank: u32,
    final_bit: u32,
    result: Option<AnimationEntry>,
}

impl Default for AnimationDialog {
    fn default() -> Self {
        Self {
            open: false,
            focus_first: false,
            kind: AnimationDialogKind::Flash,
            name: String::new(),
            targets: Vec::new(),
            enable_on_start: true,
            arm_immediately: true,
            flash_frames: 30,
            on_frames: 6,
            off_frames: 6,
            flicker_total: 120,
            flicker_forever: false,
            start_on_phase: true,
            line_idx: 0,
            edge_rising: true,
            enable_to: true,
            polarity: true,
            waypoints_text: "0,0; 200,0; 200,200".to_string(),
            speed: 300.0,
            start_trig_enabled: false,
            start_bank: 0,
            start_bit: 0,
            start_trig_rising: true,
            start_trig_output: false,
            cancel_trig_enabled: false,
            cancel_bank: 0,
            cancel_bit: 0,
            cancel_trig_rising: true,
            cancel_trig_output: false,
            cancel_disable: false,
            cancel_toggle_pd: false,
            cancel_restore: false,
            cancel_pulse_enabled: false,
            cancel_pulse_bank: 0,
            cancel_pulse_bit: 0,
            final_trig_enabled: false,
            final_bank: 0,
            final_bit: 0,
            result: None,
        }
    }
}

impl AnimationDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.focus_first = true;
        self.result = None;
    }

    pub fn take_result(&mut self) -> Option<AnimationEntry> {
        self.result.take()
    }

    fn parse_waypoints(text: &str) -> Vec<[f32; 2]> {
        text.split(';')
            .filter_map(|pair| {
                let mut it = pair.split(',').map(|s| s.trim().parse::<f32>());
                match (it.next(), it.next()) {
                    (Some(Ok(x)), Some(Ok(y))) => Some([x, y]),
                    _ => None,
                }
            })
            .collect()
    }

    fn build(&self, lines: &[TriggerLine], selected: Vec<u32>) -> AnimationEntry {
        let trigger = lines.get(self.line_idx).map(|l| l.bit);
        let edge = if self.edge_rising { VtlEdge::Rising } else { VtlEdge::Falling };
        let animation = match self.kind {
            AnimationDialogKind::Flash => Animation::FlashForNFrames { duration_frames: self.flash_frames },
            AnimationDialogKind::Flicker => Animation::FlickerForNFrames {
                on_frames: self.on_frames.max(1),
                off_frames: self.off_frames.max(1),
                total_frames: (!self.flicker_forever).then_some(self.flicker_total),
                start_on_phase: self.start_on_phase,
            },
            AnimationDialogKind::EnableOnEdge => Animation::EnableOnTriggerEdge {
                trigger: trigger.unwrap_or(VtlBit { bank: 0, bit: 0, kind: VtlKind::Input }),
                edge,
                enabled: self.enable_to,
            },
            AnimationDialogKind::CoupleVisibility => Animation::CoupleVisibilityToTriggerLine {
                trigger: trigger.unwrap_or(VtlBit { bank: 0, bit: 0, kind: VtlKind::Input }),
                polarity: self.polarity,
            },
            AnimationDialogKind::MoveSegments => Animation::MoveAlongSegments2D {
                waypoints: Self::parse_waypoints(&self.waypoints_text),
                speed_px_per_sec: self.speed,
            },
        };

        let mut entry = AnimationEntry::new(animation, selected);
        entry.config.name = self.name.trim().to_string();
        if self.enable_on_start {
            entry.config.start_action |= StartAction::ENABLE;
        }
        if self.start_trig_enabled {
            let edge = if self.start_trig_rising { VtlEdge::Rising } else { VtlEdge::Falling };
            let kind = if self.start_trig_output { VtlKind::Output } else { VtlKind::Input };
            entry.config.start_trigger =
                Some((VtlBit { bank: self.start_bank as usize, bit: self.start_bit as u8, kind }, edge));
        }
        if self.cancel_trig_enabled {
            let edge = if self.cancel_trig_rising { VtlEdge::Rising } else { VtlEdge::Falling };
            let kind = if self.cancel_trig_output { VtlKind::Output } else { VtlKind::Input };
            entry.config.cancel_trigger =
                Some((VtlBit { bank: self.cancel_bank as usize, bit: self.cancel_bit as u8, kind }, edge));
        }
        // Cancel actions apply to both edge and software cancel.
        let mut cancel_action = CancelAction::empty();
        if self.cancel_disable { cancel_action |= CancelAction::DISABLE; }
        if self.cancel_toggle_pd { cancel_action |= CancelAction::TOGGLE_PHOTODIODE; }
        if self.cancel_restore { cancel_action |= CancelAction::RESTORE_STATE; }
        if self.cancel_pulse_enabled {
            cancel_action |= CancelAction::CANCEL_ACTION_TRIGGER_LINE;
            entry.config.cancel_action_trigger_line = Some(VtlBit {
                bank: self.cancel_pulse_bank as usize,
                bit: self.cancel_pulse_bit as u8,
                kind: VtlKind::Output,
            });
        }
        entry.config.cancel_action = cancel_action;
        if self.final_trig_enabled {
            entry.config.final_action |= FinalAction::FINAL_ACTION_TRIGGER_LINE;
            entry.config.final_action_trigger_line = Some(VtlBit {
                bank: self.final_bank as usize,
                bit: self.final_bit as u8,
                kind: VtlKind::Output,
            });
        }
        if self.arm_immediately {
            entry.config.state = AnimState::Armed;
        }
        entry
    }

    /// `stimuli` are `(handle, label)` of currently-defined stimuli; `lines` are
    /// the named VTL lines that can act as triggers.
    pub fn show(&mut self, ctx: &egui::Context, stimuli: &[(u32, String)], lines: &[TriggerLine]) {
        if !self.open {
            return;
        }
        // Keep the target checkbox vec sized to the current stimulus list.
        if self.targets.len() != stimuli.len() {
            self.targets.resize(stimuli.len(), false);
        }

        let mut open = self.open;
        egui::Window::new("Create Animation")
            .open(&mut open)
            .resizable(true)
            .default_size([340.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Type:");
                    let r = ui.selectable_value(&mut self.kind, AnimationDialogKind::Flash, "Flash");
                    if self.focus_first {
                        r.request_focus();
                        self.focus_first = false;
                    }
                    ui.selectable_value(&mut self.kind, AnimationDialogKind::Flicker, "Flicker");
                    ui.selectable_value(&mut self.kind, AnimationDialogKind::EnableOnEdge, "OnEdge");
                    ui.selectable_value(&mut self.kind, AnimationDialogKind::CoupleVisibility, "Couple");
                    ui.selectable_value(&mut self.kind, AnimationDialogKind::MoveSegments, "Move");
                });
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.name);
                });

                ui.separator();
                ui.label(egui::RichText::new("Target stimuli").strong());
                if stimuli.is_empty() {
                    ui.label(egui::RichText::new("(no stimuli defined)").color(egui::Color32::DARK_GRAY));
                }
                egui::ScrollArea::vertical().max_height(96.0).show(ui, |ui| {
                    for (i, (handle, label)) in stimuli.iter().enumerate() {
                        ui.checkbox(&mut self.targets[i], format!("#{handle} {label}"));
                    }
                });

                ui.separator();
                match self.kind {
                    AnimationDialogKind::Flash => {
                        ui.horizontal(|ui| {
                            ui.label("Duration (frames)");
                            ui.add(egui::DragValue::new(&mut self.flash_frames).range(1..=100_000));
                        });
                    }
                    AnimationDialogKind::Flicker => {
                        ui.horizontal(|ui| {
                            ui.label("On / Off frames");
                            ui.add(egui::DragValue::new(&mut self.on_frames).range(1..=10_000));
                            ui.add(egui::DragValue::new(&mut self.off_frames).range(1..=10_000));
                        });
                        ui.checkbox(&mut self.flicker_forever, "Run forever");
                        if !self.flicker_forever {
                            ui.horizontal(|ui| {
                                ui.label("Total frames");
                                ui.add(egui::DragValue::new(&mut self.flicker_total).range(1..=1_000_000));
                            });
                        }
                        ui.checkbox(&mut self.start_on_phase, "Start in on-phase");
                    }
                    AnimationDialogKind::EnableOnEdge | AnimationDialogKind::CoupleVisibility => {
                        self.trigger_picker(ui, lines);
                        if self.kind == AnimationDialogKind::EnableOnEdge {
                            ui.horizontal(|ui| {
                                ui.label("VtlEdge");
                                ui.selectable_value(&mut self.edge_rising, true, "Rising");
                                ui.selectable_value(&mut self.edge_rising, false, "Falling");
                            });
                            ui.checkbox(&mut self.enable_to, "Set enabled = true");
                        } else {
                            ui.checkbox(&mut self.polarity, "Visible when line high");
                        }
                    }
                    AnimationDialogKind::MoveSegments => {
                        ui.horizontal(|ui| {
                            ui.label("Speed (px/s)");
                            ui.add(egui::DragValue::new(&mut self.speed).speed(1.0));
                        });
                        ui.label("Waypoints  \"x,y; x,y; …\"");
                        ui.text_edit_singleline(&mut self.waypoints_text);
                    }
                }

                ui.separator();
                ui.checkbox(&mut self.enable_on_start, "Enable stimuli on start");
                ui.checkbox(&mut self.arm_immediately, "Arm immediately");

                ui.separator();
                ui.label(egui::RichText::new("Triggers (any line)").strong());
                let bank_max = vtl::MAX_BANKS as u32 - 1;
                ui.checkbox(&mut self.start_trig_enabled, "Start on VTL edge");
                if self.start_trig_enabled {
                    ui.horizontal(|ui| {
                        ui.label("Bank/Bit");
                        ui.add(egui::DragValue::new(&mut self.start_bank).range(0..=bank_max));
                        ui.add(egui::DragValue::new(&mut self.start_bit).range(0..=63));
                        ui.selectable_value(&mut self.start_trig_rising, true, "Rising");
                        ui.selectable_value(&mut self.start_trig_rising, false, "Falling");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Line");
                        ui.selectable_value(&mut self.start_trig_output, false, "Input");
                        ui.selectable_value(&mut self.start_trig_output, true, "Output");
                    });
                }
                ui.checkbox(&mut self.cancel_trig_enabled, "Cancel on VTL edge");
                if self.cancel_trig_enabled {
                    ui.horizontal(|ui| {
                        ui.label("Bank/Bit");
                        ui.add(egui::DragValue::new(&mut self.cancel_bank).range(0..=bank_max));
                        ui.add(egui::DragValue::new(&mut self.cancel_bit).range(0..=63));
                        ui.selectable_value(&mut self.cancel_trig_rising, true, "Rising");
                        ui.selectable_value(&mut self.cancel_trig_rising, false, "Falling");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Line");
                        ui.selectable_value(&mut self.cancel_trig_output, false, "Input");
                        ui.selectable_value(&mut self.cancel_trig_output, true, "Output");
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("On cancel:");
                    ui.checkbox(&mut self.cancel_disable, "Disable");
                    ui.checkbox(&mut self.cancel_toggle_pd, "Photodiode");
                    ui.checkbox(&mut self.cancel_restore, "Restore");
                });
                ui.checkbox(&mut self.cancel_pulse_enabled, "Pulse VTL line on cancel");
                if self.cancel_pulse_enabled {
                    ui.horizontal(|ui| {
                        ui.label("Bank/Bit");
                        ui.add(egui::DragValue::new(&mut self.cancel_pulse_bank).range(0..=bank_max));
                        ui.add(egui::DragValue::new(&mut self.cancel_pulse_bit).range(0..=63));
                    });
                }
                ui.checkbox(&mut self.final_trig_enabled, "Pulse VTL line on completion");
                if self.final_trig_enabled {
                    ui.horizontal(|ui| {
                        ui.label("Bank/Bit");
                        ui.add(egui::DragValue::new(&mut self.final_bank).range(0..=bank_max));
                        ui.add(egui::DragValue::new(&mut self.final_bit).range(0..=63));
                    });
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        let selected: Vec<u32> = stimuli.iter().enumerate()
                            .filter(|(i, _)| self.targets.get(*i).copied().unwrap_or(false))
                            .map(|(_, (h, _))| *h)
                            .collect();
                        self.result = Some(self.build(lines, selected));
                        self.open = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                    }
                });
            });

        self.open &= open;
    }

    fn trigger_picker(&mut self, ui: &mut egui::Ui, lines: &[TriggerLine]) {
        if lines.is_empty() {
            ui.label(egui::RichText::new("(no named VTL lines — trigger defaults to bank 0 bit 0)")
                .color(egui::Color32::DARK_GRAY));
            return;
        }
        if self.line_idx >= lines.len() {
            self.line_idx = 0;
        }
        ui.horizontal(|ui| {
            ui.label("Trigger line");
            egui::ComboBox::from_id_salt("anim_trigger_line")
                .selected_text(&lines[self.line_idx].label)
                .show_ui(ui, |ui| {
                    for (i, l) in lines.iter().enumerate() {
                        ui.selectable_value(&mut self.line_idx, i, &l.label);
                    }
                });
        });
    }
}
