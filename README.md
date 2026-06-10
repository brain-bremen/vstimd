# Visual Stimulation Daemon - vstimd

[![Build and Test](https://github.com/vstimd/vstimd/actions/workflows/ci.yml/badge.svg)](https://github.com/vstimd/vstimd/actions/workflows/ci.yml)

> **Status:** Pre-alpha — under active development, not yet suitable for production use.

vstimd is a visual stimulus server with strong guarantees for accurate and
precise frame timing as well as low-latency with broad compatibility with
clients (Python/PsychoPy, MATLAB, Bonsai). This is achieved by handling
rendering on a dedicated device and controlling it through cross-platform,
cross-language user-friendly clients.

The primary deployment platform is the [Direct Rendering Manager](https://en.wikipedia.org/wiki/Direct_Rendering_Manager)
(DRM) on NVIDIA Jetson Orin Nano, Raspberry Pi 5, and other Linux-based
systems. Using DRM and bypassing any windowing system enables stable and
low-latency rendering with few skipped frames.

vstimd combines ideas and concepts from Michael Stephan's
[StimServer](https://github.com/esi-neuroscience/StimServer) C++ visual
stimulus server and Andreas Kreiter's **VStim** project. 

This project is licensed under the GNU AGPLv3. Copyright (c) 2026 Joscha Schmiedt, University of Bremen. 

## Goals

- stable and low-latency rendering of visual stimuli for psychophysics
  experiments
- cross-platform client support (Linux, Windows, macOS) with different API
  flavours (PsychoPy, Bonsai, StimServer)
- Deterministic event logging for experiment replay and analysis
  latency

## Quick Start

```sh
# Terminal 1 — start the server
cargo run --release
# Press D to spawn demo stimuli (cyan circle + magenta rect)
# Press F1 to toggle the debug overlay (frame timing, stimulus list, command log)

# Terminal 2 — run the flash example
cd client/python
uv run examples/flash_rects.py              # 4 flashes at 2 Hz
uv run examples/flash_rects.py --flashes 8 --hz 4
```

Or drive the server directly from Python:

```python
from vstimd import Connection

with Connection() as conn:
    h = conn.stimuli.create_rect(x=-200, y=0, width=300, height=200, r=1.0, g=0.0, b=0.0)
    conn.stimuli.set_enabled(h, False)
    conn.stimuli.delete(h)
    info = conn.system.query_server_info()
    print(info.version)
```

## Building

```sh
# Rust server
cargo build
cargo build --release
cargo test
cargo clippy

# Run options
cargo run --release                   # fullscreen (auto-detects DRM or desktop)
cargo run --release -- --windowed 1280x720
cargo run --release -- --null         # ZMQ only, no display (also: VSTIMD_NULL=1)

# Python client (requires uv)
cd client/python
uv sync
uv run examples/flash_rects.py
```

## Documentation

- [`docs/PLAN.md`](docs/PLAN.md) — full design and roadmap
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — module structure, wire protocol, threading model, stack
- [`docs/BARE_METAL_LINUX.md`](docs/BARE_METAL_LINUX.md) — DRM/console rendering on Linux (Jetson, Pi)
- [`docs/PYTHON_CLIENT.md`](docs/PYTHON_CLIENT.md) — Python client API and PsychoPy compatibility
- [`docs/INPUT_LATENCY.md`](docs/INPUT_LATENCY.md) — latency analysis for position input
- [`docs/3D_ROADMAP.md`](docs/3D_ROADMAP.md) — 3-D stimulus roadmap
