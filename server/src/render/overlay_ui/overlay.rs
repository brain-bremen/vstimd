use std::sync::{Arc, Mutex, RwLock};

use crate::io_config::{load_config, save_config};
use crate::log_buffer::LogBuffer;
use crate::render::SystemMetrics;
use super::animation_dialog::TriggerLine;
use super::file_browser::BrowserMode;
use super::overlay_state::{OverlayGroup, OverlayState};
use crate::scene::stimulus::ShapeStimulus;
use crate::scene::{AnimState, LoadMode, SceneState, Stimulus};
use crate::timing::{FramePhases, FrameStats};
use crate::vtl_state::{VtlConfig, VtlState};

pub use crate::render::system_info::{ClockSource, SystemInfo};
use crate::render::StimulusDisplayInfo;

#[derive(Clone, Copy, PartialEq, Default)]
enum BankFmt { #[default] Dec, Hex, Bin }

const FOCUS_STROKE: egui::Color32 = egui::Color32::from_rgb(90, 160, 255);

/// One dark background color per panel slot (indices match `OverlayGroup::index()`).
const PANEL_COLORS: [egui::Color32; 12] = [
    egui::Color32::from_rgb(25, 28, 65),  // 0 Stimuli    — indigo
    egui::Color32::from_rgb(15, 50, 50),  // 1 Log        — teal
    egui::Color32::from_rgb(15, 55, 20),  // 2 VTL        — forest green
    egui::Color32::from_rgb(60, 35, 12),  // 3 Animations — amber
    egui::Color32::from_rgb(12, 30, 62),  // 4 System     — navy
    egui::Color32::from_rgb(55, 20, 50),  // 5 Config     — magenta
    egui::Color32::from_rgb(50, 50, 12),  // 6 Benchmarks — olive
    egui::Color32::from_rgb(55, 18, 18),  // 7            — crimson
    egui::Color32::from_rgb(15, 42, 35),  // 8            — sea green
    egui::Color32::from_rgb(35, 15, 58),  // 9            — violet
    egui::Color32::from_rgb(58, 35, 15),  // 10           — sienna
    egui::Color32::from_rgb(25, 45, 15),  // 11           — moss
];

fn group_frame(group: OverlayGroup) -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL_COLORS[group.index() % PANEL_COLORS.len()])
        .inner_margin(egui::Margin::same(8))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(55)))
}

pub struct OverlayArgs<'a> {
    pub scene: &'a Arc<RwLock<SceneState>>,
    pub vtl: Option<&'a Mutex<VtlState>>,
    pub frame_stats: &'a mut FrameStats,
    pub last_phases: FramePhases,
    pub sys: &'a SystemInfo,
    pub display: &'a StimulusDisplayInfo,
    pub wireframe: Option<bool>,
    pub metrics: &'a SystemMetrics,
    pub log_buffer: &'a LogBuffer,
    pub overlay: &'a mut OverlayState,
}

/// Render the title bar and content of one group inside a `Panel::left` that
/// the caller already opened. Paints a focus-accent border when `is_focused`.
fn group_panel_header(
    ui: &mut egui::Ui,
    group: OverlayGroup,
    is_focused: bool,
    want_focus: bool,
    closed: &mut bool,
    add: impl FnOnce(&mut egui::Ui, bool),
) {
    if is_focused {
        ui.painter().rect_stroke(
            ui.max_rect(),
            egui::CornerRadius::ZERO,
            egui::Stroke::new(2.0, FOCUS_STROKE),
            egui::StrokeKind::Inside,
        );
    }
    ui.horizontal(|ui| {
        if is_focused {
            ui.label(egui::RichText::new("▶").color(FOCUS_STROKE));
        }
        ui.label(
            egui::RichText::new(format!("{} [{}]", group.title(), group.fkey_label()))
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("x").clicked() {
                *closed = true;
            }
        });
    });
    ui.separator();
    add(ui, want_focus);
}

pub fn build_overlay_ui(ctx: &egui::Context, args: &mut OverlayArgs<'_>) {
    let OverlayArgs {
        scene, vtl, frame_stats, last_phases, sys, display, wireframe, metrics, log_buffer, overlay,
    } = args;
    let last_phases = *last_phases;

    overlay.benchmark.tick(scene, frame_stats);

    let focused = overlay.focused;
    let focus_now = overlay.pending_focus;
    overlay.pending_focus = false;

    let OverlayState {
        master_visible,
        visible,
        focused: _,
        pending_focus: _,
        wireframe_toggle_requested,
        file_browser,
        benchmark,
        stimulus_dialog,
        animation_dialog,
    } = &mut **overlay;

    let want = |g: OverlayGroup| focus_now && focused == g;
    let foc  = |g: OverlayGroup| focused == g;

    // ── Top panel — each visible group is a Panel::left inside ───────────────
    // Panel::left fills the full height of Panel::top, so no circular height
    // dependency. The top panel auto-sizes from the tallest left panel.
    const GROUP_W: f32 = 310.0;
    #[allow(deprecated)]
    egui::Panel::top("overlay_panel")
        .resizable(true)
        .default_size(360.0)
        .show(ctx, |ui| {

        // ── Stimuli ───────────────────────────────────────────────────────────
        if visible[OverlayGroup::Stimuli.index()] {
            let mut closed = false;
            egui::Panel::left("ovl_stimuli").resizable(false).default_size(GROUP_W)
                .frame(group_frame(OverlayGroup::Stimuli))
                .show_inside(ui, |ui| {
                group_panel_header(ui, OverlayGroup::Stimuli,
                    foc(OverlayGroup::Stimuli), want(OverlayGroup::Stimuli), &mut closed,
                    |ui, want_focus| {
                    ui.horizontal(|ui| {
                        let new_btn = ui.button("➕ New stimulus");
                        if want_focus { new_btn.request_focus(); }
                        if new_btn.clicked() { stimulus_dialog.open(); }
                        if ui.button("Spawn demo").clicked() {
                            crate::render::spawn_demo_stimuli(scene);
                        }
                    });
                    ui.separator();
                    if let Ok(mut sc) = scene.try_write() {
                        let handles: Vec<u32> = sc.stimuli.keys().copied().collect();
                        let mut to_delete: Option<u32> = None;
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                            egui::Grid::new("stimuli_grid").striped(true).num_columns(6)
                                .spacing([8.0, 2.0]).show(ui, |ui| {
                                ui.label(egui::RichText::new("En").strong());
                                ui.label(egui::RichText::new("Handle / type").strong());
                                ui.label(egui::RichText::new("Name").strong());
                                ui.label(egui::RichText::new("Pos (px)").strong());
                                ui.label(egui::RichText::new("Size (px)").strong());
                                ui.label("");
                                ui.end_row();
                                for h in handles {
                                    if let Some(entry) = sc.stimuli.get_mut(&h) {
                                        let stim = &entry.stimulus;
                                        let type_name = stim.type_name();
                                        let pos = stim.transform().live.pos;
                                        let size_label = match stim {
                                            Stimulus::Grating(s) => {
                                                let [hw, hh] = s.size.live;
                                                format!("{}×{}", (hw*2.0) as i32, (hh*2.0) as i32)
                                            }
                                            Stimulus::Shape(ShapeStimulus::Rect(s)) => {
                                                let [hw, hh] = s.size.live;
                                                format!("{}×{}", (hw*2.0) as i32, (hh*2.0) as i32)
                                            }
                                            Stimulus::Shape(ShapeStimulus::Circle(s)) =>
                                                format!("r={}", s.radius.live as i32),
                                            Stimulus::Shape(ShapeStimulus::Ellipse(s)) => {
                                                let [rx, ry] = s.radii.live;
                                                format!("{}×{}", (rx*2.0) as i32, (ry*2.0) as i32)
                                            }
                                            Stimulus::Text(s) => {
                                                let [w, h] = s.box_size.live;
                                                format!("{}×{}", w as i32, h as i32)
                                            }
                                        };
                                        let name_label = entry.name.as_deref().unwrap_or("");
                                        let uuid_str = entry.id.to_string();
                                        let flags = entry.stimulus.flags_mut();
                                        ui.checkbox(&mut flags.enabled, "");
                                        ui.label(format!("#{h} {type_name}"));
                                        let disp = if name_label.is_empty() {
                                            &uuid_str[..8]
                                        } else { name_label };
                                        ui.label(egui::RichText::new(disp).color(
                                            if name_label.is_empty() {
                                                egui::Color32::DARK_GRAY
                                            } else { egui::Color32::WHITE }
                                        )).on_hover_text(&uuid_str);
                                        ui.label(format!("{:.0},{:.0}", pos[0], pos[1]));
                                        ui.label(size_label);
                                        if ui.small_button("x")
                                            .on_hover_text("Delete stimulus").clicked() {
                                            to_delete = Some(h);
                                        }
                                        ui.end_row();
                                    }
                                }
                            });
                        });
                        if let Some(h) = to_delete { sc.stimuli.shift_remove(&h); }
                    }
                });
            });
            if closed { visible[OverlayGroup::Stimuli.index()] = false; }
        }

        // ── Log ───────────────────────────────────────────────────────────────
        if visible[OverlayGroup::Log.index()] {
            let mut closed = false;
            egui::Panel::left("ovl_log").resizable(false).default_size(GROUP_W)
                .frame(group_frame(OverlayGroup::Log))
                .show_inside(ui, |ui| {
                group_panel_header(ui, OverlayGroup::Log,
                    foc(OverlayGroup::Log), want(OverlayGroup::Log), &mut closed,
                    |ui, _| {
                    ui.label(egui::RichText::new("Server log").strong());
                    let entries = log_buffer.lock()
                        .map(|buf| buf.iter().map(|e| {
                            let color = match e.level {
                                log::Level::Error => egui::Color32::RED,
                                log::Level::Warn  => egui::Color32::YELLOW,
                                log::Level::Info  => egui::Color32::WHITE,
                                _                 => egui::Color32::GRAY,
                            };
                            (color, format!("[{:>8.1}ms] {:5} {}", e.elapsed_ms, e.level, e.message))
                        }).collect::<Vec<_>>())
                        .unwrap_or_default();
                    egui::ScrollArea::vertical().id_salt("server_log")
                        .stick_to_bottom(true).max_height(160.0).show(ui, |ui| {
                        for (color, text) in entries { ui.colored_label(color, text); }
                    });
                    ui.separator();
                    if let Ok(sc) = scene.try_read() {
                        ui.label(egui::RichText::new(format!(
                            "IPC commands: {}  errors: {}",
                            sc.runtime.command_log_total, sc.runtime.command_log_errors,
                        )).strong());
                        egui::ScrollArea::vertical().id_salt("ipc_log")
                            .stick_to_bottom(true).max_height(140.0).show(ui, |ui| {
                            for entry in &sc.runtime.command_log {
                                let color = if entry.ok {
                                    egui::Color32::from_rgb(80, 200, 80)
                                } else { egui::Color32::RED };
                                ui.colored_label(color, format!(
                                    "[{:>8.1}ms] #{} {} → {}",
                                    entry.elapsed_ms, entry.handle, entry.summary,
                                    if entry.ok { format!("ok ({})", entry.response) }
                                    else { "err".to_string() },
                                ));
                            }
                        });
                    }
                });
            });
            if closed { visible[OverlayGroup::Log.index()] = false; }
        }

        // ── VTL ───────────────────────────────────────────────────────────────
        if visible[OverlayGroup::Vtl.index()] {
            let mut closed = false;
            egui::Panel::left("ovl_vtl").resizable(false).default_size(GROUP_W)
                .frame(group_frame(OverlayGroup::Vtl))
                .show_inside(ui, |ui| {
                group_panel_header(ui, OverlayGroup::Vtl,
                    foc(OverlayGroup::Vtl), want(OverlayGroup::Vtl), &mut closed,
                    |ui, want_focus| {
                    vtl_group(ctx, ui, want_focus, *vtl);
                });
            });
            if closed { visible[OverlayGroup::Vtl.index()] = false; }
        }

        // ── Animations ────────────────────────────────────────────────────────
        if visible[OverlayGroup::Animations.index()] {
            let mut closed = false;
            egui::Panel::left("ovl_anim").resizable(false).default_size(GROUP_W)
                .frame(group_frame(OverlayGroup::Animations))
                .show_inside(ui, |ui| {
                group_panel_header(ui, OverlayGroup::Animations,
                    foc(OverlayGroup::Animations), want(OverlayGroup::Animations), &mut closed,
                    |ui, want_focus| {
                    let new_btn = ui.button("➕ New animation");
                    if want_focus { new_btn.request_focus(); }
                    if new_btn.clicked() { animation_dialog.open(); }
                    ui.separator();
                    if let Ok(mut sc) = scene.try_write() {
                        let handles: Vec<u32> = sc.animations.keys().copied().collect();
                        if handles.is_empty() {
                            ui.label(egui::RichText::new("(no animations)")
                                .color(egui::Color32::DARK_GRAY));
                        }
                        let mut arm: Option<u32> = None;
                        let mut disarm: Option<u32> = None;
                        let mut trigger: Option<u32> = None;
                        let mut delete: Option<u32> = None;
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                            for h in &handles {
                                if let Some(entry) = sc.animations.get(h) {
                                    let (state_txt, state_col) = match entry.state {
                                        AnimState::Idle           => ("Idle",    egui::Color32::GRAY),
                                        AnimState::Armed          => ("Armed",   egui::Color32::YELLOW),
                                        AnimState::Running { .. } => ("Running", egui::Color32::from_rgb(80,200,80)),
                                        AnimState::Done           => ("Done",    egui::Color32::DARK_GRAY),
                                    };
                                    let name = if entry.name.is_empty() {
                                        format!("anim #{h}")
                                    } else { format!("#{h} {}", entry.name) };
                                    ui.horizontal(|ui| {
                                        ui.colored_label(state_col, format!("● {state_txt}"));
                                        ui.label(format!("{name}  ({} stim)", entry.stimuli.len()));
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.small_button("Arm").clicked() { arm = Some(*h); }
                                        if ui.small_button("Disarm").clicked() { disarm = Some(*h); }
                                        if ui.small_button("Trigger")
                                            .on_hover_text("Fire start trigger or run now").clicked() {
                                            trigger = Some(*h);
                                        }
                                        if ui.small_button("x")
                                            .on_hover_text("Delete animation").clicked() {
                                            delete = Some(*h);
                                        }
                                    });
                                    ui.separator();
                                }
                            }
                        });
                        if let Some(h) = arm    { sc.arm_animation(h); }
                        if let Some(h) = disarm { sc.disarm_animation(h); }
                        if let Some(h) = delete { sc.delete_animation(h); }
                        if let Some(h) = trigger {
                            let start_trigger = sc.animations.get(&h)
                                .and_then(|e| e.start_trigger);
                            sc.arm_animation(h);
                            if let (Some((bit, edge)), Some(v)) = (start_trigger, *vtl)
                                && let Ok(vst) = v.try_lock()
                            {
                                let owner = vst.owner();
                                let mask = 1u64 << bit.bit;
                                match edge {
                                    crate::scene::Edge::Rising => {
                                        owner.set_input_bit(bit.bank, bit.bit);
                                        owner.set_input_rise(bit.bank, mask);
                                    }
                                    crate::scene::Edge::Falling => {
                                        owner.clear_input_bit(bit.bank, bit.bit);
                                        owner.set_input_fall(bit.bank, mask);
                                    }
                                }
                            }
                        }
                    }
                });
            });
            if closed { visible[OverlayGroup::Animations.index()] = false; }
        }

        // ── System ────────────────────────────────────────────────────────────
        if visible[OverlayGroup::System.index()] {
            let mut closed = false;
            egui::Panel::left("ovl_system").resizable(false).default_size(GROUP_W)
                .frame(group_frame(OverlayGroup::System))
                .show_inside(ui, |ui| {
                group_panel_header(ui, OverlayGroup::System,
                    foc(OverlayGroup::System), want(OverlayGroup::System), &mut closed,
                    |ui, _| {
                    system_group(ui, sys, display, *wireframe, metrics, scene,
                        wireframe_toggle_requested);
                    ui.separator();
                    frame_timing(ui, frame_stats, last_phases);
                });
            });
            if closed { visible[OverlayGroup::System.index()] = false; }
        }

        // ── Config ────────────────────────────────────────────────────────────
        if visible[OverlayGroup::Config.index()] {
            let mut closed = false;
            egui::Panel::left("ovl_config").resizable(false).default_size(GROUP_W)
                .frame(group_frame(OverlayGroup::Config))
                .show_inside(ui, |ui| {
                group_panel_header(ui, OverlayGroup::Config,
                    foc(OverlayGroup::Config), want(OverlayGroup::Config), &mut closed,
                    |ui, want_focus| {
                    ui.label("Save or load the scene + VTL configuration.");
                    ui.horizontal(|ui| {
                        let save = ui.button("Save…");
                        if want_focus { save.request_focus(); }
                        if save.clicked() { file_browser.open_save(); }
                        if ui.button("Open (replace)…").clicked() {
                            file_browser.open_load_replace();
                        }
                        if ui.button("Open (additive)…").clicked() {
                            file_browser.open_load_additive();
                        }
                    });
                });
            });
            if closed { visible[OverlayGroup::Config.index()] = false; }
        }

        // ── Benchmarks ────────────────────────────────────────────────────────
        if visible[OverlayGroup::Benchmarks.index()] {
            let mut closed = false;
            egui::Panel::left("ovl_bench").resizable(false).default_size(GROUP_W)
                .frame(group_frame(OverlayGroup::Benchmarks))
                .show_inside(ui, |ui| {
                group_panel_header(ui, OverlayGroup::Benchmarks,
                    foc(OverlayGroup::Benchmarks), want(OverlayGroup::Benchmarks), &mut closed,
                    |ui, want_focus| {
                    ui.heading("Grating stress test");
                    if benchmark.is_running() {
                        let remaining = benchmark.remaining_frames(frame_stats).unwrap_or(0);
                        ui.label(format!("Running… {remaining} frames remaining"));
                    } else {
                        let run = ui.button("Run (200 gratings, 300 frames)");
                        if want_focus { run.request_focus(); }
                        if run.clicked() {
                            benchmark.start_grating_stress(scene, frame_stats,
                                (display.width_px, display.height_px), 20, 10, 300);
                        }
                        if let Some(r) = benchmark.last_result() {
                            ui.separator();
                            ui.label(format!(
                                "{} gratings × {} frames → {} dropped",
                                r.grating_count, r.duration_frames, r.drop_count,
                            ));
                        }
                    }
                });
            });
            if closed { visible[OverlayGroup::Benchmarks.index()] = false; }
        }

        // Central panel consumes remaining space so egui doesn't complain about
        // unoccupied area inside the top panel.
        egui::CentralPanel::default().show_inside(ui, |_| {});
    }); // Panel::top

    // Hide master when all groups were closed via x button.
    if !visible.iter().any(|&v| v) {
        *master_visible = false;
    }

    // ── Dialogs (modal floating windows) ────────────────────────────────────────
    stimulus_dialog.show(ctx);
    if let Some(entry) = stimulus_dialog.take_result() {
        scene.write().unwrap().add_stimulus(entry);
    }

    let (stim_list, trigger_lines) = collect_dialog_inputs(scene, *vtl);
    animation_dialog.show(ctx, &stim_list, &trigger_lines);
    if let Some(entry) = animation_dialog.take_result() {
        scene.write().unwrap().add_animation(entry);
    }

    file_browser.show(ctx);
    if let Some((mode, path)) = file_browser.take_result() {
        handle_file_result(mode, path, scene, *vtl);
    }
}

/// Gather the stimulus list and named VTL trigger lines the animation dialog
/// offers as choices.
fn collect_dialog_inputs(
    scene: &Arc<RwLock<SceneState>>,
    vtl: Option<&Mutex<VtlState>>,
) -> (Vec<(u32, String)>, Vec<TriggerLine>) {
    let stim_list: Vec<(u32, String)> = scene
        .try_read()
        .map(|sc| {
            sc.stimuli.iter().map(|(&h, e)| {
                let label = e.name.clone().unwrap_or_else(|| e.stimulus.type_name().to_string());
                (h, label)
            }).collect()
        })
        .unwrap_or_default();

    let trigger_lines: Vec<TriggerLine> = vtl
        .and_then(|v| v.try_lock().ok())
        .map(|vst| {
            vst.names.iter().map(|e| TriggerLine {
                label: format!("{} ({}/{}, {:?})", e.name, e.bank, e.bit, e.direction),
                bit: crate::scene::VtlBit { bank: e.bank as usize, bit: e.bit },
            }).collect()
        })
        .unwrap_or_default();

    (stim_list, trigger_lines)
}

fn handle_file_result(
    mode: BrowserMode,
    path: std::path::PathBuf,
    scene: &Arc<RwLock<SceneState>>,
    vtl: Option<&Mutex<VtlState>>,
) {
    match mode {
        BrowserMode::Save => {
            let scene_guard = scene.read().unwrap();
            let default_vtl = VtlConfig::default();
            let vtl_guard = vtl.and_then(|v| v.try_lock().ok());
            let vtl_cfg = vtl_guard.as_ref().map(|v| &v.config).unwrap_or(&default_vtl);
            if let Err(e) = save_config(&scene_guard.config, vtl_cfg, &path) {
                log::error!("Config save failed: {e}");
            } else {
                log::info!("Config saved to {:?}", path);
            }
        }
        BrowserMode::OpenReplace | BrowserMode::OpenAdditive => {
            let load_mode = if matches!(mode, BrowserMode::OpenReplace) {
                LoadMode::Replace
            } else {
                LoadMode::Additive
            };
            match load_config(&path) {
                Ok((scene_cfg, io)) => {
                    if let Some(v) = vtl
                        && let Ok(mut v) = v.lock() {
                            v.config.names = io.vtl.names;
                            v.sync_names_to_shm();
                        }
                    scene.write().unwrap().load_snapshot(scene_cfg, load_mode);
                    log::info!("Config loaded from {:?}", path);
                }
                Err(e) => log::error!("Config load failed: {e}"),
            }
        }
    }
}

fn frame_timing(ui: &mut egui::Ui, frame_stats: &mut FrameStats, last_phases: FramePhases) {
    ui.label(egui::RichText::new("Frame timing").strong());
    let s = frame_stats.summary();
    ui.label(format!("FPS: {:.1}  drops: {}", s.fps, s.drop_count));
    ui.label(format!("frame: {:.2} ms  jitter: ±{:.2} ms", s.mean_ms, s.std_ms));
    ui.label(format!("min: {:.2} ms  max: {:.2} ms", s.min_ms, s.max_ms));
    ui.label(format!(
        "phases µs: tess/upload {:>5}  fence {:>5}  acquire {:>5}  record {:>5}  submit {:>5}",
        last_phases.tessellate_us, last_phases.fence_us, last_phases.acquire_us,
        last_phases.record_us, last_phases.submit_us,
    ));

    let durations: Vec<f64> = frame_stats.durations_recent_ns().map(|d| d as f64 / 1_000_000.0).collect();
    if !durations.is_empty() {
        let expected_ms = frame_stats.expected_ns() as f64 / 1_000_000.0;
        let max_ms = durations.iter().cloned().fold(0.0_f64, f64::max).max(expected_ms * 2.5);
        let desired = egui::vec2(ui.available_width(), 64.0);
        let (resp, painter) = ui.allocate_painter(desired, egui::Sense::hover());
        let r = resp.rect;
        painter.rect_filled(r, 0.0, egui::Color32::from_gray(20));
        let n = durations.len();
        let bar_w = r.width() / n as f32;
        for (i, &ms) in durations.iter().enumerate() {
            let frac = (ms / max_ms).min(1.0) as f32;
            let color = if ms > expected_ms * 1.25 {
                egui::Color32::RED
            } else {
                egui::Color32::from_rgb(80, 200, 80)
            };
            let x0 = r.left() + i as f32 * bar_w;
            let x1 = (x0 + bar_w - 1.0).max(x0);
            let y1 = r.bottom();
            let y0 = y1 - frac * r.height();
            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1)), 0.0, color);
        }
        let exp_y = r.bottom() - (expected_ms / max_ms).min(1.0) as f32 * r.height();
        painter.line_segment(
            [egui::pos2(r.left(), exp_y), egui::pos2(r.right(), exp_y)],
            egui::Stroke::new(1.0, egui::Color32::YELLOW),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn system_group(
    ui: &mut egui::Ui,
    sys: &SystemInfo,
    display: &StimulusDisplayInfo,
    wireframe: Option<bool>,
    metrics: &SystemMetrics,
    scene: &Arc<RwLock<SceneState>>,
    wireframe_toggle_requested: &mut bool,
) {
    ui.label(format!("HW: {}", sys.host.hardware_model));
    let mode_suffix = display.mode_index.map(|i| format!("  [mode {i}]")).unwrap_or_default();
    ui.label(format!(
        "Screen: {}×{}@{:.3} Hz{}",
        display.width_px, display.height_px, display.refresh_hz, mode_suffix,
    ));
    ui.label(format!("Host: {}  IP: {}  ZMQ: {}", sys.host.hostname, sys.host.local_ip, sys.host.zmq_port));
    ui.label(format!("Backend: {:?}", sys.backend));
    let (clock_label, clock_color) = match sys.clock_source {
        ClockSource::DrmVblank        => ("Clock: DRM vblank",               egui::Color32::from_rgb(80, 200, 80)),
        ClockSource::VkDisplayControl => ("Clock: VK_EXT_display_control",   egui::Color32::from_rgb(80, 200, 80)),
        ClockSource::PresentWait      => ("Clock: VK_KHR_present_wait",      egui::Color32::from_rgb(80, 200, 80)),
        ClockSource::DisplayTiming    => ("Clock: VK_GOOGLE_display_timing", egui::Color32::YELLOW),
        ClockSource::GpuCompletion    => ("Clock: GPU-completion (inaccurate)", egui::Color32::RED),
    };
    ui.colored_label(clock_color, clock_label);

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("CPU:");
        ui.add(egui::ProgressBar::new(metrics.cpu_pct / 100.0).desired_width(80.0));
        ui.label(format!("{:.0}%  (proc {:.0}%)", metrics.cpu_pct, metrics.process_cpu_pct));
    });
    ui.horizontal(|ui| {
        let (used, total) = (metrics.ram_used_mb, metrics.ram_total_mb);
        let frac = if total > 0 { used as f32 / total as f32 } else { 0.0 };
        ui.label("RAM:");
        ui.add(egui::ProgressBar::new(frac).desired_width(80.0));
        ui.label(format!("{} / {} MB  (proc {} MB)", used, total, metrics.process_rss_mb));
    });
    if let Some(gpu_pct) = metrics.gpu_util_pct {
        ui.horizontal(|ui| {
            ui.label("GPU:");
            ui.add(egui::ProgressBar::new(gpu_pct / 100.0).desired_width(80.0));
            let vram_label = match (metrics.gpu_mem_used_mb, metrics.gpu_mem_total_mb) {
                (Some(used), Some(total)) => format!("{:.0}%  VRAM {}/{} MB", gpu_pct, used, total),
                _ => format!("{:.0}%", gpu_pct),
            };
            ui.label(vram_label);
        });
    }

    ui.separator();
    ui.horizontal(|ui| {
        if let Ok(mut sc) = scene.try_write() {
            let pd = sc.photodiode.enabled;
            if ui.button(if pd { "Photodiode: ON" } else { "Photodiode: off" }).clicked() {
                sc.photodiode.enabled = !sc.photodiode.enabled;
                sc.photodiode.flicker = true;
                sc.photodiode.lit = false;
            }
        }
        if let Some(wf) = wireframe {
            if ui.button(if wf { "Wireframe: ON" } else { "Wireframe: off" }).clicked() {
                *wireframe_toggle_requested = true;
            }
        } else {
            ui.add_enabled(false, egui::Button::new("Wireframe: n/a"));
        }
    });
}

fn vtl_group(ctx: &egui::Context, ui: &mut egui::Ui, want_focus: bool, vtl: Option<&Mutex<VtlState>>) {
    let vtl_guard = vtl.and_then(|v| v.try_lock().ok());
    let Some(ref vtl_st) = vtl_guard else {
        ui.label(egui::RichText::new("VTL not available").color(egui::Color32::DARK_GRAY));
        return;
    };
    let owner = vtl_st.owner();
    let inputs:  Vec<_> = vtl_st.names.iter().filter(|e| e.direction == vtl::Direction::Input).collect();
    let outputs: Vec<_> = vtl_st.names.iter().filter(|e| e.direction == vtl::Direction::Output).collect();

    // --- Bank view (integer representation) ---
    let fmt_id = egui::Id::new("vtl_bank_fmt");
    let mut fmt: BankFmt = ctx.data(|d| d.get_temp(fmt_id)).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Banks").strong());
        ui.separator();
        ui.selectable_value(&mut fmt, BankFmt::Dec, "Dec");
        ui.selectable_value(&mut fmt, BankFmt::Hex, "Hex");
        ui.selectable_value(&mut fmt, BankFmt::Bin, "Bin");
    });
    ctx.data_mut(|d| d.insert_temp(fmt_id, fmt));

    let fmt_val = |val: u64| -> String {
        match fmt {
            BankFmt::Dec => format!("{}", val),
            BankFmt::Hex => format!("0x{:016X}", val),
            BankFmt::Bin => {
                let s = format!("{:064b}", val);
                s.as_bytes().chunks(8).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(" ")
            }
        }
    };

    let n_in  = owner.num_input_banks()  as usize;
    let n_out = owner.num_output_banks() as usize;
    egui::Grid::new("vtl_bank_grid").num_columns(3).spacing([8.0, 2.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("Dir").strong());
        ui.label(egui::RichText::new("Bank").strong());
        ui.label(egui::RichText::new("Value").strong());
        ui.end_row();
        for b in 0..n_in {
            ui.label("In");
            ui.label(format!("{}", b));
            ui.label(egui::RichText::new(fmt_val(owner.input_state(b))).monospace());
            ui.end_row();
        }
        for b in 0..n_out {
            ui.label("Out");
            ui.label(format!("{}", b));
            ui.label(egui::RichText::new(fmt_val(owner.output_state(b))).monospace());
            ui.end_row();
        }
    });
    ui.separator();

    let dot = |ui: &mut egui::Ui, high: bool| {
        let color = if high { egui::Color32::from_rgb(80, 200, 80) } else { egui::Color32::DARK_GRAY };
        let (resp, painter) = ui.allocate_painter(egui::vec2(12.0, 12.0), egui::Sense::hover());
        painter.circle_filled(resp.rect.center(), 5.0, color);
    };

    if vtl_st.names.is_empty() {
        ui.label(egui::RichText::new("(no named lines)").color(egui::Color32::DARK_GRAY));
    }

    if !inputs.is_empty() {
        ui.label(egui::RichText::new("Inputs — Tab to a line, Enter/Space to fire").strong());
        egui::Grid::new("vtl_input_grid").striped(true).num_columns(5).spacing([8.0, 2.0]).show(ui, |ui| {
            ui.label(egui::RichText::new("Name").strong());
            ui.label(egui::RichText::new("Bank/Bit").strong());
            ui.label(egui::RichText::new("Level").strong());
            ui.label(egui::RichText::new("Rise/Fall").strong());
            ui.label(egui::RichText::new("Fire").strong());
            ui.end_row();
            for (i, e) in inputs.iter().enumerate() {
                let b = e.bank as usize;
                let mask = 1u64 << e.bit;
                let high  = owner.input_state(b) & mask != 0;
                let rise  = owner.peek_input_rise(b) & mask != 0;
                let fall  = owner.peek_input_fall(b) & mask != 0;
                ui.label(&e.name);
                ui.label(format!("{}/{}", e.bank, e.bit));
                dot(ui, high);
                ui.label(format!("{}/{}", rise as u8, fall as u8));
                ui.horizontal(|ui| {
                    let up = ui.button("↑").on_hover_text("Fire rising edge");
                    if want_focus && i == 0 {
                        up.request_focus();
                    }
                    if up.clicked() {
                        owner.set_input_bit(b, e.bit);
                        owner.set_input_rise(b, mask);
                    }
                    if ui.button("↓").on_hover_text("Fire falling edge").clicked() {
                        owner.clear_input_bit(b, e.bit);
                        owner.set_input_fall(b, mask);
                    }
                });
                ui.end_row();
            }
        });
        ui.add_space(4.0);
    }

    if !outputs.is_empty() {
        ui.label(egui::RichText::new("Outputs").strong());
        egui::Grid::new("vtl_output_grid").striped(true).num_columns(3).spacing([8.0, 2.0]).show(ui, |ui| {
            ui.label(egui::RichText::new("Name").strong());
            ui.label(egui::RichText::new("Bank/Bit").strong());
            ui.label(egui::RichText::new("Level").strong());
            ui.end_row();
            for e in &outputs {
                let b = e.bank as usize;
                let mask = 1u64 << e.bit;
                let high = owner.output_state(b) & mask != 0;
                ui.label(&e.name);
                ui.label(format!("{}/{}", e.bank, e.bit));
                dot(ui, high);
                ui.end_row();
            }
        });
    }
}
