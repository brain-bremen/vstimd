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

The server uses the following priority chain to lock to the display vblank:

1. **DRM vblank** (`drmWaitVBlank`) — used in bare-metal DRM mode. Most precise; hardware
   interrupt from the display controller.
2. **`VK_KHR_present_wait`** — Vulkan extension, blocks until the swapchain image is
   displayed. Available on most drivers in desktop mode.
3. **`VK_GOOGLE_display_timing`** — predicted presentation timestamps.
4. **GPU fence completion** — fallback; waits for the GPU to finish, not for the actual flip.

The active clock source is shown in the **System** panel of the debug overlay (F1).

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
