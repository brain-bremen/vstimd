/// Benchmarks for the CPU-side tessellation path.
///
/// These run without a display or GPU and are designed to be repeatable on
/// any target system (Jetson Nano, Raspberry Pi 5, development machine).
///
/// Run with:
///   cargo bench --bench tess
///   cargo bench --bench tess -- --output-format bencher | tee bench.txt
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

use vstimd::render::tess::tessellate_stimulus;
use vstimd::scene::{
    Deferred, DiscStimulus, EllipseStimulus, RectStimulus, SceneState, ShapeAppearance,
    ShapeStimulus, Stimulus, StimulusFlags, Transform2D,
};

const SCREEN: (u32, u32) = (2560, 1440);

fn make_rect() -> Stimulus {
    Stimulus::Shape(ShapeStimulus::Rect(RectStimulus {
        flags: StimulusFlags::enabled(true),
        transform: Deferred::new(Transform2D { pos: [100.0, 50.0], angle: 15.0 }),
        appearance: Deferred::new(ShapeAppearance {
            fill_color: vstimd::Color::new(1.0, 0.0, 0.0, 1.0),
            ..Default::default()
        }),
        size: Deferred::new([120.0, 60.0]),
    }))
}

fn make_disc() -> Stimulus {
    Stimulus::Shape(ShapeStimulus::Disc(DiscStimulus {
        flags: StimulusFlags::enabled(true),
        transform: Deferred::new(Transform2D { pos: [-200.0, 100.0], angle: 0.0 }),
        appearance: Deferred::new(ShapeAppearance {
            fill_color: vstimd::Color::new(0.0, 0.8, 0.8, 1.0),
            ..Default::default()
        }),
        radius: Deferred::new(80.0),
    }))
}

fn make_ellipse() -> Stimulus {
    Stimulus::Shape(ShapeStimulus::Ellipse(EllipseStimulus {
        flags: StimulusFlags::enabled(true),
        transform: Deferred::new(Transform2D { pos: [300.0, -100.0], angle: 30.0 }),
        appearance: Deferred::new(ShapeAppearance {
            fill_color: vstimd::Color::new(0.5, 0.5, 1.0, 1.0),
            ..Default::default()
        }),
        radii: Deferred::new([100.0, 50.0]),
    }))
}

fn bench_single_stimulus(c: &mut Criterion) {
    let rect = make_rect();
    let disc = make_disc();
    let ellipse = make_ellipse();

    let mut group = c.benchmark_group("tessellate_single");
    group.bench_function("Rect", |b| {
        b.iter(|| tessellate_stimulus(black_box(&rect), black_box(SCREEN)))
    });
    group.bench_function("Disc", |b| {
        b.iter(|| tessellate_stimulus(black_box(&disc), black_box(SCREEN)))
    });
    group.bench_function("Ellipse", |b| {
        b.iter(|| tessellate_stimulus(black_box(&ellipse), black_box(SCREEN)))
    });
    group.finish();
}

fn bench_scene_update(c: &mut Criterion) {
    let counts = [1usize, 5, 10, 20, 50];
    let mut group = c.benchmark_group("scene_update_n_stimuli");

    for &n in &counts {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            // Build a scene with n disc+rect pairs (same as the demo key).
            let mut scene = SceneState::new();
            for i in 0..n {
                let h = scene.alloc_stim_handle();
                scene.stimuli.insert(
                    h,
                    Stimulus::Shape(ShapeStimulus::Disc(DiscStimulus {
                        flags: StimulusFlags::enabled(true),
                        transform: Deferred::new(Transform2D {
                            pos: [i as f32 * 30.0, 0.0],
                            angle: 0.0,
                        }),
                        appearance: Deferred::new(ShapeAppearance::default()),
                        radius: Deferred::new(40.0),
                    })),
                );
                let h = scene.alloc_stim_handle();
                scene.stimuli.insert(
                    h,
                    Stimulus::Shape(ShapeStimulus::Rect(RectStimulus {
                        flags: StimulusFlags::enabled(true),
                        transform: Deferred::new(Transform2D {
                            pos: [i as f32 * 30.0, 100.0],
                            angle: 0.0,
                        }),
                        appearance: Deferred::new(ShapeAppearance::default()),
                        size: Deferred::new([60.0, 30.0]),
                    })),
                );
            }

            b.iter(|| {
                let handles: Vec<u32> = scene.stimuli.keys().copied().collect();
                for handle in &handles {
                    let (verts, idxs) =
                        tessellate_stimulus(&scene.stimuli[handle], black_box(SCREEN));
                    black_box((verts, idxs));
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_single_stimulus, bench_scene_update);
criterion_main!(benches);
