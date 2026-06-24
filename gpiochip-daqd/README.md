# gpiochip-daqd

Bridges [VTL](../vtl/) shared-memory trigger lines to physical GPIO pins via
the Linux GPIO character device (`/dev/gpiochipN`).

- **Output lines**: vstimd writes `output_state`; this daemon drives the
  corresponding GPIO pins to match.
- **Input lines**: kernel edge events on GPIO pins are written into VTL
  `input_state` and rise/fall latches with no busy-wait.

## Compatible devices

Any Linux board that exposes GPIO through the kernel character device interface
(`CONFIG_GPIO_CDEV`).  Tested configurations:

| Device | Chip | Notes |
|--------|------|-------|
| NVIDIA Jetson Orin Nano | `/dev/gpiochip0` (tegra234-gpio, 164 lines) | See `config/jetson-orin-nano_in16_out4.toml` |
| Raspberry Pi 4 / earlier | `/dev/gpiochip0` (BCM2835/BCM2711) | Line offsets = BCM GPIO numbers |
| Raspberry Pi 5 | `/dev/gpiochip4` (RP1) | Note: different chip path |

Run `gpioinfo /dev/gpiochipN` to list line offsets on your board.

## Usage

```bash
# Default config path: /etc/braemons/gpiochip-daqd/gpiochip-daqd.toml
gpiochip-daqd

# Custom config
gpiochip-daqd -c config/jetson-orin-nano_in16_out4.toml

# Standalone mode: create the VTL segment without vstimd (for GPIO testing)
gpiochip-daqd -c config/jetson-orin-nano_in16_out4.toml --standalone
```

Set `RUST_LOG=gpiochip_daqd=debug` to log every pin change.

## Configuration

The config format is device-agnostic. Only `chip` and `gpio_line` values
differ between boards.

```toml
[vtl]
shm_name = "/vstimd_vtl"

[gpio]
chip = "/dev/gpiochip0"

[[outputs]]
name      = "stim_onset"   # must match the name vstimd registers in VTL
vtl_bank  = 0
vtl_bit   = 36             # vtl_bit = 40-pin header pin number (by convention)
gpio_line = 53             # kernel line offset within the chip

[[inputs]]
name      = "scanner_trigger"
vtl_bank  = 0
vtl_bit   = 29
gpio_line = 105
edge      = "both"         # "rising" | "falling" | "both"
```

### VTL bit numbering convention

`vtl_bit` is set equal to the physical header pin number of the board.
This makes the mapping self-documenting: bit 29 in the VTL corresponds to
header pin 29 on the connector, no lookup needed.

Inputs and outputs use independent VTL arrays (`input_state` vs
`output_state`), so the same bit number can appear in both without conflict.

See the device config files in `config/` for complete pin-to-line mappings.

## Building

```bash
cargo build --release -p gpiochip-daqd
```

### Hardware loopback tests

Requires two GPIO pins wired together:

```bash
export LOOPBACK_CHIP=/dev/gpiochip0
export LOOPBACK_OUT=137   # header pin 26 on Jetson Orin Nano
export LOOPBACK_IN=105    # header pin 29 on Jetson Orin Nano
cargo test --features hw-tests -p gpiochip-daqd -- --test-threads=1
```

## Systemd

The unit file at `packaging/systemd/gpiochip-daqd.service` starts after
`vstimd.service` and stops automatically if vstimd exits (`BindsTo`).

Real-time scheduling (`SCHED_FIFO`) is requested via the unit and per-thread
in Rust; `CAP_SYS_NICE` is granted via `AmbientCapabilities`.

## Future DAQ backends

The VTL bridge contract (shared memory layout in `vtl/`) is device-agnostic.
Other backends can follow the same pattern:

- `nidaqmx-daqd` — National Instruments DAQmx
- `labjack-daqd` — LabJack T-series
