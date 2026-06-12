use std::sync::{Arc, Mutex, RwLock};

use crate::log_buffer::LogBuffer;
use crate::render::BenchmarkState;
use crate::render::FileBrowser;
use crate::render::MetricsSampler;
use crate::render::overlay::{OverlayArgs, build_overlay_ui};
use crate::render::system_info::SystemInfo;
use crate::render::vk::{
    EguiFrameData, GlyphAtlas, SceneCache, VkContext,
    VkEguiRenderer, VkGratingPipeline, VkPipeline, VkTextPipeline, render_frame,
};
use crate::scene::stimulus::text::{TextFontSystem, TextSwashCache};
use crate::scene::SceneState;
use crate::timing::{FramePhases, FrameStats, FrameTick};
use crate::vtl_state::VtlState;

/// Vulkan rendering resources and per-frame bookkeeping shared between the DRM
/// and winit backends.
///
/// Backend-specific state (input devices, window handle, vblank clock source,
/// display geometry) lives in the embedding backend struct alongside this.
///
/// Drop order: `drop()` explicitly frees all GPU resources while `ctx` is still
/// alive, then Rust drops all fields in declaration order.
pub struct RenderState {
    pub ctx: VkContext,
    pub pipeline: VkPipeline,
    pub grating_pipeline: VkGratingPipeline,
    pub wireframe_pipeline: VkPipeline,
    pub wireframe_grating: VkGratingPipeline,
    pub wireframe: bool,
    pub scene_cache: SceneCache,
    pub glyph_atlas: GlyphAtlas,
    pub text_pipeline: VkTextPipeline,
    pub font_system: TextFontSystem,
    pub swash_cache: TextSwashCache,
    pub egui_renderer: VkEguiRenderer,
    pub egui_ctx: egui::Context,
    pub scene: Arc<RwLock<SceneState>>,
    pub frame_stats: FrameStats,
    pub last_phases: FramePhases,
    pub frame_index: usize,
    pub show_overlay: bool,
    pub benchmark: BenchmarkState,
    pub local_ip: String,
    pub log_buffer: LogBuffer,
    pub metrics: MetricsSampler,
    pub file_browser: FileBrowser,
}

impl Drop for RenderState {
    fn drop(&mut self) {
        self.egui_renderer.destroy(&self.ctx.device);
        self.text_pipeline.destroy(&self.ctx.device);
        unsafe { self.glyph_atlas.destroy(&self.ctx.device) };
        self.scene_cache.destroy_all(&self.ctx.device);
        self.wireframe_grating.destroy(&self.ctx.device);
        self.wireframe_pipeline.destroy(&self.ctx.device);
        self.grating_pipeline.destroy(&self.ctx.device);
        self.pipeline.destroy(&self.ctx.device);
    }
}

impl RenderState {
    /// One frame step: optionally build the egui overlay, then tessellate the
    /// scene, record and submit the Vulkan command buffer, and present.
    ///
    /// The sequence inside this call is:
    ///   1. Build egui overlay UI (if `egui_raw_input` is `Some`)
    ///   2. Wait for the previous frame's fence
    ///   3. Establish the screen clock (`screen_clock`, present-wait, or GPU time)
    ///   4. Tessellate dirty stimuli into GPU buffers
    ///   5. Acquire the next swapchain image (blocks on FIFO vblank in winit mode)
    ///   6. Record render commands (solid pass → grating pass → egui pass)
    ///   7. Submit to the GPU queue
    ///   8. Present to the display
    ///
    /// `screen_clock` — vblank-anchored timestamp from the DRM backend's
    /// `wait_vblank` ioctl.  Pass `None` for the winit backend; `render_frame`
    /// then derives the clock from `VK_KHR_present_wait` or GPU-completion time.
    ///
    /// `egui_raw_input` — platform input for the overlay.  The DRM backend
    /// constructs a minimal `RawInput` from screen geometry and libinput
    /// navigation keys; the winit backend converts winit events via `egui_winit`.
    /// Pass `None` (or when `show_overlay` is `false`) to skip egui entirely.
    ///
    /// Returns `(tick, platform_output)`:
    /// - `tick` is `None` when the swapchain is out of date — the caller must
    ///   recreate it before the next call.
    /// - `platform_output` carries cursor/clipboard side-effects from egui.
    ///   The winit backend forwards it to `egui_winit`; DRM ignores it.
    pub fn render_one_frame(
        &mut self,
        screen_clock: Option<std::time::Instant>,
        egui_raw_input: Option<egui::RawInput>,
        sys_info: &SystemInfo,
        vtl: Option<&Mutex<VtlState>>,
    ) -> (Option<FrameTick>, Option<egui::PlatformOutput>) {
        let (pipe, grate) = if self.wireframe {
            (&self.wireframe_pipeline, &self.wireframe_grating)
        } else {
            (&self.pipeline, &self.grating_pipeline)
        };

        // Build egui overlay output when the overlay is visible and input is
        // available.  Stored as a local so that references into it that are
        // passed to `EguiFrameData` outlive the `render_frame` call.
        let mut egui_store: Option<(egui::TexturesDelta, Vec<egui::ClippedPrimitive>, f32)> = None;
        let mut platform_output: Option<egui::PlatformOutput> = None;

        if self.show_overlay
            && let Some(raw_input) = egui_raw_input
        {
            let phases = self.last_phases;
            let metrics = self.metrics.sample();
            let output = self.egui_ctx.run_ui(raw_input, |ctx| {
                build_overlay_ui(ctx, &mut OverlayArgs {
                    scene: &self.scene,
                    vtl,
                    frame_stats: &mut self.frame_stats,
                    last_phases: phases,
                    sys: sys_info,
                    metrics,
                    log_buffer: &self.log_buffer,
                    bench: &mut self.benchmark,
                    file_browser: &mut self.file_browser,
                });
            });
            platform_output = Some(output.platform_output);
            let ppp = output.pixels_per_point;
            let primitives = self.egui_ctx.tessellate(output.shapes, ppp);
            egui_store = Some((output.textures_delta, primitives, ppp));
        }

        let egui_data = egui_store.as_ref().map(|(td, prims, ppp)| EguiFrameData {
            textures_delta: td,
            primitives: prims,
            pixels_per_point: *ppp,
        });
        let egui_renderer = egui_store.as_ref().map(|_| &mut self.egui_renderer);

        let tick = render_frame(
            &self.ctx,
            pipe,
            grate,
            &self.text_pipeline,
            &mut self.scene_cache,
            &mut self.glyph_atlas,
            &mut self.font_system,
            &mut self.swash_cache,
            &self.scene,
            &mut self.frame_index,
            &mut self.frame_stats,
            egui_renderer,
            egui_data,
            screen_clock,
        );

        if let Some(ref t) = tick {
            self.last_phases = t.phases;
        }

        (tick, platform_output)
    }
}
