# Frame Timing

Precise frame timing is the central guarantee vstimd provides. This page explains what
"frame-accurate" means, how the server achieves it, and how to verify it.

## What is measured

The key metric is **command-to-photon latency**: the time from when a client sends a command
to when the corresponding change appears on screen.

| Stage | Latency |
|---|---|
| ZMQ round-trip (client → server → client) | ~100–400 µs |
| Next vsync wait | 0–1 frame (0–16.7 ms at 60 Hz) |
| GPU render | 0.5–2 ms |
| Display panel response | 1–10 ms |
| **Total (60 Hz)** | **~2–30 ms** |

The dominant term is the vsync wait. Use [deferred mode](deferred-mode.md) to control exactly
which frame a batch of changes becomes visible.

## Vblank synchronisation

The server uses the following priority chain to lock to the display vblank. Each source
is tried in order; on error the source is permanently disabled and the next is used.
The active source is shown in the **System** panel of the debug overlay (F1).

### Priority chain

1. **DRM vblank** (`DRM_IOCTL_WAIT_VBLANK`) — bare-metal DRM mode only. A blocking
   kernel ioctl that fires at the start of the vertical blanking interval. Most precise;
   zero offset from true vblank, minimal kernel overhead.

2. **`VK_EXT_display_control`** (`vkRegisterDisplayEventEXT`) — bare-metal DRM mode
   fallback. Creates a one-shot Vulkan fence that signals on `FIRST_PIXEL_OUT`, which
   is the first active scanline after the blanking interval ends. On a 120 Hz display
   this is approximately **330 µs** after true vblank — a fixed, predictable offset.
   Used on Jetson Orin (Tegra nvdisplay) where `DRM_IOCTL_WAIT_VBLANK` is not
   available when `VK_KHR_display` holds DRM master (the driver does not enable
   vblank IRQs for non-master file descriptors).

3. **`VK_KHR_present_wait`** — Vulkan extension that wakes after the previous frame is
   confirmed presented. Available on most drivers in desktop (winit) mode.

4. **GPU fence completion** — last resort. Wakes when the GPU finishes rendering, not
   when the frame appears on screen. Adds up to one frame of unpredictable latency.
   The overlay labels this *"Clock: GPU-completion (inaccurate)"*.

### Why FIFO acquire is not used as a vblank source

`vkAcquireNextImageKHR` with `FIFO` present mode blocks at the display refresh rate,
but the block point is at **image acquisition** — before tessellation and upload.
This means scene inputs are processed after the sync point; they are stale by one
full frame relative to sources 1–3, which all fire after the previous frame is already
visible and before tessellation begins.

For input-latency-sensitive applications (neuroscience, psychophysics) the DRM /
`VK_EXT_display_control` approach is the correct choice.

### Platform notes

| Platform | Source selected | Reason |
|---|---|---|
| Jetson Orin (nvdisplay) | VK_EXT_display_control | nvdisplay does not enable vblank IRQs for non-master fds |
| Generic Linux desktop | DrmVblank or PresentWait | Depends on driver and present mode |
| Any platform | GpuCompletion | Last resort if no vblank source is available |

### VK_EXT_display_control prerequisites

The extension requires three things at init time:

1. `VK_EXT_display_surface_counter` enabled as an **instance** extension.
2. `VK_EXT_display_control` enabled as a **device** extension.
3. `VkSwapchainCounterCreateInfoEXT` with `VK_SURFACE_COUNTER_VBLANK_BIT_EXT` chained
   into swapchain creation — without this, `vkRegisterDisplayEventEXT` returns
   `ERROR_UNKNOWN`.

These are handled automatically by `drm/init.rs` and `vk/context.rs`; the extension is
only activated when the driver advertises all required capabilities.

## Present mode

The server always uses `VK_PRESENT_MODE_FIFO_KHR` (vsync, no tearing) in production.
`MAILBOX_KHR` (triple-buffer) is available via `--present-mode mailbox` for benchmarking
only — it reduces vsync wait by up to one frame but makes it impossible to determine which
frame a deferred flip was first visible on.

## Debug overlay

Press **F1** to open the overlay. The **Frame Timing** panel shows:

- FPS and measured frame duration
- Per-frame sparkline (red bars = missed vblank)
- Phase breakdown: tessellate / upload / fence / acquire / record / submit (in µs)
- Drop count and jitter

## Hardware timing test

The `tools/timing_test` tool drives the server from Python and records photodiode responses
with a DAQ to measure render-to-photon latency precisely:

```sh
cd tools/timing_test
uv run python -m vstimd_timing_test --backend auto --hz 60 --duration 5 --out result.csv
```

| Metric | PASS | WARN | FAIL |
|---|---|---|---|
| Dropped frames (in 300) | 0 | 1–2 | ≥ 3 |
| Jitter (std) | < 0.3 ms | 0.3–1.0 ms | > 1.0 ms |
| Render-to-photon latency | < 10 ms | 10–20 ms | > 20 ms |
