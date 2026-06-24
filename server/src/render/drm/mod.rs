mod display_guard;
mod init;
mod input;
mod vblank;
mod vt;

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
use self::vblank::{DrmVblank, VkVblank};

/// Bare-metal Linux render state — drives the display directly via
/// `VK_KHR_display` without a compositor.
///
/// Fields drop in declaration order: `rs` (Vulkan resources) before
/// `display_guard` (CRTC restore) before `vt_guard` (KD_TEXT restore), so
/// the VT is returned to text mode only after Vulkan has fully released the
/// display hardware.
pub struct DrmRenderState {
    rs: RenderState,
    vtl: Option<Arc<Mutex<VtlState>>>,
    input: InputState,
    drm_vblank: Option<DrmVblank>,
    vk_vblank: Option<VkVblank>,
    /// One-shot FIRST_PIXEL_OUT fence registered just before the previous present;
    /// collected at the top of the next iteration to get an accurate vblank time
    /// without double-blocking with the FIFO acquire.
    pending_vblank_fence: Option<ash::vk::Fence>,
    display_info: StimulusDisplayInfo,
    hardware_model: String,
    /// Animation output bits accumulated this frame; committed at [A] next frame.
    pending_outputs: [u64; vtl::MAX_BANKS],
    /// Holds the CRTC snapshot; dropped before `vt_guard` to restore the
    /// console framebuffer before KD_TEXT is re-enabled.
    #[allow(dead_code)]
    display_guard: Option<DisplayGuard>,
    /// Activates the target VT and holds KD_GRAPHICS; dropped last so the
    /// terminal isn't returned to text mode until Vulkan teardown is complete.
    #[allow(dead_code)]
    vt_guard: vt::VtGuard,
    /// True while our VT is not the active one (we released the input grab).
    suspended: bool,
}

fn check_device_permissions() {
    // Root can access all devices regardless of group membership.
    if unsafe { libc::getuid() } == 0 {
        return;
    }

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
    pub fn new(scene: Arc<RwLock<SceneState>>, vtl: Option<Arc<Mutex<VtlState>>>, log_buffer: LogBuffer, hardware_model: String) -> Self {
        check_device_permissions();

        // Snapshot display state first, while the current VT still has an
        // active CRTC mode.  On Jetson nvdisplay the VT switch deactivates
        // the CRTC (mode → None), so we must save state before switching.
        let display_guard = DisplayGuard::acquire();

        // Open DRM vblank here, before the VT switch and before Vulkan init.
        // Both events clear the CRTC mode: the VT switch deactivates it on
        // nvdisplay, and VK_KHR_display acquiring DRM master also returns
        // mode → None.  wait_vblank is unprivileged so the fd stays valid
        // throughout the session.
        let drm_vblank = DrmVblank::open();

        // Now activate the target VT and set KD_GRAPHICS so the kernel stops
        // writing text over our framebuffer.
        let vt_guard = vt::VtGuard::acquire();

        // Initialise Vulkan — VK_KHR_display acquires DRM master internally.
        let (ctx, display_info, vk_display) = init::init();
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
        // Build VkVblank before ctx moves into RenderState.
        let vk_vblank = ctx.display_control.as_ref().map(|loader| {
            VkVblank::new(ctx.device.clone(), loader.clone(), vk_display)
        });

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
            vk_vblank,
            pending_vblank_fence: None,
            display_info,
            hardware_model,
            pending_outputs: [0; vtl::MAX_BANKS],
            display_guard,
            vt_guard,
            suspended: false,
        }
    }

    fn sys_info(&self) -> SystemInfo {
        SystemInfo {
            display: self.display_info.clone(),
            backend: RenderTarget::Drm,
            local_ip: self.rs.local_ip.clone(),
            hostname: String::new(),
            gpu_name: String::new(),
            hardware_model: self.hardware_model.clone(),
            wireframe: None,
            clock_source: if self.drm_vblank.is_some() {
                ClockSource::DrmVblank
            } else if self.vk_vblank.is_some() {
                ClockSource::VkDisplayControl
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

    /// Collect the FIRST_PIXEL_OUT fence registered at the end of the previous
    /// frame.  Blocks for the remaining portion of the frame period (~7 ms at
    /// 120 Hz) until the display signals that our previous present is on screen.
    ///
    /// DRM vblank (when available) is a simple blocking ioctl that replaces the
    /// collect/register two-phase scheme used by the VK path.
    fn wait_vblank(&mut self) -> Option<std::time::Instant> {
        if let Some(vblank) = self.drm_vblank.as_ref() {
            match vblank.wait() {
                Some(t) => return Some(t),
                None => {
                    log::warn!("vstimd: disabling DRM vblank clock after wait_vblank error");
                    self.drm_vblank = None;
                }
            }
        }
        // VK path: collect the fence registered at the end of the previous frame.
        // On frame 0 there is no pending fence; we return None and render without
        // a vblank timestamp (render_one_frame falls back to Instant::now()).
        if let Some(fence) = self.pending_vblank_fence.take() {
            if let Some(vblank) = self.vk_vblank.as_ref() {
                match vblank.collect(fence) {
                    Some(t) => return Some(t),
                    None => {
                        log::warn!("vstimd: disabling VK_EXT_display_control vblank after error");
                        self.vk_vblank = None;
                    }
                }
            } else {
                // vk_vblank was disabled between register and collect; destroy fence.
                unsafe { self.rs.ctx.device.destroy_fence(fence, None) };
            }
        }
        None
    }

    /// Register the FIRST_PIXEL_OUT fence for collection at the top of the next
    /// frame.  Called just before render/present so the fence captures the vblank
    /// that will show the frame we are about to submit.
    ///
    /// Not called on the DRM vblank path (DRM handles its own blocking ioctl).
    fn register_vblank(&mut self) {
        if self.drm_vblank.is_some() {
            return;
        }
        // vkRegisterDisplayEventEXT always returns ERROR_UNKNOWN on NVIDIA Tegra
        // before the first present.  Skip frame 0 to avoid a spurious warning.
        if self.rs.frame_index == 0 {
            return;
        }
        if let Some(vblank) = self.vk_vblank.as_ref() {
            self.pending_vblank_fence = vblank.register();
        }
    }

    pub fn run_loop(mut self) {
        let mut clock_logged = false;
        loop {
            if crate::shutdown::is_requested() {
                return;
            }

            // Handle VT_PROCESS signals: release input grab when switching away,
            // re-acquire when switching back, so the other VT's session gets input.
            if self.vt_guard.release_requested() {
                self.input.suspend();
                self.vt_guard.allow_release();
                self.suspended = true;
                log::info!("vstimd: VT released — input suspended");
            }
            if self.vt_guard.acquire_requested() {
                self.vt_guard.confirm_acquire();
                self.input.resume();
                self.suspended = false;
                log::info!("vstimd: VT re-acquired — input resumed");
            }
            if self.suspended {
                std::thread::sleep(std::time::Duration::from_millis(16));
                continue;
            }

            // 1. Poll keyboard input (non-blocking libinput drain).
            let (app_keys, nav_events) = self.input.poll();
            for key in app_keys {
                match key {
                    AppKey::Escape => return,
                    AppKey::SwitchVt(n) => self.vt_guard.switch_to(n),
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

            // 3. Collect the FIRST_PIXEL_OUT fence from the previous present
            //    (VK path) or block on the DRM vblank ioctl.  When this returns,
            //    the previous frame is confirmed visible on the display.
            let screen_clock = self.wait_vblank();

            // Log the settled clock source once, after frame 1 (when the VK
            // fence has been collected for the first time).
            if !clock_logged && self.rs.frame_index > 0 {
                clock_logged = true;
                log::info!("vstimd: vblank clock: {}", self.sys_info().clock_source.as_str());
            }

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

            // Register the FIRST_PIXEL_OUT fence for the frame we are about to
            // present.  The fence is collected at the top of the next iteration.
            // This two-phase register→collect pattern avoids double-blocking with
            // the FIFO vkAcquireNextImageKHR (which also syncs to the display).
            self.register_vblank();

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
