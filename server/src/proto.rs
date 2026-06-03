pub mod vstimd {
    pub mod v1 {
        #![allow(clippy::large_enum_variant)]
        include!(concat!(env!("OUT_DIR"), "/vstimd.v1.rs"));
    }
}

pub use vstimd::v1::*;
