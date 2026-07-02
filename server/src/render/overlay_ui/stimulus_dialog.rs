//! Keyboard-driven dialog for creating a stimulus from the overlay.
//!
//! Modeled on [`FileBrowser`](super::file_browser::FileBrowser): an `open` flag
//! plus [`take_result`](StimulusDialog::take_result) that yields a finished
//! [`StimulusEntry`] for the caller to insert via [`SceneState::add_stimulus`].
//! All inputs are `DragValue`/`TextEdit` widgets reachable by Tab, so the dialog
//! is fully usable without a mouse (the DRM rig has no pointer).

use uuid::Uuid;

use crate::Color;
use crate::scene::{
    CircleStimulus, Deferred, EllipseStimulus, GratingParams, GratingStimulus, RectStimulus,
    ShapeAppearance, ShapeCommon, Stimulus, StimulusFlags, StimulusSceneEntry, Transform2D,
    Waveform,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum StimulusDialogKind {
    Rect,
    Circle,
    Ellipse,
    Grating,
}

pub struct StimulusDialog {
    pub open: bool,
    /// Set when the dialog is opened so the first field grabs keyboard focus.
    focus_first: bool,
    kind: StimulusDialogKind,
    name: String,
    pos: [f32; 2],
    angle: f32,
    /// Full width/height in px (halved on build).
    rect_size: [f32; 2],
    circle_radius: f32,
    /// Full diameters in px (halved on build).
    ellipse_size: [f32; 2],
    grating_size: [f32; 2],
    fill: [f32; 4],
    grating_sf: f32,
    grating_contrast: f32,
    grating_drift: f32,
    grating_waveform: Waveform,
    result: Option<StimulusSceneEntry>,
}

impl Default for StimulusDialog {
    fn default() -> Self {
        Self {
            open: false,
            focus_first: false,
            kind: StimulusDialogKind::Rect,
            name: String::new(),
            pos: [0.0, 0.0],
            angle: 0.0,
            rect_size: [120.0, 80.0],
            circle_radius: 80.0,
            ellipse_size: [160.0, 100.0],
            grating_size: [200.0, 200.0],
            fill: [0.0, 0.8, 0.8, 1.0],
            grating_sf: 0.05,
            grating_contrast: 1.0,
            grating_drift: 1.0,
            grating_waveform: Waveform::Sin,
            result: None,
        }
    }
}

impl StimulusDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.focus_first = true;
        self.result = None;
    }

    pub fn take_result(&mut self) -> Option<StimulusSceneEntry> {
        self.result.take()
    }

    fn build_entry(&self) -> StimulusSceneEntry {
        let flags = StimulusFlags::enabled(true);
        let transform = Deferred::new(Transform2D {
            pos: self.pos,
            angle: self.angle,
        });
        let appearance = Deferred::new(ShapeAppearance {
            fill_color: Color::new(self.fill[0], self.fill[1], self.fill[2], self.fill[3]),
            ..Default::default()
        });
        let common = ShapeCommon {
            flags,
            transform,
            appearance,
        };
        let stimulus = match self.kind {
            StimulusDialogKind::Rect => Stimulus::Rect(RectStimulus {
                common,
                size: Deferred::new([self.rect_size[0] / 2.0, self.rect_size[1] / 2.0]),
            }),
            StimulusDialogKind::Circle => Stimulus::Circle(CircleStimulus {
                common,
                radius: Deferred::new(self.circle_radius),
            }),
            StimulusDialogKind::Ellipse => Stimulus::Ellipse(EllipseStimulus {
                common,
                radii: Deferred::new([self.ellipse_size[0] / 2.0, self.ellipse_size[1] / 2.0]),
            }),
            StimulusDialogKind::Grating => Stimulus::Grating(GratingStimulus::new(
                self.pos,
                self.angle,
                [self.grating_size[0] / 2.0, self.grating_size[1] / 2.0],
                GratingParams {
                    sf: self.grating_sf,
                    contrast: self.grating_contrast,
                    drift_speed: self.grating_drift,
                    waveform: self.grating_waveform,
                    ..Default::default()
                },
            )),
        };
        let name = (!self.name.trim().is_empty()).then(|| self.name.trim().to_string());
        StimulusSceneEntry::new(Uuid::new_v4(), name, stimulus)
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Create Stimulus")
            .open(&mut open)
            .resizable(false)
            .default_size([320.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Type:");
                    let r = ui.selectable_value(&mut self.kind, StimulusDialogKind::Rect, "Rect");
                    if self.focus_first {
                        r.request_focus();
                        self.focus_first = false;
                    }
                    ui.selectable_value(&mut self.kind, StimulusDialogKind::Circle, "Circle");
                    ui.selectable_value(&mut self.kind, StimulusDialogKind::Ellipse, "Ellipse");
                    ui.selectable_value(&mut self.kind, StimulusDialogKind::Grating, "Grating");
                });
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.name);
                });
                egui::Grid::new("stim_dialog_grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Position x,y");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut self.pos[0]).speed(1.0));
                            ui.add(egui::DragValue::new(&mut self.pos[1]).speed(1.0));
                        });
                        ui.end_row();

                        if self.kind != StimulusDialogKind::Circle {
                            ui.label("Angle°");
                            ui.add(egui::DragValue::new(&mut self.angle).speed(1.0));
                            ui.end_row();
                        }

                        match self.kind {
                            StimulusDialogKind::Rect => {
                                ui.label("Size w×h");
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::DragValue::new(&mut self.rect_size[0])
                                            .speed(1.0)
                                            .range(1.0..=4096.0),
                                    );
                                    ui.add(
                                        egui::DragValue::new(&mut self.rect_size[1])
                                            .speed(1.0)
                                            .range(1.0..=4096.0),
                                    );
                                });
                                ui.end_row();
                            }
                            StimulusDialogKind::Circle => {
                                ui.label("Radius");
                                ui.add(
                                    egui::DragValue::new(&mut self.circle_radius)
                                        .speed(1.0)
                                        .range(1.0..=4096.0),
                                );
                                ui.end_row();
                            }
                            StimulusDialogKind::Ellipse => {
                                ui.label("Size w×h");
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::DragValue::new(&mut self.ellipse_size[0])
                                            .speed(1.0)
                                            .range(1.0..=4096.0),
                                    );
                                    ui.add(
                                        egui::DragValue::new(&mut self.ellipse_size[1])
                                            .speed(1.0)
                                            .range(1.0..=4096.0),
                                    );
                                });
                                ui.end_row();
                            }
                            StimulusDialogKind::Grating => {
                                ui.label("Size w×h");
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::DragValue::new(&mut self.grating_size[0])
                                            .speed(1.0)
                                            .range(1.0..=4096.0),
                                    );
                                    ui.add(
                                        egui::DragValue::new(&mut self.grating_size[1])
                                            .speed(1.0)
                                            .range(1.0..=4096.0),
                                    );
                                });
                                ui.end_row();
                                ui.label("Spatial freq (cyc/px)");
                                ui.add(
                                    egui::DragValue::new(&mut self.grating_sf)
                                        .speed(0.001)
                                        .range(0.0..=1.0),
                                );
                                ui.end_row();
                                ui.label("Contrast");
                                ui.add(
                                    egui::DragValue::new(&mut self.grating_contrast)
                                        .speed(0.01)
                                        .range(0.0..=1.0),
                                );
                                ui.end_row();
                                ui.label("Drift (cyc/s)");
                                ui.add(egui::DragValue::new(&mut self.grating_drift).speed(0.05));
                                ui.end_row();
                                ui.label("Waveform");
                                ui.horizontal(|ui| {
                                    ui.selectable_value(
                                        &mut self.grating_waveform,
                                        Waveform::Sin,
                                        "Sin",
                                    );
                                    ui.selectable_value(
                                        &mut self.grating_waveform,
                                        Waveform::Sqr,
                                        "Sqr",
                                    );
                                    ui.selectable_value(
                                        &mut self.grating_waveform,
                                        Waveform::Saw,
                                        "Saw",
                                    );
                                    ui.selectable_value(
                                        &mut self.grating_waveform,
                                        Waveform::Tri,
                                        "Tri",
                                    );
                                });
                                ui.end_row();
                            }
                        }

                        // Grating uses its own fore/back colors; fill applies to shapes only.
                        if self.kind != StimulusDialogKind::Grating {
                            ui.label("Fill RGBA");
                            ui.horizontal(|ui| {
                                for c in &mut self.fill {
                                    ui.add(egui::DragValue::new(c).speed(0.01).range(0.0..=1.0));
                                }
                            });
                            ui.end_row();
                        }
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        self.result = Some(self.build_entry());
                        self.open = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                    }
                });
            });

        // Combine window-X close (open) with button-driven close (self.open).
        self.open &= open;
    }
}
