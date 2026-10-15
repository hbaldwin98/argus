//! Which saved views a view links to, in the order it names them: what the
//! links list offers while a saved view is open.
//!
//! A link can sit on any value, on the plain-text elements that take `opens`,
//! or on an embed. The walk reaches every one of them, including the kinds
//! the terminal leaves to the desktop, so a link there is still offered here.

use argus_protocol::{Block, RowItem, TreeNode, Value};

/// The ids a view links to, each once, in the order the view reads.
pub fn linked_views(blocks: &[Block]) -> Vec<String> {
    let mut found = Vec::new();
    blocks_links(blocks, &mut found);
    let mut unique: Vec<String> = Vec::new();
    for id in found {
        if !unique.contains(&id) {
            unique.push(id);
        }
    }
    unique
}

fn blocks_links(blocks: &[Block], out: &mut Vec<String>) {
    for block in blocks {
        block_links(block, out);
        for (_, inner) in block.children() {
            blocks_links(inner, out);
        }
    }
}

fn block_links(block: &Block, out: &mut Vec<String>) {
    match block {
        Block::Embed { view } => out.push(view.clone()),
        Block::Section { opens, .. } | Block::Card { opens, .. } | Block::Details { opens, .. } => {
            opened(opens, out)
        }
        Block::Fields { fields } => fields.iter().for_each(|field| value(&field.value, out)),
        Block::Table { rows, .. } => rows.iter().flatten().for_each(|v| value(v, out)),
        Block::List { items } => items.iter().for_each(|item| opened(&item.opens, out)),
        Block::Stats { stats } => stats.iter().for_each(|stat| opened(&stat.opens, out)),
        Block::Tree { nodes } => tree(nodes, out),
        Block::Graph { nodes, .. } => nodes.iter().for_each(|node| opened(&node.opens, out)),
        Block::Timeline { events } => events.iter().for_each(|event| opened(&event.opens, out)),
        Block::Checklist { items } => items.iter().for_each(|item| opened(&item.opens, out)),
        Block::Board { columns } => {
            for card in columns.iter().flat_map(|column| &column.cards) {
                opened(&card.opens, out);
                if let Some(v) = &card.value {
                    value(v, out);
                }
            }
        }
        Block::Row { items, .. } => {
            for item in items {
                if let RowItem::Value { value: v } = item {
                    value(v, out);
                }
            }
        }
        _ => {}
    }
}

fn tree(nodes: &[TreeNode], out: &mut Vec<String>) {
    for node in nodes {
        opened(&node.opens, out);
        if let Some(v) = &node.value {
            value(v, out);
        }
        tree(&node.children, out);
    }
}

fn opened(opens: &Option<String>, out: &mut Vec<String>) {
    out.extend(opens.iter().cloned());
}

fn value(v: &Value, out: &mut Vec<String>) {
    if let Value::View { id, .. } = v {
        out.push(id.clone());
    }
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
    fn a_view_with_no_links_has_none() {
        assert!(linked_views(&blocks(serde_json::json!([{ "kind": "divider" }]))).is_empty());
    }
}
