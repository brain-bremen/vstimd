//! DRM display-state save and restore.
//!
//! Mirrors SDL's `KMSDRM_DestroySurfaces` approach:
//!
//! **On `acquire()`**
//!   1. Iterate `/dev/dri/card0..7` with the `drm` crate.
//!   2. Pick the first card that has connected connectors (the display
//!      controller — on Tegra Jetson this is `card1`, on desktop NVIDIA
//!      it is `card0`).
//!   3. Walk connectors → encoders → CRTCs, saving every active CRTC's
//!      original mode, framebuffer handle, and connector.
//!   4. Call `release_master_lock()` so the Vulkan driver can acquire DRM
//!      master when it creates its `VK_KHR_display` surface.
//!
//! **On `restore_with_file(drm_fd)`** — the primary restore path:
//!   Called from `explicit_cleanup()` in `mod.rs` **before** `VkContext`
//!   drops and before `vkReleaseDisplayEXT` fires.  At that moment the
//!   `drm_fd` passed to `vkAcquireDrmDisplayEXT` still holds DRM master
//!   (NVIDIA keeps master on the caller's fd for the lifetime of the
//!   acquired display).  Calling `set_crtc()` on that fd succeeds because
//!   master is held.
//!
//!   This mirrors SDL's `KMSDRM_DestroySurfaces`: SDL holds master on its
//!   own fd the whole time and calls `drmModeSetCrtc` before closing the fd.
//!
//! **On `drop()`** — fallback path:
//!   If `restore_with_file` was not called or failed, `drop()` attempts
//!   `acquire_master_lock()` + `set_crtc()`.  On a logind desktop this races
//!   with the compositor (which grabs master the instant NVIDIA releases it)
//!   and will log EBUSY / EPERM; those errors are harmless if the primary
//!   restore path already succeeded.

use drm::Device as DrmDevice;
use drm::control::Device as CtrlDevice;
use drm::control::connector;
use std::cell::Cell;
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
/// restores it on exit so the framebuffer console can reclaim the display.
///
/// **Why we always restore, even with a compositor running on another VT:**
/// `vkAcquireDrmDisplayEXT` grants Vulkan exclusive ownership of the display
/// hardware, bypassing the compositor entirely.  When `vkReleaseDisplayEXT`
/// fires, the spec does *not* guarantee the driver restores the CRTC — in
/// practice NVIDIA leaves it pointing at the (now-destroyed) Vulkan swapchain
/// image, causing a "no signal" condition.  The compositor on a different VT
/// never receives an event telling it to re-drive this CRTC, so we must do
/// the restore ourselves before we lose DRM master.
///
/// The restore is best-effort: if `acquire_master_lock` fails (e.g. another
/// process grabbed it first) we log and continue — the caller should have
/// already ensured it is the only holder by the time `drop` fires.
pub struct DisplayGuard {
    card: Card,
    saved: Vec<SavedOutput>,
    /// Set to `true` by `restore_with_file` after at least one successful
    /// `set_crtc`.  `drop()` checks this to suppress the noisy fallback
    /// error messages when the primary path already restored the display.
    restored: Cell<bool>,
}

impl DisplayGuard {
    /// Find the display controller and snapshot current CRTC state.
    ///
    /// Walks `/dev/dri/card0..7`, picks the first card that has connected
    /// connectors, and records the current CRTC mode + framebuffer for every
    /// active output.  Then calls `release_master_lock()` so the Vulkan driver
    /// can acquire DRM master via `vkAcquireDrmDisplayEXT`.
    pub fn acquire() -> Option<Self> {
        for n in 0..8u32 {
            let path = format!("/dev/dri/card{n}");
            let file = match OpenOptions::new().read(true).write(true).open(&path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let card = Card(file);

            // Cards without modesetting resources (e.g. pure render nodes or
            // nvgpu on Tegra) will fail resource_handles().
            let res = match card.resource_handles() {
                Ok(r) => r,
                Err(_) => continue,
            };
            if res.connectors().is_empty() || res.crtcs().is_empty() {
                continue;
            }

            // Walk all connectors and record the state of every active CRTC.
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

                // A CRTC with no mode is inactive — skip it.
                let mode = match crtc_info.mode() {
                    Some(m) => m,
                    None => continue,
                };

                eprintln!(
                    "wonderlamp: [{path}] saved CRTC {crtc_h:?} \
                     {:?} fb={:?}",
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

            // Drop DRM master so Vulkan (VK_KHR_display) can acquire it.
            // This mirrors SDL's KMSDRM_DropMaster() call before Vulkan init.
            if let Err(e) = card.release_master_lock() {
                eprintln!("wonderlamp: release_master_lock on {path}: {e} (continuing)");
            }

            return Some(Self {
                card,
                saved,
                restored: Cell::new(false),
            });
        }

        eprintln!(
            "wonderlamp: no display controller found — \
             CRTC restore on exit will be skipped"
        );
        None
    }

    /// **Primary restore path** — call this from `explicit_cleanup()` while
    /// Vulkan is still running and `drm_fd` still holds DRM master.
    ///
    /// `drm_fd` is the `std::fs::File` that was passed to
    /// `vkAcquireDrmDisplayEXT`.  Per the spec that fd must have had DRM
    /// master permissions when the call was made; NVIDIA keeps master on it
    /// for the lifetime of the acquired display.  Calling `set_crtc()` on it
    /// before `vkReleaseDisplayEXT` fires wins the race against the compositor
    /// that otherwise grabs master the instant NVIDIA releases it.
    ///
    /// Sets `self.restored = true` if at least one CRTC was successfully
    /// restored so the fallback `drop()` path stays quiet.
    pub fn restore_with_file(&self, drm_fd: &File) {
        // Temporary Card that borrows the fd without taking ownership.
        struct BorrowedCard<'a>(&'a File);
        impl AsFd for BorrowedCard<'_> {
            fn as_fd(&self) -> BorrowedFd<'_> {
                self.0.as_fd()
            }
        }
        impl DrmDevice for BorrowedCard<'_> {}
        impl CtrlDevice for BorrowedCard<'_> {}

        let card = BorrowedCard(drm_fd);
        let mut any_ok = false;

        for out in &self.saved {
            match card.set_crtc(
                out.crtc_handle,
                out.framebuffer,
                (0, 0),
                &[out.connector_handle],
                Some(out.mode),
            ) {
                Ok(()) => {
                    eprintln!(
                        "wonderlamp: CRTC {:?} pre-release restore OK → fb {:?}",
                        out.crtc_handle, out.framebuffer
                    );
                    any_ok = true;
                }
                Err(e) => eprintln!(
                    "wonderlamp: CRTC {:?} pre-release restore failed: {e}",
                    out.crtc_handle
                ),
            }
        }

        if any_ok {
            self.restored.set(true);
        }
    }

    /// **Fallback restore path** — called from `drop()`.
    ///
    /// Re-acquires DRM master (which often fails on logind desktops because the
    /// compositor beats us to it) and attempts `set_crtc`.  If
    /// `restore_with_file` already succeeded this is a no-op.
    pub fn restore(&self) {
        if self.restored.get() {
            eprintln!("wonderlamp: DisplayGuard: already restored — skipping fallback");
            return;
        }

        // Vulkan released DRM master when its display/device was torn down.
        // Re-acquire so we can reprogram the CRTC.
        if let Err(e) = self.card.acquire_master_lock() {
            // Another process (e.g. compositor resuming on the same VT) may
            // have beaten us to master.  Log it but still attempt set_crtc —
            // on some drivers the call succeeds without explicit master if the
            // fd originally acquired the display.
            eprintln!(
                "wonderlamp: DisplayGuard: acquire_master_lock: {e} (attempting set_crtc anyway)"
            );
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

impl Drop for DisplayGuard {
    fn drop(&mut self) {
        self.restore();
    }
}
