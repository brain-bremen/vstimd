use uuid::Uuid;

use super::deferred::Deferred;
use super::state::SceneState;
use super::stimulus::{CircleStimulus, EllipseStimulus, RectStimulus, ShapeStimulus, Stimulus, StimulusEntry};
use super::stimulus::{DrawMode as SceneDrawMode, ShapeAppearance, StimulusFlags, Transform2D};
use super::stimulus::grating::{
    GratingStimulus, grating_params_from_proto, grating_query_params, proto_to_mask,
    proto_to_waveform,
};
use super::stimulus::text::{
    TextStimulus, anchor_from_str, proto_to_language_style, text_query_params,
    text_render_params_from_proto,
};
use crate::ipc::{err, err_not_found, err_wrong_type, ok_ack, ok_body, ok_handle, ok_handle_with_id};
use super::animation::{AnimState, Animation, AnimationEntry, Edge, FinalAction, StartAction, VtlBit};
use crate::proto;
use crate::proto::request;
use crate::Color;
use crate::vtl_state::{VtlConfig, VtlNameEntry, VtlState};
use crate::io_config::{
    is_format_error, is_not_found, list_config_names, load_config,
    parse_config_json, retrieve_config_json,
};

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
        Some(request::Body::SetCircleRadius(c)) => format!("SetCircleRadius({:.0})", c.radius),
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
        Some(request::Body::CreateText(c)) => {
            let pos = c.pos.as_ref();
            format!(
                "CreateText {:?} pos=({:.1},{:.1})",
                c.text,
                pos.map_or(0.0, |v| v.x),
                pos.map_or(0.0, |v| v.y),
            )
        }
        Some(request::Body::SetText(c)) => format!("SetText({:?})", c.text),
        Some(request::Body::SetTextColor(_)) => "SetTextColor".into(),
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
        Some(request::Body::SetName(c)) => format!("SetName({:?})", c.name),
        Some(request::Body::CreatePolygon(_)) => "CreatePolygon".into(),
        Some(request::Body::SetPolygonVertices(_)) => "SetPolygonVertices".into(),
        Some(request::Body::SetVirtualTriggerLineName(c)) => format!("SetVirtualTriggerLineName({:?})", c.name),
        Some(request::Body::ListVirtualTriggerLines(_)) => "ListVirtualTriggerLines".into(),
        Some(request::Body::SetInputVirtualTriggerLine(c)) => format!("SetInputVirtualTriggerLine(val={})", c.value),
        Some(request::Body::ToggleInputVirtualTriggerLine(_)) => "ToggleInputVirtualTriggerLine".into(),
        Some(request::Body::ClearInputVirtualTriggerLineLatches(_)) => "ClearInputVirtualTriggerLineLatches".into(),
        Some(request::Body::SetInputVirtualTriggerLineBank(c)) => format!("SetInputVirtualTriggerLineBank(bank={} val={:#018x})", c.bank, c.value),
        Some(request::Body::SetOutputVirtualTriggerLine(c)) => format!("SetOutputVirtualTriggerLine(val={})", c.value),
        Some(request::Body::ToggleOutputVirtualTriggerLine(_)) => "ToggleOutputVirtualTriggerLine".into(),
        Some(request::Body::SetOutputVirtualTriggerLineBank(c)) => format!("SetOutputVirtualTriggerLineBank(bank={} val={:#018x})", c.bank, c.value),
        Some(request::Body::BringToFront(_)) => "BringToFront".into(),
        Some(request::Body::SendToBack(_)) => "SendToBack".into(),
        Some(request::Body::SwapDrawOrder(c)) => format!("SwapDrawOrder({}, {})", c.handle_a, c.handle_b),
        Some(request::Body::CreateAnimation(c)) => format!("CreateAnimation({:?})", c.name),
        Some(request::Body::ArmAnimation(c)) => format!("ArmAnimation({})", c.handle),
        Some(request::Body::DisarmAnimation(c)) => format!("DisarmAnimation({})", c.handle),
        Some(request::Body::DeleteAnimation(c)) => format!("DeleteAnimation({})", c.handle),
        Some(request::Body::ListAnimations(_)) => "ListAnimations".into(),
        Some(request::Body::QueryAnimation(c)) => format!("QueryAnimation({})", c.handle),
        Some(request::Body::WaitForFrames(c)) => format!("WaitForFrames({})", c.count),
        Some(request::Body::WaitUntil(c)) => format!("WaitUntil({}ns)", c.server_time_ns),
        Some(request::Body::ListConfigs(_)) => "ListConfigs".into(),
        Some(request::Body::LoadConfig(c)) => format!("LoadConfig({:?})", c.name),
        Some(request::Body::UploadConfig(c)) => format!("UploadConfig({:?})", c.name),
        Some(request::Body::RetrieveConfig(_)) => "RetrieveConfig".into(),
        Some(request::Body::Shutdown(_)) => "Shutdown".into(),
        None => "?".into(),
    }
}

// ── DrawMode conversion ───────────────────────────────────────────────────────

fn proto_draw_mode_to_scene(mode: i32) -> Result<SceneDrawMode, Box<proto::Response>> {
    match proto::ShapeDrawMode::try_from(mode).unwrap_or(proto::ShapeDrawMode::Unspecified) {
        proto::ShapeDrawMode::Unspecified => Ok(SceneDrawMode::Fill),
        proto::ShapeDrawMode::Filled => Ok(SceneDrawMode::Fill),
        proto::ShapeDrawMode::Outlined => Ok(SceneDrawMode::Stroke),
        proto::ShapeDrawMode::FilledAndOutlined => Ok(SceneDrawMode::FillAndStroke),
    }
}

fn scene_draw_mode_to_proto(mode: SceneDrawMode) -> i32 {
    match mode {
        SceneDrawMode::Fill => proto::ShapeDrawMode::Filled as i32,
        SceneDrawMode::Stroke => proto::ShapeDrawMode::Outlined as i32,
        SceneDrawMode::FillAndStroke => proto::ShapeDrawMode::FilledAndOutlined as i32,
    }
}

// ── Main dispatcher ───────────────────────────────────────────────────────────

impl SceneState {
    pub fn handle_request(&mut self, req: proto::Request, vtl: Option<&mut VtlState>) -> proto::Response {
        let log_handle = match &req.target {
            Some(request::Target::Stimulus(h)) => *h,
            _ => 0,
        };
        let log_summary = command_summary(&req);

        let response = match req.body {
            None => err(proto::ErrorCode::InvalidArgument, "empty request body"),
            Some(body) => match req.target {
                Some(request::Target::System(_)) | None => self.handle_system_command(body, vtl),
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

    fn handle_system_command(&mut self, body: request::Body, vtl: Option<&mut VtlState>) -> proto::Response {
        match body {
            request::Body::CreateRect(cmd) => self.cmd_create_rect(cmd),
            request::Body::CreateCircle(cmd) => self.cmd_create_circle(cmd),
            request::Body::CreateEllipse(cmd) => self.cmd_create_ellipse(cmd),
            request::Body::CreateGrating(cmd) => self.cmd_create_grating(cmd),
            request::Body::CreateText(cmd) => self.cmd_create_text(cmd),
            request::Body::CreatePolygon(_) => err(
                proto::ErrorCode::NotSupported,
                "CreatePolygon is not yet implemented",
            ),
            request::Body::SetBackground(cmd) => self.cmd_set_background(cmd),
            request::Body::SetDeferredMode(cmd) => self.cmd_set_deferred_mode(cmd),
            request::Body::DeleteAll(_) => self.cmd_delete_all(),
            request::Body::SetAllEnabled(cmd) => self.cmd_set_all_enabled(cmd),
            request::Body::QueryServerInfo(_) => self.cmd_query_server_info(),
            request::Body::ListStimuli(_) => self.cmd_list_stimuli(),
            request::Body::SetVirtualTriggerLineName(cmd) => self.cmd_set_virtual_trigger_line_name(cmd, vtl),
            request::Body::ListVirtualTriggerLines(_) => self.cmd_list_virtual_trigger_lines(vtl.as_deref()),
            request::Body::SetInputVirtualTriggerLine(cmd) => self.cmd_set_input_virtual_trigger_line(cmd, vtl.as_deref()),
            request::Body::ToggleInputVirtualTriggerLine(cmd) => self.cmd_toggle_input_virtual_trigger_line(cmd, vtl.as_deref()),
            request::Body::ClearInputVirtualTriggerLineLatches(cmd) => self.cmd_clear_input_virtual_trigger_line_latches(cmd, vtl.as_deref()),
            request::Body::SetInputVirtualTriggerLineBank(cmd) => self.cmd_set_input_virtual_trigger_line_bank(cmd, vtl.as_deref()),
            request::Body::SetOutputVirtualTriggerLine(cmd) => self.cmd_set_output_virtual_trigger_line(cmd, vtl),
            request::Body::ToggleOutputVirtualTriggerLine(cmd) => self.cmd_toggle_output_virtual_trigger_line(cmd, vtl),
            request::Body::SetOutputVirtualTriggerLineBank(cmd) => self.cmd_set_output_virtual_trigger_line_bank(cmd, vtl),
            request::Body::SwapDrawOrder(_) => err(proto::ErrorCode::NotSupported, "SwapDrawOrder not yet implemented"),
            request::Body::CreateAnimation(cmd) => self.cmd_create_animation(cmd, vtl.as_deref()),
            request::Body::ArmAnimation(cmd) => self.cmd_arm_animation(cmd),
            request::Body::DisarmAnimation(cmd) => self.cmd_disarm_animation(cmd),
            request::Body::DeleteAnimation(cmd) => self.cmd_delete_animation(cmd),
            request::Body::ListAnimations(_) => self.cmd_list_animations(),
            request::Body::QueryAnimation(cmd) => self.cmd_query_animation(cmd),
            request::Body::ListConfigs(_) => self.cmd_list_configs(),
            request::Body::LoadConfig(cmd) => self.cmd_load_config(cmd, vtl),
            request::Body::UploadConfig(cmd) => self.cmd_upload_config(cmd, vtl),
            request::Body::RetrieveConfig(_) => self.cmd_retrieve_config(vtl.as_deref()),
            request::Body::Shutdown(_) => {
                crate::shutdown::request();
                ok_ack()
            }
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
            | request::Body::CreateText(_)
            | request::Body::CreatePolygon(_)
            | request::Body::SetBackground(_)
            | request::Body::SetDeferredMode(_)
            | request::Body::DeleteAll(_)
            | request::Body::SetAllEnabled(_)
            | request::Body::QueryServerInfo(_)
            | request::Body::ListStimuli(_)
            | request::Body::SetVirtualTriggerLineName(_)
            | request::Body::ListVirtualTriggerLines(_)
            | request::Body::SetInputVirtualTriggerLine(_)
            | request::Body::ToggleInputVirtualTriggerLine(_)
            | request::Body::ClearInputVirtualTriggerLineLatches(_)
            | request::Body::SetInputVirtualTriggerLineBank(_)
            | request::Body::SetOutputVirtualTriggerLine(_)
            | request::Body::ToggleOutputVirtualTriggerLine(_)
            | request::Body::SetOutputVirtualTriggerLineBank(_)
            | request::Body::SwapDrawOrder(_)
            | request::Body::CreateAnimation(_)
            | request::Body::ArmAnimation(_)
            | request::Body::DisarmAnimation(_)
            | request::Body::DeleteAnimation(_)
            | request::Body::ListAnimations(_)
            | request::Body::QueryAnimation(_)
            | request::Body::WaitForFrames(_)
            | request::Body::WaitUntil(_)
            | request::Body::ListConfigs(_)
            | request::Body::LoadConfig(_)
            | request::Body::UploadConfig(_)
            | request::Body::RetrieveConfig(_)
            | request::Body::Shutdown(_) => err(
                proto::ErrorCode::WrongTarget,
                "system command must use target.system (not a stimulus handle)",
            ),
            request::Body::SetEnabled(cmd) => self.cmd_set_enabled(handle, cmd),
            request::Body::Delete(_) => self.cmd_delete(handle),
            request::Body::SetName(cmd) => self.cmd_set_name(handle, cmd),
            request::Body::SetPosition(cmd) => self.cmd_set_position(handle, cmd),
            request::Body::SetOrientation(cmd) => self.cmd_set_orientation(handle, cmd),
            request::Body::SetFillColor(cmd) => self.cmd_set_fill_color(handle, cmd),
            request::Body::SetAlpha(cmd) => self.cmd_set_alpha(handle, cmd),
            request::Body::SetRectSize(cmd) => self.cmd_set_rect_size(handle, cmd),
            request::Body::SetCircleRadius(cmd) => self.cmd_set_circle_radius(handle, cmd),
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
            request::Body::SetText(cmd) => self.cmd_set_text(handle, cmd),
            request::Body::SetTextColor(cmd) => self.cmd_set_text_color(handle, cmd),
            request::Body::SetPolygonVertices(_) => err(
                proto::ErrorCode::NotSupported,
                "SetPolygonVertices is not yet implemented",
            ),
            request::Body::BringToFront(_) => err(
                proto::ErrorCode::NotSupported,
                "BringToFront not yet implemented",
            ),
            request::Body::SendToBack(_) => err(
                proto::ErrorCode::NotSupported,
                "SendToBack not yet implemented",
            ),
            request::Body::QueryStimulus(_) => self.cmd_query_stimulus(handle),
        }
    }

    // ── CreateRect ────────────────────────────────────────────────────────────

    fn cmd_create_rect(&mut self, cmd: proto::CreateRectRequest) -> proto::Response {
        let id = match parse_or_new_uuid(&cmd.id) {
            Ok(id) => id,
            Err(resp) => return *resp,
        };
        let center = cmd.center.unwrap_or_default();
        let width  = if cmd.width  == 0.0 { 100.0 } else { cmd.width  };
        let height = if cmd.height == 0.0 { 100.0 } else { cmd.height };
        let fill   = color_or_default(cmd.fill_color, self.config.default_fill);
        let entry  = StimulusEntry::new(id, nonempty(cmd.name), Stimulus::Shape(ShapeStimulus::Rect(RectStimulus {
            flags: StimulusFlags::enabled(true),
            transform:  Deferred::new(Transform2D { pos: [center.x, center.y], angle: 0.0 }),
            appearance: Deferred::new(ShapeAppearance {
                fill_color:    fill,
                outline_color: self.config.default_outline,
                ..Default::default()
            }),
            size: Deferred::new([width / 2.0, height / 2.0]),
        })));
        let handle = self.add_stimulus(entry);
        ok_handle_with_id(handle, &id)
    }

    // ── CreateCircle ──────────────────────────────────────────────────────────

    fn cmd_create_circle(&mut self, cmd: proto::CreateCircleRequest) -> proto::Response {
        let id = match parse_or_new_uuid(&cmd.id) {
            Ok(id) => id,
            Err(resp) => return *resp,
        };
        let center = cmd.center.unwrap_or_default();
        let radius = if cmd.radius == 0.0 { 50.0 } else { cmd.radius };
        let fill   = color_or_default(cmd.fill_color, self.config.default_fill);
        let entry  = StimulusEntry::new(id, nonempty(cmd.name), Stimulus::Shape(ShapeStimulus::Circle(CircleStimulus {
            flags: StimulusFlags::enabled(true),
            transform:  Deferred::new(Transform2D { pos: [center.x, center.y], angle: 0.0 }),
            appearance: Deferred::new(ShapeAppearance {
                fill_color:    fill,
                outline_color: self.config.default_outline,
                ..Default::default()
            }),
            radius: Deferred::new(radius),
        })));
        let handle = self.add_stimulus(entry);
        ok_handle_with_id(handle, &id)
    }

    // ── CreateEllipse ─────────────────────────────────────────────────────────

    fn cmd_create_ellipse(&mut self, cmd: proto::CreateEllipseRequest) -> proto::Response {
        let id = match parse_or_new_uuid(&cmd.id) {
            Ok(id) => id,
            Err(resp) => return *resp,
        };
        let center = cmd.center.unwrap_or_default();
        let width  = if cmd.width  == 0.0 { 100.0 } else { cmd.width  };
        let height = if cmd.height == 0.0 { 100.0 } else { cmd.height };
        let fill   = color_or_default(cmd.fill_color, self.config.default_fill);
        let entry  = StimulusEntry::new(id, nonempty(cmd.name), Stimulus::Shape(ShapeStimulus::Ellipse(EllipseStimulus {
            flags: StimulusFlags::enabled(true),
            transform:  Deferred::new(Transform2D { pos: [center.x, center.y], angle: cmd.angle }),
            appearance: Deferred::new(ShapeAppearance {
                fill_color:    fill,
                outline_color: self.config.default_outline,
                ..Default::default()
            }),
            radii: Deferred::new([width / 2.0, height / 2.0]),
        })));
        let handle = self.add_stimulus(entry);
        ok_handle_with_id(handle, &id)
    }

    // ── SetEnabled ────────────────────────────────────────────────────────────

    fn cmd_set_enabled(&mut self, handle: u32, cmd: proto::SetEnabledRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            Some(entry) => {
                if self.runtime.deferred_mode {
                    entry.stimulus.flags_mut().enabled_copy = cmd.enabled;
                } else {
                    entry.stimulus.flags_mut().enabled = cmd.enabled;
                    entry.stimulus.flags_mut().mark_dirty();
                }
                ok_ack()
            }
            None => err_not_found(handle),
        }
    }

    // ── Delete ────────────────────────────────────────────────────────────────

    fn cmd_delete(&mut self, handle: u32) -> proto::Response {
        match self.config.stimuli.shift_remove(&handle) {
            Some(_) => ok_ack(),
            None => err_not_found(handle),
        }
    }

    // ── SetPosition ───────────────────────────────────────────────────────────

    fn cmd_set_position(&mut self, handle: u32, cmd: proto::SetPositionRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            Some(entry) => {
                entry.stimulus.move_to(self.runtime.deferred_mode, cmd.x, cmd.y);
                ok_ack()
            }
            None => err_not_found(handle),
        }
    }

    // ── SetOrientation ────────────────────────────────────────────────────────

    fn cmd_set_orientation(&mut self, handle: u32, cmd: proto::SetOrientationRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            Some(entry) => {
                entry.stimulus.set_angle(self.runtime.deferred_mode, cmd.angle_deg);
                ok_ack()
            }
            None => err_not_found(handle),
        }
    }

    // ── SetFillColor ──────────────────────────────────────────────────────────

    fn cmd_set_fill_color(&mut self, handle: u32, cmd: proto::SetFillColorRequest) -> proto::Response {
        let c = match cmd.color {
            Some(c) => c.into(),
            None => return err(proto::ErrorCode::InvalidArgument, "fill color must be set"),
        };
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Shape(s) => {
                    let deferred = self.runtime.deferred_mode;
                    let app = s.appearance_mut();
                    let prev = if deferred { app.copy } else { app.live };
                    app.set(deferred, ShapeAppearance { fill_color: c, ..prev });
                    if !deferred { s.flags_mut().mark_dirty(); }
                    ok_ack()
                }
                stim => err(proto::ErrorCode::WrongStimulusType,
                    format!("SetFillColor is not supported for {} stimuli", stim.type_name())),
            },
        }
    }

    // ── SetAlpha ──────────────────────────────────────────────────────────────

    fn cmd_set_alpha(&mut self, handle: u32, cmd: proto::SetAlphaRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Shape(s) => {
                    let deferred = self.runtime.deferred_mode;
                    let app = s.appearance_mut();
                    let mut prev = if deferred { app.copy } else { app.live };
                    prev.fill_color.a = cmd.opacity;
                    app.set(deferred, prev);
                    if !deferred { s.flags_mut().mark_dirty(); }
                    ok_ack()
                }
                stim => err(proto::ErrorCode::WrongStimulusType,
                    format!("SetAlpha is not supported for {} stimuli", stim.type_name())),
            },
        }
    }

    // ── SetRectSize ───────────────────────────────────────────────────────────

    fn cmd_set_rect_size(&mut self, handle: u32, cmd: proto::SetRectSizeRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Shape(ShapeStimulus::Rect(s)) => {
                    s.size.set(self.runtime.deferred_mode, [cmd.width / 2.0, cmd.height / 2.0]);
                    if !self.runtime.deferred_mode { s.flags.mark_dirty(); }
                    ok_ack()
                }
                stim => err_wrong_type(stim, "SetRectSize", "Rect"),
            },
        }
    }

    // ── SetCircleRadius ───────────────────────────────────────────────────────

    fn cmd_set_circle_radius(&mut self, handle: u32, cmd: proto::SetCircleRadiusRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Shape(ShapeStimulus::Circle(s)) => {
                    s.radius.set(self.runtime.deferred_mode, cmd.radius);
                    if !self.runtime.deferred_mode { s.flags.mark_dirty(); }
                    ok_ack()
                }
                stim => err_wrong_type(stim, "SetCircleRadius", "Circle"),
            },
        }
    }

    // ── SetEllipseSize ────────────────────────────────────────────────────────

    fn cmd_set_ellipse_size(
        &mut self,
        handle: u32,
        cmd: proto::SetEllipseSizeRequest,
    ) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Shape(ShapeStimulus::Ellipse(s)) => {
                    s.radii.set(self.runtime.deferred_mode, [cmd.width / 2.0, cmd.height / 2.0]);
                    if !self.runtime.deferred_mode { s.flags.mark_dirty(); }
                    ok_ack()
                }
                stim => err_wrong_type(stim, "SetEllipseSize", "Ellipse"),
            },
        }
    }

    // ── SetDrawMode ───────────────────────────────────────────────────────────

    fn cmd_set_draw_mode(&mut self, handle: u32, cmd: proto::SetDrawModeRequest) -> proto::Response {
        let mode = match proto_draw_mode_to_scene(cmd.mode) {
            Ok(m) => m,
            Err(e) => return *e,
        };
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Shape(s) => {
                    let deferred = self.runtime.deferred_mode;
                    let app = s.appearance_mut();
                    let prev = if deferred { app.copy } else { app.live };
                    app.set(deferred, ShapeAppearance { draw_mode: mode, ..prev });
                    if !deferred { s.flags_mut().mark_dirty(); }
                    ok_ack()
                }
                stim => err(proto::ErrorCode::WrongStimulusType,
                    format!("SetDrawMode is not supported for {} stimuli", stim.type_name())),
            },
        }
    }

    // ── SetOutlineColor ───────────────────────────────────────────────────────

    fn cmd_set_outline_color(
        &mut self,
        handle: u32,
        cmd: proto::SetOutlineColorRequest,
    ) -> proto::Response {
        let c = match cmd.color {
            Some(c) => c.into(),
            None => return err(proto::ErrorCode::InvalidArgument, "outline color must be set"),
        };
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Shape(s) => {
                    let deferred = self.runtime.deferred_mode;
                    let app = s.appearance_mut();
                    let prev = if deferred { app.copy } else { app.live };
                    app.set(deferred, ShapeAppearance { outline_color: c, ..prev });
                    if !deferred { s.flags_mut().mark_dirty(); }
                    ok_ack()
                }
                stim => err(proto::ErrorCode::WrongStimulusType,
                    format!("SetOutlineColor is not supported for {} stimuli", stim.type_name())),
            },
        }
    }

    // ── SetOutlineWidth ───────────────────────────────────────────────────────

    fn cmd_set_outline_width(
        &mut self,
        handle: u32,
        cmd: proto::SetOutlineWidthRequest,
    ) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Shape(s) => {
                    let deferred = self.runtime.deferred_mode;
                    let app = s.appearance_mut();
                    let prev = if deferred { app.copy } else { app.live };
                    app.set(deferred, ShapeAppearance { stroke_width: cmd.line_width, ..prev });
                    if !deferred { s.flags_mut().mark_dirty(); }
                    ok_ack()
                }
                stim => err(proto::ErrorCode::WrongStimulusType,
                    format!("SetOutlineWidth is not supported for {} stimuli", stim.type_name())),
            },
        }
    }

    // ── CreateGrating ────────────────────────────────────────────────────────

    fn cmd_create_grating(&mut self, cmd: proto::CreateGratingRequest) -> proto::Response {
        // Borrow cmd fully before any partial moves.
        let id = match parse_or_new_uuid(&cmd.id) {
            Ok(id) => id,
            Err(resp) => return *resp,
        };
        let params = grating_params_from_proto(&cmd);
        let center = cmd.center.unwrap_or_default();
        let width  = if cmd.width  == 0.0 { 200.0 } else { cmd.width };
        let height = if cmd.height == 0.0 { 200.0 } else { cmd.height };
        let angle  = cmd.angle;
        let name   = nonempty(cmd.name);
        let handle = self.alloc_stim_handle();
        self.config.stimuli.insert(handle, StimulusEntry::new(id, name,
            Stimulus::Grating(GratingStimulus::new(
                [center.x, center.y],
                angle,
                [width / 2.0, height / 2.0],
                params,
            )),
        ));
        ok_handle_with_id(handle, &id)
    }

    // ── Grating setters ───────────────────────────────────────────────────────

    fn cmd_set_grating_phase(&mut self, handle: u32, cmd: proto::SetGratingPhaseRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Grating(s) => { s.set_phase(self.runtime.deferred_mode, cmd.phase); ok_ack() }
                stim => err_wrong_type(stim, "SetGratingPhase", "Grating"),
            },
        }
    }

    fn cmd_set_grating_sf(&mut self, handle: u32, cmd: proto::SetGratingSfRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Grating(s) => { s.set_sf(self.runtime.deferred_mode, cmd.sf); ok_ack() }
                stim => err_wrong_type(stim, "SetGratingSf", "Grating"),
            },
        }
    }

    fn cmd_set_grating_contrast(&mut self, handle: u32, cmd: proto::SetGratingContrastRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Grating(s) => { s.set_contrast(self.runtime.deferred_mode, cmd.contrast); ok_ack() }
                stim => err_wrong_type(stim, "SetGratingContrast", "Grating"),
            },
        }
    }

    fn cmd_set_grating_waveform(&mut self, handle: u32, cmd: proto::SetGratingWaveformRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Grating(s) => { s.set_waveform(self.runtime.deferred_mode, proto_to_waveform(cmd.waveform)); ok_ack() }
                stim => err_wrong_type(stim, "SetGratingWaveform", "Grating"),
            },
        }
    }

    fn cmd_set_grating_mask(&mut self, handle: u32, cmd: proto::SetGratingMaskRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Grating(s) => { s.set_mask(self.runtime.deferred_mode, proto_to_mask(cmd.mask)); ok_ack() }
                stim => err_wrong_type(stim, "SetGratingMask", "Grating"),
            },
        }
    }

    fn cmd_set_grating_drift_speed(&mut self, handle: u32, cmd: proto::SetGratingDriftSpeedRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Grating(s) => { s.set_drift_speed(self.runtime.deferred_mode, cmd.speed); ok_ack() }
                stim => err_wrong_type(stim, "SetGratingDriftSpeed", "Grating"),
            },
        }
    }

    fn cmd_set_grating_drift_decoupled(&mut self, handle: u32, cmd: proto::SetGratingDriftDecoupledRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Grating(s) => { s.set_drift_decoupled(self.runtime.deferred_mode, cmd.decoupled); ok_ack() }
                stim => err_wrong_type(stim, "SetGratingDriftDecoupled", "Grating"),
            },
        }
    }

    fn cmd_set_grating_drift_angle(&mut self, handle: u32, cmd: proto::SetGratingDriftAngleRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Grating(s) => { s.set_drift_angle(self.runtime.deferred_mode, cmd.angle_deg); ok_ack() }
                stim => err_wrong_type(stim, "SetGratingDriftAngle", "Grating"),
            },
        }
    }

    fn cmd_set_grating_fore_color(&mut self, handle: u32, cmd: proto::SetGratingForeColorRequest) -> proto::Response {
        let c = match cmd.fore_color {
            Some(c) => c.into(),
            None => return err(proto::ErrorCode::InvalidArgument, "fore_color must be set"),
        };
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Grating(s) => { s.set_fore_color(self.runtime.deferred_mode, c); ok_ack() }
                stim => err_wrong_type(stim, "SetGratingForeColor", "Grating"),
            },
        }
    }

    fn cmd_set_grating_back_color(&mut self, handle: u32, cmd: proto::SetGratingBackColorRequest) -> proto::Response {
        let c = match cmd.back_color {
            Some(c) => c.into(),
            None => return err(proto::ErrorCode::InvalidArgument, "back_color must be set"),
        };
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Grating(s) => { s.set_back_color(self.runtime.deferred_mode, c); ok_ack() }
                stim => err_wrong_type(stim, "SetGratingBackColor", "Grating"),
            },
        }
    }

    fn cmd_set_grating_opacity(&mut self, handle: u32, cmd: proto::SetGratingOpacityRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Grating(s) => { s.set_opacity(self.runtime.deferred_mode, cmd.opacity); ok_ack() }
                stim => err_wrong_type(stim, "SetGratingOpacity", "Grating"),
            },
        }
    }

    // ── CreateText ────────────────────────────────────────────────────────────

    fn cmd_create_text(&mut self, cmd: proto::CreateTextRequest) -> proto::Response {
        let id = match parse_or_new_uuid(&cmd.id) {
            Ok(id) => id,
            Err(resp) => return *resp,
        };
        let pos = cmd.pos.unwrap_or_default();
        let size = cmd.size.unwrap_or_default();
        let box_size = [
            if size.x == 0.0 { 200.0 } else { size.x },
            if size.y == 0.0 { 100.0 } else { size.y },
        ];
        let letter_height_px = if cmd.letter_height == 0.0 { 32.0 } else { cmd.letter_height };
        let anchor = anchor_from_str(&cmd.anchor);
        let language_style = proto_to_language_style(cmd.language_style);
        let params = text_render_params_from_proto(&cmd);
        let name = nonempty(cmd.name);
        let handle = self.alloc_stim_handle();
        self.config.stimuli.insert(handle, StimulusEntry::new(id, name,
            Stimulus::Text(TextStimulus::new(
                [pos.x, pos.y],
                box_size,
                cmd.text,
                cmd.font,
                letter_height_px,
                anchor,
                language_style,
                params,
            )),
        ));
        ok_handle_with_id(handle, &id)
    }

    // ── SetText ───────────────────────────────────────────────────────────────

    fn cmd_set_text(&mut self, handle: u32, cmd: proto::SetTextRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Text(s) => {
                    s.set_text(self.runtime.deferred_mode, cmd.text);
                    ok_ack()
                }
                stim => err_wrong_type(stim, "SetText", "Text"),
            },
        }
    }

    // ── SetTextColor ──────────────────────────────────────────────────────────

    fn cmd_set_text_color(&mut self, handle: u32, cmd: proto::SetTextColorRequest) -> proto::Response {
        let c = match cmd.color {
            Some(c) => c.into(),
            None => return err(proto::ErrorCode::InvalidArgument, "color must be set"),
        };
        match self.config.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(entry) => match &mut entry.stimulus {
                Stimulus::Text(s) => {
                    s.set_color(self.runtime.deferred_mode, c);
                    ok_ack()
                }
                stim => err_wrong_type(stim, "SetTextColor", "Text"),
            },
        }
    }

    // ── SetBackground ─────────────────────────────────────────────────────────

    fn cmd_set_background(&mut self, cmd: proto::SetBackgroundRequest) -> proto::Response {
        let c = match cmd.color {
            Some(c) => c.into(),
            None => {
                return err(proto::ErrorCode::InvalidArgument, "background color must be set");
            }
        };
        self.config.background.set(self.runtime.deferred_mode, c);
        ok_ack()
    }

    // ── SetDeferredMode ───────────────────────────────────────────────────────

    fn cmd_set_deferred_mode(&mut self, cmd: proto::SetDeferredModeRequest) -> proto::Response {
        if cmd.active {
            self.begin_deferred();
        } else if cmd.cancel {
            self.runtime.deferred_mode = false;
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
        let (w, h) = self.runtime.screen_size.unwrap_or((0, 0));
        let bg = self.config.background.live;
        let version = parse_cargo_version();
        ok_body(proto::response::Body::ServerInfo(proto::QueryServerInfoResponse {
            width: w,
            height: h,
            frame_rate: self.runtime.frame_rate,
            background_color: Some(bg.into()),
            backend: proto::RenderBackend::Unspecified as i32,
            version: Some(version),
        }))
    }

    // ── SetName ───────────────────────────────────────────────────────────────

    fn cmd_set_name(&mut self, handle: u32, cmd: proto::SetNameRequest) -> proto::Response {
        match self.config.stimuli.get_mut(&handle) {
            Some(entry) => { entry.name = nonempty(cmd.name); ok_ack() }
            None => err_not_found(handle),
        }
    }

    // ── QueryStimulus ─────────────────────────────────────────────────────────

    fn cmd_query_stimulus(&self, handle: u32) -> proto::Response {
        let entry = match self.config.stimuli.get(&handle) {
            Some(e) => e,
            None => return err_not_found(handle),
        };
        let stim = &entry.stimulus;

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
                        ShapeStimulus::Circle(d) => (
                            proto::StimulusType::Circle as i32,
                            Some(proto::StimulusParams {
                                shape: Some(proto::stimulus_params::Shape::Circle(proto::CircleParams {
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
                        Some(a.fill_color.into()),
                        Some(a.outline_color.into()),
                        a.stroke_width,
                        scene_draw_mode_to_proto(a.draw_mode),
                        a.fill_color.a,
                    )
                }
                Stimulus::Grating(s) => {
                    let fc = s.params.live.fore_color;
                    let op = s.params.live.opacity;
                    (
                        proto::StimulusType::Grating as i32,
                        Some(grating_query_params(s)),
                        Some(proto::Color { r: fc.r, g: fc.g, b: fc.b, a: op }),
                        None,
                        0.0,
                        proto::ShapeDrawMode::Filled as i32,
                        op,
                    )
                }
                Stimulus::Text(s) => {
                    let c = s.params.live.color;
                    (
                        proto::StimulusType::Text as i32,
                        Some(text_query_params(s)),
                        Some(c.into()),
                        None,
                        0.0,
                        proto::ShapeDrawMode::Filled as i32,
                        c.a,
                    )
                }
            };

        let draw_order = self.config.stimuli.get_index_of(&handle).unwrap_or(0) as u32;
        ok_body(proto::response::Body::StimulusInfo(proto::QueryStimulusResponse {
            stimulus_type,
            enabled: stim.flags().enabled,
            anim_enabled: stim.flags().anim_enabled,
            pos: Some(proto::Vec2 { x: pos[0], y: pos[1] }),
            orientation: angle,
            opacity,
            fill_color,
            outline_color,
            outline_width,
            draw_mode,
            params,
            id: entry.id.to_string(),
            name: entry.name.clone().unwrap_or_default(),
            draw_order,
        }))
    }

    // ── Virtual Trigger Line commands ─────────────────────────────────────────

    fn cmd_set_virtual_trigger_line_name(
        &mut self,
        cmd: proto::SetVirtualTriggerLineNameRequest,
        vtl: Option<&mut VtlState>,
    ) -> proto::Response {
        use vtl::{Direction, MAX_BANKS};

        if cmd.bank >= MAX_BANKS as u32 {
            return err(proto::ErrorCode::InvalidArgument, "bank out of range");
        }
        if cmd.bit >= 64 {
            return err(proto::ErrorCode::InvalidArgument, "bit must be 0..63");
        }
        let dir = match proto::VirtualTriggerLineDirection::try_from(cmd.direction) {
            Ok(proto::VirtualTriggerLineDirection::Input)  => Direction::Input,
            Ok(proto::VirtualTriggerLineDirection::Output) => Direction::Output,
            _ => return err(proto::ErrorCode::InvalidArgument, "direction must be INPUT or OUTPUT"),
        };
        let Some(vtl) = vtl else {
            return err(proto::ErrorCode::NotSupported, "VTL shared memory not available");
        };

        if !cmd.name.is_empty()
            && vtl.names.iter().any(|e| e.name == cmd.name && (e.bank != cmd.bank as u8 || e.bit != cmd.bit as u8))
        {
            return err(proto::ErrorCode::InvalidArgument, "name already assigned to a different line");
        }

        vtl.names.retain(|e| !(e.bank == cmd.bank as u8 && e.bit == cmd.bit as u8));
        if !cmd.name.is_empty() {
            vtl.names.push(VtlNameEntry {
                name:      cmd.name,
                bank:      cmd.bank as u8,
                bit:       cmd.bit  as u8,
                direction: dir,
            });
        }

        vtl.sync_names_to_shm();
        ok_ack()
    }

    fn cmd_list_virtual_trigger_lines(&self, vtl: Option<&VtlState>) -> proto::Response {
        let Some(vtl) = vtl else {
            return ok_body(proto::response::Body::VirtualTriggerLineList(
                proto::ListVirtualTriggerLinesResponse { lines: vec![] },
            ));
        };
        let owner = vtl.owner();

        let state_word = |bank: usize, dir: vtl::Direction| -> u64 {
            match dir {
                vtl::Direction::Input  => owner.input_state(bank),
                vtl::Direction::Output => owner.output_state(bank),
            }
        };

        let lines: Vec<proto::VirtualTriggerLineInfo> = vtl.names.iter().map(|e| {
            let high = state_word(e.bank as usize, e.direction) >> e.bit & 1 == 1;
            proto::VirtualTriggerLineInfo {
                name:      e.name.clone(),
                bank:      e.bank as u32,
                bit:       e.bit  as u32,
                direction: match e.direction {
                    vtl::Direction::Input  => proto::VirtualTriggerLineDirection::Input  as i32,
                    vtl::Direction::Output => proto::VirtualTriggerLineDirection::Output as i32,
                },
                high,
            }
        }).collect();
        ok_body(proto::response::Body::VirtualTriggerLineList(
            proto::ListVirtualTriggerLinesResponse { lines },
        ))
    }

    fn cmd_set_input_virtual_trigger_line(
        &self,
        cmd: proto::SetInputVirtualTriggerLineRequest,
        vtl: Option<&VtlState>,
    ) -> proto::Response {
        let Some(vtl) = vtl else {
            return err(proto::ErrorCode::NotSupported, "VTL shared memory not available");
        };
        let (bank, bit) = match resolve_vtl_handle(cmd.handle.as_ref(), &vtl.names) {
            Ok(v) => v,
            Err(e) => return *e,
        };
        let owner = vtl.owner();
        if cmd.value {
            if owner.set_input_bit(bank, bit) {
                owner.set_input_rise(bank, 1u64 << bit);
            }
        } else {
            if owner.clear_input_bit(bank, bit) {
                owner.set_input_fall(bank, 1u64 << bit);
            }
        }
        ok_ack()
    }

    fn cmd_toggle_input_virtual_trigger_line(
        &self,
        cmd: proto::ToggleInputVirtualTriggerLineRequest,
        vtl: Option<&VtlState>,
    ) -> proto::Response {
        let Some(vtl) = vtl else {
            return err(proto::ErrorCode::NotSupported, "VTL shared memory not available");
        };
        let (bank, bit) = match resolve_vtl_handle(cmd.handle.as_ref(), &vtl.names) {
            Ok(v) => v,
            Err(e) => return *e,
        };
        let owner = vtl.owner();
        let mask = 1u64 << bit;
        let rose = owner.toggle_input_bit(bank, bit);
        if rose { owner.set_input_rise(bank, mask); } else { owner.set_input_fall(bank, mask); }
        ok_body(proto::response::Body::VirtualTriggerLineState(
            proto::VirtualTriggerLineStateResponse { high: rose },
        ))
    }

    fn cmd_clear_input_virtual_trigger_line_latches(
        &self,
        cmd: proto::ClearInputVirtualTriggerLineLatchesRequest,
        vtl: Option<&VtlState>,
    ) -> proto::Response {
        let Some(vtl) = vtl else {
            return err(proto::ErrorCode::NotSupported, "VTL shared memory not available");
        };
        let (bank, bit) = match resolve_vtl_handle(cmd.handle.as_ref(), &vtl.names) {
            Ok(v) => v,
            Err(e) => return *e,
        };
        let owner = vtl.owner();
        let mask = 1u64 << bit;
        owner.drain_input_rise(bank, mask);
        owner.drain_input_fall(bank, mask);
        ok_ack()
    }

    fn cmd_set_input_virtual_trigger_line_bank(
        &self,
        cmd: proto::SetInputVirtualTriggerLineBankRequest,
        vtl: Option<&VtlState>,
    ) -> proto::Response {
        let Some(vtl) = vtl else {
            return err(proto::ErrorCode::NotSupported, "VTL shared memory not available");
        };
        if cmd.bank >= vtl::MAX_BANKS as u32 {
            return err(proto::ErrorCode::InvalidArgument, "bank out of range");
        }
        let owner = vtl.owner();
        let bank    = cmd.bank as usize;
        let prev    = owner.input_state(bank);
        let next    = cmd.value;
        let rising  = (!prev) & next;
        let falling = prev & (!next);
        owner.set_input_state(bank, next);
        if rising  != 0 { owner.set_input_rise(bank,  rising);  }
        if falling != 0 { owner.set_input_fall(bank, falling); }
        ok_ack()
    }

    fn cmd_set_output_virtual_trigger_line(
        &self,
        cmd: proto::SetOutputVirtualTriggerLineRequest,
        vtl: Option<&mut VtlState>,
    ) -> proto::Response {
        let Some(vtl) = vtl else {
            return err(proto::ErrorCode::NotSupported, "VTL shared memory not available");
        };
        let (bank, bit) = match resolve_vtl_handle(cmd.handle.as_ref(), &vtl.names) {
            Ok(v) => v,
            Err(e) => return *e,
        };
        vtl.set_staged_bit(bank, bit, cmd.value);
        ok_ack()
    }

    fn cmd_toggle_output_virtual_trigger_line(
        &self,
        cmd: proto::ToggleOutputVirtualTriggerLineRequest,
        vtl: Option<&mut VtlState>,
    ) -> proto::Response {
        let Some(vtl) = vtl else {
            return err(proto::ErrorCode::NotSupported, "VTL shared memory not available");
        };
        let (bank, bit) = match resolve_vtl_handle(cmd.handle.as_ref(), &vtl.names) {
            Ok(v) => v,
            Err(e) => return *e,
        };
        let high = (vtl.staged[bank] >> bit) & 1 == 0; // will be high after toggle
        vtl.set_staged_bit(bank, bit, high);
        ok_body(proto::response::Body::VirtualTriggerLineState(
            proto::VirtualTriggerLineStateResponse { high },
        ))
    }

    fn cmd_set_output_virtual_trigger_line_bank(
        &self,
        cmd: proto::SetOutputVirtualTriggerLineBankRequest,
        vtl: Option<&mut VtlState>,
    ) -> proto::Response {
        let Some(vtl) = vtl else {
            return err(proto::ErrorCode::NotSupported, "VTL shared memory not available");
        };
        if cmd.bank >= vtl::MAX_BANKS as u32 {
            return err(proto::ErrorCode::InvalidArgument, "bank out of range");
        }
        vtl.set_staged_bank(cmd.bank as usize, cmd.value);
        ok_ack()
    }

    // ── ListStimuli ───────────────────────────────────────────────────────────

    fn cmd_list_stimuli(&self) -> proto::Response {
        let entries: Vec<proto::StimulusEntry> = self
            .stimuli
            .iter()
            .map(|(&handle, entry)| {
                let stim = &entry.stimulus;
                let stimulus_type = match stim {
                    Stimulus::Shape(ShapeStimulus::Rect(_))    => proto::StimulusType::Rect,
                    Stimulus::Shape(ShapeStimulus::Ellipse(_)) => proto::StimulusType::Ellipse,
                    Stimulus::Shape(ShapeStimulus::Circle(_))  => proto::StimulusType::Circle,
                    Stimulus::Grating(_)                       => proto::StimulusType::Grating,
                    Stimulus::Text(_)                          => proto::StimulusType::Text,
                } as i32;
                proto::StimulusEntry {
                    handle,
                    stimulus_type,
                    enabled: stim.flags().enabled,
                    id: entry.id.to_string(),
                    name: entry.name.clone().unwrap_or_default(),
                }
            })
            .collect();
        ok_body(proto::response::Body::StimulusList(proto::ListStimuliResponse { entries }))
    }

    // ── Animation commands ────────────────────────────────────────────────────

    fn cmd_create_animation(
        &mut self,
        cmd: proto::CreateAnimationRequest,
        vtl: Option<&VtlState>,
    ) -> proto::Response {
        let vtl_names: &[VtlNameEntry] = vtl.map_or(&[], |v| v.names.as_slice());
        let start_action = StartAction::from_bits_truncate(cmd.start_action_mask as u8);

        let start_action_trigger_line = if start_action.contains(StartAction::START_ACTION_TRIGGER_LINE) {
            match resolve_vtl_handle(cmd.start_action_trigger_line.as_ref(), vtl_names) {
                Ok((bank, bit)) => Some(VtlBit { bank, bit }),
                Err(e) => return *e,
            }
        } else {
            None
        };

        let final_action = FinalAction::from_bits_truncate(cmd.final_action_mask as u8);

        let final_action_trigger_line = if final_action.contains(FinalAction::FINAL_ACTION_TRIGGER_LINE) {
            match resolve_vtl_handle(cmd.final_action_trigger_line.as_ref(), vtl_names) {
                Ok((bank, bit)) => Some(VtlBit { bank, bit }),
                Err(e) => return *e,
            }
        } else {
            None
        };

        let start_trigger = if cmd.start_trigger.is_some() {
            match resolve_vtl_handle(cmd.start_trigger.as_ref(), vtl_names) {
                Ok((bank, bit)) => Some((VtlBit { bank, bit }, proto_vtl_edge(cmd.start_edge))),
                Err(e) => return *e,
            }
        } else {
            None
        };

        let animation = match proto_to_animation(&cmd, vtl_names) {
            Ok(a) => a,
            Err(e) => return *e,
        };

        let handle = self.alloc_anim_handle();
        self.config.animations.insert(handle, AnimationEntry {
            config: super::animation::AnimationConfig {
                name: cmd.name,
                state: AnimState::Idle,
                stimuli: cmd.stimuli,
                start_action,
                start_action_trigger_line,
                final_action,
                final_action_trigger_line,
                start_trigger,
                animation,
            },
            captured_user_enabled: None,
        });
        ok_handle(handle)
    }

    fn cmd_arm_animation(&mut self, cmd: proto::ArmAnimationRequest) -> proto::Response {
        match self.config.animations.get_mut(&cmd.handle) {
            Some(entry) => { entry.state = AnimState::Armed; ok_ack() }
            None => err(proto::ErrorCode::HandleNotFound,
                format!("animation handle {} not found", cmd.handle)),
        }
    }

    fn cmd_disarm_animation(&mut self, cmd: proto::DisarmAnimationRequest) -> proto::Response {
        let entry = match self.config.animations.get_mut(&cmd.handle) {
            Some(e) => e,
            None => return err(proto::ErrorCode::HandleNotFound,
                format!("animation handle {} not found", cmd.handle)),
        };

        let was_running = matches!(entry.state, AnimState::Running { .. });
        let stim_handles = entry.stimuli.clone();
        entry.state = AnimState::Idle;

        // Release any anim_enabled hold. Safe to do unconditionally: setting true when
        // already true is a no-op, and we don't need to track which animation types hold it.
        if was_running {
            for sh in stim_handles {
                if let Some(se) = self.config.stimuli.get_mut(&sh) {
                    se.stimulus.flags_mut().anim_enabled = true;
                    se.stimulus.flags_mut().mark_dirty();
                }
            }
        }
        ok_ack()
    }

    fn cmd_delete_animation(&mut self, cmd: proto::DeleteAnimationRequest) -> proto::Response {
        let entry = match self.config.animations.shift_remove(&cmd.handle) {
            Some(e) => e,
            None => return err(proto::ErrorCode::HandleNotFound,
                format!("animation handle {} not found", cmd.handle)),
        };
        if matches!(entry.state, AnimState::Running { .. }) {
            for sh in entry.config.stimuli {
                if let Some(se) = self.config.stimuli.get_mut(&sh) {
                    se.stimulus.flags_mut().anim_enabled = true;
                    se.stimulus.flags_mut().mark_dirty();
                }
            }
        }
        ok_ack()
    }

    fn cmd_list_animations(&self) -> proto::Response {
        let animations: Vec<proto::AnimationInfo> = self.config.animations.iter().map(|(&handle, entry)| {
            let state = match entry.state {
                AnimState::Idle             => proto::AnimationState::Idle    as i32,
                AnimState::Armed            => proto::AnimationState::Armed   as i32,
                AnimState::Running { .. }   => proto::AnimationState::Running as i32,
                AnimState::Done             => proto::AnimationState::Done    as i32,
            };
            proto::AnimationInfo {
                handle,
                name:      entry.name.clone(),
                state,
                type_name: entry.animation.type_name().to_string(),
            }
        }).collect();
        ok_body(proto::response::Body::AnimationList(
            proto::ListAnimationsResponse { animations },
        ))
    }

    fn cmd_query_animation(&self, cmd: proto::QueryAnimationRequest) -> proto::Response {
        let entry = match self.config.animations.get(&cmd.handle) {
            Some(e) => e,
            None => return err(proto::ErrorCode::HandleNotFound,
                format!("animation handle {} not found", cmd.handle)),
        };

        let state = match entry.state {
            AnimState::Idle           => proto::AnimationState::Idle    as i32,
            AnimState::Armed          => proto::AnimationState::Armed   as i32,
            AnimState::Running { .. } => proto::AnimationState::Running as i32,
            AnimState::Done           => proto::AnimationState::Done    as i32,
        };

        let (start_trigger, start_edge) = match entry.start_trigger {
            Some((bit, edge)) => (Some(vtl_bit_to_proto(bit)), edge_to_proto(edge)),
            None              => (None, 0),
        };

        let params = proto::CreateAnimationRequest {
            name:                entry.name.clone(),
            start_action_mask:   entry.start_action.bits() as u32,
            start_action_trigger_line: entry.start_action_trigger_line.map(vtl_bit_to_proto),
            final_action_mask:   entry.final_action.bits() as u32,
            final_action_trigger_line: entry.final_action_trigger_line.map(vtl_bit_to_proto),
            start_trigger,
            start_edge,
            stimuli:             entry.stimuli.clone(),
            body:                Some(animation_to_proto_body(&entry.animation)),
        };

        ok_body(proto::response::Body::QueryAnimationResponse(proto::QueryAnimationResponse {
            handle: cmd.handle,
            state,
            params: Some(params),
        }))
    }
}

fn vtl_bit_to_proto(bit: VtlBit) -> proto::VirtualTriggerLineHandle {
    use proto::virtual_trigger_line_handle::Handle;
    proto::VirtualTriggerLineHandle {
        handle: Some(Handle::BankBit(proto::VirtualTriggerLineBankBit {
            bank: bit.bank as u32,
            bit:  bit.bit  as u32,
        }))
    }
}

fn edge_to_proto(e: Edge) -> i32 {
    match e {
        Edge::Rising  => proto::VtlEdge::Rising  as i32,
        Edge::Falling => proto::VtlEdge::Falling as i32,
    }
}

fn animation_to_proto_body(anim: &Animation) -> proto::create_animation_request::Body {
    use proto::create_animation_request::Body as PBody;
    match anim {
        Animation::CoupleVisibilityToTriggerLine { trigger, polarity } =>
            PBody::CoupleVisibilityToTriggerLine(proto::CoupleVisibilityToTriggerLine {
                trigger:  Some(vtl_bit_to_proto(*trigger)),
                polarity: *polarity,
            }),
        Animation::EnableOnTriggerEdge { trigger, edge, enabled } =>
            PBody::EnableOnTriggerEdge(proto::EnableOnTriggerEdge {
                trigger: Some(vtl_bit_to_proto(*trigger)),
                edge:    edge_to_proto(*edge),
                enabled: *enabled,
            }),
        Animation::FlashForNFrames { duration_frames } =>
            PBody::FlashForNFrames(proto::FlashForNFrames { duration_frames: *duration_frames }),
        Animation::FlickerForNFrames { on_frames, off_frames, total_frames, start_on_phase } =>
            PBody::FlickerForNFrames(proto::FlickerForNFrames {
                on_frames:      *on_frames,
                off_frames:     *off_frames,
                total_frames:   *total_frames,
                start_on_phase: *start_on_phase,
            }),
        Animation::MoveAlongPath2D { coords } =>
            PBody::MoveAlongPath2d(proto::MoveAlongPath2D {
                x: coords.iter().map(|c| c[0]).collect(),
                y: coords.iter().map(|c| c[1]).collect(),
            }),
        Animation::MoveAlongSegments2D { waypoints, speed_px_per_sec } =>
            PBody::MoveAlongSegments2d(proto::MoveAlongSegments2D {
                x:                waypoints.iter().map(|w| w[0]).collect(),
                y:                waypoints.iter().map(|w| w[1]).collect(),
                speed_px_per_sec: *speed_px_per_sec,
            }),
        Animation::ExternalPosition2D { shm_name, x_offset, y_offset } =>
            PBody::ExternalPosition2d(proto::ExternalPosition2D {
                shm_name: shm_name.clone(),
                x_offset: *x_offset,
                y_offset: *y_offset,
            }),
    }
}

fn proto_vtl_edge(e: i32) -> Edge {
    match proto::VtlEdge::try_from(e).unwrap_or(proto::VtlEdge::Rising) {
        proto::VtlEdge::Rising  => Edge::Rising,
        proto::VtlEdge::Falling => Edge::Falling,
    }
}

// ── Animation proto → Rust mapping ───────────────────────────────────────────

fn proto_to_animation(
    cmd: &proto::CreateAnimationRequest,
    vtl_names: &[VtlNameEntry],
) -> Result<Animation, Box<proto::Response>> {
    use proto::create_animation_request::Body as PBody;

    let vtl_bit = |h: Option<&proto::VirtualTriggerLineHandle>| -> Result<VtlBit, Box<proto::Response>> {
        let (bank, bit) = resolve_vtl_handle(h, vtl_names)?;
        Ok(VtlBit { bank, bit })
    };

    let proto_edge = |e: i32| -> Edge { proto_vtl_edge(e) };

    match cmd.body.as_ref() {
        Some(PBody::CoupleVisibilityToTriggerLine(c)) => Ok(Animation::CoupleVisibilityToTriggerLine {
            trigger:  vtl_bit(c.trigger.as_ref())?,
            polarity: c.polarity,
        }),
        Some(PBody::EnableOnTriggerEdge(c)) => Ok(Animation::EnableOnTriggerEdge {
            trigger: vtl_bit(c.trigger.as_ref())?,
            edge:    proto_edge(c.edge),
            enabled: c.enabled,
        }),
        Some(PBody::FlashForNFrames(c)) => Ok(Animation::FlashForNFrames {
            duration_frames: c.duration_frames,
        }),
        Some(PBody::FlickerForNFrames(c)) => Ok(Animation::FlickerForNFrames {
            on_frames:      c.on_frames,
            off_frames:     c.off_frames,
            total_frames:   c.total_frames,
            start_on_phase: c.start_on_phase,
        }),
        Some(PBody::MoveAlongPath2d(c)) => {
            if c.x.len() != c.y.len() {
                return Err(Box::new(err(proto::ErrorCode::InvalidArgument, "MoveAlongPath2D: x and y must have equal length")));
            }
            Ok(Animation::MoveAlongPath2D {
                coords: c.x.iter().zip(c.y.iter()).map(|(&x, &y)| [x, y]).collect(),
            })
        },
        Some(PBody::MoveAlongSegments2d(c)) => {
            if c.x.len() != c.y.len() {
                return Err(Box::new(err(proto::ErrorCode::InvalidArgument, "MoveAlongSegments2D: x and y must have equal length")));
            }
            if c.x.len() < 2 {
                return Err(Box::new(err(proto::ErrorCode::InvalidArgument, "MoveAlongSegments2D: at least 2 waypoints required")));
            }
            Ok(Animation::MoveAlongSegments2D {
                waypoints:        c.x.iter().zip(c.y.iter()).map(|(&x, &y)| [x, y]).collect(),
                speed_px_per_sec: c.speed_px_per_sec,
            })
        },
        Some(PBody::ExternalPosition2d(c)) => Ok(Animation::ExternalPosition2D {
            shm_name: c.shm_name.clone(),
            x_offset: c.x_offset,
            y_offset: c.y_offset,
        }),
        None => Err(Box::new(err(proto::ErrorCode::InvalidArgument, "animation body must be set"))),
    }
}

// ── Module-private helpers ────────────────────────────────────────────────────

fn resolve_vtl_handle(
    handle: Option<&proto::VirtualTriggerLineHandle>,
    names: &[VtlNameEntry],
) -> Result<(usize, u8), Box<proto::Response>> {
    use proto::virtual_trigger_line_handle::Handle;
    match handle.and_then(|h| h.handle.as_ref()) {
        Some(Handle::BankBit(bb)) => {
            if bb.bank >= vtl::MAX_BANKS as u32 {
                return Err(Box::new(err(proto::ErrorCode::InvalidArgument, "bank out of range")));
            }
            if bb.bit >= 64 {
                return Err(Box::new(err(proto::ErrorCode::InvalidArgument, "bit must be 0..63")));
            }
            Ok((bb.bank as usize, bb.bit as u8))
        }
        Some(Handle::Name(name)) => {
            names.iter()
                .find(|e| e.name == *name)
                .map(|e| (e.bank as usize, e.bit))
                .ok_or_else(|| Box::new(err(proto::ErrorCode::InvalidArgument,
                    format!("no virtual trigger line named {name:?}"))))
        }
        None => Err(Box::new(err(proto::ErrorCode::InvalidArgument, "handle must be set"))),
    }
}

fn color_or_default(c: Option<proto::Color>, default: Color) -> Color {
    c.map(|c| c.into()).unwrap_or(default)
}

fn parse_or_new_uuid(s: &str) -> Result<Uuid, Box<proto::Response>> {
    if s.is_empty() {
        return Ok(Uuid::new_v4());
    }
    Uuid::parse_str(s)
        .map_err(|_| Box::new(err(proto::ErrorCode::InvalidArgument, "id must be a valid UUID string")))
}

fn nonempty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
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

// ── Config persistence commands ───────────────────────────────────────────────

impl SceneState {
    fn cmd_list_configs(&self) -> proto::Response {
        match list_config_names(&self.runtime.config_dir) {
            Ok(names) => ok_body(proto::response::Body::ConfigList(proto::ListConfigsResponse { names })),
            Err(e) => err(proto::ErrorCode::FileIo, e.to_string()),
        }
    }

    fn cmd_load_config(&mut self, cmd: proto::LoadConfigRequest, vtl: Option<&mut VtlState>) -> proto::Response {
        let path = self.runtime.config_dir.join(format!("vstimd_{}.config.json", cmd.name));
        match load_config(&path) {
            Ok((scene_cfg, io)) => {
                if let Some(v) = vtl {
                    v.config.names = io.vtl.names;
                    v.sync_names_to_shm();
                }
                let mode = if cmd.additive {
                    super::scene_config::LoadMode::Additive
                } else {
                    super::scene_config::LoadMode::Replace
                };
                self.load_snapshot(scene_cfg, mode);
                ok_ack()
            }
            Err(e) if is_not_found(&e) => err(proto::ErrorCode::FileNotFound, e.to_string()),
            Err(e) if is_format_error(&e) => err(proto::ErrorCode::FileFormat, e.to_string()),
            Err(e) => err(proto::ErrorCode::FileIo, e.to_string()),
        }
    }

    fn cmd_upload_config(&mut self, cmd: proto::UploadConfigRequest, vtl: Option<&mut VtlState>) -> proto::Response {
        let (scene_cfg, io) = match parse_config_json(&cmd.json) {
            Ok(v) => v,
            Err(e) => return err(proto::ErrorCode::FileFormat, e.to_string()),
        };
        let path = self.runtime.config_dir.join(format!("vstimd_{}.config.json", cmd.name));
        if path.exists() && !cmd.overwrite {
            return err(proto::ErrorCode::FileAlreadyExists, "config already exists");
        }
        if let Err(e) = std::fs::create_dir_all(&self.runtime.config_dir)
            .and_then(|_| std::fs::write(&path, &cmd.json))
        {
            return err(proto::ErrorCode::FileIo, e.to_string());
        }
        if cmd.apply_now {
            if let Some(v) = vtl {
                v.config.names = io.vtl.names;
                v.sync_names_to_shm();
            }
            let mode = if cmd.additive {
                super::scene_config::LoadMode::Additive
            } else {
                super::scene_config::LoadMode::Replace
            };
            self.load_snapshot(scene_cfg, mode);
        }
        ok_ack()
    }

    fn cmd_retrieve_config(&self, vtl: Option<&VtlState>) -> proto::Response {
        let default_vtl = VtlConfig::default();
        let vtl_cfg = vtl.map_or(&default_vtl, |v| &v.config);
        match retrieve_config_json(&self.config, vtl_cfg) {
            Ok(json) => ok_body(proto::response::Body::RetrievedConfig(proto::RetrieveConfigResponse { json })),
            Err(e) => err(proto::ErrorCode::Unknown, e.to_string()),
        }
    }
}
