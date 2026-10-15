//! Handing a link in a view to the system's opener, so it opens in the
//! browser the person chose.
//!
//! The URL is an agent's, read from a ticket, so only an `http` or `https`
//! one is opened: the opener would as happily run a `file:` URL. It is
//! passed as one argument, never through a shell.

use std::process::{Command, Stdio};

/// Opens `url`, or says why not.
pub fn open(url: &str) -> Result<(), String> {
    if !argus_protocol::agent_view::is_web_url(url) {
        return Err("only an http or https link is opened".into());
    }
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
        .spawn()
        .map_err(|e| format!("could not open the link: {e}"))?;
    // `xdg-open` can take a while to hand over; reaped off the caller's
    // thread so it neither blocks it nor stays a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_web_link_reaches_the_opener() {
        assert_eq!(open("file:///etc/passwd").unwrap_err(), "only an http or https link is opened");
        assert!(open("javascript:alert(1)").is_err());
    }
}
