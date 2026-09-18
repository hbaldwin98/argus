//! The instance-wide lock that keeps one daemon from restoring and serving a
//! second copy of the same Argus workspace.

use std::fs::File;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io;
use std::path::Path;

pub(super) struct DaemonLock {
    // The handle is the lock. Keeping it alive for the daemon's lifetime is
    // what makes a crashed process release the instance without stale state
    // having to be interpreted.
    _file: File,
}

impl DaemonLock {
    pub(super) fn acquire() -> io::Result<Option<Self>> {
        Self::acquire_at(&argus_protocol::transport::lock_path())
    }

    fn acquire_at(path: &Path) -> io::Result<Option<Self>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;

            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(path)?;
            let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if locked == 0 {
                return Ok(Some(Self { _file: file }));
            }

            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(error);
        }

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use std::os::windows::io::FromRawHandle;
            use windows_sys::Win32::Foundation::{ERROR_SHARING_VIOLATION, INVALID_HANDLE_VALUE};
            use windows_sys::Win32::Storage::FileSystem::{
                CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
                OPEN_ALWAYS,
            };

            let wide = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>();
            let handle = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                    0,
                    std::ptr::null(),
                    OPEN_ALWAYS,
                    FILE_ATTRIBUTE_NORMAL,
                    std::ptr::null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_SHARING_VIOLATION as i32) {
                    return Ok(None);
                }
                return Err(error);
            }
            let file = unsafe { File::from_raw_handle(handle as _) };
            return Ok(Some(Self { _file: file }));
        }

        #[allow(unreachable_code)]
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Argus daemon locking is unsupported on this platform",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_process_can_hold_an_instance_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("argus.lock");
        let first = DaemonLock::acquire_at(&path)
            .unwrap()
            .expect("the first daemon should acquire the lock");
        assert!(
            DaemonLock::acquire_at(&path).unwrap().is_none(),
            "a second daemon must be refused while the first owns the lock"
        );

        drop(first);
        assert!(
            DaemonLock::acquire_at(&path)
                .unwrap()
                .is_some(),
            "the lock should be released when its owner exits"
        );
    }
}
