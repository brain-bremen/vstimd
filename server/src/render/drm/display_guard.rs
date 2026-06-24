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

                log::debug!(
                    "vstimd: [{path}] saved CRTC {crtc_h:?} {:?} fb={:?}",
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

            // Keep this card even if no CRTCs are active (e.g. early boot
            // without a display manager).  Opening the card and releasing DRM
            // master here is what triggers nvidia-modeset to probe connected
            // displays; without this, vkGetPhysicalDeviceDisplayPropertiesKHR
            // returns ERROR_UNKNOWN on bare-metal boot.
            if saved.is_empty() {
                log::info!(
                    "vstimd: display controller at {path} \
                     (no active CRTCs — nothing to save/restore)"
                );
            } else {
                log::info!(
                    "vstimd: display controller at {path} \
                     ({} active CRTC(s) saved)",
                    saved.len()
                );

                // Disable VRR (Variable Refresh Rate / G-Sync) on all active
                // CRTCs and connectors before handing the display to Vulkan.
                //
                // When nvidia-modeset sees a VRR-capable display it tries to
                // set up a "VRR Rgline active session" during VK_KHR_display
                // surface creation. On JetPack 6.x this allocation fails
                // ("nvRmApiAlloc(memory) failed for vrr 0x22"), which corrupts
                // the GPU presentation semaphore pathway and causes a PBDMA
                // semaphore acquire timeout ~4 s later → ERROR_DEVICE_LOST.
                // Explicitly setting VRR_ENABLED = 0 on the CRTC and
                // "Adaptive Sync" / "vrr_capable" to 0 on the connector
                // suppresses the attempt.
                for out in &saved {
                    disable_vrr_on_crtc(&card, out.crtc_handle);
                    disable_vrr_on_connector(&card, out.connector_handle);
                }
            }

            // Release master so Vulkan (VK_KHR_display) can take it.
            if let Err(e) = card.release_master_lock() {
                log::warn!("vstimd: release_master_lock on {path}: {e} (continuing)");
            }

            return Some(Self { card, saved });
        }

        log::warn!(
            "vstimd: no display controller found — \
             CRTC restore on exit will be skipped"
        );
        None
    }
}

// ── VRR suppression helpers ───────────────────────────────────────────────────

/// Try to set VRR_ENABLED = 0 on a CRTC.
fn disable_vrr_on_crtc(card: &Card, crtc: drm::control::crtc::Handle) {
    let props = match card.get_properties(crtc) {
        Ok(p) => p,
        Err(_) => return,
    };
    for (&prop_handle, _) in &props {
        let info = match card.get_property(prop_handle) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let name = info.name().to_string_lossy();
        if name.eq_ignore_ascii_case("VRR_ENABLED") || name.eq_ignore_ascii_case("vrr_enabled") {
            match card.set_property(crtc, prop_handle, 0) {
                Ok(()) => log::debug!("vstimd: disabled VRR_ENABLED on CRTC {crtc:?}"),
                Err(e) => log::debug!("vstimd: set VRR_ENABLED=0 on {crtc:?}: {e}"),
            }
            return;
        }
    }
}

/// Try to clear VRR / Adaptive Sync properties on a connector.
fn disable_vrr_on_connector(card: &Card, conn: drm::control::connector::Handle) {
    let props = match card.get_properties(conn) {
        Ok(p) => p,
        Err(_) => return,
    };
    for (&prop_handle, _) in &props {
        let info = match card.get_property(prop_handle) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let name = info.name().to_string_lossy();
        // Names seen in the wild: "Adaptive Sync", "VRR_CAPABLE", "vrr_capable"
        if name.eq_ignore_ascii_case("Adaptive Sync")
            || name.eq_ignore_ascii_case("VRR_CAPABLE")
            || name.eq_ignore_ascii_case("vrr_capable")
        {
            match card.set_property(conn, prop_handle, 0) {
                Ok(()) => log::debug!(
                    "vstimd: set {name}=0 on connector {conn:?}"
                ),
                Err(e) => log::debug!(
                    "vstimd: set {name}=0 on {conn:?}: {e}"
                ),
            }
        }
    }
}

impl Drop for DisplayGuard {
    fn drop(&mut self) {
        if self.saved.is_empty() {
            return;
        }

        // Re-acquire master so we can reprogram the CRTCs.
        if let Err(e) = self.card.acquire_master_lock() {
            log::warn!("vstimd: acquire_master_lock: {e} (attempting set_crtc anyway)");
        }

        for out in &self.saved {
            match self.card.set_crtc(
                out.crtc_handle,
                out.framebuffer,
                (0, 0),
                &[out.connector_handle],
                Some(out.mode),
            ) {
                Ok(()) => log::info!(
                    "vstimd: CRTC {:?} restored → fb {:?}",
                    out.crtc_handle, out.framebuffer
                ),
                Err(e) => log::error!("vstimd: set_crtc({:?}) failed: {e}", out.crtc_handle),
            }
        }

        if let Err(e) = self.card.release_master_lock() {
            log::warn!("vstimd: release_master_lock: {e}");
        }
    }
}
