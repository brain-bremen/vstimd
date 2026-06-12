use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Signal the process to shut down. Safe to call from signal handlers and
/// async contexts. Checked by all render loop backends each frame.
pub fn request() {
    REQUESTED.store(true, Ordering::Relaxed);
}

pub fn is_requested() -> bool {
    REQUESTED.load(Ordering::Relaxed)
}
