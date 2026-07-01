//! Runtime lifecycle state of an animation.

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AnimState {
    Idle,
    Armed,
    Running { frame_counter: u32 },
    Done,
}
