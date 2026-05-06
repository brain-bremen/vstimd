use std::sync::{Arc, RwLock};

use ash::vk;

use crate::render::tess;
use crate::scene::SceneState;
use crate::timing::FrameStats;

use super::buffers::GpuBuffers;
use super::context::VkContext;
use super::pipeline::VkPipeline;

/// Record and submit one frame.
///
/// Returns `false` when the swapchain is out of date and must be recreated
/// before the next call (winit resize / first DRM frame after resolution change).
pub fn render_frame(
    ctx: &VkContext,
    pipeline: &VkPipeline,
    gpu_buffers: &mut GpuBuffers,
    scene: &Arc<RwLock<SceneState>>,
    frame_index: &mut usize,
    frame_stats: &mut FrameStats,
) -> bool {
    // ── Waitable screen clock (VK_KHR_present_wait) ───────────────────────────
    // Block until the previously presented frame is confirmed on-screen.
    // This is the Vulkan equivalent of the D3D11 waitable swap chain:
    // the post-vblank signal that showed frame N-1 is the "tick" that starts
    // work on frame N.  Skip on the very first call (no prior present yet).
    let this_present_id = ctx.next_present_id.get();
    if let (Some(pw), true) = (&ctx.present_wait, this_present_id > 1) {
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
                    return false;
                }
                Err(e) => panic!("vkWaitForPresentKHR: {e}"),
            }
        }
    }
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
        gpu_buffers.meshes.retain(|h, _| sc.stimuli.contains_key(h));
        let handles: Vec<u32> = sc.stimuli.keys().copied().collect();
        for handle in handles {
            let (verts, idxs) = tess::tessellate_stimulus(&sc.stimuli[&handle], screen_size);
            gpu_buffers.upload(handle, &ctx.device, &verts, &idxs);
        }
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
    let (image_index, suboptimal) = match unsafe {
        ctx.swapchain_loader.acquire_next_image(
            ctx.swapchain,
            u64::MAX,
            frame.image_available,
            vk::Fence::null(),
        )
    } {
        Ok(r) => r,
        Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return false, // fence still signaled ✔
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
        drop(sc);

        ctx.device.cmd_end_render_pass(cb);
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
            frame_stats.on_present();
            return false;
        }
        Err(e) => panic!("queue_present: {e}"),
    }

    frame_stats.on_present();
    *frame_index = frame_index.wrapping_add(1);
    ctx.next_present_id.set(this_present_id + 1);

    !suboptimal
}
