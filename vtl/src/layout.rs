use std::sync::atomic::AtomicU64;

pub const MAX_BANKS: usize = 4;
pub const MAX_NAMED_LINES: usize = 256;

pub const MAGIC: u32 = 0x56544C31; // "VTL1"
pub const VERSION: u32 = 1;

// Offsets within the shm segment
pub const HEADER_OFFSET: usize = 0;
pub const NAMES_OFFSET: usize = 128;
// Named table: 64-byte prefix + 256 × 60-byte entries = 15424 bytes
// 128 + 15424 = 15552 → round up to 4096 boundary → 16384
pub const STATE_OFFSET: usize = 0x4000;
pub const SHM_SIZE: usize = 0x5000; // 5 pages, covers state section

/// Direction of a VTL line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Direction {
    Input  = 0,
    Output = 1,
}

impl Direction {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Input),
            1 => Some(Self::Output),
            _ => None,
        }
    }
}

/// Header at offset 0 (128 bytes, #[repr(C)]).
///
/// Written once by the owner at creation time; read by all clients.
#[repr(C)]
pub struct VtlHeader {
    pub magic:            u32,
    pub version:          u32,
    pub num_input_banks:  u32,
    pub num_output_banks: u32,
    /// Reserved seqlock counter for future multi-bank consistency. Unused in v1.
    pub seqlock:          AtomicU64,
    pub _pad:             [u8; 104],
}

const _: () = assert!(std::mem::size_of::<VtlHeader>() == 128);

/// One named line entry (60 bytes).
#[repr(C)]
pub struct VtlLineEntry {
    /// Null-terminated UTF-8 name, or all-zero if unused.
    pub name:      [u8; 56],
    pub bank:      u8,
    pub bit:       u8,
    pub direction: u8,
    pub _pad:      u8,
}

const _: () = assert!(std::mem::size_of::<VtlLineEntry>() == 60);

impl VtlLineEntry {
    pub fn name_str(&self) -> &str {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(56);
        std::str::from_utf8(&self.name[..end]).unwrap_or("")
    }

    pub fn is_used(&self) -> bool {
        self.name[0] != 0
    }
}

/// Names section at offset NAMES_OFFSET.
///
/// 64-byte header + 256 entries × 60 bytes = 15424 bytes total.
#[repr(C)]
pub struct VtlNamesSection {
    pub n_entries: u32,
    pub _pad:      [u8; 60],
    pub entries:   [VtlLineEntry; MAX_NAMED_LINES],
}

const _: () = assert!(std::mem::size_of::<VtlNamesSection>() == 15424);

/// Five arrays of atomics, one per bank. Cache-line aligned at STATE_OFFSET.
#[repr(C)]
pub struct VtlStateSection {
    /// Current input levels — nidaqd writes, vstimd reads.
    pub input_state:      [AtomicU64; MAX_BANKS],
    /// Sticky rising edge latches — nidaqd `fetch_or`s, vstimd `fetch_and`-clears.
    pub input_rise_latch: [AtomicU64; MAX_BANKS],
    /// Sticky falling edge latches.
    pub input_fall_latch: [AtomicU64; MAX_BANKS],
    /// Output line levels — vstimd writes, nidaqd reads.
    pub output_state:     [AtomicU64; MAX_BANKS],
    /// One-shot output pulses — vstimd OR-sets, nidaqd clears after driving hardware.
    pub output_set_pulse: [AtomicU64; MAX_BANKS],
}

const _: () = assert!(std::mem::size_of::<VtlStateSection>() == 160);
