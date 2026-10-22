//! The chime out of the speakers: core's two-note envelope (`system::chime`) handed to the
//! platform's own player.
//!
//! No audio crate. A device library (cpal, rodio) would need ALSA's development files on Linux,
//! which CI does not install and a packager would have to, to play 0.5 s of sine. Each platform
//! already plays a WAV from a call or a program it ships: `PlaySoundW` on Windows, `afplay` on
//! macOS, and the first of PipeWire's, PulseAudio's or ALSA's player that is there on Linux. A
//! machine with no device or no player costs the person the sound and nothing else; it is best
//! effort, as the page's was when the webview held its audio back.

use std::sync::OnceLock;

use argus_desktop_core::system::chime;

/// Samples a second: 16-bit mono at this rate is under 50 kB for the whole chime.
const RATE: u32 = 22_050;

/// The WAV, made once. Kept for the life of the process because Windows reads a sound played
/// from memory after the call has returned.
fn wav() -> &'static [u8] {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    WAV.get_or_init(|| chime::wav(RATE))
}

/// Plays the chime, without waiting for it: the caller holds the session's lock.
pub fn ring() {
    #[cfg(windows)]
    winmm::play(wav());
    #[cfg(not(windows))]
    std::thread::spawn(|| {
        if let Err(why) = other::play(wav()) {
            tracing::debug!("the chime was not played: {why}");
        }
    });
}

#[cfg(windows)]
mod winmm {
    use std::ffi::c_void;

    #[link(name = "winmm")]
    extern "system" {
        fn PlaySoundW(sound: *const c_void, module: *mut c_void, flags: u32) -> i32;
    }

    const SND_ASYNC: u32 = 0x0001;
    const SND_NODEFAULT: u32 = 0x0002;
    const SND_MEMORY: u32 = 0x0004;

    /// Starts `wav` and returns. `SND_NODEFAULT` keeps the system beep from standing in when it
    /// cannot be played.
    pub fn play(wav: &'static [u8]) {
        // SAFETY: `wav` is 'static, and so valid for as long as the asynchronous play reads it;
        // PlaySoundW reads a WAV image from memory and takes no ownership.
        let started = unsafe { PlaySoundW(wav.as_ptr().cast(), std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
        if started == 0 {
            tracing::debug!("the chime was not played: no sound device");
        }
    }
}

#[cfg(not(windows))]
mod other {
    use std::io::Write as _;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    /// The players to try, in order; each takes the file as its last argument.
    #[cfg(target_os = "macos")]
    const PLAYERS: [(&str, &[&str]); 1] = [("afplay", &[])];
    #[cfg(not(target_os = "macos"))]
    const PLAYERS: [(&str, &[&str]); 3] = [("pw-play", &[]), ("paplay", &[]), ("aplay", &["-q"])];

    /// Where the WAV is kept for a player to read.
    fn file() -> PathBuf {
        std::env::temp_dir().join(format!("argus-chime-{}.wav", std::process::id()))
    }

    /// Plays `wav` with the first player that starts, and waits for it so it is reaped.
    pub fn play(wav: &[u8]) -> Result<(), String> {
        let path = file();
        std::fs::File::create(&path).and_then(|mut f| f.write_all(wav)).map_err(|e| format!("could not write {}: {e}", path.display()))?;
        for (player, args) in PLAYERS {
            let started = Command::new(player).args(args).arg(&path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
            if let Ok(mut child) = started {
                let _ = child.wait();
                return Ok(());
            }
        }
        Err("no player (pw-play, paplay, aplay, afplay) is installed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_played_is_core_s_chime_at_the_rate_it_names() {
        let bytes = wav();
        assert_eq!(bytes, chime::wav(RATE));
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), RATE);
        assert!(bytes.len() < 50_000, "a short sound, not a file to ship: {}", bytes.len());
    }
}
