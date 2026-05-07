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
        ui.label(format!("FPS: {:.1}", s.fps));
        ui.label(format!("frame: {:.2} ms", s.mean_ms));
        ui.label(format!("jitter: {:.2} ms", s.std_ms));
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
