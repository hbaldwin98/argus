//! What the add-a-project dialog holds — the kind of add, the folder
//! browsed and the daemon's listing of it, the repositories the daemon says
//! adding it would find, the name and skips typed, and the add waiting on
//! the daemon — and what its gestures ask of the daemon.
//!
//! The folder is on the daemon's machine, so the window lists it and scans
//! it only through the daemon (`ListDirectories`, `PreviewProject`), and
//! each answer is taken only if it is for the last thing asked: a slow
//! listing must not land in a folder the person has since left.

use argus_protocol::{ClientMsg, DirListing, ProjectPreview};

use crate::page::{AddKind, AddProjectPage, FolderEntry};

pub struct AddProject {
    kind: AddKind,
    listing: Option<DirListing>,
    /// The listing asked for and not yet come.
    browsing: Option<u64>,
    preview: Option<ProjectPreview>,
    previewing: Option<u64>,
    name: String,
    skip: String,
    folder: String,
    /// The add, named, until the daemon answers it.
    adding: Option<u64>,
    /// A refused add waits for the daemon's reason, which follows it.
    reason_due: bool,
    refusal: Option<String>,
}

impl AddProject {
    /// Opens on wherever the daemon starts a browse.
    pub fn new(request_id: u64, out: &mut Vec<ClientMsg>) -> AddProject {
        let mut dialog = AddProject {
            kind: AddKind::Folder,
            listing: None,
            browsing: None,
            preview: None,
            previewing: None,
            name: String::new(),
            skip: String::new(),
            folder: String::new(),
            adding: None,
            reason_due: false,
            refusal: None,
        };
        dialog.browse(String::new(), request_id, out);
        dialog
    }

    pub fn browse(&mut self, path: String, request_id: u64, out: &mut Vec<ClientMsg>) {
        self.browsing = Some(request_id);
        out.push(ClientMsg::ListDirectories { request_id, path });
    }

    /// Into a folder the one browsed lists.
    pub fn open(&mut self, name: &str, request_id: u64, out: &mut Vec<ClientMsg>) {
        let Some(listing) = &self.listing else {
            return;
        };
        if listing.entries.iter().any(|e| e.name == name) {
            let path = join(&listing.path, name);
            self.browse(path, request_id, out);
        }
    }

    /// A listing; one for a folder since left changes nothing. The folder
    /// listed is the one a preview is asked of.
    pub fn listed(&mut self, listing: DirListing, name: impl FnOnce() -> u64, out: &mut Vec<ClientMsg>) -> bool {
        if self.browsing != Some(listing.request_id) {
            return false;
        }
        self.browsing = None;
        self.listing = Some(listing);
        self.preview = None;
        self.ask_preview(name(), out);
        true
    }

    pub fn previewed(&mut self, preview: ProjectPreview) -> bool {
        if self.previewing != Some(preview.request_id) {
            return false;
        }
        self.previewing = None;
        self.preview = Some(preview);
        true
    }

    fn ask_preview(&mut self, request_id: u64, out: &mut Vec<ClientMsg>) {
        let Some(listing) = self.listing.as_ref().filter(|l| l.error.is_none()) else {
            return;
        };
        self.previewing = Some(request_id);
        out.push(ClientMsg::PreviewProject {
            request_id,
            path: listing.path.clone(),
            skip: self.skips(),
        });
    }

    pub fn set_kind(&mut self, kind: AddKind) {
        self.kind = kind;
    }

    /// What the fields say now. New skips scan again: the preview is what
    /// the add would find, so it must skip what the add will.
    pub fn set_fields(
        &mut self,
        name: String,
        skip: String,
        folder: String,
        request_id: impl FnOnce() -> u64,
        out: &mut Vec<ClientMsg>,
    ) {
        let rescan = skip != self.skip;
        (self.name, self.skip, self.folder) = (name, skip, folder);
        if rescan {
            self.ask_preview(request_id(), out);
        }
    }

    fn skips(&self) -> Vec<String> {
        self.skip.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
    }

    /// Where the add goes: the folder browsed, or for a new repository the
    /// folder to make inside it.
    fn target(&self) -> Option<String> {
        let path = &self.listing.as_ref()?.path;
        match self.kind {
            AddKind::New => {
                let folder = self.folder.trim();
                (!folder.is_empty()).then(|| join(path, folder))
            }
            _ => Some(path.clone()),
        }
    }

    fn default_name(&self) -> String {
        self.target().as_deref().map(last_segment).unwrap_or_default().to_string()
    }

    /// Why Add cannot be pressed yet, read off what the daemon has said.
    fn blocked(&self) -> Option<String> {
        let Some(listing) = &self.listing else {
            return Some("pick a folder".into());
        };
        if listing.error.is_some() {
            return Some("that folder cannot be read".into());
        }
        match self.kind {
            AddKind::New if self.folder.trim().is_empty() => Some("name the new repository's folder".into()),
            AddKind::New if self.folder.contains(['/', '\\']) => Some("the new folder is one name, not a path".into()),
            AddKind::New => None,
            _ if self.previewing.is_some() || self.preview.is_none() => Some("looking for repositories".into()),
            AddKind::Repository if !self.is_repository() => Some("this folder is not a repository".into()),
            _ => None,
        }
    }

    fn is_repository(&self) -> bool {
        self.preview.as_ref().is_some_and(|p| p.repositories.len() == 1 && p.repositories[0].is_empty())
    }

    /// The add, named `request_id`, or none, with why said in the dialog.
    /// The dialog stays up until the daemon has made the project and a
    /// tree holds it, so a refusal leaves what was typed.
    pub fn add(&mut self, request_id: u64) -> Result<ClientMsg, ()> {
        if self.adding.is_some() {
            return Err(());
        }
        let (Some(path), None) = (self.target(), self.blocked()) else {
            self.refusal = self.blocked();
            return Err(());
        };
        let name = Some(self.name.trim().to_string()).filter(|n| !n.is_empty());
        // Only a folder is scanned for more than itself.
        let skip = if self.kind == AddKind::Folder { self.skips() } else { Vec::new() };
        self.adding = Some(request_id);
        self.refusal = None;
        Ok(ClientMsg::AddProject { path, request_id, name, skip, init: self.kind == AddKind::New })
    }

    /// The daemon refused request `request_id`; its reason comes next.
    pub fn refused(&mut self, request_id: u64) {
        if self.adding == Some(request_id) {
            self.adding = None;
            self.reason_due = true;
        }
    }

    /// The daemon's error, if it is the reason the add was refused.
    pub fn take_reason(&mut self, message: &str) -> bool {
        if !std::mem::take(&mut self.reason_due) {
            return false;
        }
        self.refusal = Some(message.to_string());
        true
    }

    /// A new connection answers nothing the last was asked: the folder is
    /// listed again, and an add in flight may or may not have been made,
    /// which the tree will say.
    pub fn reconnected(&mut self, request_id: u64, out: &mut Vec<ClientMsg>) {
        self.adding = None;
        self.reason_due = false;
        self.previewing = None;
        let path = self.listing.as_ref().map(|l| l.path.clone()).unwrap_or_default();
        self.browse(path, request_id, out);
    }

    pub fn page(&self) -> AddProjectPage {
        let listing = self.listing.as_ref();
        let preview = self.preview.as_ref();
        let folder_name = listing.map(|l| last_segment(&l.path).to_string()).unwrap_or_default();
        AddProjectPage {
            open: true,
            kind: self.kind,
            path: listing.map(|l| l.path.clone()),
            parent: listing.and_then(|l| l.parent.clone()),
            entries: listing
                .map(|l| l.entries.iter().map(|e| FolderEntry { name: e.name.clone(), is_repo: e.is_repo }).collect())
                .unwrap_or_default(),
            browsing: self.browsing.is_some(),
            listing_error: listing.and_then(|l| l.error.clone()),
            repositories: preview
                .map(|p| {
                    p.repositories.iter().map(|r| if r.is_empty() { folder_name.clone() } else { r.clone() }).collect()
                })
                .unwrap_or_default(),
            previewing: self.previewing.is_some(),
            depth: preview.map(|p| p.depth),
            always_skipped: preview.map(|p| p.always_skipped.clone()).unwrap_or_default(),
            name: self.name.clone(),
            skip: self.skip.clone(),
            folder: self.folder.clone(),
            default_name: self.default_name(),
            target: self.target().filter(|_| self.kind == AddKind::New),
            blocked: self.blocked(),
            adding: self.adding.is_some(),
            refusal: self.refusal.clone(),
        }
    }
}

/// `name` inside `dir`, spelled with the separator the daemon's own path
/// uses: the folder is on its machine, which need not be this one's kind.
fn join(dir: &str, name: &str) -> String {
    let separator = if dir.contains('\\') && !dir.contains('/') { '\\' } else { '/' };
    if dir.ends_with(['/', '\\']) {
        format!("{dir}{name}")
    } else {
        format!("{dir}{separator}{name}")
    }
}

fn last_segment(path: &str) -> &str {
    path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::DirEntry;

    fn listing(request_id: u64, path: &str) -> DirListing {
        DirListing {
            request_id,
            path: path.into(),
            parent: Some("/home".into()),
            entries: vec![DirEntry { name: "web".into(), is_repo: true }],
            error: None,
        }
    }

    fn preview(request_id: u64, repositories: &[&str]) -> ProjectPreview {
        ProjectPreview {
            request_id,
            path: "/home/work".into(),
            repositories: repositories.iter().map(|r| r.to_string()).collect(),
            depth: 8,
            always_skipped: vec![".git".into()],
            error: None,
        }
    }

    /// A dialog open on `/home/work`, its preview answered with `found`.
    fn browsed(found: &[&str]) -> AddProject {
        let mut out = Vec::new();
        let mut dialog = AddProject::new(1, &mut out);
        assert!(dialog.listed(listing(1, "/home/work"), || 2, &mut out));
        assert!(dialog.previewed(preview(2, found)));
        dialog
    }

    #[test]
    fn opening_lists_where_the_daemon_starts_and_a_listing_asks_for_its_preview() {
        let mut out = Vec::new();
        let mut dialog = AddProject::new(1, &mut out);
        assert!(matches!(&out[..], [ClientMsg::ListDirectories { request_id: 1, path }] if path.is_empty()));
        assert!(dialog.page().browsing);

        out.clear();
        dialog.listed(listing(1, "/home/work"), || 2, &mut out);
        assert!(matches!(&out[..], [ClientMsg::PreviewProject { request_id: 2, path, .. }] if path == "/home/work"));
        let page = dialog.page();
        assert_eq!((page.path.as_deref(), page.browsing, page.previewing), (Some("/home/work"), false, true));
        assert_eq!(page.blocked.as_deref(), Some("looking for repositories"));
    }

    #[test]
    fn an_answer_for_a_folder_since_left_is_dropped() {
        let mut out = Vec::new();
        let mut dialog = AddProject::new(1, &mut out);
        dialog.browse("/elsewhere".into(), 3, &mut out);
        assert!(!dialog.listed(listing(1, "/home"), || 9, &mut out), "the first listing is stale");
        assert!(dialog.listed(listing(3, "/elsewhere"), || 4, &mut out));
        assert!(!dialog.previewed(preview(9, &["old"])));
        assert!(dialog.previewed(preview(4, &["web"])));
        assert_eq!(dialog.page().repositories, ["web"]);
    }

    #[test]
    fn a_folder_adds_what_the_preview_found_skipping_what_was_typed() {
        let mut dialog = browsed(&["billing-api", "infra/k8s"]);
        let mut out = Vec::new();
        dialog.set_fields("work".into(), "vendor, archive".into(), String::new(), || 5, &mut out);
        assert!(
            matches!(&out[..], [ClientMsg::PreviewProject { request_id: 5, skip, .. }] if skip == &["vendor", "archive"]),
            "new skips scan again"
        );
        assert!(dialog.add(6).is_err(), "not until the new preview is in");
        dialog.previewed(preview(5, &["billing-api"]));

        let ClientMsg::AddProject { path, request_id, name, skip, init } = dialog.add(6).unwrap() else {
            panic!("not an add");
        };
        assert_eq!(
            (path.as_str(), request_id, name.as_deref(), skip, init),
            ("/home/work", 6, Some("work"), vec!["vendor".to_string(), "archive".to_string()], false)
        );
        assert!(dialog.page().adding);
        assert!(dialog.add(7).is_err(), "one add at a time");
    }

    #[test]
    fn one_repository_is_only_a_folder_that_is_one() {
        let mut dialog = browsed(&["web", "api"]);
        dialog.set_kind(AddKind::Repository);
        assert_eq!(dialog.page().blocked.as_deref(), Some("this folder is not a repository"));
        assert!(dialog.add(2).is_err());
        assert_eq!(dialog.page().refusal.as_deref(), Some("this folder is not a repository"), "said in the dialog");

        let mut dialog = browsed(&[""]);
        dialog.set_kind(AddKind::Repository);
        assert_eq!(dialog.page().repositories, ["work"], "the folder itself, by its name");
        let Ok(ClientMsg::AddProject { name: None, init: false, skip, .. }) = dialog.add(3) else {
            panic!("a repository is added as itself");
        };
        assert!(skip.is_empty());
    }

    #[test]
    fn a_new_repository_is_made_in_a_folder_named_inside_the_one_browsed() {
        let mut dialog = browsed(&[]);
        dialog.set_kind(AddKind::New);
        assert_eq!(dialog.page().blocked.as_deref(), Some("name the new repository's folder"));
        dialog.set_fields(String::new(), String::new(), "scratch".into(), || 9, &mut Vec::new());
        let page = dialog.page();
        assert_eq!((page.target.as_deref(), page.default_name.as_str()), (Some("/home/work/scratch"), "scratch"));
        let Ok(ClientMsg::AddProject { path, init: true, .. }) = dialog.add(4) else {
            panic!("a new repository is initialised");
        };
        assert_eq!(path, "/home/work/scratch");
    }

    #[test]
    fn a_refused_add_keeps_what_was_typed_and_says_why() {
        let mut dialog = browsed(&["web"]);
        dialog.set_fields("work".into(), String::new(), String::new(), || 9, &mut Vec::new());
        dialog.add(6).unwrap();
        assert!(!dialog.take_reason("something else"), "no add was refused yet");
        dialog.refused(6);
        assert!(dialog.take_reason("there is already a project called work"));
        let page = dialog.page();
        assert_eq!((page.adding, page.name.as_str()), (false, "work"));
        assert_eq!(page.refusal.as_deref(), Some("there is already a project called work"));
        assert!(dialog.add(7).is_ok(), "and it can be tried again");
    }

    #[test]
    fn only_a_folder_the_listing_holds_is_gone_into() {
        let mut dialog = browsed(&["web"]);
        let mut out = Vec::new();
        dialog.open("elsewhere", 5, &mut out);
        assert!(out.is_empty());
        dialog.open("web", 5, &mut out);
        assert!(matches!(&out[..], [ClientMsg::ListDirectories { request_id: 5, path }] if path == "/home/work/web"));
    }

    #[test]
    fn a_path_is_joined_in_the_daemons_own_spelling() {
        assert_eq!(join(r"C:\Users\me", "x"), r"C:\Users\me\x");
        assert_eq!(join(r"C:\", "x"), r"C:\x");
        assert_eq!(join("/", "x"), "/x");
        assert_eq!(join("/home/me", "x"), "/home/me/x");
        assert_eq!(last_segment(r"C:\Users\me\"), "me");
    }
}
