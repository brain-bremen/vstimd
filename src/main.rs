// Traits must be in scope for method dispatch; `as _` imports them silently.
use kurbo::ParamCurve as _;
use wgpu::util::DeviceExt as _;

// ── Shader ───────────────────────────────────────────────────────────────────

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

// ── Vertex ───────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

// ── CPU tessellation ─────────────────────────────────────────────────────────

struct TessellatedBezier {
    path: kurbo::BezPath,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    dirty: bool,
}

impl TessellatedBezier {
    fn new(path: kurbo::BezPath) -> Self {
        Self { path, vertices: Vec::new(), indices: Vec::new(), dirty: true }
    }

    fn set_path(&mut self, path: kurbo::BezPath) {
        self.path = path;
        self.dirty = true;
    }

    /// Tessellate a **closed** path into a centroid-fan, coloured with an
    /// animated hue cycle driven by `t` (seconds).
    fn tessellate_filled(&mut self, steps_per_seg: usize, t: f64) {
        if !self.dirty {
            return;
        }
        self.vertices.clear();
        self.indices.clear();

        let mut outline: Vec<[f32; 2]> = Vec::new();
        for seg in self.path.segments() {
            match seg {
                kurbo::PathSeg::Cubic(c) => {
                    for i in 0..steps_per_seg {
                        let u = i as f64 / steps_per_seg as f64;
                        let pt = c.eval(u);
                        outline.push([pt.x as f32, pt.y as f32]);
                    }
                }
                kurbo::PathSeg::Quad(q) => {
                    for i in 0..steps_per_seg {
                        let u = i as f64 / steps_per_seg as f64;
                        let pt = q.eval(u);
                        outline.push([pt.x as f32, pt.y as f32]);
                    }
                }
                kurbo::PathSeg::Line(l) => {
                    outline.push([l.p0.x as f32, l.p0.y as f32]);
                }
            }
        }
        if outline.is_empty() {
            self.dirty = false;
            return;
        }

        // Centroid
        let cx = outline.iter().map(|p| p[0]).sum::<f32>() / outline.len() as f32;
        let cy = outline.iter().map(|p| p[1]).sum::<f32>() / outline.len() as f32;

        let hue = (t * 0.25) as f32;
        self.vertices.push(Vertex { position: [cx, cy], color: hsv_to_rgb(hue, 0.3, 1.0) });

        for pt in outline.iter() {
            // Anchor hue to world angle so colors stay geometrically fixed
            // as the bezier parameterisation shifts during breathing.
            let world_angle = (pt[1] - cy).atan2(pt[0] - cx); // -PI..PI
            let normalized = (world_angle / (2.0 * std::f32::consts::PI)).fract().abs();
            let edge_hue = hue + normalized;
            self.vertices.push(Vertex { position: *pt, color: hsv_to_rgb(edge_hue, 1.0, 0.9) });
        }

        let n = outline.len() as u32;
        for i in 0..n {
            self.indices.push(0);
            self.indices.push(1 + i);
            self.indices.push(1 + (i + 1) % n);
        }
        self.dirty = false;
    }

    /// Adaptive tessellation for **open** paths (fan per segment).
    fn tessellate(&mut self, tolerance: f32) {
        if !self.dirty {
            return;
        }
        self.vertices.clear();
        self.indices.clear();

        let mut vertex_idx = 0u32;
        for seg in self.path.segments() {
            match seg {
                kurbo::PathSeg::Cubic(cubic) => {
                    let subdivisions = self.subdivisions_for_cubic(&cubic, tolerance);
                    let mut seg_verts = Vec::new();
                    for i in 0..=subdivisions {
                        let t = i as f64 / subdivisions as f64;
                        let pt = cubic.eval(t);
                        seg_verts.push(Vertex {
                            position: [pt.x as f32, pt.y as f32],
                            color: [1.0, 1.0, 1.0, 1.0],
                        });
                    }
                    let seg_len = seg_verts.len() as u32;
                    self.vertices.extend(seg_verts);
                    for i in 1..(seg_len - 1) {
                        self.indices.push(vertex_idx);
                        self.indices.push(vertex_idx + i);
                        self.indices.push(vertex_idx + i + 1);
                    }
                    vertex_idx += seg_len;
                }
                kurbo::PathSeg::Quad(quad) => {
                    let subdivisions = 10u32;
                    let mut seg_verts = Vec::new();
                    for i in 0..=subdivisions {
                        let t = i as f64 / subdivisions as f64;
                        let pt = quad.eval(t);
                        seg_verts.push(Vertex {
                            position: [pt.x as f32, pt.y as f32],
                            color: [1.0, 1.0, 1.0, 1.0],
                        });
                    }
                    let seg_len = seg_verts.len() as u32;
                    self.vertices.extend(seg_verts);
                    for i in 1..(seg_len - 1) {
                        self.indices.push(vertex_idx);
                        self.indices.push(vertex_idx + i);
                        self.indices.push(vertex_idx + i + 1);
                    }
                    vertex_idx += seg_len;
                }
                kurbo::PathSeg::Line(line) => {
                    self.vertices.push(Vertex {
                        position: [line.p0.x as f32, line.p0.y as f32],
                        color: [1.0, 1.0, 1.0, 1.0],
                    });
                    self.vertices.push(Vertex {
                        position: [line.p1.x as f32, line.p1.y as f32],
                        color: [1.0, 1.0, 1.0, 1.0],
                    });
                    self.indices.push(vertex_idx);
                    self.indices.push(vertex_idx + 1);
                    vertex_idx += 2;
                }
            }
        }
        self.dirty = false;
    }

    fn subdivisions_for_cubic(&self, cubic: &kurbo::CubicBez, tolerance: f32) -> u32 {
        let p0 = cubic.p0;
        let p3 = cubic.p3;
        let mid = cubic.eval(0.5);
        let chord_vec = (p3.x - p0.x, p3.y - p0.y);
        let chord_len_sq = chord_vec.0 * chord_vec.0 + chord_vec.1 * chord_vec.1;
        if chord_len_sq < 1e-6 {
            return 1;
        }
        let to_mid = (mid.x - p0.x, mid.y - p0.y);
        let t_param = (to_mid.0 * chord_vec.0 + to_mid.1 * chord_vec.1) / chord_len_sq;
        let closest = (p0.x + t_param * chord_vec.0, p0.y + t_param * chord_vec.1);
        let deviation =
            ((mid.x - closest.0).powi(2) + (mid.y - closest.1).powi(2)).sqrt();
        ((deviation / tolerance as f64).ceil() as u32).max(1).min(64)
    }
}

// ── GPU shape ────────────────────────────────────────────────────────────────

struct GPUBezierShape {
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
    index_count: u32,
}

impl GPUBezierShape {
    fn new() -> Self {
        Self { vertex_buffer: None, index_buffer: None, index_count: 0 }
    }

    fn update_buffers(&mut self, device: &wgpu::Device, tessellated: &TessellatedBezier) {
        if tessellated.vertices.is_empty() {
            self.index_count = 0;
            return;
        }
        self.vertex_buffer = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bezier vertices"),
            contents: bytemuck::cast_slice(&tessellated.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }));
        self.index_buffer = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bezier indices"),
            contents: bytemuck::cast_slice(&tessellated.indices),
            usage: wgpu::BufferUsages::INDEX,
        }));
        self.index_count = tessellated.indices.len() as u32;
    }

    fn draw<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if let (Some(vb), Some(ib)) = (&self.vertex_buffer, &self.index_buffer) {
            render_pass.set_vertex_buffer(0, vb.slice(..));
            render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.index_count, 0, 0..1);
        }
    }
}

// ── Stimulus manager ─────────────────────────────────────────────────────────

struct BezierStimulus {
    cpu_shapes: Vec<TessellatedBezier>,
    gpu_shapes: Vec<GPUBezierShape>,
    tolerance: f32,
}

impl BezierStimulus {
    fn new(num_shapes: usize, tolerance: f32) -> Self {
        Self {
            cpu_shapes: (0..num_shapes)
                .map(|_| TessellatedBezier::new(kurbo::BezPath::new()))
                .collect(),
            gpu_shapes: (0..num_shapes).map(|_| GPUBezierShape::new()).collect(),
            tolerance,
        }
    }

    fn update_shape(&mut self, index: usize, path: kurbo::BezPath) {
        if index < self.cpu_shapes.len() {
            self.cpu_shapes[index].set_path(path);
        }
    }

    fn update_gpu_buffers(&mut self, device: &wgpu::Device) {
        for (cpu, gpu) in self.cpu_shapes.iter_mut().zip(self.gpu_shapes.iter_mut()) {
            cpu.tessellate(self.tolerance);
            gpu.update_buffers(device, cpu);
        }
    }

    fn draw_all<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        for gpu_shape in &self.gpu_shapes {
            gpu_shape.draw(render_pass);
        }
    }
}

// ── Animation ────────────────────────────────────────────────────────────────

/// A 5-lobe "amoeba" whose tips breathe and the whole shape slowly rotates.
fn make_animated_path(t: f64) -> kurbo::BezPath {
    let num_lobes = 5u32;
    let base_r = 0.55_f64;
    let lobe_amp = 0.20_f64;
    let rotation = t * 0.25;
    let n_pts = (num_lobes * 7) as usize;

    let pts: Vec<kurbo::Point> = (0..n_pts)
        .map(|i| {
            // `angle` is the body-fixed position; `theta` is the world position after rotation.
            // Using `angle` (not `theta`) for the radius keeps the lobe pattern locked to the
            // rotating body, so vertices don't crawl through the lobes.
            let angle = 2.0 * std::f64::consts::PI * i as f64 / n_pts as f64;
            let theta = angle + rotation;
            let r = base_r + lobe_amp * (num_lobes as f64 * angle + t * 1.5).cos();
            kurbo::Point::new(r * theta.cos(), r * theta.sin())
        })
        .collect();

    smooth_closed_bezier(&pts)
}

/// Catmull-Rom–style smooth closed cubic Bézier through the given points.
fn smooth_closed_bezier(pts: &[kurbo::Point]) -> kurbo::BezPath {
    let n = pts.len();
    let tension = 0.33_f64;
    let mut path = kurbo::BezPath::new();
    path.move_to(pts[0]);
    for i in 0..n {
        let p0 = pts[(i + n - 1) % n];
        let p1 = pts[i];
        let p2 = pts[(i + 1) % n];
        let p3 = pts[(i + 2) % n];
        let cp1 = kurbo::Point::new(
            p1.x + (p2.x - p0.x) * tension,
            p1.y + (p2.y - p0.y) * tension,
        );
        let cp2 = kurbo::Point::new(
            p2.x - (p3.x - p1.x) * tension,
            p2.y - (p3.y - p1.y) * tension,
        );
        path.curve_to(cp1, cp2, p2);
    }
    path.close_path();
    path
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 4] {
    let h = h.fract().abs() * 6.0;
    let i = h as u32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r, g, b, 1.0]
}

// ── Pipeline ─────────────────────────────────────────────────────────────────

fn create_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("main shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(WGSL_SHADER)),
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pipeline layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("main pipeline"),
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

// ── wgpu state ───────────────────────────────────────────────────────────────

const BUF_SIZE: u64 = 512 * 1024; // 512 KB per buffer — plenty for the demo

struct State {
    // surface must be listed before window so it is dropped first
    surface: wgpu::Surface<'static>,
    device: std::sync::Arc<wgpu::Device>,
    queue: std::sync::Arc<wgpu::Queue>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    start_time: std::time::Instant,
    window: std::sync::Arc<winit::window::Window>,
}

impl State {
    fn new(window: std::sync::Arc<winit::window::Window>) -> Self {
        let instance = wgpu::Instance::default();

        // SAFETY: `surface` is stored alongside `window` (via the Arc) in the
        // same `State` struct, and `surface` is declared first so it is dropped
        // before `window`, keeping the raw handle valid for the surface's lifetime.
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

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vertex"),
            size: BUF_SIZE,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("index"),
            size: BUF_SIZE,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count: 0,
            start_time: std::time::Instant::now(),
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
        let t = self.start_time.elapsed().as_secs_f64();
        let path = make_animated_path(t);
        let mut tess = TessellatedBezier::new(path);
        tess.tessellate_filled(14, t);

        if tess.vertices.is_empty() {
            return;
        }

        let vb_bytes: &[u8] = bytemuck::cast_slice(&tess.vertices);
        let ib_bytes: &[u8] = bytemuck::cast_slice(&tess.indices);

        if vb_bytes.len() as u64 <= BUF_SIZE && ib_bytes.len() as u64 <= BUF_SIZE {
            self.queue.write_buffer(&self.vertex_buffer, 0, vb_bytes);
            self.queue.write_buffer(&self.index_buffer, 0, ib_bytes);
            self.index_count = tess.indices.len() as u32;
        }
    }

    fn render(&self) {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());

        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            rp.set_pipeline(&self.pipeline);
            rp.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            rp.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            if self.index_count > 0 {
                rp.draw_indexed(0..self.index_count, 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        self.window.request_redraw();
    }
}

// ── winit application ────────────────────────────────────────────────────────

struct App {
    state: Option<State>,
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.state.is_none() {
            let attrs = winit::window::Window::default_attributes()
                .with_title("Bézier Stimulus — wgpu + kurbo");
            let window = std::sync::Arc::new(event_loop.create_window(attrs).unwrap());
            self.state = Some(State::new(window));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(state) = &mut self.state else { return };
        match event {
            winit::event::WindowEvent::CloseRequested => event_loop.exit(),
            winit::event::WindowEvent::Resized(size) => state.resize(size),
            winit::event::WindowEvent::RedrawRequested => {
                state.update();
                state.render();
            }
            _ => {}
        }
    }
}

// ── entry point ──────────────────────────────────────────────────────────────

fn main() {
    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    // Poll continuously so the window redraws every iteration without waiting
    // for OS events — keeps the animation smooth at display refresh rate.
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = App { state: None };
    event_loop.run_app(&mut app).unwrap();
}
