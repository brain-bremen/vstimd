# Installation

## Server

The server is a Rust binary. You need a working [Rust toolchain](https://rustup.rs) (stable, edition 2024).

```sh
git clone https://github.com/vstimd/vstimd.git
cd vstimd
cargo build --release
```

The compiled binary is at `target/release/vstimd`.

### Linux dependencies

On bare-metal Linux (DRM mode), the server requires:

- Vulkan driver for your GPU (`mesa-vulkan-drivers` or vendor-specific)
- No compositor running (GDM/Xorg must be stopped — see [Bare-Metal Linux](bare-metal.md))

On desktop Linux (Wayland/X11), no extra steps are needed.

### Windows

Desktop mode only (DRM is not available). Build with the same `cargo build --release`.

## Python client

Requires Python ≥ 3.12 and [uv](https://docs.astral.sh/uv/).

```sh
cd client/python
uv sync
```

To install into an existing environment:

```sh
pip install ./client/python
```

## MATLAB client

See [MATLAB API](../api/matlab/index.md).

## Building the docs

```sh
pip install -r docs/requirements.txt
mkdocs serve        # live preview at http://127.0.0.1:8000
mkdocs build        # static output in site/
```
