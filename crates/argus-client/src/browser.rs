//! Opening a ticket's URL in the person's browser.
//!
//! The system's own opener does it — `xdg-open`, `open`, or Windows's URL
//! handler — so the browser is whichever one the person chose. Over SSH the
//! machine Argus runs on is not the one at the desk, and a browser started
//! there would open where nobody is looking, so the URL goes on the
//! clipboard instead, which OSC 52 carries home (see [`crate::clipboard`]).

use std::process::{Command, Stdio};

/// What became of a request to open a URL, for the status line.
#[derive(Debug, PartialEq, Eq)]
pub enum Opened {
    Browser,
    /// No browser to start here; the URL is on the clipboard instead.
    Copied,
    Refused(&'static str),
}

/// Opens `url`, or copies it where a browser cannot be started.
pub fn open(url: &str) -> Opened {
    if !is_web_url(url) {
        // The daemon refuses anything else when a link is made; checked
        // again because what is about to run it is a shell's opener.
        return Opened::Refused("only an http or https link is opened");
    }
    if std::env::var_os("SSH_CONNECTION").is_none() && start(url).is_ok() {
        return Opened::Browser;
    }
    if crate::clipboard::write(url) {
        Opened::Copied
    } else {
        Opened::Refused("no browser to open it in, and the clipboard is out of reach")
    }
}

fn is_web_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The opener is handed the URL as one argument, never through a shell, so
/// nothing in it is read as a command. Its output would land on the frame,
/// so it goes nowhere.
fn start(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = Command::new("open");
    #[cfg(windows)]
    let mut cmd = {
        let mut cmd = Command::new("rundll32");
        cmd.arg("url.dll,FileProtocolHandler");
        cmd
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = Command::new("xdg-open");

    let mut child = cmd
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    // Reaped off the input thread: `xdg-open` can take a while to hand
    // over, and an unreaped child stays a zombie until Argus exits.
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_web_url_reaches_the_opener() {
        assert_eq!(open("file:///etc/passwd"), Opened::Refused("only an http or https link is opened"));
        assert!(!is_web_url("javascript:alert(1)"));
        assert!(!is_web_url("https://a.example/x y"));
        assert!(is_web_url("HTTPS://linear.app/acme/issue/ENG-88"));
    }
}
