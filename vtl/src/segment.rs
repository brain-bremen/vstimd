use std::sync::atomic::Ordering;

use crate::layout::{
    Direction, VtlHeader, VtlLineEntry, VtlNamesSection, VtlStateSection,
    MAX_BANKS, NAMES_OFFSET, STATE_OFFSET,
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

// SAFETY: all mutable state is accessed through AtomicU64; the raw ptr is stable.
unsafe impl Send for VtlSegment {}
unsafe impl Sync for VtlSegment {}

impl VtlSegment {
    fn header(&self) -> &VtlHeader {
        unsafe { &*(self.ptr as *const VtlHeader) }
    }

    fn names(&self) -> &VtlNamesSection {
        unsafe { &*(self.ptr.add(NAMES_OFFSET) as *const VtlNamesSection) }
    }

    fn names_mut(&self) -> &mut VtlNamesSection {
        unsafe { &mut *(self.ptr.add(NAMES_OFFSET) as *mut VtlNamesSection) }
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
        // Relaxed read of a u32; written only under protocol setup, not per-frame.
        unsafe { std::ptr::read_volatile(&self.names().n_entries) as usize }
    }

    pub fn named_line(&self, idx: usize) -> Option<(&VtlLineEntry, Direction)> {
        if idx >= MAX_BANKS * 64 {
            return None;
        }
        let entry = &self.names().entries[idx];
        let dir = Direction::from_u8(entry.direction)?;
        Some((entry, dir))
    }

    /// Find a named line by name. Returns (index, entry, direction) or None.
    pub fn find_named_line(&self, name: &str) -> Option<(usize, &VtlLineEntry, Direction)> {
        let n = self.n_named_lines().min(MAX_BANKS * 64);
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
    pub fn write_named_line(&self, idx: usize, name: &str, bank: u8, bit: u8, dir: Direction) {
        assert!(idx < MAX_BANKS * 64);
        let entry = &mut self.names_mut().entries[idx];
        entry.name = [0u8; 56];
        let bytes = name.as_bytes();
        let len = bytes.len().min(55);
        entry.name[..len].copy_from_slice(&bytes[..len]);
        entry.bank      = bank;
        entry.bit       = bit;
        entry.direction = dir as u8;
        entry._pad      = 0;
    }

    /// Clear a named line entry (zero out the name field).
    pub fn clear_named_line(&self, idx: usize) {
        assert!(idx < MAX_BANKS * 64);
        self.names_mut().entries[idx].name = [0u8; 56];
    }

    pub fn set_n_named_lines(&self, n: usize) {
        unsafe {
            std::ptr::write_volatile(
                &mut self.names_mut().n_entries,
                n.min(MAX_BANKS * 64) as u32,
            );
        }
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
}
