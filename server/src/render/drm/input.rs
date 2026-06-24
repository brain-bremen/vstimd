use std::os::fd::OwnedFd;
use input::event::keyboard::KeyboardEventTrait as _;
use std::path::Path;

// ── Application-level key actions ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKey {
    Escape,
    F1,
    F2,
    F3,
    D,
    /// Ctrl+Alt+Fn pressed — forward to the kernel as a VT switch.
    SwitchVt(u16),
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
            log::warn!("vstimd: could not open /dev/tty — keys may echo to terminal");
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
            log::warn!("vstimd: tcsetattr failed — keys may echo to terminal");
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
    modifiers: egui::Modifiers,
    #[allow(dead_code)] // held for its Drop side-effect
    tty_kbd_guard: Option<TtyKbdGuard>,
}

impl InputState {
    pub fn new() -> Self {
        let tty_kbd_guard = TtyKbdGuard::acquire();
        let mut ctx = input::Libinput::new_with_udev(Interface);
        match ctx.udev_assign_seat("seat0") {
            Ok(()) => Self { ctx, modifiers: egui::Modifiers::default(), tty_kbd_guard },
            Err(()) => {
                log::error!(
                    "vstimd: libinput could not open seat0 — \
                     add the current user to the 'input' group and log out/in:\n  \
                     sudo usermod -aG input $USER"
                );
                std::process::exit(1);
            }
        }
    }

    /// Suspend libinput, releasing the EVIOCGRAB on all input devices so the
    /// active VT's session can receive input while vstimd is in the background.
    pub fn suspend(&mut self) {
        self.ctx.suspend();
    }

    /// Resume libinput, re-acquiring EVIOCGRAB on all input devices.
    pub fn resume(&mut self) {
        if self.ctx.resume().is_err() {
            log::warn!("vstimd: libinput resume failed");
        }
    }

    /// Drain pending events.  Returns app-level key actions and egui keyboard
    /// events for overlay navigation (Tab, arrows, Enter, Space, etc.).
    /// Non-blocking — returns immediately if there are no events.
    pub fn poll(&mut self) -> (Vec<AppKey>, Vec<egui::Event>) {
        if self.ctx.dispatch().is_err() {
            return (vec![], vec![]);
        }

        let mut app_keys = Vec::new();
        let mut egui_events = Vec::new();

        for event in self.ctx.by_ref() {
            let input::Event::Keyboard(kb) = event else { continue };
            let pressed = kb.key_state() == input::event::keyboard::KeyState::Pressed;

            match kb.key() {
                // Modifier tracking (press + release) — no separate egui event;
                // modifier state is embedded in subsequent key events.
                42 | 54  => self.modifiers.shift = pressed, // KEY_LEFTSHIFT, KEY_RIGHTSHIFT
                29 | 97  => self.modifiers.ctrl  = pressed, // KEY_LEFTCTRL,  KEY_RIGHTCTRL
                56 | 100 => self.modifiers.alt   = pressed, // KEY_LEFTALT,   KEY_RIGHTALT
                // Ctrl+Alt+F1–F12 → VT switch (libinput grabs input exclusively,
                // so the kernel never sees these; we forward them ourselves).
                code @ 59..=68 if pressed && self.modifiers.ctrl && self.modifiers.alt => {
                    app_keys.push(AppKey::SwitchVt((code - 58) as u16)); // F1=59→1 … F10=68→10
                }
                87 if pressed && self.modifiers.ctrl && self.modifiers.alt => {
                    app_keys.push(AppKey::SwitchVt(11)); // KEY_F11
                }
                88 if pressed && self.modifiers.ctrl && self.modifiers.alt => {
                    app_keys.push(AppKey::SwitchVt(12)); // KEY_F12
                }
                // App-level keys (press only)
                1  if pressed => app_keys.push(AppKey::Escape), // KEY_ESC
                32 if pressed => app_keys.push(AppKey::D),      // KEY_D
                59 if pressed => app_keys.push(AppKey::F1),     // KEY_F1
                60 if pressed => app_keys.push(AppKey::F2),     // KEY_F2
                61 if pressed => app_keys.push(AppKey::F3),     // KEY_F3
                // Navigation / interaction keys → egui events (press + release)
                code => {
                    if let Some(key) = evdev_to_egui_key(code) {
                        egui_events.push(egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers: self.modifiers,
                        });
                    }
                }
            }
        }

        (app_keys, egui_events)
    }
}

/// Map evdev key codes (linux/input-event-codes.h) to egui navigation keys.
fn evdev_to_egui_key(code: u32) -> Option<egui::Key> {
    Some(match code {
        14 => egui::Key::Backspace,
        15 => egui::Key::Tab,
        28 | 96 => egui::Key::Enter,   // KEY_ENTER, KEY_KPENTER
        57 => egui::Key::Space,
        102 => egui::Key::Home,        // KEY_HOME
        103 => egui::Key::ArrowUp,     // KEY_UP
        104 => egui::Key::PageUp,      // KEY_PAGEUP
        105 => egui::Key::ArrowLeft,   // KEY_LEFT
        106 => egui::Key::ArrowRight,  // KEY_RIGHT
        107 => egui::Key::End,         // KEY_END
        108 => egui::Key::ArrowDown,   // KEY_DOWN
        109 => egui::Key::PageDown,    // KEY_PAGEDOWN
        _ => return None,
    })
}
