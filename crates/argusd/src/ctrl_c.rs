//! Whether a pane's child can be interrupted, on Windows: the daemon hands
//! its children Ctrl-C handling switched on, however it was started itself.
//!
//! `argus` starts the daemon with `CREATE_NEW_PROCESS_GROUP`, and Windows
//! starts such a process ignoring Ctrl-C. That is a flag on the process,
//! and every child inherits it: a shell in a pane, and whatever the shell
//! runs, would never be told of the Ctrl-C its console raised. A program
//! reading keys raw still saw the byte, which is why an agent's prompt
//! and PSReadLine's took it while a running command could not be stopped.

use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

/// Clears the inherited "ignore Ctrl-C" flag. Called before any pane is
/// launched; the daemon has no console of its own for it to matter to.
pub fn restore() -> std::io::Result<()> {
    if unsafe { SetConsoleCtrlHandler(None, 0) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
