use super::display_info::StimulusDisplayInfo;
use super::RenderTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockSource {
    /// DRM kernel scanout event (`drmWaitVBlank`) — most accurate.
    DrmVblank,
    /// `VK_EXT_display_control` vblank fence — accurate, equivalent to DrmVblank.
    VkDisplayControl,
    /// `VK_KHR_present_wait` — accurate, GPU-side.
    PresentWait,
    /// `VK_GOOGLE_display_timing` — approximate.
    DisplayTiming,
    /// Fallback: `Instant::now()` after GPU fence — inaccurate.
    GpuCompletion,
}

pub struct SystemInfo {
    pub display: StimulusDisplayInfo,
    pub backend: RenderTarget,
    pub local_ip: String,
    pub hostname: String,
    pub gpu_name: String,
    /// Some(true/false) when wireframe toggle is supported; None on DRM or unsupported GPU.
    pub wireframe: Option<bool>,
    pub clock_source: ClockSource,
}

/// Resolve the default-route local IP by connecting a UDP socket (no packets sent).
pub fn query_local_ip() -> String {
    (|| -> Option<String> {
        let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        s.connect("8.8.8.8:80").ok()?;
        Some(s.local_addr().ok()?.ip().to_string())
    })()
    .unwrap_or_else(|| "unknown".to_owned())
}

pub fn query_hostname() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_owned())
}
