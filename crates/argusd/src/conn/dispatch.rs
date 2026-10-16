//! What each client message does: one exhaustive match, so a message added
//! to the protocol does not compile until it is given an arm here.
//!
//! Slow work leaves the message loop through the helper that fits its
//! shape:
//! - `spawn_reporting`: async subprocess work whose refusal is reported
//!   as an Error; any answer is sent from inside the task.
//! - `spawn_pane`: a blocking spawn, answered with `Created::Pane`.
//! - `answered`/`answer_created`: answering a named request.
//! - `reply_with`: a blocking read of one checkout.
//! - `ReviewLane`: one review at a time, one place in that turn per
//!   connection; an ask repeated while its twin waits joins it, and every
//!   ask is answered.
//! - a `Daemon` method that returns a closure: state taken under the lock
//!   on the loop, the file I/O done off it.

use super::*;
use crate::state::FeaturePart;
use crate::review_queue::ReviewAsk;
use argus_protocol::{Created, ReviewBase};

/// What one review generation comes to: the diff, or why it could not be read.
pub(super) type Generated = Result<crate::diff::GeneratedReview, String>;

/// What the loop does once a message is handled.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Continue,
    /// The acknowledgement is queued; the loop waits for it to be flushed
    /// before telling the daemon.
    Shutdown(ShutdownReason),
}

pub(super) fn dispatch(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    conn: &mut Connection,
) -> Outcome {
    let result = match msg {
        // This connection itself: who it is, what it watches, and the
        // daemon's lifetime.
        ClientMsg::Hello(hello) => {
            greet(daemon, out_tx, conn, hello);
            Ok(())
        }
        ClientMsg::View(view) => {
            daemon.report_view(conn.viewer, view);
            Ok(())
        }
        ClientMsg::WatchFocus => {
            let watch = conn.focus_watch.insert(FocusWatch::default());
            let focus = daemon.focus_in(&daemon.snapshot());
            let _ = out_tx.send(watch.first(focus));
            Ok(())
        }
        ClientMsg::WatchViews => {
            conn.agent_views = Some(replay_views(daemon, out_tx));
            Ok(())
        }
        ClientMsg::Restart => {
            let _ = out_tx.send(ServerMsg::Restarting);
            return Outcome::Shutdown(ShutdownReason::Restart);
        }
        ClientMsg::Stop => {
            let _ = out_tx.send(ServerMsg::Stopping);
            return Outcome::Shutdown(ShutdownReason::Stop);
        }

        // The panes themselves: streaming one, typing into it, starting
        // one — an editor included — and closing it.
        ClientMsg::Subscribe { pane } => super::subscribe(daemon, pane, conn.subs.runs)
            .map(|(snapshot, rx)| conn.subs.add(pane, snapshot, rx, daemon.clone())),
        ClientMsg::Unsubscribe { pane } => {
            conn.subs.remove(pane);
            daemon.release_pane_size(conn.viewer, pane);
            Ok(())
        }
        ClientMsg::Input { pane, bytes } => daemon.write_pane(pane, &bytes),
        ClientMsg::Paste { pane, text } => daemon.paste_pane(pane, &text),
        ClientMsg::PasteFile { pane, name, bytes } => {
            daemon.paste_file(pane, &name, &bytes).map(|_| ())
        }
        ClientMsg::Resize { pane, rows, cols } => daemon.resize_pane(conn.viewer, pane, rows, cols),
        ClientMsg::Scrollback { pane, offset, top } => {
            scrollback(daemon, out_tx, conn.subs.runs, pane, offset, top)
        }
        ClientMsg::SpawnShell {
            checkout,
            request_id,
        } => {
            let daemon = daemon.clone();
            spawn_pane(out_tx, request_id, move || daemon.spawn_shell(checkout))
        }
        ClientMsg::SpawnAgent {
            checkout,
            template,
            prompt,
            request_id,
        } => {
            let daemon = daemon.clone();
            spawn_pane(out_tx, request_id, move || {
                daemon.spawn_agent_asking(checkout, &template, prompt.as_deref())
            })
        }
        ClientMsg::SpawnConsole { request_id } => {
            let daemon = daemon.clone();
            spawn_pane(out_tx, request_id, move || daemon.spawn_console())
        }
        ClientMsg::Kill { pane } => daemon.close_pane(pane),
        ClientMsg::RenamePane { pane, title } => daemon.rename_pane(pane, &title),
        ClientMsg::CloseView { pane, id } => {
            daemon.close_view(pane, &id);
            Ok(())
        }
        ClientMsg::RefreshView { pane, id } => {
            daemon.refresh_view(pane, &id);
            Ok(())
        }
        ClientMsg::PressView { pane, press } => {
            let (view, action) = (press.view.clone(), press.action.clone());
            // The press is the person's, so a refusal is theirs to read
            // rather than a line in the daemon's log.
            if let Err(reason) = daemon.press_view(pane, press) {
                let _ = out_tx.send(ServerMsg::ViewPressFailed { pane, view, action, reason });
            }
            Ok(())
        }
        // Saved views are the store's, so there is no pane to name: the id
        // is. Each message needs the capability that brought it, and a
        // client that never listed it is told nothing back.
        ClientMsg::WatchSavedViews if conn.can(SAVED_VIEWS) => {
            conn.saved_views = Some(replay_saved_views(daemon, out_tx));
            Ok(())
        }
        ClientMsg::RefreshSavedView { id } if conn.can(SAVED_VIEWS) => {
            if let Err(reason) = daemon.refresh_saved_view(&id) {
                let _ = out_tx.send(ServerMsg::SavedViewFailed { id, reason });
            }
            Ok(())
        }
        ClientMsg::MarkSavedViewStale { id } if conn.can(SAVED_VIEWS) => {
            if let Err(reason) = daemon.mark_saved_view_stale(&id) {
                let _ = out_tx.send(ServerMsg::SavedViewFailed { id, reason });
            }
            Ok(())
        }
        ClientMsg::DeleteSavedView { id } if conn.can(SAVED_VIEWS) => daemon.delete_saved_view(&id).map(|_| ()),
        ClientMsg::PressSavedView { id, press } if conn.can(SAVED_VIEWS) => {
            if let Err(reason) = daemon.press_saved_view(&id, press) {
                let _ = out_tx.send(ServerMsg::SavedViewFailed { id, reason });
            }
            Ok(())
        }
        ClientMsg::WatchSavedViews
        | ClientMsg::RefreshSavedView { .. }
        | ClientMsg::MarkSavedViewStale { .. }
        | ClientMsg::DeleteSavedView { .. }
        | ClientMsg::PressSavedView { .. } => {
            tracing::debug!("a client sent a saved-view message without listing SAVED_VIEWS");
            Ok(())
        }
        ClientMsg::OpenInEditor {
            checkout,
            path,
            line,
            external,
            command,
            request_id,
        } => {
            let daemon = daemon.clone();
            spawn_pane(out_tx, request_id, move || {
                daemon.spawn_editor(checkout, &path, line, external, command.as_deref())
            })
        }

        // Features, their decision boards and their task lists. A board is
        // read at project scope, whole, and pushed at every client rather
        // than answered to one.
        ClientMsg::GetDecisions { project, checkout } => {
            daemon.decision_board(project, checkout).map(|board| {
                let _ = out_tx.send(ServerMsg::Decisions(Box::new(board)));
            })
        }
        ClientMsg::Feature {
            project,
            checkout,
            action,
        } => daemon.feature_action_for_client(project, checkout, action),
        ClientMsg::Task {
            project,
            checkout,
            feature,
            action,
        } => {
            // A read is answered to the client that asked; a change reaches
            // every client through the push instead.
            let read = action.is_read();
            daemon
                .part_action_for_client(project, checkout, &feature, action)
                .map(|list| {
                    if read {
                        let _ = out_tx.send(ServerMsg::Tasks(Box::new(list)));
                    }
                })
        }
        ClientMsg::Decide {
            project,
            checkout,
            feature,
            write,
        } => daemon.decide_for_client(project, checkout, &feature, write),
        ClientMsg::ChangeDecision {
            project,
            checkout,
            feature,
            change,
        } => daemon.change_decision_for_client(project, checkout, &feature, change),
        ClientMsg::SequenceDiagram {
            project,
            checkout,
            feature,
            action,
        } => {
            let read = action.is_read();
            daemon
                .part_action_for_client(project, checkout, &feature, action)
                .map(|list| {
                    if read {
                        let _ = out_tx.send(ServerMsg::SequenceDiagrams(Box::new(list)));
                    }
                })
        }
        ClientMsg::TrackerMap {
            project,
            checkout,
            change,
        } => daemon.change_tracker_map_for_client(project, checkout, change),
        ClientMsg::Reconcile {
            project,
            checkout,
            slug,
            action,
        } => daemon.reconcile_for_client(project, checkout, &slug, action),

        // The rows of the panel and which workspace is open. The directory
        // browser rides along because it is how a project or repository is
        // added.
        //
        // `git init` is a subprocess, so a new repository is made on its
        // own task, as `InitRepository` is.
        ClientMsg::AddProject {
            path,
            request_id,
            name,
            skip,
            init: true,
        } => {
            let answer = out_tx.clone();
            spawn_reporting(daemon, out_tx, move |d| async move {
                let made = d.init_project(&path, name.as_deref(), &skip).await;
                answered(&answer, request_id, made.map(Created::Project))
            })
        }
        ClientMsg::AddProject {
            path,
            request_id,
            name,
            skip,
            init: false,
        } => answered(
            out_tx,
            request_id,
            daemon
                .add_project(&path, name.as_deref(), &skip)
                .map(Created::Project),
        ),
        ClientMsg::AddRepository {
            project,
            path,
            request_id,
        } => answered(
            out_tx,
            request_id,
            daemon.add_repository(project, &path).map(Created::Repository),
        ),
        // Removal only rewrites config and the tree — no subprocess, no
        // directory walk — so it stays on the message loop like AddProject.
        ClientMsg::RemoveProject { project } => daemon.remove_project(project),
        ClientMsg::RemoveRepository { repository } => daemon.remove_repository(repository),
        ClientMsg::OpenWorkspace { workspace } => daemon.open_workspace(workspace),
        ClientMsg::CreateWorkspace { name } => daemon.create_workspace(&name),
        // Not tied to a checkout — the browser roams the whole filesystem —
        // so it cannot go through `reply_with`. Off the message loop all
        // the same: a directory on a cold network drive takes its time.
        ClientMsg::ListDirectories { request_id, path } => {
            let out_tx = out_tx.clone();
            tokio::task::spawn_blocking(move || {
                let mut listing = crate::browse::directories(&path);
                listing.request_id = request_id;
                let _ = out_tx.send(ServerMsg::Directories(listing));
            });
            Ok(())
        }
        // A scan up to eight folders deep, off the loop for the same reason.
        ClientMsg::PreviewProject {
            request_id,
            path,
            skip,
        } => {
            let out_tx = out_tx.clone();
            tokio::task::spawn_blocking(move || {
                let mut preview = crate::browse::project_preview(&path, &skip);
                preview.request_id = request_id;
                let _ = out_tx.send(ServerMsg::ProjectPreview(preview));
            });
            Ok(())
        }

        // Writes to Git. Each is real subprocess I/O — a slow worktree op
        // must not stall keystrokes going to some other pane — so it runs
        // on its own task and reports a refusal on its own.
        ClientMsg::CreateWorktree {
            checkout,
            branch,
            request_id,
        } => create_worktree(daemon, out_tx, checkout, branch, request_id),
        ClientMsg::RemoveCheckout { checkout } => {
            spawn_reporting(daemon, out_tx, move |d| async move {
                d.remove_checkout(checkout).await
            })
        }
        // `git init` again, and the directory it lands in may not exist yet.
        ClientMsg::InitRepository {
            project,
            path,
            request_id,
        } => {
            let answer = out_tx.clone();
            spawn_reporting(daemon, out_tx, move |d| async move {
                let made = d.init_repository(project, &path).await;
                answered(&answer, request_id, made.map(Created::Repository))
            })
        }
        ClientMsg::SwitchBranch { checkout, branch } => {
            spawn_reporting(daemon, out_tx, move |d| async move {
                d.switch_branch(checkout, &branch).await
            })
        }
        ClientMsg::CreateBranch { checkout, branch } => {
            spawn_reporting(daemon, out_tx, move |d| async move {
                d.create_branch(checkout, &branch).await
            })
        }
        ClientMsg::Fetch { checkout } => {
            spawn_reporting(
                daemon,
                out_tx,
                move |d| async move { d.fetch(checkout).await },
            )
        }
        ClientMsg::Pull { checkout } => {
            spawn_reporting(
                daemon,
                out_tx,
                move |d| async move { d.pull(checkout).await },
            )
        }
        ClientMsg::DeleteBranch {
            checkout,
            branch,
            force,
        } => delete_branch(daemon, out_tx, checkout, branch, force),

        // What a checkout contains, for the pickers and the history view.
        // Each walks the working tree or the object store, so it answers
        // from a blocking thread.
        ClientMsg::ListBranches { checkout } => {
            reply_with(daemon, out_tx, checkout, move |path| ServerMsg::Branches {
                checkout,
                branches: crate::browse::branches(&path),
            })
        }
        ClientMsg::ListFiles { checkout } => {
            reply_with(daemon, out_tx, checkout, move |path| ServerMsg::Files {
                checkout,
                files: crate::browse::files(&path),
            })
        }
        ClientMsg::ListCommits {
            request_id,
            checkout,
        } => reply_with(
            daemon,
            out_tx,
            checkout,
            move |path| match crate::diff::list_commits(&path) {
                Ok(commits) => ServerMsg::Commits {
                    request_id,
                    checkout,
                    commits,
                },
                Err(error) => ServerMsg::CommitsFailed {
                    request_id,
                    checkout,
                    message: error.to_string(),
                },
            },
        ),
        ClientMsg::ListCommitFiles { checkout, commit } => reply_with(
            daemon,
            out_tx,
            checkout,
            move |path| match crate::diff::commit_summary(&path, &commit) {
                Ok(files) => ServerMsg::CommitFiles {
                    checkout,
                    commit,
                    files,
                },
                Err(error) => ServerMsg::CommitFilesFailed {
                    checkout,
                    commit,
                    message: error.to_string(),
                },
            },
        ),

        // A repository's agent sessions, what one said, and reopening one.
        // A listing and a read open the harnesses' own files, a listing up
        // to a few hundred of them, so they answer from a blocking thread.
        ClientMsg::ListSessions {
            request_id,
            repository,
        } => daemon.list_sessions(repository).map(|list| {
            let out_tx = out_tx.clone();
            tokio::task::spawn_blocking(move || {
                let sessions = list();
                let _ = out_tx.send(ServerMsg::Sessions {
                    request_id,
                    repository,
                    sessions,
                });
            });
        }),
        ClientMsg::ReadSession {
            request_id,
            checkout,
            harness,
            session_id,
        } => daemon
            .read_session(checkout, &harness, session_id.clone())
            .map(|read| {
                let out_tx = out_tx.clone();
                tokio::task::spawn_blocking(move || {
                    let conversation = Box::new(read());
                    let _ = out_tx.send(ServerMsg::SessionTranscript {
                        request_id,
                        session_id,
                        conversation,
                    });
                });
            }),
        ClientMsg::ListResumes { request_id } => {
            let _ = out_tx.send(ServerMsg::Resumes {
                request_id,
                resumes: daemon.resumes(),
            });
            Ok(())
        }
        ClientMsg::ResumeSession {
            request_id,
            checkout,
            harness,
            session_id,
        } => {
            let daemon = daemon.clone();
            spawn_pane(out_tx, request_id, move || {
                daemon.resume_session(checkout, &harness, &session_id)
            })
        }

        // Reviewing a checkout's changes, and commenting on them to an
        // agent.
        ClientMsg::Review {
            request_id,
            checkout,
            base,
            commit,
        } => review(daemon, out_tx, &mut conn.reviews, request_id, checkout, base, commit),
        // The Code stage: a version's files, one file of it, and a search.
        ClientMsg::ListCode {
            request_id,
            checkout,
            version,
        } => reply_with(daemon, out_tx, checkout, move |repo| {
            match crate::code::list(&repo, &version) {
                Ok((files, changes)) => ServerMsg::CodeTree(argus_protocol::CodeTree {
                    request_id,
                    checkout,
                    version,
                    files,
                    changes,
                }),
                Err(error) => ServerMsg::CodeFailed {
                    request_id,
                    checkout,
                    message: error.to_string(),
                },
            }
        }),
        ClientMsg::ReadCode {
            request_id,
            checkout,
            path,
            version,
        } => reply_with(daemon, out_tx, checkout, move |repo| {
            match crate::code::read(&repo, &path, &version) {
                Ok(file) => ServerMsg::Code(Box::new(argus_protocol::CodeFile {
                    request_id,
                    checkout,
                    path,
                    version,
                    lines: file.lines,
                    removed: file.removed,
                    symbols: file.symbols,
                    note: file.note,
                })),
                Err(error) => ServerMsg::CodeFailed {
                    request_id,
                    checkout,
                    message: error.to_string(),
                },
            }
        }),
        ClientMsg::SearchCode {
            request_id,
            checkout,
            query,
            options,
        } => reply_with(daemon, out_tx, checkout, move |repo| {
            match crate::code_search::search(&repo, &query, &options) {
                Ok((matches, truncated)) => ServerMsg::CodeMatches(argus_protocol::CodeMatches {
                    request_id,
                    checkout,
                    query,
                    options,
                    matches,
                    truncated,
                }),
                Err(error) => ServerMsg::CodeFailed {
                    request_id,
                    checkout,
                    message: error.to_string(),
                },
            }
        }),
        ClientMsg::ReviewContext {
            request_id,
            checkout,
            blob,
            path,
            start,
            end,
        } => reply_with(daemon, out_tx, checkout, move |repo| {
            match crate::diff::context(&repo, &blob, &path, start, end) {
                Ok(lines) => ServerMsg::ReviewContext(argus_protocol::ReviewContext {
                    request_id,
                    checkout,
                    lines,
                }),
                Err(error) => ServerMsg::ReviewContextFailed {
                    request_id,
                    checkout,
                    message: error.to_string(),
                },
            }
        }),
        ClientMsg::ReviewComment {
            checkout,
            recipient,
            anchor,
            body,
        } => daemon
            .submit_review_comment(checkout, recipient, *anchor, body)
            .map(|(id, delivered)| {
                let _ = out_tx.send(ServerMsg::ReviewCommentSaved { id, delivered });
            }),

        // What the person did with what an extension published, on its way
        // to the extension, and what the extensions are, for
        // `argus extension`.
        ClientMsg::DescribeExtensions => {
            let _ = out_tx.send(ServerMsg::ExtensionsDescribed(daemon.describe_extensions()));
            Ok(())
        }
        ClientMsg::ExtensionAction { extension, view, row, action } => daemon.extension_action(&extension, &view, row, &action),
        ClientMsg::ExtensionCommand { extension, command, context } => {
            daemon.extension_command(conn.viewer, &extension, &command, &context)
        }
        ClientMsg::ExtensionDialogAnswer { dialog, answer } => daemon.answer_dialog(conn.viewer, dialog, answer),
    };
    if let Err(e) = result {
        let _ = out_tx.send(ServerMsg::Error {
            message: e.to_string(),
        });
    }
    Outcome::Continue
}

/// Answers a greeting whenever it comes. A client that greets reads on
/// until the answer, keeping the tree and the rest that went out first;
/// one that never greets is never sent a message it cannot read.
fn greet(
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    conn: &mut Connection,
    hello: Hello,
) {
    tracing::info!(
        version = %hello.version,
        protocol = hello.protocol,
        "client greeted"
    );
    conn.subs.runs = hello.can(CELL_RUNS);
    let _ = out_tx.send(ServerMsg::Hello(Hello::this_build()));
    // The tree went before the greeting; the consoles could not, and they
    // follow the daemon's own greeting.
    if hello.can(CONSOLES) {
        let _ = out_tx.send(ServerMsg::Consoles(daemon.consoles()));
    }
    // Likewise what the extensions publish, and whether this client is one
    // a dialog can be put to.
    let takes = hello.can(EXTENSIONS);
    if takes {
        send_extensions(daemon, out_tx);
    }
    daemon.extension_asker(conn.viewer, takes && hello.can(EXTENSION_DIALOGS));
    conn.peer = Some(hello);
}

fn scrollback(
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    runs: bool,
    pane: PaneId,
    offset: u32,
    top: Option<u64>,
) -> anyhow::Result<()> {
    let scrolled = daemon.pane_scrollback(pane, offset as usize, top)?;
    let cells = scrolled.cells;
    let (offset, depth) = (scrolled.offset as u32, scrolled.depth as u32);
    let answer = if runs {
        ServerMsg::ScrollbackRuns {
            pane,
            offset,
            depth,
            rows: cells.len() as u16,
            cols: cells.first().map_or(0, |row| row.len() as u16),
            runs: argus_protocol::grid_runs(&cells),
            top: Some(scrolled.top),
        }
    } else {
        ServerMsg::ScrollbackRows {
            pane,
            offset,
            depth,
            cells,
        }
    };
    let _ = out_tx.send(answer);
    Ok(())
}

fn create_worktree(
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    checkout: argus_protocol::CheckoutId,
    branch: String,
    request_id: u64,
) -> anyhow::Result<()> {
    let answer = out_tx.clone();
    spawn_reporting(daemon, out_tx, move |d| async move {
        // Answered before setup runs: the worktree is there, and setup can
        // take as long as an install does.
        let made = d.create_worktree(checkout, branch).await;
        let created = made.as_ref().ok().map(|id| Created::Checkout(*id));
        answer_created(&answer, request_id, created);
        d.setup_worktree(made?).await
    })
}

/// Not `spawn_reporting`: an unmerged branch comes back as a question for
/// the user rather than as an error, and only this call has one to send.
fn delete_branch(
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    checkout: argus_protocol::CheckoutId,
    branch: String,
    force: bool,
) -> anyhow::Result<()> {
    let daemon = daemon.clone();
    let out_tx = out_tx.clone();
    tokio::spawn(async move {
        let msg = match daemon.delete_branch(checkout, &branch, force).await {
            Ok(BranchDeletion::Deleted) => return,
            Ok(BranchDeletion::NotMerged(commits)) => {
                ServerMsg::BranchNotMerged {
                    checkout,
                    branch,
                    commits,
                }
            }
            Err(error) => ServerMsg::Error {
                message: error.to_string(),
            },
        };
        let _ = out_tx.send(msg);
    });
    Ok(())
}

fn review(
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    reviews: &mut ReviewLane<Generated>,
    request_id: u64,
    checkout: argus_protocol::CheckoutId,
    base: ReviewBase,
    commit: Option<String>,
) -> anyhow::Result<()> {
    let path = daemon.checkout_path(checkout)?;
    let out_tx = out_tx.clone();
    let ask = ReviewAsk::new(checkout, base, commit.clone());
    let generate = move || match commit.as_deref() {
        Some(rev) => crate::diff::generate_commit(&path, rev),
        None => crate::diff::generate(&path, base),
    }
    .map_err(|error| error.to_string());
    reviews.submit(ask, generate, move |generated| {
        let message = match generated.and_then(|generated| generated) {
            Ok(generated) => ServerMsg::Review(argus_protocol::Review {
                request_id,
                checkout,
                base: if generated.commit.is_some() {
                    ReviewBase::Commit
                } else {
                    base
                },
                files: generated.files,
                commit: generated.commit,
            }),
            Err(message) => ServerMsg::ReviewFailed {
                request_id,
                checkout,
                message,
            },
        };
        let _ = out_tx.send(message);
    });
    Ok(())
}

/// Runs a daemon call on its own task, reporting a refusal to the client
/// that asked for it.
fn spawn_reporting<F, Fut>(
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    work: F,
) -> anyhow::Result<()>
where
    F: FnOnce(Arc<Daemon>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = anyhow::Result<()>> + Send,
{
    let daemon = daemon.clone();
    let out_tx = out_tx.clone();
    tokio::spawn(async move {
        if let Err(error) = work(daemon).await {
            let _ = out_tx.send(ServerMsg::Error {
                message: error.to_string(),
            });
        }
    });
    Ok(())
}

pub(super) fn spawn_pane(
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    request_id: u64,
    spawn: impl FnOnce() -> anyhow::Result<PaneId> + Send + 'static,
) -> anyhow::Result<()> {
    let out_tx = out_tx.clone();
    tokio::task::spawn_blocking(move || {
        let made = spawn().map(Created::Pane);
        if let Err(error) = answered(&out_tx, request_id, made) {
            let _ = out_tx.send(ServerMsg::Error {
                message: error.to_string(),
            });
        }
    });
    Ok(())
}

/// Tells the client that named its request what the request made, or that
/// it made nothing. A client that predates the answer names no requests —
/// its id is zero — and so is never sent a message it cannot read.
fn answer_created(
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    request_id: u64,
    created: Option<Created>,
) {
    if request_id != 0 {
        let _ = out_tx.send(ServerMsg::Created {
            request_id,
            created,
        });
    }
}

/// Answers a named request with what it made, and hands a refusal on to be
/// reported the way it always was.
fn answered(
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    request_id: u64,
    made: anyhow::Result<Created>,
) -> anyhow::Result<()> {
    answer_created(out_tx, request_id, made.as_ref().ok().copied());
    made.map(|_| ())
}

/// Resolves a checkout to its path, then answers on a blocking thread.
fn reply_with(
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    checkout: argus_protocol::CheckoutId,
    build: impl FnOnce(std::path::PathBuf) -> ServerMsg + Send + 'static,
) -> anyhow::Result<()> {
    let path = daemon.checkout_path(checkout)?;
    let out_tx = out_tx.clone();
    tokio::task::spawn_blocking(move || {
        let _ = out_tx.send(build(path));
    });
    Ok(())
}

#[cfg(test)]
mod tests;
