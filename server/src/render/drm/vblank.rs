use std::os::fd::{AsFd, BorrowedFd};
use std::time::Instant;

use drm::Device as DrmDevice;
use drm::control::Device as ControlDevice;

struct Card(std::fs::File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl drm::Device for Card {}
impl ControlDevice for Card {}

pub struct DrmVblank {
    card: Card,
    crtc_pipe: u32,
}

impl DrmVblank {
    /// Iterate /dev/dri/card* and return a handle bound to the first CRTC that
    /// is actively driving a display (mode set). Returns `None` if none found.
    pub fn open() -> Option<Self> {
        for n in 0..8u8 {
            let path = format!("/dev/dri/card{n}");
            let Ok(file) = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
            else {
                continue;
            };
            let card = Card(file);

            // Release master immediately. Opening with O_RDWR automatically
            // grants DRM master when no other fd holds it (which is the case
            // here: DisplayGuard already released master). If we keep master,
            // VK_KHR_display cannot acquire it during swapchain creation.
            // wait_vblank is an unprivileged ioctl — no master required.
            if let Err(err) = DrmDevice::release_master_lock(&card) {
                log::warn!(
                    "vstimd: failed to release DRM master for {path}: {err}"
                );
            }

            let Ok(res) = card.resource_handles() else {
                continue;
            };
            for (pipe, &crtc_handle) in res.crtcs().iter().enumerate() {
                let Ok(crtc) = card.get_crtc(crtc_handle) else {
                    continue;
                };
                if crtc.mode().is_some() {
                    log::info!("vstimd: DRM vblank: {path} crtc[{pipe}]");
                    return Some(Self {
                        card,
                        crtc_pipe: pipe as u32,
                    });
                }
            }
        }
        log::warn!("vstimd: no active DRM CRTC found for vblank — using GPU-completion time");
        None
    }

    /// Block until the next vblank on the selected CRTC.
    /// Returns an `Instant` captured immediately after the kernel unblocks.
    pub fn wait(&self) -> Option<Instant> {
        match DrmDevice::wait_vblank(
            &self.card,
            drm::VblankWaitTarget::Relative(1),
            drm::VblankWaitFlags::empty(),
            self.crtc_pipe,
            0,
        ) {
            Ok(_) => Some(Instant::now()),
            Err(err) => {
                log::warn!(
                    "vstimd: DRM wait_vblank failed on CRTC {}: {err}",
                    self.crtc_pipe
                );
                None
            }
        }
    }
}

/// Vblank clock using `VK_EXT_display_control`.
///
/// `vkRegisterDisplayEventEXT` creates a one-shot fence that fires on the
/// display's first-pixel-out event (≈ vblank).  This is the fallback when
/// the legacy `DRM_IOCTL_WAIT_VBLANK` ioctl is not supported by the driver
/// (e.g. NVIDIA Tegra nvdisplay).
///
/// # Two-phase usage (avoids double-blocking with FIFO acquire)
///
/// With `VK_PRESENT_MODE_FIFO_KHR`, `vkAcquireNextImageKHR` already blocks at
/// the display vblank boundary.  If we also block on `FIRST_PIXEL_OUT` *before*
/// the acquire the loop runs at half the refresh rate.
///
/// The fix: **register** the fence just before render/present; **collect** it at
/// the very top of the *next* iteration before acquire.  The collect blocks for
/// the remaining ≈7 ms until FIRST_PIXEL_OUT fires, then acquire sees a free
/// image and returns immediately.
pub struct VkVblank {
    device: ash::Device,
    loader: ash::ext::display_control::Device,
    display: ash::vk::DisplayKHR,
}

impl VkVblank {
    pub fn new(
        device: ash::Device,
        loader: ash::ext::display_control::Device,
        display: ash::vk::DisplayKHR,
    ) -> Self {
        Self { device, loader, display }
    }

    /// Register a FIRST_PIXEL_OUT event and return the one-shot fence.
    /// Returns `None` on error.
    pub fn register(&self) -> Option<ash::vk::Fence> {
        let event_info = ash::vk::DisplayEventInfoEXT::default()
            .display_event(ash::vk::DisplayEventTypeEXT::FIRST_PIXEL_OUT);
        let mut fence = ash::vk::Fence::null();
        let result = unsafe {
            (self.loader.fp().register_display_event_ext)(
                self.loader.device(),
                self.display,
                &event_info as *const _,
                std::ptr::null(),
                &mut fence,
            )
        };
        if result != ash::vk::Result::SUCCESS {
            log::warn!("vstimd: vkRegisterDisplayEventEXT failed: {result:?}");
            return None;
        }
        Some(fence)
    }

    /// Wait for a previously registered fence and return the timestamp.
    /// Destroys the fence regardless of outcome.
    /// Returns `None` on error (caller should disable and fall back).
    pub fn collect(&self, fence: ash::vk::Fence) -> Option<Instant> {
        let wait_result = unsafe {
            self.device.wait_for_fences(&[fence], true, u64::MAX)
        };
        let t = Instant::now();
        unsafe { self.device.destroy_fence(fence, None) };
        wait_result.ok()?;
        Some(t)
    }
}
