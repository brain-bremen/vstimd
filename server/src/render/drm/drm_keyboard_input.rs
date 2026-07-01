use input::event::keyboard::KeyboardEventTrait as _;
use std::os::fd::OwnedFd;
use std::path::Path;

use crate::render::AppKey;
use crate::render::overlay_ui::OverlayGroup;

// ── TTY keyboard suppression guard ───────────────────────────────────────────

/// Disables echo and canonical processing on the target VT (tty3 by default,
/// `VSTIMD_TTY` override) for the lifetime of DRM mode. Flushes any buffered
/// input on drop so characters typed during the session don't appear once
/// the VT returns to text mode.
///
/// Deliberately opens the target VT device directly rather than `/dev/tty`
/// (the calling process's controlling terminal) — same reasoning as
/// [`super::drm_virtual_terminal::DrmVtGuard`]. Over SSH (no `DISPLAY` →
/// DRM auto-detected), `/dev/tty` is the SSH pty, not the console VT; tweaking
/// its termios would affect the SSH session itself and had previously
/// swallowed Ctrl+C there.
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
        let target_vt = super::drm_virtual_terminal::vt_number_from_env();
        let fd = super::drm_virtual_terminal::open_vt(target_vt);
        if fd < 0 {
            log::warn!("vstimd: could not open /dev/tty{target_vt} — keys may echo to terminal");
            return None;
        }
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut saved) } < 0 {
            unsafe { libc::close(fd) };
            return None;
        }
        let mut raw = saved;
        // Disable echo, canonical (line-buffered) mode, and signal generation
        // (Ctrl+C/Ctrl+\/Ctrl+Z) on the console VT. Real keyboard input is
        // grabbed exclusively via libinput and never reaches this tty, so
        // ISIG here is inert — but it's the console VT's own termios, not
        // whatever terminal launched the process, so it can't interfere with
        // signal delivery over SSH.
        raw.c_lflag &= !(libc::ECHO | libc::ECHOE | libc::ECHOK | libc::ECHONL | libc::ICANON | libc::ISIG);
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
            Ok(()) => Self {
                ctx,
                modifiers: egui::Modifiers::default(),
                tty_kbd_guard,
            },
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
            let input::Event::Keyboard(kb) = event else {
                continue;
            };
            let pressed = kb.key_state() == input::event::keyboard::KeyState::Pressed;
            let code = kb.key();

            // Modifier tracking (press + release) — no separate egui event;
            // modifier state is embedded in subsequent key events.
            match code {
                42 | 54 => { self.modifiers.shift = pressed; continue; } // L/R SHIFT
                29 | 97 => { self.modifiers.ctrl  = pressed; continue; } // L/R CTRL
                56 | 100 => { self.modifiers.alt  = pressed; continue; } // L/R ALT
                _ => {}
            }

            // Ctrl+Alt+F1–F12 → VT switch (libinput grabs input exclusively, so the
            // kernel never sees these; we forward them ourselves). Takes priority
            // over plain-Fn group selection.
            if pressed && self.modifiers.ctrl && self.modifiers.alt {
                let vt = match code {
                    59..=68 => Some((code - 58) as u16), // F1→1 … F10→10
                    87 => Some(11),                      // F11
                    88 => Some(12),                      // F12
                    _ => None,
                };
                if let Some(n) = vt {
                    app_keys.push(AppKey::SwitchVt(n));
                    continue;
                }
            }

            // App-level keys (press only). F-keys and Esc never type, so they
            // `continue`; KEY_D falls through so 'd' can also reach text fields.
            match code {
                1 if pressed => { app_keys.push(AppKey::Escape); continue; } // KEY_ESC
                41 if pressed => { app_keys.push(AppKey::ToggleOverlay); continue; } // KEY_GRAVE
                59..=68 | 87 | 88 if pressed => {
                    let n = match code { 87 => 11, 88 => 12, _ => (code - 58) as u8 };
                    if let Some(group) = OverlayGroup::from_fkey(n) {
                        if self.modifiers.shift {
                            app_keys.push(AppKey::HideGroup(group));
                        } else {
                            app_keys.push(AppKey::ShowGroup(group));
                        }
                    }
                    continue;
                }
                32 if pressed => app_keys.push(AppKey::D), // KEY_D — also types below
                _ => {}
            }

            // Navigation keys → egui events (press + release).
            if let Some(key) = evdev_to_egui_key(code) {
                egui_events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: self.modifiers,
                });
            }
            // Printable characters → egui text input (press only). Without this,
            // dialog text fields cannot be typed into in DRM mode.
            if pressed && let Some(ch) = evdev_to_char(code, self.modifiers.shift) {
                egui_events.push(egui::Event::Text(ch.to_string()));
            }
        }

        (app_keys, egui_events)
    }
}

/// Map evdev key codes to characters for text entry (US QWERTY layout).
/// Returns the shifted glyph when `shift` is held.
fn evdev_to_char(code: u32, shift: bool) -> Option<char> {
    let (lo, hi): (char, char) = match code {
        2 => ('1', '!'), 3 => ('2', '@'), 4 => ('3', '#'), 5 => ('4', '$'),
        6 => ('5', '%'), 7 => ('6', '^'), 8 => ('7', '&'), 9 => ('8', '*'),
        10 => ('9', '('), 11 => ('0', ')'),
        12 => ('-', '_'), 13 => ('=', '+'),
        16 => ('q', 'Q'), 17 => ('w', 'W'), 18 => ('e', 'E'), 19 => ('r', 'R'),
        20 => ('t', 'T'), 21 => ('y', 'Y'), 22 => ('u', 'U'), 23 => ('i', 'I'),
        24 => ('o', 'O'), 25 => ('p', 'P'), 26 => ('[', '{'), 27 => (']', '}'),
        30 => ('a', 'A'), 31 => ('s', 'S'), 32 => ('d', 'D'), 33 => ('f', 'F'),
        34 => ('g', 'G'), 35 => ('h', 'H'), 36 => ('j', 'J'), 37 => ('k', 'K'),
        38 => ('l', 'L'), 39 => (';', ':'), 40 => ('\'', '"'), 43 => ('\\', '|'),
        44 => ('z', 'Z'), 45 => ('x', 'X'), 46 => ('c', 'C'), 47 => ('v', 'V'),
        48 => ('b', 'B'), 49 => ('n', 'N'), 50 => ('m', 'M'),
        51 => (',', '<'), 52 => ('.', '>'), 53 => ('/', '?'),
        57 => (' ', ' '),
        _ => return None,
    };
    Some(if shift { hi } else { lo })
}

/// Map evdev key codes (linux/input-event-codes.h) to egui navigation keys.
fn evdev_to_egui_key(code: u32) -> Option<egui::Key> {
    Some(match code {
        14 => egui::Key::Backspace,
        15 => egui::Key::Tab,
        28 | 96 => egui::Key::Enter, // KEY_ENTER, KEY_KPENTER
        57 => egui::Key::Space,
        102 => egui::Key::Home,       // KEY_HOME
        103 => egui::Key::ArrowUp,    // KEY_UP
        104 => egui::Key::PageUp,     // KEY_PAGEUP
        105 => egui::Key::ArrowLeft,  // KEY_LEFT
        106 => egui::Key::ArrowRight, // KEY_RIGHT
        107 => egui::Key::End,        // KEY_END
        108 => egui::Key::ArrowDown,  // KEY_DOWN
        109 => egui::Key::PageDown,   // KEY_PAGEDOWN
        _ => return None,
    })
}
