mod bridge;
mod config;

use std::{fs, time::Duration};

use anyhow::{Context, Result};
use log::{error, info};
use vtl::VtlOwner;

const DEFAULT_CONFIG_PATH: &str = "/etc/braemons/gpiochip-daqd/gpiochip-daqd.toml";
const OUTPUT_POLL_INTERVAL: Duration = Duration::from_millis(1);
const VTL_OPEN_ATTEMPTS: u32 = 30;

struct Args {
    config: String,
    standalone: bool,
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut config = DEFAULT_CONFIG_PATH.to_string();
    let mut standalone = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--standalone" => standalone = true,
            "-c" | "--config" => {
                config = args.next()
                    .ok_or_else(|| anyhow::anyhow!("{arg} requires a path argument"))?;
            }
            other => anyhow::bail!("unknown argument: {other}\nUsage: gpiochip-daqd [-c <config>] [--standalone]"),
        }
    }

    Ok(Args { config, standalone })
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let Args { config: config_path, standalone } = parse_args()?;

    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("read config {config_path}"))?;
    let cfg: config::Config =
        toml::from_str(&raw).with_context(|| format!("parse config {config_path}"))?;

    info!(
        "gpiochip-daqd: VTL={} chip={}  ({} output(s), {} input(s)){}",
        cfg.vtl.shm_name,
        cfg.gpio.chip,
        cfg.outputs.len(),
        cfg.inputs.len(),
        if standalone { "  [standalone]" } else { "" },
    );
    for o in &cfg.outputs {
        info!(
            "  out  {:>3} '{}' → VTL bank={} bit={}",
            o.gpio_line, o.name, o.vtl_bank, o.vtl_bit
        );
    }
    for i in &cfg.inputs {
        info!(
            "  in   {:>3} '{}' → VTL bank={} bit={} edge={:?}",
            i.gpio_line, i.name, i.vtl_bank, i.vtl_bit, i.edge
        );
    }

    let config::Config { vtl: vtl_cfg, gpio, outputs, inputs } = cfg;

    // In standalone mode create the VTL segment ourselves so gpiochip-daqd
    // can run without vstimd (e.g. for GPIO loopback testing).
    // _owner must stay alive until main returns — dropping it unlinks the shm.
    let _owner: Option<VtlOwner> = if standalone {
        let banks = required_banks_from_config(&outputs, &inputs);
        let owner = VtlOwner::create(&vtl_cfg.shm_name, banks, banks)
            .with_context(|| format!("create VTL segment '{}' in standalone mode", vtl_cfg.shm_name))?;
        info!("standalone: created VTL segment '{}' ({banks} bank(s))", vtl_cfg.shm_name);
        Some(owner)
    } else {
        None
    };

    let vtl = if standalone {
        // The segment now exists; open a client view for the output loop.
        vtl::VtlClient::open(&vtl_cfg.shm_name)
            .context("open VTL client for standalone segment")?
    } else {
        bridge::open_vtl_with_retry(&vtl_cfg.shm_name, VTL_OPEN_ATTEMPTS)?
    };

    // Spawn one blocking thread per input line; each gets its own VtlClient.
    let mut _watchers = Vec::new();
    for inp in inputs {
        let client = vtl::VtlClient::open(&vtl_cfg.shm_name)
            .context("open VTL client for input watcher")?;
        _watchers.push(bridge::spawn_input_watcher(gpio.chip.clone(), inp, client));
    }

    #[cfg(target_os = "linux")]
    sd_notify::notify(false, &[sd_notify::NotifyState::Ready])?;

    // Output loop on the main thread — runs until GPIO error.
    if let Err(e) = bridge::run_output_loop(&gpio.chip, &outputs, &vtl, OUTPUT_POLL_INTERVAL) {
        error!("output loop error: {e:#}");
        return Err(e);
    }

    Ok(())
}

/// Derive the minimum number of VTL banks needed to cover all configured lines.
fn required_banks_from_config(outputs: &[config::OutputLine], inputs: &[config::InputLine]) -> u32 {
    let max_out = outputs.iter().map(|o| o.vtl_bank).max().unwrap_or(0);
    let max_in  = inputs.iter().map(|i| i.vtl_bank).max().unwrap_or(0);
    (max_out.max(max_in) as u32) + 1
}
