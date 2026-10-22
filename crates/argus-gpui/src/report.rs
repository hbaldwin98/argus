//! What the window heard, in words: the whole of it on one line for the unattended run, and the
//! pieces the probes print at each step.

use argus_gpui::connection::heard::Heard;

/// The history as the Git stage is sent it, on one line.
pub fn history(heard: &Heard) -> String {
    let Some(page) = &heard.history else {
        return "history=none".to_owned();
    };
    format!(
        "history={} branch={} rows={} commits={} files={} on={:?} loading={} empty={:?} filter={:?} path={:?} diff_rows={} diff_loading={} diff_empty={:?}",
        page.name,
        page.branch,
        page.rows.len(),
        page.rows.iter().filter(|row| row.kind == "commit").count(),
        page.rows.iter().filter(|row| row.kind == "file").count(),
        page.rows.iter().position(|row| row.on),
        page.loading,
        page.empty,
        page.filter,
        page.path,
        page.diff.len(),
        page.diff_loading,
        page.diff_empty,
    )
}

/// The review as the Git stage is sent it, on one line.
pub fn review(heard: &Heard) -> String {
    let Some(page) = &heard.review else {
        return "review=none".to_owned();
    };
    format!(
        "review={} base={} files={} rows={} file={:?} split={} loading={} empty={:?} +{} -{} unstaged={:?} staged={:?} comment={} jump={}",
        page.name,
        page.base,
        page.files.len(),
        page.rows.len(),
        page.file,
        page.split,
        page.loading,
        page.empty,
        page.added,
        page.removed,
        page.unstaged,
        page.staged,
        page.comment.is_some(),
        page.jump.is_some(),
    )
}

/// What the window heard, on one line.
pub fn line(heard: &Heard) -> String {
    let rail = &heard.rail;
    let repositories = rail.project.as_ref().map_or(0, |p| p.repositories.len());
    let checkouts = rail.project.iter().flat_map(|p| &p.repositories).map(|r| r.checkouts.len()).sum::<usize>();
    let listed: Vec<String> = rail
        .project
        .iter()
        .flat_map(|p| &p.repositories)
        .flat_map(|r| &r.checkouts)
        .flat_map(|c| &c.panes)
        .map(|pane| format!("{}:{}:{}", pane.id, if pane.agent { "agent" } else { "shell" }, pane.status))
        .collect();
    let panes = listed.len();
    format!(
        "argus-gpui heard: connection={:?} loaded={} projects={} project={} repositories={} checkouts={} panes={} [{}] shown={} docks=[{}] console=[{}] git_up={} {} screen={}",
        heard.connection(),
        rail.loaded,
        rail.projects.len(),
        rail.project.as_ref().map_or("-", |p| p.name.as_str()),
        repositories,
        checkouts,
        panes,
        listed.join(" "),
        rail.shown.as_ref().map_or("-", |s| s.title.as_str()),
        docks(&heard.docks),
        console(&heard.console),
        heard.git_up(),
        review(heard),
        screen(heard),
    )
}

/// The docks as they are drawn: each one's size and tabs, the shown one marked.
fn docks(docks: &argus_desktop_core::layout::docks::DocksPage) -> String {
    [("left", &docks.left), ("right", &docks.right), ("bottom", &docks.bottom)]
        .into_iter()
        .map(|(name, page)| {
            let tabs: Vec<String> = page
                .tabs
                .iter()
                .map(|tab| format!("{}{}", if page.shown.as_ref() == Some(&tab.panel) { "*" } else { "" }, tab.title))
                .collect();
            let open = if page.shown.is_some() { page.size.to_string() } else { "closed".to_owned() };
            format!("{name} {open} {}", tabs.join("|"))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The console as it is drawn: where it is, a tab per console (the ones on a
/// side marked), and which consoles the sides show and type in.
fn console(page: &argus_desktop_core::panes::console::ConsolePage) -> String {
    let tabs: Vec<String> =
        page.tabs.iter().map(|tab| format!("{}:{}:{}{}", tab.pane, tab.title, tab.status, if tab.on { "*" } else { "" })).collect();
    let pane = |pane: Option<u64>| pane.map_or("-".to_owned(), |pane| pane.to_string());
    format!(
        "{:?} tabs={} [{}] first={} second={} typing={} spawn={}",
        page.place,
        tabs.len(),
        tabs.join(" "),
        pane(page.showing),
        pane(page.beside),
        pane(page.typing),
        page.can_spawn
    )
}

/// The pane's screen on one line: its grid, and the first row with text on it.
pub fn screen(heard: &Heard) -> String {
    let screen = &heard.screen;
    let Some(pane) = screen.pane() else {
        return "none".to_owned();
    };
    let (rows, cols) = screen.size();
    let first: String = (0..rows)
        .map(|row| screen.row_text(row))
        .find(|text| !text.is_empty())
        .unwrap_or_default()
        .chars()
        .take(40)
        .collect();
    let cursor = screen.cursor();
    format!("pane {pane} {rows}x{cols} cursor=({},{}) first={first:?}", cursor.row, cursor.col)
}
