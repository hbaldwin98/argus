//! Every message a client can send, generated against what a host's
//! fixture holds: one strategy per [`ClientMsg`] variant, shared by the
//! fuzz and anything else that needs arbitrary requests.
//!
//! No variant can be left out. [`kind`] matches every variant, so a new
//! one does not compile until it is given a [`Kind`]; [`generate`] matches
//! every `Kind`, so that one does not compile until it has a strategy; and
//! [`Kind::ALL`] is written by the same macro as the enum, so it lists
//! them all.

pub mod board;
pub mod git;
pub mod panel;
pub mod panes;
pub mod session;
pub mod values;
pub mod views;

use std::path::Path;

use argus_protocol::{CheckoutId, ClientMsg, PaneId, ProjectId, RepositoryId, WorkspaceId};
use proptest::prelude::*;

use super::Host;

/// What the fixture holds that a request can name: every host prepared by
/// [`Known::prepare`] holds the same, so strategies built from one host's
/// fit every other's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    pub workspace: WorkspaceId,
    pub project: ProjectId,
    pub repository: RepositoryId,
    pub checkout: CheckoutId,
    /// A shell running in the checkout.
    pub pane: PaneId,
    /// A feature on the checkout's board, with one task and one open
    /// question under it.
    pub feature: String,
    pub task: i64,
    pub decision: i64,
    /// HEAD of the fixture repository.
    pub commit: String,
    /// The fixture's files, as a client names them.
    pub files: Vec<String>,
}

impl Known {
    /// Puts into `host` what requests name, and reads back what it is.
    pub fn prepare(host: &Host) -> Known {
        use argus_protocol::{DecisionState, DecisionWrite, FeatureAction, FeatureWrite, TaskAction, TaskWrite};

        let (project, checkout) = (host.project(), host.checkout());
        let pane = host.daemon.spawn_shell(checkout).expect("the fixture's shell starts");
        let open = FeatureAction::Open(FeatureWrite { title: "Fuzz".into(), body: None });
        host.daemon
            .feature_action_for_client(project, checkout, open)
            .expect("the fixture's feature opens");
        let feature = "fuzz".to_owned();
        let add = TaskAction::Add(TaskWrite { title: "a task".into(), external: None, parent: None });
        host.daemon
            .part_action_for_client(project, checkout, &feature, add)
            .expect("the fixture's task is added");
        let question = DecisionWrite { chose: "which way".into(), state: DecisionState::Open, ..Default::default() };
        host.daemon
            .decide_for_client(project, checkout, &feature, question)
            .expect("the fixture's question is asked");
        let tree = host.daemon.snapshot();
        let commit = git2::Repository::open(host.repo())
            .and_then(|repo| repo.head()?.peel_to_commit().map(|c| c.id().to_string()))
            .expect("the fixture has a HEAD");
        Known {
            workspace: host.daemon.workspaces()[0].id,
            project,
            repository: tree[0].repositories[0].id,
            checkout,
            pane,
            feature,
            task: 1,
            decision: 1,
            commit,
            files: vec!["README.md".into(), "src/lib.rs".into()],
        }
    }
}

macro_rules! kinds {
    ($($kind:ident),* $(,)?) => {
        /// A [`ClientMsg`] variant, without its fields.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Kind { $($kind),* }

        impl Kind {
            pub const ALL: &'static [Kind] = &[$(Kind::$kind),*];
        }
    };
}

kinds! {
    Subscribe, Unsubscribe, Input, Paste, PasteFile, Scrollback, Resize, SpawnShell, SpawnAgent,
    SpawnConsole, Kill, RenamePane, MoveToCheckout, CreateWorktree, RemoveCheckout, OpenWorkspace,
    CreateWorkspace, Review, ReviewContext, ListCode, ReadCode, SearchCode, OpenEdit, Edit, CloseEdit,
    ListDir, ListCommits,
    ListCommitFiles, ListSessions, ReadSession, ListResumes, ResumeSession, ReviewComment, Stage, Commit, Draft,
    GetDecisions, Feature, TrackerMap, Reconcile, Task, Decide, ChangeDecision, SequenceDiagram,
    ListBranches, ListFiles, SwitchBranch, CreateBranch, DeleteBranch, Fetch, Pull, OpenInEditor,
    RemoveProject, RemoveRepository, AddProject, PreviewProject, ListDirectories, AddRepository,
    InitRepository, Restart, Stop, View, WatchFocus, WatchViews, CloseView, RefreshView, PressView,
    WatchSavedViews, RefreshSavedView, MarkSavedViewStale, DeleteSavedView, PressSavedView,
    ExtensionAction, ExtensionCommand, ExtensionDialogAnswer, DescribeExtensions, WatchBrowsers,
    BrowserInput, CloseBrowser, Hello,
}

pub fn kind(msg: &ClientMsg) -> Kind {
    use ClientMsg as M;
    match msg {
        M::Subscribe { .. } => Kind::Subscribe,
        M::Unsubscribe { .. } => Kind::Unsubscribe,
        M::Input { .. } => Kind::Input,
        M::Paste { .. } => Kind::Paste,
        M::PasteFile { .. } => Kind::PasteFile,
        M::Scrollback { .. } => Kind::Scrollback,
        M::Resize { .. } => Kind::Resize,
        M::SpawnShell { .. } => Kind::SpawnShell,
        M::SpawnAgent { .. } => Kind::SpawnAgent,
        M::SpawnConsole { .. } => Kind::SpawnConsole,
        M::Kill { .. } => Kind::Kill,
        M::RenamePane { .. } => Kind::RenamePane,
        M::MoveToCheckout { .. } => Kind::MoveToCheckout,
        M::CreateWorktree { .. } => Kind::CreateWorktree,
        M::RemoveCheckout { .. } => Kind::RemoveCheckout,
        M::OpenWorkspace { .. } => Kind::OpenWorkspace,
        M::CreateWorkspace { .. } => Kind::CreateWorkspace,
        M::Review { .. } => Kind::Review,
        M::ReviewContext { .. } => Kind::ReviewContext,
        M::ListCode { .. } => Kind::ListCode,
        M::ReadCode { .. } => Kind::ReadCode,
        M::SearchCode { .. } => Kind::SearchCode,
        M::OpenEdit { .. } => Kind::OpenEdit,
        M::Edit { .. } => Kind::Edit,
        M::CloseEdit { .. } => Kind::CloseEdit,
        M::ListDir { .. } => Kind::ListDir,
        M::ListCommits { .. } => Kind::ListCommits,
        M::ListCommitFiles { .. } => Kind::ListCommitFiles,
        M::ListSessions { .. } => Kind::ListSessions,
        M::ReadSession { .. } => Kind::ReadSession,
        M::ListResumes { .. } => Kind::ListResumes,
        M::ResumeSession { .. } => Kind::ResumeSession,
        M::ReviewComment { .. } => Kind::ReviewComment,
        M::Stage { .. } => Kind::Stage,
        M::Commit { .. } => Kind::Commit,
        M::Draft { .. } => Kind::Draft,
        M::GetDecisions { .. } => Kind::GetDecisions,
        M::Feature { .. } => Kind::Feature,
        M::TrackerMap { .. } => Kind::TrackerMap,
        M::Reconcile { .. } => Kind::Reconcile,
        M::Task { .. } => Kind::Task,
        M::Decide { .. } => Kind::Decide,
        M::ChangeDecision { .. } => Kind::ChangeDecision,
        M::SequenceDiagram { .. } => Kind::SequenceDiagram,
        M::ListBranches { .. } => Kind::ListBranches,
        M::ListFiles { .. } => Kind::ListFiles,
        M::SwitchBranch { .. } => Kind::SwitchBranch,
        M::CreateBranch { .. } => Kind::CreateBranch,
        M::DeleteBranch { .. } => Kind::DeleteBranch,
        M::Fetch { .. } => Kind::Fetch,
        M::Pull { .. } => Kind::Pull,
        M::OpenInEditor { .. } => Kind::OpenInEditor,
        M::RemoveProject { .. } => Kind::RemoveProject,
        M::RemoveRepository { .. } => Kind::RemoveRepository,
        M::AddProject { .. } => Kind::AddProject,
        M::PreviewProject { .. } => Kind::PreviewProject,
        M::ListDirectories { .. } => Kind::ListDirectories,
        M::AddRepository { .. } => Kind::AddRepository,
        M::InitRepository { .. } => Kind::InitRepository,
        M::Restart => Kind::Restart,
        M::Stop => Kind::Stop,
        M::View(_) => Kind::View,
        M::WatchFocus => Kind::WatchFocus,
        M::WatchViews => Kind::WatchViews,
        M::CloseView { .. } => Kind::CloseView,
        M::RefreshView { .. } => Kind::RefreshView,
        M::PressView { .. } => Kind::PressView,
        M::WatchSavedViews => Kind::WatchSavedViews,
        M::RefreshSavedView { .. } => Kind::RefreshSavedView,
        M::MarkSavedViewStale { .. } => Kind::MarkSavedViewStale,
        M::DeleteSavedView { .. } => Kind::DeleteSavedView,
        M::PressSavedView { .. } => Kind::PressSavedView,
        M::ExtensionAction { .. } => Kind::ExtensionAction,
        M::ExtensionCommand { .. } => Kind::ExtensionCommand,
        M::ExtensionDialogAnswer { .. } => Kind::ExtensionDialogAnswer,
        M::DescribeExtensions => Kind::DescribeExtensions,
        M::WatchBrowsers { .. } => Kind::WatchBrowsers,
        M::BrowserInput { .. } => Kind::BrowserInput,
        M::CloseBrowser { .. } => Kind::CloseBrowser,
        M::Hello(_) => Kind::Hello,
    }
}

/// The strategy for one kind of request.
pub fn generate(kind: Kind, k: &Known) -> BoxedStrategy<ClientMsg> {
    use Kind as K;
    match kind {
        K::Subscribe => panes::subscribe(k),
        K::Unsubscribe => panes::unsubscribe(k),
        K::Input => panes::input(k),
        K::Paste => panes::paste(k),
        K::PasteFile => panes::paste_file(k),
        K::Scrollback => panes::scrollback(k),
        K::Resize => panes::resize(k),
        K::SpawnShell => panes::spawn_shell(k),
        K::SpawnAgent => panes::spawn_agent(k),
        K::SpawnConsole => panes::spawn_console(),
        K::Kill => panes::kill(k),
        K::RenamePane => panes::rename_pane(k),
        K::MoveToCheckout => panes::move_to_checkout(k),
        K::OpenInEditor => panes::open_in_editor(k),
        K::CreateWorktree => git::create_worktree(k),
        K::RemoveCheckout => git::remove_checkout(k),
        K::Review => git::review(k),
        K::ReviewContext => git::review_context(k),
        K::ListCode => git::list_code(k),
        K::ReadCode => git::read_code(k),
        K::SearchCode => git::search_code(k),
        K::OpenEdit => git::open_edit(k),
        K::Edit => git::edit(k),
        K::CloseEdit => git::close_edit(k),
        K::ListDir => git::list_dir(k),
        K::ListCommits => git::list_commits(k),
        K::ListCommitFiles => git::list_commit_files(k),
        K::ListSessions => git::list_sessions(k),
        K::ReadSession => git::read_session(k),
        K::ListResumes => git::list_resumes(),
        K::ResumeSession => git::resume_session(k),
        K::ReviewComment => git::review_comment(k),
        K::Stage => git::stage(k),
        K::Commit => git::commit_index(k),
        K::Draft => git::draft(k),
        K::ListBranches => git::list_branches(k),
        K::ListFiles => git::list_files(k),
        K::SwitchBranch => git::switch_branch(k),
        K::CreateBranch => git::create_branch(k),
        K::DeleteBranch => git::delete_branch(k),
        K::Fetch => git::fetch(k),
        K::Pull => git::pull(k),
        K::GetDecisions => board::get_decisions(k),
        K::Feature => board::feature(k),
        K::TrackerMap => board::tracker_map(k),
        K::Reconcile => board::reconcile(k),
        K::Task => board::task(k),
        K::Decide => board::decide(k),
        K::ChangeDecision => board::change_decision(k),
        K::SequenceDiagram => board::sequence_diagram(k),
        K::OpenWorkspace => panel::open_workspace(k),
        K::CreateWorkspace => panel::create_workspace(),
        K::RemoveProject => panel::remove_project(k),
        K::RemoveRepository => panel::remove_repository(k),
        K::AddProject => panel::add_project(),
        K::PreviewProject => panel::preview_project(),
        K::ListDirectories => panel::list_directories(),
        K::AddRepository => panel::add_repository(k),
        K::InitRepository => panel::init_repository(k),
        K::View => views::view(k),
        K::WatchFocus => Just(ClientMsg::WatchFocus).boxed(),
        K::WatchViews => Just(ClientMsg::WatchViews).boxed(),
        K::CloseView => views::close_view(k),
        K::RefreshView => views::refresh_view(k),
        K::PressView => views::press_view(k),
        K::WatchSavedViews => Just(ClientMsg::WatchSavedViews).boxed(),
        K::RefreshSavedView => views::refresh_saved_view(),
        K::MarkSavedViewStale => views::mark_saved_view_stale(),
        K::DeleteSavedView => views::delete_saved_view(),
        K::PressSavedView => views::press_saved_view(),
        K::ExtensionAction => views::extension_action(),
        K::ExtensionCommand => views::extension_command(k),
        K::ExtensionDialogAnswer => views::extension_dialog_answer(),
        K::DescribeExtensions => Just(ClientMsg::DescribeExtensions).boxed(),
        K::WatchBrowsers => views::watch_browsers(k),
        K::BrowserInput => views::browser_input(k),
        K::CloseBrowser => views::close_browser(k),
        K::Hello => session::hello(),
        K::Restart => Just(ClientMsg::Restart).boxed(),
        K::Stop => Just(ClientMsg::Stop).boxed(),
    }
}

/// Any request but the two that end the daemon, which a sequence can only
/// end with.
pub fn any_but_an_end(k: &Known) -> BoxedStrategy<ClientMsg> {
    let all: Vec<_> = Kind::ALL
        .iter()
        .filter(|kind| !matches!(kind, Kind::Restart | Kind::Stop))
        .map(|&kind| (1, generate(kind, k)))
        .collect();
    proptest::strategy::Union::new_weighted(all).boxed()
}

/// The field a request names itself by, for the kinds that have one.
/// Exhaustive, so a new variant is placed on one side or the other.
pub fn request_id_mut(msg: &mut ClientMsg) -> Option<&mut u64> {
    use ClientMsg as M;
    match msg {
        M::SpawnShell { request_id, .. }
        | M::SpawnAgent { request_id, .. }
        | M::SpawnConsole { request_id }
        | M::CreateWorktree { request_id, .. }
        | M::Review { request_id, .. }
        | M::ReviewContext { request_id, .. }
        | M::ListCode { request_id, .. }
        | M::ReadCode { request_id, .. }
        | M::SearchCode { request_id, .. }
        | M::ListDir { request_id, .. }
        | M::ListCommits { request_id, .. }
        | M::ListSessions { request_id, .. }
        | M::ReadSession { request_id, .. }
        | M::ListResumes { request_id }
        | M::ResumeSession { request_id, .. }
        | M::ReviewComment { request_id, .. }
        | M::Stage { request_id, .. }
        | M::Commit { request_id, .. }
        | M::Draft { request_id, .. }
        | M::Feature { request_id, .. }
        | M::OpenInEditor { request_id, .. }
        | M::AddProject { request_id, .. }
        | M::PreviewProject { request_id, .. }
        | M::ListDirectories { request_id, .. }
        | M::AddRepository { request_id, .. }
        | M::InitRepository { request_id, .. } => Some(request_id),
        M::Subscribe { .. }
        | M::Unsubscribe { .. }
        | M::Input { .. }
        | M::Paste { .. }
        | M::PasteFile { .. }
        | M::Scrollback { .. }
        | M::Resize { .. }
        | M::Kill { .. }
        | M::RenamePane { .. }
        | M::MoveToCheckout { .. }
        | M::RemoveCheckout { .. }
        | M::OpenWorkspace { .. }
        | M::CreateWorkspace { .. }
        | M::ListCommitFiles { .. }
        | M::GetDecisions { .. }
        | M::TrackerMap { .. }
        | M::Reconcile { .. }
        | M::Task { .. }
        | M::Decide { .. }
        | M::ChangeDecision { .. }
        | M::SequenceDiagram { .. }
        | M::ListBranches { .. }
        | M::ListFiles { .. }
        | M::SwitchBranch { .. }
        | M::CreateBranch { .. }
        | M::DeleteBranch { .. }
        | M::Fetch { .. }
        | M::Pull { .. }
        | M::RemoveProject { .. }
        | M::RemoveRepository { .. }
        | M::Restart
        | M::Stop
        | M::View(_)
        | M::WatchFocus
        | M::WatchViews
        | M::CloseView { .. }
        | M::RefreshView { .. }
        | M::PressView { .. }
        | M::WatchSavedViews
        | M::RefreshSavedView { .. }
        | M::MarkSavedViewStale { .. }
        | M::DeleteSavedView { .. }
        | M::PressSavedView { .. }
        | M::ExtensionAction { .. }
        | M::ExtensionCommand { .. }
        | M::ExtensionDialogAnswer { .. }
        | M::DescribeExtensions
        | M::WatchBrowsers { .. }
        | M::BrowserInput { .. }
        | M::CloseBrowser { .. }
        | M::Hello(_)
        | M::OpenEdit { .. }
        | M::Edit { .. }
        | M::CloseEdit { .. } => None,
    }
}

/// Gives every named request in `msgs` an id of its own, counting from
/// one, so an answer can only be the answer to one request. A request
/// generated with zero keeps it: zero asks for no answer.
pub fn name_requests(msgs: &mut [ClientMsg]) {
    let mut next = 0;
    for msg in msgs {
        if let Some(id) = request_id_mut(msg).filter(|id| **id != 0) {
            next += 1;
            *id = next;
        }
    }
}

/// `msg` as it is sent to the host rooted at `root`.
pub fn fill_in(msg: &ClientMsg, root: &Path) -> ClientMsg {
    values::fill_in(msg, root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    fn known() -> Known {
        Known {
            workspace: WorkspaceId(1),
            project: ProjectId(4),
            repository: RepositoryId(2),
            checkout: CheckoutId(3),
            pane: PaneId(5),
            feature: "fuzz".into(),
            task: 1,
            decision: 1,
            commit: "3fd528433e5aba5eb30936cc62a85d8dfdcd4877".into(),
            files: vec!["README.md".into(), "src/lib.rs".into()],
        }
    }

    /// The compiler holds every variant to a kind and every kind to a
    /// strategy; this holds each strategy to its own kind, and each request
    /// to the wire, so a generator that drifted is caught here rather than
    /// as a fuzz failure nobody can read.
    #[test]
    fn every_kind_generates_its_own_variant_and_survives_the_wire() {
        let k = known();
        let mut runner = TestRunner::deterministic();
        for &want in Kind::ALL {
            let strategy = generate(want, &k);
            for _ in 0..32 {
                let msg = strategy.new_tree(&mut runner).unwrap().current();
                assert_eq!(kind(&msg), want, "{msg:?}");
                let filled = fill_in(&msg, Path::new("/tmp/host"));
                let bytes = rmp_serde::to_vec_named(&filled).unwrap();
                let back: ClientMsg = rmp_serde::from_slice(&bytes).unwrap();
                assert_eq!(kind(&back), want, "{filled:?} came back as {back:?}");
            }
        }
    }

    #[test]
    fn every_kind_is_listed_once() {
        let mut seen = std::collections::BTreeSet::new();
        for kind in Kind::ALL {
            assert!(seen.insert(kind), "{kind:?} is listed twice");
        }
    }

    #[test]
    fn named_requests_are_numbered_and_unnamed_ones_left_at_zero() {
        let k = CheckoutId(3);
        let mut msgs = vec![
            ClientMsg::SpawnShell { checkout: k, request_id: 1 },
            ClientMsg::SpawnShell { checkout: k, request_id: 0 },
            ClientMsg::Stop,
            ClientMsg::SpawnShell { checkout: k, request_id: 1 },
        ];
        name_requests(&mut msgs);
        let ids: Vec<_> = msgs.iter_mut().map(|m| request_id_mut(m).copied()).collect();
        assert_eq!(ids, [Some(1), Some(0), None, Some(2)]);
    }

    #[test]
    fn a_path_is_filled_in_under_the_hosts_directory() {
        let msg = ClientMsg::ListDirectories { request_id: 1, path: format!("{}/repo", values::ROOT) };
        let filled = fill_in(&msg, Path::new(r"C:\tmp\host"));
        assert!(matches!(filled, ClientMsg::ListDirectories { ref path, .. } if path == "C:/tmp/host/repo"), "{filled:?}");
    }
}
