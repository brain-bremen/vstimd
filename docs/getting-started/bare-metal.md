# Bare-Metal Linux Rendering

> **Status:** Implemented (Jetson Orin Nano / Jetson Nano) / Planned (Raspberry Pi 5)
> **Last updated:** 2025-05-05

Run vstimd without a compositor on Linux using KMS/DRM for display ownership and raw Vulkan for rendering. No X11, no Wayland, no display server required.

---

## Target Platforms

| Platform | Status | Display API | Notes |
|---|---|---|---|
| NVIDIA Jetson Nano | Working | `VK_KHR_display` | Similar architecture to Orin |
| NVIDIA Jetson Orin Nano | Working | `VK_KHR_display` | See setup below |
| Raspberry Pi 5 | Planned | `VK_EXT_acquire_drm_display` (expected) | Hardware not yet available |

---

## Motivation

The windowed stack (`winit` + Vulkan surface) assumes a display server is running. For latency-sensitive psychophysics experiments on dedicated hardware — headless servers, embedded systems, single-board computers — the compositor is an unnecessary layer that adds scheduling jitter and prevents direct vblank control.

The bare-metal path removes the compositor entirely, giving the process exclusive ownership of the display plane and deterministic frame timing.

---

## Platform-Specific Setup

### NVIDIA Jetson Orin Nano (L4T R36.x / JetPack 6.x)

#### Hardware architecture

The Jetson Orin Nano has a **split DRM node architecture** — the GPU and display controller are separate hardware blocks:

| DRM node | Hardware | Role |
|---|---|---|
| `card0` / `renderD128` | nvgpu (`13e00000.host1x`) | GPU — Vulkan rendering |
| `card1` / `renderD129` | nvdisplay (`13800000.display`) | Display controller — scanout, KMS connectors |

`VK_EXT_acquire_drm_display` does **not** work on this hardware because that extension requires the Vulkan physical device and the DRM display node to be the same hardware node. They are not. Use `VK_KHR_display` instead — the Vulkan driver enumerates and drives the display controller directly without a DRM fd.

#### One-time kernel configuration

`nvidia-drm` must be loaded with `modeset=1` for the display controller to register as `card1`. Without this, `card1` does not exist and `vkGetPhysicalDeviceDisplayPropertiesKHR` returns `VK_ERROR_UNKNOWN`.

Make it permanent:

```bash
echo 'options nvidia-drm modeset=1 fbdev=1' | sudo tee /etc/modprobe.d/nvidia-drm.conf
sudo reboot
```

`fbdev=1` additionally creates `/dev/fb0`, enabling a framebuffer console on the physical display when no Vulkan app is running.

#### Running without a display manager

GDM (or any compositor) must not be running — it holds the display and `VK_KHR_display` will fail with `VK_ERROR_UNKNOWN`. Stopping GDM alone is not sufficient; logind also holds the seat's DRM master reference and must be released:

```bash
sudo systemctl stop gdm
sudo loginctl terminate-seat seat0
```

Then run the server:

```bash
cd ~/src/vstimd
cargo run --release
```

To restore the desktop afterwards:

```bash
sudo systemctl start gdm
```

#### Persistent headless setup (no desktop)

If the machine is dedicated to vstimd and you never need a desktop:

```bash
sudo systemctl disable gdm
sudo systemctl set-default multi-user.target
```

The physical display will show a framebuffer console at boot (requires `fbdev=1` above) and the app takes over the display when launched.

#### Permissions

The running user needs:
- `video` group — DRM access to `/dev/dri/card0`, `/dev/dri/card1`
- `input` group — libinput access to `/dev/input/*`

#### Known driver issue: VRR causes GPU device loss (JetPack 6.x)

**Symptom:** vstimd renders 3–4 frames then crashes with `ERROR_DEVICE_LOST`. The kernel log shows:

```
nvidia-modeset: ERROR: GPU:0: nvRmApiAlloc(memory) failed for vrr 0x22
nvidia-modeset: ERROR: GPU:0: Failed to setup Rgline active session for vrr
nvgpu: ga10b_pbdma_handle_intr_0_acquire: semaphore acquire timeout!
```

**Root cause:** When `VK_KHR_display` acquires a VRR-capable (G-Sync / Adaptive Sync) display, `nvidia-modeset` automatically attempts to allocate a VRR "Rgline active session". On JetPack 6.x (driver 540.x) this allocation fails with error `0x22` and leaves the GPU presentation semaphore pathway in a corrupted state. The PBDMA engine times out waiting on the semaphore 4 seconds later, causing `ERROR_DEVICE_LOST`.

**Fix:** Disable HDMI FRL (Fixed Rate Link), the HDMI 2.1 transport that enables the high-bandwidth modes required for VRR. Disabling it forces HDMI 2.0 TMDS, which has ample bandwidth for 4K@60Hz but does not expose VRR capability to `nvidia-modeset`.

Apply immediately (takes effect without reboot):

```bash
sudo sh -c 'echo 1 > /sys/module/nvidia_modeset/parameters/disable_hdmi_frl'
```

Make it permanent:

```bash
echo 'options nvidia-modeset disable_hdmi_frl=1' | sudo tee /etc/modprobe.d/nvidia-modeset-nofrl.conf
```

**Remaining benign log noise after the fix:**

| Message | Source | Meaning |
|---|---|---|
| `Failed to set variable refresh rate with invalid minimum frame time` | `nvidia-modeset` | Driver detects VRR range is inconsistent (30 Hz min > 48 Hz max) and skips VRR setup cleanly. Harmless. |
| `nvdisplayr: secure read @0x000000ffffffff00: EMEM address decode error` | `tegra-mc` | Display controller briefly reads from a stale framebuffer address during CRTC restore on exit (GDM's original framebuffer was freed when GDM was stopped). Harmless after exit. |

---

### Raspberry Pi 5

> **Placeholder — hardware not yet available.**
>
> Expected approach: standard `vc4`/`v3d` KMS drivers, `VK_EXT_acquire_drm_display` via Mesa v3dv ICD. Setup notes to be written once the device is in hand.

---

## Architecture

The DRM backend uses:
- **ash** (raw Vulkan bindings) for all GPU operations
- **drm** crate for display discovery (enumerate connectors/modes)
- **input** crate (libinput) for keyboard/mouse events
- **VK_KHR_display** or **VK_EXT_acquire_drm_display** for surface creation (platform-dependent)

Shaders are written in GLSL and compiled to SPIR-V at build time.

---

## Dependencies (Linux-only)

| Crate | Version | Purpose |
|---|---|---|
| `ash` | ~0.38 | Raw Vulkan bindings |
| `drm` | ~0.15 | DRM device open + display enumeration |
| `input` | ~0.9 | libinput keyboard/mouse events |
| `libc` | ~0.2 | VT switching, ioctl calls |

---

## Permissions

The process needs:
- `video` group — DRM master access to `/dev/dri/card0`
- `input` group — libinput access to `/dev/input/*`
- Or run as root for test deployments

On minimal embedded systems without udev, use `Libinput::new_from_path` to open `/dev/input/eventN` devices directly (eliminates the udev transitive dependency).

---

## Implementation Status

**✅ Implemented:**
- DRM display discovery and mode selection
- Vulkan initialization via `VK_KHR_display` (Jetson)
- Vulkan swapchain creation (FIFO present mode)
- Render pass and solid-colour pipeline
- GPU buffer management (host-coherent memory)
- libinput keyboard/mouse handling
- Main render loop with vsync
- Auto-detection of console vs desktop mode

**🚧 Partial:**
- egui overlay (context wired, Vulkan renderer TODO)

**📋 Planned:**
- Raspberry Pi 5 support (`VK_EXT_acquire_drm_display`)
- Textured pipeline for bitmap stimuli
- Custom shader pipeline

---

## Module Structure

| Module | Contents |
|---|---|
| `server/src/render/drm/mod.rs` | Entry point, auto-detection, `DrmRenderState` |
| `server/src/render/drm/display.rs` | DRM display discovery |
| `server/src/render/drm/input.rs` | libinput keyboard/mouse handling |
| `server/src/render/winit_vk/mod.rs` | Desktop backend entry point, `WinitVkRenderState` |
| `server/src/render/vk/` | Shared Vulkan code (both backends) |
| `server/src/render/vk/context.rs` | `VkContext`: instance, device, queue |
| `server/src/render/vk/pipeline.rs` | `VkPipeline`: render pass, pipeline, shaders |
| `server/src/render/vk/gpu_buffers.rs` | `GpuBuffers`: vertex/index buffer management |
| `server/src/render/vk/tess.rs` | Tessellation (Rect/Circle/Ellipse) |
| `server/src/render/vk/frame.rs` | Frame rendering logic |

---

## Running

```bash
# Auto-detected mode
cargo run --release
# On Linux without DISPLAY/WAYLAND_DISPLAY: uses DRM mode
# On Linux with display server: uses desktop mode  
# On Windows: always uses desktop mode

# Test ZMQ client/server protocol
cd client/python && uv run examples/flash_rects.py

# Verify input:
# - D: spawn demo stimuli
# - F1: toggle overlay (desktop mode only)
# - ESC: clean exit
# - Alt+Enter: toggle fullscreen (desktop mode only)
```
