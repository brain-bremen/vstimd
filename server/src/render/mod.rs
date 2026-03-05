mod tess;

pub use tess::Vertex;

use std::collections::HashMap;

use wgpu::util::DeviceExt as _;

use crate::scene::SceneState;

// ── WGSL shader ───────────────────────────────────────────────────────────────

const WGSL_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color:    vec4<f32>,
}
struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       color:    vec4<f32>,
}
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_pos = vec4<f32>(in.position, 0.0, 1.0);
    out.color    = in.color;
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

// ── Pipeline ──────────────────────────────────────────────────────────────────

fn create_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("solid shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(WGSL_SHADER)),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pipeline layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("solid pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 8,
                        shader_location: 1,
                    },
                ],
            }],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    })
}

// ── GPU buffers ───────────────────────────────────────────────────────────────

struct StimulusMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

struct GpuBuffers {
    meshes: HashMap<u32, StimulusMesh>,
}

impl GpuBuffers {
    fn new() -> Self {
        Self { meshes: HashMap::new() }
    }

    fn upload(&mut self, handle: u32, device: &wgpu::Device, verts: &[Vertex], idxs: &[u32]) {
        if verts.is_empty() {
            self.meshes.remove(&handle);
            return;
        }
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(idxs),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.meshes
            .insert(handle, StimulusMesh { vertex_buffer: vb, index_buffer: ib, index_count: idxs.len() as u32 });
    }
}

// ── Frame timing ──────────────────────────────────────────────────────────────

const FRAME_HISTORY: usize = 120;

pub struct FrameSummary {
    pub fps: f64,
    pub mean_ms: f64,
    pub std_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub drop_count: u64,
    pub frame_index: u64,
}

struct FrameStats {
    frame_index: u64,
    last_present: Option<std::time::Instant>,
    durations_ns: [u64; FRAME_HISTORY],
    ring_head: usize,
    valid_count: usize,
    drop_count: u64,
    expected_frame_ns: u64,
}

impl FrameStats {
    fn new(target_hz: f64) -> Self {
        Self {
            frame_index: 0,
            last_present: None,
            durations_ns: [0; FRAME_HISTORY],
            ring_head: 0,
            valid_count: 0,
            drop_count: 0,
            expected_frame_ns: (1_000_000_000.0 / target_hz) as u64,
        }
    }

    fn on_present(&mut self) {
        let now = std::time::Instant::now();
        if let Some(last) = self.last_present {
            let dur_ns = now.duration_since(last).as_nanos() as u64;
            let threshold = self.expected_frame_ns * 3 / 2;
            if dur_ns > threshold && self.expected_frame_ns > 0 {
                self.drop_count += (dur_ns / self.expected_frame_ns).saturating_sub(1);
            }
            self.durations_ns[self.ring_head] = dur_ns;
            self.ring_head = (self.ring_head + 1) % FRAME_HISTORY;
            if self.valid_count < FRAME_HISTORY {
                self.valid_count += 1;
            }
        }
        self.last_present = Some(now);
        self.frame_index += 1;
    }

    fn summary(&self) -> FrameSummary {
        let durations = &self.durations_ns[..self.valid_count.min(FRAME_HISTORY)];
        if durations.is_empty() {
            return FrameSummary {
                fps: 0.0, mean_ms: 0.0, std_ms: 0.0,
                min_ms: 0.0, max_ms: 0.0,
                drop_count: self.drop_count,
                frame_index: self.frame_index,
            };
        }
        let n = durations.len() as f64;
        let mean_ns = durations.iter().sum::<u64>() as f64 / n;
        let var_ns = durations.iter().map(|&d| { let x = d as f64 - mean_ns; x * x }).sum::<f64>() / n;
        FrameSummary {
            fps:        if mean_ns > 0.0 { 1_000_000_000.0 / mean_ns } else { 0.0 },
            mean_ms:    mean_ns / 1_000_000.0,
            std_ms:     var_ns.sqrt() / 1_000_000.0,
            min_ms:     *durations.iter().min().unwrap() as f64 / 1_000_000.0,
            max_ms:     *durations.iter().max().unwrap() as f64 / 1_000_000.0,
            drop_count: self.drop_count,
            frame_index: self.frame_index,
        }
    }
}

// ── egui overlay (feature = "overlay") ───────────────────────────────────────

#[cfg(feature = "overlay")]
struct OverlayRenderer {
    ctx: egui::Context,
    renderer: egui_wgpu::Renderer,
    winit_state: egui_winit::State,
}

#[cfg(feature = "overlay")]
impl OverlayRenderer {
    fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        window: &winit::window::Window,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Self {
        let ctx = egui::Context::default();
        let renderer = egui_wgpu::Renderer::new(device, surface_format, None, 1, false);
        let winit_state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            event_loop,
            None,
            None,
            None,
        );
        Self { ctx, renderer, winit_state }
    }

    fn on_window_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> egui_winit::EventResponse {
        self.winit_state.on_window_event(window, event)
    }

    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        window: &winit::window::Window,
        stats: &FrameSummary,
        pixels_per_point: f32,
    ) {
        let raw_input = self.winit_state.take_egui_input(window);
        let full_output = self.ctx.run(raw_input, |ctx| {
            egui::Window::new("Frame Timing")
                .default_pos([8.0, 8.0])
                .resizable(false)
                .show(ctx, |ui| {
                    let fps_color = if stats.fps < 55.0 { egui::Color32::RED }
                        else if stats.fps < 58.0 { egui::Color32::YELLOW }
                        else { egui::Color32::GREEN };
                    ui.colored_label(fps_color, format!("FPS:    {:5.1}", stats.fps));

                    let jitter_color = if stats.std_ms > 1.0 { egui::Color32::RED }
                        else if stats.std_ms > 0.3 { egui::Color32::YELLOW }
                        else { egui::Color32::GREEN };
                    ui.colored_label(jitter_color, format!("Jitter: {:5.2} ms (std)", stats.std_ms));
                    ui.label(format!("Mean:   {:5.2} ms", stats.mean_ms));
                    ui.label(format!("Min:    {:5.2} ms", stats.min_ms));
                    ui.label(format!("Max:    {:5.2} ms", stats.max_ms));

                    let drop_color = if stats.drop_count >= 3 { egui::Color32::RED }
                        else if stats.drop_count >= 1 { egui::Color32::YELLOW }
                        else { egui::Color32::GREEN };
                    ui.colored_label(drop_color, format!("Drops:  {}", stats.drop_count));
                    ui.label(format!("Frame:  {}", stats.frame_index));
                });
        });

        self.winit_state.handle_platform_output(window, full_output.platform_output);
        let tris = self.ctx.tessellate(full_output.shapes, pixels_per_point);
        for (id, delta) in full_output.textures_delta.set {
            self.renderer.update_texture(device, queue, id, &delta);
        }
        let screen_desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [window.inner_size().width, window.inner_size().height],
            pixels_per_point,
        };
        self.renderer.update_buffers(device, queue, encoder, &tris, &screen_desc);
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui overlay pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            self.renderer.render(&mut rp, &tris, &screen_desc);
        }
        for id in full_output.textures_delta.free {
            self.renderer.free_texture(&id);
        }
    }
}

// ── Render state ──────────────────────────────────────────────────────────────

struct RenderState {
    // `surface` must be declared before `window` so it drops first.
    surface: wgpu::Surface<'static>,
    device: std::sync::Arc<wgpu::Device>,
    queue: std::sync::Arc<wgpu::Queue>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    gpu_buffers: GpuBuffers,
    scene: SceneState,
    frame_stats: FrameStats,
    show_overlay: bool,
    #[cfg(feature = "overlay")]
    overlay: Option<OverlayRenderer>,
    window: std::sync::Arc<winit::window::Window>,
}

impl RenderState {
    fn new(
        window: std::sync::Arc<winit::window::Window>,
        scene: SceneState,
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
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface not supported by the selected adapter");
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

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn update(&mut self) {
        if self.scene.pending_flip {
            self.scene.apply_flip();
        }

        let screen_size = (self.config.width, self.config.height);
        self.scene.screen_size = screen_size;
        self.scene.frame_rate = self.frame_stats.summary().fps as f32;

        // Remove GPU buffers for stimuli that no longer exist.
        self.gpu_buffers.meshes.retain(|h, _| self.scene.stimuli.contains_key(h));

        // Tessellate every stimulus and upload. In Phase 2 this will be
        // made incremental (only upload dirty stimuli).
        let handles: Vec<u32> = self.scene.stimuli.keys().copied().collect();
        for handle in handles {
            let (verts, idxs) =
                tess::tessellate_stimulus(&self.scene.stimuli[&handle], screen_size);
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
            let bg = self.scene.background.live;
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
            for (handle, _) in &self.scene.stimuli {
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

// ── winit application ─────────────────────────────────────────────────────────

pub struct App {
    /// Held here until the window is created, then moved into `RenderState`.
    scene: Option<SceneState>,
    state: Option<RenderState>,
}

impl App {
    pub fn new(scene: SceneState) -> Self {
        Self { scene: Some(scene), state: None }
    }
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.state.is_none() {
            let attrs = winit::window::Window::default_attributes()
                .with_title("Wonderlamp — wgpu stimulus demo");
            let window = std::sync::Arc::new(event_loop.create_window(attrs).unwrap());
            let scene = self.scene.take().expect("scene already consumed");
            self.state = Some(RenderState::new(
                window,
                scene,
                #[cfg(feature = "overlay")]
                event_loop,
            ));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(state) = &mut self.state else { return };

        #[cfg(feature = "overlay")]
        if let Some(overlay) = &mut state.overlay {
            let response = overlay.on_window_event(&state.window, &event);
            if response.consumed {
                return;
            }
        }

        match event {
            winit::event::WindowEvent::CloseRequested => event_loop.exit(),
            winit::event::WindowEvent::Resized(size) => state.resize(size),
            winit::event::WindowEvent::KeyboardInput {
                event: winit::event::KeyEvent {
                    physical_key: winit::keyboard::PhysicalKey::Code(
                        winit::keyboard::KeyCode::F1,
                    ),
                    state: winit::event::ElementState::Pressed,
                    ..
                },
                ..
            } => {
                state.show_overlay = !state.show_overlay;
            }
            winit::event::WindowEvent::RedrawRequested => {
                state.update();
                state.render();
            }
            _ => {}
        }
    }
}
