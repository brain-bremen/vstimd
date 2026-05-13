pub mod vertex;
pub use vertex::Vertex;

pub mod display_info;
pub use display_info::StimulusDisplayInfo;

pub mod system_info;
pub use system_info::{query_local_ip, SystemInfo};

pub(crate) mod benchmark;
pub use benchmark::BenchmarkState;
pub(crate) mod overlay;
pub mod tess;
pub(crate) mod vk;

#[cfg(target_os = "linux")]
pub mod drm;
pub mod winit_vk;

#[cfg(target_os = "linux")]
pub use drm::DrmRenderState;
pub use winit_vk::WinitApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowMode {
    #[default]
    Fullscreen,
    Windowed { width: u32, height: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderTarget {
    Drm,
    Desktop(WindowMode),
    Null,
}

pub(crate) fn spawn_demo_stimuli(
    scene: &std::sync::Arc<std::sync::RwLock<crate::scene::SceneState>>,
) {
    use rand::Rng;
    use rand::RngExt;
    use crate::scene::{
        Deferred, DiscStimulus, GratingParams, GratingStimulus, RectStimulus, ShapeAppearance,
        Stimulus, StimulusFlags, Transform2D, Waveform,
    };

    let mut rng = rand::rng();


    let mut sc = scene.write().expect("scene lock poisoned");
    let h1 = sc.alloc_stim_handle();
    sc.stimuli.insert(
        h1,
        Stimulus::Disc(DiscStimulus {
            flags: StimulusFlags {
                enabled: true,
                ..Default::default()
            },
            transform: Deferred::new(Transform2D {
                pos: [rng.random_range(-500.0..500.0), rng.random_range(-500.0..500.0)],
                angle: 0.0,
            }),
            appearance: Deferred::new(ShapeAppearance {
                fill_color: [0.0, 0.8, 0.8, 1.0],
                ..Default::default()
            }),
            radius: Deferred::new(80.0),
        }),
    );
    let h2 = sc.alloc_stim_handle();
    sc.stimuli.insert(
        h2,
        Stimulus::Rect(RectStimulus {
            flags: StimulusFlags {
                enabled: true,
                ..Default::default()
            },
            transform: Deferred::new(Transform2D {
                pos: [rng.random_range(-500.0..500.0), rng.random_range(-500.0..500.0)],
                angle: 30.0,
            }),
            appearance: Deferred::new(ShapeAppearance {
                fill_color: [0.8, 0.0, 0.8, 1.0],
                ..Default::default()
            }),
            size: Deferred::new([120.0, 50.0]),
        }),
    );
    let h3 = sc.alloc_stim_handle();
    sc.stimuli.insert(
        h3,
        Stimulus::Grating(GratingStimulus {
            flags: StimulusFlags { enabled: true, ..Default::default() },
            transform: Deferred::new(Transform2D { pos: [0.0, 0.0], angle: 0.0 }),
            color: Deferred::new([1.0, 1.0, 1.0, 1.0]),
            size: Deferred::new([100.0, 100.0]),
            params: Deferred::new(GratingParams {
                sf: 0.05,
                contrast: 1.0,
                drift_speed: 1.0,
                waveform: Waveform::Sin,
                ..Default::default()
            }),
            phase_accum: 0.0,
        }),
    );
    log::info!("Demo: spawned disc #{h1}, rect #{h2}, grating #{h3}");
}
