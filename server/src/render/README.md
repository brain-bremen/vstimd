# render module

All GPU and Vulkan (ash) interaction is isolated here. No other module in the codebase should depend on ash directly.

## Responsibilities

- **Shared Vulkan resources** (`render_state.rs`) — `RenderState` holds the `VkContext` (instance, physical device, device, queues) and orchestrates the per-frame render logic for both backends.
- **Vulkan pipelines** (`vk/pipeline.rs`, `vk/grating_pipeline.rs`) — WGSL→SPIR-V compilation (naga, at build time) and Vulkan `VkPipeline` construction for solid-colour and grating stimuli.
- **GPU buffer management** (`vk/buffers.rs`) — uploads tessellated vertex/index data to Vulkan device-local buffers, keyed by stimulus handle.
- **Tessellation** (`tess.rs`) — converts `scene::Stimulus` objects into triangulated vertex/index arrays (CPU-side, no ash dependency itself, but tightly coupled to the vertex format).
- **egui overlay** (`overlay.rs`, feature-gated behind `overlay`) — diagnostic frame-timing HUD rendered on top of the scene using a custom Vulkan egui renderer (`vk/egui/`).
- **DRM/console backend** (`drm/`) — `VK_KHR_display` surface, libinput keyboard, vblank wait with multiple fallback strategies.
- **Desktop backend** (`winit_vk/`) — winit window, `VK_KHR_surface` via ash-window (Wayland/X11/Win32).

## Data flow

```
SceneState (shared via Arc<RwLock<…>>)
    │
    ▼
tess::tessellate_stimulus()  →  (Vec<Vertex>, Vec<u32>)
    │
    ▼
GpuBuffers::upload()         →  Vulkan vertex/index buffers
    │
    ▼
RenderState::render()        →  draw calls + present
```

`RenderState` is the only consumer of scene data for rendering. The scene module defines stimulus types and logic without any GPU awareness.
