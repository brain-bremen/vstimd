use std::time::{Duration, Instant};

use nvml_wrapper::Nvml;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Pid, ProcessRefreshKind, RefreshKind, System};

pub struct SystemMetrics {
    pub cpu_pct: f32,
    pub process_cpu_pct: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub process_rss_mb: u64,
    pub gpu_util_pct: Option<f32>,
    pub gpu_mem_used_mb: Option<u64>,
    pub gpu_mem_total_mb: Option<u64>,
}

pub struct MetricsSampler {
    sys: System,
    pid: Pid,
    last_sample: Instant,
    cached: SystemMetrics,
    nvml: Option<Nvml>,
}

impl SystemMetrics {
    fn zero() -> Self {
        Self {
            cpu_pct: 0.0,
            process_cpu_pct: 0.0,
            ram_used_mb: 0,
            ram_total_mb: 0,
            process_rss_mb: 0,
            gpu_util_pct: None,
            gpu_mem_used_mb: None,
            gpu_mem_total_mb: None,
        }
    }
}

impl Default for MetricsSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsSampler {
    pub fn new() -> Self {
        let pid = sysinfo::get_current_pid().unwrap_or(Pid::from(0));
        let mut sys = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        // First refresh so the second one can compute a CPU delta.
        sys.refresh_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );

        let nvml = Nvml::init()
            .map_err(|e| log::debug!("NVML unavailable, GPU metrics disabled: {e}"))
            .ok();

        Self {
            sys,
            pid,
            last_sample: Instant::now(),
            cached: SystemMetrics::zero(),
            nvml,
        }
    }

    /// Returns cached metrics, refreshing at most twice per second.
    pub fn sample(&mut self) -> &SystemMetrics {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_sample) < Duration::from_millis(500) {
            return &self.cached;
        }
        self.last_sample = now;

        self.sys.refresh_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        self.sys.refresh_process_specifics(
            self.pid,
            ProcessRefreshKind::new().with_cpu().with_memory(),
        );

        let cpus = self.sys.cpus();
        let cpu_pct = if cpus.is_empty() {
            0.0
        } else {
            cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
        };

        let (proc_cpu, proc_rss) = self
            .sys
            .process(self.pid)
            .map(|p| (p.cpu_usage(), p.memory()))
            .unwrap_or((0.0, 0));

        let (gpu_util_pct, gpu_mem_used_mb, gpu_mem_total_mb) =
            self.nvml.as_ref().and_then(|nvml| {
                let dev = nvml.device_by_index(0).ok()?;
                let util = dev.utilization_rates().ok()?.gpu as f32;
                let mem = dev.memory_info().ok()?;
                Some((Some(util), Some(mem.used / 1_048_576), Some(mem.total / 1_048_576)))
            }).unwrap_or((None, None, None));

        self.cached = SystemMetrics {
            cpu_pct,
            process_cpu_pct: proc_cpu,
            ram_used_mb: self.sys.used_memory() / 1_048_576,
            ram_total_mb: self.sys.total_memory() / 1_048_576,
            process_rss_mb: proc_rss / 1_048_576,
            gpu_util_pct,
            gpu_mem_used_mb,
            gpu_mem_total_mb,
        };
        &self.cached
    }
}
