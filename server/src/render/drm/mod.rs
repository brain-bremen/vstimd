mod display_guard;
mod init;
mod input;

use std::sync::{Arc, RwLock};

use crate::log_buffer::LogBuffer;
use crate::render::vk::{
    EguiFrameData, GpuBuffers, VkContext, VkEguiRenderer, VkGratingPipeline, VkPipeline,
    render_frame,
};
use crate::scene::SceneState;
use crate::timing::FrameStats;

use self::display_guard::DisplayGuard;
use self::input::{AppKey, InputState};
use crate::render::overlay::build_overlay_ui;
use crate::render::{RenderTarget, StimulusDisplayInfo, SystemInfo, query_local_ip};
use crate::timing::FramePhases;

/// Bare-metal Linux render state — drives the display directly via
/// `VK_KHR_display` without a compositor.
///
/// Fields are declared in logical drop order (first declared = first dropped).
/// `display_guard` is last so CRTC restore fires after Vulkan tears down.
pub struct DrmRenderState {
    ctx: VkContext,
    pipeline: VkPipeline,
    grating_pipeline: VkGratingPipeline,
    wireframe_pipeline: VkPipeline,
    wireframe_grating: VkGratingPipeline,
    wireframe: bool,
    gpu_buffers: GpuBuffers,
    egui_renderer: VkEguiRenderer,
    egui_ctx: egui::Context,
    input: InputState,
    scene: Arc<RwLock<SceneState>>,
    frame_stats: FrameStats,
    last_phases: FramePhases,
    show_overlay: bool,
    display_info: StimulusDisplayInfo,
    local_ip: String,
    log_buffer: LogBuffer,
    /// display_guard and vt_guard are Option<_> so they can survive the
    /// DrmRenderState and be dropped in the correct order.  The compiler
    /// warns "never read" but they are consumed by their Drop impls.
    #[allow(dead_code)]
    display_guard: Option<DisplayGuard>,
}

impl Drop for DrmRenderState {
    fn drop(&mut self) {
        self.egui_renderer.destroy(&self.ctx.device);
        self.gpu_buffers.destroy_all(&self.ctx.device);
        self.wireframe_grating.destroy(&self.ctx.device);
        self.wireframe_pipeline.destroy(&self.ctx.device);
        self.grating_pipeline.destroy(&self.ctx.device);
        self.pipeline.destroy(&self.ctx.device);
    }
}

fn check_device_permissions() {
    let mut missing: Vec<String> = Vec::new();

    // DRM node: need read+write on at least one card
    let drm_ok = (0..8u32).any(|n| {
        let path = format!("/dev/dri/card{n}\0");
        unsafe { libc::access(path.as_ptr() as *const libc::c_char, libc::R_OK | libc::W_OK) == 0 }
    });
    if !drm_ok {
        missing.push(
            "  /dev/dri/card* — add user to 'video' group:\n    sudo usermod -aG video $USER"
                .to_string(),
        );
    }

    // Input devices: need read on at least one event node
    let input_ok = std::fs::read_dir("/dev/input")
        .ok()
        .map(|dir| {
            dir.filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("event"))
                .any(|e| {
                    let path = e.path();
                    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                    unsafe { libc::access(c_path.as_ptr(), libc::R_OK) == 0 }
                })
        })
        .unwrap_or(false);
    if !input_ok {
        missing.push(
            "  /dev/input/event* — add user to 'input' group:\n    sudo usermod -aG input $USER"
                .to_string(),
        );
    }

    if !missing.is_empty() {
        log::error!(
            "vstimd: missing device permissions — log out and back in after fixing:\n{}",
            missing.join("\n")
        );
        std::process::exit(1);
    }
}

impl DrmRenderState {
    pub fn new(scene: Arc<RwLock<SceneState>>, log_buffer: LogBuffer) -> Self {
        check_device_permissions();

        // Snapshot display state before Vulkan takes DRM master.
        let display_guard = DisplayGuard::acquire();

        // Initialise Vulkan — VK_KHR_display acquires DRM master internally.
        let (ctx, display_info) = init::init();
        let wf_mode = if ctx.supports_wireframe {
            ash::vk::PolygonMode::LINE
        } else {
            ash::vk::PolygonMode::FILL
        };
        let pipeline = VkPipeline::new(&ctx.device, ctx.render_pass, ash::vk::PolygonMode::FILL);
        let grating_pipeline = VkGratingPipeline::new(&ctx.device, ctx.render_pass, ash::vk::PolygonMode::FILL);
        let wireframe_pipeline = VkPipeline::new(&ctx.device, ctx.render_pass, wf_mode);
        let wireframe_grating = VkGratingPipeline::new(&ctx.device, ctx.render_pass, wf_mode);

        ctx.set_debug_name(pipeline.pipeline, "solid_pipeline");
        ctx.set_debug_name(grating_pipeline.pipeline, "grating_pipeline");
        ctx.set_debug_name(wireframe_pipeline.pipeline, "solid_wireframe_pipeline");
        ctx.set_debug_name(wireframe_grating.pipeline, "grating_wireframe_pipeline");
        ctx.set_debug_name(ctx.render_pass, "render_pass");
        ctx.set_debug_name(ctx.egui_render_pass, "egui_render_pass");
        for (i, frame) in ctx.frames.iter().enumerate() {
            ctx.set_debug_name(frame.command_buffer, &format!("frame[{i}]_cmd"));
        }
        for (i, img) in ctx.swapchain_images.iter().enumerate() {
            ctx.set_debug_name(*img, &format!("swapchain[{i}]"));
        }

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
            grating_pipeline,
            wireframe_pipeline,
            wireframe_grating,
            wireframe: false,
            gpu_buffers,
            egui_renderer,
            egui_ctx,
            input,
            scene,
            frame_stats: FrameStats::new(display_info.refresh_hz),
            last_phases: FramePhases::default(),
            show_overlay: false,
            display_info,
            local_ip: query_local_ip(),
            log_buffer,
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
                    AppKey::F3 => {
                        if self.ctx.supports_wireframe {
                            self.wireframe = !self.wireframe;
                            log::info!(
                                "vstimd: wireframe {}",
                                if self.wireframe { "ON" } else { "OFF" }
                            );
                        }
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
                let sys = SystemInfo {
                    display: self.display_info.clone(),
                    backend: RenderTarget::Drm,
                    local_ip: self.local_ip.clone(),
                    hostname: String::new(),
                    gpu_name: String::new(),
                    wireframe: None,
                };
                let output = self.egui_ctx.run_ui(raw_input, |ctx| {
                    build_overlay_ui(ctx, &self.scene, &self.frame_stats, phases, &sys, &self.log_buffer);
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
            let (pipe, grate) = if self.wireframe {
                (&self.wireframe_pipeline, &self.wireframe_grating)
            } else {
                (&self.pipeline, &self.grating_pipeline)
            };
            if let Some(t) = render_frame(
                &self.ctx,
                pipe,
                grate,
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
