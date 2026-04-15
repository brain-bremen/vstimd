mod input;
mod vk_buffers;
mod vk_frame;
mod vk_init;
mod vk_pipeline;

use std::sync::{Arc, RwLock};

use crate::scene::SceneState;
use crate::timing::FrameStats;

use self::input::InputState;
use self::vk_buffers::GpuBuffers;
use self::vk_init::VkContext;
use self::vk_pipeline::VkPipeline;

/// Bare-metal Linux render state — owns all Vulkan and input handles.
///
/// Created with [`RenderState::new`]; drives itself with [`RenderState::run_loop`].
pub struct RenderState {
    ctx: VkContext,
    pipeline: VkPipeline,
    gpu_buffers: GpuBuffers,
    input: InputState,
    scene: Arc<RwLock<SceneState>>,
    frame_stats: FrameStats,
    pub show_overlay: bool,
}

impl RenderState {
    /// Initialise the bare-metal Vulkan renderer.
    ///
    /// Discovers the first connected DRM display, acquires it exclusively via
    /// `VK_EXT_acquire_drm_display`, and sets up a full Vulkan swapchain ready
    /// for rendering.  Panics with a descriptive message if any step fails.
    pub fn new(scene: Arc<RwLock<SceneState>>) -> Self {
        let (ctx, display) = vk_init::init();

        eprintln!("wonderlamp: display {}×{}", display.width, display.height);
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
            show_overlay: false,
        }
    }

    /// Enter the render loop.  Blocks until the user presses ESC or the process
    /// receives a signal.  Cleans up all GPU resources on return.
    pub fn run_loop(mut self) {
        vk_frame::run_loop(
            &self.ctx,
            &self.pipeline,
            &mut self.gpu_buffers,
            Arc::clone(&self.scene),
            &mut self.input,
            &mut self.frame_stats,
            &mut self.show_overlay,
        );

        // Flush outstanding GPU work before Drop frees Vulkan handles.
        unsafe { self.ctx.device.device_wait_idle().ok() };
        self.gpu_buffers.destroy_all(&self.ctx.device);
        self.pipeline.destroy(&self.ctx.device);
    }
}
