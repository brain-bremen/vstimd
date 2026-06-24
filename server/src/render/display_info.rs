#[derive(Clone, Debug)]
pub struct StimulusDisplayInfo {
    pub width_px: u32,
    pub height_px: u32,
    pub refresh_hz: f64,
    /// Index into the driver's display mode list, if known (DRM backend only).
    pub mode_index: Option<usize>,
}
