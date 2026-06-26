use std::sync::{Arc, Mutex, RwLock};

use crate::render::system_info::HostInfo;
use crate::scene::SceneState;
use crate::vtl_state::VtlState;

/// Everything a render backend needs from `main`: shared state plus host info.
pub struct BackendData {
    pub scene: Arc<RwLock<SceneState>>,
    pub vtl: Option<Arc<Mutex<VtlState>>>,
    pub host_info: HostInfo,
}

pub trait RenderBackend {
    fn run(self, on_ready: impl FnOnce());
}
