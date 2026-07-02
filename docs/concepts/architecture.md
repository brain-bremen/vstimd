# Architecture

## Overview

vstimd has a client-server architecture. The server owns the display and renders stimuli;
clients connect over TCP and send commands using [protobuf](../protocol/index.md) over ZMQ.

```
┌──────────────────────────────────────────────────────┐
│  vstimd server                                       │
│                                                      │
│  ┌──────────────┐   Arc<RwLock<SceneState>>          │
│  │  ZMQ thread  │ ──────────────────────────┐        │
│  │  (tokio)     │                           ▼        │
│  └──────────────┘                   ┌──────────────┐ │
│        ▲ TCP:5555                   │ Render thread │ │──► Display
│        │ protobuf                   │ (Vulkan/DRM)  │ │
│  ┌─────┴────────┐                   └──────────────┘ │
│  │ Python /     │                                    │
│  │ MATLAB / C#  │                                    │
│  └──────────────┘                                    │
└──────────────────────────────────────────────────────┘
```

## Threads

| Thread | Role |
|---|---|
| **Render** (main thread) | Vulkan render loop, vsync-locked. Holds write lock once per frame for tessellation, then read lock for draw. |
| **ZMQ server** (background) | Accepts client connections, decodes protobuf requests, calls `SceneState::handle_request`, encodes responses. Holds write lock per command. |

The `RwLock` write lock is dropped before the render pass begins, so the ZMQ thread always
has a window to process commands between frames.

## Rendering backends

| Backend | When | Surface |
|---|---|---|
| **DRM/console** | Linux, no display server | `VK_KHR_display` — direct KMS/DRM, no compositor |
| **Desktop** | Linux with X11/Wayland, or Windows | `VK_KHR_surface` via ash-window + winit |
| **Null** | `--null` flag | No display, ZMQ server only |

Auto-detection checks `DISPLAY` / `WAYLAND_DISPLAY` environment variables at startup.

## Render loop (per frame)

```
acquire swapchain image
  │
  ├── deferred flip (if pending)     ← atomically promote staged changes
  │
  ├── tessellate dirty stimuli       ← CPU: lyon → Vec<Vertex>
  │
  ├── upload changed GPU buffers     ← PCIe DMA
  │
  ├── Vulkan render pass
  │     ├── clear to background colour
  │     ├── draw stimuli (insertion order)
  │     └── egui overlay (if F1 visible)
  │
  ├── vkQueuePresentKHR
  │
  └── vblank wait
        ├── DRM vblank (preferred, bare-metal)
        ├── VK_KHR_present_wait
        ├── VK_GOOGLE_display_timing
        └── GPU fence completion (fallback)
```

## Scene state

`SceneState` holds all stimulus data and is the only shared mutable state between threads.
Stimuli are stored as a flat `IndexMap<u32, Stimulus>` where the key is the server-assigned
handle returned to the client on creation.

Each stimulus is a variant of the `Stimulus` enum — no trait objects, no heap allocation per
stimulus. Shared fields (position, colour, enabled flag) are held in component structs
(`Transform2D`, `ShapeAppearance`, `StimulusFlags`) composed into each variant.

