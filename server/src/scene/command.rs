use super::deferred::Deferred;
use super::state::SceneState;
use super::stimulus::{DiscStimulus, EllipseStimulus, RectStimulus, ShapeStimulus, Stimulus};
use super::stimulus::{DrawMode as SceneDrawMode, ShapeAppearance, StimulusFlags, Transform2D};
use super::stimulus::grating::{
    GratingStimulus, grating_params_from_proto, grating_query_params, proto_to_mask,
    proto_to_waveform,
};
use crate::ipc::{err, err_not_found, err_wrong_type, ok_ack, ok_body, ok_handle};
use crate::proto;
use crate::proto::request;

// ── Request summary for the command log ───────────────────────────────────────

fn command_summary(req: &proto::Request) -> String {
    match &req.body {
        Some(request::Body::CreateRect(c)) => {
            format!("CreateRect {:.0}×{:.0}", c.width, c.height)
        }
        Some(request::Body::CreateCircle(c)) => format!("CreateCircle r={:.0}", c.radius),
        Some(request::Body::CreateEllipse(c)) => {
            format!("CreateEllipse {:.0}×{:.0}", c.width, c.height)
        }
        Some(request::Body::SetEnabled(c)) => {
            format!("SetEnabled({})", if c.enabled { "on" } else { "off" })
        }
        Some(request::Body::Delete(_)) => "Delete".into(),
        Some(request::Body::SetPosition(c)) => format!("SetPosition({:.1},{:.1})", c.x, c.y),
        Some(request::Body::SetOrientation(c)) => format!("SetOrientation({:.1}°)", c.angle_deg),
        Some(request::Body::SetFillColor(_)) => "SetFillColor".into(),
        Some(request::Body::SetAlpha(c)) => format!("SetAlpha({:.2})", c.opacity),
        Some(request::Body::SetRectSize(c)) => {
            format!("SetRectSize {:.0}×{:.0}", c.width, c.height)
        }
        Some(request::Body::SetDiscRadius(c)) => format!("SetDiscRadius({:.0})", c.radius),
        Some(request::Body::SetEllipseSize(c)) => {
            format!("SetEllipseSize {:.0}×{:.0}", c.width, c.height)
        }
        Some(request::Body::SetDrawMode(_)) => "SetDrawMode".into(),
        Some(request::Body::SetOutlineColor(_)) => "SetOutlineColor".into(),
        Some(request::Body::SetOutlineWidth(c)) => format!("SetOutlineWidth({:.1})", c.line_width),
        Some(request::Body::SetBackground(_)) => "SetBackground".into(),
        Some(request::Body::SetDeferredMode(c)) => {
            if c.cancel {
                "SetDeferredMode(cancel)".into()
            } else if c.active {
                "SetDeferredMode(begin)".into()
            } else {
                "SetDeferredMode(end/flip)".into()
            }
        }
        Some(request::Body::DeleteAll(_)) => "DeleteAll".into(),
        Some(request::Body::SetAllEnabled(c)) => {
            format!("SetAllEnabled({})", if c.enabled { "on" } else { "off" })
        }
        Some(request::Body::CreateGrating(c)) => {
            let center = c.center.as_ref();
            format!(
                "CreateGrating {:.0}×{:.0} sf={:.4} pos=({:.1},{:.1})",
                c.width, c.height, c.sf,
                center.map_or(0.0, |v| v.x),
                center.map_or(0.0, |v| v.y),
            )
        }
        Some(request::Body::SetGratingPhase(c)) => format!("SetGratingPhase({:.3})", c.phase),
        Some(request::Body::SetGratingSf(c)) => format!("SetGratingSf({:.4})", c.sf),
        Some(request::Body::SetGratingContrast(c)) => {
            format!("SetGratingContrast({:.2})", c.contrast)
        }
        Some(request::Body::SetGratingWaveform(_)) => "SetGratingWaveform".into(),
        Some(request::Body::SetGratingMask(_)) => "SetGratingMask".into(),
        Some(request::Body::SetGratingDriftSpeed(c)) => {
            format!("SetGratingDriftSpeed({:.3})", c.speed)
        }
        Some(request::Body::SetGratingDriftDecoupled(c)) => {
            format!("SetGratingDriftDecoupled({})", c.decoupled)
        }
        Some(request::Body::SetGratingDriftAngle(c)) => {
            format!("SetGratingDriftAngle({:.1}°)", c.angle_deg)
        }
        Some(request::Body::SetGratingForeColor(_)) => "SetGratingForeColor".into(),
        Some(request::Body::SetGratingBackColor(_)) => "SetGratingBackColor".into(),
        Some(request::Body::SetGratingOpacity(c)) => format!("SetGratingOpacity({:.2})", c.opacity),
        Some(request::Body::QueryServerInfo(_)) => "QueryServerInfo".into(),
        Some(request::Body::QueryStimulus(_)) => "QueryStimulus".into(),
        Some(request::Body::ListStimuli(_)) => "ListStimuli".into(),
        None => "?".into(),
    }
}

// ── DrawMode conversion ───────────────────────────────────────────────────────

fn proto_draw_mode_to_scene(mode: i32) -> Result<SceneDrawMode, Box<proto::Response>> {
    match proto::DrawMode::try_from(mode).unwrap_or(proto::DrawMode::Unspecified) {
        proto::DrawMode::Unspecified => Err(Box::new(err(
            proto::ErrorCode::InvalidArgument,
            "draw_mode must be set explicitly (UNSPECIFIED is not a valid value)",
        ))),
        proto::DrawMode::Filled => Ok(SceneDrawMode::Fill),
        proto::DrawMode::Outlined => Ok(SceneDrawMode::Stroke),
        proto::DrawMode::FilledAndOutlined => Ok(SceneDrawMode::FillAndStroke),
    }
}

fn scene_draw_mode_to_proto(mode: SceneDrawMode) -> i32 {
    match mode {
        SceneDrawMode::Fill => proto::DrawMode::Filled as i32,
        SceneDrawMode::Stroke => proto::DrawMode::Outlined as i32,
        SceneDrawMode::FillAndStroke => proto::DrawMode::FilledAndOutlined as i32,
    }
}

// ── Main dispatcher ───────────────────────────────────────────────────────────

impl SceneState {
    pub fn handle_request(&mut self, req: proto::Request) -> proto::Response {
        let log_handle = match &req.target {
            Some(request::Target::Stimulus(h)) => *h,
            _ => 0,
        };
        let log_summary = command_summary(&req);

        let response = match req.body {
            None => err(proto::ErrorCode::InvalidArgument, "empty request body"),
            Some(body) => match req.target {
                Some(request::Target::System(_)) | None => self.handle_system_command(body),
                Some(request::Target::Stimulus(handle)) => {
                    self.handle_stimulus_command(handle, body)
                }
            },
        };

        self.push_command_log(log_handle, log_summary.clone(), &response);

        if response.code == proto::ErrorCode::Ok as i32 {
            if log_handle == 0 {
                log::debug!("ipc: {} → handle {}", log_summary, response.handle);
            } else {
                log::debug!("ipc: [{}] {}", log_handle, log_summary);
            }
        } else {
            log::warn!("ipc: [{}] {} → error {}: {}", log_handle, log_summary, response.code, response.error);
        }

        response
    }

    // ── System command dispatcher ─────────────────────────────────────────────

    fn handle_system_command(&mut self, body: request::Body) -> proto::Response {
        match body {
            request::Body::CreateRect(cmd) => self.cmd_create_rect(cmd),
            request::Body::CreateCircle(cmd) => self.cmd_create_circle(cmd),
            request::Body::CreateEllipse(cmd) => self.cmd_create_ellipse(cmd),
            request::Body::CreateGrating(cmd) => self.cmd_create_grating(cmd),
            request::Body::SetBackground(cmd) => self.cmd_set_background(cmd),
            request::Body::SetDeferredMode(cmd) => self.cmd_set_deferred_mode(cmd),
            request::Body::DeleteAll(_) => self.cmd_delete_all(),
            request::Body::SetAllEnabled(cmd) => self.cmd_set_all_enabled(cmd),
            request::Body::QueryServerInfo(_) => self.cmd_query_server_info(),
            request::Body::ListStimuli(_) => self.cmd_list_stimuli(),
            _ => err(
                proto::ErrorCode::WrongTarget,
                "command requires a stimulus handle (target.stimulus > 0)",
            ),
        }
    }

    // ── Stimulus command dispatcher ───────────────────────────────────────────

    fn handle_stimulus_command(&mut self, handle: u32, body: request::Body) -> proto::Response {
        match body {
            request::Body::CreateRect(_)
            | request::Body::CreateCircle(_)
            | request::Body::CreateEllipse(_)
            | request::Body::CreateGrating(_)
            | request::Body::SetBackground(_)
            | request::Body::SetDeferredMode(_)
            | request::Body::DeleteAll(_)
            | request::Body::SetAllEnabled(_)
            | request::Body::QueryServerInfo(_)
            | request::Body::ListStimuli(_) => err(
                proto::ErrorCode::WrongTarget,
                "system command must use target.system (not a stimulus handle)",
            ),
            request::Body::SetEnabled(cmd) => self.cmd_set_enabled(handle, cmd),
            request::Body::Delete(_) => self.cmd_delete(handle),
            request::Body::SetPosition(cmd) => self.cmd_set_position(handle, cmd),
            request::Body::SetOrientation(cmd) => self.cmd_set_orientation(handle, cmd),
            request::Body::SetFillColor(cmd) => self.cmd_set_fill_color(handle, cmd),
            request::Body::SetAlpha(cmd) => self.cmd_set_alpha(handle, cmd),
            request::Body::SetRectSize(cmd) => self.cmd_set_rect_size(handle, cmd),
            request::Body::SetDiscRadius(cmd) => self.cmd_set_disc_radius(handle, cmd),
            request::Body::SetEllipseSize(cmd) => self.cmd_set_ellipse_size(handle, cmd),
            request::Body::SetDrawMode(cmd) => self.cmd_set_draw_mode(handle, cmd),
            request::Body::SetOutlineColor(cmd) => self.cmd_set_outline_color(handle, cmd),
            request::Body::SetOutlineWidth(cmd) => self.cmd_set_outline_width(handle, cmd),
            request::Body::SetGratingPhase(cmd) => self.cmd_set_grating_phase(handle, cmd),
            request::Body::SetGratingSf(cmd) => self.cmd_set_grating_sf(handle, cmd),
            request::Body::SetGratingContrast(cmd) => self.cmd_set_grating_contrast(handle, cmd),
            request::Body::SetGratingWaveform(cmd) => self.cmd_set_grating_waveform(handle, cmd),
            request::Body::SetGratingMask(cmd) => self.cmd_set_grating_mask(handle, cmd),
            request::Body::SetGratingDriftSpeed(cmd) => {
                self.cmd_set_grating_drift_speed(handle, cmd)
            }
            request::Body::SetGratingDriftDecoupled(cmd) => {
                self.cmd_set_grating_drift_decoupled(handle, cmd)
            }
            request::Body::SetGratingDriftAngle(cmd) => {
                self.cmd_set_grating_drift_angle(handle, cmd)
            }
            request::Body::SetGratingForeColor(cmd) => {
                self.cmd_set_grating_fore_color(handle, cmd)
            }
            request::Body::SetGratingBackColor(cmd) => {
                self.cmd_set_grating_back_color(handle, cmd)
            }
            request::Body::SetGratingOpacity(cmd) => {
                self.cmd_set_grating_opacity(handle, cmd)
            }
            request::Body::QueryStimulus(_) => self.cmd_query_stimulus(handle),
        }
    }

    // ── CreateRect ────────────────────────────────────────────────────────────

    fn cmd_create_rect(&mut self, cmd: proto::CreateRectRequest) -> proto::Response {
        let center = cmd.center.unwrap_or_default();
        let width = if cmd.width == 0.0 { 100.0 } else { cmd.width };
        let height = if cmd.height == 0.0 { 100.0 } else { cmd.height };
        let fill = color_or_default(cmd.fill, self.default_fill);
        let handle = self.alloc_stim_handle();
        self.stimuli.insert(
            handle,
            Stimulus::Shape(ShapeStimulus::Rect(RectStimulus {
                flags: StimulusFlags { enabled: true, ..Default::default() },
                transform: Deferred::new(Transform2D { pos: [center.x, center.y], angle: 0.0 }),
                appearance: Deferred::new(ShapeAppearance {
                    fill_color: fill,
                    outline_color: self.default_outline,
                    ..Default::default()
                }),
                size: Deferred::new([width / 2.0, height / 2.0]),
            })),
        );
        ok_handle(handle)
    }

    // ── CreateCircle ──────────────────────────────────────────────────────────

    fn cmd_create_circle(&mut self, cmd: proto::CreateCircleRequest) -> proto::Response {
        let center = cmd.center.unwrap_or_default();
        let radius = if cmd.radius == 0.0 { 50.0 } else { cmd.radius };
        let fill = color_or_default(cmd.fill, self.default_fill);
        let handle = self.alloc_stim_handle();
        self.stimuli.insert(
            handle,
            Stimulus::Shape(ShapeStimulus::Disc(DiscStimulus {
                flags: StimulusFlags { enabled: true, ..Default::default() },
                transform: Deferred::new(Transform2D { pos: [center.x, center.y], angle: 0.0 }),
                appearance: Deferred::new(ShapeAppearance {
                    fill_color: fill,
                    outline_color: self.default_outline,
                    ..Default::default()
                }),
                radius: Deferred::new(radius),
            })),
        );
        ok_handle(handle)
    }

    // ── CreateEllipse ─────────────────────────────────────────────────────────

    fn cmd_create_ellipse(&mut self, cmd: proto::CreateEllipseRequest) -> proto::Response {
        let center = cmd.center.unwrap_or_default();
        let width = if cmd.width == 0.0 { 100.0 } else { cmd.width };
        let height = if cmd.height == 0.0 { 100.0 } else { cmd.height };
        let fill = color_or_default(cmd.fill, self.default_fill);
        let handle = self.alloc_stim_handle();
        self.stimuli.insert(
            handle,
            Stimulus::Shape(ShapeStimulus::Ellipse(EllipseStimulus {
                flags: StimulusFlags { enabled: true, ..Default::default() },
                transform: Deferred::new(Transform2D {
                    pos: [center.x, center.y],
                    angle: cmd.angle,
                }),
                appearance: Deferred::new(ShapeAppearance {
                    fill_color: fill,
                    outline_color: self.default_outline,
                    ..Default::default()
                }),
                radii: Deferred::new([width / 2.0, height / 2.0]),
            })),
        );
        ok_handle(handle)
    }

    // ── SetEnabled ────────────────────────────────────────────────────────────

    fn cmd_set_enabled(&mut self, handle: u32, cmd: proto::SetEnabledRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            Some(stim) => {
                if self.deferred_mode {
                    stim.flags_mut().enabled_copy = cmd.enabled;
                } else {
                    stim.flags_mut().enabled = cmd.enabled;
                    stim.flags_mut().mark_dirty();
                }
                ok_ack()
            }
            None => err_not_found(handle),
        }
    }

    // ── Delete ────────────────────────────────────────────────────────────────

    fn cmd_delete(&mut self, handle: u32) -> proto::Response {
        match self.stimuli.shift_remove(&handle) {
            Some(_) => ok_ack(),
            None => err_not_found(handle),
        }
    }

    // ── SetPosition ───────────────────────────────────────────────────────────

    fn cmd_set_position(&mut self, handle: u32, cmd: proto::SetPositionRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            Some(stim) => {
                stim.move_to(self.deferred_mode, cmd.x, cmd.y);
                ok_ack()
            }
            None => err_not_found(handle),
        }
    }

    // ── SetOrientation ────────────────────────────────────────────────────────

    fn cmd_set_orientation(&mut self, handle: u32, cmd: proto::SetOrientationRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            Some(stim) => {
                stim.set_angle(self.deferred_mode, cmd.angle_deg);
                ok_ack()
            }
            None => err_not_found(handle),
        }
    }

    // ── SetFillColor ──────────────────────────────────────────────────────────

    fn cmd_set_fill_color(&mut self, handle: u32, cmd: proto::SetFillColorRequest) -> proto::Response {
        let c = match cmd.color {
            Some(c) => [c.r, c.g, c.b, c.a],
            None => {
                return err(proto::ErrorCode::InvalidArgument, "fill color must be set");
            }
        };
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Shape(s)) => {
                let deferred = self.deferred_mode;
                let app = s.appearance_mut();
                let prev = if deferred { app.copy } else { app.live };
                app.set(deferred, ShapeAppearance { fill_color: c, ..prev });
                if !deferred {
                    s.flags_mut().mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err(
                proto::ErrorCode::WrongStimulusType,
                format!("SetFillColor is not supported for {} stimuli", stim.type_name()),
            ),
        }
    }

    // ── SetAlpha ──────────────────────────────────────────────────────────────

    fn cmd_set_alpha(&mut self, handle: u32, cmd: proto::SetAlphaRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Shape(s)) => {
                let deferred = self.deferred_mode;
                let app = s.appearance_mut();
                let mut prev = if deferred { app.copy } else { app.live };
                prev.fill_color[3] = cmd.opacity;
                app.set(deferred, prev);
                if !deferred {
                    s.flags_mut().mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err(
                proto::ErrorCode::WrongStimulusType,
                format!("SetAlpha is not supported for {} stimuli", stim.type_name()),
            ),
        }
    }

    // ── SetRectSize ───────────────────────────────────────────────────────────

    fn cmd_set_rect_size(&mut self, handle: u32, cmd: proto::SetRectSizeRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Shape(ShapeStimulus::Rect(s))) => {
                s.size.set(self.deferred_mode, [cmd.width / 2.0, cmd.height / 2.0]);
                if !self.deferred_mode {
                    s.flags.mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err_wrong_type(stim, "SetRectSize", "Rect"),
        }
    }

    // ── SetDiscRadius ─────────────────────────────────────────────────────────

    fn cmd_set_disc_radius(&mut self, handle: u32, cmd: proto::SetDiscRadiusRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Shape(ShapeStimulus::Disc(s))) => {
                s.radius.set(self.deferred_mode, cmd.radius);
                if !self.deferred_mode {
                    s.flags.mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err_wrong_type(stim, "SetDiscRadius", "Disc"),
        }
    }

    // ── SetEllipseSize ────────────────────────────────────────────────────────

    fn cmd_set_ellipse_size(
        &mut self,
        handle: u32,
        cmd: proto::SetEllipseSizeRequest,
    ) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Shape(ShapeStimulus::Ellipse(s))) => {
                s.radii.set(self.deferred_mode, [cmd.width / 2.0, cmd.height / 2.0]);
                if !self.deferred_mode {
                    s.flags.mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err_wrong_type(stim, "SetEllipseSize", "Ellipse"),
        }
    }

    // ── SetDrawMode ───────────────────────────────────────────────────────────

    fn cmd_set_draw_mode(&mut self, handle: u32, cmd: proto::SetDrawModeRequest) -> proto::Response {
        let mode = match proto_draw_mode_to_scene(cmd.mode) {
            Ok(m) => m,
            Err(e) => return *e,
        };
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Shape(s)) => {
                let deferred = self.deferred_mode;
                let app = s.appearance_mut();
                let prev = if deferred { app.copy } else { app.live };
                app.set(deferred, ShapeAppearance { draw_mode: mode, ..prev });
                if !deferred {
                    s.flags_mut().mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err(
                proto::ErrorCode::WrongStimulusType,
                format!("SetDrawMode is not supported for {} stimuli", stim.type_name()),
            ),
        }
    }

    // ── SetOutlineColor ───────────────────────────────────────────────────────

    fn cmd_set_outline_color(
        &mut self,
        handle: u32,
        cmd: proto::SetOutlineColorRequest,
    ) -> proto::Response {
        let c = match cmd.color {
            Some(c) => [c.r, c.g, c.b, c.a],
            None => {
                return err(proto::ErrorCode::InvalidArgument, "outline color must be set");
            }
        };
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Shape(s)) => {
                let deferred = self.deferred_mode;
                let app = s.appearance_mut();
                let prev = if deferred { app.copy } else { app.live };
                app.set(deferred, ShapeAppearance { outline_color: c, ..prev });
                if !deferred {
                    s.flags_mut().mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err(
                proto::ErrorCode::WrongStimulusType,
                format!("SetOutlineColor is not supported for {} stimuli", stim.type_name()),
            ),
        }
    }

    // ── SetOutlineWidth ───────────────────────────────────────────────────────

    fn cmd_set_outline_width(
        &mut self,
        handle: u32,
        cmd: proto::SetOutlineWidthRequest,
    ) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Shape(s)) => {
                let deferred = self.deferred_mode;
                let app = s.appearance_mut();
                let prev = if deferred { app.copy } else { app.live };
                app.set(deferred, ShapeAppearance { stroke_width: cmd.line_width, ..prev });
                if !deferred {
                    s.flags_mut().mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err(
                proto::ErrorCode::WrongStimulusType,
                format!("SetOutlineWidth is not supported for {} stimuli", stim.type_name()),
            ),
        }
    }

    // ── CreateGrating ────────────────────────────────────────────────────────

    fn cmd_create_grating(&mut self, cmd: proto::CreateGratingRequest) -> proto::Response {
        let center = cmd.center.unwrap_or_default();
        let width  = if cmd.width  == 0.0 { 200.0 } else { cmd.width };
        let height = if cmd.height == 0.0 { 200.0 } else { cmd.height };
        let handle = self.alloc_stim_handle();
        self.stimuli.insert(
            handle,
            Stimulus::Grating(GratingStimulus::new(
                [center.x, center.y],
                cmd.angle,
                [width / 2.0, height / 2.0],
                grating_params_from_proto(&cmd),
            )),
        );
        ok_handle(handle)
    }

    // ── Grating setters ───────────────────────────────────────────────────────

    fn cmd_set_grating_phase(&mut self, handle: u32, cmd: proto::SetGratingPhaseRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => { s.set_phase(self.deferred_mode, cmd.phase); ok_ack() }
            Some(stim) => err_wrong_type(stim, "SetGratingPhase", "Grating"),
        }
    }

    fn cmd_set_grating_sf(&mut self, handle: u32, cmd: proto::SetGratingSfRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => { s.set_sf(self.deferred_mode, cmd.sf); ok_ack() }
            Some(stim) => err_wrong_type(stim, "SetGratingSf", "Grating"),
        }
    }

    fn cmd_set_grating_contrast(&mut self, handle: u32, cmd: proto::SetGratingContrastRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => { s.set_contrast(self.deferred_mode, cmd.contrast); ok_ack() }
            Some(stim) => err_wrong_type(stim, "SetGratingContrast", "Grating"),
        }
    }

    fn cmd_set_grating_waveform(&mut self, handle: u32, cmd: proto::SetGratingWaveformRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => { s.set_waveform(self.deferred_mode, proto_to_waveform(cmd.waveform)); ok_ack() }
            Some(stim) => err_wrong_type(stim, "SetGratingWaveform", "Grating"),
        }
    }

    fn cmd_set_grating_mask(&mut self, handle: u32, cmd: proto::SetGratingMaskRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => { s.set_mask(self.deferred_mode, proto_to_mask(cmd.mask)); ok_ack() }
            Some(stim) => err_wrong_type(stim, "SetGratingMask", "Grating"),
        }
    }

    fn cmd_set_grating_drift_speed(&mut self, handle: u32, cmd: proto::SetGratingDriftSpeedRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => { s.set_drift_speed(self.deferred_mode, cmd.speed); ok_ack() }
            Some(stim) => err_wrong_type(stim, "SetGratingDriftSpeed", "Grating"),
        }
    }

    fn cmd_set_grating_drift_decoupled(&mut self, handle: u32, cmd: proto::SetGratingDriftDecoupledRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => { s.set_drift_decoupled(self.deferred_mode, cmd.decoupled); ok_ack() }
            Some(stim) => err_wrong_type(stim, "SetGratingDriftDecoupled", "Grating"),
        }
    }

    fn cmd_set_grating_drift_angle(&mut self, handle: u32, cmd: proto::SetGratingDriftAngleRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => { s.set_drift_angle(self.deferred_mode, cmd.angle_deg); ok_ack() }
            Some(stim) => err_wrong_type(stim, "SetGratingDriftAngle", "Grating"),
        }
    }

    fn cmd_set_grating_fore_color(&mut self, handle: u32, cmd: proto::SetGratingForeColorRequest) -> proto::Response {
        let c = match cmd.fore_color {
            Some(c) => [c.r, c.g, c.b, c.a],
            None => return err(proto::ErrorCode::InvalidArgument, "fore_color must be set"),
        };
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => { s.set_fore_color(self.deferred_mode, c); ok_ack() }
            Some(stim) => err_wrong_type(stim, "SetGratingForeColor", "Grating"),
        }
    }

    fn cmd_set_grating_back_color(&mut self, handle: u32, cmd: proto::SetGratingBackColorRequest) -> proto::Response {
        let c = match cmd.back_color {
            Some(c) => [c.r, c.g, c.b, c.a],
            None => return err(proto::ErrorCode::InvalidArgument, "back_color must be set"),
        };
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => { s.set_back_color(self.deferred_mode, c); ok_ack() }
            Some(stim) => err_wrong_type(stim, "SetGratingBackColor", "Grating"),
        }
    }

    fn cmd_set_grating_opacity(&mut self, handle: u32, cmd: proto::SetGratingOpacityRequest) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => { s.set_opacity(self.deferred_mode, cmd.opacity); ok_ack() }
            Some(stim) => err_wrong_type(stim, "SetGratingOpacity", "Grating"),
        }
    }

    // ── SetBackground ─────────────────────────────────────────────────────────

    fn cmd_set_background(&mut self, cmd: proto::SetBackgroundRequest) -> proto::Response {
        let c = match cmd.color {
            Some(c) => [c.r, c.g, c.b, c.a],
            None => {
                return err(proto::ErrorCode::InvalidArgument, "background color must be set");
            }
        };
        self.background.set(self.deferred_mode, c);
        ok_ack()
    }

    // ── SetDeferredMode ───────────────────────────────────────────────────────

    fn cmd_set_deferred_mode(&mut self, cmd: proto::SetDeferredModeRequest) -> proto::Response {
        if cmd.active {
            self.begin_deferred();
        } else if cmd.cancel {
            self.deferred_mode = false;
        } else {
            self.end_deferred();
        }
        ok_ack()
    }

    // ── DeleteAll ─────────────────────────────────────────────────────────────

    fn cmd_delete_all(&mut self) -> proto::Response {
        self.clear_all(false);
        ok_ack()
    }

    // ── SetAllEnabled ─────────────────────────────────────────────────────────

    fn cmd_set_all_enabled(&mut self, cmd: proto::SetAllEnabledRequest) -> proto::Response {
        self.set_all_enabled(cmd.enabled, false);
        ok_ack()
    }

    // ── QueryServerInfo ───────────────────────────────────────────────────────

    fn cmd_query_server_info(&self) -> proto::Response {
        let Some((w, h)) = self.screen_size else {
            return err(proto::ErrorCode::NotReady, "display not yet initialised");
        };
        let bg = self.background.live;
        let version = parse_cargo_version();
        ok_body(proto::response::Body::ServerInfo(proto::QueryServerInfoResponse {
            width: w,
            height: h,
            frame_rate: self.frame_rate,
            background_color: Some(proto::Color { r: bg[0], g: bg[1], b: bg[2], a: bg[3] }),
            backend: proto::Backend::Unspecified as i32,
            version: Some(version),
        }))
    }

    // ── QueryStimulus ─────────────────────────────────────────────────────────

    fn cmd_query_stimulus(&self, handle: u32) -> proto::Response {
        let stim = match self.stimuli.get(&handle) {
            Some(s) => s,
            None => return err_not_found(handle),
        };

        let pos = stim.get_pos();
        let angle = stim.transform().live.angle;

        let (stimulus_type, params, fill_color, outline_color, outline_width, draw_mode, opacity) =
            match stim {
                Stimulus::Shape(s) => {
                    let a = s.appearance().live;
                    let (st, p) = match s {
                        ShapeStimulus::Rect(r) => (
                            proto::StimulusType::Rect as i32,
                            Some(proto::StimulusParams {
                                shape: Some(proto::stimulus_params::Shape::Rect(proto::RectParams {
                                    width: r.size.live[0] * 2.0,
                                    height: r.size.live[1] * 2.0,
                                })),
                            }),
                        ),
                        ShapeStimulus::Disc(d) => (
                            proto::StimulusType::Disc as i32,
                            Some(proto::StimulusParams {
                                shape: Some(proto::stimulus_params::Shape::Disc(proto::DiscParams {
                                    radius: d.radius.live,
                                })),
                            }),
                        ),
                        ShapeStimulus::Ellipse(e) => (
                            proto::StimulusType::Ellipse as i32,
                            Some(proto::StimulusParams {
                                shape: Some(proto::stimulus_params::Shape::Ellipse(proto::EllipseParams {
                                    width: e.radii.live[0] * 2.0,
                                    height: e.radii.live[1] * 2.0,
                                })),
                            }),
                        ),
                    };
                    (
                        st, p,
                        Some(proto::Color { r: a.fill_color[0], g: a.fill_color[1], b: a.fill_color[2], a: a.fill_color[3] }),
                        Some(proto::Color { r: a.outline_color[0], g: a.outline_color[1], b: a.outline_color[2], a: a.outline_color[3] }),
                        a.stroke_width,
                        scene_draw_mode_to_proto(a.draw_mode),
                        a.fill_color[3],
                    )
                }
                Stimulus::Grating(s) => {
                    let fc = s.params.live.fore_color;
                    let op = s.params.live.opacity;
                    (
                        proto::StimulusType::Grating as i32,
                        Some(grating_query_params(s)),
                        Some(proto::Color { r: fc[0], g: fc[1], b: fc[2], a: op }),
                        None,
                        0.0,
                        proto::DrawMode::Filled as i32,
                        op,
                    )
                }
            };

        ok_body(proto::response::Body::StimulusInfo(proto::QueryStimulusResponse {
            stimulus_type,
            enabled: stim.flags().enabled,
            pos: Some(proto::Vec2 { x: pos[0], y: pos[1] }),
            orientation: angle,
            opacity,
            fill_color,
            outline_color,
            outline_width,
            draw_mode,
            params,
        }))
    }

    // ── ListStimuli ───────────────────────────────────────────────────────────

    fn cmd_list_stimuli(&self) -> proto::Response {
        let entries: Vec<proto::StimulusEntry> = self
            .stimuli
            .iter()
            .map(|(&handle, stim)| {
                let stimulus_type = match stim {
                    Stimulus::Shape(ShapeStimulus::Rect(_))    => proto::StimulusType::Rect,
                    Stimulus::Shape(ShapeStimulus::Ellipse(_)) => proto::StimulusType::Ellipse,
                    Stimulus::Shape(ShapeStimulus::Disc(_))    => proto::StimulusType::Disc,
                    Stimulus::Grating(_)                       => proto::StimulusType::Grating,
                } as i32;
                proto::StimulusEntry { handle, stimulus_type, enabled: stim.flags().enabled }
            })
            .collect();
        ok_body(proto::response::Body::StimulusList(proto::ListStimuliResponse { entries }))
    }
}

// ── Module-private helpers ────────────────────────────────────────────────────

fn color_or_default(c: Option<proto::Color>, default: [f32; 4]) -> [f32; 4] {
    c.map(|c| [c.r, c.g, c.b, c.a]).unwrap_or(default)
}

fn parse_cargo_version() -> proto::Version {
    let s = env!("CARGO_PKG_VERSION");
    let mut parts = s.splitn(3, '.').map(|p| p.parse::<u32>().unwrap_or(0));
    proto::Version {
        major: parts.next().unwrap_or(0),
        minor: parts.next().unwrap_or(0),
        patch: parts.next().unwrap_or(0),
    }
}
