# VStim v3

A Rust rewrite of the [StimServer](https://github.com/) C++ visual stimulus server, combined with ideas from the **VStim** project.

The original StimServer used MFC/C++/Direct3D11 with a client/server architecture driven by Windows named pipes with binary messages. This project ports that architecture to Rust, targeting Linux as the primary deployment platform, replacing named pipes with ZeroMQ for cross-platform IPC, and adding a modern GPU rendering stack.

## Goals

- Spiritual successor to the C++ StimServer, using ZeroMQ in place of Windows named pipes for cross-platform IPC
- GPU-accelerated 2-D and 3-D stimulus rendering via **wgpu**
- Low-latency rendering loop with no blocking on the render thread
- Deterministic event logging for experiment replay and analysis
- Shared-memory position input (gaze/joystick) to avoid ZeroMQ round-trip latency

## Current Status

The project is in early development. The current code is a 2-D Bézier rendering demo: running `cargo run` opens a window showing a 5-lobe amoeba shape that breathes and slowly rotates, with a colour gradient anchored to world angle.

The `extern/` directory contains git submodules for external references:

- `extern/StimServer/` — the original C++ reference implementation
- `extern/psychopy/` — PsychoPy, referenced for stimulus design ideas

## Demo

```sh
cargo run
```

Opens a window with an animated amoeba shape. No ZeroMQ server yet — stimulus parameters are hard-coded in `main.rs`.

## Stack

| Crate | Role |
|---|---|
| [wgpu](https://github.com/gfx-rs/wgpu) | GPU rendering (DirectX 12 / Vulkan / Metal) |
| [kurbo](https://github.com/linebender/kurbo) | Bézier path representation and evaluation |
| [winit](https://github.com/rust-windowing/winit) | Cross-platform window and event loop |
| [bytemuck](https://github.com/Lokathor/bytemuck) | Safe `&[Vertex]` → `&[u8]` casts for buffer uploads |
| [pollster](https://github.com/zesterer/pollster) | Block on wgpu async calls without a full async runtime |

## Architecture

### Current Code

- `TessellatedBezier` — holds a `kurbo::BezPath` and a CPU-side vertex/index buffer; tessellates the path into a centroid triangle fan on demand.
- `GPUBezierShape` — owns the `wgpu::Buffer` pair for a single shape; recreated whenever the tessellation changes.
- `BezierStimulus` — manages a fixed-size array of shapes (library stub, not yet wired to the demo).
- `State` — wgpu device/queue/surface and the per-frame upload loop.
- `App` — winit `ApplicationHandler` that drives the event loop.

### Planned Architecture

See [`docs/PLAN.md`](docs/PLAN.md) for the full design and roadmap. Additional planning documents are in the `docs/` directory.

## Building

```sh
cargo build
cargo build --release
cargo test
cargo clippy
```

## Relationship to VStim

| Version | Language | Renderer | Notes |
|---|---|---|---|
| VStim v1 | C++ / MFC | Direct3D 9 | Original monolithic stimulus software |
| VStim v2 | C++ / MFC | Direct3D 11 | Monolithic rewrite with improved renderer |
| StimServer | C++ / MFC | Direct3D 11 | Standalone server with client/server architecture over Windows named pipes (binary protocol) |
| VStim v3 (this repo) | Rust | wgpu (DX12/Vulkan/Metal) | Rust rewrite combining VStim and StimServer, cross-platform |