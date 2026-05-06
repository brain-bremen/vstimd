mod display_guard;
mod init;
mod input;
mod vt;

use std::sync::{Arc, RwLock};

use crate::render::vk::{GpuBuffers, VkContext, VkPipeline, render_frame};
use crate::scene::SceneState;
use crate::timing::FrameStats;

use self::display_guard::DisplayGuard;
use self::input::{AppKey, InputState};
use self::vt::VtGuard;

/// Bare-metal Linux render state — drives the display directly via
/// `VK_KHR_display` without a compositor.
///
/// # Field / drop order
///
/// Rust drops struct fields in **declaration order** (first field = first
/// dropped).  The order here is deliberate:
///
/// 1. `ctx`           — VkContext::drop() calls device_wait_idle, then
///                      destroys all Vulkan objects and releases DRM master.
/// 2. `pipeline`      — no custom Drop (already destroyed in explicit_cleanup)
/// 3. `gpu_buffers`   — no custom Drop (already destroyed in explicit_cleanup)
/// 4. `input`         — libinput context freed
/// 5. `scene`         — Arc ref-count decremented
/// 6. `frame_stats`   — POD, trivial
/// 7. `display_guard` — **after Vulkan**, re-acquires DRM master and calls
///                      drmModeSetCrtc to point the CRTC back at the original
///                      fbcon framebuffer → fixes "no signal" on exit.
/// 8. `vt_guard`      — **last**: restores keyboard mode and VT_AUTO so the
///                      framebuffer console can take over the terminal.
pub struct DrmRenderState {
    ctx: VkContext,
    pipeline: VkPipeline,
    gpu_buffers: GpuBuffers,
    input: InputState,
    scene: Arc<RwLock<SceneState>>,
    frame_stats: FrameStats,
    /// display_guard and vt_guard are Option<_> so they can survive the
    /// DrmRenderState and be dropped in the correct order.  The compiler
    /// warns "never read" but they are consumed by their Drop impls.
    #[allow(dead_code)]
    display_guard: Option<DisplayGuard>,
    vt_guard: Option<VtGuard>,
}

impl DrmRenderState {
    pub fn new(scene: Arc<RwLock<SceneState>>) -> Self {
        // 1. Acquire the current VT *before* Vulkan starts so the console fd
        //    is open and keyboard mode is saved.
        let vt_guard = VtGuard::acquire();

        // 2. Snapshot display state *before* Vulkan takes DRM master.
        //    We always restore the CRTC on exit: vkAcquireDrmDisplayEXT bypasses
        //    any compositor, so nobody else will fix the CRTC after vkReleaseDisplayEXT.
        let display_guard = DisplayGuard::acquire();

        // 3. Initialise Vulkan — VK_KHR_display acquires DRM master internally.
        let (ctx, _display_info) = init::init();
        let pipeline = VkPipeline::new(&ctx.device, ctx.render_pass);
        let gpu_buffers = GpuBuffers::new(&ctx.instance, ctx.physical_device);
        let input = InputState::new();

        Self {
            ctx,
            pipeline,
            gpu_buffers,
            input,
            scene,
            frame_stats: FrameStats::new(60.0),
            display_guard,
            vt_guard,
        }
    }

    pub fn run_loop(mut self) {
        let mut frame_index: usize = 0;
        loop {
            // Check for pending VT-switch signals (Alt+Fn) each frame.
            // The closures are stubs for now; wiring full surface teardown /
            // recreation will be done when VT switching support is added.
            if let Some(vt) = &self.vt_guard {
                vt.poll(
                    || { /* VT release: should drop DRM master / Vulkan surfaces */ },
                    || { /* VT acquire: should re-create DRM master / Vulkan surfaces */ },
                );
            }

            for key in self.input.poll() {
                match key {
                    AppKey::Escape => {
                        // Explicitly destroy GPU resources while the device is
                        // still alive, then return.  The struct's Drop will then
                        // fire in field order: ctx → … → display_guard → vt_guard.
                        self.explicit_cleanup();
                        return;
                    }
                    AppKey::D => crate::render::spawn_demo_stimuli(&self.scene),
                    AppKey::F1 => {} // overlay not implemented for DRM yet
                }
            }

            // `None` means the swapchain is out of date (rare in DRM mode).
            let _tick = render_frame(
                &self.ctx,
                &self.pipeline,
                &mut self.gpu_buffers,
                &self.scene,
                &mut frame_index,
                &mut self.frame_stats,
            );
        }
        // When the loop exits (consuming `self`), fields drop in declaration
        // order — see the struct-level doc comment for the rationale.
    }

    /// Destroy GPU-owned resources while `ctx.device` is still live.
    ///
    /// Must be called before `return` so that vertex/index buffers and the
    /// pipeline are freed before `VkContext::drop()` tears down the device.
    ///
    /// Also performs the **primary CRTC restore** here, before `VkContext`
    /// drops and fires `vkReleaseDisplayEXT`.  At this point the `drm_fd`
    /// stored in `ctx.release_display` still holds DRM master (NVIDIA keeps
    /// it for the lifetime of the acquired display).  After `VkContext` drops,
    /// the compositor races to reclaim master and we lose the window.
    fn explicit_cleanup(&mut self) {
        unsafe { self.ctx.device.device_wait_idle().ok() };
        self.gpu_buffers.destroy_all(&self.ctx.device);
        self.pipeline.destroy(&self.ctx.device);

        // Primary CRTC restore: use the vkAcquireDrmDisplayEXT fd while it
        // still holds DRM master, before vkReleaseDisplayEXT fires.
        if let Some(ref dg) = self.display_guard {
            if let Some((_, _, Some(ref drm_fd))) = self.ctx.release_display {
                dg.restore_with_file(drm_fd);
            }
        }

        // display_guard and vt_guard are intentionally left in place; they
        // are released by their Drop impls in the correct order once `self`
        // is consumed after this function returns.
    }
}
