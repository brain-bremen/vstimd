pub mod grating_params;
pub mod grating_pipeline;
pub mod grating_proto;
pub mod grating_stimulus;
pub mod grating_tess;

pub use grating_params::{GratingMask, GratingParams, Waveform};
pub use grating_pipeline::{GratingPushConstants, VkGratingPipeline};
pub use grating_proto::{
    grating_params_from_proto, grating_query_params, mask_to_proto, proto_to_mask,
    proto_to_waveform, waveform_to_proto,
};
pub use grating_stimulus::{GratingStimulus, build_grating_push_constants, grating_phase_inc};
pub use grating_tess::tessellate_grating;
