# Installation

!!! danger "Alpha software — not ready for production"
    vstimd is in **early alpha**. The APIs, wire protocol, and behaviour can change at
    any time, features are incomplete, and it has **not** been validated for experiments
    or data collection. Use it for evaluation and development only — **do not rely on it
    in production yet**.

## Server

### Linux dependencies

#### Ubuntu / Debian

```sh
sudo apt install build-essential pkg-config \
    libdrm-dev libudev-dev libinput-dev \
    protobuf-compiler
```

#### Fedora / RHEL

```sh
sudo dnf install gcc pkg-config \
    libdrm-devel systemd-devel libinput-devel \
    protobuf-compiler
```

### Manual installation

The server is a Rust binary. You need a working [Rust toolchain](https://rustup.rs) (stable, edition 2024), and node.js (v22 or later)

```sh
git clone https://github.com/vstimd/vstimd.git
cd vstimd
make build
sudo make install
```

### Package installation (planned)

A package will be available soon for Debian/Ubuntu-based distributions as well
as RHEL-based distributions. This will include systemd service files and other
configuration files. A Ubuntu PPA will be available soon.


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

## MATLAB client (planned)

A MATLAB client is planned but does not exist yet.

## Building the docs

The documentation is built with [MkDocs](https://www.mkdocs.org) (1.x) and the
[Material](https://squidfunk.github.io/mkdocs-material/) theme. The build
environment is declared in `docs/pyproject.toml` and managed with
[uv](https://docs.astral.sh/uv/):

```sh
make docs
```

The published site is built automatically by
[Read the Docs](https://readthedocs.org) (see `.readthedocs.yaml`), which runs the
same `uv run --project docs mkdocs build`.
