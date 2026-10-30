//! What the add-a-project dialog holds — the kind of add, the folder
//! browsed and the daemon's listing of it, the repositories the daemon says
//! adding it would find, the name and skips typed, and the add waiting on
//! the daemon — what its gestures ask of the daemon, and what the page
//! last heard of it.
//!
//! The folder is on the daemon's machine, so the window lists it and scans
//! it only through the daemon (`ListDirectories`, `PreviewProject`), and
//! each answer is taken only if it is for the last thing asked: a slow
//! listing must not land in a folder the person has since left.

use argus_protocol::{ClientMsg, DirListing, ProjectId, ProjectInfo, ProjectPreview, RequestRefusal};

use super::add_page::{AddKind, AddProjectPage, FolderEntry, Held, ProjectRef};
use crate::connection::page::Event;
use crate::connection::sent::Sent;

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
    /// The project each repository goes into; `None` makes a project.
    into: Option<ProjectId>,
    /// The adds asked and not yet answered.
    waiting: Vec<u64>,
    /// Adds the daemon made that no tree holds yet.
    settling: usize,
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
            into: None,
            waiting: Vec::new(),
            settling: 0,
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

    /// Adds into `project`, or with `None` makes a project of its own.
    pub fn add_to(&mut self, project: Option<ProjectId>) {
        self.into = project;
    }

    /// Opened for a project: adding a repository to it, or a new one in it.
    pub fn for_project(mut self, project: ProjectId, kind: AddKind) -> AddProject {
        self.into = Some(project);
        self.kind = kind;
        self
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
            AddKind::Folder if self.into.is_some() && self.preview.as_ref().is_some_and(|p| p.repositories.is_empty()) => {
                Some("no repositories in this folder".into())
            }
            _ => None,
        }
    }

    fn is_repository(&self) -> bool {
        self.preview.as_ref().is_some_and(|p| p.repositories.len() == 1 && p.repositories[0].is_empty())
    }

    /// The adds, each named by `request(into)`, or none, with why said in the dialog. `into` is the
    /// project a repository goes into, `None` for the project to be made. The dialog stays up until
    /// the daemon has made them and a tree holds them, so a refusal leaves what was typed.
    #[allow(clippy::result_unit_err)]
    pub fn add(&mut self, mut request: impl FnMut(Option<ProjectId>) -> u64) -> Result<Vec<ClientMsg>, ()> {
        if self.is_adding() {
            return Err(());
        }
        let (Some(path), None) = (self.target(), self.blocked()) else {
            self.refusal = self.blocked();
            return Err(());
        };
        self.refusal = None;
        let Some(project) = self.into else {
            let request_id = request(None);
            self.waiting.push(request_id);
            let name = Some(self.name.trim().to_string()).filter(|n| !n.is_empty());
            // Only a folder is scanned for more than itself.
            let skip = if self.kind == AddKind::Folder { self.skips() } else { Vec::new() };
            return Ok(vec![ClientMsg::AddProject { path, request_id, name, skip, init: self.kind == AddKind::New }]);
        };
        let paths = match self.kind {
            AddKind::Folder => self.found().map(|rel| within(&path, rel)).collect(),
            _ => vec![path],
        };
        let init = self.kind == AddKind::New;
        let msgs = paths
            .into_iter()
            .map(|path| {
                let request_id = request(Some(project));
                self.waiting.push(request_id);
                if init {
                    ClientMsg::InitRepository { project, path, request_id }
                } else {
                    ClientMsg::AddRepository { project, path, request_id }
                }
            })
            .collect();
        Ok(msgs)
    }

    /// The repositories the preview found, root-relative; empty for the folder itself.
    fn found(&self) -> impl Iterator<Item = &str> {
        self.preview.iter().flat_map(|p| p.repositories.iter().map(String::as_str))
    }

    fn is_adding(&self) -> bool {
        !self.waiting.is_empty() || self.settling > 0
    }

    /// The daemon made a repository for the add named `request_id`; a tree is yet to hold it.
    pub fn made(&mut self, request_id: u64) {
        if let Some(at) = self.waiting.iter().position(|id| *id == request_id) {
            self.waiting.remove(at);
            self.settling += 1;
        }
    }

    /// A tree holds a repository an add made. Whether the whole add is done, and the dialog is to
    /// go: nothing waits, nothing is yet to be held, and nothing was refused.
    pub fn settled(&mut self) -> bool {
        self.settling = self.settling.saturating_sub(1);
        self.waiting.is_empty() && self.settling == 0 && self.refusal.is_none()
    }

    /// Takes the daemon's refusal of an add waiting on it, saying why in
    /// the dialog; false for any other request's.
    pub fn refused(&mut self, refusal: &RequestRefusal) -> bool {
        let Some(at) = self.waiting.iter().position(|id| *id == refusal.request_id) else {
            return false;
        };
        self.waiting.remove(at);
        self.refusal = Some(refusal.reason.clone());
        true
    }

    /// A new connection answers nothing the last was asked: the folder is
    /// listed again, and an add in flight may or may not have been made,
    /// which the tree will say.
    pub fn reconnected(&mut self, request_id: u64, out: &mut Vec<ClientMsg>) {
        self.waiting.clear();
        self.settling = 0;
        self.previewing = None;
        let path = self.listing.as_ref().map(|l| l.path.clone()).unwrap_or_default();
        self.browse(path, request_id, out);
    }

    pub fn page(&self, tree: &[ProjectInfo], current: Option<ProjectId>) -> AddProjectPage {
        let listing = self.listing.as_ref();
        let preview = self.preview.as_ref();
        let folder_name = listing.map(|l| last_segment(&l.path).to_string()).unwrap_or_default();
        let named = |id: ProjectId| tree.iter().find(|p| p.id == id).map(|p| ProjectRef { id: p.id.0, name: p.name.clone() });
        let held = |path: &str| self.into.and_then(|project| held_at(tree, project, path));
        AddProjectPage {
            open: true,
            kind: self.kind,
            path: listing.map(|l| l.path.clone()),
            parent: listing.and_then(|l| l.parent.clone()),
            entries: listing
                .map(|l| {
                    l.entries
                        .iter()
                        .map(|e| FolderEntry { name: e.name.clone(), is_repo: e.is_repo, held: held(&join(&l.path, &e.name)).filter(|_| e.is_repo) })
                        .collect()
                })
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
            into: self.into.and_then(named),
            projects: tree.iter().map(|p| ProjectRef { id: p.id.0, name: p.name.clone() }).collect(),
            current: current.and_then(named),
            own_project: self
                .into
                .filter(|_| self.kind == AddKind::Repository)
                .zip(listing)
                .and_then(|(project, l)| own_project_at(tree, project, &l.path))
                .map(|p| ProjectRef { id: p.id.0, name: p.name.clone() }),
            adding: self.is_adding(),
            refusal: self.refusal.clone(),
        }
    }
}

/// The dialog as the window holds it, while open, and what the page last
/// heard of it.
#[derive(Default)]
pub struct Adding {
    dialog: Option<AddProject>,
    sent: Sent<AddProjectPage>,
}

impl Adding {
    pub fn open(&mut self, dialog: AddProject) {
        self.dialog = Some(dialog);
    }

    pub fn close(&mut self) {
        self.dialog = None;
    }

    pub fn get(&self) -> Option<&AddProject> {
        self.dialog.as_ref()
    }

    pub fn get_mut(&mut self) -> Option<&mut AddProject> {
        self.dialog.as_mut()
    }

    /// What the page has not heard of the dialog.
    pub fn take(&mut self, tree: &[ProjectInfo], current: Option<ProjectId>) -> Option<Event> {
        let page = self.dialog.as_ref().map(|dialog| dialog.page(tree, current));
        self.sent.take(page).map(|page| Event::AddProject(Box::new(page)))
    }

    /// The page reloaded and holds nothing: an open dialog is sent again.
    pub fn reload(&mut self) {
        self.sent.forget();
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

/// A repository found under `root` by its root-relative name; empty names the folder itself.
fn within(root: &str, relative: &str) -> String {
    if relative.is_empty() {
        return root.to_owned();
    }
    let backslashed = root.contains('\\') && !root.contains('/');
    join(root, &if backslashed { relative.replace('/', "\\") } else { relative.to_owned() })
}

/// Paths as the daemon spells them need not match in separator or in a trailing one; case is left
/// alone, since a wrong match would say a folder is held that is not.
fn same_path(a: &str, b: &str) -> bool {
    let norm = |p: &str| p.replace('\\', "/").trim_end_matches('/').to_owned();
    norm(a) == norm(b)
}

/// Where the tree holds the repository at `path`, from the point of view of adding to `target`.
fn held_at(tree: &[ProjectInfo], target: ProjectId, path: &str) -> Option<Held> {
    if tree.iter().any(|p| p.id == target && holds(p, path)) {
        return Some(Held::InTarget);
    }
    own_project_at(tree, target, path).map(|_| Held::OwnProject)
}

fn holds(project: &ProjectInfo, path: &str) -> bool {
    project.repositories.iter().any(|r| r.checkouts.iter().any(|c| same_path(&c.path, path)))
}

/// The project, other than `target`, whose only repository is the one at `path`.
fn own_project_at<'a>(tree: &'a [ProjectInfo], target: ProjectId, path: &str) -> Option<&'a ProjectInfo> {
    tree.iter().find(|p| p.id != target && p.repositories.len() == 1 && holds(p, path))
}

fn last_segment(path: &str) -> &str {
    path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests;
