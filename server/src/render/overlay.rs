use std::sync::{Arc, RwLock};

use crate::scene::SceneState;
use crate::timing::FrameStats;

pub fn build_overlay_ui(
    ctx: &egui::Context,
    scene: &Arc<RwLock<SceneState>>,
    frame_stats: &FrameStats,
) {
    egui::Window::new("Frame Timing").show(ctx, |ui| {
        let s = frame_stats.summary();
        ui.label(format!("FPS: {:.1}  drops: {}", s.fps, s.drop_count));
        ui.label(format!("frame: {:.2} ms  jitter: ±{:.2} ms", s.mean_ms, s.std_ms));
        ui.label(format!("min: {:.2} ms  max: {:.2} ms", s.min_ms, s.max_ms));

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
}
