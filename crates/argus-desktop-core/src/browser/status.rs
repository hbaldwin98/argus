//! Who has a browser, said one way wherever it is said (SHELL-BROWSER-9):
//! on a tile, in a cell's strip and by the dot on a nav sub-row.

use argus_protocol::BrowserDriver;

use super::PageBrowser;

/// Who has the browser. A ladder: held by the person, else the agent
/// driving, else the person driving, else nobody, which is no status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Held,
    Agent,
    Person,
}

impl Status {
    /// The words a tile, strip or row says.
    pub fn word(self) -> &'static str {
        match self {
            Status::Held => "You have control",
            Status::Agent => "Agent driving",
            Status::Person => "You’re driving",
        }
    }

    /// The class the page gives the chip and the dot.
    pub fn class(self) -> &'static str {
        match self {
            Status::Held => "held",
            Status::Agent => "agent",
            Status::Person => "person",
        }
    }
}

/// `browser`'s status; `None` says nothing.
pub fn status(browser: &PageBrowser) -> Option<Status> {
    if browser.held {
        return Some(Status::Held);
    }
    match browser.driver {
        Some(BrowserDriver::Agent) => Some(Status::Agent),
        Some(BrowserDriver::Person) => Some(Status::Person),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser(held: bool, driver: Option<BrowserDriver>) -> PageBrowser {
        PageBrowser {
            pane: 1,
            owner: 1,
            agent: String::new(),
            url: String::new(),
            title: String::new(),
            tabs: Vec::new(),
            shown: None,
            following: true,
            held,
            driver,
            cursor: argus_protocol::BrowserCursor::Default,
            dialog: None,
            select: None,
            sized: None,
            context: None,
        }
    }

    #[test]
    fn held_outranks_who_is_driving() {
        assert_eq!(status(&browser(true, Some(BrowserDriver::Agent))), Some(Status::Held));
        assert_eq!(status(&browser(true, None)), Some(Status::Held));
        assert_eq!(status(&browser(false, Some(BrowserDriver::Agent))), Some(Status::Agent));
        assert_eq!(status(&browser(false, Some(BrowserDriver::Person))), Some(Status::Person));
        assert_eq!(status(&browser(false, None)), None);
    }

    #[test]
    fn each_status_has_the_pages_words_and_class() {
        assert_eq!((Status::Held.word(), Status::Held.class()), ("You have control", "held"));
        assert_eq!((Status::Agent.word(), Status::Agent.class()), ("Agent driving", "agent"));
        assert_eq!((Status::Person.word(), Status::Person.class()), ("You’re driving", "person"));
    }
}
