//! The only `unsafe` in the crate: safe wrappers for the system calls std does not cover.

use std::ffi::CString;
use std::fs::File;
use std::io::{self, ErrorKind};
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Duration;

/// Blocks until one of `fds` has something to read or `timeout` runs out. `None` waits
/// forever.
pub fn wait(fds: &[BorrowedFd<'_>], timeout: Option<Duration>) -> io::Result<()> {
    let mut fds: Vec<libc::pollfd> = fds
        .iter()
        .map(|fd| libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    // Rounded up, so that a wait never ends before the timeout.
    let millis = match timeout {
        Some(timeout) => i32::try_from(timeout.as_nanos().div_ceil(1_000_000)).unwrap_or(i32::MAX),
        None => -1,
    };

    // SAFETY: `fds` points to `fds.len()` valid entries for the whole call.
    let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, millis) };
    if ready < 0 {
        let err = io::Error::last_os_error();
        if err.kind() != ErrorKind::Interrupted {
            return Err(err);
        }
    }
    Ok(())
}

/// Returns a non-blocking file that becomes readable whenever an entry of `dir` is created
/// or has its attributes changed.
pub fn watch_dir(dir: &Path) -> io::Result<File> {
    let dir = CString::new(dir.as_os_str().as_bytes())?;

    // SAFETY: the call takes no pointers.
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` was just returned by the kernel and nothing else owns it.
    let watch = unsafe { File::from_raw_fd(fd) };

    // SAFETY: `dir` is a valid NUL-terminated string for the whole call.
    let added = unsafe { libc::inotify_add_watch(fd, dir.as_ptr(), libc::IN_CREATE | libc::IN_ATTRIB) };
    if added < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(watch)
}
