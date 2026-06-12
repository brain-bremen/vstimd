mod display_guard;
mod init;
mod input;
mod vblank;

use std::sync::{Arc, Mutex, RwLock};

use crate::log_buffer::LogBuffer;
use crate::render::BenchmarkState;
use crate::render::MetricsSampler;
use crate::render::RenderState;
use crate::render::system_info::ClockSource;
use crate::render::vk::{GlyphAtlas, SceneCache, VkEguiRenderer, VkGratingPipeline, VkPipeline, VkTextPipeline};
use crate::scene::stimulus::text::{TextFontSystem, TextSwashCache};
use crate::render::{FileBrowser, RenderTarget, StimulusDisplayInfo, SystemInfo, query_local_ip};
use crate::scene::SceneState;
use crate::timing::{FramePhases, FrameStats};
use crate::vtl_state::VtlState;
extern crate vtl;

use self::display_guard::DisplayGuard;
use self::input::{AppKey, InputState};
use self::vblank::DrmVblank;

/// Bare-metal Linux render state — drives the display directly via
/// `VK_KHR_display` without a compositor.
///
/// Fields drop in declaration order: `rs` (Vulkan resources) before
/// `display_guard` (CRTC restore), so the kernel display is restored only
/// after Vulkan has released DRM master.
pub struct DrmRenderState {
    rs: RenderState,
    vtl: Option<Arc<Mutex<VtlState>>>,
    input: InputState,
    drm_vblank: Option<DrmVblank>,
    display_info: StimulusDisplayInfo,
    /// Animation output bits accumulated this frame; committed at [A] next frame.
    pending_outputs: [u64; vtl::MAX_BANKS],
    /// Holds the CRTC snapshot; dropped last to restore the console after
    /// Vulkan teardown.  `#[allow(dead_code)]` silences the "never read"
    /// warning — the value is consumed by its `Drop` impl.
    #[allow(dead_code)]
    display_guard: Option<DisplayGuard>,
}

fn check_device_permissions() {
    let mut missing: Vec<String> = Vec::new();

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

    let input_ok = unsafe {
        // Look up the GID of the "input" group.
        let grp = libc::getgrnam(c"input".as_ptr());
        if grp.is_null() {
            // No "input" group on this system — skip the check.
            true
        } else {
            let input_gid = (*grp).gr_gid;
            // Check effective GID and supplementary groups.
            if libc::getegid() == input_gid {
                true
            } else {
                let mut groups = vec![0u32; 64];
                let n = libc::getgroups(groups.len() as libc::c_int, groups.as_mut_ptr());
                n > 0 && groups[..n as usize].contains(&input_gid)
            }
        }
    };
    if !input_ok {
        missing.push(
            "  not in 'input' group — add user and log out/in:\n    sudo usermod -aG input $USER"
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
    pub fn new(scene: Arc<RwLock<SceneState>>, vtl: Option<Arc<Mutex<VtlState>>>, log_buffer: LogBuffer) -> Self {
        check_device_permissions();

        // Snapshot display state before Vulkan takes DRM master.
        let display_guard = DisplayGuard::acquire();

        // Open DRM vblank BEFORE Vulkan init. After VK_KHR_display takes DRM
        // master the KMS CRTC state changes and get_crtc().mode() returns None,
        // so we must query it while the kernel still shows the CRTC as active.
        // The fd stays valid for wait_vblank throughout the session (no master
        // required for that ioctl).
        let drm_vblank = DrmVblank::open();

        // Initialise Vulkan — VK_KHR_display acquires DRM master internally.
        let (ctx, display_info) = init::init();
        let wf_mode = if ctx.supports_wireframe {
            ash::vk::PolygonMode::LINE
        } else {
            ash::vk::PolygonMode::FILL
        };
        let pipeline = VkPipeline::new(&ctx.device, ctx.render_pass, ash::vk::PolygonMode::FILL);
        let grating_pipeline =
            VkGratingPipeline::new(&ctx.device, &ctx.instance, ctx.physical_device, ctx.render_pass, ash::vk::PolygonMode::FILL);
        let wireframe_pipeline = VkPipeline::new(&ctx.device, ctx.render_pass, wf_mode);
        let wireframe_grating = VkGratingPipeline::new(&ctx.device, &ctx.instance, ctx.physical_device, ctx.render_pass, wf_mode);

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

        let egui_renderer = VkEguiRenderer::new(
            &ctx.device,
            &ctx.instance,
            ctx.physical_device,
            ctx.egui_render_pass,
        );

        let scene_cache = SceneCache::new(&ctx.instance, ctx.physical_device);
        let glyph_atlas = GlyphAtlas::new(&ctx.device, &ctx.instance, ctx.physical_device);
        let text_pipeline = VkTextPipeline::new(
            &ctx.device,
            ctx.render_pass,
            glyph_atlas.descriptor_set_layout,
        );
        let config_dir = scene.read().unwrap().runtime.config_dir.clone();
        let rs = RenderState {
            frame_stats: FrameStats::new(display_info.refresh_hz),
            ctx,
            pipeline,
            grating_pipeline,
            wireframe_pipeline,
            wireframe_grating,
            wireframe: false,
            scene_cache,
            glyph_atlas,
            text_pipeline,
            font_system: TextFontSystem::new(),
            swash_cache: TextSwashCache::new(),
            egui_renderer,
            egui_ctx: egui::Context::default(),
            scene,
            last_phases: FramePhases::default(),
            frame_index: 0,
            show_overlay: false,
            benchmark: BenchmarkState::new(),
            local_ip: query_local_ip(),
            log_buffer,
            metrics: MetricsSampler::new(),
            file_browser: FileBrowser::new(config_dir),
        };

        Self {
            rs,
            vtl,
            input: InputState::new(),
            drm_vblank,
            display_info,
            pending_outputs: [0; vtl::MAX_BANKS],
            display_guard,
        }
    }

    fn sys_info(&self) -> SystemInfo {
        SystemInfo {
            display: self.display_info.clone(),
            backend: RenderTarget::Drm,
            local_ip: self.rs.local_ip.clone(),
            hostname: String::new(),
            gpu_name: String::new(),
            wireframe: None,
            clock_source: if self.drm_vblank.is_some() {
                ClockSource::DrmVblank
            } else if self.rs.ctx.present_wait.is_some() {
                ClockSource::PresentWait
            } else {
                ClockSource::GpuCompletion
            },
        }
    }

    fn build_egui_raw_input(&self, nav_events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(
                    self.rs.ctx.extent.width as f32,
                    self.rs.ctx.extent.height as f32,
                ),
            )),
            viewports: std::iter::once((
                egui::ViewportId::ROOT,
                egui::ViewportInfo {
                    native_pixels_per_point: Some(1.0), // TODO: compute from EDID DPI or make configurable
                    ..Default::default()
                },
            ))
            .collect(),
            events: nav_events,
            ..Default::default()
        }
    }

    fn wait_vblank(&mut self) -> Option<std::time::Instant> {
        let vblank = self.drm_vblank.as_ref()?;
        match vblank.wait() {
            Some(t) => Some(t),
            None => {
                log::warn!(
                    "vstimd: disabling DRM vblank clock after wait_vblank error"
                );
                // One-way fallback: stop issuing the ioctl and use
                // GPU-completion timestamps instead.
                self.drm_vblank = None;
                None
            }
        }
    }

    pub fn run_loop(mut self) {
        // SIGTERM/SIGINT → set flag → clean exit so Drop restores the CRTC.
        static SHUTDOWN: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        extern "C" fn on_signal(_: libc::c_int) {
            SHUTDOWN.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        unsafe {
            libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
            libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        }

        loop {
            if SHUTDOWN.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            // 1. Poll keyboard input (non-blocking libinput drain).
            let (app_keys, nav_events) = self.input.poll();
            for key in app_keys {
                match key {
                    AppKey::Escape => return,
                    AppKey::D => crate::render::spawn_demo_stimuli(&self.rs.scene),
                    AppKey::F1 => self.rs.show_overlay = !self.rs.show_overlay,
                    AppKey::F2 => {
                        let mut sc = self.rs.scene.write().expect("scene lock");
                        sc.photodiode.enabled = !sc.photodiode.enabled;
                        sc.photodiode.flicker = true;
                        sc.photodiode.lit = false;
                    }
                    AppKey::F3 => {
                        if self.rs.ctx.supports_wireframe {
                            self.rs.wireframe = !self.rs.wireframe;
                            log::info!(
                                "vstimd: wireframe {}",
                                if self.rs.wireframe { "ON" } else { "OFF" }
                            );
                        }
                    }
                }
            }

            // 2. Build egui raw input (DRM: screen rect + libinput nav keys).
            let egui_raw_input = self.rs.show_overlay
                .then(|| self.build_egui_raw_input(nav_events));

            // 3. Wait for the next vblank (blocking kernel scanout ioctl).
            //    When this returns, the previous frame is confirmed visible on
            //    the display.  This is the canonical "frame start" boundary.
            let screen_clock = self.wait_vblank();

            // [A] Commit previous frame's animation outputs; poll inputs.
            if let Some(vtl) = &self.vtl {
                let (input_edges, output_snapshot) = {
                    let mut v = vtl.lock().unwrap();
                    v.write_outputs(&self.pending_outputs);
                    let edges = v.poll();
                    let snap  = v.output_snapshot();
                    (edges, snap)
                };
                self.pending_outputs = [0; vtl::MAX_BANKS];
                self.rs.scene.write().unwrap().advance_animations(
                    &input_edges, &output_snapshot, &mut self.pending_outputs,
                );
            }

            // 4. Render: build overlay UI, tessellate scene, record Vulkan
            //    commands, submit to GPU, present to display.
            //    The frame prepared here will become visible at the next vblank.
            let sys_info = self.sys_info();
            self.rs.render_one_frame(screen_clock, egui_raw_input, &sys_info, self.vtl.as_deref());

            // pending_outputs is already saved for commit at next [A].
        }
        // When the loop exits, `self` is consumed and fields drop in
        // declaration order: `rs` (Vulkan teardown) → `input` → `drm_vblank`
        // → `display_info` → `display_guard` (CRTC restore).
        // The CRTC restore therefore fires after Vulkan has released DRM master.
    }
}
