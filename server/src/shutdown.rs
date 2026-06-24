use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Signal the process to shut down. Safe to call from signal handlers and
/// async contexts. Checked by all render loop backends each frame.
///
/// Release/Acquire pairing ensures the store from a signal handler (possibly
/// on a different thread on ARM) is visible to the render loop's load.
pub fn request() {
    REQUESTED.store(true, Ordering::Release);
}

pub fn is_requested() -> bool {
    REQUESTED.load(Ordering::Acquire)
}
