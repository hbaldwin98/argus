//! What the rail's server dot and its menu say about the daemon: the connection in words, the
//! alarm it raised, and the console and chime the menu toggles.
//!
//! The dot is the foot of the icon strip (`rail::server`); the old text foot is gone, so the
//! tallies are the filter chips' and the console count a badge on the strip. The words are one
//! shape for every state, so the dot's tooltip does not rearrange while the daemon comes and
//! goes: only the dot and the words change. A remote daemon is named, so what the rail lists is
//! never taken for this machine's.

use crate::connection::heard::Link;

/// What the dot is called while it says the connection (SHELL-FOOT-3): the dot says connecting and
/// stopped on its own, so only a daemon that was there and is being reached again earns words
/// (`GPUI.md`, cross-cutting decision 1).
pub fn words(link: &Link) -> String {
    match link {
        Link::Connected { host } => format!("argusd · {}", host.as_deref().unwrap_or("local")),
        Link::Reconnecting => "argusd · reconnecting".to_owned(),
        Link::Connecting | Link::Stopped => "argusd".to_owned(),
    }
}

/// What the dot is called to a reader that cannot see the colour: the state in every case.
pub fn label(link: &Link) -> String {
    match link {
        Link::Connecting => "argusd · connecting".to_owned(),
        Link::Stopped => "argusd · stopped".to_owned(),
        other => words(other),
    }
}

/// What the menu needs of the console: whether it is up, and how many consoles the daemon holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Console {
    pub shown: bool,
    pub count: usize,
}

/// Everything the server dot says: what the status holds of it, and the hover text of the
/// settings gear and the console entry with the keys the keymap names for them.
#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    pub link: Link,
    pub alarm: Option<String>,
    pub chime: bool,
    pub console: Console,
    /// `Hide the console (\`)`: the menu entry's words, and its keys when the keymap has been heard.
    pub console_title: String,
    pub settings_title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_row_names_the_host_the_daemon_is_reached_on() {
        assert_eq!(words(&Link::Connected { host: None }), "argusd · local");
        assert_eq!(words(&Link::Connected { host: Some("dev-box".into()) }), "argusd · dev-box");
        assert_eq!(words(&Link::Reconnecting), "argusd · reconnecting");
        assert_eq!(words(&Link::Connecting), "argusd");
        assert_eq!(words(&Link::Stopped), "argusd");
    }

    #[test]
    fn the_state_is_named_to_a_reader_that_cannot_see_the_dot() {
        assert_eq!(label(&Link::Connecting), "argusd · connecting");
        assert_eq!(label(&Link::Stopped), "argusd · stopped");
        assert_eq!(label(&Link::Reconnecting), "argusd · reconnecting");
        assert_eq!(label(&Link::Connected { host: None }), "argusd · local");
    }
}
