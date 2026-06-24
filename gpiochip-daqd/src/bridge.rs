use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use gpio_cdev::{Chip, EventRequestFlags, EventType, LineRequestFlags};
use log::{debug, info, warn};
use vtl::{VtlClient, VtlSegment};

use crate::config::{Edge, InputLine, OutputLine};

const CONSUMER: &str = "gpiochip-daqd";

/// SCHED_FIFO priority for the output polling thread (timing-critical).
pub const PRIO_OUTPUT: i32 = 60;
/// SCHED_FIFO priority for input watcher threads (interrupt-driven, less critical).
pub const PRIO_INPUT: i32 = 50;

/// Attempt to promote the calling thread to `SCHED_FIFO` at `priority` (1–99).
///
/// Fails silently with a warning if the process lacks `CAP_SYS_NICE` (e.g.
/// running unprivileged in development).  Always succeeds when launched via the
/// systemd unit, which grants `CAP_SYS_NICE` via `CapabilityBoundingSet`.
#[cfg(target_os = "linux")]
pub fn set_thread_realtime(priority: i32) {
    let param = libc::sched_param { sched_priority: priority };
    let ret = unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &param) };
    if ret != 0 {
        warn!(
            "sched_setscheduler(SCHED_FIFO, {priority}) failed: {} \
             (running without CAP_SYS_NICE?)",
            std::io::Error::last_os_error()
        );
    } else {
        info!("thread promoted to SCHED_FIFO priority {priority}");
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set_thread_realtime(_priority: i32) {}

/// Drives GPIO output pins from VTL `output_state`, polled every `interval`.
///
/// Runs forever (or until a GPIO error). Intended for the main thread after
/// input watchers have been spawned.
pub fn run_output_loop(
    chip_path: &str,
    outputs: &[OutputLine],
    vtl: &VtlSegment,
    interval: Duration,
) -> Result<()> {
    set_thread_realtime(PRIO_OUTPUT);

    if outputs.is_empty() {
        info!("no output lines configured, output loop idle");
        loop {
            thread::sleep(Duration::from_secs(60));
        }
    }

    let mut chip = Chip::new(chip_path)
        .with_context(|| format!("open GPIO chip {chip_path}"))?;

    let handles: Vec<_> = outputs
        .iter()
        .map(|o| {
            let line = chip
                .get_line(o.gpio_line)
                .with_context(|| format!("get GPIO line {} for '{}'", o.gpio_line, o.name))?;
            let h = line
                .request(LineRequestFlags::OUTPUT, 0, CONSUMER)
                .with_context(|| format!("request output GPIO line {} ('{}')", o.gpio_line, o.name))?;
            info!(
                "output '{}': VTL bank={} bit={} → GPIO line {}",
                o.name, o.vtl_bank, o.vtl_bit, o.gpio_line
            );
            Ok(h)
        })
        .collect::<Result<_>>()?;

    let mut prev = vec![0u8; handles.len()];

    loop {
        for (i, (out, handle)) in outputs.iter().zip(handles.iter()).enumerate() {
            let level = ((vtl.output_state(out.vtl_bank as usize) >> out.vtl_bit) & 1) as u8;
            if level != prev[i] {
                handle
                    .set_value(level)
                    .with_context(|| format!("set GPIO line {} ('{}')", out.gpio_line, out.name))?;
                prev[i] = level;
                debug!(
                    "out  {:>3} '{}' → {}",
                    out.gpio_line, out.name, if level == 1 { "HIGH" } else { "LOW" }
                );
            }
        }
        thread::sleep(interval);
    }
}

/// Spawns a thread that blocks on GPIO edge events for one input line and
/// writes `input_state` and rise/fall latches into the VTL segment.
///
/// The thread runs until an I/O error; join the handle to observe it.
pub fn spawn_input_watcher(
    chip_path: String,
    inp: InputLine,
    vtl: VtlClient,
) -> thread::JoinHandle<Result<()>> {
    thread::Builder::new()
        .name(format!("vtl-in:{}", inp.name))
        .spawn(move || run_input_loop(&chip_path, &inp, &vtl))
        .expect("spawn input watcher thread")
}

/// Public re-export of the input loop for integration tests.
#[allow(dead_code)]
pub fn run_input_loop_pub(chip_path: &str, inp: &InputLine, vtl: &VtlSegment) -> Result<()> {
    run_input_loop(chip_path, inp, vtl)
}

/// Drive all output pins exactly once from the current VTL `output_state`.
///
/// Opens a fresh `Chip`, drives every pin, and returns.  Use from tests to
/// exercise one tick of the output loop without spawning a thread.
#[allow(dead_code)]
pub fn poll_outputs_once(chip_path: &str, outputs: &[OutputLine], vtl: &VtlSegment) -> Result<()> {
    if outputs.is_empty() {
        return Ok(());
    }
    let mut chip = Chip::new(chip_path)
        .with_context(|| format!("open GPIO chip {chip_path}"))?;
    for out in outputs {
        let line = chip.get_line(out.gpio_line)
            .with_context(|| format!("get GPIO line {}", out.gpio_line))?;
        let handle = line
            .request(LineRequestFlags::OUTPUT, 0, CONSUMER)
            .with_context(|| format!("request GPIO line {}", out.gpio_line))?;
        let level = ((vtl.output_state(out.vtl_bank as usize) >> out.vtl_bit) & 1) as u8;
        handle.set_value(level)
            .with_context(|| format!("set GPIO line {}", out.gpio_line))?;
    }
    Ok(())
}

fn run_input_loop(chip_path: &str, inp: &InputLine, vtl: &VtlSegment) -> Result<()> {
    set_thread_realtime(PRIO_INPUT);

    let mut chip = Chip::new(chip_path)
        .with_context(|| format!("open GPIO chip {chip_path}"))?;

    let event_flags = match inp.edge {
        Edge::Rising  => EventRequestFlags::RISING_EDGE,
        Edge::Falling => EventRequestFlags::FALLING_EDGE,
        Edge::Both    => EventRequestFlags::BOTH_EDGES,
    };

    let line = chip
        .get_line(inp.gpio_line)
        .with_context(|| format!("get GPIO line {} for '{}'", inp.gpio_line, inp.name))?;

    let events = line
        .events(LineRequestFlags::INPUT, event_flags, CONSUMER)
        .with_context(|| format!("request events on GPIO line {} ('{}')", inp.gpio_line, inp.name))?;

    info!(
        "input '{}': GPIO line {} → VTL bank={} bit={} edge={:?}",
        inp.name, inp.gpio_line, inp.vtl_bank, inp.vtl_bit, inp.edge
    );

    let mask = 1u64 << inp.vtl_bit;
    let bank = inp.vtl_bank as usize;

    for event in events {
        match event.context("GPIO event read error")?.event_type() {
            EventType::RisingEdge => {
                debug!("in   {:>3} '{}' ↑ HIGH", inp.gpio_line, inp.name);
                vtl.set_input_bit(bank, inp.vtl_bit);
                vtl.set_input_rise(bank, mask);
            }
            EventType::FallingEdge => {
                debug!("in   {:>3} '{}' ↓ LOW", inp.gpio_line, inp.name);
                vtl.clear_input_bit(bank, inp.vtl_bit);
                vtl.set_input_fall(bank, mask);
            }
        }
    }

    Err(anyhow::anyhow!(
        "GPIO event stream ended unexpectedly for '{}'",
        inp.name
    ))
}

/// Opens the VTL segment, retrying until vstimd creates it.
pub fn open_vtl_with_retry(shm_name: &str, max_attempts: u32) -> Result<VtlClient> {
    for attempt in 1..=max_attempts {
        match VtlClient::open(shm_name) {
            Ok(c) => return Ok(c),
            Err(e) if attempt < max_attempts => {
                warn!(
                    "VTL segment '{}' not available ({}), retrying ({}/{})…",
                    shm_name, e, attempt, max_attempts
                );
                thread::sleep(Duration::from_secs(1));
            }
            Err(e) => {
                return Err(e).with_context(|| format!("open VTL segment '{shm_name}'"));
            }
        }
    }
    unreachable!()
}
