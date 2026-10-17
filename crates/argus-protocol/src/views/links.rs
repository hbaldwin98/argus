//! Which saved views a view links to, in the order it names them: what the
//! terminal's links list offers while a saved view is open, and what the
//! desktop's sidebar nests a linked view under.
//!
//! A link can sit on any value, on the plain-text elements that take `opens`,
//! or on an embed. The walk reaches every one of them, including the kinds
//! the terminal leaves to the desktop, so a link there is still offered there.

use crate::views::agent_view::{Block, Value};
use crate::views::tree::{leaves_within, Leaf};

/// The ids a view links to, each once, in the order the view reads.
pub fn linked_views(blocks: &[Block]) -> Vec<String> {
    let mut unique: Vec<String> = Vec::new();
    for leaf in leaves_within(blocks) {
        let id = match leaf {
            Leaf::SavedView(id) => id,
            Leaf::Value(Value::View { id, .. }) => id.as_str(),
            Leaf::Value(_) | Leaf::Action(..) => continue,
        };
        if !unique.iter().any(|u| u == id) {
            unique.push(id.to_string());
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(json: serde_json::Value) -> Vec<Block> {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn a_view_links_to_each_view_it_names_once_in_reading_order() {
        let found = linked_views(&blocks(serde_json::json!([
            { "kind": "section", "heading": "a", "opens": "first", "blocks": [
                { "kind": "fields", "fields": [
                    { "label": "x", "value": { "kind": "view", "id": "second" } } ] } ] },
            { "kind": "list", "items": [ { "title": "t", "opens": "first" } ] },
            { "kind": "embed", "view": "third" }
        ])));
        assert_eq!(found, vec!["first", "second", "third"]);
    }

    #[test]
    fn a_link_inside_a_container_the_terminal_leaves_out_is_still_found() {
        let found = linked_views(&blocks(serde_json::json!([
            { "kind": "timeline", "events": [ { "title": "e", "opens": "deep" } ] }
        ])));
        assert_eq!(found, vec!["deep"]);
    }

    #[test]
    fn every_place_a_view_can_link_from_is_found_in_reading_order() {
        let view = |id: &str| serde_json::json!({ "kind": "view", "id": id });
        let found = linked_views(&blocks(serde_json::json!([
            { "kind": "embed", "view": "embed" },
            { "kind": "section", "heading": "s", "blocks": [], "opens": "section" },
            { "kind": "details", "summary": "d", "blocks": [], "opens": "details" },
            { "kind": "card", "opens": "card", "blocks": [
                { "kind": "fields", "fields": [{ "label": "f", "value": view("field") }] } ] },
            { "kind": "table", "columns": [{ "title": "a" }], "rows": [[view("cell")]],
              "details": [[{ "kind": "section", "heading": "s", "blocks": [], "opens": "detail" }]] },
            { "kind": "list", "items": [{ "opens": "item" }] },
            { "kind": "checklist", "items": [{ "label": "c", "done": true, "opens": "check" }] },
            { "kind": "timeline", "events": [{ "title": "e", "opens": "event" }] },
            { "kind": "stats", "stats": [{ "label": "s", "value": "1", "opens": "stat" }] },
            { "kind": "graph", "nodes": [{ "id": "n", "label": "n", "opens": "node" }], "edges": [] },
            { "kind": "tree", "nodes": [{ "label": "a", "opens": "branch", "value": view("branch-value"),
              "children": [{ "label": "b", "opens": "leaf" }] }] },
            { "kind": "board", "columns": [{ "title": "c", "cards": [{ "title": "k", "opens": "board", "value": view("board-value") }] }] },
            { "kind": "row", "items": [{ "kind": "value", "value": view("row") }] },
            { "kind": "embed", "view": "card" }
        ])));
        assert_eq!(
            found,
            vec![
                "embed", "section", "details", "card", "field", "cell", "detail", "item", "check", "event", "stat", "node",
                "branch", "branch-value", "leaf", "board", "board-value", "row",
            ]
        );
    }

    #[test]
    fn a_view_with_no_links_has_none() {
        assert!(linked_views(&blocks(serde_json::json!([{ "kind": "divider" }]))).is_empty());
    }
}
