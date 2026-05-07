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
use crate::render::overlay::build_overlay_ui;
use crate::timing::FramePhases;

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
    last_phases: FramePhases,
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
        let (ctx, display_info) = init::init();
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
            frame_stats: FrameStats::new(display_info.refresh_mhz as f64 / 1000.0),
            last_phases: FramePhases::default(),
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
                    AppKey::F2 => {
                        let mut sc = self.scene.write().expect("scene lock");
                        sc.photodiode.enabled = !sc.photodiode.enabled;
                        sc.photodiode.flicker = true;
                        sc.photodiode.lit = false;
                    }
                }
            }

            // Build egui overlay if enabled (keyboard-only interaction).
            // Stored outside the `if` so the borrows in `EguiFrameData` live
            // long enough to reach `render_frame`.
            let mut egui_output_store: Option<(
                egui::epaint::textures::TexturesDelta,
                Vec<egui::ClippedPrimitive>,
                f32,
            )> = None;
            if self.show_overlay {
                let raw_input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(self.ctx.extent.width as f32, self.ctx.extent.height as f32),
                    )),
                    viewports: std::iter::once((
                        egui::ViewportId::ROOT,
                        egui::ViewportInfo {
                            native_pixels_per_point: Some(1.0), // TODO: compute from EDID DPI or make configurable
                            ..Default::default()
                        },
                    ))
                    .collect(),
                    ..Default::default()
                };
                let phases = self.last_phases;
                let output = self.egui_ctx.run_ui(raw_input, |ctx| {
                    build_overlay_ui(ctx, &self.scene, &self.frame_stats, phases);
                });
                let ppp = output.pixels_per_point;
                let textures_delta = output.textures_delta;
                let primitives = self.egui_ctx.tessellate(output.shapes, ppp);
                egui_output_store = Some((textures_delta, primitives, ppp));
            }
            let (egui_renderer, egui_data) =
                if let Some((textures_delta, primitives, ppp)) = egui_output_store.as_ref() {
                    let data = EguiFrameData {
                        textures_delta,
                        primitives,
                        pixels_per_point: *ppp,
                    };
                    (Some(&mut self.egui_renderer), Some(data))
                } else {
                    (None, None)
                };

            // `None` means the swapchain is out of date (rare in DRM mode).
            if let Some(t) = render_frame(
                &self.ctx,
                &self.pipeline,
                &mut self.gpu_buffers,
                &self.scene,
                &mut frame_index,
                &mut self.frame_stats,
                egui_renderer,
                egui_data,
            ) {
                self.last_phases = t.phases;
            }
        }
        // When the loop exits (consuming `self`), fields drop in declaration
        // order: ctx → pipeline → gpu_buffers → input → scene → frame_stats
        // → display_guard.  The CRTC restore in DisplayGuard::drop() therefore
        // fires after Vulkan has already released DRM master.
    }
}

