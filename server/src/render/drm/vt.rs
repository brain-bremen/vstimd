use std::sync::atomic::{AtomicBool, Ordering};

/// Guard that activates a specific virtual terminal, switches it to
/// `KD_GRAPHICS` mode, and restores everything on drop.
///
/// Opening the VT device directly (rather than `/dev/tty`) lets vstimd take
/// over a chosen VT regardless of which terminal the process was started from.
/// Default is tty3; override with `VSTIMD_TTY=<n>`.
///
/// On drop the VT is restored to `KD_TEXT` and the previously active VT is
/// reactivated.
pub struct VtGuard {
    fd: libc::c_int,
    prev_vt: u16,
    /// The fd that Vulkan's VK_KHR_display holds DRM master on.
    /// Discovered after Vulkan init via find_drm_master_fd().
    /// Dropped on VT release, re-acquired on VT acquire.
    drm_master_fd: Option<libc::c_int>,
}

// ioctl codes from <linux/kd.h> and <linux/vt.h>
const KDSETMODE: libc::c_ulong = 0x4B3A;
const KD_TEXT: libc::c_int = 0x00;
const KD_GRAPHICS: libc::c_int = 0x01;
const VT_ACTIVATE: libc::c_ulong = 0x5606;
const VT_WAITACTIVE: libc::c_ulong = 0x5607;
const VT_SETMODE: libc::c_ulong = 0x5602;
const VT_RELDISP: libc::c_ulong = 0x5605;
const VT_PROCESS: u8 = 1;
const VT_AUTO: u8 = 0;
const VT_ACKACQ: libc::c_int = 2;

// DRM master management — _IO('d', 0x1e/0x1f)
const DRM_IOCTL_SET_MASTER: libc::c_ulong = 0x641e;
const DRM_IOCTL_DROP_MASTER: libc::c_ulong = 0x641f;

#[repr(C)]
struct VtMode {
    mode: u8,
    waitv: u8,
    relsig: libc::c_short,
    acqsig: libc::c_short,
    frsig: libc::c_short,
}

// Set by signal handlers; checked and cleared each frame by the render loop.
static VT_RELEASE_REQUESTED: AtomicBool = AtomicBool::new(false);
static VT_ACQUIRE_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_sigusr1(_: libc::c_int) {
    VT_RELEASE_REQUESTED.store(true, Ordering::Relaxed);
}
extern "C" fn handle_sigusr2(_: libc::c_int) {
    VT_ACQUIRE_REQUESTED.store(true, Ordering::Relaxed);
}

impl VtGuard {
    pub fn acquire() -> Self {
        let target_vt = vt_number_from_env();

        // Read the currently active VT so we can restore it on exit.
        let prev_vt = active_vt().unwrap_or(1);

        let fd = open_vt(target_vt);
        if fd < 0 {
            log::error!(
                "vstimd: cannot open /dev/tty{target_vt}: {}",
                std::io::Error::last_os_error()
            );
            std::process::exit(1);
        }

        // Switch to the target VT.
        if unsafe { libc::ioctl(fd, VT_ACTIVATE, target_vt as libc::c_int) } < 0 {
            log::error!(
                "vstimd: VT_ACTIVATE tty{target_vt} failed: {}",
                std::io::Error::last_os_error()
            );
            unsafe { libc::close(fd) };
            std::process::exit(1);
        }
        if unsafe { libc::ioctl(fd, VT_WAITACTIVE, target_vt as libc::c_int) } < 0 {
            log::warn!(
                "vstimd: VT_WAITACTIVE tty{target_vt}: {}",
                std::io::Error::last_os_error()
            );
        }

        // Suppress kernel text/cursor output on this VT.
        if unsafe { libc::ioctl(fd, KDSETMODE, KD_GRAPHICS) } < 0 {
            log::error!(
                "vstimd: KDSETMODE KD_GRAPHICS on tty{target_vt} failed: {}",
                std::io::Error::last_os_error()
            );
            unsafe { libc::close(fd) };
            std::process::exit(1);
        }

        // Enable VT_PROCESS mode: the kernel asks us before switching away
        // (SIGUSR1) and notifies us when we're active again (SIGUSR2).  We
        // use this to release/re-acquire the libinput EVIOCGRAB so the other
        // VT's session can receive input while vstimd is in the background.
        unsafe {
            libc::signal(libc::SIGUSR1, handle_sigusr1 as *const () as libc::sighandler_t);
            libc::signal(libc::SIGUSR2, handle_sigusr2 as *const () as libc::sighandler_t);
            let mode = VtMode {
                mode: VT_PROCESS,
                waitv: 0,
                relsig: libc::SIGUSR1 as libc::c_short,
                acqsig: libc::SIGUSR2 as libc::c_short,
                frsig: 0,
            };
            if libc::ioctl(fd, VT_SETMODE, &mode) < 0 {
                log::warn!(
                    "vstimd: VT_SETMODE VT_PROCESS failed: {} — Ctrl+Alt+Fn will not release input grab",
                    std::io::Error::last_os_error()
                );
            }
        }

        log::info!("vstimd: activated tty{target_vt} (KD_GRAPHICS); was tty{prev_vt}");
        Self { fd, prev_vt, drm_master_fd: None }
    }

    /// Switch the active VT to `vt` without exiting.
    ///
    /// Used to forward Ctrl+Alt+Fn because libinput holds an exclusive
    /// EVIOCGRAB and the kernel never sees those key combos on its own.
    ///
    /// Only VT_ACTIVATE is issued here — no VT_WAITACTIVE.  Since our VT is in
    /// VT_PROCESS mode, VT_ACTIVATE causes the kernel to send SIGUSR1 asking us
    /// to release.  The main loop checks release_requested() next iteration,
    /// calls allow_release(), and the switch then completes.  Blocking on
    /// VT_WAITACTIVE here would deadlock: the switch can't complete until
    /// allow_release() is called, which can't happen while we're blocked.
    pub fn switch_to(&self, vt: u16) {
        unsafe {
            libc::ioctl(self.fd, VT_ACTIVATE, vt as libc::c_int);
        }
        log::info!("vstimd: requesting switch to tty{vt}");
    }

    /// Returns true (and clears the flag) if the kernel has requested a VT
    /// switch away.  Caller must suspend input and call `allow_release`.
    pub fn release_requested(&self) -> bool {
        VT_RELEASE_REQUESTED.swap(false, Ordering::Relaxed)
    }

    /// Returns true (and clears the flag) if our VT has become active again.
    /// Caller must call `confirm_acquire` and resume input.
    pub fn acquire_requested(&self) -> bool {
        VT_ACQUIRE_REQUESTED.swap(false, Ordering::Relaxed)
    }

    /// Acknowledge the kernel's VT-release request so the switch proceeds.
    ///
    /// Drops DRM master so the kernel VT console can drive the display.
    /// On the first call, scans /proc/self/fd to find Vulkan's DRM master fd;
    /// we do this lazily (not at startup) so we never temporarily drop master
    /// while Vulkan is initialising or rendering normally.
    pub fn allow_release(&mut self) {
        // Lazy discovery: find the fd holding DRM master and drop it.
        // DROP_MASTER succeeds only on the fd that currently holds master,
        // so whichever /dev/dri/card* fd succeeds is Vulkan's fd.
        if self.drm_master_fd.is_none() {
            if let Ok(dir) = std::fs::read_dir("/proc/self/fd") {
                for entry in dir.flatten() {
                    let fd: libc::c_int = match entry.file_name().to_str().and_then(|s| s.parse().ok()) {
                        Some(n) => n,
                        None => continue,
                    };
                    let link = match std::fs::read_link(entry.path()) {
                        Ok(l) => l,
                        Err(_) => continue,
                    };
                    if !link.to_str().map(|s| s.contains("/dri/card")).unwrap_or(false) {
                        continue;
                    }
                    let ret = unsafe { libc::ioctl(fd, DRM_IOCTL_DROP_MASTER, 0) };
                    if ret == 0 {
                        log::info!("vstimd: DRM master dropped (fd={fd}, {})", link.display());
                        self.drm_master_fd = Some(fd);
                        // Skip the second DROP below — already done.
                        unsafe { libc::ioctl(self.fd, VT_RELDISP, 1 as libc::c_int); }
                        return;
                    }
                }
                log::warn!("vstimd: DRM master fd not found — display may not switch");
            }
        } else if let Some(fd) = self.drm_master_fd {
            let ret = unsafe { libc::ioctl(fd, DRM_IOCTL_DROP_MASTER, 0) };
            if ret < 0 {
                log::warn!("vstimd: DRM DROP_MASTER failed: {}", std::io::Error::last_os_error());
            } else {
                log::info!("vstimd: DRM master dropped");
            }
        }
        unsafe { libc::ioctl(self.fd, VT_RELDISP, 1 as libc::c_int); }
    }

    /// Acknowledge that we have re-acquired the VT.
    ///
    /// Re-acquires DRM master so Vulkan can resume presenting frames.
    pub fn confirm_acquire(&self) {
        unsafe { libc::ioctl(self.fd, VT_RELDISP, VT_ACKACQ); }
        if let Some(fd) = self.drm_master_fd {
            let ret = unsafe { libc::ioctl(fd, DRM_IOCTL_SET_MASTER, 0) };
            if ret < 0 {
                log::warn!("vstimd: DRM SET_MASTER failed: {}", std::io::Error::last_os_error());
            } else {
                log::info!("vstimd: DRM master re-acquired");
            }
        }
    }
}

impl Drop for VtGuard {
    fn drop(&mut self) {
        unsafe {
            // Restore VT_AUTO so the kernel handles subsequent switches itself.
            let mode = VtMode { mode: VT_AUTO, waitv: 0, relsig: 0, acqsig: 0, frsig: 0 };
            libc::ioctl(self.fd, VT_SETMODE, &mode);
            libc::ioctl(self.fd, KDSETMODE, KD_TEXT);
            libc::ioctl(self.fd, VT_ACTIVATE, self.prev_vt as libc::c_int);
            libc::close(self.fd);
        }
        log::info!("vstimd: VT restored to KD_TEXT; switching back to tty{}", self.prev_vt);
    }
}

/// Open the TTY device for `target_vt`.
///
/// When systemd has already opened the device via `TTYPath=` + `StandardInput=tty`,
/// stdin (fd 0) *is* `/dev/tty{target_vt}`. Dup-ing it avoids needing the
/// vstimd user to have direct open permission on the device node (which is
/// `crw-------` / root-only when no login session owns it).
fn open_vt(target_vt: u16) -> libc::c_int {
    let expected = format!("/dev/tty{target_vt}");
    if ttyname_of(0).as_deref() == Some(&expected) {
        let fd = unsafe { libc::fcntl(0, libc::F_DUPFD_CLOEXEC, 0) };
        if fd >= 0 {
            return fd;
        }
    }
    // Fall back to a direct open (works when run with sufficient permissions,
    // e.g. during development or with a udev rule granting group access).
    let path = format!("{expected}\0");
    unsafe {
        libc::open(
            path.as_ptr() as *const libc::c_char,
            libc::O_WRONLY | libc::O_CLOEXEC,
        )
    }
}

/// Return the path of the TTY attached to `fd`, or `None`.
fn ttyname_of(fd: libc::c_int) -> Option<String> {
    let mut buf = [0u8; 64];
    let ret = unsafe {
        libc::ttyname_r(fd, buf.as_mut_ptr() as *mut libc::c_char, buf.len())
    };
    if ret != 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..end]).into_owned())
}

/// VT number from `VSTIMD_TTY=<n>`, defaulting to 3.
fn vt_number_from_env() -> u16 {
    match std::env::var("VSTIMD_TTY") {
        Ok(s) => match s.trim().parse::<u16>() {
            Ok(n) if n >= 1 => n,
            _ => {
                log::warn!("vstimd: VSTIMD_TTY={s:?} is not a valid VT number, using 3");
                3
            }
        },
        Err(_) => 3,
    }
}

/// Read the currently active VT number from `/sys/class/tty/tty0/active`
/// (returns e.g. `"tty1"`).  Falls back to `None` if the file cannot be read.
fn active_vt() -> Option<u16> {
    let s = std::fs::read_to_string("/sys/class/tty/tty0/active").ok()?;
    s.trim().strip_prefix("tty")?.parse().ok()
}
