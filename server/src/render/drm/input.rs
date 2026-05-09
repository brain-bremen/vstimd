use std::os::fd::OwnedFd;
use input::event::keyboard::KeyboardEventTrait as _;
use std::path::Path;

// ── Application-level key actions ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKey {
    Escape,
    F1,
    F2,
    D,
}

// ── TTY keyboard suppression guard ───────────────────────────────────────────

/// Disables echo and canonical processing on the controlling TTY for the
/// lifetime of DRM mode. Flushes any buffered input on drop so characters
/// typed during the session don't appear in the shell afterwards.
///
/// Uses tcsetattr rather than KDSKBMODE: the latter only works on real VT
/// console nodes and requires CAP_SYS_TTY_CONFIG; tcsetattr works on any
/// tty type (VT or pts) without elevated permissions.
struct TtyKbdGuard {
    fd: libc::c_int,
    saved: libc::termios,
}

impl TtyKbdGuard {
    fn acquire() -> Option<Self> {
        let fd = unsafe { libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
        if fd < 0 {
            log::warn!("wonderlamp: could not open /dev/tty — keys may echo to terminal");
            return None;
        }
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut saved) } < 0 {
            unsafe { libc::close(fd) };
            return None;
        }
        let mut raw = saved;
        // Disable echo and canonical (line-buffered) mode.
        raw.c_lflag &= !(libc::ECHO | libc::ECHOE | libc::ECHOK | libc::ECHONL
            | libc::ICANON | libc::ISIG);
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } < 0 {
            log::warn!("wonderlamp: tcsetattr failed — keys may echo to terminal");
            unsafe { libc::close(fd) };
            return None;
        }
        Some(Self { fd, saved })
    }
}

impl Drop for TtyKbdGuard {
    fn drop(&mut self) {
        unsafe {
            // Discard any keys buffered during DRM mode before restoring.
            libc::tcflush(self.fd, libc::TCIFLUSH);
            libc::tcsetattr(self.fd, libc::TCSANOW, &self.saved);
            libc::close(self.fd);
        }
    }
}

// ── libinput interface impl ───────────────────────────────────────────────────

struct Interface;

impl input::LibinputInterface for Interface {
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        use std::os::unix::fs::OpenOptionsExt as _;
        std::fs::OpenOptions::new()
            .read(true)
            .write(flags & 0b11 != 0) // O_WRONLY=1, O_RDWR=2
            .custom_flags(flags)
            .open(path)
            .map(|f| f.into())
            .map_err(|e| e.raw_os_error().unwrap_or(-1))
    }

    fn close_restricted(&mut self, fd: OwnedFd) {
        drop(fd);
    }
}

// ── InputState ───────────────────────────────────────────────────────────────

/// Wraps a libinput context and provides a simple per-frame key event drain.
pub struct InputState {
    ctx: input::Libinput,
    #[allow(dead_code)] // held for its Drop side-effect
    tty_kbd_guard: Option<TtyKbdGuard>,
}

impl InputState {
    pub fn new() -> Self {
        let tty_kbd_guard = TtyKbdGuard::acquire();
        let mut ctx = input::Libinput::new_with_udev(Interface);
        match ctx.udev_assign_seat("seat0") {
            Ok(()) => Self { ctx, tty_kbd_guard },
            Err(()) => {
                log::error!(
                    "wonderlamp: libinput could not open seat0 — \
                     add the current user to the 'input' group and log out/in:\n  \
                     sudo usermod -aG input $USER"
                );
                std::process::exit(1);
            }
        }
    }

    /// Drain pending events and return any recognised key presses.
    /// Non-blocking — returns immediately if there are no events.
    pub fn poll(&mut self) -> Vec<AppKey> {
        if self.ctx.dispatch().is_err() {
            return vec![];
        }

        let mut keys = Vec::new();
        for event in self.ctx.by_ref() {
            if let input::Event::Keyboard(kb) = event
                && kb.key_state() == input::event::keyboard::KeyState::Pressed
            {
                // Evdev key codes (from linux/input-event-codes.h)
                match kb.key() {
                    1 => keys.push(AppKey::Escape),  // KEY_ESC
                    32 => keys.push(AppKey::D),      // KEY_D
                    59 => keys.push(AppKey::F1),     // KEY_F1
                    60 => keys.push(AppKey::F2),     // KEY_F2
                    _ => {}
                }
            }
        }
        keys
    }
}
