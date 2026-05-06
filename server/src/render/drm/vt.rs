//! Virtual Terminal management for the DRM backend.
//!
//! Follows SDL's `SDL_evdev_kbd.c` pattern exactly:
//!
//! 1. Open `/dev/tty` — the **current** VT; no new VT is allocated.
//! 2. Probe `KDGKBTYPE` to confirm we are on a real Linux console.
//! 3. Save the keyboard mode with `KDGKBMODE`.
//! 4. Mute console keystrokes with `KDSKBMODE(K_OFF)`.
//! 5. Find two free real-time signals (fallback: SIGUSR1 / SIGUSR2).
//! 6. Set `VT_PROCESS` mode so the kernel sends those signals before
//!    switching away from our VT, giving us a chance to drop/reacquire
//!    DRM master and recreate Vulkan surfaces.
//! 7. On drop: restore keyboard mode, set `VT_AUTO` so the kernel
//!    resumes automatic VT switching and fbcon can reclaim the display.

use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicI32, Ordering};

// ── VT / KD ioctl constants (linux/vt.h, linux/kd.h) ────────────────────────

const VT_SETMODE: libc::c_ulong = 0x5602;
const VT_RELDISP: libc::c_ulong = 0x5605;
const KDGKBTYPE: libc::c_ulong = 0x4B33;
const KDGKBMODE: libc::c_ulong = 0x4B44;
const KDSKBMODE: libc::c_ulong = 0x4B45;

/// 84-key AT keyboard — unused after relaxing VT check, kept for doc value.
#[allow(dead_code)]
const KB_84: u8 = 0x01;
/// 101-key AT keyboard — what the Linux kernel writes into KDGKBTYPE.
#[allow(dead_code)]
const KB_101: u8 = 0x02;

/// Kernel-controlled VT switching (the normal default).
const VT_AUTO: i8 = 0x00;
/// Process-controlled VT switching (we handle signals).
const VT_PROCESS: i8 = 0x01;
/// Argument to VT_RELDISP to acknowledge a VT acquisition.
const VT_ACKACQ: libc::c_long = 2;
/// Disable keyboard → console translation entirely.
const K_OFF: libc::c_long = 4;

#[repr(C)]
#[derive(Default)]
struct VtMode {
    mode: i8,
    waitv: i8,
    relsig: libc::c_short,
    acqsig: libc::c_short,
    frsig: libc::c_short,
}

// ── Global signal state (signal handlers can only write atomics) ─────────────

const SIG_NONE: i32 = 0;
const SIG_RELEASE: i32 = 1;
const SIG_ACQUIRE: i32 = 2;

static VT_SIGNAL_PENDING: AtomicI32 = AtomicI32::new(SIG_NONE);

extern "C" fn on_vt_release(_: libc::c_int) {
    VT_SIGNAL_PENDING.store(SIG_RELEASE, Ordering::SeqCst);
}

extern "C" fn on_vt_acquire(_: libc::c_int) {
    VT_SIGNAL_PENDING.store(SIG_ACQUIRE, Ordering::SeqCst);
}

// ── Signal helpers ────────────────────────────────────────────────────────────

/// Try to install `handler` on `signum`.
/// Returns `true` only if the signal had the default handler (SIG_DFL) and
/// installation succeeded — i.e. the signal was genuinely free to use.
fn try_install_signal(signum: i32, handler: extern "C" fn(libc::c_int)) -> bool {
    let mut old: libc::sigaction = unsafe { std::mem::zeroed() };
    if unsafe { libc::sigaction(signum, std::ptr::null(), &mut old) } < 0 {
        return false;
    }
    // Leave the signal alone if something else has already claimed it.
    if old.sa_sigaction != libc::SIG_DFL as libc::sighandler_t {
        return false;
    }
    let mut new: libc::sigaction = unsafe { std::mem::zeroed() };
    new.sa_sigaction = handler as libc::sighandler_t;
    new.sa_flags = libc::SA_RESTART;
    (unsafe { libc::sigaction(signum, &new, std::ptr::null_mut()) }) >= 0
}

/// Find an unused signal for the given handler.
/// Tries SIGRTMIN+2 .. SIGRTMAX first (SDL's preferred range), then
/// SIGUSR1 / SIGUSR2 as a fallback.  Returns 0 if nothing is available.
fn find_free_signal(handler: extern "C" fn(libc::c_int)) -> i32 {
    let rtmin = libc::SIGRTMIN();
    let rtmax = libc::SIGRTMAX();
    for sig in (rtmin + 2)..=rtmax {
        if try_install_signal(sig, handler) {
            return sig;
        }
    }
    if try_install_signal(libc::SIGUSR1, handler) {
        return libc::SIGUSR1;
    }
    if try_install_signal(libc::SIGUSR2, handler) {
        return libc::SIGUSR2;
    }
    0
}

// ── Console TTY open helper ──────────────────────────────────────────────────

/// Try to open a file descriptor that is definitely on a real Linux virtual
/// console, returning the `File` and a human-readable path string for logs.
///
/// Strategy:
/// 1. Open `/dev/tty` (the controlling terminal of the process).  If
///    `KDGKBTYPE` later succeeds on this fd, we are already on a real VT.
/// 2. If `/dev/tty` is a pty (SSH, tmux, terminal emulator), try to discover
///    the active VT number from `/sys/class/tty/tty0/active` and open that
///    device directly (e.g. `/dev/tty3`).
///
/// Returns `None` only if no console fd can be obtained at all.
fn open_console_tty() -> Option<(File, String)> {
    // Path 1: controlling terminal.
    if let Ok(f) = OpenOptions::new().read(true).write(true).open("/dev/tty") {
        // Quick-probe: try KDGKBTYPE; if it works we're done.
        let mut kbtype: libc::c_char = 0;
        if unsafe { libc::ioctl(f.as_raw_fd(), KDGKBTYPE, &mut kbtype as *mut _) } == 0 {
            return Some((f, "/dev/tty".into()));
        }
        // /dev/tty exists but is a pty — fall through to sysfs lookup.
    }

    // Path 2: active VT from sysfs (e.g. "tty3" → /dev/tty3).
    // /sys/class/tty/tty0/active contains the name of the foreground VT.
    if let Ok(active) = std::fs::read_to_string("/sys/class/tty/tty0/active") {
        let name = active.trim(); // e.g. "tty3"
        let path = format!("/dev/{name}");
        if let Ok(f) = OpenOptions::new().read(true).write(true).open(&path) {
            eprintln!("wonderlamp: /dev/tty is not a real VT — trying active VT {path}");
            return Some((f, path));
        }
    }

    None
}

// ── VtGuard ───────────────────────────────────────────────────────────────────

/// RAII guard that takes ownership of the current VT for graphics use.
///
/// On creation it:
///   - opens `/dev/tty` and validates this is a real Linux console,
///   - saves and mutes the keyboard mode,
///   - installs `VT_PROCESS` mode with signal handlers.
///
/// On drop it:
///   - restores the saved keyboard mode,
///   - sets `VT_AUTO` so the kernel and fbcon can reclaim the display.
pub struct VtGuard {
    tty: File,
    old_kbd_mode: i32,
    // Stored so we can restore the original signal handlers in a future
    // full VT-switch implementation (SDL cleans these up in kbd_vt_quit).
    #[allow(dead_code)]
    release_sig: i32,
    #[allow(dead_code)]
    acquire_sig: i32,
}

impl VtGuard {
    /// Acquire the current VT.
    ///
    /// Returns `None` when not running on a physical Linux console
    /// (e.g. inside X, Wayland, tmux, SSH without a TTY).
    pub fn acquire() -> Option<Self> {
        // Try to open /dev/tty (the controlling terminal of this process).
        // Fall back to the active VT read from sysfs if /dev/tty is a pty.
        let (tty, path_used) = open_console_tty()?;

        let fd = tty.as_raw_fd();

        // KDGKBTYPE succeeds only on a real Linux virtual console (/dev/ttyN).
        // On a pty (xterm, SSH, tmux) or serial tty it fails with ENOTTY.
        // The kernel always writes KB_101 (2) on success; if it returns 0 the
        // ioctl still succeeded on a VT — accept it rather than bailing.
        let mut kbtype: libc::c_char = 0;
        let ioctl_ret = unsafe { libc::ioctl(fd, KDGKBTYPE, &mut kbtype as *mut _) };
        if ioctl_ret < 0 {
            let err = std::io::Error::last_os_error();
            eprintln!(
                "wonderlamp: {path_used}: KDGKBTYPE ioctl failed ({err}) \
                 — VT management skipped"
            );
            return None;
        }
        // kbtype == 0 is unexpected (kernel writes KB_101) but we accept it:
        // the ioctl succeeding at all means we are on a real VT fd.
        eprintln!("wonderlamp: {path_used}: KDGKBTYPE={kbtype} — VT confirmed");

        // Save the current keyboard mode so we can restore it on exit.
        let mut old_kbd_mode: libc::c_int = 0;
        unsafe { libc::ioctl(fd, KDGKBMODE, &mut old_kbd_mode as *mut _) };

        // Mute console keyboard: keystrokes go to libinput only.
        if unsafe { libc::ioctl(fd, KDSKBMODE, K_OFF) } < 0 {
            eprintln!(
                "wonderlamp: KDSKBMODE(K_OFF) failed: {}",
                std::io::Error::last_os_error()
            );
        }

        // Find free signals for VT release and acquire notifications.
        let release_sig = find_free_signal(on_vt_release);
        let acquire_sig = find_free_signal(on_vt_acquire);

        if release_sig != 0 && acquire_sig != 0 {
            let mode = VtMode {
                mode: VT_PROCESS,
                waitv: 0,
                relsig: release_sig as libc::c_short,
                acqsig: acquire_sig as libc::c_short,
                frsig: libc::SIGIO as libc::c_short,
            };
            if unsafe { libc::ioctl(fd, VT_SETMODE, &mode as *const _) } < 0 {
                eprintln!(
                    "wonderlamp: VT_SETMODE(VT_PROCESS) failed: {} (continuing)",
                    std::io::Error::last_os_error()
                );
            }
        } else {
            eprintln!(
                "wonderlamp: could not find free signals for VT switching \
                 — Alt+Fn VT switching will be unresponsive"
            );
        }

        eprintln!(
            "wonderlamp: VT acquired (kbd_mode={old_kbd_mode}, \
             rel_sig={release_sig}, acq_sig={acquire_sig})"
        );

        Some(Self {
            tty,
            old_kbd_mode,
            release_sig,
            acquire_sig,
        })
    }

    /// Poll for a pending VT-switch signal and acknowledge it.
    ///
    /// Call this once per frame from the render loop.
    ///
    /// `on_release` is invoked when the kernel wants to switch away from our VT
    /// (the caller should drop Vulkan surfaces / DRM master here).
    /// `on_acquire` is invoked when the VT is given back.
    pub fn poll(&self, on_release: impl FnOnce(), on_acquire: impl FnOnce()) {
        let pending = VT_SIGNAL_PENDING.load(Ordering::SeqCst);
        if pending == SIG_NONE {
            return;
        }

        let fd = self.tty.as_raw_fd();
        if pending == SIG_RELEASE {
            on_release();
            // Acknowledge: tell the kernel it may proceed with the switch.
            unsafe { libc::ioctl(fd, VT_RELDISP, 1 as libc::c_long) };
        } else {
            on_acquire();
            // Acknowledge: confirm we have re-acquired the VT.
            unsafe { libc::ioctl(fd, VT_RELDISP, VT_ACKACQ) };
        }
        VT_SIGNAL_PENDING
            .compare_exchange(pending, SIG_NONE, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
    }

    fn restore(&self) {
        let fd = self.tty.as_raw_fd();

        // Return VT switching to kernel-automatic mode so fbcon can re-appear.
        let auto_mode = VtMode {
            mode: VT_AUTO,
            ..Default::default()
        };
        unsafe {
            if libc::ioctl(fd, VT_SETMODE, &auto_mode as *const _) < 0 {
                eprintln!(
                    "wonderlamp: VT_SETMODE(VT_AUTO) failed: {}",
                    std::io::Error::last_os_error()
                );
            }
        }

        // Restore the keyboard mode that was active before we muted it.
        unsafe {
            if libc::ioctl(fd, KDSKBMODE, self.old_kbd_mode as libc::c_long) < 0 {
                eprintln!(
                    "wonderlamp: KDSKBMODE({}) failed: {}",
                    self.old_kbd_mode,
                    std::io::Error::last_os_error()
                );
            }
        }

        eprintln!(
            "wonderlamp: VT released (kbd_mode restored to {})",
            self.old_kbd_mode
        );
    }
}

impl Drop for VtGuard {
    fn drop(&mut self) {
        self.restore();
    }
}
