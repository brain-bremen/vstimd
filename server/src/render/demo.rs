pub(crate) fn spawn_demo_stimuli(
    scene: &std::sync::Arc<std::sync::RwLock<crate::scene::SceneState>>,
) {
    use crate::scene::{
        Anchor, CircleStimulus, Deferred, GratingParams, GratingStimulus, LanguageStyle,
        RectStimulus, ShapeAppearance, ShapeCommon, Stimulus, StimulusFlags, StimulusSceneEntry,
        TextRenderParams, TextStimulus, Transform2D, Waveform,
    };
    use rand::RngExt;
    use uuid::Uuid;

    let mut rng = rand::rng();

    let mut sc = scene.write().expect("scene lock poisoned");
    let h1 = sc.alloc_stim_handle();
    sc.stimuli.insert(
        h1,
        StimulusSceneEntry::new(
            Uuid::new_v4(),
            Some("demo_circle".into()),
            Stimulus::Circle(CircleStimulus {
                common: ShapeCommon {
                    flags: StimulusFlags::enabled(true),
                    transform: Deferred::new(Transform2D {
                        pos: [
                            rng.random_range(-500.0..500.0),
                            rng.random_range(-500.0..500.0),
                        ],
                        angle: 0.0,
                    }),
                    appearance: Deferred::new(ShapeAppearance {
                        fill_color: crate::Color::new(0.0, 0.8, 0.8, 1.0),
                        ..Default::default()
                    }),
                },
                radius: Deferred::new(80.0),
            }),
        ),
    );
    let h2 = sc.alloc_stim_handle();
    sc.stimuli.insert(
        h2,
        StimulusSceneEntry::new(
            Uuid::new_v4(),
            Some("demo_rect".into()),
            Stimulus::Rect(RectStimulus {
                common: ShapeCommon {
                    flags: StimulusFlags::enabled(true),
                    transform: Deferred::new(Transform2D {
                        pos: [
                            rng.random_range(-500.0..500.0),
                            rng.random_range(-500.0..500.0),
                        ],
                        angle: 30.0,
                    }),
                    appearance: Deferred::new(ShapeAppearance {
                        fill_color: crate::Color::new(0.8, 0.0, 0.8, 1.0),
                        ..Default::default()
                    }),
                },
                size: Deferred::new([120.0, 50.0]),
            }),
        ),
    );
    let h3 = sc.alloc_stim_handle();
    sc.stimuli.insert(
        h3,
        StimulusSceneEntry::new(
            Uuid::new_v4(),
            Some("demo_grating".into()),
            Stimulus::Grating(GratingStimulus::new(
                [100.0, -200.0],
                0.0,
                [100.0, 100.0],
                GratingParams {
                    sf: 0.05,
                    contrast: 1.0,
                    drift_speed: 1.0,
                    waveform: Waveform::Sin,
                    ..Default::default()
                },
            )),
        ),
    );
    let h4 = sc.alloc_stim_handle();
    sc.stimuli.insert(
        h4,
        StimulusSceneEntry::new(
            Uuid::new_v4(),
            Some("demo_text".into()),
            Stimulus::Text(TextStimulus::new(
                [0.0, 200.0],
                [400.0, 80.0],
                "vstimd".into(),
                "".into(), // falls back to DEFAULT_FONT_FAMILY ("Ubuntu Light")
                48.0,
                Anchor::Center,
                LanguageStyle::default(),
                TextRenderParams {
                    color: crate::Color::new(1.0, 1.0, 0.0, 1.0),
                    ..Default::default()
                },
            )),
        ),
    );
    log::info!("Demo: spawned circle #{h1}, rect #{h2}, grating #{h3}, text #{h4}");
}
