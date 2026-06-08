use std::ffi::CString;
use std::io;
use std::ops::Deref;

use crate::layout::{SHM_SIZE, MAGIC, VERSION, MAX_BANKS};
use crate::segment::VtlSegment;

/// Creates and owns a VTL shared memory segment.
///
/// The segment is created at construction (via `shm_open(O_CREAT)`) and
/// destroyed (via `shm_unlink`) when this value is dropped.
///
/// Use [`VtlClient`](crate::VtlClient) to attach to an existing segment.
pub struct VtlOwner {
    seg:  VtlSegment,
    name: CString,
}

impl VtlOwner {
    /// Create a new VTL segment at `shm_name` (e.g. `"/vstimd_vtl"`).
    ///
    /// `num_input_banks` and `num_output_banks` must each be in the range
    /// `1..=MAX_BANKS` (currently 4).  Use `1` for a minimal single-bank
    /// segment, or a higher value when multiple banks are needed.
    pub fn create(shm_name: &str, num_input_banks: u32, num_output_banks: u32) -> io::Result<Self> {
        if num_input_banks as usize > MAX_BANKS || num_output_banks as usize > MAX_BANKS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("bank counts must be <= {MAX_BANKS}"),
            ));
        }

        #[cfg(not(unix))]
        {
            let _ = (shm_name, num_input_banks, num_output_banks);
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "VTL shared memory is only supported on unix",
            ));
        }

        #[cfg(unix)]
        {
        let name = CString::new(shm_name).expect("shm_name must not contain nul");
        let fd = unsafe {
            libc::shm_open(
                name.as_ptr(),
                libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        if unsafe { libc::ftruncate(fd, SHM_SIZE as libc::off_t) } < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(err);
        }

        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                SHM_SIZE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        // Capture the mmap error before close() can overwrite errno.
        let mmap_err = if ptr == libc::MAP_FAILED { Some(io::Error::last_os_error()) } else { None };
        unsafe { libc::close(fd) };
        if let Some(e) = mmap_err {
            unsafe { libc::shm_unlink(name.as_ptr()) };
            return Err(e);
        }

        let seg = VtlSegment { ptr: ptr as *mut u8, size: SHM_SIZE };

        // Write header (OS zero-initialises the shm, so just fill in non-zero fields).
        unsafe {
            let h = seg.ptr as *mut crate::layout::VtlHeader;
            std::ptr::write_volatile(&mut (*h).magic,            MAGIC);
            std::ptr::write_volatile(&mut (*h).version,          VERSION);
            std::ptr::write_volatile(&mut (*h).num_input_banks,  num_input_banks);
            std::ptr::write_volatile(&mut (*h).num_output_banks, num_output_banks);
        }

        Ok(Self { seg, name })
        }
    }
}

impl Deref for VtlOwner {
    type Target = VtlSegment;
    fn deref(&self) -> &VtlSegment {
        &self.seg
    }
}

impl Drop for VtlOwner {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::munmap(self.seg.ptr as *mut libc::c_void, self.seg.size);
            libc::shm_unlink(self.name.as_ptr());
        }
    }
}
