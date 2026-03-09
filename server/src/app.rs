use std::sync::{Arc, RwLock};

use crate::render::RenderState;
use crate::scene::SceneState;

pub struct App {
    /// Held here until the window is created, then moved into `RenderState`.
    scene: Option<Arc<RwLock<SceneState>>>,
    state: Option<RenderState>,
}

impl App {
    pub fn new(scene: Arc<RwLock<SceneState>>) -> Self {
        Self { scene: Some(scene), state: None }
    }
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.state.is_none() {
            // Always open on the primary monitor (fallback: first available).
            // Borderless fullscreen lets the DXGI flip chain go directly to
            // the display (Hardware: Independent Flip in PresentMon).
            let monitor = event_loop
                .primary_monitor()
                .or_else(|| event_loop.available_monitors().next());
            let fullscreen = winit::window::Fullscreen::Borderless(monitor);

            let attrs = winit::window::Window::default_attributes()
                .with_title("Wonderlamp")
                .with_fullscreen(Some(fullscreen));
            let window = std::sync::Arc::new(event_loop.create_window(attrs).unwrap());
            let scene = self.scene.take().expect("scene already consumed");
            self.state = Some(RenderState::new(
                window,
                scene,
                #[cfg(feature = "overlay")]
                event_loop,
            ));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(state) = &mut self.state else { return };

        #[cfg(feature = "overlay")]
        if let Some(overlay) = &mut state.overlay {
            let response = overlay.on_window_event(&state.window, &event);
            if response.consumed {
                return;
            }
        }

        match event {
            winit::event::WindowEvent::CloseRequested => event_loop.exit(),
            winit::event::WindowEvent::Resized(size) => state.resize(size),
            winit::event::WindowEvent::KeyboardInput {
                event: winit::event::KeyEvent {
                    physical_key: winit::keyboard::PhysicalKey::Code(
                        winit::keyboard::KeyCode::F1,
                    ),
                    state: winit::event::ElementState::Pressed,
                    ..
                },
                ..
            } => {
                state.show_overlay = !state.show_overlay;
            }
            winit::event::WindowEvent::RedrawRequested => state.tick(),
            _ => {}
        }
    }
}
