use std::io;
use std::ops::Deref;

use crate::layout::{SHM_SIZE, MAGIC, VERSION, MAX_BANKS};
use crate::segment::VtlSegment;

/// Creates and owns a VTL shared memory segment.
///
/// On Linux/macOS the segment is backed by POSIX shared memory (`shm_open`).
/// On Windows it is backed by a named file mapping over the pagefile.
///
/// The segment is destroyed (unlinked / handle closed) when this value is dropped.
///
/// Use [`VtlClient`](crate::VtlClient) to attach to an existing segment.
pub struct VtlOwner {
    seg: VtlSegment,
    #[cfg(unix)]
    name: std::ffi::CString,
    #[cfg(windows)]
    mapping_handle: windows_sys::Win32::Foundation::HANDLE,
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

        #[cfg(unix)]
        {
            let name = std::ffi::CString::new(shm_name).expect("shm_name must not contain nul");
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

            // OS zero-initialises POSIX shm; just fill in the non-zero header fields.
            unsafe {
                let h = seg.ptr as *mut crate::layout::VtlHeader;
                std::ptr::write_volatile(&mut (*h).magic,            MAGIC);
                std::ptr::write_volatile(&mut (*h).version,          VERSION);
                std::ptr::write_volatile(&mut (*h).num_input_banks,  num_input_banks);
                std::ptr::write_volatile(&mut (*h).num_output_banks, num_output_banks);
            }

            Ok(Self { seg, name })
        }

        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
            use windows_sys::Win32::System::Memory::{
                CreateFileMappingW, MapViewOfFile, FILE_MAP_ALL_ACCESS, PAGE_READWRITE,
            };

            let name_wide = windows_wide_name(shm_name);
            let mapping_handle = unsafe {
                CreateFileMappingW(
                    INVALID_HANDLE_VALUE, // pagefile-backed
                    std::ptr::null(),     // default security attributes
                    PAGE_READWRITE,
                    (SHM_SIZE >> 32) as u32,
                    (SHM_SIZE & 0xFFFF_FFFF) as u32,
                    name_wide.as_ptr(),
                )
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

            // Pagefile-backed mappings are zero-initialised by Windows; write header.
            unsafe {
                let h = seg.ptr as *mut crate::layout::VtlHeader;
                std::ptr::write_volatile(&mut (*h).magic,            MAGIC);
                std::ptr::write_volatile(&mut (*h).version,          VERSION);
                std::ptr::write_volatile(&mut (*h).num_input_banks,  num_input_banks);
                std::ptr::write_volatile(&mut (*h).num_output_banks, num_output_banks);
            }

            Ok(Self { seg, mapping_handle })
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = (shm_name, num_input_banks, num_output_banks);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "VTL shared memory is not supported on this platform",
            ))
        }
    }
}

impl Deref for VtlOwner {
    type Target = VtlSegment;
    fn deref(&self) -> &VtlSegment {
        &self.seg
    }
}

// SAFETY: HANDLE (*mut c_void on windows-sys 0.61) is a Windows kernel object
// reference that is safe to send and share across threads.
#[cfg(windows)]
unsafe impl Send for VtlOwner {}
#[cfg(windows)]
unsafe impl Sync for VtlOwner {}

impl Drop for VtlOwner {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::munmap(self.seg.ptr as *mut libc::c_void, self.seg.size);
            libc::shm_unlink(self.name.as_ptr());
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

/// Convert a POSIX-style shm name (e.g. `"/vstimd_vtl"`) to a null-terminated
/// UTF-16 Windows object name (e.g. `"Local\vstimd_vtl\0"`).
#[cfg(windows)]
pub(crate) fn windows_wide_name(shm_name: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    let stripped = shm_name.strip_prefix('/').unwrap_or(shm_name);
    let win_name = format!("Local\\{stripped}");
    OsStr::new(&win_name).encode_wide().chain(std::iter::once(0)).collect()
}
