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
The Jetson Orin Nano has 6 Cortex-A78AE cores.  Pinning threads to dedicated
cores eliminates OS migration overhead and prevents cache thrashing between
the GPIO bridge and vstimd's render loop.

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

3. **Kernel** (optional, best latency): add `isolcpus=4,5 nohz_full=4,5
   rcu_nocbs=4,5` to `/boot/extlinux/extlinux.conf` on the Jetson.  Removes
   cores 4–5 from the general scheduler entirely; worst-case output jitter drops
   from ~200 µs to ~10 µs.  Requires a reboot and affects all processes.

Config addition needed:
```toml
[affinity]
output_core = 5   # optional; omit to disable pinning
input_core  = 4
```

### Output timing accuracy
The 1 ms `thread::sleep` drifts under load (std sleep is a minimum, not exact).
For tighter output timing consider:
- `timerfd_create(CLOCK_MONOTONIC)` with a fixed interval replacing `sleep`
- Accepting that outputs are inherently coarser than interrupt-driven inputs
  and documenting the expected worst-case latency

### Per-line GPIO chip
All lines must currently share the same `[gpio] chip`.  Some Jetson header pins
are on `gpiochip1` (tegra234-gpio-aon).  Config should allow per-line chip
override:
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
`tests/loopback.rs` documents GPIO line *offsets* but not which 40-pin header
pins they correspond to on Jetson Orin Nano.  Add a mapping table so it's easy
to pick a safe loopback pair without consulting the datasheet.

### Output pulse test
No test exercises `output_set_pulse` / `poll_outputs_once` for one-shot pulse
behaviour (drive high → delay → drive low).

### Latency measurement test
Add a hardware test that timestamps the GPIO event (`LineEvent::timestamp`) and
compares it to the `Instant` before `set_value` to characterise round-trip
latency of the loopback path.

### CI for hw-tests
Hardware tests are manually opt-in only.  If a Jetson with loopback wiring is
available in CI, wire up `--features hw-tests` with the appropriate env vars.
