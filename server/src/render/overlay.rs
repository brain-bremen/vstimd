use std::sync::{Arc, RwLock};

use crate::log_buffer::LogBuffer;
use crate::render::SystemMetrics;
use crate::scene::stimulus::ShapeStimulus;
use crate::scene::{SceneState, Stimulus};
use crate::timing::{FramePhases, FrameStats};

pub use super::system_info::{ClockSource, SystemInfo};
use super::benchmark::BenchmarkState;

pub struct OverlayArgs<'a> {
    pub scene: &'a Arc<RwLock<SceneState>>,
    pub frame_stats: &'a mut FrameStats,
    pub last_phases: FramePhases,
    pub sys: &'a SystemInfo,
    pub metrics: &'a SystemMetrics,
    pub log_buffer: &'a LogBuffer,
    pub bench: &'a mut BenchmarkState,
}

pub fn build_overlay_ui(ctx: &egui::Context, args: &mut OverlayArgs<'_>) {
    let OverlayArgs { scene, frame_stats, last_phases, sys, metrics, log_buffer, bench } = args;
    let last_phases = *last_phases;
    egui::Window::new("System").show(ctx, |ui| {
        ui.label(format!(
            "Screen: {}×{}@{:.3} Hz",
            sys.display.width_px, sys.display.height_px, sys.display.refresh_hz,
        ));
        ui.label(format!("Host: {}  IP: {}", sys.hostname, sys.local_ip));
        ui.label(format!("Backend: {:?}", sys.backend));
        let (clock_label, clock_color) = match sys.clock_source {
            ClockSource::DrmVblank     => ("Clock: DRM vblank",                     egui::Color32::from_rgb(80, 200, 80)),
            ClockSource::PresentWait   => ("Clock: VK_KHR_present_wait",            egui::Color32::from_rgb(80, 200, 80)),
            ClockSource::DisplayTiming => ("Clock: VK_GOOGLE_display_timing",       egui::Color32::YELLOW),
            ClockSource::GpuCompletion => ("Clock: GPU-completion (inaccurate)",    egui::Color32::RED),
        };
        ui.colored_label(clock_color, clock_label);
        if let Some(wf) = sys.wireframe {
            ui.label(format!("Wireframe [F3]: {}", if wf { "ON" } else { "off" }));
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("CPU:");
            ui.add(egui::ProgressBar::new(metrics.cpu_pct / 100.0).desired_width(80.0));
            ui.label(format!(
                "{:.0}%  (proc {:.0}%)",
                metrics.cpu_pct, metrics.process_cpu_pct,
            ));
        });
        ui.horizontal(|ui| {
            let used = metrics.ram_used_mb;
            let total = metrics.ram_total_mb;
            let frac = if total > 0 { used as f32 / total as f32 } else { 0.0 };
            ui.label("RAM:");
            ui.add(egui::ProgressBar::new(frac).desired_width(80.0));
            ui.label(format!(
                "{} / {} MB  (proc {} MB)",
                used, total, metrics.process_rss_mb,
            ));
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

    egui::Window::new("Stimuli").default_size([420.0, 200.0]).show(ctx, |ui| {
        if let Ok(mut sc) = scene.try_write() {
            let handles: Vec<u32> = sc.stimuli.keys().copied().collect();
            egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                egui::Grid::new("stimuli_grid")
                    .striped(true)
                    .num_columns(5)
                    .spacing([8.0, 2.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("En").strong());
                        ui.label(egui::RichText::new("Handle / type").strong());
                        ui.label(egui::RichText::new("Name").strong());
                        ui.label(egui::RichText::new("Position (px)").strong());
                        ui.label(egui::RichText::new("Size (px)").strong());
                        ui.end_row();
                        for h in handles {
                            if let Some(entry) = sc.stimuli.get_mut(&h) {
                                let stim = &entry.stimulus;
                                let type_name = stim.type_name();
                                let pos = stim.transform().live.pos;
                                let size_label = match stim {
                                    Stimulus::Grating(s) => {
                                        let [hw, hh] = s.size.live;
                                        format!("{}×{}", (hw * 2.0) as i32, (hh * 2.0) as i32)
                                    }
                                    Stimulus::Shape(ShapeStimulus::Rect(s)) => {
                                        let [hw, hh] = s.size.live;
                                        format!("{}×{}", (hw * 2.0) as i32, (hh * 2.0) as i32)
                                    }
                                    Stimulus::Shape(ShapeStimulus::Circle(s)) => {
                                        format!("r={}", s.radius.live as i32)
                                    }
                                    Stimulus::Shape(ShapeStimulus::Ellipse(s)) => {
                                        let [rx, ry] = s.radii.live;
                                        format!("{}×{}", (rx * 2.0) as i32, (ry * 2.0) as i32)
                                    }
                                };
                                let name_label = entry.name.as_deref().unwrap_or("");
                                let uuid_str = entry.id.to_string();
                                let flags = entry.stimulus.flags_mut();
                                ui.checkbox(&mut flags.enabled, "");
                                ui.label(format!("#{h} {type_name}"));
                                let display = if name_label.is_empty() {
                                    &uuid_str[..8]
                                } else {
                                    name_label
                                };
                                ui.label(
                                    egui::RichText::new(display)
                                        .color(if name_label.is_empty() {
                                            egui::Color32::DARK_GRAY
                                        } else {
                                            egui::Color32::WHITE
                                        })
                                ).on_hover_text(&uuid_str);
                                ui.label(format!("{:.0},{:.0}", pos[0], pos[1]));
                                ui.label(size_label);
                                ui.end_row();
                            }
                        }
                    });
            });
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

    // Tick the benchmark every frame so it can detect completion.
    bench.tick(scene, frame_stats);

    egui::Window::new("Benchmarks").show(ctx, |ui| {
        ui.heading("Grating stress test");
        if bench.is_running() {
            let remaining = bench.remaining_frames(frame_stats).unwrap_or(0);
            ui.label(format!("Running… {remaining} frames remaining"));
        } else {
            // 20 × 10 = 200 gratings, 300 frames (~5 s at 60 Hz)
            if ui.button("Run (200 gratings, 300 frames)").clicked() {
                bench.start_grating_stress(scene, frame_stats, (sys.display.width_px, sys.display.height_px), 20, 10, 300);
            }
            if let Some(r) = bench.last_result() {
                ui.separator();
                ui.label(format!(
                    "{} gratings × {} frames → {} dropped",
                    r.grating_count, r.duration_frames, r.drop_count,
                ));
            }
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
