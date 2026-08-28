//! Launching a pane's child, and the Windows job object that bounds an
//! agent's descendants.
//!
//! An agent spawns subprocesses of its own, and on Windows those outlive a
//! killed parent unless something owns them. A job object is that owner:
//! the pane's process is assigned to one at spawn, so closing the pane
//! takes the whole tree with it, and a runaway cannot take the machine
//! down on its way.

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
