# Bare-Metal Linux Rendering

> **Status:** Planned
> **Last updated:** 2026-04-14

Run Wonderlamp without a compositor on Linux using KMS/DRM for display ownership and raw Vulkan for rendering. No X11, no Wayland, no display server required.

---

## Motivation

The default stack (`winit` + `wgpu`) assumes a display server is running. For latency-sensitive psychophysics experiments on dedicated hardware — headless servers, embedded systems, single-board computers — the compositor is an unnecessary layer that adds scheduling jitter and prevents direct vblank control.

The bare-metal path removes the compositor entirely, giving the process exclusive ownership of the display plane and deterministic frame timing.

---

## Architecture Options

### Option A — wgpu + DRM (keep wgpu, drop winit)

`raw-window-handle` 0.6 defines `DrmWindowHandle` / `DrmDisplayHandle`. wgpu 27's Vulkan HAL accepts these via `SurfaceTargetUnsafe::RawHandle`, which causes it to call `vkCreateDisplayPlaneSurfaceKHR` internally. This keeps WGSL shaders and the entire existing render pipeline; only `winit` is dropped.

**Trade-off:** wgpu pulls in ~120 transitive crates. The DRM surface path in wgpu is supported but not heavily exercised; it may have rough edges.

### Option B — ash + KMS/DRM (recommended)

Drop `wgpu` entirely. Use `ash` (raw Vulkan bindings), the `drm` crate only to discover the display (open the fd, enumerate connectors/modes), then hand the display lease to Vulkan via `VK_EXT_acquire_drm_display` and drop the `drm` handle. Use `vkCreateDisplaySurfaceKHR` for the surface. Use the `input` crate (libinput) for keyboard input. The WGSL shader is compiled to SPIR-V at build time by `naga` (a build-dep, zero runtime cost).

**Trade-off:** ~600–800 lines of new code, but dramatically fewer runtime dependencies and full Vulkan control. Matches the "low-level, minimal deps" design intent.

**Recommendation: Option B.**

---

## New Dependencies (Linux-only feature)

| Crate | Version | Purpose | Replaces |
|---|---|---|---|
| `ash` | ~0.38 | Raw Vulkan bindings | `wgpu` |
| `drm` | ~0.14 | DRM device open + display enumeration | winit display backend |
| `input` | ~0.9 | libinput keyboard/mouse events | winit input |
| `naga` | ~23 | WGSL → SPIR-V at build time (build-dep only) | — |

---

## Feature Layout

```toml
[features]
default      = ["wgpu-backend"]
wgpu-backend = ["dep:wgpu", "dep:winit", "dep:pollster", "overlay"]
overlay      = ["dep:egui", "dep:egui-wgpu", "dep:egui-winit"]
drm          = ["dep:ash", "dep:drm", "dep:input"]
# drm and overlay are mutually exclusive (enforced in build.rs)
```

Build commands:

```bash
# Dev / Windows (unchanged):
cargo build --release

# Bare-metal Linux:
cargo build --release --no-default-features --features drm
```

---

## Permissions

The process needs:
- `video` group — DRM master access to `/dev/dri/card0`
- `input` group — libinput access to `/dev/input/*`
- Or run as root for test deployments

On minimal embedded systems without udev, use `Libinput::new_from_path` to open `/dev/input/eventN` devices directly (eliminates the udev transitive dependency).

---

## Implementation Plan

### Phase A — Preparation (no behaviour change)

**A1. Extract `Vertex` to `server/src/render/vertex.rs`**
Move the `Vertex` struct (currently `tess.rs:10-15`) into its own file shared by both backends. Update `tess.rs` and `gpu_buffers.rs` to `use super::vertex::Vertex`.

**A2. Extract WGSL shader to `server/shaders/solid.wgsl`**
Move the `WGSL_SHADER` inline string from `pipeline.rs:3-23` to a file. Reference it with `include_str!` in the wgpu path. This file is the SPIR-V source for the drm path.

**A3. Add naga SPIR-V emission in `build.rs`**
Gated on `CARGO_FEATURE_DRM`: read `shaders/solid.wgsl`, use naga to parse and emit SPIR-V for `vs_main` and `fs_main`, write `$OUT_DIR/solid.spv`.

**A4. Update `server/Cargo.toml`**
Make `wgpu`, `winit`, `pollster` optional; add `ash`, `drm`, `input` as optional; add `naga` to build-dependencies; define features as above.

**A5. Gate `app.rs` out of drm builds**
Add `#[cfg(not(feature = "drm"))]` to `pub mod app` in `lib.rs` and to wgpu-specific sub-modules in `render/mod.rs`.

---

### Phase B — DRM Display Discovery (`server/src/render/drm/display.rs`)

1. Open `/dev/dri/card0` via `drm::card::Card::open()`
2. Enumerate connectors via `device.resource_handles()`
3. Find first connected connector; read preferred/current mode
4. Expose `DisplayInfo { drm_fd: RawFd, connector_id: u32, width: u32, height: u32 }`

The `drm::Card` handle stays alive until Vulkan acquires the display, then is dropped.

---

### Phase C — Vulkan Init (`server/src/render/drm/vk_init.rs`)

**Instance extensions:**
- `VK_KHR_surface`, `VK_KHR_display`, `VK_EXT_acquire_drm_display`, `VK_KHR_get_physical_device_properties2`

**Physical device:** first device supporting `VK_KHR_display` with a graphics queue

**Acquire DRM display:**
```
vkAcquireDrmDisplayEXT(physical_device, drm_fd, vk_display)
```
Drop the `drm::Card` after this call — Vulkan owns the display.

**Surface:** `vkCreateDisplaySurfaceKHR` with the display mode matching the connector's resolution

**Logical device:** `VK_KHR_swapchain`

**Swapchain:** `FIFO` present mode, 2 images, `B8G8R8A8_UNORM` / `SRGB_NONLINEAR`

**Per-frame sync:** semaphores (image_available, render_done) + fences × 2 frames-in-flight

> `VK_EXT_acquire_drm_display` is available on Mesa (AMD/Intel/Nouveau) and NVIDIA proprietary ≥ 470. A clear startup error lists available extensions if the device does not support it.

---

### Phase D — Render Pass & Pipeline (`server/src/render/drm/vk_pipeline.rs`)

- **Render pass:** single color attachment, `CLEAR` on load, `STORE` on done, `PRESENT_SRC_KHR` final layout
- **Shader modules:** `include_bytes!(concat!(env!("OUT_DIR"), "/solid.spv"))` for both entry points
- **Pipeline layout:** empty (no descriptors, no push constants)
- **Graphics pipeline:**
  - Vertex stride = 24 bytes (`size_of::<Vertex>()`); location 0 = `vec2` at offset 0, location 1 = `vec4` at offset 8
  - Topology: `TRIANGLE_LIST`
  - Viewport/scissor: dynamic state
  - Blend: `SRC_ALPHA` / `ONE_MINUS_SRC_ALPHA` (mirrors wgpu `ALPHA_BLENDING`)

---

### Phase E — GPU Buffers (`server/src/render/drm/vk_buffers.rs`)

- Memory allocator helper: selects `HOST_VISIBLE | HOST_COHERENT` memory type
- `GpuMesh { vertex_buffer, vertex_memory, index_buffer, index_memory, index_count }`
- `GpuBuffers` (HashMap keyed by stimulus handle): `upload` → `vkMapMemory` / memcpy / `vkUnmapMemory`

Host-coherent memory eliminates flush/barrier boilerplate. Adequate for current stimulus counts; can be upgraded to device-local + staging if profiling demands it.

---

### Phase F — Input (`server/src/render/drm/input.rs`)

- `Libinput::new_with_udev(interface, None)` on `"seat0"` (or `new_from_path` for udev-free systems)
- `poll()` called once per frame — non-blocking `dispatch()` + event drain
- Key mapping: `KEY_ESC` → shutdown, `KEY_F1` → toggle overlay flag, `KEY_D` → demo stimuli

---

### Phase G — Frame Loop (`server/src/render/drm/vk_frame.rs`)

Replaces the winit event loop with a plain `loop {}`:

```
loop {
    input.poll()                        // handle key events; ESC breaks
    scene.write() → tessellate → upload // same logic as wgpu update()
    vkAcquireNextImageKHR               // wait=image_available semaphore
    record command buffer               // clear → bind pipeline → draw_indexed per mesh
    vkQueueSubmit                       // wait=image_available, signal=render_done
    vkQueuePresentKHR                   // wait=render_done
    vkWaitForFences                     // FIFO blocks at vblank here
    frame_stats.on_present()
}
```

`FIFO` present mode is the vsync gate — no sleep, no busy-spin. CPU stays one frame ahead of GPU.

---

### Phase H — Public Interface (`server/src/render/drm/mod.rs`)

```rust
pub struct RenderState { /* vk, buffers, input, scene, frame_stats */ }

impl RenderState {
    pub fn new(scene: Arc<RwLock<SceneState>>) -> Self
    pub fn run_loop(mut self)   // blocks until ESC
}
```

Same public surface as the wgpu `RenderState`.

---

### Phase I — `main.rs` Branch

```rust
fn main() {
    let scene = Arc::new(RwLock::new(SceneState::new()));
    let _zmq = ipc::spawn_zmq_thread(Arc::clone(&scene), "tcp://0.0.0.0:5555");

    #[cfg(feature = "drm")]
    render::RenderState::new(scene).run_loop();

    #[cfg(not(feature = "drm"))]
    { /* existing winit path unchanged */ }
}
```

---

## Files Changed

| File | Change |
|---|---|
| `server/Cargo.toml` | Make wgpu/winit/pollster optional; add ash/drm/input; add naga build-dep; define features |
| `server/build.rs` | SPIR-V emission (naga, gated on `CARGO_FEATURE_DRM`) |
| `server/shaders/solid.wgsl` | **NEW** — extracted from `pipeline.rs` |
| `server/src/render/vertex.rs` | **NEW** — `Vertex` struct from `tess.rs` |
| `server/src/render/mod.rs` | cfg-gate wgpu/drm sub-modules |
| `server/src/render/tess.rs` | Use `super::vertex::Vertex` |
| `server/src/render/drm/` | **NEW** — Phases B–H |
| `server/src/main.rs` | `#[cfg(feature = "drm")]` branch |
| `server/src/lib.rs` | Gate `pub mod app` on `not(feature = "drm")` |

**Unchanged:** `scene/`, `ipc.rs`, `proto.rs`, `render/tess.rs` (logic), `render/pipeline.rs`, `render/state.rs`, `render/gpu_buffers.rs`, `render/overlay.rs`

---

## Verification

```bash
# Build bare-metal target (on Linux)
cargo build --release --no-default-features --features drm

# Confirm no winit/wgpu symbols linked
nm -D target/release/wonderlamp_server | grep -i 'winit\|wgpu' | wc -l
# → 0

# Run on bare tty (no X/Wayland session)
sudo ./target/release/wonderlamp_server

# Verify ZMQ still works
cd client-python && uv run examples/flash_rects.py

# Verify input: D → demo stimuli, F1 → overlay toggle, ESC → clean exit

# Regression: wgpu path unchanged
cargo build --release
cargo test
cargo clippy
```
