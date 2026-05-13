use std::sync::{Arc, RwLock};

use ash::vk;

use crate::render::tess::{self, tessellate_photodiode};
use crate::scene::stimulus::Stimulus;

const PHOTODIODE_HANDLE: u32 = u32::MAX;
use crate::scene::SceneState;
use crate::timing::{FramePhases, FrameStats, FrameTick};

use super::buffers::GpuBuffers;
use super::context::VkContext;
use super::egui::VkEguiRenderer;
use super::pipeline::VkPipeline;
use crate::scene::stimulus::grating::{
    GratingPushConstants, VkGratingPipeline, build_grating_push_constants, grating_phase_inc,
};

/// Optional egui overlay data for a frame
pub struct EguiFrameData<'a> {
    pub textures_delta: &'a egui::TexturesDelta,
    pub primitives: &'a [egui::ClippedPrimitive],
    pub pixels_per_point: f32,
}

/// Record and submit one frame.
///
/// Returns `Some(FrameTick)` on success. The tick contains the vblank
/// timestamp and dropped-frame count for this frame — together these form
/// the time axis of the stimulus server.
///
/// Returns `None` when the swapchain is out of date; the caller must call
/// `ctx.recreate_swapchain(new_extent)` before the next call.
#[allow(clippy::too_many_arguments)]
pub fn render_frame(
    ctx: &VkContext,
    pipeline: &VkPipeline,
    grating_pipeline: &VkGratingPipeline,
    gpu_buffers: &mut GpuBuffers,
    scene: &Arc<RwLock<SceneState>>,
    frame_index: &mut usize,
    frame_stats: &mut FrameStats,
    mut egui_renderer: Option<&mut VkEguiRenderer>,
    egui_data: Option<EguiFrameData>,
    screen_clock: Option<std::time::Instant>,
) -> Option<FrameTick> {
    let this_present_id = ctx.next_present_id.get();

    // -- Wait for this slot's previous GPU work (must precede upload) ----------
    // Moved before tessellation so gpu_buffers.upload() (which destroys old
    // vk::Buffer/DeviceMemory) is only called once the GPU has finished reading
    // the previous frame's buffers.
    let frame = &ctx.frames[*frame_index % ctx.frames.len()];
    let t_fence_start = std::time::Instant::now();
    unsafe {
        ctx.device
            .wait_for_fences(&[frame.in_flight], true, u64::MAX)
            .expect("fence wait");
        // NOTE: do NOT reset the fence here. If acquire_next_image fails with
        // OUT_OF_DATE below, we return early without ever calling queue_submit,
        // which means the fence would stay reset-but-never-signaled. The next
        // call to render_frame would then wait on it forever. Reset only after
        // a successful acquire, immediately before queue_submit.
    }
    let fence_us = t_fence_start.elapsed().as_micros() as u32;

    // ── Screen clock ──────────────────────────────────────────────────────────
    // Priority 1: DRM vblank time pre-computed by the caller (DRM mode).
    // Priority 2: VK_KHR_present_wait (winit mode when extension is present).
    // Priority 3: Instant::now() — GPU-completion time only (inaccurate).
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
        let mut sc = scene.write().expect("scene lock poisoned");
        if sc.pending_flip {
            sc.apply_flip();
        }
        sc.screen_size = Some(screen_size);
        sc.frame_rate = fps;
        // When the screen size changes all NDC coordinates are stale.
        if sc.last_uploaded_size != screen_size {
            sc.last_uploaded_size = screen_size;
            for stim in sc.stimuli.values_mut() {
                stim.flags_mut().mark_dirty();
            }
        }
        gpu_buffers
            .meshes
            .retain(|h, _| *h == PHOTODIODE_HANDLE || sc.stimuli.contains_key(h));
        let handles: Vec<u32> = sc.stimuli.keys().copied().collect();
        for handle in handles {
            // Advance drift accumulator for visible gratings before tessellation.
            if let Some(Stimulus::Grating(s)) = sc.stimuli.get_mut(&handle)
                && s.flags.is_visible()
                && s.params.live.drift_speed != 0.0
            {
                let inc = grating_phase_inc(s, fps);
                s.phase_accum += inc;
            }

            let stim = &sc.stimuli[&handle];
            if !stim.flags().dirty && gpu_buffers.meshes.contains_key(&handle) {
                continue;
            }
            let (verts, idxs) = tess::tessellate_stimulus(stim, screen_size);
            log::debug!(
                "tess #{handle} {} screen={screen_size:?} verts={} idxs={}{}",
                stim.type_name(),
                verts.len(),
                idxs.len(),
                if let Stimulus::Grating(s) = stim {
                    format!(
                        " pos={:?} size={:?} enabled={}",
                        s.transform.live.pos, s.size.live, s.flags.enabled
                    )
                } else {
                    String::new()
                }
            );
            gpu_buffers.upload(handle, &ctx.device, &verts, &idxs);
            sc.stimuli[&handle].flags_mut().dirty = false;
        }
        sc.photodiode.advance();
        let pd = &sc.photodiode;
        let geometry_changed = (pd.enabled != gpu_buffers.pd_enabled)
            || (pd.enabled
                && (pd.position != gpu_buffers.pd_position
                    || screen_size != gpu_buffers.pd_screen_size));
        if geometry_changed {
            let (pd_verts, pd_idxs) = tessellate_photodiode(pd, screen_size);
            gpu_buffers.upload(PHOTODIODE_HANDLE, &ctx.device, &pd_verts, &pd_idxs);
            gpu_buffers.pd_enabled = pd.enabled;
            gpu_buffers.pd_lit = pd.enabled.then_some(pd.lit);
            gpu_buffers.pd_position = pd.position;
            gpu_buffers.pd_screen_size = screen_size;
        } else if pd.enabled && gpu_buffers.pd_lit != Some(pd.lit) {
            let (pd_verts, _) = tessellate_photodiode(pd, screen_size);
            gpu_buffers.overwrite_vertices(PHOTODIODE_HANDLE, &ctx.device, &pd_verts);
            gpu_buffers.pd_lit = Some(pd.lit);
        }
    } // write lock dropped — ZMQ thread can run
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
        Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return None, // fence still signaled
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
            vk::ClearColorValue {
                float32: sc.background.live,
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

        // Collect draws, separating solid stimuli from gratings.
        // Gratings carry push constants computed from the current scene state.
        let sc = scene.read().expect("scene lock poisoned");
        let screen_w = ctx.extent.width as f32;
        let screen_h = ctx.extent.height as f32;

        let mut solid_draws: Vec<(vk::Buffer, vk::Buffer, u32)> = Vec::new();
        let mut grating_draws: Vec<(vk::Buffer, vk::Buffer, u32, GratingPushConstants)> =
            Vec::new();

        for (h, stim) in &sc.stimuli {
            if !stim.is_visible() {
                continue;
            }
            if let Some(mesh) = gpu_buffers.meshes.get(h).filter(|m| m.index_count > 0) {
                if let Stimulus::Grating(s) = stim {
                    let pc = build_grating_push_constants(s, screen_w, screen_h);
                    grating_draws.push((
                        mesh.vertex_buffer,
                        mesh.index_buffer,
                        mesh.index_count,
                        pc,
                    ));
                } else {
                    solid_draws.push((mesh.vertex_buffer, mesh.index_buffer, mesh.index_count));
                }
            }
        }
        // Photodiode is always solid.
        if let Some(m) = gpu_buffers
            .meshes
            .get(&PHOTODIODE_HANDLE)
            .filter(|m| m.index_count > 0)
        {
            solid_draws.push((m.vertex_buffer, m.index_buffer, m.index_count));
        }
        drop(sc);

        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: ctx.extent.width as f32,
            height: ctx.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };

        // ── Pass 1: solid stimuli ─────────────────────────────────────────────
        ctx.cmd_begin_label(cb, "solid stimuli", [0.3, 0.7, 1.0, 1.0]);
        if !solid_draws.is_empty() {
            ctx.device
                .cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, pipeline.pipeline);
            ctx.device
                .cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
            ctx.device
                .cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));

            for (vbuf, ibuf, index_count) in solid_draws {
                ctx.device.cmd_bind_vertex_buffers(cb, 0, &[vbuf], &[0]);
                ctx.device
                    .cmd_bind_index_buffer(cb, ibuf, 0, vk::IndexType::UINT32);
                ctx.device.cmd_draw_indexed(cb, index_count, 1, 0, 0, 0);
            }
        }
        ctx.cmd_end_label(cb);

        // ── Pass 2: grating stimuli ───────────────────────────────────────────
        ctx.cmd_begin_label(cb, "grating stimuli", [0.8, 0.5, 0.1, 1.0]);
        if !grating_draws.is_empty() {
            if let Some((_, _, _, pc)) = grating_draws.first() {
                log::debug!(
                    "draw {} gratings: first center={:?} half_size={:?} screen_half={:?} contrast={} sf={}",
                    grating_draws.len(),
                    pc.center_px,
                    pc.half_size,
                    pc.screen_half,
                    pc.contrast,
                    pc.sf
                );
            }
            ctx.device.cmd_bind_pipeline(
                cb,
                vk::PipelineBindPoint::GRAPHICS,
                grating_pipeline.pipeline,
            );
            ctx.device
                .cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
            ctx.device
                .cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));

            for (vbuf, ibuf, index_count, pc) in grating_draws {
                let pc_bytes: &[u8] = bytemuck::bytes_of(&pc);
                ctx.device.cmd_push_constants(
                    cb,
                    grating_pipeline.layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    pc_bytes,
                );
                ctx.device.cmd_bind_vertex_buffers(cb, 0, &[vbuf], &[0]);
                ctx.device
                    .cmd_bind_index_buffer(cb, ibuf, 0, vk::IndexType::UINT32);
                ctx.device.cmd_draw_indexed(cb, index_count, 1, 0, 0, 0);
            }
        }
        ctx.cmd_end_label(cb);

        ctx.device.cmd_end_render_pass(cb);

        // -- Optional egui overlay pass ---------------------------------------
        if let (Some(renderer), Some(data)) = (egui_renderer.as_mut(), egui_data.as_ref()) {
            // Update textures
            renderer.update_textures(
                &ctx.device,
                ctx.graphics_queue,
                ctx.command_pool,
                data.textures_delta,
            );

            // Upload mesh data
            renderer.upload_meshes(&ctx.device, data.primitives, data.pixels_per_point);

            // Begin egui render pass (LOADs existing color attachment)
            let egui_rp_info = vk::RenderPassBeginInfo::default()
                .render_pass(ctx.egui_render_pass)
                .framebuffer(ctx.framebuffers[image_index as usize])
                .render_area(render_area);

            ctx.device
                .cmd_begin_render_pass(cb, &egui_rp_info, vk::SubpassContents::INLINE);

            ctx.cmd_begin_label(cb, "egui overlay", [0.5, 0.9, 0.3, 1.0]);
            // Paint egui
            renderer.paint(
                &ctx.device,
                cb,
                data.primitives,
                (ctx.extent.width, ctx.extent.height),
                data.pixels_per_point,
            );
            ctx.cmd_end_label(cb);

            ctx.device.cmd_end_render_pass(cb);
        }

        ctx.device
            .end_command_buffer(cb)
            .expect("end_command_buffer");
    }
    let record_us = t_record_start.elapsed().as_micros() as u32;

    // -- Submit ---------------------------------------------------------------
    let t_submit_start = std::time::Instant::now();
    // Reset the fence here — after a successful acquire — so that an early
    // return above (OUT_OF_DATE) never leaves it in the reset-but-unsignaled
    // state that would deadlock the next wait_for_fences call.
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

    // -- Present --------------------------------------------------------------
    // Tag with a monotonic ID so vkWaitForPresentKHR can block on it at the
    // start of the NEXT frame (the screen-clock tick).
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
    let present_ok = unsafe {
        ctx.swapchain_loader
            .queue_present(ctx.graphics_queue, &present_info)
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
            dropped_frames,
            this_present_id,
            tessellate_us,
            fence_us,
            acquire_us,
            record_us,
            submit_us
        );
    }
    *frame_index = frame_index.wrapping_add(1);
    ctx.next_present_id.set(this_present_id + 1);

    Some(FrameTick {
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
    })
}
