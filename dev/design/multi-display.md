# Multi-Display Architecture

## Current Status (Single Display)

As of the egui overlay implementation, vstimd renders to a single display:
- **One `VkContext`** with one surface, one swapchain, one set of framebuffers
- **One render pass** per frame (stimuli + optional egui overlay)
- **DRM backend:** Acquires all displays to prevent CRTC blanking, but renders to `display[0]` only
- **Winit backend:** One window on the primary monitor

The egui overlay composites directly into the same framebuffer as the stimuli.

## Design Goal: Multi-Display Support

**Requirement:** vstimd should take control of all connected displays and allow:
1. **Stimuli on all displays** (mirrored or independent scenes)
2. **egui overlay on any display** (selectable via CLI or API)
3. **Independent refresh rates** per display (if hardware supports it)

### Use Cases

| Scenario | Stimulus Display(s) | Overlay Display |
|---|---|---|
| **Single-display debug** | Display 0 | Display 0 (composited) |
| **Dual-display experiment** | Display 0 (subject) | Display 1 (experimenter) |
| **Multi-projector setup** | Displays 0–2 (tiled scene) | Display 3 (control panel) |

---

## Proposed Architecture

### 1. Refactor `VkContext`

Split per-display state into a `DisplayContext` struct:

```rust
pub struct DisplayContext {
    pub surface: vk::SurfaceKHR,
    pub swapchain: vk::Swapchain,
    pub swapchain_loader: ash::khr::swapchain::Device,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub extent: vk::Extent2D,
    pub format: vk::Format,
    pub present_mode: vk::PresentMode,
    pub frames: Vec<FrameSync>,  // per-display sync objects
}

pub struct VkContext {
    pub displays: Vec<DisplayContext>,  // One per physical display
    pub render_pass: vk::RenderPass,
    pub egui_render_pass: vk::RenderPass,
    pub command_pool: vk::CommandPool,  // Shared pool
    pub graphics_queue: vk::Queue,
    pub graphics_queue_family: u32,
    pub device: ash::Device,
    pub physical_device: vk::PhysicalDevice,
    pub instance: ash::Instance,
    pub entry: ash::Entry,
    // ... other shared state
}
```

### 2. Render Loop Changes

**Current (single display):**
```rust
render_frame(ctx, pipeline, gpu_buffers, scene, ...)
```

**Multi-display:**
```rust
for (display_index, display) in ctx.displays.iter_mut().enumerate() {
    render_frame_to_display(
        ctx,               // Shared device/queue
        display,           // Per-display surface/swapchain
        display_index,
        pipeline,
        gpu_buffers,
        scene,
        egui_renderer,
        egui_data,         // Only render if overlay_display == display_index
    );
}
```

**Key changes:**
- `render_frame_to_display()` accepts a `&mut DisplayContext` instead of pulling from `VkContext`
- Each display has its own `acquire_next_image` → record → submit → present cycle
- **Synchronization:** Use one fence per display; wait on all fences before starting the next frame
- **Present:** Multiple `vkQueuePresentKHR` calls (one per swapchain), or use the multi-swapchain feature if supported

### 3. DRM Backend: Enumerate All Displays

**Current behavior:**
- `vkEnumeratePhysicalDeviceDisplaysKHR` → `all_displays`
- `vkAcquireDrmDisplayEXT` on all displays (to prevent CRTC blanking)
- Create surface for `all_displays[0]` only

**Multi-display:**
- Acquire all displays (already done)
- **For each display:**
  - Pick a mode via `vkGetDisplayModePropertiesKHR`
  - Find a compatible plane via `vkGetDisplayPlaneSupportedDisplaysKHR`
  - Call `vkCreateDisplayPlaneSurfaceKHR`
  - Create swapchain
- Store all `DisplayContext` instances in `VkContext::displays`

### 4. Winit Backend: Multiple Windows

**Current behavior:**
- One window created in `WinitApp::resumed()`
- `ApplicationHandler::window_event()` dispatches to that window

**Multi-display:**
- Enumerate monitors via `event_loop.available_monitors()`
- Create one window per monitor (or per CLI-specified display)
- Each window gets its own Vulkan surface
- `window_event()` must route events by `WindowId`
- egui overlay rendered only to the window designated as `overlay_display`

**Keyboard/mouse routing:**
- Stimuli render to all displays
- Input only sent to egui if `event.window_id == overlay_window_id`

### 5. CLI Interface

```
cargo run --release -- \
  --displays all \
  --overlay-display 1 \
  --fullscreen
```

| Flag | Description |
|---|---|
| `--displays all` | Render stimuli to all connected displays (default) |
| `--displays 0` | Render only to display 0 |
| `--displays 0,2` | Render to displays 0 and 2 |
| `--overlay-display 1` | Show egui overlay on display 1 |
| `--no-overlay` | Disable overlay entirely |

### 6. Scene API Extensions (Future)

Allow stimuli to specify target display(s):

```python
conn = vstimd_client.Connection()
rect = conn.create_rect(x=0, y=0, width=100, height=100, displays=[0, 1])  # Render to both
conn.set_display_mask(rect, [0])  # Move to display 0 only
```

**Implementation:** Add `display_mask: u32` bitfield to `StimulusFlags` in the protobuf schema.

---

## Implementation Plan

### Phase 1: Refactor `VkContext` (Breaking Change)
- [ ] Extract `DisplayContext` struct
- [ ] Update `build_context` to accept a `Vec<DisplayInfo>` and create multiple surfaces
- [ ] Update `render_frame` → `render_frame_to_display` signature
- [ ] Update `recreate_swapchain` to operate on a specific display index

### Phase 2: DRM Multi-Surface Creation
- [ ] Loop over `all_displays` and create one surface per display
- [ ] Store all `DisplayContext` in `VkContext::displays`
- [ ] Modify `DrmRenderState::run_loop` to render to all displays

### Phase 3: Winit Multi-Window Support
- [ ] Enumerate monitors and create one window per display
- [ ] Route `WindowEvent` by `WindowId`
- [ ] Render to all windows; overlay only on designated display

### Phase 4: CLI + Configuration
- [ ] Add `--displays` and `--overlay-display` flags
- [ ] Parse display selection and pass to `DrmRenderState::new()` / `WinitApp::new()`

### Phase 5: Per-Display Scene Rendering (Optional)
- [ ] Add `display_mask` to `StimulusFlags`
- [ ] Filter draw calls in `render_frame_to_display` based on mask
- [ ] Expose `set_display_mask` in Python client

---

## Technical Challenges

### 1. Synchronization
**Problem:** Multiple swapchains need independent frame pacing but shared scene updates.

**Solution:**
- Use one fence per display (already done in `FrameSync`)
- Wait on **all** display fences before starting scene `update()`
- Present all displays in sequence (slowest display sets the pace)

### 2. Refresh Rate Mismatch
**Problem:** Display 0 runs at 60 Hz, Display 1 at 120 Hz.

**Options:**
- **Option A (simplest):** Lock all displays to the **lowest common refresh rate** (e.g., 60 Hz)
- **Option B (complex):** Render each display at its native rate; duplicate stimuli frames for slower displays
- **Recommendation:** Start with Option A; implement Option B only if scientifically required

### 3. Overlay Input Routing (Winit)
**Problem:** Mouse/keyboard events arrive per-window; egui expects one event stream.

**Solution:**
- Only call `egui_winit.on_window_event()` for events from the overlay window
- Ignore input from stimulus-only windows

### 4. DRM Plane Assignment
**Problem:** On some hardware, each display requires a specific plane index.

**Current code:**
```rust
let plane_index = (0..plane_props.len() as u32)
    .find(|&i| /* check plane supports vk_display */)
    .unwrap_or(0);
```

**Multi-display:** Store `(display, plane_index)` pairs; iterate when creating surfaces.

---

## Testing Strategy

1. **Single-display regression:** Verify current behavior still works after refactor
2. **DRM dual-display:** Test on hardware with 2 monitors (Jetson Nano + external HDMI)
3. **Winit dual-window:** Test on desktop Linux with 2 monitors
4. **Overlay routing:** Press F1 and verify overlay appears on the correct display
5. **Refresh rate lock:** Measure frame timing on mismatched displays

---

## References

- `server/src/render/vk/context.rs` — Current single-display `VkContext`
- `server/src/render/drm/init.rs` — DRM display enumeration and acquisition
- `server/src/render/winit_vk/mod.rs` — Winit window creation
- Vulkan spec: `VK_KHR_display` multi-display examples
- [egui multi-viewport](https://docs.rs/egui/latest/egui/viewport/index.html) — For future multi-window UI
