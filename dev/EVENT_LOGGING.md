# Event Logging and Replay

> Companion document to `PLAN.md`, `STIMULUS_DATA_MODEL.md`, `INPUT_LATENCY.md`, and
> `3D_ROADMAP.md`.
> Covers the design of the event logging system: the internal event type, the wire format,
> verbosity levels, the messenger thread, ZMQ publication, SQLite export, and deterministic
> replay from a saved log.

---

## Table of Contents

1. [Goals and Non-Goals](#1-goals-and-non-goals)
2. [What Must Be Logged for Replay to Work](#2-what-must-be-logged-for-replay-to-work)
3. [Format Strategy: Three Distinct Layers](#3-format-strategy-three-distinct-layers)
4. [Layer 1 — Internal Event Enum (Copy types, zero allocation)](#4-layer-1--internal-event-enum)
5. [Layer 2 — Wire Format (FlatBuffers)](#5-layer-2--wire-format-flatbuffers)
6. [Layer 3 — Log File Layout](#6-layer-3--log-file-layout)
7. [Verbosity Levels](#7-verbosity-levels)
8. [The Messenger Thread](#8-the-messenger-thread)
9. [ZeroMQ Event Publication](#9-zeromq-event-publication)
10. [SQLite Export](#10-sqlite-export)
11. [Replay Mode](#11-replay-mode)
12. [Threading and Timing Design](#12-threading-and-timing-design)
13. [Crate Dependencies](#13-crate-dependencies)
14. [Schema: FlatBuffers `.fbs` File](#14-schema-flatbuffers-fbs-file)
15. [Impact on Existing Architecture](#15-impact-on-existing-architecture)
16. [Open Questions](#16-open-questions)

---

## 1. Goals and Non-Goals

### Goals

- Record every state-changing input with a high-precision timestamp and a frame index.
- Support multiple verbosity levels so a production run logs only what is needed.
- Publish events in real time over a ZeroMQ PUB socket so external tools can subscribe.
- Save logs to disk in a compact binary format suitable for long experiments.
- Support optional export to SQLite for post-hoc querying.
- Support **deterministic replay**: loading a log file and re-running the exact same visual
  sequence, frame for frame, without any live ZMQ clients or hardware.
- **The render thread must never block and must never heap-allocate** when emitting events.

### Non-Goals

- The log is not a general-purpose tracing system (that is `tracing` / `tracing-subscriber`'s
  job). It is an experiment-level event log: coarser-grained, domain-specific, replay-capable.
- The log does not capture raw video output. Replay reconstructs frames from events; it does
  not play back a video.
- The log does not replace the ZMQ control channel. Clients still send commands over ZMQ;
  the logger observes those commands as they pass through `SceneState::handle_request`.

---

## 2. What Must Be Logged for Replay to Work

Replay is the hardest constraint and it shapes the entire system. A log that supports replay
must record **inputs**, not derived state. If any input is missing, replay diverges silently.

### 2.1 ZMQ commands (straightforward)

Every `Request` received over the ZMQ REP socket is already a serialised protobuf message.
The ZMQ thread wraps the raw received bytes in `Arc<[u8]>` and sends that into the event
channel. One allocation (the ZMQ receive buffer), zero copies. On replay, the same bytes are
decoded and fed back through `SceneState::handle_request`. The response is discarded.

### 2.2 Per-frame shared-memory position reads (the hard case)

`AnimExternalPos::advance()` reads `(x, y)` from shared memory **every frame**. These reads
are not commands — they are continuous inputs that produce a different value on every frame.
For replay to be exact, each sampled value must be logged at the frame it was read.

The solution: at Trace verbosity, `advance()` emits a `PositionSample` event carrying the
`(x, y)` pair. On replay, instead of reading from shared memory, the replay driver injects
the logged value via `SceneState::inject_position(anim_handle, x, y)`. Shared memory is not
opened at all during replay.

This is the *only* mechanism that makes position-animated stimuli replay correctly.

### 2.3 Frame boundaries (essential for ordering)

The render thread emits `FrameBegin` at the start of every frame. This event is the primary
ordering key. The replay driver reads ahead in the file until it finds the next `FrameBegin`,
dispatches all events for the current frame, then advances.

### 2.4 What does NOT need to be logged for replay

- Tessellated geometry (derived from stimulus parameters already in the log).
- GPU buffer uploads (derived).
- The egui overlay state (not part of the visual stimulus output).
- `tracing` spans (developer diagnostics, not experiment inputs).

---

## 3. Format Strategy: Three Distinct Layers

The previous design conflated the internal representation of an event with its wire format,
using `prost`-generated protobuf types throughout. This is the wrong choice for a system
where the render thread must emit events without heap-allocating.

**The problem with protobuf (`prost`) on the hot path:**

- `prost`-generated message types own all variable-length data: `String`, `Vec<u8>`,
  `Vec<T>`. Constructing one requires heap allocation.
- `prost::Message::encode` always serialises into a caller-provided `Vec<u8>`, meaning
  every emit on the render thread would touch the allocator.
- The internal event type and the wire type are the same object, so the render thread pays
  the full serialisation cost even when nothing is being written to disk.

**The problem with FlatBuffers on the hot path (write side):**

- `FlatBufferBuilder` can be reused across messages (call `reset()` between messages),
  avoiding repeated allocations on the *messenger* thread.
- But a builder cannot be cheaply sent through a channel — it is a large struct owning an
  internal `Vec<u8>`. Sending a completed FlatBuffer through the channel means either
  copying the bytes or sending the builder itself (which transfers ownership and forces a new
  allocation on the next message).
- FlatBuffers shine on the **read side**: a received or file-mapped `&[u8]` can be walked
  without any deserialisation. This is exactly what the **replay driver** needs.

**The solution: three separate layers with different representations:**

```
Layer 1 — Internal event enum
  Plain Rust enum, all variants are Copy or contain Arc<[u8]>.
  Constructed on the emitting thread (render, ZMQ, etc.).
  Sent through a crossbeam_channel::bounded<Event>.
  Zero heap allocation for fixed-size events (the common case on the render thread).

Layer 2 — FlatBuffers wire encoding
  Performed on the messenger thread only, using a single reusable FlatBufferBuilder.
  Produces a &[u8] into the builder's buffer.
  Written to the log file and/or the ZMQ PUB socket from those same bytes.
  Zero-copy on the replay read path: the file is read into a Vec<u8> and walked as a
  FlatBuffer table without deserialisation.

Layer 3 — Log file
  A sequence of framed FlatBuffers records with a fixed binary file header.
  Described in §6.
```

The protobuf control protocol (`vstimd.proto`) is **unchanged**: ZMQ REQ/REP for commands
still uses `prost`. Only the *event log* format switches to FlatBuffers. The raw bytes of
an incoming ZMQ request (already a protobuf blob) are stored verbatim in the log as an
opaque `[ubyte]` field — no re-encoding.

---

## 4. Layer 1 — Internal Event Enum

### 4.1 Design rules

1. All variants that the **render thread** emits must be `Copy` — no heap allocation at the
   emit site.
2. Variants that contain variable-length data (command bytes, object type names) are only
   ever emitted from the **ZMQ thread** or the **session lifecycle code**, never from the
   render thread. They carry `Arc<[u8]>` or a small fixed-length string type to keep the
   channel send cheap (a pointer copy, not a deep copy).
3. The enum is `#[non_exhaustive]` to make future additions a non-breaking change inside the
   crate, while still forcing exhaustive handling in the messenger thread (which is in the
   same crate and uses a non-`_` catch-all).

### 4.2 Fixed-size helper types

```rust
/// A short string that fits inline without heap allocation.
/// 31 bytes of UTF-8 content + 1 byte length. Enough for type names like
/// "RectStimulus", "AnimHarmonic", "WgslShaderStimulus".
#[derive(Clone, Copy)]
pub struct ShortStr {
    len:  u8,
    data: [u8; 31],
}

impl ShortStr {
    pub fn new(s: &str) -> Self {
        let bytes = s.as_bytes();
        let len   = bytes.len().min(31) as u8;
        let mut data = [0u8; 31];
        data[..len as usize].copy_from_slice(&bytes[..len as usize]);
        Self { len, data }
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.data[..self.len as usize]).unwrap_or("")
    }
}
```

### 4.3 The Event type

```rust
/// Nanoseconds since session start, from a monotonic Instant.
pub type Ns = u64;

/// The internal event type. Sent through the crossbeam channel.
/// Variants marked [COPY] are safe to emit from the render thread without allocation.
/// Variants marked [ARC]  carry reference-counted data; emitted from other threads.
#[derive(Clone)]
pub enum Event {

    // ── Session lifecycle ─────────────────────────────────────────────────
    /// [ARC] Emitted once at startup by the main thread.
    SessionStart(Arc<SessionStartData>),
    /// [COPY] Emitted once at shutdown by the main thread.
    SessionEnd { total_frames: u64, duration_ns: Ns },

    // ── Frame boundaries ─────────────────────────────────────────────────
    /// [COPY] Emitted at the very start of each render frame.
    /// Level: Trace.
    FrameBegin { frame_index: u64, timestamp_ns: Ns, vsync_interval_ns: u64 },

    // ── ZMQ commands ──────────────────────────────────────────────────────
    /// [ARC] A Request was received over the ZMQ REP socket.
    /// The bytes are the raw protobuf-encoded vstimd::Request, already owned
    /// by the ZMQ receive buffer. Wrapped in Arc to avoid copying.
    /// Level: Debug.
    CommandReceived { frame_index: u64, timestamp_ns: Ns, bytes: Arc<[u8]> },
    /// [ARC] The Response sent back to the ZMQ client.
    /// Level: Debug.
    CommandResponse { frame_index: u64, timestamp_ns: Ns, bytes: Arc<[u8]> },

    // ── Object lifecycle ──────────────────────────────────────────────────
    /// [COPY] A stimulus or animation was created.
    /// Level: Info.
    ObjectCreated { frame_index: u64, timestamp_ns: Ns, handle: u32, type_name: ShortStr },
    /// [COPY] A stimulus or animation was deleted.
    /// Level: Info.
    ObjectDeleted { frame_index: u64, timestamp_ns: Ns, handle: u32 },

    // ── Deferred mode ─────────────────────────────────────────────────────
    /// [COPY] Deferred mode started or ended via command.
    /// Level: Info.
    DeferredMode { frame_index: u64, timestamp_ns: Ns, started: bool },
    /// [COPY] The pending_flip was consumed: all copy→live promotions happened.
    /// Level: Info.
    DeferredFlip { frame_index: u64, timestamp_ns: Ns },

    // ── Background colour ─────────────────────────────────────────────────
    /// [COPY] Level: Info.
    BackgroundChanged { frame_index: u64, timestamp_ns: Ns, rgba: [f32; 4] },

    // ── Position samples ──────────────────────────────────────────────────
    /// [COPY] Sampled value from AnimExternalPos / AnimZmqPos / AnimMousePos /
    /// AnimGamepadPos. Emitted every frame the animation is active.
    /// Level: Trace (required for full replay).
    PositionSample {
        frame_index:  u64,
        timestamp_ns: Ns,
        anim_handle:  u32,
        x:            f32,
        y:            f32,
        source:       PositionSource,
    },

    // ── Errors and performance ─────────────────────────────────────────────
    /// [COPY] A frame exceeded its vsync budget.
    /// Level: Warn.
    DroppedFrame { frame_index: u64, frame_time_ns: u64, budget_ns: u64 },
    /// [ARC] An error or warning with a message string.
    /// Level: Warn or Error.
    ServerError { frame_index: u64, timestamp_ns: Ns, code: i16, message: Arc<str> },

    // ── Photodiode ────────────────────────────────────────────────────────
    /// [COPY] Level: Info.
    PhotodiodeChanged { frame_index: u64, timestamp_ns: Ns, lit: bool },

    // ── Log control ───────────────────────────────────────────────────────
    /// [COPY] Internal sentinel: flush and sync the log file to disk.
    FlushLog,
    /// [COPY] Internal sentinel: open a new log file.
    StartLog { path: Option<[u8; 256]> },  // fixed-size path buffer, None = auto-name
    /// [COPY] Internal sentinel: close the current log file.
    StopLog,
}

/// Non-Copy data for SessionStart. Only constructed once.
pub struct SessionStartData {
    pub server_version:    String,
    pub screen_width:      u32,
    pub screen_height:     u32,
    pub refresh_rate_hz:   f32,
    pub zmq_rep_addr:      String,
    pub zmq_pub_addr:      String,
    pub session_id:        u128,           // UUID
    pub start_time_unix_ns: u64,           // wall-clock for external correlation
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PositionSource {
    SharedMemory = 0,
    ZmqSub       = 1,
    Mouse        = 2,
    Gamepad      = 3,
}
```

### 4.4 Allocation profile

| Variant | Render thread safe? | Allocation |
|---|---|---|
| `FrameBegin` | Yes | None — pure `Copy` |
| `PositionSample` | Yes | None — pure `Copy` |
| `DroppedFrame` | Yes | None — pure `Copy` |
| `DeferredFlip` | Yes | None — pure `Copy` |
| `DeferredMode` | Yes | None — pure `Copy` |
| `PhotodiodeChanged` | Yes | None — pure `Copy` |
| `ObjectCreated` | Yes | None — `ShortStr` is inline |
| `ObjectDeleted` | Yes | None — pure `Copy` |
| `BackgroundChanged` | Yes | None — pure `Copy` |
| `CommandReceived` | No (ZMQ thread) | One `Arc` wrap of the existing ZMQ buffer |
| `CommandResponse` | No (ZMQ thread) | One `Arc` wrap of the response buffer |
| `ServerError` | No | One `Arc<str>` |
| `SessionStart` | No (startup only) | One `Arc<SessionStartData>` |

The render thread emits only the first nine variants. None of them touch the allocator.

### 4.5 Emit helpers

```rust
/// Emit an event. Never blocks. Silently increments a drop counter if the channel is full.
/// Use this for all render-thread emissions.
#[inline(always)]
pub fn emit(tx: &EventSender, ev: Event) {
    if tx.try_send(ev).is_err() {
        DROPPED_EVENT_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

/// Emit a Trace-level event. Compiled to nothing when the `log-trace` feature is absent.
/// Use this for FrameBegin, PositionSample — the highest-volume events.
#[macro_export]
macro_rules! emit_trace {
    ($tx:expr, $ev:expr) => {
        #[cfg(feature = "log-trace")]
        $crate::logging::emit($tx, $ev);
    };
}
```

The `log-trace` Cargo feature is **off by default**. Release builds have zero overhead for
Trace-level events. It is enabled by `--features log-trace` or by the `--record` CLI flag
(which sets the feature at runtime via the verbosity filter on the messenger thread — compile-
time elimination is an additional optimisation, not the primary gate).

---

## 5. Layer 2 — Wire Format (FlatBuffers)

### 5.1 Why FlatBuffers for the wire format

| Property | protobuf (prost) | FlatBuffers |
|---|---|---|
| Write side | Allocates `Vec<u8>` per encode | Builder reusable across messages; `reset()` between records |
| Read side | Must fully decode into owned types | Zero-copy: walk `&[u8]` directly as a table |
| Schema evolution | Backward/forward compatible | Backward/forward compatible (with care) |
| Rust support | `prost` — excellent | `flatbuffers` crate — good |
| Replay read path | Deserialise every record | Map file into memory; access fields without deserialisation |
| Python/MATLAB reader | `protobuf` library | `flatbuffers` library (both excellent) |
| Variable-length strings | Owned `String` | `&str` into the buffer — no copy |
| File size | Compact (varint-encoded) | Slightly larger (alignment padding) |

The decisive advantage is the **replay read path**: with FlatBuffers the replay driver can
`mmap` (or `read`) the log file and walk records without a single heap allocation. With
protobuf, every `CommandReceived` record requires decoding the outer envelope into an owned
struct before the inner bytes can be extracted.

The messenger thread owns a single `FlatBufferBuilder` that is `reset()` between records.
After `builder.finish(root, ...)`, `builder.finished_data()` returns a `&[u8]` into the
builder's internal buffer. That slice is written to the file and the ZMQ socket. No
intermediate copy.

### 5.2 Builder lifecycle on the messenger thread

```rust
// Inside the messenger thread loop:
let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(4096);

loop {
    let event = rx.recv()?;

    builder.reset();  // Reuse the allocation; capacity grows but never shrinks.

    encode_event(&mut builder, &event, &config);
    // builder.finished_data() is valid until the next reset().

    let wire_bytes: &[u8] = builder.finished_data();

    // Write the 4-byte length prefix + flatbuffer bytes to the file.
    file_writer.write_record(wire_bytes)?;

    // Publish to ZMQ PUB socket (if level passes the filter).
    if should_publish(&event, config.zmq_level) {
        zmq_pub.send(wire_bytes)?;
    }

    // Drain remaining events without re-entering recv().
    while let Ok(ev) = rx.try_recv() {
        builder.reset();
        encode_event(&mut builder, &ev, &config);
        let wire_bytes = builder.finished_data();
        file_writer.write_record(wire_bytes)?;
        if should_publish(&ev, config.zmq_level) {
            zmq_pub.send(wire_bytes)?;
        }
    }

    file_writer.flush_if_due()?;
}
```

`encode_event` maps from the internal `Event` enum to the FlatBuffers schema defined in §14.
For `CommandReceived { bytes: Arc<[u8]> }`, the raw bytes are written as a `[ubyte]` field
— the Arc is dereferenced once, copied into the builder's buffer. This is the only copy of
the command bytes in the entire pipeline (ZMQ receive buffer → Arc → FlatBuffer field).

### 5.3 Zero-copy reads on the replay path

```rust
// Replay driver reading a record from the file:
let record_bytes: &[u8] = file_reader.next_record()?;

// FlatBuffers access — no allocation, no deserialisation:
let event = vstimd_log::root_as_log_event(record_bytes)?;

match event.kind_type() {
    EventKind::CommandReceived => {
        // bytes() returns a &[u8] into record_bytes — no copy.
        let req_bytes = event.kind_as_command_received().unwrap().bytes().unwrap();
        let request   = proto::Request::decode(req_bytes)?;
        scene.handle_request(request);
    }
    EventKind::PositionSample => {
        let ps = event.kind_as_position_sample().unwrap();
        scene.inject_position(ps.anim_handle(), ps.x(), ps.y());
    }
    // ...
}
```

The `req_bytes` slice points directly into the log file's read buffer. `proto::Request::decode`
then deserialises from those bytes. This is the one point where protobuf decoding happens on
the replay path — unavoidable since the commands are stored as opaque protobuf blobs — but it
happens on a non-real-time thread with no latency budget.

---

## 6. Layer 3 — Log File Layout

### 6.1 Requirements

- **Appendable**: sequential writes only, no seeks. Safe on spinning disks and SSDs.
- **Crash-safe**: a partial last record is detectable and skippable; all earlier records
  are intact.
- **Self-describing**: the file header embeds format version and session metadata.
- **Streamable**: `tail -c +0 -f session.wllog | reader` works in real time.

### 6.2 Binary layout

```
┌─────────────────────────────────────────────────────────┐
│  FILE HEADER  (64 bytes, fixed)                         │
│  magic:           b"VSTIMLOG"  (8 bytes)                │
│  format_version:  u16 LE      (2 bytes)  currently 1   │
│  schema_version:  u16 LE      (2 bytes)  FBS schema ver │
│  session_id:      u128 LE     (16 bytes) random UUID    │
│  start_unix_ns:   u64 LE      (8 bytes)  wall clock     │
│  reserved:        [u8; 26]               zero-filled    │
├─────────────────────────────────────────────────────────┤
│  RECORD 0                                               │
│    length:  u32 LE   (4 bytes)                          │
│    data:    [u8; length]  (FlatBuffer-encoded LogEvent) │
├─────────────────────────────────────────────────────────┤
│  RECORD 1                                               │
│    length:  u32 LE                                      │
│    data:    [u8; length]                                │
├─────────────────────────────────────────────────────────┤
│  ...                                                    │
└─────────────────────────────────────────────────────────┘
```

The length-prefix approach (identical to TFRecord, protobuf `writeDelimitedTo`) allows a
reader to skip a truncated final record: if `length` bytes are not available, the record is
incomplete and is discarded. All preceding records are valid.

### 6.3 File naming

```
vstimd_<session_id_short>_<YYYYMMDD_HHMMSS>.wllog
```

Example: `vstimd_a3f2c1d8_20250614_143022.wllog`

Written to `--log-dir` (default `./logs/`). The `.wllog` extension is application-specific
and not shared with any other tool.

### 6.4 Write buffering

The messenger thread maintains a `BufWriter<File>` with a 256 KB buffer. It flushes when:

- The buffer is full (automatic, handled by `BufWriter`).
- A `FrameBegin` event is processed (once per frame, keeping the file current to within
  one frame ≈ 4–8 ms at 120–240 Hz).
- A `FlushLog` control event is received (explicit sync requested by the experiment script).
- The session ends (`SessionEnd`).

`file.sync_data()` (fdatasync) is called only on explicit `FlushLog` — not on every frame
flush. A `BufWriter` flush writes to the kernel page cache; `sync_data` writes to stable
storage. The distinction matters: page-cache writes are fast and sufficient for most
experiments; `sync_data` is only needed before a power cycle or machine crash.

---

## 7. Verbosity Levels

Five levels matching `tracing` convention. Each destination (file, ZMQ PUB, SQLite) has an
independently configurable minimum level:

```
--log-level-file    <level>  (default: debug)
--log-level-zmq     <level>  (default: info)
--log-level-sqlite  <level>  (default: info)
```

| Level | Value | Events included |
|---|---|---|
| `error` | 0 | `ServerError` (fatal) |
| `warn`  | 1 | + `ServerError` (warnings), `DroppedFrame` |
| `info`  | 2 | + `SessionStart/End`, `ObjectCreated/Deleted`, `DeferredMode`, `DeferredFlip`, `BackgroundChanged`, `PhotodiodeChanged` |
| `debug` | 3 | + `CommandReceived`, `CommandResponse` |
| `trace` | 4 | + `FrameBegin`, `PositionSample` |

**For full replay, `--log-level-file` must be `trace`.** Use `--record` as a convenience
alias. A `debug`-level log supports replay only if no position-animated stimuli were active
during the session.

The runtime log level is stored as an `AtomicU8` in the messenger config, readable without
locking from emitting threads. It can be changed at runtime via `CmdSetLogLevel`.

---

## 8. The Messenger Thread

### 8.1 Responsibilities

The messenger thread is a plain `std::thread` (not tokio — it does blocking I/O by design):

1. Drain `Event` values from the `crossbeam_channel::Receiver<Event>`.
2. Encode each event to FlatBuffers using a reusable `FlatBufferBuilder`.
3. Write to the log file if recording is active and the event's level passes the file filter.
4. Publish to the ZMQ PUB socket if the event's level passes the ZMQ filter.
5. Batch-insert to SQLite if enabled and the event's level passes the SQLite filter.
6. Handle control sentinels (`FlushLog`, `StartLog`, `StopLog`, `SessionEnd`).
7. Feed the egui overlay's frame-time ring buffer (feature-gated).

It is the **only** thread that does I/O. All other threads emit via `try_send`.

### 8.2 Channel

```rust
/// Enough capacity to absorb a full second of Trace-level events without backpressure.
/// At 240 Hz × ~6 events/frame = ~1440 events/s.
const EVENT_CHANNEL_CAPACITY: usize = 16_384;

pub type EventSender   = crossbeam_channel::Sender<Event>;
pub type EventReceiver = crossbeam_channel::Receiver<Event>;

pub fn event_channel() -> (EventSender, EventReceiver) {
    crossbeam_channel::bounded(EVENT_CHANNEL_CAPACITY)
}
```

If the channel fills up (messenger thread is slower than emission rate), `try_send` returns
`Err`. The emitting thread increments `DROPPED_EVENT_COUNT` (an `AtomicU64`) and moves on.
The render thread is never blocked. The drop counter is periodically flushed as a `Warn`-level
event so the operator knows the log has gaps.

### 8.3 MessengerConfig

```rust
pub struct MessengerConfig {
    pub log_dir:              PathBuf,
    pub log_level_file:       LogLevel,
    pub log_level_zmq:        LogLevel,
    pub log_level_sqlite:     LogLevel,
    pub zmq_pub_addr:         String,         // e.g. "tcp://*:5556"
    pub sqlite_path:          Option<PathBuf>,
    pub sqlite_batch_size:    usize,          // rows per transaction (default 500)
    pub sqlite_flush_interval_s: f32,         // seconds between commits (default 5.0)
    pub start_time_unix_ns:   u64,
    pub session_id:           u128,
}
```

---

## 9. ZeroMQ Event Publication

### 9.1 Socket type and addressing

A **ZMQ PUB socket** on a separate port from the REP control socket (default `tcp://*:5556`,
configurable via `--zmq-pub-addr`). The REP and PUB sockets are on different ports and have
different semantics — PUB is fire-and-forget to all subscribers.

### 9.2 Message format

Each ZMQ publication is a two-frame multipart message:

```
Frame 0: 1 byte — LogLevel as u8 (topic byte for ZMQ subscription filtering)
Frame 1: N bytes — FlatBuffer-encoded LogEvent (identical bytes to what was written to file)
```

Using the same FlatBuffers encoding for both file and ZMQ means a subscriber uses the same
generated code to read live events and replayed events. The `session_id` in the `SessionStart`
event allows a subscriber to correlate live ZMQ events with a file on disk.

### 9.3 Subscription filtering

Subscribers set a ZMQ topic filter on the level byte:

```python
# Subscribe to Info (2), Warn (1), and Error (0) only:
for level in [0, 1, 2]:
    sub.setsockopt(zmq.SUBSCRIBE, bytes([level]))

# Subscribe to everything:
sub.setsockopt(zmq.SUBSCRIBE, b"")
```

### 9.4 Back-pressure

The PUB socket has a configurable send high-water mark (default 4096 messages). If a
subscriber is slow, messages are dropped silently — the PUB socket never blocks the messenger
thread. The file is the authoritative record; the ZMQ stream is a convenience feed.

---

## 10. SQLite Export

### 10.1 Why SQLite is not the primary real-time store

SQLite WAL mode can sustain ~10 000 small inserts/second. At Trace level with 240 Hz and
several active position animations, the event rate is ~1500 events/second — borderline — but
WAL checkpoint flushes can spike to tens of milliseconds unpredictably. These spikes would
propagate backpressure through the channel and eventually stall the messenger thread.

SQLite is therefore kept as an **export** and **deferred monitoring** target only.

### 10.2 Two modes

**Post-hoc export** (`vstimd-convert`): a standalone binary reads a `.wllog` file and
populates a `.db` file. No latency constraints. The recommended workflow for analysis.

**Deferred real-time** (optional, Info level only): the messenger thread accumulates rows in
memory and commits a batch transaction every `sqlite_flush_interval_s` seconds or every
`sqlite_batch_size` rows. Provides a queryable live view with a few seconds of lag, useful
for the egui "recent events" panel. Enabled by `--sqlite-live path/to/session.db`.

### 10.3 SQLite schema

```sql
CREATE TABLE session (
    id               BLOB    PRIMARY KEY,  -- 16-byte UUID
    start_unix_ns    INTEGER NOT NULL,
    end_unix_ns      INTEGER,
    screen_width     INTEGER,
    screen_height    INTEGER,
    refresh_rate_hz  REAL,
    server_version   TEXT
);

CREATE TABLE events (
    rowid        INTEGER PRIMARY KEY,
    session_id   BLOB    NOT NULL REFERENCES session(id),
    timestamp_ns INTEGER NOT NULL,   -- ns since session start
    frame_index  INTEGER NOT NULL,
    level        INTEGER NOT NULL,   -- 0=Error … 4=Trace
    kind         TEXT    NOT NULL,   -- event type name e.g. "CommandReceived"
    payload      BLOB    NOT NULL    -- FlatBuffer bytes of the event body
);
CREATE INDEX events_frame ON events(session_id, frame_index);
CREATE INDEX events_kind  ON events(kind, frame_index);

CREATE TABLE frames (
    session_id        BLOB    NOT NULL REFERENCES session(id),
    frame_index       INTEGER NOT NULL,
    timestamp_ns      INTEGER NOT NULL,
    vsync_interval_ns INTEGER NOT NULL,
    dropped           INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (session_id, frame_index)
);

CREATE TABLE stimulus_events (
    session_id   BLOB    NOT NULL REFERENCES session(id),
    frame_index  INTEGER NOT NULL,
    timestamp_ns INTEGER NOT NULL,
    handle       INTEGER NOT NULL,
    kind         TEXT    NOT NULL,  -- "created", "deleted"
    object_type  TEXT,
    payload      BLOB
);
CREATE INDEX stim_events_handle ON stimulus_events(session_id, handle, frame_index);
```

### 10.4 `vstimd-convert` tool

```
vstimd-convert  input.wllog  output.db  [--min-level info]  [--frames 0..9999]
```

A small binary in the same Cargo workspace. Reads the `.wllog` sequentially, decodes each
FlatBuffer record, and inserts rows. No network or render dependency.

---

## 11. Replay Mode

### 11.1 Invocation

```
vstimd --replay session.wllog [--speed 1.0] [--start-frame 0] [--end-frame N]
```

| Flag | Default | Meaning |
|---|---|---|
| `--speed` | `1.0` | Playback speed multiplier. `0.0` = step mode (Space to advance). |
| `--start-frame` | `0` | Seek forward to this frame before playback begins. |
| `--end-frame` | (end of file) | Stop after this frame. |
| `--no-vsync` | off | Disable VSync for as-fast-as-possible playback. |

### 11.2 What replay changes

In replay mode:

- The ZMQ REP socket is **not opened**. No live clients.
- All shared-memory regions are **not mapped**. No live hardware.
- The ZMQ PUB socket **is opened** so subscribers can observe the replayed session.
- A new log file **is not written** unless `--record` is also passed (replay of a replay).
- The render thread runs normally: wgpu renders frames, vsync locks the rate.
- A `ReplayDriver` replaces the ZMQ server as the source of scene commands.

### 11.3 ReplayDriver

```rust
pub struct ReplayDriver {
    reader:         LogFileReader,
    /// Events read ahead from the file, waiting for their frame to arrive.
    lookahead:      VecDeque<(u64, Event)>,   // (frame_index, event)
    eof:            bool,
}

impl ReplayDriver {
    /// Called by the render thread at the start of frame `frame_index`,
    /// before animations advance. Dispatches all events belonging to this frame.
    pub fn advance(&mut self, frame_index: u64, scene: &mut SceneState) {
        // Fill the lookahead buffer until we have seen the FrameBegin
        // for frame_index + 1, or EOF.
        self.fill_to_frame(frame_index + 1);

        // Dispatch everything with frame_index == current frame.
        while let Some((fi, _)) = self.lookahead.front() {
            if *fi != frame_index { break; }
            let (_, event) = self.lookahead.pop_front().unwrap();
            self.dispatch(event, scene);
        }
    }

    fn dispatch(&self, event: Event, scene: &mut SceneState) {
        match event {
            Event::CommandReceived { bytes, .. } => {
                // Decode the protobuf Request and feed through the normal path.
                // The response is silently discarded.
                if let Ok(req) = proto::Request::decode(&*bytes) {
                    let _ = scene.handle_request(req);
                }
            }
            Event::PositionSample { anim_handle, x, y, .. } => {
                // Inject the logged position into the animation, bypassing shm/zmq.
                scene.inject_position(anim_handle, x, y);
            }
            // All other event types are informational during replay.
            _ => {}
        }
    }
}
```

The `LogFileReader` decodes FlatBuffer records from the `.wllog` file using
`flatbuffers::root_as_log_event(record_bytes)` and converts them back into the internal
`Event` enum. For `CommandReceived`, the inner `[ubyte]` field is copied into a new
`Arc<[u8]>` — this is the only allocation during replay, and it happens on the (non-real-time)
reader, not on the render thread.

### 11.4 inject_position

```rust
impl SceneState {
    /// Called by the replay driver before advance() runs on a given frame.
    /// Sets the injected_pos on the target animation so advance() uses the
    /// logged value instead of reading from shared memory or a ZMQ socket.
    pub fn inject_position(&mut self, anim_handle: u32, x: f32, y: f32) {
        if let Some(anim) = self.animations.get_mut(&anim_handle) {
            anim.inject_position(x, y);
        }
    }
}
```

The `inject_position` method on the `Animation` trait has a default no-op implementation.
Only `AnimExternalPos`, `AnimZmqPos`, `AnimMousePos`, and `AnimGamepadPos` override it.
During live operation, `injected_pos` is always `None` and the normal read path is taken.

### 11.5 Timing during replay

**Real-time** (`--speed 1.0`): vsync-locked as normal. The replay driver dispatches events
for the current frame regardless of wall-clock time. If the original session was at 120 Hz
and the replay display is at 60 Hz, each frame of the original maps to two display frames;
use `--speed 0.5` to compensate.

**Step mode** (`--speed 0.0`): the render loop pauses after completing each frame and waits
for `Space` (next frame) or `Esc` (exit). Ideal for debugging specific frames.

**As-fast-as-possible** (`--no-vsync`): disables `PresentMode::Fifo`. Renders at GPU speed.
Useful for offline analysis pipelines that generate output images without a real display.

### 11.6 What replay does NOT guarantee

- **Pixel-identical GPU output** if the driver, wgpu version, or screen resolution differs.
  The stimulus geometry and sequence are identical; GPU rounding may differ.
- **Exact wall-clock timing**: the frame sequence is identical; timestamps are not.
- **External hardware effects**: reward delivery, TTL pulses, and other outputs triggered by
  the experiment control script are not part of the log.
- **File-loaded assets**: if a bitmap, mesh, WGSL shader, or `.ply` file referenced in the
  log has moved or changed, replay will diverge. The log stores the file path and a SHA-256
  hash at load time; `vstimd-convert` warns when hashes do not match. (See §16.4.)

---

## 12. Threading and Timing Design

### 12.1 Timestamps

Two values are recorded on every event:

**`timestamp_ns: u64`** — nanoseconds since session start. Acquired via
`session_start_instant.elapsed().as_nanos() as u64` where `session_start_instant` is an
`Instant` captured once at startup. `Instant` is monotonic and sub-microsecond precision on
Linux (`CLOCK_MONOTONIC`). Values are relative to session start so they fit in a `u32` for
sessions under ~4 seconds and comfortably in `u64` for sessions up to 584 years.

**`frame_index: u64`** — render frame counter. The render thread calls
`FRAME_INDEX.fetch_add(1, Ordering::Relaxed)` at the start of each frame. All other threads
call `FRAME_INDEX.load(Ordering::Relaxed)` when constructing an event. This is the
authoritative ordering key for multi-stimulus experiments. Wall-clock time is secondary; frame
index is the ground truth for "what appeared on screen at the same moment."

```rust
pub static FRAME_INDEX:       AtomicU64 = AtomicU64::new(0);
pub static SESSION_START:     OnceLock<Instant> = OnceLock::new();

pub fn session_elapsed_ns() -> u64 {
    SESSION_START.get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0)
}

pub fn current_frame() -> u64 {
    FRAME_INDEX.load(Ordering::Relaxed)
}
```

### 12.2 Thread ownership

| Thread | Reads | Writes |
|---|---|---|
| Render thread | `FRAME_INDEX` (Relaxed) | `FRAME_INDEX` (fetch_add), `EventSender` (try_send) |
| ZMQ REP thread | `FRAME_INDEX` (Relaxed) | `EventSender` (try_send) |
| Shared-memory / gamepad thread | `FRAME_INDEX` (Relaxed) | `EventSender` (try_send) |
| Messenger thread | `EventReceiver` (blocking recv) | Log file, ZMQ PUB, SQLite |
| Replay driver | Log file (sequential read) | `SceneState` (via render thread) |

No locks are held during event emission. The channel is the only synchronisation point, and
it is non-blocking on the sender side.

### 12.3 Render thread emit budget

On a 240 Hz display the render thread has a 4.17 ms frame budget. A `try_send` into a
`crossbeam_channel::bounded` takes approximately 20–50 ns under contention. At Trace level
with 6 events per frame, the total emit overhead is ~300 ns = 0.007% of the frame budget.
This is negligible.

---

## 13. Crate Dependencies

```toml
# Event channel
crossbeam-channel = "0.5"     # lock-free bounded MPSC

# Wire format
flatbuffers = "24"             # FlatBuffers runtime + generated code
                               # Build tool: `flatc` compiler (invoked in build.rs)

# SQLite (optional export and deferred real-time)
rusqlite = { version = "0.32", optional = true, features = ["bundled"] }

# Session IDs
uuid = { version = "1", features = ["v4"] }

# SHA-256 for file asset fingerprinting (replay integrity)
sha2 = "0.10"
```

`flatc` (the FlatBuffers schema compiler) must be installed on the build machine. `build.rs`
invokes it to generate `src/proto/vstimd_log_generated.rs` from `fbs/vstimd_log.fbs`.
Alternatively, pre-generated Rust code can be committed to the repository, which removes the
`flatc` build-time dependency.

---

## 14. Schema: FlatBuffers `.fbs` File

```fbs
// fbs/vstimd_log.fbs
// FlatBuffers schema for the vstimd event log.
// Wire format for .wllog files and ZMQ PUB event stream.

namespace vstimd.log;

// ── Shared types ─────────────────────────────────────────────────────────────

struct Vec2 { x: float32; y: float32; }
struct Rgba { r: float32; g: float32; b: float32; a: float32; }

enum LogLevel : uint8 { Error = 0, Warn = 1, Info = 2, Debug = 3, Trace = 4 }

enum PositionSource : uint8 {
    SharedMemory = 0,
    ZmqSub       = 1,
    Mouse        = 2,
    Gamepad      = 3,
}

// ── Event body union ─────────────────────────────────────────────────────────

union EventBody {
    SessionStart,
    SessionEnd,
    FrameBegin,
    CommandReceived,
    CommandResponse,
    ObjectCreated,
    ObjectDeleted,
    DeferredMode,
    DeferredFlip,
    BackgroundChanged,
    PositionSample,
    DroppedFrame,
    ServerError,
    PhotodiodeChanged,
}

// ── Top-level record (one per length-prefixed entry in the file) ─────────────

table LogEvent {
    timestamp_ns: uint64;
    frame_index:  uint64;
    level:        LogLevel;
    body:         EventBody;
}

root_type LogEvent;

// ── Event body tables ─────────────────────────────────────────────────────────

table SessionStart {
    server_version:     string;
    screen_width:       uint32;
    screen_height:      uint32;
    refresh_rate_hz:    float32;
    zmq_rep_addr:       string;
    zmq_pub_addr:       string;
    session_id:         [uint8];  // 16-byte UUID
    start_unix_ns:      uint64;   // wall-clock for external instrument correlation
}

table SessionEnd {
    total_frames: uint64;
    duration_ns:  uint64;
}

table FrameBegin {
    frame_index:       uint64;
    vsync_interval_ns: uint64;
}

table CommandReceived {
    // Raw protobuf-encoded vstimd::Request bytes, stored verbatim.
    // The replay driver decodes these with prost::Message::decode.
    bytes: [uint8] (required);
}

table CommandResponse {
    bytes: [uint8] (required);
}

table ObjectCreated {
    handle:      uint32;
    object_type: string;   // e.g. "RectStimulus", "AnimHarmonic"
}

table ObjectDeleted {
    handle: uint32;
}

table DeferredMode {
    started: bool;
}

table DeferredFlip {}

table BackgroundChanged {
    colour: Rgba;
}

table PositionSample {
    anim_handle: uint32;
    pos:         Vec2;
    source:      PositionSource;
}

table DroppedFrame {
    frame_index:   uint64;
    frame_time_ns: uint64;
    budget_ns:     uint64;
}

table ServerError {
    code:    int16;
    message: string;
}

table PhotodiodeChanged {
    lit: bool;
}
```

The use of a `union` for `EventBody` means the FlatBuffers runtime generates a tag byte
plus an offset to the correct table. Accessing the body is a single pointer dereference
into the existing buffer — zero allocation, zero copy.

---

## 15. Impact on Existing Architecture

### 15.1 `SceneState`

Gains an `EventSender` field. All methods that change state emit the appropriate event:

```rust
pub struct SceneState {
    // ... existing fields ...
    pub event_tx: EventSender,
}
```

`handle_request` emits `CommandReceived` (with the raw request bytes) before processing and
`CommandResponse` after. Object creation methods emit `ObjectCreated`. Deferred-mode entry
and exit emit `DeferredMode`. The render thread's flip step emits `DeferredFlip`.

### 15.2 Render thread (per-frame additions)

```rust
fn begin_frame(frame_index: u64, scene: &SceneState) {
    FRAME_INDEX.fetch_add(1, Ordering::Relaxed);
    emit_trace!(&scene.event_tx, Event::FrameBegin {
        frame_index,
        timestamp_ns:      session_elapsed_ns(),
        vsync_interval_ns: measure_vsync_interval(),
    });
}
```

### 15.3 Position animations

Each position animation (`AnimExternalPos`, `AnimZmqPos`, `AnimMousePos`, `AnimGamepadPos`)
gains:

```rust
pub injected_pos: Option<(f32, f32)>,
```

In `advance()`:

```rust
let (x, y) = match self.injected_pos.take() {
    Some(pos) => pos,          // replay: use the logged value
    None      => self.read(),  // live: read from shm / zmq / input device
};

emit_trace!(&event_tx, Event::PositionSample {
    frame_index:  current_frame(),
    timestamp_ns: session_elapsed_ns(),
    anim_handle:  self.handle,
    x, y,
    source: Self::SOURCE,
});
```

### 15.4 New `main.rs` startup path

```rust
fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if let Some(path) = &args.replay {
        return replay::run(path, &args);
    }

    let session_id           = uuid::Uuid::new_v4();
    let (event_tx, event_rx) = event_channel();
    let messenger_cfg        = MessengerConfig::from_args(&args, session_id);

    std::thread::Builder::new()
        .name("messenger".into())
        .spawn(move || messenger::run(event_rx, messenger_cfg))?;

    let scene = Arc::new(RwLock::new(SceneState::new(event_tx)));
    // ... rest unchanged ...
}
```

### 15.5 Module layout additions

```
src/
├── logging/
│   ├── mod.rs              # Event enum, EventKind, LogLevel, emit!, emit_trace!
│   │                       # FRAME_INDEX, SESSION_START, session_elapsed_ns()
│   ├── messenger.rs        # run_messenger, MessengerConfig, FlatBufferBuilder loop
│   ├── encode.rs           # encode_event: Event → FlatBuffers in the builder
│   ├── file_writer.rs      # BufWriter<File> with length-prefix framing
│   ├── zmq_pub.rs          # ZMQ PUB socket publisher
│   └── sqlite_writer.rs    # Deferred batch SQLite writer (feature = "rusqlite")
│
├── replay/
│   ├── mod.rs              # run_replay entry point, timing modes
│   ├── driver.rs           # ReplayDriver: lookahead buffer, dispatch
│   └── log_reader.rs       # LogFileReader: reads and decodes .wllog records
│
└── bin/
    └── convert.rs          # vstimd-convert: .wllog → SQLite
```

---

## 16. Open Questions

### 16.1 Log rotation for long experiments

An experiment at Trace level running for 8 hours produces roughly 1–6 GB depending on the
number of active position animations. Options:

- **Client-controlled**: the experiment script calls `CmdStartLog` / `CmdStopLog` around
  each block of trials. Simple, explicit, no server logic needed.
- **Size-based rotation**: the messenger thread starts a new file when the current file
  exceeds a configurable size limit (e.g. 2 GB). Files are numbered with a suffix.
- **Time-based rotation**: new file every N minutes.

Recommendation: implement `CmdStartLog` / `CmdStopLog` first. Add automatic rotation when
real experiment sizes are known.

### 16.2 Compression

LZ4 compression (via `lz4_flex`) applied to each 64 KB block could reduce file size by 3–5×
with negligible CPU overhead (~100 MB/s compression on a single core, well within the
messenger thread's budget). This is a straightforward addition once the basic format is
stable. The format version byte in the file header reserves space for this.

### 16.3 `flatc` as a build dependency

`flatc` must be present at build time. Options:

- **Require `flatc`**: document it in the README. Fine for a lab tool.
- **Pre-generate and commit**: commit `src/logging/vstimd_log_generated.rs` to the repository.
  `build.rs` only re-runs `flatc` if `fbs/vstimd_log.fbs` has changed. Removes the build-time
  dependency for users who do not modify the schema.
- **Use a `flatbuffers` proc-macro**: the `planus` crate offers a proc-macro approach that
  does not require a separate compiler binary.

Recommendation: pre-generate and commit the generated file. This is the most robust approach
for a scientific software tool where reproducible builds matter.

### 16.4 File asset fingerprinting for replay integrity

File-loaded stimuli (bitmaps, WGSL shaders, glTF meshes, `.ply` Gaussian splat scenes,
animation path files) are referenced by path in the log. If the file has moved or changed,
replay diverges silently. The `ObjectCreated` event for file-loaded stimuli should include:

```fbs
table ObjectCreated {
    handle:      uint32;
    object_type: string;
    file_path:   string;     // original path at load time
    file_sha256: [uint8];    // 32-byte SHA-256 hash of the file contents at load time
}
```

`vstimd-convert` and the replay driver both check the hash and emit a warning if it does not
match the file currently on disk.

### 16.5 Dropped event reporting

When the event channel fills up, `emit` silently drops events and increments
`DROPPED_EVENT_COUNT`. This counter should be reported:

- In the egui overlay (visible in real time).
- As a `Warn`-level `ServerError` event emitted by the messenger thread when it next has
  capacity (the messenger thread checks the counter periodically).
- In the `SessionEnd` event's summary.

This gives the operator visibility into whether the log has gaps, without requiring the
render thread to do anything beyond an atomic increment.

---

*End of document. See `PLAN.md` for integration into the implementation schedule (Phases
13–16), `STIMULUS_DATA_MODEL.md` for the composition model, `INPUT_LATENCY.md` for position
control design, and `3D_ROADMAP.md` for how file-loaded 3-D assets interact with replay.*