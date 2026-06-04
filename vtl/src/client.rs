use std::io;
use std::ops::Deref;

use crate::layout::SHM_SIZE;
use crate::segment::VtlSegment;

/// Attaches to an existing VTL shared memory segment created by a [`VtlOwner`](crate::VtlOwner).
///
/// Does not unlink the segment on drop — only unmaps the view.
pub struct VtlClient {
    seg: VtlSegment,
}

impl VtlClient {
    /// Open an existing VTL segment at `shm_name` (e.g. `"/vstimd_vtl"`).
    ///
    /// Returns an error if the segment does not exist or the magic/version are invalid.
    pub fn open(shm_name: &str) -> io::Result<Self> {
        let name = std::ffi::CString::new(shm_name).expect("shm_name must not contain nul");
        let fd = unsafe { libc::shm_open(name.as_ptr(), libc::O_RDWR, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
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
            return Err(e);
        }

        let seg = VtlSegment { ptr: ptr as *mut u8, size: SHM_SIZE };

        if !seg.is_valid() {
            unsafe { libc::munmap(ptr, SHM_SIZE) };
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid VTL magic/version"));
        }

        Ok(Self { seg })
    }
}

impl Deref for VtlClient {
    type Target = VtlSegment;
    fn deref(&self) -> &VtlSegment {
        &self.seg
    }
}

impl Drop for VtlClient {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.seg.ptr as *mut libc::c_void, self.seg.size);
        }
    }
}
