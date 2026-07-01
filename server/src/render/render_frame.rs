use std::sync::Mutex;

use ash::vk;

use crate::render::overlay_ui::{OverlayArgs, build_overlay_ui};
use crate::render::render_state::RenderState;
use crate::render::tess::{self, tessellate_photodiode};
use crate::render::vk::{TextPushConstants, TextVertex};
use crate::scene::photodiode::PHOTODIODE_HANDLE;
use crate::scene::stimulus::grating::{build_grating_push_constants, grating_phase_inc};
use crate::scene::stimulus::text::layout_and_rasterize;
use crate::scene::stimulus::{DrawMode, Stimulus};
use crate::timing::{FramePhases, FrameTick};
use crate::vtl_state::VtlState;

/// Overlay tessellation output — lives on the stack between the egui UI pass
/// and the GPU recording pass, so its references stay valid for both.
struct EguiStore {
    textures_delta: egui::TexturesDelta,
    primitives: Vec<egui::ClippedPrimitive>,
    pixels_per_point: f32,
}

/// Build the egui overlay (if visible) then tessellate, record, submit, and
/// present one GPU frame.
///
/// `log_buffer` and `metrics` are owned by the backend and passed in so the
/// render state itself holds no application-level observability data.
///
/// Returns `(None, _)` when the swapchain is out of date; the caller must
/// recreate it before the next call.
pub fn render_frame(
    rs: &mut RenderState,
    screen_clock: Option<std::time::Instant>,
    egui_raw_input: Option<egui::RawInput>,
    vtl: Option<&Mutex<VtlState>>,
) -> (Option<FrameTick>, Option<egui::PlatformOutput>) {
    // ── 1. Build egui overlay ─────────────────────────────────────────────────
    let mut egui_store: Option<EguiStore> = None;
    let mut platform_output: Option<egui::PlatformOutput> = None;

    let wireframe = rs
        .ctx
        .supports_wireframe
        .then_some(rs.scene_renderer.wireframe);

    if let Some(ui) = &mut rs.ui
        && ui.overlay.master_visible
        && let Some(raw_input) = egui_raw_input
    {
        let phases = rs.timing.last_phases;
        let crate::render::ui_renderer::UiRenderer {
            ref mut egui_ctx,
            ref mut overlay,
            ref mut metrics,
            ref log_buffer,
            ..
        } = *ui;
        let metrics_sample = metrics.sample();
        let output = egui_ctx.run_ui(raw_input, |ctx| {
            build_overlay_ui(
                ctx,
                &mut OverlayArgs {
                    scene: &rs.scene_renderer.scene,
                    vtl,
                    frame_stats: &mut rs.timing.stats,
                    last_phases: phases,
                    sys: &rs.system_info,
                    display: &rs.display_info,
                    wireframe,
                    metrics: metrics_sample,
                    log_buffer,
                    overlay,
                },
            );
        });
        platform_output = Some(output.platform_output);
        let ppp = output.pixels_per_point;
        let primitives = ui.egui_ctx.tessellate(output.shapes, ppp);
        egui_store = Some(EguiStore {
            textures_delta: output.textures_delta,
            primitives,
            pixels_per_point: ppp,
        });
    }

    // Apply the overlay's wireframe toggle request (the System group can't reach
    // the scene-renderer pipeline state itself). Read+reset, then act, so we
    // don't hold a borrow of `rs.ui` while mutating `rs.scene_renderer`.
    let toggle_wireframe = rs
        .ui
        .as_mut()
        .map(|ui| std::mem::take(&mut ui.overlay.wireframe_toggle_requested))
        .unwrap_or(false);
    if toggle_wireframe && rs.ctx.supports_wireframe {
        rs.scene_renderer.wireframe = !rs.scene_renderer.wireframe;
        log::info!(
            "vstimd: wireframe {}",
            if rs.scene_renderer.wireframe {
                "ON"
            } else {
                "OFF"
            }
        );
    }

    // ── 2. Wait for this slot's previous GPU work ─────────────────────────────
    let ctx = &rs.ctx;
    let this_present_id = ctx.next_present_id.get();
    let frame = &ctx.frames[rs.timing.frame_index % ctx.frames.len()];
    let t_fence_start = std::time::Instant::now();
    unsafe {
        ctx.device
            .wait_for_fences(&[frame.in_flight], true, u64::MAX)
            .expect("fence wait");
    }
    let fence_us = t_fence_start.elapsed().as_micros() as u32;

    // ── 3. Screen clock ───────────────────────────────────────────────────────
    let vblank_time = if let Some(t) = screen_clock {
        t
    } else if let (Some(pw), true) = (&ctx.present_wait, this_present_id > 1) {
        unsafe {
            match pw.wait_for_present(ctx.swapchain, this_present_id - 1, 100_000_000) {
                Ok(()) => {}
                Err(vk::Result::TIMEOUT) => log::warn!(
                    "vstimd: vkWaitForPresentKHR timed out (id {})",
                    this_present_id - 1
                ),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::ERROR_SURFACE_LOST_KHR) => {
                    return (None, platform_output);
                }
                Err(e) => panic!("vkWaitForPresentKHR: {e}"),
            }
        }
        std::time::Instant::now()
    } else {
        std::time::Instant::now()
    };

    // ── 4. Tessellate scene into GPU buffers ──────────────────────────────────
    let t_tess_start = std::time::Instant::now();
    {
        let fps = rs.timing.stats.summary().fps as f32;
        let screen_size = (ctx.extent.width, ctx.extent.height);
        let screen_w = ctx.extent.width as f32;
        let screen_h = ctx.extent.height as f32;
        let mut sc = rs
            .scene_renderer
            .scene
            .write()
            .expect("scene lock poisoned");
        if sc.runtime.pending_flip {
            sc.apply_flip();
        }
        sc.runtime.frame_count += 1;
        let _ = sc.runtime.frame_notifier.send(sc.runtime.frame_count);
        sc.runtime.screen_size = Some(screen_size);
        sc.runtime.frame_rate = fps;
        if sc.runtime.last_uploaded_size != screen_size {
            sc.runtime.last_uploaded_size = screen_size;
            for entry in sc.stimuli.values_mut() {
                entry.stimulus.flags_mut().mark_dirty();
            }
        }

        let cache = &mut rs.scene_renderer.scene_cache;
        cache
            .solid
            .fill_meshes
            .retain(|h, _| *h == PHOTODIODE_HANDLE || sc.stimuli.contains_key(h));
        cache
            .solid
            .stroke_meshes
            .retain(|h, _| sc.stimuli.contains_key(h));
        cache.text.meshes.retain(|h, _| sc.stimuli.contains_key(h));

        for (&handle, entry) in sc.stimuli.iter_mut() {
            match &mut entry.stimulus {
                Stimulus::Grating(s) => {
                    if s.flags.is_visible() && s.params.live.drift_speed != 0.0 {
                        s.phase_accum += grating_phase_inc(s, fps);
                    }
                }

                Stimulus::Text(text) => {
                    let has_mesh = cache.text.meshes.contains_key(&handle);
                    if !text.flags.dirty && (text.flags.is_visible() == has_mesh) {
                        continue;
                    }
                    let glyphs = if text.flags.is_visible() {
                        layout_and_rasterize(
                            text,
                            screen_w,
                            screen_h,
                            &mut rs.text.font_system,
                            &mut rs.text.swash_cache,
                        )
                    } else {
                        Default::default()
                    };

                    let half_w = screen_w * 0.5;
                    let half_h = screen_h * 0.5;
                    let mut verts: Vec<TextVertex> = Vec::new();
                    let mut idxs: Vec<u32> = Vec::new();
                    for g in &glyphs {
                        let ae = rs.text.glyph_atlas.lookup(g.key).or_else(|| {
                            rs.text.glyph_atlas.insert(
                                g.key,
                                &g.bitmap,
                                g.bitmap_width,
                                g.bitmap_height,
                            )
                        });
                        let Some(e) = ae else { continue };
                        let x0 = g.screen_x / half_w - 1.0;
                        let y0 = 1.0 - g.screen_y / half_h;
                        let x1 = (g.screen_x + e.pixel_w as f32) / half_w - 1.0;
                        let y1 = 1.0 - (g.screen_y + e.pixel_h as f32) / half_h;
                        let base = verts.len() as u32;
                        verts.extend_from_slice(&[
                            TextVertex {
                                position: [x0, y0],
                                uv: [e.u0, e.v0],
                            },
                            TextVertex {
                                position: [x1, y0],
                                uv: [e.u1, e.v0],
                            },
                            TextVertex {
                                position: [x1, y1],
                                uv: [e.u1, e.v1],
                            },
                            TextVertex {
                                position: [x0, y1],
                                uv: [e.u0, e.v1],
                            },
                        ]);
                        idxs.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                    }
                    cache
                        .text
                        .upload(handle, &ctx.device, bytemuck::cast_slice(&verts), &idxs);
                    text.flags.dirty = false;
                }

                shape @ (Stimulus::Rect(_) | Stimulus::Ellipse(_) | Stimulus::Circle(_)) => {
                    let has_mesh = cache.solid.fill_meshes.contains_key(&handle)
                        || cache.solid.stroke_meshes.contains_key(&handle);
                    if !shape.flags().dirty && (shape.flags().is_visible() == has_mesh) {
                        continue;
                    }
                    let tr =
                        tess::tessellate_shape_stimulus(shape, screen_size).expect("shape variant");
                    log::debug!(
                        "tess #{handle} {} screen={screen_size:?} fill_verts={} stroke_verts={}",
                        shape.type_name(),
                        tr.fill.0.len(),
                        tr.stroke.0.len(),
                    );
                    cache.solid.upload(
                        handle,
                        &ctx.device,
                        (&tr.fill.0, &tr.fill.1),
                        (&tr.stroke.0, &tr.stroke.1),
                    );
                    shape.flags_mut().dirty = false;
                }
            }
        }

        sc.photodiode.advance();
        let pd = &sc.photodiode;
        let geometry_changed = (pd.enabled != cache.photodiode.enabled)
            || (pd.enabled
                && (pd.position != cache.photodiode.position
                    || screen_size != cache.photodiode.screen_size));
        if geometry_changed {
            let (pd_verts, pd_idxs) = tessellate_photodiode(pd, screen_size);
            cache.solid.upload(
                PHOTODIODE_HANDLE,
                &ctx.device,
                (&pd_verts, &pd_idxs),
                (&[], &[]),
            );
            cache.photodiode.enabled = pd.enabled;
            cache.photodiode.lit = pd.enabled.then_some(pd.lit);
            cache.photodiode.position = pd.position;
            cache.photodiode.screen_size = screen_size;
        } else if pd.enabled && cache.photodiode.lit != Some(pd.lit) {
            let (pd_verts, _) = tessellate_photodiode(pd, screen_size);
            cache
                .solid
                .overwrite_fill_vertices(PHOTODIODE_HANDLE, &ctx.device, &pd_verts);
            cache.photodiode.lit = Some(pd.lit);
        }
    } // scene write lock dropped — ZMQ thread can run

    rs.text
        .glyph_atlas
        .flush(&ctx.device, ctx.graphics_queue, ctx.command_pool);
    let tessellate_us = t_tess_start.elapsed().as_micros() as u32;

    // ── 5. Select pipeline pair (after tessellation to avoid borrow conflict) ──
    // `pipe` and `grate` borrow from `scene_renderer.{pipeline,grating_pipeline}`,
    // which are disjoint from `scene_cache` already mutated above.
    let wireframe = rs.scene_renderer.wireframe;
    let pipe = if wireframe {
        &rs.scene_renderer.wireframe_pipeline
    } else {
        &rs.scene_renderer.pipeline
    };
    let grate = if wireframe {
        &rs.scene_renderer.wireframe_grating
    } else {
        &rs.scene_renderer.grating_pipeline
    };

    // ── 6. Acquire swapchain image ────────────────────────────────────────────
    let t_acquire_start = std::time::Instant::now();
    let (image_index, _suboptimal) = match unsafe {
        ctx.swapchain_loader.acquire_next_image(
            ctx.swapchain,
            u64::MAX,
            frame.image_available,
            vk::Fence::null(),
        )
    } {
        Ok(r) => r,
        Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return (None, platform_output),
        Err(e) => panic!("acquire_next_image: {e}"),
    };
    let acquire_us = t_acquire_start.elapsed().as_micros() as u32;

    // ── 7. Record command buffer ──────────────────────────────────────────────
    let t_record_start = std::time::Instant::now();
    let cb = frame.command_buffer;
    unsafe {
        ctx.device
            .reset_command_buffer(cb, vk::CommandBufferResetFlags::empty())
            .expect("cmd reset");
        ctx.device
            .begin_command_buffer(
                cb,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
            .expect("begin_command_buffer");

        let bg = {
            let sc = rs.scene_renderer.scene.read().expect("scene lock poisoned");
            vk::ClearColorValue {
                float32: sc.background.live.into(),
            }
        };
        let render_area = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: ctx.extent,
        };
        let clear_value = vk::ClearValue { color: bg };
        let rp_info = vk::RenderPassBeginInfo::default()
            .render_pass(ctx.render_pass)
            .framebuffer(ctx.framebuffers[image_index as usize])
            .render_area(render_area)
            .clear_values(std::slice::from_ref(&clear_value));

        ctx.device
            .cmd_begin_render_pass(cb, &rp_info, vk::SubpassContents::INLINE);

        let sc = rs.scene_renderer.scene.read().expect("scene lock poisoned");
        let screen_w = ctx.extent.width as f32;
        let screen_h = ctx.extent.height as f32;
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: screen_w,
            height: screen_h,
            min_depth: 0.0,
            max_depth: 1.0,
        };

        #[derive(PartialEq)]
        enum Bound {
            None,
            Solid,
            Grating,
            Text,
        }
        let mut bound = Bound::None;
        let quad = &grate.quad;

        ctx.cmd_begin_label(cb, "stimuli", [0.3, 0.7, 1.0, 1.0]);
        for (h, entry) in &sc.stimuli {
            let stim = &entry.stimulus;
            if !stim.is_visible() {
                continue;
            }

            if let Stimulus::Grating(s) = stim {
                if bound != Bound::Grating {
                    ctx.device.cmd_bind_pipeline(
                        cb,
                        vk::PipelineBindPoint::GRAPHICS,
                        grate.pipeline,
                    );
                    ctx.device
                        .cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
                    ctx.device
                        .cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));
                    ctx.device
                        .cmd_bind_vertex_buffers(cb, 0, &[quad.vertex_buffer], &[0]);
                    ctx.device.cmd_bind_index_buffer(
                        cb,
                        quad.index_buffer,
                        0,
                        vk::IndexType::UINT32,
                    );
                    bound = Bound::Grating;
                }
                let pc = build_grating_push_constants(s, screen_w, screen_h);
                ctx.device.cmd_push_constants(
                    cb,
                    grate.layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    bytemuck::bytes_of(&pc),
                );
                ctx.device
                    .cmd_draw_indexed(cb, quad.index_count, 1, 0, 0, 0);
            } else if let Stimulus::Text(t) = stim {
                if bound != Bound::Text {
                    ctx.device.cmd_bind_pipeline(
                        cb,
                        vk::PipelineBindPoint::GRAPHICS,
                        rs.text.text_pipeline.pipeline,
                    );
                    ctx.device
                        .cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
                    ctx.device
                        .cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));
                    ctx.device.cmd_bind_descriptor_sets(
                        cb,
                        vk::PipelineBindPoint::GRAPHICS,
                        rs.text.text_pipeline.layout,
                        0,
                        &[rs.text.glyph_atlas.descriptor_set],
                        &[],
                    );
                    bound = Bound::Text;
                }
                ctx.device.cmd_push_constants(
                    cb,
                    rs.text.text_pipeline.layout,
                    vk::ShaderStageFlags::FRAGMENT,
                    0,
                    bytemuck::bytes_of(&TextPushConstants {
                        color: t.params.live.color.into(),
                    }),
                );
                if let Some(mesh) = rs
                    .scene_renderer
                    .scene_cache
                    .text
                    .meshes
                    .get(h)
                    .filter(|m| m.index_count > 0)
                {
                    ctx.device
                        .cmd_bind_vertex_buffers(cb, 0, &[mesh.vertex_buffer], &[0]);
                    ctx.device.cmd_bind_index_buffer(
                        cb,
                        mesh.index_buffer,
                        0,
                        vk::IndexType::UINT32,
                    );
                    ctx.device
                        .cmd_draw_indexed(cb, mesh.index_count, 1, 0, 0, 0);
                }
            } else {
                let draw_mode = stim
                    .shape_appearance()
                    .map(|a| a.live.draw_mode)
                    .unwrap_or(DrawMode::Fill);
                let draw_fill = matches!(draw_mode, DrawMode::Fill | DrawMode::FillAndStroke);
                let draw_stroke = matches!(draw_mode, DrawMode::Stroke | DrawMode::FillAndStroke);
                if (draw_fill || draw_stroke) && bound != Bound::Solid {
                    ctx.device.cmd_bind_pipeline(
                        cb,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipe.pipeline,
                    );
                    ctx.device
                        .cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
                    ctx.device
                        .cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));
                    bound = Bound::Solid;
                }
                if draw_fill
                    && let Some(m) = rs
                        .scene_renderer
                        .scene_cache
                        .solid
                        .fill_meshes
                        .get(h)
                        .filter(|m| m.index_count > 0)
                {
                    ctx.device
                        .cmd_bind_vertex_buffers(cb, 0, &[m.vertex_buffer], &[0]);
                    ctx.device
                        .cmd_bind_index_buffer(cb, m.index_buffer, 0, vk::IndexType::UINT32);
                    ctx.device.cmd_draw_indexed(cb, m.index_count, 1, 0, 0, 0);
                }
                if draw_stroke
                    && let Some(m) = rs
                        .scene_renderer
                        .scene_cache
                        .solid
                        .stroke_meshes
                        .get(h)
                        .filter(|m| m.index_count > 0)
                {
                    ctx.device
                        .cmd_bind_vertex_buffers(cb, 0, &[m.vertex_buffer], &[0]);
                    ctx.device
                        .cmd_bind_index_buffer(cb, m.index_buffer, 0, vk::IndexType::UINT32);
                    ctx.device.cmd_draw_indexed(cb, m.index_count, 1, 0, 0, 0);
                }
            }
        }

        // Photodiode drawn on top, always solid pipeline.
        if let Some(m) = rs
            .scene_renderer
            .scene_cache
            .solid
            .fill_meshes
            .get(&PHOTODIODE_HANDLE)
            .filter(|m| m.index_count > 0)
        {
            if bound != Bound::Solid {
                ctx.device
                    .cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, pipe.pipeline);
                ctx.device
                    .cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
                ctx.device
                    .cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));
            }
            ctx.device
                .cmd_bind_vertex_buffers(cb, 0, &[m.vertex_buffer], &[0]);
            ctx.device
                .cmd_bind_index_buffer(cb, m.index_buffer, 0, vk::IndexType::UINT32);
            ctx.device.cmd_draw_indexed(cb, m.index_count, 1, 0, 0, 0);
        }
        ctx.cmd_end_label(cb);
        ctx.device.cmd_end_render_pass(cb);

        // ── Optional egui overlay pass ────────────────────────────────────────
        if let (Some(ui), Some(store)) = (rs.ui.as_mut(), egui_store.as_ref()) {
            ui.egui_renderer.update_textures(
                &ctx.device,
                ctx.graphics_queue,
                ctx.command_pool,
                &store.textures_delta,
            );
            ui.egui_renderer
                .upload_meshes(&ctx.device, &store.primitives, store.pixels_per_point);

            let egui_rp = vk::RenderPassBeginInfo::default()
                .render_pass(ctx.egui_render_pass)
                .framebuffer(ctx.framebuffers[image_index as usize])
                .render_area(render_area);
            ctx.device
                .cmd_begin_render_pass(cb, &egui_rp, vk::SubpassContents::INLINE);
            ctx.cmd_begin_label(cb, "egui overlay", [0.5, 0.9, 0.3, 1.0]);
            ui.egui_renderer.paint(
                &ctx.device,
                cb,
                &store.primitives,
                (ctx.extent.width, ctx.extent.height),
                store.pixels_per_point,
            );
            ctx.cmd_end_label(cb);
            ctx.device.cmd_end_render_pass(cb);
        }

        ctx.device
            .end_command_buffer(cb)
            .expect("end_command_buffer");
    }
    let record_us = t_record_start.elapsed().as_micros() as u32;

    // ── 8. Submit ─────────────────────────────────────────────────────────────
    let t_submit_start = std::time::Instant::now();
    unsafe {
        ctx.device
            .reset_fences(&[frame.in_flight])
            .expect("fence reset");
    }
    let wait_sems = [frame.image_available];
    let signal_sems = [frame.render_done];
    let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
    let cbs = [cb];
    unsafe {
        if let Err(e) = ctx.device.queue_submit(
            ctx.graphics_queue,
            &[vk::SubmitInfo::default()
                .wait_semaphores(&wait_sems)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&cbs)
                .signal_semaphores(&signal_sems)],
            frame.in_flight,
        ) {
            log::error!(
                "vstimd: queue_submit failed: {e} \
                 [frame={} tess={}µs fence={}µs acquire={}µs record={}µs]",
                this_present_id,
                tessellate_us,
                fence_us,
                acquire_us,
                record_us
            );
            std::process::exit(1);
        }
    }

    // ── 9. Present ────────────────────────────────────────────────────────────
    let present_ids = [this_present_id];
    let mut present_id_ext = vk::PresentIdKHR::default().present_ids(&present_ids);
    let swapchains = [ctx.swapchain];
    let image_indices_arr = [image_index];
    let mut present_info = vk::PresentInfoKHR::default()
        .wait_semaphores(&signal_sems)
        .swapchains(&swapchains)
        .image_indices(&image_indices_arr);
    if ctx.present_wait.is_some() {
        present_info = present_info.push_next(&mut present_id_ext);
    }
    match unsafe {
        ctx.swapchain_loader
            .queue_present(ctx.graphics_queue, &present_info)
    } {
        Ok(_) | Err(vk::Result::SUBOPTIMAL_KHR) => {}
        Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
            rs.timing.frame_index = rs.timing.frame_index.wrapping_add(1);
            return (None, platform_output);
        }
        Err(e) => panic!("queue_present: {e}"),
    }
    let submit_us = t_submit_start.elapsed().as_micros() as u32;

    // ── 10. Stats + return ────────────────────────────────────────────────────
    let warming_up = rs.timing.stats.is_warming_up();
    let dropped_frames = rs.timing.stats.on_present(vblank_time);
    if dropped_frames > 0 && !warming_up {
        log::warn!(
            "vstimd: {} dropped frame(s) before frame {} \
             [tess={}µs fence={}µs acquire={}µs record={}µs submit={}µs]",
            dropped_frames,
            this_present_id,
            tessellate_us,
            fence_us,
            acquire_us,
            record_us,
            submit_us
        );
    }
    rs.timing.frame_index = rs.timing.frame_index.wrapping_add(1);
    ctx.next_present_id.set(this_present_id + 1);

    let tick = FrameTick {
        frame: this_present_id,
        vblank_time,
        dropped_frames,
        phases: FramePhases {
            tessellate_us,
            fence_us,
            acquire_us,
            record_us,
            submit_us,
        },
    };
    rs.timing.last_phases = tick.phases;

    (Some(tick), platform_output)
}
