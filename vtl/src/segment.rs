use std::sync::atomic::Ordering;

use crate::layout::{
    Direction, VtlHeader, VtlLineEntry, VtlNamesSection, VtlStateSection,
    MAX_BANKS, MAX_NAMED_LINES, NAMES_OFFSET, STATE_OFFSET,
};

/// Raw access to a VTL shared memory segment.
///
/// Obtained via [`VtlOwner`](crate::VtlOwner) or [`VtlClient`](crate::VtlClient).
/// All methods are safe to call from multiple threads because every access goes
/// through `AtomicU64`.
pub struct VtlSegment {
    pub(crate) ptr:  *mut u8,
    pub(crate) size: usize,
}

// SAFETY: the raw ptr is stable (mmap never moves).
// State-section fields are exclusively AtomicU64, so concurrent access is safe.
// Names-section writes go through raw-pointer ops (no &mut reference is ever
// created), and n_entries is AtomicU32 with Release/Acquire ordering.  Callers
// must serialize writes to the entry fields themselves (protocol: owner writes
// during setup, clients read-only afterwards).
unsafe impl Send for VtlSegment {}
unsafe impl Sync for VtlSegment {}

impl VtlSegment {
    fn header(&self) -> &VtlHeader {
        unsafe { &*(self.ptr as *const VtlHeader) }
    }

    fn names(&self) -> &VtlNamesSection {
        unsafe { &*(self.ptr.add(NAMES_OFFSET) as *const VtlNamesSection) }
    }

    fn names_ptr(&self) -> *mut VtlNamesSection {
        unsafe { self.ptr.add(NAMES_OFFSET) as *mut VtlNamesSection }
    }

    fn state(&self) -> &VtlStateSection {
        unsafe { &*(self.ptr.add(STATE_OFFSET) as *const VtlStateSection) }
    }

    // ── Input state ───────────────────────────────────────────────────────────

    pub fn input_state(&self, bank: usize) -> u64 {
        self.state().input_state[bank].load(Ordering::Acquire)
    }

    pub fn set_input_state(&self, bank: usize, val: u64) {
        self.state().input_state[bank].store(val, Ordering::Release);
    }

    // ── Input latches ─────────────────────────────────────────────────────────

    /// Atomically OR `bits` into the rising latch (nidaqd / software-trigger side).
    pub fn set_input_rise(&self, bank: usize, bits: u64) {
        self.state().input_rise_latch[bank].fetch_or(bits, Ordering::AcqRel);
    }

    /// Atomically OR `bits` into the falling latch.
    pub fn set_input_fall(&self, bank: usize, bits: u64) {
        self.state().input_fall_latch[bank].fetch_or(bits, Ordering::AcqRel);
    }

    /// Atomically drain (fetch-and-clear) the rising latch bits indicated by `mask`.
    /// Returns the bits that were set before clearing.
    pub fn drain_input_rise(&self, bank: usize, mask: u64) -> u64 {
        self.state().input_rise_latch[bank].fetch_and(!mask, Ordering::AcqRel) & mask
    }

    /// Atomically drain the falling latch bits indicated by `mask`.
    pub fn drain_input_fall(&self, bank: usize, mask: u64) -> u64 {
        self.state().input_fall_latch[bank].fetch_and(!mask, Ordering::AcqRel) & mask
    }

    /// Read current rising latch value without clearing.
    pub fn peek_input_rise(&self, bank: usize) -> u64 {
        self.state().input_rise_latch[bank].load(Ordering::Acquire)
    }

    /// Read current falling latch value without clearing.
    pub fn peek_input_fall(&self, bank: usize) -> u64 {
        self.state().input_fall_latch[bank].load(Ordering::Acquire)
    }

    // ── Output state ──────────────────────────────────────────────────────────

    pub fn output_state(&self, bank: usize) -> u64 {
        self.state().output_state[bank].load(Ordering::Acquire)
    }

    pub fn set_output_state(&self, bank: usize, val: u64) {
        self.state().output_state[bank].store(val, Ordering::Release);
    }

    /// Atomically OR `bits` into the one-shot output pulse word (vstimd side).
    pub fn set_output_pulse(&self, bank: usize, bits: u64) {
        self.state().output_set_pulse[bank].fetch_or(bits, Ordering::AcqRel);
    }

    /// Atomically drain output pulse bits (nidaqd side: call after driving hardware).
    pub fn drain_output_pulse(&self, bank: usize, mask: u64) -> u64 {
        self.state().output_set_pulse[bank].fetch_and(!mask, Ordering::AcqRel) & mask
    }

    pub fn peek_output_pulse(&self, bank: usize) -> u64 {
        self.state().output_set_pulse[bank].load(Ordering::Acquire)
    }

    // ── Named line table ──────────────────────────────────────────────────────

    pub fn n_named_lines(&self) -> usize {
        (self.names().n_entries.load(Ordering::Acquire) as usize).min(MAX_NAMED_LINES)
    }

    pub fn named_line(&self, idx: usize) -> Option<(&VtlLineEntry, Direction)> {
        if idx >= self.n_named_lines() {
            return None;
        }
        let entry = &self.names().entries[idx];
        let dir = Direction::from_u8(entry.direction)?;
        Some((entry, dir))
    }

    /// Find a named line by name. Returns (index, entry, direction) or None.
    pub fn find_named_line(&self, name: &str) -> Option<(usize, &VtlLineEntry, Direction)> {
        let n = self.n_named_lines().min(MAX_NAMED_LINES);
        for i in 0..n {
            let e = &self.names().entries[i];
            if e.name_str() == name {
                let dir = Direction::from_u8(e.direction)?;
                return Some((i, e, dir));
            }
        }
        None
    }

    /// Set or update a named line entry at `idx`. Writes into the shm names table.
    ///
    /// Uses raw-pointer writes so no `&mut` reference is ever created over shared
    /// memory, avoiding aliasing UB with concurrent `&`-reads on the same segment.
    pub fn write_named_line(&self, idx: usize, name: &str, bank: u8, bit: u8, dir: Direction) {
        assert!(idx < MAX_NAMED_LINES);
        let bytes = name.as_bytes();
        let len = bytes.len().min(55);
        unsafe {
            let base = self.names_ptr();
            let entry = std::ptr::addr_of_mut!((*base).entries[idx]);
            let name_ptr = std::ptr::addr_of_mut!((*entry).name) as *mut u8;
            std::ptr::write_bytes(name_ptr, 0, 56);
            if len > 0 {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), name_ptr, len);
            }
            std::ptr::write(std::ptr::addr_of_mut!((*entry).bank),      bank);
            std::ptr::write(std::ptr::addr_of_mut!((*entry).bit),       bit);
            std::ptr::write(std::ptr::addr_of_mut!((*entry).direction), dir as u8);
            std::ptr::write(std::ptr::addr_of_mut!((*entry)._pad),      0u8);
        }
    }

    /// Clear a named line entry (zero out the name field).
    pub fn clear_named_line(&self, idx: usize) {
        assert!(idx < MAX_NAMED_LINES);
        unsafe {
            let base = self.names_ptr();
            let name_ptr = std::ptr::addr_of_mut!((*base).entries[idx].name) as *mut u8;
            std::ptr::write_bytes(name_ptr, 0, 56);
        }
    }

    /// Publish the number of valid entries with Release ordering so all preceding
    /// `write_named_line` stores are visible to any reader that Acquire-loads this.
    pub fn set_n_named_lines(&self, n: usize) {
        self.names().n_entries.store(n.min(MAX_NAMED_LINES) as u32, Ordering::Release);
    }

    // ── Header ────────────────────────────────────────────────────────────────

    pub fn num_input_banks(&self) -> u32 {
        unsafe { std::ptr::read_volatile(&self.header().num_input_banks) }
    }

    pub fn num_output_banks(&self) -> u32 {
        unsafe { std::ptr::read_volatile(&self.header().num_output_banks) }
    }

    /// Returns `true` if the magic and version fields are valid.
    pub fn is_valid(&self) -> bool {
        let h = self.header();
        unsafe {
            std::ptr::read_volatile(&h.magic)   == crate::layout::MAGIC
                && std::ptr::read_volatile(&h.version) == crate::layout::VERSION
        }
    }

    // ── Atomic bit helpers ────────────────────────────────────────────────────

    /// Atomically set one bit of `input_state[bank]`.
    /// Returns `true` if the bit was previously clear (rising edge).
    pub fn set_input_bit(&self, bank: usize, bit: u8) -> bool {
        assert!(bank < MAX_BANKS, "bank must be < {}", MAX_BANKS);
        assert!(bit < 64, "bit must be 0..63");
        let mask = 1u64 << bit;
        self.state().input_state[bank].fetch_or(mask, Ordering::AcqRel) & mask == 0
    }

    /// Atomically clear one bit of `input_state[bank]`.
    /// Returns `true` if the bit was previously set (falling edge).
    pub fn clear_input_bit(&self, bank: usize, bit: u8) -> bool {
        assert!(bank < MAX_BANKS, "bank must be < {}", MAX_BANKS);
        assert!(bit < 64, "bit must be 0..63");
        let mask = 1u64 << bit;
        self.state().input_state[bank].fetch_and(!mask, Ordering::AcqRel) & mask != 0
    }

    /// Atomically toggle one bit of `input_state[bank]`.
    /// Returns `true` if the bit transitioned low→high, `false` for high→low.
    pub fn toggle_input_bit(&self, bank: usize, bit: u8) -> bool {
        assert!(bank < MAX_BANKS, "bank must be < {}", MAX_BANKS);
        assert!(bit < 64, "bit must be 0..63");
        let mask = 1u64 << bit;
        self.state().input_state[bank].fetch_xor(mask, Ordering::AcqRel) & mask == 0
    }
}
