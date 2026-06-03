# Protocol Overview

vstimd uses **protobuf over ZMQ REQ/REP**. All clients speak the same wire protocol regardless
of language.

## Transport

- **Socket type:** ZMQ REQ/REP (synchronous request-reply)
- **Default address:** `tcp://0.0.0.0:5555` (server binds), `tcp://localhost:5555` (client connects)
- **Encoding:** protobuf (proto3)
- **Schema:** `proto/vstimd/v1/` in the repository

## Message structure

Every request is a `Request` message with a **target** (system or a stimulus handle) and a
**command** field:

```protobuf
message Request {
    oneof target {
        SystemTarget system   = 1;  // scene-wide commands
        uint32       stimulus = 2;  // per-stimulus commands (handle > 0)
    }
    oneof command {
        // System commands
        CreateRectRequest     create_rect     = 10;
        CreateCircleRequest   create_circle   = 11;
        CreateEllipseRequest  create_ellipse  = 12;
        CreateGratingRequest  create_grating  = 13;
        SetBackgroundRequest  set_background  = 20;
        // ... (see commands reference)
    }
}
```

Every response is a `Response` message:

```protobuf
message Response {
    ErrorCode code    = 1;   // OK = 0; non-zero on error
    string    error   = 2;   // human-readable on error
    uint32    handle  = 3;   // set on Create* commands
    // ... query response fields
}
```

## Schema files

| File | Contents |
|---|---|
| `common.proto` | `Vec2`, `Color`, `DrawMode`, `StimulusType` |
| `service.proto` | `Request`, `Response`, `ErrorCode` |
| `stimuli_2d.proto` | All create / mutate / query messages for 2-D stimuli |
| `system.proto` | Scene-wide commands and `QueryServerInfoResponse` |

## Error codes

| Code | Meaning |
|---|---|
| `ERROR_CODE_OK` | Success |
| `ERROR_CODE_HANDLE_NOT_FOUND` | No stimulus with the given handle |
| `ERROR_CODE_WRONG_STIMULUS_TYPE` | Command not valid for this stimulus type |
| `ERROR_CODE_WRONG_TARGET` | System command sent to a stimulus handle or vice versa |
| `ERROR_CODE_CREATION_FAILED` | Could not create the stimulus |
| `ERROR_CODE_INVALID_ARGUMENT` | Parameter value out of range or malformed |
| `ERROR_CODE_NOT_SUPPORTED` | Feature not available in this build |
| `ERROR_CODE_NOT_READY` | Server not ready (e.g. display not initialised) |
