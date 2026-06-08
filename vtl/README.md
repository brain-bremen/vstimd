# vtl — Virtual Trigger Lines

A lock-free POSIX shared memory segment for exchanging hardware trigger line state
between vstimd and companion daemons (e.g. `nidaqd` for NI-DAQ hardware).

## Concepts

### Input lines

Represent signals arriving *into* vstimd from the outside world.

| Role | Actor |
|---|---|
| Canonical writer | `nidaqd` — writes `input_state` and sets latches when a DAQ edge is detected |
| Software writer | ZMQ `SetInput*` commands — simulate a hardware trigger for testing |
| Intended reader | vstimd render loop — calls `VtlState::poll()` once per frame at the **start** of each frame, drains rise/fall latches, and feeds detected edges to the animation system |

### Output lines

Represent signals driven *by* vstimd to report what is on screen.

| Role | Actor |
|---|---|
| Canonical writer | vstimd render loop — calls `VtlState::write_outputs()` once per frame at the **end** of each frame, after present/vsync, to commit the frame's output state |
| Software writer | ZMQ `SetOutput*` commands — manual override for testing |
| Intended reader | `nidaqd` — polls `output_state` to pulse NI-DAQ hardware lines (frame-sync pulses, stimulus-onset markers) |

> **Current status:** `VtlState::poll()` and `VtlState::write_outputs()` are implemented
> but not yet wired into the render loop.  Currently both directions are accessible only
> via ZMQ commands.

### Shared memory layout

Each bank is a `u64` bitmask: up to 64 lines per bank, up to 4 banks.
vstimd is created with `num_input_banks=4, num_output_banks=1`.

## Shared memory layout

The segment lives at `/dev/shm/vstimd_vtl` (POSIX shm name `/vstimd_vtl`).
Total size: **0x5000 bytes** (5 × 4 KiB pages).

```
Offset      Size    Field
──────────────────────────────────────────────────────────
0x0000       128    Header
0x0080    15424    Names section  (64-byte prefix + 256 × 60-byte entries)
0x4000       160    State section  (5 arrays × 4 banks × 8 bytes)
```

### Header (offset 0x0000, 128 bytes)

```
+00  u32  magic            0x56544C31  ("VTL1")
+04  u32  version          1
+08  u32  num_input_banks
+0C  u32  num_output_banks
+10  u64  seqlock          reserved (always 0)
+18  104  _pad
```

### Names section (offset 0x0080)

```
+00  u32  n_entries        number of valid name entries (atomic, Release/Acquire)
+04  60   _pad
+40  60   entries[0]       first VtlLineEntry
+7C  60   entries[1]
 …
```

Each `VtlLineEntry` (60 bytes):

```
+00  56   name             null-terminated UTF-8, all-zero if unused
+38   1   bank             0..3
+39   1   bit              0..63
+3A   1   direction        0 = Input, 1 = Output
+3B   1   _pad
```

### State section (offset 0x4000, 160 bytes)

Five `[u64; 4]` arrays, each element is an `AtomicU64`:

```
+0x00  32   input_state[4]       current input levels
+0x20  32   input_rise_latch[4]  sticky rising-edge latches (OR-set by writer, fetch_and-clear by reader)
+0x40  32   input_fall_latch[4]  sticky falling-edge latches
+0x60  32   output_state[4]      current output levels
+0x80  32   output_set_pulse[4]  one-shot output pulses
```

## Reading from the terminal

### Quick sanity check (xxd)

```bash
# Verify magic "VTL1" at byte 0 and inspect the first 32 bytes:
xxd -l 32 /dev/shm/vstimd_vtl
# Expected first 4 bytes: 31 4c 54 56 (little-endian "VTL1")
```

### Read the state section

The state section starts at byte **0x4000 = 16384**.
Each u64 is little-endian.

```bash
# input_state bank 0 (bytes 0x4000..0x4008)
xxd -s 0x4000 -l 8 /dev/shm/vstimd_vtl

# All 5 arrays × 4 banks (160 bytes from 0x4000):
xxd -s 0x4000 -l 160 /dev/shm/vstimd_vtl
```

Interpret the output in groups of 8 bytes (one u64 per bank, little-endian):

```
Line 0x4000:  XX XX XX XX XX XX XX XX  ← input_state[bank=0]
Line 0x4008:  XX XX XX XX XX XX XX XX  ← input_state[bank=1]
Line 0x4010:  XX XX XX XX XX XX XX XX  ← input_state[bank=2]
Line 0x4018:  XX XX XX XX XX XX XX XX  ← input_state[bank=3]
Line 0x4020:  ...                      ← input_rise_latch[bank=0]
...
Line 0x4060:  XX XX XX XX XX XX XX XX  ← output_state[bank=0]
```

### Read named lines

The names section starts at byte **0x80 = 128**.
The first 4 bytes are `n_entries` (u32 LE), then a 60-byte pad, then 256 × 60-byte entries.

```bash
# n_entries (how many lines are registered):
python3 -c "
import struct, mmap, os
fd = os.open('/dev/shm/vstimd_vtl', os.O_RDONLY)
with mmap.mmap(fd, 0, access=mmap.ACCESS_READ) as m:
    n = struct.unpack_from('<I', m, 0x80)[0]
    print(f'{n} named lines')
    entry_base = 0x80 + 64  # skip 64-byte names-section header
    for i in range(n):
        off = entry_base + i * 60
        name = m[off:off+56].split(b'\x00')[0].decode()
        bank, bit, direction = m[off+56], m[off+57], m[off+58]
        dir_str = 'INPUT' if direction == 0 else 'OUTPUT'
        print(f'  [{i}] {name!r:30s}  bank={bank} bit={bit} {dir_str}')
os.close(fd)
"
```

### Watch input_state in a loop

```bash
watch -n 0.1 "python3 -c \"
import struct, mmap, os
fd = os.open('/dev/shm/vstimd_vtl', os.O_RDONLY)
with mmap.mmap(fd, 0, access=mmap.ACCESS_READ) as m:
    for bank in range(4):
        val = struct.unpack_from('<Q', m, 0x4000 + bank * 8)[0]
        print(f'input_state[{bank}] = {val:#018x}  ({bin(val)})')
os.close(fd)
\""
```

### One-liner with od

```bash
# input_state[0] as a single 64-bit little-endian integer:
od -j 0x4000 -N 8 -t x8 /dev/shm/vstimd_vtl
```

## Using the crate

### Owner (vstimd)

```rust
use vtl::VtlOwner;

let owner = VtlOwner::create("/vstimd_vtl", 4, 1)?;

// Software-trigger an input line (simulates hardware input):
let rose = owner.set_input_bit(0, 3);  // bank=0, bit=3

// Drive an output line:
owner.set_output_state(0, 1 << 5);    // set bit 5 of output bank 0

// Register a name visible to other tools:
owner.write_named_line(0, "stim_onset", 0, 0, vtl::Direction::Output);
owner.set_n_named_lines(1);

// Per-frame: drain latches accumulated since the last poll:
let rising  = owner.drain_input_rise(0, u64::MAX);
let falling = owner.drain_input_fall(0, u64::MAX);
```

### Client (nidaqd or test tool)

```rust
use vtl::VtlClient;

let client = VtlClient::open("/vstimd_vtl")?;

// Read current output state and drive hardware accordingly:
let out = client.output_state(0);

// Latch a hardware edge into the input banks:
client.set_input_bit(0, 7);
client.set_input_rise(0, 1 << 7);
```
