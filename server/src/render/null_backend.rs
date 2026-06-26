use crate::render::backend::BackendData;

pub struct NullBackend {
    data: BackendData,
}

impl NullBackend {
    pub fn new(data: BackendData) -> Self {
        Self { data }
    }

    pub fn run(self, on_ready: impl FnOnce()) {
        let BackendData { scene, vtl, .. } = self.data;

        log::info!("vstimd: null renderer — ZMQ server + animation loop running, no display");
        on_ready();

        let frame_period = {
            let s = scene.read().unwrap();
            std::time::Duration::from_secs_f32(1.0 / s.runtime.frame_rate)
        };
        let mut output_pending = [0u64; vtl::MAX_BANKS];
        loop {
            if crate::shutdown::is_requested() {
                break;
            }
            let t0 = std::time::Instant::now();
            let edges = vtl
                .as_ref()
                .and_then(|v| v.lock().ok().map(|mut g| g.poll()))
                .unwrap_or_default();
            {
                let mut s = scene.write().unwrap();
                if s.runtime.pending_flip {
                    s.apply_flip();
                }
                s.runtime.frame_count += 1;
                let _ = s.runtime.frame_notifier.send(s.runtime.frame_count);
                let output_snapshot = [0u64; vtl::MAX_BANKS];
                s.advance_animations(&edges, &output_snapshot, &mut output_pending);
            }
            if let Some(remaining) = frame_period.checked_sub(t0.elapsed()) {
                std::thread::sleep(remaining);
            }
        }
    }
}
