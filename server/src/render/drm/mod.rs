mod display_guard;
mod init;
mod input;

use std::sync::{Arc, RwLock};

use crate::render::vk::{
    EguiFrameData, GpuBuffers, VkContext, VkEguiRenderer, VkPipeline, render_frame,
};
use crate::scene::SceneState;
use crate::timing::FrameStats;

use self::display_guard::DisplayGuard;
use self::input::{AppKey, InputState};

/// Bare-metal Linux render state — drives the display directly via
/// `VK_KHR_display` without a compositor.
///
/// Fields are declared in logical drop order (first declared = first dropped).
/// `display_guard` is last so CRTC restore fires after Vulkan tears down.
pub struct DrmRenderState {
    ctx: VkContext,
    pipeline: VkPipeline,
    gpu_buffers: GpuBuffers,
    egui_renderer: VkEguiRenderer,
    egui_ctx: egui::Context,
    input: InputState,
    scene: Arc<RwLock<SceneState>>,
    frame_stats: FrameStats,
    show_overlay: bool,
    /// display_guard and vt_guard are Option<_> so they can survive the
    /// DrmRenderState and be dropped in the correct order.  The compiler
    /// warns "never read" but they are consumed by their Drop impls.
    #[allow(dead_code)]
    display_guard: Option<DisplayGuard>,
}

impl DrmRenderState {
    pub fn new(scene: Arc<RwLock<SceneState>>) -> Self {
        // Snapshot display state before Vulkan takes DRM master.
        let display_guard = DisplayGuard::acquire();

        // Initialise Vulkan — VK_KHR_display acquires DRM master internally.
        let (ctx, _display_info) = init::init();
        let pipeline = VkPipeline::new(&ctx.device, ctx.render_pass);
        let gpu_buffers = GpuBuffers::new(&ctx.instance, ctx.physical_device);
        let egui_renderer = VkEguiRenderer::new(
            &ctx.device,
            &ctx.instance,
            ctx.physical_device,
            ctx.egui_render_pass,
        );
        let egui_ctx = egui::Context::default();
        let input = InputState::new();

        Self {
            ctx,
            pipeline,
            gpu_buffers,
            egui_renderer,
            egui_ctx,
            input,
            scene,
            frame_stats: FrameStats::new(60.0),
            show_overlay: false,
            display_guard,
        }
    }

    pub fn run_loop(mut self) {
        let mut frame_index: usize = 0;
        loop {
            for key in self.input.poll() {
                match key {
                    AppKey::Escape => return,
                    AppKey::D => crate::render::spawn_demo_stimuli(&self.scene),
                    AppKey::F1 => self.show_overlay = !self.show_overlay,
                }
            }

            // Build egui overlay if enabled (keyboard-only interaction)
            let (egui_renderer, egui_data) = if self.show_overlay {
                let raw_input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(self.ctx.extent.width as f32, self.ctx.extent.height as f32),
                    )),
                    pixels_per_point: Some(1.0), // TODO: compute from EDID DPI or make configurable
                    ..Default::default()
                };
                let output = self.egui_ctx.run_ui(raw_input, |ctx| {
                    build_overlay_ui(ctx, &self.scene, &self.frame_stats);
                });

                // Tessellate egui output
                let primitives = self
                    .egui_ctx
                    .tessellate(output.shapes, output.pixels_per_point);

                let data = EguiFrameData {
                    textures_delta: &output.textures_delta,
                    primitives: &primitives,
                    pixels_per_point: output.pixels_per_point,
                };
                (Some(&mut self.egui_renderer), Some(data))
            } else {
                (None, None)
            };

            // `None` means the swapchain is out of date (rare in DRM mode).
            let _tick = render_frame(
                &self.ctx,
                &self.pipeline,
                &mut self.gpu_buffers,
                &self.scene,
                &mut frame_index,
                &mut self.frame_stats,
                egui_renderer,
                egui_data,
            );
        }
        // When the loop exits (consuming `self`), fields drop in declaration
        // order: ctx → pipeline → gpu_buffers → input → scene → frame_stats
        // → display_guard.  The CRTC restore in DisplayGuard::drop() therefore
        // fires after Vulkan has already released DRM master.
    }
}

/// Build egui overlay UI (shared between winit and DRM backends)
fn build_overlay_ui(
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
