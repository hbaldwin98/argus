//! How the sidebar's Views fold lists the saved views: grouped by scope, filtered to where the
//! person is, nested under the view that links to them, each lit while a panel shows it.
//!
//! The page does this in `views/saved/logic.ts` and `drawSavedNav`; here it is a function of the
//! page's data, so the window draws rows and the rules are tested once in Rust. The fold's state
//! (open, here or all, which nested lists are unfolded) is the window's own.

use std::collections::HashSet;

use crate::layout::docks::{DocksPage, Panel};
use crate::views::saved::{SavedPanel, SavedViewsPage};

/// A saved view as the sidebar knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavView {
    pub id: String,
    pub title: String,
    /// What the group is called: `Global`, `argus`, `argus #7`.
    pub scope: String,
    /// About where the person is, which the Here filter keeps.
    pub here: bool,
    pub stale: bool,
    /// The view the sidebar lists this one under, in the same scope.
    pub under: Option<String>,
    /// The panel it opens in on its own, which a drag from the sidebar moves.
    pub panel: Panel,
}

/// The sidebar's view of the page's saved views.
pub fn views_of(page: &SavedViewsPage) -> Vec<NavView> {
    page.views
        .iter()
        .map(|saved| NavView {
            id: saved.id.clone(),
            title: saved.view.title.clone(),
            scope: saved.scope.label.clone(),
            here: saved.here,
            stale: saved.stale,
            under: saved.under.clone(),
            panel: saved.panel.clone(),
        })
        .collect()
}

/// The ids of the views a panel in a dock, or floating, shows now.
pub fn lit(panels: &[SavedPanel], docks: &DocksPage) -> HashSet<String> {
    let shown = |panel: &Panel| {
        [&docks.left, &docks.right, &docks.bottom].iter().any(|dock| dock.shown.as_ref() == Some(panel))
            || docks.floating.iter().any(|float| &float.panel == panel)
    };
    panels.iter().filter(|p| shown(&p.panel)).flat_map(|p| p.columns.iter().cloned()).collect()
}

/// A title as the sidebar lists it under its group's label: an agent often names a view after its
/// repository too (`reliab: the change flow`), and under `reliab` that is said twice. A title that
/// is nothing but the label is kept whole.
pub fn under_group(title: &str, group: &str) -> String {
    let Some(lead) = title.get(..group.len()).filter(|_| !group.is_empty()) else { return title.to_owned() };
    if lead.to_lowercase() != group.to_lowercase() {
        return title.to_owned();
    }
    let rest = title[group.len()..].trim_start();
    let mut chars = rest.chars();
    match chars.next() {
        Some(':' | '-' | '\u{2013}' | '\u{2014}' | '\u{b7}' | '|' | '/') => {
            let after = chars.as_str().trim_start();
            if after.is_empty() { title.to_owned() } else { after.to_owned() }
        }
        _ => title.to_owned(),
    }
}

/// One line of the fold's list, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A scope's label, over the views of it.
    Group(String),
    /// A view. `inner` is how many views are listed under it at every depth; `folded` says whether
    /// they are hidden. `depth` is how far under another it stands.
    View { view: NavView, label: String, depth: usize, inner: usize, folded: bool },
    /// What the Here filter leaves when nothing is about where the person is.
    None,
}

/// The fold's rows: the views in groups by scope, in the order Rust sent them (global first, then
/// repositories, then features), with `here_only` the ones about where the person is. Each view
/// stands under the view named in `under` when the group shows it, else at the top; the views listed
/// under one are hidden unless `unfolded` holds its id.
pub fn rows(views: &[NavView], here_only: bool, unfolded: &HashSet<String>) -> Vec<Row> {
    let mut groups: Vec<(&str, Vec<&NavView>)> = Vec::new();
    for view in views.iter().filter(|view| !here_only || view.here) {
        match groups.last_mut() {
            Some((label, list)) if *label == view.scope => list.push(view),
            _ => groups.push((&view.scope, vec![view])),
        }
    }
    if groups.is_empty() {
        return vec![Row::None];
    }
    let mut out = Vec::new();
    for (label, list) in groups {
        out.push(Row::Group(label.to_owned()));
        let shown: HashSet<&str> = list.iter().map(|view| view.id.as_str()).collect();
        let inside = |parent: Option<&str>| -> Vec<&NavView> {
            list.iter().copied().filter(|view| view.under.as_deref().filter(|under| shown.contains(under)) == parent).collect()
        };
        fn walk<'a>(
            parent: Option<&str>,
            depth: usize,
            label: &str,
            inside: &dyn Fn(Option<&str>) -> Vec<&'a NavView>,
            unfolded: &HashSet<String>,
            out: &mut Vec<Row>,
        ) {
            for view in inside(parent) {
                let count = count_under(&view.id, inside);
                let folded = !unfolded.contains(&view.id);
                out.push(Row::View { view: view.clone(), label: under_group(&view.title, label), depth, inner: count, folded });
                if count > 0 && !folded {
                    walk(Some(&view.id), depth + 1, label, inside, unfolded, out);
                }
            }
        }
        walk(None, 0, label, &inside, unfolded, &mut out);
    }
    out
}

/// How many views a view holds under it, at every depth.
fn count_under<'a>(id: &str, inside: &dyn Fn(Option<&str>) -> Vec<&'a NavView>) -> usize {
    inside(Some(id)).iter().map(|view| 1 + count_under(&view.id, inside)).sum()
}

#[cfg(test)]
mod tests;
