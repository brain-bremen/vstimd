pub mod vertex;
pub use vertex::Vertex;

pub mod display_info;
pub use display_info::StimulusDisplayInfo;

pub mod system_info;
pub use system_info::{query_local_ip, SystemInfo};

pub(crate) mod overlay;
pub mod tess;
pub(crate) mod vk;

#[cfg(target_os = "linux")]
pub mod drm;
pub mod winit_vk;

#[cfg(target_os = "linux")]
pub use drm::DrmRenderState;
pub use winit_vk::WinitApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    Fullscreen,
    Windowed { width: u32, height: u32 },
}

impl Default for WindowMode {
    fn default() -> Self {
        Self::Fullscreen
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderTarget {
    Drm,
    Desktop(WindowMode),
}

pub(crate) fn spawn_demo_stimuli(
    scene: &std::sync::Arc<std::sync::RwLock<crate::scene::SceneState>>,
) {
    use crate::scene::{
        Deferred, DiscStimulus, RectStimulus, ShapeAppearance, Stimulus, StimulusFlags, Transform2D,
    };

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
                pos: [-150.0, 0.0],
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
                pos: [150.0, 0.0],
                angle: 30.0,
            }),
            appearance: Deferred::new(ShapeAppearance {
                fill_color: [0.8, 0.0, 0.8, 1.0],
                ..Default::default()
            }),
            size: Deferred::new([120.0, 50.0]),
        }),
    );
    log::info!("Demo: spawned disc (handle {h1}) and rect (handle {h2})");
}
