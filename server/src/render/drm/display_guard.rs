//! DRM display-state save and restore.
//!
//! On `acquire()` the display controller DRM node is opened, and every active
//! CRTC's mode + framebuffer is saved.  DRM master is then released so that
//! Vulkan (`VK_KHR_display`) can acquire it.
//!
//! On `drop()` master is re-acquired and `set_crtc` is called for every saved
//! output, restoring the framebuffer console so the monitor gets a signal
//! after the Vulkan swapchain is torn down.

use drm::Device as DrmDevice;
use drm::control::Device as CtrlDevice;
use drm::control::connector;
use std::fs::{File, OpenOptions};
use std::os::fd::{AsFd, BorrowedFd};

// ── DRM card wrapper ──────────────────────────────────────────────────────────

struct Card(File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl DrmDevice for Card {}
impl CtrlDevice for Card {}

// ── Saved CRTC state for one active output ────────────────────────────────────

struct SavedOutput {
    crtc_handle: drm::control::crtc::Handle,
    connector_handle: drm::control::connector::Handle,
    mode: drm::control::Mode,
    framebuffer: Option<drm::control::framebuffer::Handle>,
}

// ── DisplayGuard ──────────────────────────────────────────────────────────────

/// Opens the display controller DRM node, snapshots active CRTC state, and
/// restores it on drop so the framebuffer console can reclaim the display.
pub struct DisplayGuard {
    card: Card,
    saved: Vec<SavedOutput>,
}

impl DisplayGuard {
    /// Find the display controller and snapshot current CRTC state.
    ///
    /// Walks `/dev/dri/card0..7`, picks the first card that has connected
    /// connectors, and records the current CRTC mode + framebuffer for every
    /// active output.  Then releases DRM master so the Vulkan driver can
    /// acquire it via `VK_KHR_display`.
    pub fn acquire() -> Option<Self> {
        for n in 0..8u32 {
            let path = format!("/dev/dri/card{n}");
            let file = match OpenOptions::new().read(true).write(true).open(&path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let card = Card(file);

            let res = match card.resource_handles() {
                Ok(r) => r,
                Err(_) => continue,
            };
            if res.connectors().is_empty() || res.crtcs().is_empty() {
                continue;
            }

            let mut saved: Vec<SavedOutput> = Vec::new();

            for &conn_h in res.connectors() {
                let conn = match card.get_connector(conn_h, false) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                if conn.state() != connector::State::Connected {
                    continue;
                }

                let enc_h = match conn.current_encoder() {
                    Some(h) => h,
                    None => continue,
                };

                let enc = match card.get_encoder(enc_h) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                let crtc_h = match enc.crtc() {
                    Some(h) => h,
                    None => continue,
                };

                let crtc_info = match card.get_crtc(crtc_h) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let mode = match crtc_info.mode() {
                    Some(m) => m,
                    None => continue,
                };

                eprintln!(
                    "wonderlamp: [{path}] saved CRTC {crtc_h:?} {:?} fb={:?}",
                    mode,
                    crtc_info.framebuffer()
                );

                saved.push(SavedOutput {
                    crtc_handle: crtc_h,
                    connector_handle: conn_h,
                    mode,
                    framebuffer: crtc_info.framebuffer(),
                });
            }

            if saved.is_empty() {
                continue;
            }

            eprintln!(
                "wonderlamp: display controller at {path} \
                 ({} active CRTC(s) saved)",
                saved.len()
            );

            // Release master so Vulkan (VK_KHR_display) can take it.
            if let Err(e) = card.release_master_lock() {
                eprintln!("wonderlamp: release_master_lock on {path}: {e} (continuing)");
            }

            return Some(Self { card, saved });
        }

        eprintln!(
            "wonderlamp: no display controller found — \
             CRTC restore on exit will be skipped"
        );
        None
    }
}

impl Drop for DisplayGuard {
    fn drop(&mut self) {
        // Re-acquire master so we can reprogram the CRTCs.
        if let Err(e) = self.card.acquire_master_lock() {
            eprintln!("wonderlamp: acquire_master_lock: {e} (attempting set_crtc anyway)");
        }

        for out in &self.saved {
            match self.card.set_crtc(
                out.crtc_handle,
                out.framebuffer,
                (0, 0),
                &[out.connector_handle],
                Some(out.mode),
            ) {
                Ok(()) => eprintln!(
                    "wonderlamp: CRTC {:?} restored → fb {:?}",
                    out.crtc_handle, out.framebuffer
                ),
                Err(e) => eprintln!("wonderlamp: set_crtc({:?}) failed: {e}", out.crtc_handle),
            }
        }

        if let Err(e) = self.card.release_master_lock() {
            eprintln!("wonderlamp: release_master_lock: {e}");
        }
    }
}
