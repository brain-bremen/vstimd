# Wonderlamp

A Rust rewrite of the [StimServer](https://github.com/esi-neuroscience/StimServer) C++ visual stimulus server, combined with ideas from the **VStim** project.

The original StimServer used MFC/C++/Direct3D11 with a client/server architecture driven by Windows named pipes with binary messages. This project ports that architecture to Rust, targeting Linux as the primary deployment platform, replacing named pipes with ZeroMQ for cross-platform IPC, and adding a modern GPU rendering stack.

## Goals

- Spiritual successor to the C++ StimServer, using ZeroMQ in place of Windows named pipes for cross-platform IPC
- GPU-accelerated 2-D and 3-D stimulus rendering via **wgpu**
- Low-latency rendering loop with no blocking on the render thread
- Deterministic event logging for experiment replay and analysis
- Shared-memory position input (gaze/joystick) to avoid ZeroMQ round-trip latency

## Current Status

The IPC pipeline is working end-to-end:

- The server opens a fullscreen window and binds a ZMQ REP socket on `tcp://0.0.0.0:5555`
- Clients send protobuf-encoded `Request` messages; the server dispatches them to the scene and replies with a `Response`
- Three commands are implemented: **CreateRect**, **SetEnabled**, **Delete**
- The Python client (`client-python/`) wraps the ZMQ + protobuf layer and includes an example script that creates and flashes rectangles
- An egui debug overlay (press **F1**) shows live frame timing, a stimulus list with enable toggles, and a scrolling command log

The `extern/` directory contains git submodules for external references:

- `extern/StimServer/` — the original C++ reference implementation
- `extern/psychopy/` — PsychoPy, referenced for stimulus design ideas

## Quick Start

```sh
# Terminal 1 — start the server
cd server
cargo run --release
# Press D to spawn demo stimuli (cyan disc + magenta rect)
# Press F1 to toggle the debug overlay (frame timing, stimulus list, command log)

# Terminal 2 — run the flash example
cd client-python
uv run examples/flash_rects.py              # 4 flashes at 2 Hz
uv run examples/flash_rects.py --flashes 8 --hz 4
```

Or drive the server directly from Python:

```python
from wonderlamp_client import Connection

with Connection() as conn:
    handle = conn.create_rect(x=-200, y=0, width=300, height=200, r=1.0, g=0.0, b=0.0)
    conn.set_enabled(handle, False)
    conn.delete(handle)
```

## Stack

| Crate / Library | Role |
|---|---|
| [wgpu](https://github.com/gfx-rs/wgpu) | GPU rendering (DirectX 12 / Vulkan / Metal) |
| [winit](https://github.com/rust-windowing/winit) | Cross-platform window and event loop |
| [kurbo](https://github.com/linebender/kurbo) | Bézier path representation and evaluation |
| [prost](https://github.com/tokio-rs/prost) | Protobuf encode/decode |
| [zeromq](https://github.com/zeromq/zmq.rs) | Pure-Rust async ZMQ (no libzmq dependency) |
| [tokio](https://tokio.rs) | Async runtime for the ZMQ server thread |
| [bytemuck](https://github.com/Lokathor/bytemuck) | Safe `&[Vertex]` → `&[u8]` casts for buffer uploads |
| [egui](https://github.com/emilk/egui) + egui-wgpu + egui-winit | Debug overlay (enabled by default, `--no-default-features` to strip) |
| [pyzmq](https://pyzmq.readthedocs.io) | Python ZMQ bindings (client) |
| [protobuf](https://pypi.org/project/protobuf/) | Python protobuf runtime (client) |

## Architecture

### Server

The server runs two concurrent threads sharing `Arc<RwLock<SceneState>>`:

| Thread | Role |
|---|---|
| **winit / render** | Tessellates stimuli, uploads to GPU, presents frames at vsync |
| **ZMQ server** | Receives protobuf requests, dispatches to `SceneState::handle_request`, sends responses |

The ZMQ thread holds the write lock only while dispatching one command; the render thread can always acquire it between frames.

### Wire Protocol

`server/proto/wonderlamp.proto` defines the schema. All messages are protobuf-encoded over a ZMQ REQ/REP socket pair.

| Command | Route | Effect |
|---|---|---|
| `CreateRect` | handle = 0 | Creates a rectangle, returns new handle |
| `SetEnabled` | handle > 0 | Shows or hides a stimulus |
| `Delete` | handle > 0 | Removes a stimulus |

### Python Client

`client-python/wonderlamp_client/` is a minimal Python package with a single `Connection` class. The protobuf stubs in `_proto/` are generated from `server/proto/wonderlamp.proto`.

### Planned Architecture

See [`docs/PLAN.md`](docs/PLAN.md) for the full design and roadmap.

## Building

```sh
# Rust server
cargo build
cargo build --release
cargo test
cargo clippy

# Python client (requires uv)
cd client-python
uv sync
uv run examples/flash_rects.py
```

## Relationship to VStim

| Version | Language | Renderer | Notes |
|---|---|---|---|
| VStim v1 | C++ / MFC | Direct3D 9 | Original monolithic stimulus software |
| VStim v2 | C++ / MFC | Direct3D 11 | Monolithic rewrite with improved renderer |
| StimServer | C++ / MFC | Direct3D 11 | Standalone server with client/server architecture over Windows named pipes (binary protocol) |
| Wonderlamp (this repo) | Rust | wgpu (DX12/Vulkan/Metal) | Rust rewrite combining VStim and StimServer, cross-platform |
