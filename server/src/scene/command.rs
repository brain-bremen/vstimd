use crate::proto;
use super::deferred::Deferred;
use super::state::SceneState;
use super::stimulus::{ShapeAppearance, StimulusFlags, Transform2D};
use super::stimulus::RectStimulus;
use super::stimulus::Stimulus;

impl SceneState {
    /// Dispatch a protobuf `Request` and return a `Response`.
    ///
    /// Routing:
    /// - `handle == 0` → system command (e.g. create stimulus)
    /// - `handle > 0`  → stimulus command (e.g. enable, delete)
    pub fn handle_request(&mut self, req: proto::Request) -> proto::Response {
        match req.body {
            None => proto::Response {
                handle: 0,
                error: "empty request body".into(),
            },
            Some(body) => {
                if req.handle == 0 {
                    self.handle_system_command(body)
                } else {
                    self.handle_stimulus_command(req.handle, body)
                }
            }
        }
    }

    fn handle_system_command(&mut self, body: proto::request::Body) -> proto::Response {
        match body {
            proto::request::Body::CreateRect(cmd) => self.cmd_create_rect(cmd),
            _ => proto::Response {
                handle: 0,
                error: "command requires a stimulus handle (handle > 0)".into(),
            },
        }
    }

    fn handle_stimulus_command(
        &mut self,
        handle: u32,
        body: proto::request::Body,
    ) -> proto::Response {
        match body {
            proto::request::Body::CreateRect(_) => proto::Response {
                handle: 0,
                error: "CreateRect is a system command (use handle = 0)".into(),
            },
            proto::request::Body::SetEnabled(cmd) => self.cmd_set_enabled(handle, cmd),
            proto::request::Body::Delete(_) => self.cmd_delete(handle),
        }
    }

    // ── CreateRect ───────────────────────────────────────────────────────────

    fn cmd_create_rect(&mut self, cmd: proto::CreateRect) -> proto::Response {
        let center = cmd.center.unwrap_or_default();
        let width = if cmd.width == 0.0 { 100.0 } else { cmd.width };
        let height = if cmd.height == 0.0 { 100.0 } else { cmd.height };
        let fill = match cmd.fill {
            Some(c) => [c.r, c.g, c.b, c.a],
            None => self.default_fill,
        };

        let handle = self.alloc_stim_handle();
        self.stimuli.insert(
            handle,
            Stimulus::Rect(RectStimulus {
                flags: StimulusFlags {
                    enabled: true,
                    ..Default::default()
                },
                transform: Deferred::new(Transform2D {
                    pos: [center.x, center.y],
                    angle: 0.0,
                }),
                appearance: Deferred::new(ShapeAppearance {
                    fill_color: fill,
                    ..Default::default()
                }),
                size: Deferred::new([width / 2.0, height / 2.0]),
            }),
        );

        proto::Response {
            handle: handle as i32,
            error: String::new(),
        }
    }

    // ── SetEnabled ───────────────────────────────────────────────────────────

    fn cmd_set_enabled(&mut self, handle: u32, cmd: proto::SetEnabled) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            Some(stim) => {
                if self.deferred_mode {
                    stim.flags_mut().enabled_copy = cmd.enabled;
                } else {
                    stim.flags_mut().enabled = cmd.enabled;
                }
                proto::Response {
                    handle: -1,
                    error: String::new(),
                }
            }
            None => proto::Response {
                handle: 0,
                error: format!("stimulus handle {} not found", handle),
            },
        }
    }

    // ── Delete ───────────────────────────────────────────────────────────────

    fn cmd_delete(&mut self, handle: u32) -> proto::Response {
        match self.stimuli.shift_remove(&handle) {
            Some(_) => proto::Response {
                handle: -1,
                error: String::new(),
            },
            None => proto::Response {
                handle: 0,
                error: format!("stimulus handle {} not found", handle),
            },
        }
    }
}
