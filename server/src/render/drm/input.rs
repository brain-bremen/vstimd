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
///
/// If libinput fails to initialise (e.g. missing permissions), `poll` returns
/// an empty list so the server still runs headlessly via ZMQ commands.
pub struct InputState {
    ctx: Option<input::Libinput>,
}

impl InputState {
    pub fn new() -> Self {
        let mut ctx = input::Libinput::new_with_udev(Interface);
        match ctx.udev_assign_seat("seat0") {
            Ok(()) => Self { ctx: Some(ctx) },
            Err(()) => {
                eprintln!(
                    "wonderlamp: libinput failed to open seat0 — \
                     keyboard shortcuts unavailable (is the 'input' group set?)"
                );
                Self { ctx: None }
            }
        }
    }

    /// Drain pending events and return any recognised key presses.
    /// Non-blocking — returns immediately if there are no events.
    pub fn poll(&mut self) -> Vec<AppKey> {
        let Some(ctx) = &mut self.ctx else {
            return vec![];
        };

        if ctx.dispatch().is_err() {
            return vec![];
        }

        let mut keys = Vec::new();
        while let Some(event) = ctx.next() {
            if let input::Event::Keyboard(kb) = event {
                if kb.key_state() == input::event::keyboard::KeyState::Pressed {
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
        }
        keys
    }
}
