//! What a client may tell a checkout's index, and the commit made of it:
//! files and picked lines staged or given back at once, and `git commit`.
//!
//! Every write is the `git` executable's, as in `git_ops`; only the content
//! of a partly staged file is worked out here, by `review::picked`.

use argus_protocol::{is_checkout_relative, CheckoutId, IndexChange};

use super::git_ops::{run_git, run_git_fed};
use super::*;

impl Daemon {
    /// Writes `change` to the checkout's index, then tells every client.
    pub async fn stage(&self, checkout: CheckoutId, change: IndexChange) -> anyhow::Result<()> {
        let root = self.checkout_path(checkout)?;
        match change {
            IndexChange::Stage { paths } => {
                let paths = checked_paths(&paths)?;
                run_git(&root, &[&["--literal-pathspecs", "add", "-A", "--"][..], &paths].concat()).await?;
            }
            // `reset` reads an unborn HEAD as an empty tree, so a file
            // added before the first commit leaves the index.
            IndexChange::Unstage { paths } => {
                let paths = checked_paths(&paths)?;
                run_git(&root, &[&["--literal-pathspecs", "reset", "-q", "--"][..], &paths].concat()).await?;
            }
            IndexChange::Lines { from, path, lines } => {
                checked_paths(std::slice::from_ref(&path))?;
                let read = root.clone();
                let file = path.clone();
                let picked = tokio::task::spawn_blocking(move || {
                    crate::review::picked::pick(&read, from, &file, &lines)
                })
                .await??;
                match picked.entry {
                    Some((mode, blob)) => {
                        let info = format!("{mode:o},{blob},{path}");
                        run_git(&root, &["update-index", "--add", "--cacheinfo", &info]).await?;
                    }
                    None => run_git(&root, &["update-index", "--force-remove", "--", &path]).await?,
                }
            }
        }
        self.after_git_write(checkout);
        Ok(())
    }

    /// Commits the index with the person's own git, which runs their hooks
    /// and signs as they have it set to; its refusal, an empty index
    /// included, is passed back as it stands.
    pub async fn commit(&self, checkout: CheckoutId, message: &str) -> anyhow::Result<()> {
        if message.trim().is_empty() {
            anyhow::bail!("a commit needs a message");
        }
        let root = self.checkout_path(checkout)?;
        run_git_fed(&root, &["commit", "-q", "-F", "-"], Some(message.as_bytes())).await?;
        self.after_git_write(checkout);
        Ok(())
    }
}

/// `paths` when every one stays inside the checkout; a pathspec git would
/// read as a flag or a walk out of it is refused before git sees it.
fn checked_paths(paths: &[String]) -> anyhow::Result<Vec<&str>> {
    if paths.is_empty() {
        anyhow::bail!("no files were named");
    }
    paths
        .iter()
        .map(|path| match is_checkout_relative(path) {
            true => Ok(path.as_str()),
            false => Err(anyhow::anyhow!("{path} is not a file in this checkout")),
        })
        .collect()
}
