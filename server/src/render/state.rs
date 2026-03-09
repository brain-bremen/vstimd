use std::sync::{Arc, RwLock};

use crate::scene::SceneState;
use crate::timing::FrameStats;

use super::gpu_buffers::GpuBuffers;
#[cfg(feature = "overlay")]
use super::overlay::OverlayRenderer;
use super::pipeline::create_pipeline;
use super::tess;

/// Owns the wgpu device/surface/pipeline and drives the per-frame render loop.
///
/// `scene` is shared with the ZMQ server thread via `Arc<RwLock<SceneState>>`.
/// The locking discipline used each frame is:
///
/// 1. **`update()`** — acquires a **write lock** to apply any pending deferred
///    flip, update bookkeeping fields (`screen_size`, `frame_rate`), and
///    tessellate all stimuli.  The lock is dropped when `update()` returns.
/// 2. **`render()`** — acquires a **read lock** only long enough to snapshot
///    the background colour and collect the draw list from `gpu_buffers`.
///    The lock is dropped before GPU submission.
///
/// This means the ZMQ thread can acquire a write lock in the gap between
/// `update()` and `render()` (or after `render()`) — it is never blocked for
/// more than the time it takes to finish the tessellation pass.
pub struct RenderState {
    // `surface` must be declared before `window` so it drops first.
    surface: wgpu::Surface<'static>,
    device: std::sync::Arc<wgpu::Device>,
    queue: std::sync::Arc<wgpu::Queue>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    gpu_buffers: GpuBuffers,
    scene: Arc<RwLock<SceneState>>,
    frame_stats: FrameStats,
    pub show_overlay: bool,
    #[cfg(feature = "overlay")]
    pub overlay: Option<OverlayRenderer>,
    pub window: std::sync::Arc<winit::window::Window>,
}

impl RenderState {
    pub fn new(
        window: std::sync::Arc<winit::window::Window>,
        scene: Arc<RwLock<SceneState>>,
        #[cfg(feature = "overlay")] event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Self {
        let instance = wgpu::Instance::default();

        // SAFETY: `surface` and `window` are stored together in this struct,
        // with `surface` declared first so it drops before `window`.
        let surface: wgpu::Surface<'static> = unsafe {
            let target = wgpu::SurfaceTargetUnsafe::from_window(&*window)
                .expect("failed to create surface target");
            instance.create_surface_unsafe(target).expect("failed to create surface")
        };

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("no suitable GPU adapter found");

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("failed to create device");
        let device = std::sync::Arc::new(device);
        let queue = std::sync::Arc::new(queue);

        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface not supported by the selected adapter");

        // Fifo (vsync): tear-free, one frame of latency, presents at vblank.
        // With the borderless fullscreen window covering the full monitor,
        // DXGI bypasses DWM and PresentMon should report
        // "Hardware: Independent Flip".
        config.present_mode = wgpu::PresentMode::Fifo;

        surface.configure(&device, &config);

        let pipeline = create_pipeline(&device, config.format);

        #[cfg(feature = "overlay")]
        let overlay = Some(OverlayRenderer::new(&device, config.format, &window, event_loop));

        Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            gpu_buffers: GpuBuffers::new(),
            scene,
            frame_stats: FrameStats::new(60.0),
            show_overlay: true,
            #[cfg(feature = "overlay")]
            overlay,
            window,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Main frame tick: advance scene state then render.
    /// Called once per frame from the winit event loop (see `app.rs`).
    pub fn tick(&mut self) {
        self.update();
        self.render();
    }

    fn update(&mut self) {
        let screen_size = (self.config.width, self.config.height);
        let fps = self.frame_stats.summary().fps as f32;

        let mut scene = self.scene.write().expect("scene lock poisoned");

        if scene.pending_flip {
            scene.apply_flip();
        }

        scene.screen_size = screen_size;
        scene.frame_rate = fps;

        // Remove GPU buffers for stimuli that no longer exist.
        self.gpu_buffers.meshes.retain(|h, _| scene.stimuli.contains_key(h));

        // Tessellate every stimulus and upload. In Phase 2 this will be
        // made incremental (only upload dirty stimuli).
        let handles: Vec<u32> = scene.stimuli.keys().copied().collect();
        for handle in handles {
            let (verts, idxs) = tess::tessellate_stimulus(&scene.stimuli[&handle], screen_size);
            self.gpu_buffers.upload(handle, &self.device, &verts, &idxs);
        }
    }

    fn render(&mut self) {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());

        {
            let scene = self.scene.read().expect("scene lock poisoned");
            let bg = scene.background.live;
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg[0] as f64,
                            g: bg[1] as f64,
                            b: bg[2] as f64,
                            a: bg[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            rp.set_pipeline(&self.pipeline);

            // Draw stimuli in insertion order (= draw order).
            for (handle, _) in &scene.stimuli {
                if let Some(mesh) = self.gpu_buffers.meshes.get(handle) {
                    if mesh.index_count > 0 {
                        rp.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        rp.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        rp.draw_indexed(0..mesh.index_count, 0, 0..1);
                    }
                }
            }
        }

        #[cfg(feature = "overlay")]
        if self.show_overlay {
            if let Some(overlay) = &mut self.overlay {
                let summary = self.frame_stats.summary();
                let ppp = self.window.scale_factor() as f32;
                overlay.render(
                    &self.device, &self.queue, &mut encoder,
                    &view, &self.window, &summary, ppp,
                );
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        self.frame_stats.on_present();
        self.window.request_redraw();
    }
}
