//! Launching a pane's child, bounding what it starts, and ending all of it.
//!
//! An agent spawns subprocesses of its own, and those outlive a killed
//! parent unless something reaches them. On Windows a job object is that
//! owner: the pane's process is assigned to one at spawn, so closing the
//! pane takes the whole tree with it, and a runaway cannot take the machine
//! down on its way. On Unix the child is its own session leader, and the
//! session is what gets swept.

use super::*;


#[cfg(windows)]
#[derive(Clone, Copy)]
pub(super) struct JobLimits {
    pub(super) memory_bytes: usize,
    pub(super) active_processes: u32,
}

#[cfg(windows)]
pub(super) struct ProcessJob {
    handle: OwnedHandle,
}

#[cfg(windows)]
impl ProcessJob {
    pub(super) fn new(limits: JobLimits) -> anyhow::Result<Self> {
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
            JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_JOB_MEMORY
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        info.BasicLimitInformation.ActiveProcessLimit = limits.active_processes;
        info.JobMemoryLimit = limits.memory_bytes;
        let configured = unsafe {
            SetInformationJobObject(
                handle.as_raw_handle() as _,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        };
        if configured == 0 {
            return Err(anyhow::anyhow!(
                "could not configure agent process limits: {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(Self { handle })
    }

    pub(super) fn assign(&self, process: RawHandle) -> anyhow::Result<()> {
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        let assigned =
            unsafe { AssignProcessToJobObject(self.handle.as_raw_handle() as _, process as _) };
        if assigned == 0 {
            return Err(anyhow::anyhow!(
                "could not contain agent process: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    pub(super) fn terminate(&self) -> anyhow::Result<()> {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        let terminated = unsafe { TerminateJobObject(self.handle.as_raw_handle() as _, 1) };
        if terminated == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn active_processes(&self) -> anyhow::Result<u32> {
        use windows_sys::Win32::System::JobObjects::{
            JobObjectBasicAccountingInformation, QueryInformationJobObject,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        };

        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        let queried = unsafe {
            QueryInformationJobObject(
                self.handle.as_raw_handle() as _,
                JobObjectBasicAccountingInformation,
                std::ptr::from_mut(&mut info).cast(),
                std::mem::size_of_val(&info) as u32,
                std::ptr::null_mut(),
            )
        };
        if queried == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(info.ActiveProcesses)
    }
}

#[cfg(windows)]
pub(super) fn job_for(resource_policy: ResourcePolicy) -> anyhow::Result<Option<ProcessJob>> {
    match resource_policy {
        ResourcePolicy::Unrestricted => Ok(None),
        ResourcePolicy::Agent => ProcessJob::new(JobLimits {
            memory_bytes: AGENT_JOB_MEMORY_BYTES,
            active_processes: AGENT_JOB_PROCESS_LIMIT,
        })
        .map(Some),
    }
}

#[cfg(windows)]
pub(super) fn assign_to_job(
    job: Option<&ProcessJob>,
    child: &mut Box<dyn Child + Send + Sync>,
) -> anyhow::Result<()> {
    let Some(job) = job else {
        return Ok(());
    };
    let Some(handle) = child.as_raw_handle() else {
        let _ = child.kill();
        anyhow::bail!("agent process did not expose a Windows process handle");
    };
    if let Err(error) = job.assign(handle) {
        let _ = child.kill();
        return Err(error);
    }
    Ok(())
}

/// Ends a pane's child and every process it started: the job, when the
/// pane has one, and otherwise just the child.
#[cfg(windows)]
pub(super) fn end_process_tree(
    job: Option<&ProcessJob>,
    child: &StdMutex<Box<dyn Child + Send + Sync>>,
) -> anyhow::Result<()> {
    if let Some(job) = job {
        return job.terminate();
    }
    child.lock().unwrap().kill()?;
    Ok(())
}

/// Ends a pane's child and every process it started.
///
/// portable_pty spawns the child as its own session leader. Signal its
/// session before touching just the direct child: a shell can background a
/// job (e.g. an agent's own subprocess) into a *new* process group that
/// still belongs to the same session, so a plain `child.kill()` — or even a
/// single `killpg` on the leader's own group — leaves it running with no
/// tracked pane left to reap it.
#[cfg(unix)]
pub(super) fn end_process_tree(
    child: &StdMutex<Box<dyn Child + Send + Sync>>,
) -> anyhow::Result<()> {
    let mut child = child.lock().unwrap();
    if let Some(pid) = child.process_id() {
        kill_session(pid as libc::pid_t);
    }
    child.kill()?;
    Ok(())
}

/// Sends SIGKILL to every process in `leader`'s session, not just its own
/// process group. On Linux this walks `/proc` for processes whose session id
/// (the field portable_pty sets to the pty child's own pid via `setsid`)
/// matches `leader`, so a job a shell backgrounded into a fresh process
/// group — or later reparented to init once its parent exits — is still
/// reached; session id survives both, unlike parentage. Other Unixes have no
/// `/proc`, and Darwin's `ps` accepts `-o sess` but always reports 0 for it
/// (confirmed against our own leader, which is definitely its own session),
/// so those ask the kernel directly via `getsid(2)` for each pid `ps` lists
/// (only `ps`'s pid column is used; the broken sess column is not). If
/// neither source is available, falls back to signaling just the leader's
/// own process group, which misses a backgrounded or reparented descendant.
#[cfg(unix)]
fn kill_session(leader: libc::pid_t) {
    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let Ok(pid) = entry.file_name().to_string_lossy().parse::<libc::pid_t>() else {
                    continue;
                };
                let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
                    continue;
                };
                // `comm` is user-controlled and may itself contain
                // parentheses or spaces; the last ')' is always the real
                // boundary, per proc(5).
                let Some(after_comm) = stat.rfind(')').map(|i| &stat[i + 1..]) else {
                    continue;
                };
                let session = after_comm
                    .split_whitespace()
                    .nth(3) // state, ppid, pgrp, session
                    .and_then(|s| s.parse::<libc::pid_t>().ok());
                if session == Some(leader) {
                    unsafe {
                        libc::kill(pid, libc::SIGKILL);
                    }
                }
            }
            return;
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        // BSD `ps` treats `-e` as "show environment", not "every process"
        // (that's the Linux/SysV meaning) — `-A` is the portable "every
        // process" flag here, without which this only sees processes
        // sharing the caller's own controlling terminal.
        if let Ok(output) = crate::command::quiet("ps")
            .into_std()
            .args(["-A", "-o", "pid="])
            .output()
        {
            let mut any = false;
            for pid in String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|s| s.trim().parse::<libc::pid_t>().ok())
            {
                if unsafe { libc::getsid(pid) } == leader {
                    any = true;
                    unsafe {
                        libc::kill(pid, libc::SIGKILL);
                    }
                }
            }
            if any {
                return;
            }
        }
    }
    unsafe {
        libc::kill(-leader, libc::SIGKILL);
    }
}

/// Builds the command to run a named program with args. On Windows this
/// routes through `cmd.exe /C` so PATHEXT resolution finds `.cmd`/`.bat`
/// shims (e.g. npm-installed CLIs) the same way a typed command would;
/// `CreateProcess` alone only resolves bare `.exe` targets.
#[cfg(windows)]
pub(super) fn program_command(program: &str, args: &[String]) -> CommandBuilder {
    let mut c = CommandBuilder::new("cmd.exe");
    c.arg("/C");
    c.arg(program);
    c.args(args);
    c
}

pub(super) fn strip_herdr_context(
    command: &mut CommandBuilder,
    keys: impl IntoIterator<Item = std::ffi::OsString>,
) {
    for key in keys {
        if key.to_string_lossy().starts_with("HERDR_") {
            command.env_remove(key);
        }
    }
}

pub(super) fn spawn_output_reader(
    mut reader: Box<dyn Read + Send>,
    byte_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if byte_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

#[cfg(unix)]
pub(super) fn program_command(program: &str, args: &[String]) -> CommandBuilder {
    let mut c = CommandBuilder::new(program);
    c.args(args);
    c
}
