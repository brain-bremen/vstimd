pub mod vertex;
pub use vertex::Vertex;

pub(crate) mod overlay;
pub(crate) mod tess;
pub(crate) mod vk;

#[cfg(target_os = "linux")]
pub mod drm;
pub mod winit_vk;

#[cfg(target_os = "linux")]
pub use drm::DrmRenderState;
pub use winit_vk::{WindowMode, WinitApp};

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
    eprintln!("Demo: spawned disc (handle {h1}) and rect (handle {h2})");
}
