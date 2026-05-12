use std::sync::{Arc, RwLock};

use crate::log_buffer::LogBuffer;
use crate::scene::SceneState;
use crate::timing::{FramePhases, FrameStats};

pub use super::system_info::SystemInfo;

pub fn build_overlay_ui(
    ctx: &egui::Context,
    scene: &Arc<RwLock<SceneState>>,
    frame_stats: &FrameStats,
    last_phases: FramePhases,
    sys: &SystemInfo,
    log_buffer: &LogBuffer,
) {
    egui::Window::new("System").show(ctx, |ui| {
        ui.label(format!(
            "Screen: {}×{}@{:.3} Hz",
            sys.display.width_px, sys.display.height_px, sys.display.refresh_hz,
        ));
        ui.label(format!("Host: {}  IP: {}", sys.hostname, sys.local_ip));
        ui.label(format!("Backend: {:?}", sys.backend));
        if let Some(wf) = sys.wireframe {
            ui.label(format!("Wireframe [F3]: {}", if wf { "ON" } else { "off" }));
        }
    });

    egui::Window::new("Frame Timing").show(ctx, |ui| {
        let s = frame_stats.summary();
        ui.label(format!("FPS: {:.1}  drops: {}", s.fps, s.drop_count));
        ui.label(format!("frame: {:.2} ms  jitter: ±{:.2} ms", s.mean_ms, s.std_ms));
        ui.label(format!("min: {:.2} ms  max: {:.2} ms", s.min_ms, s.max_ms));
        ui.separator();
        ui.label("Last frame phases (µs):");
        ui.label(format!(
            "  tess/upload {:>5}  fence {:>5}  acquire {:>5}",
            last_phases.tessellate_us, last_phases.fence_us, last_phases.acquire_us,
        ));
        ui.label(format!(
            "  record      {:>5}  submit {:>5}",
            last_phases.record_us, last_phases.submit_us,
        ));

        // Frame-time sparkline — each bar = one frame, red = missed vblank.
        let durations: Vec<f64> = frame_stats
            .durations_recent_ns()
            .map(|d| d as f64 / 1_000_000.0)
            .collect();
        if !durations.is_empty() {
            let expected_ms = frame_stats.expected_ns() as f64 / 1_000_000.0;
            let max_ms = durations
                .iter()
                .cloned()
                .fold(0.0_f64, f64::max)
                .max(expected_ms * 2.5);

            let desired = egui::vec2(ui.available_width(), 64.0);
            let (resp, painter) =
                ui.allocate_painter(desired, egui::Sense::hover());
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
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1)),
                    0.0,
                    color,
                );
            }
            // Yellow reference line at the expected frame period.
            let exp_y = r.bottom() - (expected_ms / max_ms).min(1.0) as f32 * r.height();
            painter.line_segment(
                [egui::pos2(r.left(), exp_y), egui::pos2(r.right(), exp_y)],
                egui::Stroke::new(1.0, egui::Color32::YELLOW),
            );
        }
    });

    egui::Window::new("Stimuli").show(ctx, |ui| {
        if let Ok(mut sc) = scene.try_write() {
            let handles: Vec<u32> = sc.stimuli.keys().copied().collect();
            for h in handles {
                if let Some(stim) = sc.stimuli.get_mut(&h) {
                    let type_name = stim.type_name();
                    let flags = stim.flags_mut();
                    ui.checkbox(&mut flags.enabled, format!("#{h} {type_name}"));
                }
            }
        }
    });

    egui::Window::new("IPC Log").default_size([500.0, 160.0]).show(ctx, |ui| {
        if let Ok(sc) = scene.try_read() {
            ui.label(format!(
                "total: {}  errors: {}",
                sc.command_log_total, sc.command_log_errors
            ));
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(120.0)
                .show(ui, |ui| {
                    for entry in &sc.command_log {
                        let color = if entry.ok {
                            egui::Color32::from_rgb(80, 200, 80)
                        } else {
                            egui::Color32::RED
                        };
                        ui.colored_label(
                            color,
                            format!(
                                "[{:>8.1}ms] #{} {} → {}",
                                entry.elapsed_ms,
                                entry.handle,
                                entry.summary,
                                if entry.ok {
                                    format!("ok ({})", entry.response)
                                } else {
                                    "err".to_string()
                                },
                            ),
                        );
                    }
                });
        }
    });

    egui::Window::new("Server Log").default_size([600.0, 200.0]).show(ctx, |ui| {
        let entries = log_buffer
            .lock()
            .map(|buf| buf.iter().map(|e| {
                let color = match e.level {
                    log::Level::Error => egui::Color32::RED,
                    log::Level::Warn  => egui::Color32::YELLOW,
                    log::Level::Info  => egui::Color32::WHITE,
                    _                 => egui::Color32::GRAY,
                };
                let text = format!(
                    "[{:>8.1}ms] {:5} {}",
                    e.elapsed_ms,
                    e.level,
                    e.message,
                );
                (color, text)
            }).collect::<Vec<_>>())
            .unwrap_or_default();

        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .max_height(160.0)
            .show(ui, |ui| {
                for (color, text) in entries {
                    ui.colored_label(color, text);
                }
            });
    });
}
