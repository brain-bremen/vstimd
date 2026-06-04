use std::sync::{Arc, RwLock};

use ash::vk;

use crate::render::tess::{self, tessellate_photodiode};
use crate::render::vk::scene_cache::SceneCache;
use crate::scene::stimulus::text::{
    TextFontSystem, TextSwashCache, layout_and_rasterize,
};
use crate::scene::stimulus::{DrawMode, Stimulus};
use crate::scene::vtl_state::VtlFrameState;

const PHOTODIODE_HANDLE: u32 = u32::MAX;
use crate::scene::SceneState;
use crate::timing::{FramePhases, FrameStats, FrameTick};

use super::context::VkContext;
use super::egui::VkEguiRenderer;
use super::pipeline::VkPipeline;
use super::text_atlas::GlyphAtlas;
use super::text_pipeline::{TextPushConstants, TextVertex, VkTextPipeline};
use crate::scene::stimulus::grating::{
    VkGratingPipeline, build_grating_push_constants, grating_phase_inc,
};

/// Optional egui overlay data for a frame
pub struct EguiFrameData<'a> {
    pub textures_delta: &'a egui::TexturesDelta,
    pub primitives: &'a [egui::ClippedPrimitive],
    pub pixels_per_point: f32,
}

/// Record and submit one frame.
///
/// Returns `Some(FrameTick)` on success, `None` when the swapchain is out of
/// date (caller must recreate before the next call).
#[allow(clippy::too_many_arguments)]
pub fn render_frame(
    ctx: &VkContext,
    pipeline: &VkPipeline,
    grating_pipeline: &VkGratingPipeline,
    text_pipeline: &VkTextPipeline,
    scene_cache: &mut SceneCache,
    glyph_atlas: &mut GlyphAtlas,
    font_system: &mut TextFontSystem,
    swash_cache: &mut TextSwashCache,
    scene: &Arc<RwLock<SceneState>>,
    frame_index: &mut usize,
    frame_stats: &mut FrameStats,
    mut egui_renderer: Option<&mut VkEguiRenderer>,
    egui_data: Option<EguiFrameData>,
    screen_clock: Option<std::time::Instant>,
    vtl_frame_state: Option<&mut VtlFrameState>,
) -> Option<FrameTick> {
    let this_present_id = ctx.next_present_id.get();

    // -- Wait for this slot's previous GPU work (must precede upload) ----------
    let frame = &ctx.frames[*frame_index % ctx.frames.len()];
    let t_fence_start = std::time::Instant::now();
    unsafe {
        ctx.device
            .wait_for_fences(&[frame.in_flight], true, u64::MAX)
            .expect("fence wait");
    }
    let fence_us = t_fence_start.elapsed().as_micros() as u32;

    // ── Screen clock ──────────────────────────────────────────────────────────
    let vblank_time = if let Some(t) = screen_clock {
        t
    } else if let (Some(pw), true) = (&ctx.present_wait, this_present_id > 1) {
        unsafe {
            match pw.wait_for_present(ctx.swapchain, this_present_id - 1, 3_000_000_000) {
                Ok(()) => {}
                Err(vk::Result::TIMEOUT) => {
                    log::warn!(
                        "vstimd: vkWaitForPresentKHR timed out (id {})",
                        this_present_id - 1
                    );
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::ERROR_SURFACE_LOST_KHR) => {
                    return None;
                }
                Err(e) => panic!("vkWaitForPresentKHR: {e}"),
            }
        }
        std::time::Instant::now()
    } else {
        std::time::Instant::now()
    };

    // -- Update: tessellate scene into GPU buffers ----------------------------
    let t_tess_start = std::time::Instant::now();
    {
        let fps = frame_stats.summary().fps as f32;
        let screen_size = (ctx.extent.width, ctx.extent.height);
        let screen_w = ctx.extent.width as f32;
        let screen_h = ctx.extent.height as f32;
        let mut sc = scene.write().expect("scene lock poisoned");
        if sc.pending_flip {
            sc.apply_flip();
        }

        // Keep references alive for upcoming VTL edge consumption wiring.
        // Do not drain latches here until edges are actually consumed.
        let _ = (vtl_frame_state, sc.vtl.as_ref());

        sc.screen_size = Some(screen_size);
        sc.frame_rate = fps;
        if sc.last_uploaded_size != screen_size {
            sc.last_uploaded_size = screen_size;
            for entry in sc.stimuli.values_mut() {
                entry.stimulus.flags_mut().mark_dirty();
            }
        }
        scene_cache.solid.fill_meshes.retain(|h, _| *h == PHOTODIODE_HANDLE || sc.stimuli.contains_key(h));
        scene_cache.solid.stroke_meshes.retain(|h, _| sc.stimuli.contains_key(h));
        scene_cache.text.meshes.retain(|h, _| sc.stimuli.contains_key(h));

        let handles: Vec<u32> = sc.stimuli.keys().copied().collect();
        for handle in handles {
            // Gratings: advance drift accumulator; skip tessellation (no mesh).
            if let Some(entry) = sc.stimuli.get_mut(&handle)
                && let Stimulus::Grating(s) = &mut entry.stimulus
            {
                if s.flags.is_visible() && s.params.live.drift_speed != 0.0 {
                    let inc = grating_phase_inc(s, fps);
                    s.phase_accum += inc;
                }
                continue;
            }

            // Text: lay out, rasterize into atlas, build glyph quads.
            if matches!(sc.stimuli[&handle].stimulus, Stimulus::Text(_)) {
                let (skip, glyphs) = {
                    let Stimulus::Text(text) = &sc.stimuli[&handle].stimulus else { unreachable!() };
                    let has_mesh = scene_cache.text.meshes.contains_key(&handle);
                    if !text.flags.dirty && (text.flags.is_visible() == has_mesh) {
                        (true, vec![])
                    } else {
                        let gs = if text.flags.is_visible() {
                            layout_and_rasterize(text, screen_w, screen_h, font_system, swash_cache)
                        } else {
                            Default::default()
                        };
                        (false, gs)
                    }
                }; // sc borrow released

                if !skip {
                    let half_w = screen_w * 0.5;
                    let half_h = screen_h * 0.5;
                    let mut verts: Vec<TextVertex> = Vec::new();
                    let mut idxs:  Vec<u32>       = Vec::new();
                    for g in &glyphs {
                        let ae = glyph_atlas.lookup(g.key)
                            .or_else(|| glyph_atlas.insert(g.key, &g.bitmap, g.bitmap_width, g.bitmap_height));
                        let Some(e) = ae else { continue };
                        let x0 = g.screen_x / half_w - 1.0;
                        let y0 = 1.0 - g.screen_y / half_h;
                        let x1 = (g.screen_x + e.pixel_w as f32) / half_w - 1.0;
                        let y1 = 1.0 - (g.screen_y + e.pixel_h as f32) / half_h;
                        let base = verts.len() as u32;
                        verts.extend_from_slice(&[
                            TextVertex { position: [x0, y0], uv: [e.u0, e.v0] },
                            TextVertex { position: [x1, y0], uv: [e.u1, e.v0] },
                            TextVertex { position: [x1, y1], uv: [e.u1, e.v1] },
                            TextVertex { position: [x0, y1], uv: [e.u0, e.v1] },
                        ]);
                        idxs.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
                    }
                    scene_cache.text.upload(handle, &ctx.device, bytemuck::cast_slice(&verts), &idxs);
                    sc.stimuli[&handle].stimulus.flags_mut().dirty = false;
                }
                continue;
            }

            // Shapes: lyon tessellation.
            let entry = &sc.stimuli[&handle];
            let Stimulus::Shape(shape) = &entry.stimulus else { continue };
            let has_mesh = scene_cache.solid.fill_meshes.contains_key(&handle)
                || scene_cache.solid.stroke_meshes.contains_key(&handle);
            if !shape.flags().dirty && (shape.flags().is_visible() == has_mesh) {
                continue;
            }
            let tess = tess::tessellate_shape_stimulus(shape, screen_size);
            log::debug!(
                "tess #{handle} {} screen={screen_size:?} fill_verts={} stroke_verts={}",
                shape.type_name(),
                tess.fill.0.len(),
                tess.stroke.0.len(),
            );
            scene_cache.solid.upload(handle, &ctx.device,
                (&tess.fill.0,   &tess.fill.1),
                (&tess.stroke.0, &tess.stroke.1));
            sc.stimuli[&handle].stimulus.flags_mut().dirty = false;
        }

        // Photodiode indicator.
        sc.photodiode.advance();
        let pd = &sc.photodiode;
        let geometry_changed = (pd.enabled != scene_cache.photodiode.enabled)
            || (pd.enabled
                && (pd.position != scene_cache.photodiode.position
                    || screen_size != scene_cache.photodiode.screen_size));
        if geometry_changed {
            let (pd_verts, pd_idxs) = tessellate_photodiode(pd, screen_size);
            scene_cache.solid.upload(PHOTODIODE_HANDLE, &ctx.device, (&pd_verts, &pd_idxs), (&[], &[]));
            scene_cache.photodiode.enabled     = pd.enabled;
            scene_cache.photodiode.lit         = pd.enabled.then_some(pd.lit);
            scene_cache.photodiode.position    = pd.position;
            scene_cache.photodiode.screen_size = screen_size;
        } else if pd.enabled && scene_cache.photodiode.lit != Some(pd.lit) {
            let (pd_verts, _) = tessellate_photodiode(pd, screen_size);
            scene_cache.solid.overwrite_fill_vertices(PHOTODIODE_HANDLE, &ctx.device, &pd_verts);
            scene_cache.photodiode.lit = Some(pd.lit);
        }
    } // write lock dropped — ZMQ thread can run

    // Upload any new glyph bitmaps to the GPU atlas (no-op when not dirty).
    glyph_atlas.flush(&ctx.device, ctx.graphics_queue, ctx.command_pool);

    let tessellate_us = t_tess_start.elapsed().as_micros() as u32;

    // -- Acquire swapchain image ----------------------------------------------
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
        Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return None,
        Err(e) => panic!("acquire_next_image: {e}"),
    };
    let acquire_us = t_acquire_start.elapsed().as_micros() as u32;

    // -- Record command buffer ------------------------------------------------
    let t_record_start = std::time::Instant::now();
    let cb = frame.command_buffer;
    unsafe {
        ctx.device
            .reset_command_buffer(cb, vk::CommandBufferResetFlags::empty())
            .expect("command buffer reset");
        ctx.device
            .begin_command_buffer(
                cb,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
            .expect("begin_command_buffer");

        let bg = {
            let sc = scene.read().expect("scene lock poisoned");
            vk::ClearColorValue { float32: sc.background.live }
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

        ctx.device.cmd_begin_render_pass(cb, &rp_info, vk::SubpassContents::INLINE);

        let sc = scene.read().expect("scene lock poisoned");
        let screen_w = ctx.extent.width as f32;
        let screen_h = ctx.extent.height as f32;

        let viewport = vk::Viewport {
            x: 0.0, y: 0.0,
            width: screen_w, height: screen_h,
            min_depth: 0.0, max_depth: 1.0,
        };

        #[derive(PartialEq)]
        enum Bound { None, Solid, Grating, Text }
        let mut bound = Bound::None;
        let quad = &grating_pipeline.quad;

        ctx.cmd_begin_label(cb, "stimuli", [0.3, 0.7, 1.0, 1.0]);
        for (h, entry) in &sc.stimuli {
            let stim = &entry.stimulus;
            if !stim.is_visible() {
                continue;
            }

            if let Stimulus::Grating(s) = stim {
                if bound != Bound::Grating {
                    ctx.device.cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, grating_pipeline.pipeline);
                    ctx.device.cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
                    ctx.device.cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));
                    ctx.device.cmd_bind_vertex_buffers(cb, 0, &[quad.vertex_buffer], &[0]);
                    ctx.device.cmd_bind_index_buffer(cb, quad.index_buffer, 0, vk::IndexType::UINT32);
                    bound = Bound::Grating;
                }
                let pc = build_grating_push_constants(s, screen_w, screen_h);
                ctx.device.cmd_push_constants(
                    cb, grating_pipeline.layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0, bytemuck::bytes_of(&pc),
                );
                ctx.device.cmd_draw_indexed(cb, quad.index_count, 1, 0, 0, 0);

            } else if let Stimulus::Text(t) = stim {
                if bound != Bound::Text {
                    ctx.device.cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, text_pipeline.pipeline);
                    ctx.device.cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
                    ctx.device.cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));
                    ctx.device.cmd_bind_descriptor_sets(
                        cb, vk::PipelineBindPoint::GRAPHICS,
                        text_pipeline.layout, 0,
                        &[glyph_atlas.descriptor_set], &[],
                    );
                    bound = Bound::Text;
                }
                ctx.device.cmd_push_constants(
                    cb, text_pipeline.layout,
                    vk::ShaderStageFlags::FRAGMENT,
                    0, bytemuck::bytes_of(&TextPushConstants { color: t.params.live.color }),
                );
                if let Some(mesh) = scene_cache.text.meshes.get(h).filter(|m| m.index_count > 0) {
                    ctx.device.cmd_bind_vertex_buffers(cb, 0, &[mesh.vertex_buffer], &[0]);
                    ctx.device.cmd_bind_index_buffer(cb, mesh.index_buffer, 0, vk::IndexType::UINT32);
                    ctx.device.cmd_draw_indexed(cb, mesh.index_count, 1, 0, 0, 0);
                }

            } else {
                let draw_mode = match stim {
                    Stimulus::Shape(s) => s.appearance().live.draw_mode,
                    _                  => DrawMode::Fill,
                };
                let draw_fill   = matches!(draw_mode, DrawMode::Fill | DrawMode::FillAndStroke);
                let draw_stroke = matches!(draw_mode, DrawMode::Stroke | DrawMode::FillAndStroke);

                if (draw_fill || draw_stroke) && bound != Bound::Solid {
                    ctx.device.cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, pipeline.pipeline);
                    ctx.device.cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
                    ctx.device.cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));
                    bound = Bound::Solid;
                }
                if draw_fill
                    && let Some(mesh) = scene_cache.solid.fill_meshes.get(h).filter(|m| m.index_count > 0)
                {
                    ctx.device.cmd_bind_vertex_buffers(cb, 0, &[mesh.vertex_buffer], &[0]);
                    ctx.device.cmd_bind_index_buffer(cb, mesh.index_buffer, 0, vk::IndexType::UINT32);
                    ctx.device.cmd_draw_indexed(cb, mesh.index_count, 1, 0, 0, 0);
                }
                if draw_stroke
                    && let Some(mesh) = scene_cache.solid.stroke_meshes.get(h).filter(|m| m.index_count > 0)
                {
                    ctx.device.cmd_bind_vertex_buffers(cb, 0, &[mesh.vertex_buffer], &[0]);
                    ctx.device.cmd_bind_index_buffer(cb, mesh.index_buffer, 0, vk::IndexType::UINT32);
                    ctx.device.cmd_draw_indexed(cb, mesh.index_count, 1, 0, 0, 0);
                }
            }
        }

        // Photodiode drawn on top, always solid.
        if let Some(m) = scene_cache.solid.fill_meshes.get(&PHOTODIODE_HANDLE).filter(|m| m.index_count > 0) {
            if bound != Bound::Solid {
                ctx.device.cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, pipeline.pipeline);
                ctx.device.cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
                ctx.device.cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));
            }
            ctx.device.cmd_bind_vertex_buffers(cb, 0, &[m.vertex_buffer], &[0]);
            ctx.device.cmd_bind_index_buffer(cb, m.index_buffer, 0, vk::IndexType::UINT32);
            ctx.device.cmd_draw_indexed(cb, m.index_count, 1, 0, 0, 0);
        }
        ctx.cmd_end_label(cb);

        ctx.device.cmd_end_render_pass(cb);

        // -- Optional egui overlay pass ---------------------------------------
        if let (Some(renderer), Some(data)) = (egui_renderer.as_mut(), egui_data.as_ref()) {
            renderer.update_textures(
                &ctx.device, ctx.graphics_queue, ctx.command_pool, data.textures_delta,
            );
            renderer.upload_meshes(&ctx.device, data.primitives, data.pixels_per_point);

            let egui_rp_info = vk::RenderPassBeginInfo::default()
                .render_pass(ctx.egui_render_pass)
                .framebuffer(ctx.framebuffers[image_index as usize])
                .render_area(render_area);
            ctx.device.cmd_begin_render_pass(cb, &egui_rp_info, vk::SubpassContents::INLINE);

            ctx.cmd_begin_label(cb, "egui overlay", [0.5, 0.9, 0.3, 1.0]);
            renderer.paint(
                &ctx.device, cb, data.primitives,
                (ctx.extent.width, ctx.extent.height), data.pixels_per_point,
            );
            ctx.cmd_end_label(cb);

            ctx.device.cmd_end_render_pass(cb);
        }

        ctx.device.end_command_buffer(cb).expect("end_command_buffer");
    }
    let record_us = t_record_start.elapsed().as_micros() as u32;

    // -- Submit ---------------------------------------------------------------
    let t_submit_start = std::time::Instant::now();
    unsafe {
        ctx.device.reset_fences(&[frame.in_flight]).expect("fence reset");
    }
    let wait_sems   = [frame.image_available];
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
                this_present_id, tessellate_us, fence_us, acquire_us, record_us
            );
            std::process::exit(1);
        }
    }

    // -- Present --------------------------------------------------------------
    let present_ids = [this_present_id];
    let mut present_id_ext = vk::PresentIdKHR::default().present_ids(&present_ids);
    let swapchains         = [ctx.swapchain];
    let image_indices_arr  = [image_index];
    let mut present_info   = vk::PresentInfoKHR::default()
        .wait_semaphores(&signal_sems)
        .swapchains(&swapchains)
        .image_indices(&image_indices_arr);
    if ctx.present_wait.is_some() {
        present_info = present_info.push_next(&mut present_id_ext);
    }
    let present_ok = unsafe {
        ctx.swapchain_loader.queue_present(ctx.graphics_queue, &present_info)
    };
    match present_ok {
        Ok(_) | Err(vk::Result::SUBOPTIMAL_KHR) => {}
        Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
            *frame_index = frame_index.wrapping_add(1);
            return None;
        }
        Err(e) => panic!("queue_present: {e}"),
    }

    let submit_us = t_submit_start.elapsed().as_micros() as u32;

    let dropped_frames = frame_stats.on_present(vblank_time);
    if dropped_frames > 0 {
        log::warn!(
            "vstimd: {} dropped frame(s) before frame {} \
             [tess={}µs fence={}µs acquire={}µs record={}µs submit={}µs]",
            dropped_frames, this_present_id,
            tessellate_us, fence_us, acquire_us, record_us, submit_us
        );
    }
    *frame_index = frame_index.wrapping_add(1);
    ctx.next_present_id.set(this_present_id + 1);

    Some(FrameTick {
        frame: this_present_id,
        vblank_time,
        dropped_frames,
        phases: FramePhases {
            tessellate_us, fence_us, acquire_us, record_us, submit_us,
        },
    })
}
