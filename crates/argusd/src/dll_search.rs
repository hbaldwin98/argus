//! Where the daemon loads libraries from, on Windows: its own directory and
//! System32, never the directory it was started in.
//!
//! portable-pty loads `conpty.dll` by bare name, which is what lets a user
//! put a newer ConPTY beside `argusd.exe`. Windows' default search goes on
//! to the current directory — the daemon inherits the one `argus` was run
//! from, often a repository — and System32 holds no `conpty.dll` to be found
//! first, so one checked into a repository would be loaded into the daemon.

use windows_sys::Win32::System::LibraryLoader::{
    SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_APPLICATION_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
};

/// Confines every later load of a library by name to the daemon's own
/// directory and System32. Called before anything can load one.
pub fn restrict() -> std::io::Result<()> {
    let flags = LOAD_LIBRARY_SEARCH_APPLICATION_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32;
    if unsafe { SetDefaultDllDirectories(flags) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::FreeLibrary;
    use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;

    /// Whether a library loads by `name` alone, as portable-pty loads
    /// ConPTY.
    fn loads(name: &str) -> bool {
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            return false;
        }
        unsafe { FreeLibrary(module) };
        true
    }

    #[test]
    fn a_library_in_the_working_directory_is_not_loaded_by_name() {
        // A real library under a name found nowhere but the working
        // directory, standing in for a conpty.dll in a repository.
        let root = std::env::var_os("SystemRoot").expect("Windows names its own directory");
        let system = std::path::Path::new(&root).join("System32").join("version.dll");
        let name = format!("argus-planted-{}.dll", std::process::id());
        let planted = std::env::current_dir().unwrap().join(&name);
        std::fs::copy(&system, &planted).unwrap();

        let before = loads(&name);
        let restricted = restrict();
        let after = loads(&name);
        let _ = std::fs::remove_file(&planted);

        restricted.unwrap();
        assert!(before, "by default the working directory is searched");
        assert!(!after, "and once restricted it is not");
    }
}
