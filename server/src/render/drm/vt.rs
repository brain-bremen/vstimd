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
}

// ioctl codes from <linux/kd.h> and <linux/vt.h>
const KDSETMODE: libc::c_ulong = 0x4B3A;
const KD_TEXT: libc::c_int = 0x00;
const KD_GRAPHICS: libc::c_int = 0x01;
const VT_ACTIVATE: libc::c_ulong = 0x5606;
const VT_WAITACTIVE: libc::c_ulong = 0x5607;

impl VtGuard {
    pub fn acquire() -> Self {
        let target_vt = vt_number_from_env();

        // Read the currently active VT so we can restore it on exit.
        let prev_vt = active_vt().unwrap_or(1);

        let path = format!("/dev/tty{target_vt}\0");
        let fd = unsafe {
            libc::open(
                path.as_ptr() as *const libc::c_char,
                libc::O_RDWR | libc::O_CLOEXEC,
            )
        };
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

        log::info!("vstimd: activated tty{target_vt} (KD_GRAPHICS); was tty{prev_vt}");
        Self { fd, prev_vt }
    }
}

impl Drop for VtGuard {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.fd, KDSETMODE, KD_TEXT);
            libc::ioctl(self.fd, VT_ACTIVATE, self.prev_vt as libc::c_int);
            libc::close(self.fd);
        }
        log::info!("vstimd: VT restored to KD_TEXT; switching back to tty{}", self.prev_vt);
    }
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
