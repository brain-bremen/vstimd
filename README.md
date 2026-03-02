# vstim_server

A Rust playground for GPU-accelerated visual stimulus rendering using **wgpu** and **kurbo**.

The goal is a low-latency rendering loop:  animated shapes defined as Bézier paths, tessellated on the CPU each frame, and uploaded to the GPU as plain triangle fans.

## Demo

Running `cargo run` opens a window showing a 5-lobe amoeba shape that breathes and slowly rotates, with a colour gradient anchored to world angle.

## Stack

| Crate | Role |
|---|---|
| [wgpu](https://github.com/gfx-rs/wgpu) | GPU rendering (DirectX 12 / Vulkan / Metal) |
| [kurbo](https://github.com/linebender/kurbo) | Bézier path representation and evaluation |
| [winit](https://github.com/rust-windowing/winit) | Cross-platform window and event loop |
| [bytemuck](https://github.com/Lokathor/bytemuck) | Safe `&[Vertex]` → `&[u8]` casts for buffer uploads |
| [pollster](https://github.com/zesterer/pollster) | Block on wgpu async calls without a full async runtime |

## Structure

- `TessellatedBezier` — holds a `kurbo::BezPath` and a CPU-side vertex/index buffer; tessellates the path into a centroid triangle fan on demand.
- `GPUBezierShape` — owns the `wgpu::Buffer` pair for a single shape; recreated whenever the tessellation changes.
- `BezierStimulus` — manages a fixed-size array of shapes (library code, not yet wired to the demo).
- `State` — wgpu device/queue/surface and the per-frame upload loop.
- `App` — winit `ApplicationHandler` that drives the event loop.
