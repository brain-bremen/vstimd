# Server (Rust) API

The server internals are documented via `cargo doc`. Generate and open locally:

```sh
cd server
cargo doc --no-deps --open
```

## Key types

| Type | Module | Description |
|---|---|---|
| `SceneState` | `scene::state` | All stimulus data; shared between render and ZMQ threads |
| `Stimulus` | `scene::stimulus` | Enum of all stimulus variants |
| `RenderState` | `render::render_state` | Shared Vulkan resources and per-frame render logic |
| `StimuliClient` | `ipc` | ZMQ REP server thread |

## Contributing

See [`BUILDING.md`](https://github.com/braemons/vstimd/blob/main/BUILDING.md) for build
instructions, clippy configuration, and test conventions.
