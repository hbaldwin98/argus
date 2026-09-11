//! What the user changed about the panel at runtime — projects and
//! repositories added or removed, workspaces declared — and the UI state
//! that survives a restart.

use super::*;

impl Store {
    pub fn add_project(&self, overlay: &ProjectOverlay) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO project_overlay (root, name, workspace) VALUES (?1, ?2, ?3)
             ON CONFLICT(root) DO UPDATE SET name = excluded.name, workspace = excluded.workspace",
            rusqlite::params![path_text(&overlay.root), overlay.name, overlay.workspace],
        )?;
        // Adding a project back is the undo for having removed it.
        tx.execute(
            "DELETE FROM project_hidden WHERE name = ?1",
            [&overlay.name],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn project_overlays(&self) -> Result<Vec<ProjectOverlay>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT name, root, workspace FROM project_overlay ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| {
            Ok(ProjectOverlay {
                name: r.get(0)?,
                root: PathBuf::from(r.get::<_, String>(1)?),
                workspace: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Takes a project out of the panel for good.
    ///
    /// One the user added is dropped; one the config declares is recorded
    /// as hidden instead, since taking a row out of the panel is not
    /// permission to edit their file. Which it is answers itself: an
    /// overlay is keyed by root, so a root with a row here is one Argus
    /// added. Extra repositories go with it either way.
    pub fn remove_project(&self, name: &str, root: Option<&Path>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let dropped = match root {
            Some(root) => tx.execute(
                "DELETE FROM project_overlay WHERE root = ?1",
                [path_text(root)],
            )?,
            None => 0,
        };
        tx.execute("DELETE FROM repo_overlay WHERE project = ?1", [name])?;
        if dropped == 0 {
            tx.execute(
                "INSERT OR IGNORE INTO project_hidden (name) VALUES (?1)",
                [name],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn hidden_projects(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT name FROM project_hidden")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn add_repo(&self, project: &str, path: &Path) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO repo_overlay (project, path) VALUES (?1, ?2)",
            rusqlite::params![project, path_text(path)],
        )?;
        Ok(())
    }

    /// Extra repository paths for one project, in the order they were added.
    pub fn repos_for(&self, project: &str) -> Result<Vec<PathBuf>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT path FROM repo_overlay WHERE project = ?1 ORDER BY rowid")?;
        let rows = stmt.query_map([project], |r| Ok(PathBuf::from(r.get::<_, String>(0)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---- excluded repositories ---------------------------------------

    pub fn exclude_repo(&self, path: &Path) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO repo_excluded (path) VALUES (?1)",
            [path_text(path)],
        )?;
        Ok(())
    }

    pub fn excluded_repos(&self) -> Result<Vec<PathBuf>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT path FROM repo_excluded")?;
        let rows = stmt.query_map([], |r| Ok(PathBuf::from(r.get::<_, String>(0)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Replaces the exclusion set with exactly these paths — how exclusions
    /// under a removed project are forgotten.
    pub fn set_excluded_repos(&self, paths: &[PathBuf]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM repo_excluded", [])?;
        {
            let mut stmt = tx.prepare("INSERT INTO repo_excluded (path) VALUES (?1)")?;
            for p in paths {
                stmt.execute([path_text(p)])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ---- workspaces ---------------------------------------------------

    /// Declaring a workspace is what makes an empty one exist at all: a
    /// workspace with no projects has nothing else to imply it.
    pub fn add_workspace(&self, name: &str) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO workspace_overlay (name) VALUES (?1)",
            [name],
        )?;
        Ok(())
    }

    pub fn workspace_overlays(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT name FROM workspace_overlay ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---- UI state ------------------------------------------------------

    /// The workspace that was open when the daemon last exited. The caller
    /// resolves it against the workspaces that actually exist — the name may
    /// since have been removed from the config.
    pub fn open_workspace(&self) -> Result<Option<String>> {
        Ok(self.ui_get("open_workspace")?.filter(|s| !s.is_empty()))
    }

    pub fn set_open_workspace(&self, name: &str) -> Result<()> {
        self.ui_set("open_workspace", name)
    }

    fn ui_get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn();
        Ok(conn
            .query_row("SELECT value FROM ui_state WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    fn ui_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO ui_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    // ---- startup read ---------------------------------------------------

    /// Everything the tree needs from the store, in one call.
    ///
    /// Repositories are gathered per project here rather than left to a
    /// later lookup so that one failure can be reported once, at startup,
    /// instead of silently costing one project its rows.
    pub fn overlays(&self) -> Result<Overlays> {
        let mut projects = Vec::new();
        for overlay in self.project_overlays()? {
            let repos = self.repos_for(&overlay.name)?;
            projects.push((overlay, repos));
        }
        let mut repos = Vec::new();
        {
            let conn = self.conn();
            let mut stmt = conn.prepare("SELECT project, path FROM repo_overlay ORDER BY rowid")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    PathBuf::from(r.get::<_, String>(1)?),
                ))
            })?;
            for row in rows {
                repos.push(row?);
            }
        }
        // A repository listed under an added project is already carried by
        // that project; leaving it here too would install it twice.
        let added: Vec<&str> = projects.iter().map(|(p, _)| p.name.as_str()).collect();
        repos.retain(|(project, _)| !added.contains(&project.as_str()));

        Ok(Overlays {
            projects,
            hidden: self.hidden_projects()?,
            repos,
            workspaces: self.workspace_overlays()?,
            excluded: self.excluded_repos()?,
            open_workspace: self.open_workspace()?,
        })
    }
}
