//! Which process is a pane's console server, on Windows.
//!
//! ConPTY starts the server (`conhost.exe --headless`, or the
//! `OpenConsole.exe` beside a sideloaded `conpty.dll`) as a child of whoever
//! opens the pseudoconsole, and hands back no handle to it; portable-pty
//! keeps even the pseudoconsole private. A server nobody contains outlives
//! an owner that dies, and an orphaned one spins on its broken pipes, so the
//! pane has to find it to put it in its job.

use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
};

const SERVER_NAMES: &[&str] = &["conhost.exe", "openconsole.exe"];

/// One pseudoconsole opens at a time in this process, so a server that
/// appears while one opens can only be that one's. Panes open concurrently
/// in the test harness and when the daemon restores a session.
static OPENING: Mutex<()> = Mutex::new(());

/// Opens a pseudoconsole with `open` and finds the server it started,
/// with the access a job needs to take it in. The server is `None` when it
/// cannot be told apart: a pane without its server contained still works,
/// as every pane did before this existed.
pub(super) fn open_with_server<T>(
    open: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<(T, Option<OwnedHandle>)> {
    let _opening = OPENING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let me = unsafe { GetCurrentProcessId() };
    let before = servers_started_by(me);
    let opened = open()?;
    // A PID already present before is not the new server even if it is
    // the same number: a server of ours that exited in between could have
    // handed its PID on, and claiming that one would contain a stranger.
    let new: Vec<u32> = servers_started_by(me)
        .into_iter()
        .filter(|pid| !before.contains(pid))
        .collect();
    let [pid] = new[..] else {
        tracing::warn!("could not tell this pane's console server apart: {new:?}");
        return Ok((opened, None));
    };
    let raw = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
    if raw.is_null() {
        tracing::warn!(
            "could not open console server {pid}: {}",
            std::io::Error::last_os_error()
        );
        return Ok((opened, None));
    }
    Ok((opened, Some(unsafe { OwnedHandle::from_raw_handle(raw as _) })))
}

/// The console servers whose parent is `parent`, by PID.
pub(super) fn servers_started_by(parent: u32) -> Vec<u32> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        if entry.th32ParentProcessID == parent && is_server(&entry.szExeFile) {
            found.push(entry.th32ProcessID);
        }
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    found
}

fn is_server(exe: &[u16]) -> bool {
    let len = exe.iter().position(|&c| c == 0).unwrap_or(exe.len());
    let name = String::from_utf16_lossy(&exe[..len]).to_ascii_lowercase();
    SERVER_NAMES.contains(&name.as_str())
}
