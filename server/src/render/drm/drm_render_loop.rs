use std::sync::{Arc, Mutex};

use crate::log_buffer::LogBuffer;
use crate::render::AppKey;
use crate::render::RenderState;
use crate::render::backend::BackendData;
use crate::render::system_info::SystemInfo;
use crate::render::RenderTarget;
use crate::render::{SceneRenderer, TextRenderer, UiRenderer};
use crate::render::render_frame;
use crate::timing::FrameTiming;
use crate::vtl_state::VtlState;
extern crate vtl;

use super::drm_display_guard::DrmDisplayGuard;
use super::drm_keyboard_input::InputState;
use super::drm_vblank::{DrmVblank, DrmVblankState, VkVblank};
use super::drm_virtual_terminal::DrmVtGuard;

// ── Public backend ────────────────────────────────────────────────────────────

pub struct DrmBackend {
    data: BackendData,
    log_buffer: LogBuffer,
}

impl DrmBackend {
    pub fn new(data: BackendData, log_buffer: LogBuffer) -> Self {
        Self { data, log_buffer }
    }
}

impl DrmBackend {
    pub fn run(self, on_ready: impl FnOnce()) {
        let data = DrmRenderLoopData::new(self.data, self.log_buffer);
        on_ready();
        data.run_loop();
    }
}

// ── DrmRenderLoopData ─────────────────────────────────────────────────────────

/// All data required to run one iteration of the DRM render loop.
///
/// Fields drop in declaration order: `rs` (Vulkan resources) before
/// `display_guard` (CRTC restore) before `vt_guard` (KD_TEXT restore), so
/// the VT is returned to text mode only after Vulkan has fully released the
/// display hardware.
struct DrmRenderLoopData {
    rs: RenderState,
    vtl: Option<Arc<Mutex<VtlState>>>,
    input: InputState,
    vblank: DrmVblankState,
    /// Holds the CRTC snapshot; dropped before `vt_guard` to restore the
    /// console framebuffer before KD_TEXT is re-enabled.
    #[allow(dead_code)]
    display_guard: Option<DrmDisplayGuard>,
    /// Activates the target VT and holds KD_GRAPHICS; dropped last so the
    /// terminal isn't returned to text mode until Vulkan teardown is complete.
    #[allow(dead_code)]
    vt_guard: DrmVtGuard,
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
        unsafe {
            libc::access(
                path.as_ptr() as *const libc::c_char,
                libc::R_OK | libc::W_OK,
            ) == 0
        }
    });
    if !drm_ok {
        missing.push(
            "  /dev/dri/card* — add user to 'video' group:\n    sudo usermod -aG video $USER"
                .to_string(),
        );
    }

    let input_ok = unsafe {
        let grp = libc::getgrnam(c"input".as_ptr());
        if grp.is_null() {
            true
        } else {
            let input_gid = (*grp).gr_gid;
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

impl DrmRenderLoopData {
    fn new(data: BackendData, log_buffer: LogBuffer) -> Self {
        let BackendData { scene, vtl, host_info } = data;
        check_device_permissions();

        // Snapshot display state first, while the current VT still has an
        // active CRTC mode.  On Jetson nvdisplay the VT switch deactivates
        // the CRTC (mode → None), so we must save state before switching.
        let display_guard = DrmDisplayGuard::acquire();

        // Open DRM vblank here, before the VT switch and before Vulkan init.
        // Both events clear the CRTC mode: the VT switch deactivates it on
        // nvdisplay, and VK_KHR_display acquiring DRM master also returns
        // mode → None.  wait_vblank is unprivileged so the fd stays valid
        // throughout the session.
        let drm_vblank = DrmVblank::open();

        // Now activate the target VT and set KD_GRAPHICS so the kernel stops
        // writing text over our framebuffer.
        let vt_guard = DrmVtGuard::acquire();

        // Initialise Vulkan — VK_KHR_display acquires DRM master internally.
        let (ctx, display_info, vk_display) = super::drm_init::init();

        // Build scene + text sub-renderers first (before ctx moves).
        let config_dir = scene.read().unwrap().runtime.config_dir.clone();
        let scene_renderer = SceneRenderer::new(&ctx, scene);
        let text = TextRenderer::new(&ctx);

        ctx.set_debug_name(ctx.render_pass, "render_pass");
        ctx.set_debug_name(ctx.egui_render_pass, "egui_render_pass");
        for (i, frame) in ctx.frames.iter().enumerate() {
            ctx.set_debug_name(frame.command_buffer, &format!("frame[{i}]_cmd"));
        }
        for (i, img) in ctx.swapchain_images.iter().enumerate() {
            ctx.set_debug_name(*img, &format!("swapchain[{i}]"));
        }

        let ui = UiRenderer::new(&ctx, config_dir, log_buffer);

        // Build vblank state before ctx moves into RenderState.
        let vk_vblank = ctx
            .display_control
            .as_ref()
            .map(|loader| VkVblank::new(ctx.device.clone(), loader.clone(), vk_display));
        let vblank = DrmVblankState::new(ctx.device.clone(), drm_vblank, vk_vblank);

        let system_info = SystemInfo {
            host: host_info,
            gpu_name: String::new(),
            backend: RenderTarget::Drm,
            supports_wireframe: ctx.supports_wireframe,
            clock_source: vblank.clock_source(ctx.present_wait.is_some()),
        };

        let rs = RenderState {
            scene_renderer,
            text,
            ui: Some(ui),
            timing: FrameTiming::new(display_info.refresh_hz),
            system_info,
            display_info,
            ctx,
        };

        Self {
            rs,
            vtl,
            input: InputState::new(),
            vblank,
            display_guard,
            vt_guard,
            suspended: false,
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

    fn run_loop(mut self) {
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
                    AppKey::Quit => {
                        crate::shutdown::request();
                    }
                    // Esc never quits — it closes a dialog or hides the overlay.
                    // (Quit via Ctrl+Q, SIGINT, or Ctrl+Alt+Fn to another VT then kill.)
                    AppKey::Escape => {
                        if let Some(ui) = &mut self.rs.ui {
                            ui.overlay.handle_escape();
                        }
                    }
                    AppKey::ToggleOverlay => {
                        if let Some(ui) = &mut self.rs.ui {
                            ui.overlay.toggle_master();
                        }
                    }
                    AppKey::ShowGroup(group) => {
                        if let Some(ui) = &mut self.rs.ui {
                            ui.overlay.show_group(group);
                        }
                    }
                    AppKey::HideGroup(group) => {
                        if let Some(ui) = &mut self.rs.ui {
                            ui.overlay.hide_group(group);
                        }
                    }
                    AppKey::SwitchVt(n) => self.vt_guard.switch_to(n),
                    // Demo spawn only when the overlay is hidden, so 'd' types
                    // into dialog fields while the overlay is up.
                    AppKey::D => {
                        let overlay_up = self.rs.ui.as_ref().is_some_and(|ui| ui.overlay.master_visible);
                        if !overlay_up {
                            crate::render::spawn_demo_stimuli(&self.rs.scene_renderer.scene);
                        }
                    }
                }
            }

            // 2. Build egui raw input (DRM: screen rect + libinput nav keys).
            let egui_raw_input = self.rs.ui
                .as_ref()
                .filter(|ui| ui.overlay.master_visible)
                .map(|_| self.build_egui_raw_input(nav_events));

            // 3. Block on vblank: DRM ioctl path blocks here directly; VK path
            //    collects the FIRST_PIXEL_OUT fence registered at end of last frame.
            //    When this returns, the previous frame is confirmed visible.
            let screen_clock = self.vblank.wait();

            // Log the settled clock source once, after frame 1 (when the VK
            // fence has been collected for the first time).
            if !clock_logged && self.rs.timing.frame_index > 0 {
                clock_logged = true;
                log::info!(
                    "vstimd: vblank clock: {}",
                    self.rs.system_info.clock_source.as_str()
                );
            }

            // [A] Commit staged outputs; poll inputs; advance animations.
            if let Some(vtl) = &self.vtl {
                let (input_edges, output_snapshot, mut staged) = {
                    let mut v = vtl.lock().unwrap();
                    v.commit_staged();
                    let edges = v.poll();
                    let snap  = v.output_snapshot();
                    let staged = v.staged;
                    (edges, snap, staged)
                };
                self.rs.scene_renderer.scene.write().unwrap().advance_animations(
                    &input_edges,
                    &output_snapshot,
                    &mut staged,
                );
                vtl.lock().unwrap().staged = staged;
            }

            // Register the FIRST_PIXEL_OUT fence for the frame we are about to
            // present.  The fence is collected at the top of the next iteration.
            // This two-phase register→collect pattern avoids double-blocking with
            // the FIFO vkAcquireNextImageKHR (which also syncs to the display).
            self.vblank.register(self.rs.timing.frame_index as u64);

            // 4. Render: build overlay UI, tessellate scene, record Vulkan
            //    commands, submit to GPU, present to display.
            //    The frame prepared here will become visible at the next vblank.
            render_frame(
                &mut self.rs,
                screen_clock,
                egui_raw_input,
                self.vtl.as_deref(),
            );

        }
        // When the loop exits, `self` is consumed and fields drop in
        // declaration order: `rs` (Vulkan teardown) → `input` → `vblank`
        // → `display_guard` (CRTC restore) → `vt_guard` (KD_TEXT restore).
        // The CRTC restore therefore fires after Vulkan has released DRM master.
    }
}
