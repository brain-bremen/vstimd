# egui Overlay Integration Plan

**Goal:** Integrate egui overlay support into both DRM/console and winit desktop rendering paths using a custom Vulkan renderer.

## Why Not Use `egui-ash`?

**TL;DR:** Version incompatibility + architectural mismatch.

The `egui-ash` crate exists but has problems for vstimd:

1. **Version mismatch:**
   - `egui-ash 0.4.0` uses `ash 0.37`, `egui 0.25`, `ash-window 0.12`
   - vstimd uses `ash 0.38`, `egui 0.33`, `ash-window 0.13`
   - Downgrading would lose bug fixes and features

2. **Architectural incompatibility:**
   - `egui-ash` is a **full application framework** with `egui_ash::run()` that owns the event loop
   - vstimd has **custom event loops** in both `drm/mod.rs` and `winit_vk/mod.rs` for precise frame timing
   - `egui-ash` manages swapchains; vstimd's `VkContext` already does this
   - `egui-ash` uses `AppCreator`/`App` traits; vstimd has its own structure with `SceneState`, ZMQ server thread, etc.

3. **Tight vsync requirements:**
   - vstimd needs `VK_KHR_present_wait` and frame-perfect timing
   - `egui-ash` abstracts away low-level presentation control

**Conclusion:** A custom lightweight egui renderer (300-500 lines) is better than forcing vstimd's architecture into `egui-ash`'s framework.

---

## Current State

### What's Already Working

1. **winit backend (`render/winit_vk/mod.rs`):**
   - `egui_ctx: egui::Context` instantiated
   - `egui_winit::State` handles input forwarding and raw input collection
   - `build_overlay_ui()` builds UI into `egui::FullOutput`
   - **Missing:** Vulkan renderer — tessellation happens but shapes are not painted

2. **DRM backend (`render/drm/mod.rs`):**
   - No egui integration at all
   - F1 key handler exists but is a no-op
   - Keyboard input via libinput (arrow keys, Enter, Escape available)

3. **Shared Vulkan code (`render/vk/`):**
   - `VkContext`: device, swapchain, render pass, framebuffers
   - `VkPipeline`: single pipeline for stimulus rendering (vertex + fragment)
   - `GpuBuffers`: per-stimulus vertex/index buffer upload
   - `render_frame()`: single render pass, one pipeline bind, draws all stimuli

### Dependencies Already in Cargo.toml
```toml
egui       = "0.33"
egui-winit = "0.33"  # desktop only
```

---

## Architecture Overview

### Renderer Separation

We need **two separate render passes**:
1. **Stimulus pass** (existing): Clears background, draws stimuli
2. **egui pass** (new): Draws egui overlay on top with alpha blending

This requires:
- Separate `VkPipeline` for egui
- Separate `GpuBuffers` management for egui mesh data
- Texture atlas handling (egui uses one dynamic texture per font + user images)

### Module Structure

```
render/
  vk/
    context.rs       # unchanged
    pipeline.rs      # unchanged (stimulus pipeline)
    buffers.rs       # unchanged (stimulus buffers)
    frame.rs         # MODIFY: split into two render passes
    egui/            # NEW MODULE
      mod.rs         # pub use pipeline + renderer
      pipeline.rs    # VkEguiPipeline — second pipeline for egui
      renderer.rs    # VkEguiRenderer — manages egui mesh buffers + texture atlas
      shaders.wgsl   # egui vertex + fragment shaders
  drm/
    mod.rs           # MODIFY: add egui state + conditional rendering
  winit_vk/
    mod.rs           # MODIFY: wire up VkEguiRenderer
```

---

## Detailed Design

### 1. `render/vk/egui/shaders.wgsl`

**Inputs:**
- Vertex: `pos: vec2<f32>`, `uv: vec2<f32>`, `color: vec4<f32>` (sRGBA premultiplied alpha)
- Uniform: `screen_size: vec2<f32>` (pixels) as push constant
- Texture: `sampler + texture_2d<f32>` for font atlas

**Vertex shader:**
- Transform `pos` from egui's screen pixels to Vulkan NDC: `x' = (x / screen_width) * 2.0 - 1.0`
- Pass through `uv` and `color`

**Fragment shader:**
- Sample texture at `uv`
- Multiply texture alpha by vertex color alpha
- Output premultiplied alpha `vec4<f32>`

**Compile:** Use `naga` in `build.rs` to compile WGSL → SPIR-V, same as `solid.wgsl`.

---

### 2. `render/vk/egui/pipeline.rs`

```rust
pub struct VkEguiPipeline {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,  // push constant: vec2<f32> screen_size
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub sampler: vk::Sampler,
}

impl VkEguiPipeline {
    pub fn new(device: &ash::Device, render_pass: vk::RenderPass) -> Self;
    pub fn destroy(&self, device: &ash::Device);
}
```

**Pipeline state:**
- Topology: `TRIANGLE_LIST`
- Vertex input: 3 attributes (pos, uv, color)
- Dynamic state: `VIEWPORT`, `SCISSOR`
- Blend: premultiplied alpha: `(ONE, ONE_MINUS_SRC_ALPHA)`
- Cull: `NONE`
- Depth: none (2D overlay, no depth buffer)
- Descriptor set: one combined image sampler for texture atlas
- Push constant: `vec2<f32>` screen size (offset 0, size 8 bytes, vertex stage)

**Sampler:**
- Linear filtering: `VK_FILTER_LINEAR`
- Clamp to edge: `VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE`
- No mipmaps (egui doesn't use them)

---

### 3. `render/vk/egui/renderer.rs`

```rust
pub struct VkEguiRenderer {
    pipeline: VkEguiPipeline,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: HashMap<egui::TextureId, vk::DescriptorSet>,
    textures: HashMap<egui::TextureId, VkEguiTexture>,
    mesh_buffers: VkEguiMeshBuffers,
    mem_props: vk::PhysicalDeviceMemoryProperties,
}

struct VkEguiTexture {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    size: (u32, u32),
}

struct VkEguiMeshBuffers {
    vertex_buffer: vk::Buffer,
    vertex_memory: vk::DeviceMemory,
    vertex_capacity: usize,
    index_buffer: vk::Buffer,
    index_memory: vk::DeviceMemory,
    index_capacity: usize,
}

impl VkEguiRenderer {
    pub fn new(
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        render_pass: vk::RenderPass,
    ) -> Self;

    /// Process egui texture deltas (allocate/free/update textures)
    pub fn update_textures(
        &mut self,
        device: &ash::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        textures_delta: &egui::TexturesDelta,
    );

    /// Upload mesh data for the current frame
    pub fn upload_meshes(
        &mut self,
        device: &ash::Device,
        primitives: &[egui::ClippedPrimitive],
        pixels_per_point: f32,
    );

    /// Record draw calls into an existing command buffer
    pub fn paint(
        &self,
        device: &ash::Device,
        command_buffer: vk::CommandBuffer,
        primitives: &[egui::ClippedPrimitive],
        screen_size_pixels: (u32, u32),
        pixels_per_point: f32,
    );

    pub fn destroy(&mut self, device: &ash::Device);
}
```

**Texture management:**
- `update_textures()` called once per frame with `egui::TexturesDelta`
- Supports:
  - `Set`: allocate new texture or update existing (atlas changes when text changes)
  - `Free`: deallocate texture
- Font atlas: `egui::TextureId::Managed(0)` is the primary font texture (RGBA, sRGB)
- User images: `TextureId::User(u64)` for custom images (not used yet in vstimd)

**Mesh upload:**
- `upload_meshes()` concatenates all `ClippedPrimitive::Mesh` into one vertex/index buffer pair
- Grows buffers if needed (double capacity)
- Uses same `alloc_upload()` pattern as `GpuBuffers` (host-visible, coherent)

**Rendering:**
- `paint()` iterates `ClippedPrimitive`s:
  - `Mesh`: set scissor rect, bind descriptor set for texture, draw indexed
  - `Callback`: not supported (3D callbacks not needed for vstimd overlay)

---

### 4. `render/vk/frame.rs` — Two-Pass Rendering

**Current structure:**
```rust
pub fn render_frame(...) -> Option<FrameTick> {
    // 1. Wait for present
    // 2. Tessellate stimuli into GpuBuffers
    // 3. Acquire swapchain image
    // 4. Record command buffer:
    //    - begin render pass (clear background)
    //    - bind stimulus pipeline
    //    - draw all stimuli
    //    - end render pass
    // 5. Submit + present
}
```

**New structure:**
```rust
pub fn render_frame(
    ctx: &VkContext,
    stimulus_pipeline: &VkPipeline,
    gpu_buffers: &mut GpuBuffers,
    egui_renderer: Option<&mut VkEguiRenderer>,  // None for DRM when overlay disabled
    egui_output: Option<(egui::FullOutput, f32)>,  // (output, pixels_per_point)
    scene: &Arc<RwLock<SceneState>>,
    frame_index: &mut usize,
    frame_stats: &mut FrameStats,
) -> Option<FrameTick> {
    // 1-3: same (wait, tessellate stimuli, acquire)
    
    // 4a. Process egui textures if overlay is active
    if let (Some(renderer), Some((output, ppp))) = (egui_renderer.as_mut(), egui_output.as_ref()) {
        renderer.update_textures(device, queue, command_pool, &output.textures_delta);
        renderer.upload_meshes(device, &primitives, *ppp);
    }

    // 4b. Record command buffer with two subpasses or two render passes
    //     Option A: Two separate render passes (simpler, more explicit)
    //     Option B: One render pass, two subpasses (more efficient, more complex)
    //     → Choose Option A for clarity and compatibility

    unsafe {
        // Begin command buffer
        device.reset_command_buffer(cb, ...);
        device.begin_command_buffer(cb, ...);

        // ── Stimulus pass ──────────────────────────────────────────
        let render_area = vk::Rect2D { ... };
        let clear_value = vk::ClearValue { color: bg };
        let rp_info = vk::RenderPassBeginInfo::default()
            .render_pass(ctx.render_pass)
            .framebuffer(ctx.framebuffers[image_index])
            .render_area(render_area)
            .clear_values(std::slice::from_ref(&clear_value));

        device.cmd_begin_render_pass(cb, &rp_info, vk::SubpassContents::INLINE);
        device.cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, stimulus_pipeline.pipeline);
        device.cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
        device.cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));

        // Draw stimuli
        let sc = scene.read().unwrap();
        for (handle, _) in &sc.stimuli {
            if let Some(mesh) = gpu_buffers.meshes.get(handle) {
                // ... bind buffers, draw indexed
            }
        }
        drop(sc);
        device.cmd_end_render_pass(cb);

        // ── egui pass ─────────────────────────────────────────────
        if let (Some(renderer), Some((output, ppp))) = (egui_renderer, egui_output.as_ref()) {
            // Begin render pass with LOAD_OP_LOAD (preserve stimulus content)
            let clear_value = vk::ClearValue::default();  // unused (LOAD_OP_LOAD)
            let rp_info = vk::RenderPassBeginInfo::default()
                .render_pass(ctx.egui_render_pass)  // NEW: separate render pass
                .framebuffer(ctx.framebuffers[image_index])
                .render_area(render_area)
                .clear_values(std::slice::from_ref(&clear_value));

            device.cmd_begin_render_pass(cb, &rp_info, vk::SubpassContents::INLINE);
            let primitives = ctx.egui_ctx.tessellate(output.shapes.clone(), *ppp);
            renderer.paint(device, cb, &primitives, (extent.width, extent.height), *ppp);
            device.cmd_end_render_pass(cb);
        }

        device.end_command_buffer(cb);
    }

    // 5. Submit + present (same)
}
```

**Render pass consideration:**
- **Option A (recommended):** Two separate render passes
  - Stimulus pass: `VK_ATTACHMENT_LOAD_OP_CLEAR` (clear background)
  - egui pass: `VK_ATTACHMENT_LOAD_OP_LOAD` (preserve stimulus content)
  - Simpler, no subpass dependencies needed
  - Slight overhead (two render pass begin/end), but negligible for vstimd's use case

- **Option B:** One render pass, two subpasses
  - Subpass 0: stimulus rendering
  - Subpass 1: egui rendering (depends on subpass 0 color output)
  - Requires subpass dependency: `VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT` → `VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT`
  - More efficient in theory, but overkill for this workload

**Decision: Use Option A** (two separate render passes) for clarity and ease of debugging.

---

### 5. `render/vk/context.rs` — Add egui Render Pass

Add a second render pass for egui overlay:

```rust
pub struct VkContext {
    // ... existing fields ...
    pub egui_render_pass: vk::RenderPass,  // NEW
}

impl VkContext {
    // In build_context() or a helper:
    pub fn create_egui_render_pass(device: &ash::Device, format: vk::Format) -> vk::RenderPass {
        let attachment = vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)   // preserve stimulus content
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

        let color_ref = vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(std::slice::from_ref(&color_ref));

        let dependency = vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

        let rp_info = vk::RenderPassCreateInfo::default()
            .attachments(std::slice::from_ref(&attachment))
            .subpasses(std::slice::from_ref(&subpass))
            .dependencies(std::slice::from_ref(&dependency));

        unsafe { device.create_render_pass(&rp_info, None).unwrap() }
    }
}

impl Drop for VkContext {
    fn drop(&mut self) {
        unsafe {
            // ... existing cleanup ...
            self.device.destroy_render_pass(self.egui_render_pass, None);
        }
    }
}
```

---

### 6. `render/drm/mod.rs` — Keyboard-Interactive egui

DRM mode gets full keyboard interaction using egui's built-in keyboard navigation.

```rust
pub struct DrmRenderState {
    ctx: VkContext,
    pipeline: VkPipeline,
    gpu_buffers: GpuBuffers,
    egui_renderer: VkEguiRenderer,          // NEW (always present)
    egui_ctx: egui::Context,                // NEW (always present)
    show_overlay: bool,                     // NEW
    // ... rest unchanged ...
}

impl DrmRenderState {
    pub fn new(scene: Arc<RwLock<SceneState>>) -> Self {
        // ... existing init ...

        let egui_renderer = VkEguiRenderer::new(
            &ctx.device,
            &ctx.instance,
            ctx.physical_device,
            ctx.egui_render_pass,
        );
        let egui_ctx = egui::Context::default();

        Self {
            // ...
            egui_renderer,
            egui_ctx,
            show_overlay: false,
        }
    }

    pub fn run_loop(mut self) {
        let mut raw_input = egui::RawInput::default();
        raw_input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(
                self.ctx.extent.width as f32,
                self.ctx.extent.height as f32,
            ),
        ));
        raw_input.pixels_per_point = Some(1.0);

        loop {
            // ... vt poll ...

            // Process input events
            for key in self.input.poll() {
                match key {
                    AppKey::F1 => {
                        self.show_overlay = !self.show_overlay;
                    }
                    AppKey::Escape if self.show_overlay => {
                        self.show_overlay = false;
                    }
                    AppKey::Escape => {
                        self.explicit_cleanup();
                        return;
                    }
                    // Forward to egui when overlay is shown
                    _ if self.show_overlay => {
                        input::push_egui_key_event(&mut raw_input, key, true);
                    }
                    AppKey::D => {
                        crate::render::spawn_demo_stimuli(&self.scene);
                    }
                }
            }

            let egui_output = if self.show_overlay {
                let output = self.egui_ctx.run(raw_input.take(), |ctx| {
                    build_overlay_ui(ctx, &self.scene, &self.frame_stats);
                });
                raw_input = egui::RawInput::default();
                raw_input.screen_rect = Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(
                        self.ctx.extent.width as f32,
                        self.ctx.extent.height as f32,
                    ),
                ));
                raw_input.pixels_per_point = Some(1.0);
                raw_input.time = Some(output.platform_output.elapsed_time);
                Some((output, 1.0))
            } else {
                None
            };

            render_frame(
                &self.ctx,
                &self.pipeline,
                &mut self.gpu_buffers,
                Some(&mut self.egui_renderer),
                egui_output,
                &self.scene,
                &mut frame_index,
                &mut self.frame_stats,
            );
        }
    }

    fn explicit_cleanup(&mut self) {
        // ... existing cleanup ...
        self.egui_renderer.destroy(&self.ctx.device);
    }
}

// In render/drm/input.rs — add new helper:
mod input {
    use super::AppKey;

    pub fn push_egui_key_event(
        raw_input: &mut egui::RawInput,
        key: AppKey,
        pressed: bool,
    ) {
        use egui::{Event, Key};
        let egui_key = match key {
            AppKey::Up => Key::ArrowUp,
            AppKey::Down => Key::ArrowDown,
            AppKey::Left => Key::ArrowLeft,
            AppKey::Right => Key::ArrowRight,
            AppKey::Enter => Key::Enter,
            AppKey::Tab => Key::Tab,
            AppKey::Space => Key::Space,
            _ => return,
        };
        raw_input.events.push(Event::Key {
            key: egui_key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        });
    }
}

// Move build_overlay_ui to render/overlay.rs so both backends can call it
fn build_overlay_ui(
    ctx: &egui::Context,
    scene: &Arc<RwLock<SceneState>>,
    frame_stats: &FrameStats,
) {
    // ... existing UI from winit_vk/mod.rs ...
    // Make it keyboard-navigable:
    // - Use .selectable() for list items
    // - Add keyboard shortcuts to window titles
}
```

**Keyboard navigation:**
- Arrow keys: navigate between widgets
- Tab: next widget
- Enter/Space: activate focused widget (toggle checkbox, click button)
- Escape: close overlay
- F1: toggle overlay on/off

**No mouse needed** — egui supports full keyboard navigation out of the box.

---

### 7. `render/winit_vk/mod.rs` — Wire Up Renderer

```rust
struct State {
    ctx: VkContext,
    pipeline: VkPipeline,
    gpu_buffers: GpuBuffers,
    egui_renderer: VkEguiRenderer,  // NEW
    egui_winit: egui_winit::State,
    window: Arc<Window>,
    scene: Arc<RwLock<SceneState>>,
    frame_stats: FrameStats,
    frame_index: usize,
    egui_ctx: egui::Context,
    show_overlay: bool,
}

impl State {
    fn new(...) -> Self {
        // ... existing init ...
        let egui_renderer = VkEguiRenderer::new(
            &ctx.device,
            &ctx.instance,
            ctx.physical_device,
            ctx.egui_render_pass,
        );

        Self {
            // ...
            egui_renderer,
        }
    }

    fn render(&mut self) {
        let egui_output = if self.show_overlay {
            let raw_input = self.egui_winit.take_egui_input(&self.window);
            let output = self.egui_ctx.run(raw_input, |ctx| {
                build_overlay_ui(ctx, &self.scene, &self.frame_stats);
            });
            self.egui_winit.handle_platform_output(&self.window, output.platform_output);
            let ppp = output.pixels_per_point;
            Some((output, ppp))
        } else {
            None
        };

        let tick = render_frame(
            &self.ctx,
            &self.pipeline,
            &mut self.gpu_buffers,
            Some(&mut self.egui_renderer),
            egui_output,
            &self.scene,
            &mut self.frame_index,
            &mut self.frame_stats,
        );

        // ... handle tick / swapchain recreation ...
    }
}
```

---

## Build System Changes

### `build.rs`

Add egui shaders to `build.rs`:

```rust
fn main() {
    // ... existing protobuf + solid.wgsl compilation ...

    // Compile egui shaders
    let egui_wgsl = std::fs::read_to_string("src/render/vk/egui/shaders.wgsl")
        .expect("failed to read egui shaders.wgsl");

    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );

    let module = naga::front::wgsl::parse_str(&egui_wgsl)
        .expect("failed to parse egui WGSL");

    let info = validator.validate(&module).expect("WGSL validation failed");

    let spv_bytes = naga::back::spv::write_vec(
        &module,
        &info,
        &naga::back::spv::Options {
            lang_version: (1, 0),
            flags: naga::back::spv::WriterFlags::empty(),
            capabilities: None,
            bounds_check_policies: Default::default(),
            zero_initialize_workgroup_memory: false,
            binding_map: Default::default(),
        },
        None,
    )
    .expect("SPIR-V generation failed");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let spv_path = format!("{}/egui.spv", out_dir);
    std::fs::write(&spv_path, bytemuck::cast_slice(&spv_bytes)).unwrap();
    println!("cargo:rerun-if-changed=src/render/vk/egui/shaders.wgsl");
}
```

---

## Implementation Phases

### Phase 1: Core Renderer (winit backend only)
**Goal:** Get egui rendering in winit/desktop mode.

1. Create `render/vk/egui/shaders.wgsl`
2. Update `build.rs` to compile egui shaders
3. Implement `render/vk/egui/pipeline.rs`
4. Implement `render/vk/egui/renderer.rs`
5. Add `egui_render_pass` to `VkContext`
6. Modify `render/vk/frame.rs` to accept egui renderer + output
7. Wire up `render/winit_vk/mod.rs` to use `VkEguiRenderer`
8. Test: F1 overlay in desktop mode

**Success criteria:**
- F1 toggles overlay on/off
- Frame timing window displays FPS
- Stimuli window shows list of stimuli with checkboxes

### Phase 2: DRM Backend Support
**Goal:** Get egui rendering in DRM/console mode with keyboard interaction.

1. Wire up `render/drm/mod.rs` with `VkEguiRenderer`
2. Implement `input::push_egui_key_event()` to forward keyboard to egui
3. F1 key toggles overlay, arrow keys navigate, Enter activates

**Success criteria:**
- F1 toggles overlay in DRM mode
- Arrow keys navigate between widgets
- Enter/Space toggles stimulus checkboxes
- Tab cycles focus
- Escape closes overlay

### Phase 3: Refinements
**Goal:** Polish and optimize.

1. **Texture atlas caching:** Avoid re-uploading font atlas every frame if unchanged
2. **Buffer resizing heuristics:** Smart growth strategy (avoid frequent reallocs)
3. **Descriptor set pooling:** Reuse descriptor sets across frames
4. **Error handling:** Graceful fallback if egui init fails (non-critical feature)
5. **Input integration for DRM:** Investigate touch/mouse via libinput (low priority)

---

## Testing Strategy

### Unit Tests (if feasible)
- Shader compilation in `build.rs`
- Vertex format matches egui's `epaint::Vertex`

### Integration Tests
- Create `tests/egui_render.rs` to verify pipeline creation (no GPU execution)

### Manual Testing
1. **Desktop mode (Linux/Windows):**
   - `cargo run --release`
   - Press F1, verify overlay appears
   - Press D, verify stimuli appear in overlay window
   - Click checkboxes, verify stimuli toggle visibility

2. **DRM mode (Linux, no display server):**
   - SSH into Jetson Nano
   - `sudo systemctl disable --now gdm`
   - `cargo run --release`
   - Press F1, verify overlay appears on monitor
   - Press D, verify stimuli are listed (read-only)

---

## Open Questions / Decisions

### Q1: One render pass or two?
**Decision:** Two separate render passes (stimulus + egui) for simplicity and debuggability.

### Q2: Keyboard interaction on DRM?
**Decision:** Yes — use egui's keyboard navigation. libinput already provides keyboard events. Map:
- Arrow keys → egui focus navigation
- Enter → activate focused widget
- Tab → next widget
- Escape → close overlay

### Q3: Handle egui `PaintCallback`?
**Answer:** Not needed for vstimd's overlay use case (frame timing + stimulus list). Skip for now.

### Q4: sRGB color space?
**Answer:** egui expects sRGB vertex colors. Our swapchain is likely linear (need to verify). Options:
- Convert vertex colors from sRGB → linear in fragment shader
- Use sRGB swapchain format (may not be available on all hardware)
- Ignore and accept slight color inaccuracy (egui overlay is diagnostic, not color-critical)

**Decision:** Convert in fragment shader (safe, portable).

### Q5: DPI scaling on DRM?
**Answer:** Hard to query without compositor. Options:
- Default to `pixels_per_point = 1.0` (96 DPI)
- Read from DRM connector properties (EDID → physical size → DPI)
- Make it configurable via command-line arg

**Decision:** Start with 1.0, add CLI flag later if needed.

---

## Summary

This plan provides a clear path to integrate egui into both rendering backends:
- **winit:** Full input + display (interactive overlay)
- **DRM:** Display-only (read-only diagnostic HUD)

The implementation is split into three phases, with Phase 1 being the immediate priority. The architecture is designed to be maintainable, with clear separation between stimulus rendering and overlay rendering.

**Next steps:**
1. Review this plan
2. Begin Phase 1 implementation
3. Iterate based on testing feedback
