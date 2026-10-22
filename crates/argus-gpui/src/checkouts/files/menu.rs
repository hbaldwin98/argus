//! What a right-click on a row of the Files panel offers (`menuFor`, `ck/files.ts`): the file
//! opened in Code or the editor, shown in the file manager, its path copied, and a shell here.
//!
//! A choice is acted on once the menu has gone, against the page as it is then: the rows can have
//! moved between the click and the choosing, so a way carries the row's path and nothing else.

use argus_desktop_core::checkouts::files::{FilesGesture, OpenIn};
use argus_desktop_core::checkouts::files_page::{FileRow, FilesPage};
use argus_desktop_core::connection::page::Call;

use crate::kit::menu::item::MenuItem;

/// What a choice does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Way {
    Open(String, OpenIn),
    Reveal(String),
    CopyPath(String),
    CopyRelative(String),
    Shell,
}

/// The list for `row`: a file may be opened, a folder only revealed and copied; the file manager
/// only for a daemon on this machine.
pub fn items(files: &FilesPage, row: &FileRow) -> Vec<MenuItem<Way>> {
    let mut list = Vec::new();
    if !row.dir {
        list.push(MenuItem::choice("Open in Code", Way::Open(row.path.clone(), OpenIn::Code)).keyed(vec![vec!["Enter".to_owned()]]));
        list.push(MenuItem::choice("Open in the editor", Way::Open(row.path.clone(), OpenIn::Editor)).keyed(vec![vec!["O".to_owned()]]));
    }
    if files.can_reveal {
        list.push(MenuItem::choice(files.reveal.clone(), Way::Reveal(row.path.clone())));
    }
    list.push(MenuItem::choice("Copy path", Way::CopyPath(row.path.clone())));
    list.push(MenuItem::choice("Copy relative path", Way::CopyRelative(row.path.clone())));
    list.push(MenuItem::Rule);
    list.push(MenuItem::choice("Open a shell here", Way::Shell));
    list
}

/// What `way` comes to against `files` as it stands: the calls it sends.
pub fn calls(way: &Way, files: &FilesPage) -> Vec<Call> {
    let gesture = |gesture| Call::Files { gesture };
    match way {
        Way::Open(path, how) => vec![gesture(FilesGesture::Open { path: path.clone(), how: *how })],
        Way::Reveal(path) => vec![gesture(FilesGesture::Reveal { path: Some(path.clone()) })],
        Way::CopyPath(path) => vec![Call::CopyText { text: files.path_of(path) }],
        Way::CopyRelative(path) => vec![Call::CopyText { text: path.clone() }],
        Way::Shell => vec![gesture(FilesGesture::Shell)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::menu::item::lines;

    fn row(path: &str, dir: bool) -> FileRow {
        FileRow { path: path.into(), name: path.rsplit('/').next().unwrap().into(), depth: 0, dir, open: false, mark: None }
    }

    fn page(local: bool) -> FilesPage {
        FilesPage { root: "C:\\src\\night\\\\".into(), separator: "\\".into(), reveal: "Show in Explorer".into(), can_reveal: local, ..FilesPage::default() }
    }

    /// FILES-16.
    #[test]
    fn a_file_offers_opening_the_file_manager_when_local_both_copies_and_a_shell() {
        let list = items(&page(true), &row("src/a.rs", false));
        assert_eq!(lines(&list), ["Open in Code", "Open in the editor", "Show in Explorer", "Copy path", "Copy relative path", "---", "Open a shell here"]);
        let remote = items(&page(false), &row("src/a.rs", false));
        assert_eq!(lines(&remote), ["Open in Code", "Open in the editor", "Copy path", "Copy relative path", "---", "Open a shell here"]);
    }

    #[test]
    fn a_folder_is_not_opened_in_code_or_the_editor() {
        let list = items(&page(true), &row("src", true));
        assert_eq!(lines(&list), ["Show in Explorer", "Copy path", "Copy relative path", "---", "Open a shell here"]);
    }

    /// FILES-16, FILES-24: the reveal names the row's path, not null.
    #[test]
    fn each_choice_comes_to_its_call() {
        let files = page(true);
        assert_eq!(format!("{:?}", calls(&Way::Open("src/a.rs".into(), OpenIn::Code), &files)), "[Files { gesture: Open { path: \"src/a.rs\", how: Code } }]");
        assert_eq!(format!("{:?}", calls(&Way::Reveal("src/a.rs".into()), &files)), "[Files { gesture: Reveal { path: Some(\"src/a.rs\") } }]");
        assert_eq!(format!("{:?}", calls(&Way::Shell, &files)), "[Files { gesture: Shell }]");
    }

    #[test]
    fn copy_path_is_spelled_by_the_daemons_host_and_the_relative_one_as_it_is() {
        let files = page(true);
        assert_eq!(format!("{:?}", calls(&Way::CopyPath("src/a.rs".into()), &files)), "[CopyText { text: \"C:\\\\src\\\\night\\\\src\\\\a.rs\" }]");
        assert_eq!(format!("{:?}", calls(&Way::CopyRelative("src/a.rs".into()), &files)), "[CopyText { text: \"src/a.rs\" }]");
    }
}
