# gpiochip-daqd — TODO

## Missing features

### Output pulse support
`VtlSegment::output_set_pulse` / `drain_output_pulse` are not handled.
vstimd can fire one-shot TTL pulses (e.g. frame-sync markers) via this field,
but the output loop currently only mirrors sustained `output_state` levels.
The output loop needs a second check: drain pulse bits → drive pin high → short
delay → drive pin low.

### Input watcher restart
If a GPIO event handle returns an I/O error the input thread exits permanently.
The main thread should detect the dead `JoinHandle` and re-spawn it with
back-off.

### VTL segment reconnect
If vstimd restarts it unlinks and recreates `/dev/shm/vstimd_vtl`.  All open
`VtlClient` handles become stale (they map the old anonymous segment).  The
daemon needs to detect the stale mapping (e.g. re-check magic periodically) and
re-open with `open_vtl_with_retry`.

### Graceful shutdown
No SIGTERM/SIGINT handler.  The daemon should drain output pins to 0 on exit
so downstream equipment isn't left with a stale high signal.

### CPU core affinity
Pinning threads to dedicated cores eliminates OS migration overhead and
prevents cache thrashing between the GPIO bridge and vstimd's render loop.

Proposed layout:

| Thread / process  | Core(s) | Notes |
|-------------------|---------|-------|
| vstimd render     | 0–3     | Heavy; leave the high cores free |
| gpiochip-daqd input watchers | 4 | Interrupt-driven; stable L1 cache |
| gpiochip-daqd output loop    | 5 | Timing-critical; dedicated core |

Implementation — two layers:

1. **systemd** (`gpiochip-daqd.service`): `CPUAffinity=4 5` — confines the whole
   process to cores 4–5 at the OS scheduler level, keeping it off vstimd's cores.
   No code change required.

2. **Rust** (`bridge.rs`): call `libc::sched_setaffinity(0, &cpuset)` at the top
   of `run_output_loop` (core 5 only) and `run_input_loop` (core 4 only).
   `libc` is already a dependency.  Example:
   ```rust
   let mut set = unsafe { std::mem::zeroed::<libc::cpu_set_t>() };
   unsafe { libc::CPU_SET(core, &mut set); }
   unsafe { libc::sched_setaffinity(0, std::mem::size_of_val(&set), &set); }
   ```
   Expose the core assignments as config fields (default `None` = no pinning)
   so they can be disabled without a recompile.

3. **Kernel** (optional, best latency): isolate cores from the general scheduler
   via `isolcpus`, `nohz_full`, `rcu_nocbs` kernel parameters.  Worst-case
   output jitter drops from ~200 µs to ~10 µs.  Requires a reboot.

Config addition needed:
```toml
[affinity]
output_core = 5   # optional; omit to disable pinning
input_core  = 4
```

### GPIO pull resistors
`gpio-cdev` 0.6 wraps the kernel v1 ABI and does not expose the bias flags
(`GPIOHANDLE_REQUEST_BIAS_PULL_UP/DOWN`, bits 5–7) added in Linux 5.5.
Currently using hardware pull-down resistors as a workaround.

Options to fix in software:
- **Raw bits**: pass `LineRequestFlags::from_bits_retain(flags.bits() | (1 << 6))`
  through to the ioctl — works on kernel ≥ 5.5, no new dependencies, but
  relies on undocumented crate internals.
- **Migrate to `gpiod` crate**: wraps `libgpiod` 2.x (v2 ABI), full bias +
  debounce + nanosecond edge timestamps. Requires building `libgpiod` 2.x
  from source — not in Ubuntu 22.04 repos, only 1.6.3 is installed.

Config addition needed (once implemented):
```toml
[[inputs]]
pull = "down"   # "up" | "down" | "none" (default: "none")
```

### ~~Output latency: replace polling with semaphore~~ ✓ Done
A process-shared POSIX semaphore (`output_sem`) lives in the VTL segment at
`OUTPUT_SEM_OFFSET`.  `VtlSegment` exposes `signal_output()` / `wait_output()`
/ `try_wait_output()`.  vstimd calls `write_outputs_immediate()` (write +
`sem_post`) at every output change; gpiochip-daqd blocks on `wait_output()`
instead of sleeping.  Round-trip latency drops from ~1 ms to ~50 µs with
SCHED_FIFO priority 60.

### Per-line GPIO chip
All lines must currently share the same `[gpio] chip`.  On some boards, pins
are spread across multiple chips.  Config should allow per-line chip override:
```toml
[[outputs]]
name      = "stim_onset"
gpio_chip = "/dev/gpiochip1"   # override
gpio_line = 5
```

### VTL named-line registration
The daemon should write its configured lines into the VTL names table
(`write_named_line` / `set_n_named_lines`) so that `gpioinfo`-style tooling and
vstimd's ZMQ `ListVtlLines` command can discover them by name.

### Debian packaging scripts
`Cargo.toml` references `packaging/debian/` maintainer scripts (postinst,
prerm) but those files do not exist yet.  At minimum `postinst` should create
the config directory and set GPIO group permissions.

## Known gaps in tests

### Hardware pin mapping table
`tests/loopback.rs` documents GPIO line *offsets* but not which header pins
they correspond to.  Device-specific config files (e.g.
`jetson-orin-nano_in16_out4.toml`) carry this mapping; link to them from the
test docs so it's easy to pick a safe loopback pair.

### Output pulse test
No test exercises `output_set_pulse` / `poll_outputs_once` for one-shot pulse
behaviour (drive high → delay → drive low).

### Latency measurement test
Add a hardware test that timestamps the GPIO event (`LineEvent::timestamp`) and
compares it to the `Instant` before `set_value` to characterise round-trip
latency of the loopback path.

### CI for hw-tests
Hardware tests are manually opt-in only.  If a board with loopback wiring is
available in CI, wire up `--features hw-tests` with the appropriate env vars.
