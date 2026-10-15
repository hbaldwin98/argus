//! Which process is a pane's console server, on Windows.
//!
//! ConPTY starts the server (`conhost.exe --headless`, or the
//! `OpenConsole.exe` beside a sideloaded `conpty.dll`) as a child of whoever
//! opens the pseudoconsole, and hands back no handle to it; portable-pty
//! keeps even the pseudoconsole private. A server nobody contains outlives
//! an owner that dies, and an orphaned one spins on its broken pipes, so the
//! pane has to find it to put it in its job.
//!
//! It is found by diffing bare PID lists and examining only the PIDs that
//! are new. A process snapshot (`CreateToolhelp32Snapshot`, or the
//! `NtQuerySystemInformation` class beneath it) gathers every thread of every
//! process, and took half a second on a machine with four hundred of them:
//! two per pane put a second on every shell and agent started.

use std::collections::HashSet;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::Mutex;

use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
use windows_sys::Win32::System::ProcessStatus::EnumProcesses;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_BASIC_INFORMATION,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
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
    let before: HashSet<u32> = process_ids().into_iter().collect();
    let opened = open()?;
    // A PID already present before is not the new server even if it is
    // the same number: a server of ours that exited in between could have
    // handed its PID on, and claiming that one would contain a stranger.
    let mut new: Vec<(u32, OwnedHandle)> = process_ids()
        .into_iter()
        .filter(|pid| !before.contains(pid))
        .filter_map(|pid| open_server(me, pid).map(|server| (pid, server)))
        .collect();
    if new.len() != 1 {
        let pids: Vec<u32> = new.iter().map(|(pid, _)| *pid).collect();
        tracing::warn!("could not tell this pane's console server apart: {pids:?}");
        return Ok((opened, None));
    }
    Ok((opened, new.pop().map(|(_, server)| server)))
}

/// The console servers whose parent is `parent`, by PID.
#[cfg(test)]
pub(super) fn servers_started_by(parent: u32) -> Vec<u32> {
    process_ids()
        .into_iter()
        .filter(|&pid| open_server(parent, pid).is_some())
        .collect()
}

/// Every process's PID, and nothing else about it, which is what keeps
/// this cheap.
fn process_ids() -> Vec<u32> {
    let mut ids = vec![0u32; 1024];
    loop {
        let size = std::mem::size_of_val(&ids[..]) as u32;
        let mut needed = 0;
        if unsafe { EnumProcesses(ids.as_mut_ptr(), size, &mut needed) } == 0 {
            return Vec::new();
        }
        // A full buffer may have cut the list short: it never says how
        // much more there was.
        if needed < size {
            ids.truncate(needed as usize / std::mem::size_of::<u32>());
            return ids;
        }
        ids.resize(ids.len() * 2, 0);
    }
}

/// `pid`, opened with the access a job needs, if it is a console server
/// whose parent is `parent`. It is examined through the handle that is
/// returned, so the PID cannot pass to another process between the check
/// and its use.
fn open_server(parent: u32, pid: u32) -> Option<OwnedHandle> {
    let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA | PROCESS_TERMINATE;
    let raw = unsafe { OpenProcess(access, 0, pid) };
    if raw.is_null() {
        return None;
    }
    let process = unsafe { OwnedHandle::from_raw_handle(raw as _) };
    (parent_of(&process) == Some(parent) && is_server(&image_name(&process)?))
        .then_some(process)
}

fn parent_of(process: &OwnedHandle) -> Option<u32> {
    let mut info: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let status = unsafe {
        NtQueryInformationProcess(
            process.as_raw_handle() as _,
            ProcessBasicInformation,
            std::ptr::from_mut(&mut info).cast(),
            std::mem::size_of_val(&info) as u32,
            std::ptr::null_mut(),
        )
    };
    (status >= 0).then_some(info.InheritedFromUniqueProcessId as u32)
}

fn image_name(process: &OwnedHandle) -> Option<String> {
    let mut path = [0u16; 1024];
    let mut len = path.len() as u32;
    let named = unsafe {
        QueryFullProcessImageNameW(
            process.as_raw_handle() as _,
            PROCESS_NAME_WIN32,
            path.as_mut_ptr(),
            &mut len,
        )
    };
    (named != 0).then(|| String::from_utf16_lossy(&path[..len as usize]))
}

fn is_server(path: &str) -> bool {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path).to_ascii_lowercase();
    SERVER_NAMES.contains(&name.as_str())
}
