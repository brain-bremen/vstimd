mod render;
mod scene;

use scene::{
    Deferred, DiscStimulus, RectStimulus, SceneState, ShapeAppearance, Stimulus, StimulusFlags,
    Transform2D,
};

fn main() {
    let mut scene = SceneState::new();

    // Disc (circle) — cyan, left of centre
    let h1 = scene.alloc_stim_handle();
    scene.stimuli.insert(
        h1,
        Stimulus::Disc(DiscStimulus {
            flags:      StimulusFlags { enabled: true, ..Default::default() },
            transform:  Deferred::new(Transform2D { pos: [-150.0, 0.0], angle: 0.0 }),
            appearance: Deferred::new(ShapeAppearance {
                fill_color: [0.0, 0.8, 0.8, 1.0],
                ..Default::default()
            }),
            radius: Deferred::new(80.0),
        }),
    );

    // Rect — magenta, right of centre, rotated 30°
    let h2 = scene.alloc_stim_handle();
    scene.stimuli.insert(
        h2,
        Stimulus::Rect(RectStimulus {
            flags:      StimulusFlags { enabled: true, ..Default::default() },
            transform:  Deferred::new(Transform2D { pos: [150.0, 0.0], angle: 30.0 }),
            appearance: Deferred::new(ShapeAppearance {
                fill_color: [0.8, 0.0, 0.8, 1.0],
                ..Default::default()
            }),
            size: Deferred::new([120.0, 50.0]),
        }),
    );

    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = render::App::new(scene);
    event_loop.run_app(&mut app).unwrap();
}
