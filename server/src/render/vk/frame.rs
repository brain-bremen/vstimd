use std::sync::{Arc, RwLock};

use ash::vk;

use crate::render::tess::{self, tessellate_photodiode};

const PHOTODIODE_HANDLE: u32 = u32::MAX;
use crate::scene::SceneState;
use crate::timing::{FrameStats, FrameTick};

use super::buffers::GpuBuffers;
use super::context::VkContext;
use super::egui::VkEguiRenderer;
use super::pipeline::VkPipeline;

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
pub fn render_frame(
    ctx: &VkContext,
    pipeline: &VkPipeline,
    gpu_buffers: &mut GpuBuffers,
    scene: &Arc<RwLock<SceneState>>,
    frame_index: &mut usize,
    frame_stats: &mut FrameStats,
    mut egui_renderer: Option<&mut VkEguiRenderer>,
    egui_data: Option<EguiFrameData>,
) -> Option<FrameTick> {
    // ── Waitable screen clock (VK_KHR_present_wait) ───────────────────────────
    // Block until the previously presented frame is confirmed on-screen.
    // The Instant captured here is the best available proxy for the vblank
    // that just fired — this is the "tick" that starts work on the new frame.
    // On the first call (no prior present) we fall through immediately and
    // use the current time as a starting-point approximation.
    let this_present_id = ctx.next_present_id.get();
    let vblank_time = if let (Some(pw), true) = (&ctx.present_wait, this_present_id > 1) {
        unsafe {
            match pw.wait_for_present(ctx.swapchain, this_present_id - 1, 3_000_000_000) {
                Ok(()) => {}
                Err(vk::Result::TIMEOUT) => {
                    eprintln!(
                        "wonderlamp: vkWaitForPresentKHR timed out (id {})",
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
        // First frame or no present_wait support: use current time.
        std::time::Instant::now()
    };
    // -- Update: tessellate scene into GPU buffers ----------------------------
    {
        let fps = frame_stats.summary().fps as f32;
        let screen_size = (ctx.extent.width, ctx.extent.height);
        let mut sc = scene.write().expect("scene lock poisoned");
        if sc.pending_flip {
            sc.apply_flip();
        }
        sc.screen_size = screen_size;
        sc.frame_rate = fps;
        gpu_buffers.meshes.retain(|h, _| *h == PHOTODIODE_HANDLE || sc.stimuli.contains_key(h));
        let handles: Vec<u32> = sc.stimuli.keys().copied().collect();
        for handle in handles {
            let (verts, idxs) = tess::tessellate_stimulus(&sc.stimuli[&handle], screen_size);
            gpu_buffers.upload(handle, &ctx.device, &verts, &idxs);
        }
        sc.photodiode.advance();
        let (pd_verts, pd_idxs) = tessellate_photodiode(&sc.photodiode, screen_size);
        gpu_buffers.upload(PHOTODIODE_HANDLE, &ctx.device, &pd_verts, &pd_idxs);
    } // write lock dropped — ZMQ thread can run

    let frame = &ctx.frames[*frame_index % ctx.frames.len()];

    // -- Wait for this slot's previous GPU work -------------------------------
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

    // -- Acquire swapchain image ----------------------------------------------
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

    // -- Record command buffer ------------------------------------------------
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
        ctx.device
            .cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, pipeline.pipeline);

        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: ctx.extent.width as f32,
            height: ctx.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        ctx.device
            .cmd_set_viewport(cb, 0, std::slice::from_ref(&viewport));
        ctx.device
            .cmd_set_scissor(cb, 0, std::slice::from_ref(&render_area));

        let sc = scene.read().expect("scene lock poisoned");
        for (handle, _) in &sc.stimuli {
            if let Some(mesh) = gpu_buffers.meshes.get(handle) {
                if mesh.index_count > 0 {
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
            }
        }
        // Photodiode corner square is drawn on top of all stimuli.
        if let Some(mesh) = gpu_buffers.meshes.get(&PHOTODIODE_HANDLE) {
            if mesh.index_count > 0 {
                ctx.device
                    .cmd_bind_vertex_buffers(cb, 0, &[mesh.vertex_buffer], &[0]);
                ctx.device.cmd_bind_index_buffer(
                    cb,
                    mesh.index_buffer,
                    0,
                    vk::IndexType::UINT32,
                );
                ctx.device.cmd_draw_indexed(cb, mesh.index_count, 1, 0, 0, 0);
            }
        }
        drop(sc);

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

            // Paint egui
            renderer.paint(
                &ctx.device,
                cb,
                data.primitives,
                (ctx.extent.width, ctx.extent.height),
                data.pixels_per_point,
            );

            ctx.device.cmd_end_render_pass(cb);
        }

        ctx.device
            .end_command_buffer(cb)
            .expect("end_command_buffer");
    }

    // -- Submit ---------------------------------------------------------------
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
        ctx.device
            .queue_submit(
                ctx.graphics_queue,
                &[vk::SubmitInfo::default()
                    .wait_semaphores(&wait_sems)
                    .wait_dst_stage_mask(&wait_stages)
                    .command_buffers(&cbs)
                    .signal_semaphores(&signal_sems)],
                frame.in_flight,
            )
            .expect("queue_submit");
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

    let dropped_frames = frame_stats.on_present(vblank_time);
    if dropped_frames > 0 {
        eprintln!(
            "wonderlamp: {} dropped frame(s) before frame {}",
            dropped_frames, this_present_id
        );
    }
    *frame_index = frame_index.wrapping_add(1);
    ctx.next_present_id.set(this_present_id + 1);

    Some(FrameTick {
        frame: this_present_id,
        vblank_time,
        dropped_frames,
    })
}
