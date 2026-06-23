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

    /// Block until the next display vblank.
    /// Returns `None` on error (caller should disable and fall back).
    pub fn wait(&self) -> Option<Instant> {
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
        let wait_result = unsafe {
            self.device.wait_for_fences(&[fence], true, u64::MAX)
        };
        let t = Instant::now();
        unsafe { self.device.destroy_fence(fence, None) };
        wait_result.ok()?;
        Some(t)
    }
}
