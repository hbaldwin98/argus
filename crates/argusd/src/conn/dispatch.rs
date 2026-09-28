//! What each client message does, grouped by what it touches. A concern's
//! messages are handled in one function here, and `dispatch` tries each in
//! turn.

use super::*;
use crate::state::FeaturePart;
use argus_protocol::Created;

pub(super) fn dispatch(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    subs: &mut Subscriptions,
    review_task: &mut Option<tokio::task::JoinHandle<()>>,
    viewer: ViewerId,
) -> anyhow::Result<()> {
    dispatch_pane(msg, daemon, out_tx, subs, viewer)
        .or_else(|msg| dispatch_boards(msg, daemon, out_tx))
        .or_else(|msg| dispatch_panel(msg, daemon, out_tx))
        .or_else(|msg| dispatch_git(msg, daemon, out_tx))
        .or_else(|msg| dispatch_checkout_reads(msg, daemon, out_tx))
        .or_else(|msg| dispatch_review(msg, daemon, out_tx, review_task))
        .unwrap_or_else(|_| unreachable!("every client message is dispatched"))
}

/// The panes themselves: streaming one, typing into it, starting one — an
/// editor included — and closing it.
fn dispatch_pane(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    subs: &mut Subscriptions,
    viewer: ViewerId,
) -> DispatchResult {
    let result = match msg {
        ClientMsg::Subscribe { pane } => super::subscribe(daemon, pane, subs.runs)
            .map(|(snapshot, rx)| subs.add(pane, snapshot, rx, daemon.clone())),
        ClientMsg::Unsubscribe { pane } => {
            subs.remove(pane);
            daemon.release_pane_size(viewer, pane);
            Ok(())
        }
        ClientMsg::Input { pane, bytes } => daemon.write_pane(pane, &bytes),
        ClientMsg::Paste { pane, text } => daemon.paste_pane(pane, &text),
        ClientMsg::Resize { pane, rows, cols } => daemon.resize_pane(viewer, pane, rows, cols),
        ClientMsg::Scrollback { pane, offset } => daemon
            .pane_scrollback(pane, offset as usize)
            .map(|(cells, offset, depth)| {
                let _ = out_tx.send(ServerMsg::ScrollbackRows {
                    pane,
                    offset: offset as u32,
                    depth: depth as u32,
                    cells,
                });
            }),
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
            request_id,
        } => {
            let daemon = daemon.clone();
            spawn_pane(out_tx, request_id, move || {
                daemon.spawn_agent(checkout, &template)
            })
        }
        ClientMsg::Kill { pane } => daemon.close_pane(pane),
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
        msg => return Err(msg),
    };
    Ok(result)
}

/// Features, their decision boards and their task lists. A board is read at
/// project scope, whole, and pushed at every client rather than answered to
/// one.
fn dispatch_boards(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
) -> DispatchResult {
    let result = match msg {
        ClientMsg::GetDecisions { project, checkout } => {
            daemon.decision_board(project, checkout).map(|board| {
                let _ = out_tx.send(ServerMsg::Decisions(Box::new(board)));
            })
        }
        ClientMsg::OpenFeature {
            project,
            checkout,
            write,
        } => daemon.open_feature_for_client(project, checkout, write),
        ClientMsg::RenameFeature {
            project,
            checkout,
            slug,
            title,
        } => daemon.rename_feature_for_client(project, checkout, &slug, &title),
        ClientMsg::RemoveFeature {
            project,
            checkout,
            slug,
        } => daemon.remove_feature_for_client(project, checkout, &slug),
        ClientMsg::TransferFeature {
            project,
            source,
            destination,
            slug,
        } => daemon.transfer_feature_for_client(project, source, destination, &slug),
        ClientMsg::SetFeatureBody {
            project,
            checkout,
            slug,
            body,
        } => daemon.set_feature_body_for_client(project, checkout, &slug, body),
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
        ClientMsg::MoveFeature {
            project,
            checkout,
            slug,
            state,
            detail,
        } => daemon.move_feature_for_client(project, checkout, &slug, state, detail),
        msg => return Err(msg),
    };
    Ok(result)
}

/// The rows of the panel and which workspace is open. The directory
/// browser rides along because it is how a project or repository is added.
fn dispatch_panel(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
) -> DispatchResult {
    let result = match msg {
        ClientMsg::AddProject { path, request_id } => answered(
            out_tx,
            request_id,
            daemon.add_project(&path).map(Created::Project),
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
        msg => return Err(msg),
    };
    Ok(result)
}

/// Writes to Git. Each is real subprocess I/O, so it runs on its own task and
/// reports a refusal on its own rather than stalling this connection.
fn dispatch_git(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
) -> DispatchResult {
    let result = match msg {
        // Both do real subprocess I/O (`git worktree add`/`remove`), so they
        // run on their own task instead of blocking this connection's
        // message loop — a slow worktree op must not stall keystrokes going
        // to some other pane. Each reports its own error asynchronously.
        ClientMsg::CreateWorktree {
            checkout,
            branch,
            request_id,
        } => {
            let answer = out_tx.clone();
            spawn_reporting(daemon, out_tx, move |d| async move {
                // Answered before setup runs: the worktree is there, and
                // setup can take as long as an install does.
                let made = d.create_worktree(checkout, branch).await;
                let created = made.as_ref().ok().map(|id| Created::Checkout(*id));
                answer_created(&answer, request_id, created);
                d.setup_worktree(made?).await
            })
        }
        ClientMsg::RemoveCheckout { checkout } => {
            spawn_reporting(daemon, out_tx, move |d| async move {
                d.remove_checkout(checkout).await
            })
        }
        // Same reasoning: `git init` is a subprocess, and the directory it
        // lands in may not exist yet.
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
        // Not `spawn_reporting`: an unmerged branch comes back as a
        // question for the user rather than as an error, and only this
        // call has one to send.
        ClientMsg::DeleteBranch {
            checkout,
            branch,
            force,
        } => {
            let daemon = daemon.clone();
            let out_tx = out_tx.clone();
            tokio::spawn(async move {
                let msg = match daemon.delete_branch(checkout, &branch, force).await {
                    Ok(BranchDeletion::Deleted) => return,
                    Ok(BranchDeletion::NotMerged) => {
                        ServerMsg::BranchNotMerged { checkout, branch }
                    }
                    Err(error) => ServerMsg::Error {
                        message: error.to_string(),
                    },
                };
                let _ = out_tx.send(msg);
            });
            Ok(())
        }
        msg => return Err(msg),
    };
    Ok(result)
}

/// What a checkout contains, for the pickers and the history view. Each
/// walks the working tree or the object store, so it answers from a
/// blocking thread.
fn dispatch_checkout_reads(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
) -> DispatchResult {
    let result = match msg {
        // Listing walks a working tree, so it goes off the message loop.
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
        msg => return Err(msg),
    };
    Ok(result)
}

/// Reviewing a checkout's changes, and commenting on them to an agent.
fn dispatch_review(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    review_task: &mut Option<tokio::task::JoinHandle<()>>,
) -> DispatchResult {
    let result = match msg {
        ClientMsg::Review {
            request_id,
            checkout,
            base,
            commit,
        } => daemon.checkout_path(checkout).map(|path| {
            if let Some(task) = review_task.take() {
                task.abort();
            }
            let out_tx = out_tx.clone();
            *review_task = Some(tokio::spawn(async move {
                let Ok(permit) = REVIEW_PERMIT.acquire().await else {
                    return;
                };
                let generated = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    match commit.as_deref() {
                        Some(rev) => crate::diff::generate_commit(&path, rev),
                        None => crate::diff::generate(&path, base),
                    }
                })
                .await;
                let message = match generated {
                    Ok(Ok(generated)) => ServerMsg::Review(argus_protocol::Review {
                        request_id,
                        checkout,
                        base: if generated.commit.is_some() {
                            argus_protocol::ReviewBase::Commit
                        } else {
                            base
                        },
                        files: generated.files,
                        commit: generated.commit,
                    }),
                    Ok(Err(error)) => ServerMsg::ReviewFailed {
                        request_id,
                        checkout,
                        message: error.to_string(),
                    },
                    Err(error) => ServerMsg::ReviewFailed {
                        request_id,
                        checkout,
                        message: error.to_string(),
                    },
                };
                let _ = out_tx.send(message);
            }));
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
        msg => return Err(msg),
    };
    Ok(result)
}

type DispatchResult = Result<anyhow::Result<()>, ClientMsg>;

/// Runs a daemon call on its own task, reporting a refusal to the client
/// that asked for it.
///
/// Every git mutation has this shape: real subprocess I/O that must not
/// stall this connection's message loop, nothing to answer with when it
/// works, and a message worth showing when it does not.
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
