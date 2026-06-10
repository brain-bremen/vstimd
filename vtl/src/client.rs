use std::io;
use std::ops::Deref;

use crate::layout::{SHM_SIZE, MAX_BANKS};
use crate::segment::VtlSegment;

/// Attaches to an existing VTL shared memory segment created by a [`VtlOwner`](crate::VtlOwner).
///
/// Does not unlink the segment on drop — only unmaps the view.
pub struct VtlClient {
    seg: VtlSegment,
    #[cfg(windows)]
    mapping_handle: windows_sys::Win32::Foundation::HANDLE,
}

impl VtlClient {
    /// Open an existing VTL segment at `shm_name` (e.g. `"/vstimd_vtl"`).
    ///
    /// Returns an error if the segment does not exist or the magic/version are invalid.
    pub fn open(shm_name: &str) -> io::Result<Self> {
        #[cfg(unix)]
        {
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

            if seg.num_input_banks() as usize > MAX_BANKS || seg.num_output_banks() as usize > MAX_BANKS {
                unsafe { libc::munmap(ptr, SHM_SIZE) };
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid VTL bank counts (max {MAX_BANKS})"),
                ));
            }

            Ok(Self { seg })
        }

        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::CloseHandle;
            use windows_sys::Win32::System::Memory::{
                MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
                FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS,
            };

            let name_wide = crate::owner::windows_wide_name(shm_name);
            let mapping_handle = unsafe {
                OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, name_wide.as_ptr())
            };
            if mapping_handle.is_null() {
                return Err(io::Error::last_os_error());
            }

            let mapped = unsafe {
                MapViewOfFile(mapping_handle, FILE_MAP_ALL_ACCESS, 0, 0, SHM_SIZE)
            };
            if mapped.Value.is_null() {
                let err = io::Error::last_os_error();
                unsafe { CloseHandle(mapping_handle) };
                return Err(err);
            }

            let seg = VtlSegment { ptr: mapped.Value as *mut u8, size: SHM_SIZE };

            if !seg.is_valid() {
                unsafe {
                    UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: mapped.Value });
                    CloseHandle(mapping_handle);
                }
                return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid VTL magic/version"));
            }

            if seg.num_input_banks() as usize > MAX_BANKS || seg.num_output_banks() as usize > MAX_BANKS {
                unsafe {
                    UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: mapped.Value });
                    CloseHandle(mapping_handle);
                }
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid VTL bank counts (max {MAX_BANKS})"),
                ));
            }

            Ok(Self { seg, mapping_handle })
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = shm_name;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "VTL shared memory is not supported on this platform",
            ))
        }
    }
}

impl Deref for VtlClient {
    type Target = VtlSegment;
    fn deref(&self) -> &VtlSegment {
        &self.seg
    }
}

// SAFETY: HANDLE (*mut c_void on windows-sys 0.61) is a Windows kernel object
// reference that is safe to send and share across threads.
#[cfg(windows)]
unsafe impl Send for VtlClient {}
#[cfg(windows)]
unsafe impl Sync for VtlClient {}

impl Drop for VtlClient {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::munmap(self.seg.ptr as *mut libc::c_void, self.seg.size);
        }

        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::Foundation::CloseHandle;
            use windows_sys::Win32::System::Memory::{UnmapViewOfFile, MEMORY_MAPPED_VIEW_ADDRESS};
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: self.seg.ptr as *mut _ });
            CloseHandle(self.mapping_handle);
        }
    }
}
