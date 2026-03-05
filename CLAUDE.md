# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**VStim v3** is a Rust rewrite of the C++ StimServer visual stimulus server, combined with ideas from the VStim project. The Rust server binary is named `vstim_server`; the overall project (server + Python client + tools) is VStim v3.

The original StimServer used MFC/C++/Direct3D11 with a client/server architecture over Windows named pipes (binary protocol). This project ports that architecture to Rust, targeting Linux as the primary deployment platform, replacing named pipes with ZeroMQ for cross-platform IPC, and adding a modern GPU rendering stack.

The `extern/` directory contains git submodules for external references: `extern/StimServer/` holds the original C++ reference implementation and `extern/psychopy/` holds PsychoPy for stimulus design reference.

## Build & Run

```bash
cargo run           # Debug build — opens animated amoeba window
cargo build
cargo build --release
cargo test
cargo clippy
```

The `.cargo/config.toml` explicitly sets the MSVC linker path to avoid Git Bash's conflicting `/usr/bin/link.exe` on Windows.

## Stack

- **wgpu 27.0.1** — GPU abstraction (DirectX 12/Vulkan/Metal)
- **winit 0.30** — Window + event loop (`ApplicationHandler` trait)
- **kurbo 0.13** — Bézier path math
- **bytemuck 1** — Safe vertex buffer casting (`Pod`/`Zeroable`)
- **pollster 0.3** — Blocking async executor
- Rust edition 2024

## Architecture

### Current Code (`src/main.rs`)

Everything is in one file. Layer order bottom-up:

1. **WGSL shader** — Simple passthrough (position vec2 + color vec4)
2. **`TessellatedBezier`** — CPU tessellation of `kurbo::BezPath` into triangle fans (centroid fan for filled, adaptive subdivision for open paths). Colors are animated HSV keyed to world-space angle.
3. **`GPUBezierShape`** — Owns wgpu vertex/index buffers; recreated when dirty.
4. **`BezierStimulus`** — Fixed-size array of CPU+GPU shape pairs (library stub, not yet wired into demo).
5. **`State`** — wgpu device/queue/surface/pipeline. **Field order matters**: `surface` declared before `window` so it drops first (required for `Surface<'static>` safety).
6. **`App` / event loop** — `winit::ApplicationHandler` + `ControlFlow::Poll` for continuous rendering.

### Critical wgpu 27 API Notes

- `adapter.request_device(&DeviceDescriptor::default())` — 1 argument, no trace path
- `RenderPassColorAttachment` requires `depth_slice: None`
- `RenderPipelineDescriptor` has no `push_constant_ranges` — put it on `PipelineLayoutDescriptor`
- `VertexState`/`FragmentState` require `compilation_options: Default::default()`
- Surface: use `create_surface_unsafe` for `Surface<'static>`; store `surface` before `window` in struct

### Planned Architecture (see `docs/`)

See `docs/PLAN.md` for the full design and roadmap. Additional planning documents are in the `docs/` directory.

Key architectural decisions already made:
- Stimulus types: flat `enum` with composition (not trait objects or inheritance)
- Position input: keep shared memory (ZMQ latency too high at ~100–400 µs round-trip)
- 2-D and 3-D coexist in one frame (3-D rendered first, 2-D overlaid)
- Render thread must never block or heap-allocate on event emission
