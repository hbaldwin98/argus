//! The DevTools pipe to a browser: two anonymous pipes the browser is
//! started holding, in place of a port any process on the machine could
//! reach. Messages on them end with a NUL. Each end is read or written on a
//! thread of its own, since an anonymous pipe is not asynchronous on
//! Windows.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};

use tokio::sync::mpsc;

/// The daemon's ends: what it writes the browser, and what it reads.
pub(crate) struct Pipe {
    to_browser: File,
    from_browser: File,
}

/// The browser's ends, handed to it as it starts and closed here after.
pub(crate) struct BrowserEnds {
    #[cfg(windows)]
    read: std::os::windows::io::OwnedHandle,
    #[cfg(windows)]
    write: std::os::windows::io::OwnedHandle,
    #[cfg(unix)]
    read: std::os::fd::OwnedFd,
    #[cfg(unix)]
    write: std::os::fd::OwnedFd,
}

/// A pair of pipes: the daemon's ends, kept from any child; the browser's,
/// for the browser alone to inherit.
pub(crate) fn make() -> std::io::Result<(Pipe, BrowserEnds)> {
    let (browser_read, daemon_write) = anonymous()?;
    let (daemon_read, browser_write) = anonymous()?;
    let pipe = Pipe { to_browser: File::from(daemon_write), from_browser: File::from(daemon_read) };
    pipe.keep_to_self()?;
    Ok((pipe, BrowserEnds { read: browser_read, write: browser_write }))
}

impl BrowserEnds {
    /// The browser's arguments that say where its pipe is, and on Unix the
    /// moves that put it there: Chrome reads fd 3 and writes fd 4.
    pub(crate) fn hand_to(&self, command: &mut tokio::process::Command) {
        command.arg("--remote-debugging-pipe");
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            command.arg(format!(
                "--remote-debugging-io-pipes={},{}",
                self.read.as_raw_handle() as usize,
                self.write.as_raw_handle() as usize
            ));
        }
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let (read, write) = (self.read.as_raw_fd(), self.write.as_raw_fd());
            // SAFETY: between fork and exec only async-signal-safe calls
            // are made: fcntl and dup2.
            unsafe {
                command.pre_exec(move || {
                    // Moved clear of 3 and 4 first, so putting one there
                    // never closes the other.
                    let read = libc::fcntl(read, libc::F_DUPFD, 10);
                    let write = libc::fcntl(write, libc::F_DUPFD, 10);
                    if read < 0 || write < 0 || libc::dup2(read, 3) < 0 || libc::dup2(write, 4) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
    }
}

#[cfg(windows)]
fn anonymous() -> std::io::Result<(std::os::windows::io::OwnedHandle, std::os::windows::io::OwnedHandle)> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::Pipes::CreatePipe;

    let inherit = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let (mut read, mut write): (HANDLE, HANDLE) = (std::ptr::null_mut(), std::ptr::null_mut());
    // SAFETY: both out-pointers are valid, and the handles are owned from
    // here on.
    if unsafe { CreatePipe(&mut read, &mut write, &inherit, 0) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let (read, write) = unsafe { (OwnedHandle::from_raw_handle(read as _), OwnedHandle::from_raw_handle(write as _)) };
    Ok((read, write))
}

#[cfg(unix)]
fn anonymous() -> std::io::Result<(std::os::fd::OwnedFd, std::os::fd::OwnedFd)> {
    use std::os::fd::{FromRawFd, OwnedFd};
    let mut fds = [0; 2];
    // SAFETY: `fds` has room for the two descriptors pipe writes.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    for fd in fds {
        // Closed on exec in every child but the browser, which is given its
        // ends under fds of its own in `hand_to`.
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

impl Pipe {
    /// Keeps the daemon's ends from every child it starts: on Windows each
    /// was made inheritable with its pair.
    fn keep_to_self(&self) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
            for file in [&self.to_browser, &self.from_browser] {
                // SAFETY: the handle is this file's and open.
                if unsafe { SetHandleInformation(file.as_raw_handle() as _, HANDLE_FLAG_INHERIT, 0) } == 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
        }
        Ok(())
    }

    /// The pipe as messages: what is sent on the sender is written to the
    /// browser, and each message it writes comes on the receiver, which
    /// ends when the browser closes its end.
    pub(crate) fn open(self) -> (mpsc::UnboundedSender<String>, mpsc::UnboundedReceiver<String>) {
        let (out, mut outgoing) = mpsc::unbounded_channel::<String>();
        let (incoming, received) = mpsc::unbounded_channel();
        let Pipe { mut to_browser, from_browser } = self;
        std::thread::spawn(move || {
            while let Some(message) = outgoing.blocking_recv() {
                let sent = to_browser.write_all(message.as_bytes()).and_then(|()| to_browser.write_all(b"\0"));
                if sent.is_err() {
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            let mut reader = BufReader::new(from_browser);
            let mut message = Vec::new();
            loop {
                message.clear();
                match reader.read_until(0, &mut message) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if message.last() == Some(&0) {
                            message.pop();
                        }
                        let text = String::from_utf8_lossy(&message).into_owned();
                        if incoming.send(text).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        (out, received)
    }
}
