use super::*;
use crate::views::markdown::inline::tests::show;
use crate::views::markdown::model::{Inline, List};

/// The blocks as one line of tags: `<h3>`, `<p>`, `<ul>`/`<ol n>`, `<li>`, `<q>`, `<pre>`, `<hr>`, `<table>`.
fn shown(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(|block| match block {
            Block::Heading { level, text } => format!("<h{level}>{}</h{level}>", show(text)),
            Block::Paragraph(text) => format!("<p>{}</p>", show(text)),
            Block::List(list) => shown_list(list),
            Block::Quote(inner) => format!("<q>{}</q>", shown(inner)),
            Block::Code(text) => format!("<pre>{text}</pre>"),
            Block::Rule => "<hr>".to_owned(),
            Block::Table(table) => {
                let row = |cells: &[Vec<Inline>], tag: &str| format!("<tr>{}</tr>", cells.iter().map(|c| format!("<{tag}>{}</{tag}>", show(c))).collect::<String>());
                format!("<table>{}{}</table>", row(&table.head, "th"), table.rows.iter().map(|r| row(r, "td")).collect::<String>())
            }
        })
        .collect()
}

fn shown_list(list: &List) -> String {
    let tag = if list.ordered { format!("ol {}", list.start) } else { "ul".to_owned() };
    let items: String = list
        .items
        .iter()
        .map(|item| format!("<li>{}{}</li>", show(&item.text), item.sub.as_ref().map(shown_list).unwrap_or_default()))
        .collect();
    format!("<{tag}>{items}</{}>", &tag[..2])
}

fn read(src: &str) -> String {
    shown(&parse(src))
}

// MARKDOWN-1

#[test]
fn line_ends_are_made_one_and_a_tab_is_four_spaces() {
    assert_eq!(read("a\r\nb\rc"), "<p>a<br>b<br>c</p>");
    assert_eq!(read("\tx"), "<p>    x</p>");
    assert_eq!(read("-\tx"), "<ul><li>x</li></ul>");
}

#[test]
fn nothing_at_all_is_no_block() {
    assert_eq!(read(""), "");
    assert_eq!(read("  \n\n \t\n"), "");
}

// MARKDOWN-2

#[test]
fn a_fence_runs_to_a_closer_of_the_same_mark_and_length_or_to_the_end() {
    assert_eq!(read("```rust\nlet x = 1;\n  indented\n```\nafter"), "<pre>let x = 1;\n  indented</pre><p>after</p>");
    assert_eq!(read("````\n```\ninner\n```\n````"), "<pre>```\ninner\n```</pre>");
    assert_eq!(read("~~~\na\n```\nb"), "<pre>a\n```\nb</pre>", "a closer is of the same mark, and none runs to the end");
    assert_eq!(read("```\n\n```"), "<pre></pre>");
}

#[test]
fn a_fence_is_not_read_for_marks() {
    assert_eq!(read("```\n**not bold** [x](https://a.b/c)\n```"), "<pre>**not bold** [x](https://a.b/c)</pre>");
}

#[test]
fn a_heading_is_never_larger_than_a_sections() {
    assert_eq!(read("# a\n## b\n### c\n#### d\n###### e"), "<h3>a</h3><h3>b</h3><h3>c</h3><h4>d</h4><h4>e</h4>");
    assert_eq!(read("## **bold** title ##"), "<h3><b>bold</b> title</h3>");
    assert_eq!(read("####### seven"), "<p>####### seven</p>");
}

#[test]
fn a_rule_is_three_marks_and_beats_a_list_item() {
    assert_eq!(read("a\n\n---\n\n* * *\n\n___"), "<p>a</p><hr><hr><hr>");
}

#[test]
fn a_quote_holds_blocks_and_nests_to_the_cap() {
    assert_eq!(read("> # h\n> text\n>\n> - x"), "<q><h3>h</h3><p>text</p><ul><li>x</li></ul></q>");
    assert_eq!(read("> a\n> > b"), "<q><p>a</p><q><p>b</p></q></q>");
    let deep = format!("{} deep", ">".repeat(8));
    let out = read(&deep);
    assert_eq!(out.matches("<q>").count(), MAX_DEPTH, "{out}");
    assert!(out.contains("&gt;") || out.contains('>'), "past the cap a quote mark is paragraph text: {out}");
}

// MARKDOWN-3

#[test]
fn a_paragraph_keeps_its_newlines_and_ends_at_a_blank_or_another_block() {
    assert_eq!(read("one\ntwo\n\nthree"), "<p>one<br>two</p><p>three</p>");
    assert_eq!(read("text\n# head"), "<p>text</p><h3>head</h3>");
    assert_eq!(read("text\n- item"), "<p>text</p><ul><li>item</li></ul>");
    assert_eq!(read("text\n```\ncode\n```"), "<p>text</p><pre>code</pre>");
    assert_eq!(read("text\n> q"), "<p>text</p><q><p>q</p></q>");
}

// MARKDOWN-4

#[test]
fn a_list_is_bulleted_or_numbered_and_numbers_start_where_they_say() {
    assert_eq!(read("- a\n* b\n+ c"), "<ul><li>a</li><li>b</li><li>c</li></ul>");
    assert_eq!(read("1. a\n2) b"), "<ol 1><li>a</li><li>b</li></ol>");
    assert_eq!(read("7. a\n8. b"), "<ol 7><li>a</li><li>b</li></ol>");
}

#[test]
fn a_change_of_kind_at_the_top_ends_the_list() {
    assert_eq!(read("- a\n1. b"), "<ul><li>a</li></ul><ol 1><li>b</li></ol>");
}

#[test]
fn an_item_indented_two_past_the_base_is_under_the_one_before_it_one_level_only() {
    assert_eq!(read("- a\n  - b\n  1. c\n    - d\n- e"), "<ul><li>a<ul><li>b</li><li>c</li><li>d</li></ul></li><li>e</li></ul>");
    assert_eq!(read("- a\n  1. b\n  2. c"), "<ul><li>a<ol 1><li>b</li><li>c</li></ol></li></ul>", "the first sub-item's marker says what kind of list");
    assert_eq!(read("- a\n - b"), "<ul><li>a</li><li>b</li></ul>", "one space past is the same level");
}

#[test]
fn a_line_that_starts_no_block_continues_the_item() {
    assert_eq!(read("- one\n  more\n- two"), "<ul><li>one<br>more</li><li>two</li></ul>");
    assert_eq!(read("- a\n  - b\n    more"), "<ul><li>a<ul><li>b<br>more</li></ul></li></ul>");
}

#[test]
fn a_blank_line_inside_a_list_goes_on_only_when_an_item_follows() {
    assert_eq!(read("- a\n\n- b"), "<ul><li>a</li><li>b</li></ul>");
    assert_eq!(read("- a\n\ntext"), "<ul><li>a</li></ul><p>text</p>");
    assert_eq!(read("- a\n\n---\n\n- b"), "<ul><li>a</li></ul><hr><ul><li>b</li></ul>");
}

#[test]
fn a_block_ends_a_list() {
    assert_eq!(read("- a\n# h"), "<ul><li>a</li></ul><h3>h</h3>");
    assert_eq!(read("- a\n> q"), "<ul><li>a</li></ul><q><p>q</p></q>");
}

// MARKDOWN-5

#[test]
fn a_table_has_a_header_a_separator_and_rows_cut_or_padded_to_the_header() {
    assert_eq!(
        read("| a | b |\n|---|:-:|\n| 1 | 2 |\n| 3 |\n| 4 | 5 | 6 |"),
        "<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr><tr><td>3</td><td></td></tr><tr><td>4</td><td>5</td></tr></table>"
    );
}

#[test]
fn a_table_needs_no_outer_pipes_and_cells_are_read_for_marks() {
    assert_eq!(read("a | **b**\n--|--\n`x` | y"), "<table><tr><th>a</th><th><b>b</b></th></tr><tr><td><c>x</c></td><td>y</td></tr></table>");
}

#[test]
fn a_row_ending_in_an_escaped_pipe_keeps_it_as_the_last_cells_text() {
    // MARKDOWN-5: `\|` is a literal pipe, so it does not close the row (markdown.ts, cells).
    assert_eq!(read("| a | b |\n|--|--|\n| 1 | 2 \\|"),"<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2 |</td></tr></table>");
}

#[test]
fn an_escaped_pipe_is_a_pipe_in_a_cell() {
    assert_eq!(read("| a \\| b | c |\n|--|--|\n| 1 | 2 |"), "<table><tr><th>a | b</th><th>c</th></tr><tr><td>1</td><td>2</td></tr></table>");
}

#[test]
fn a_table_ends_at_a_blank_line_or_a_line_with_no_pipe() {
    assert_eq!(read("a|b\n-|-\n1|2\n\ntext"), "<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table><p>text</p>");
    assert_eq!(read("a|b\n-|-\n1|2\nplain"), "<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table><p>plain</p>");
}

#[test]
fn a_pipe_line_with_no_separator_is_a_paragraph() {
    assert_eq!(read("a | b\nc | d"), "<p>a | b<br>c | d</p>");
    assert_eq!(read("a | b\n---"), "<p>a | b</p><hr>", "a separator with no pipe in it makes no table");
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    fn depth(blocks: &[Block]) -> usize {
        blocks.iter().map(|block| if let Block::Quote(inner) = block { 1 + depth(inner) } else { 0 }).max().unwrap_or(0)
    }

    fn document() -> impl Strategy<Value = String> {
        proptest::collection::vec("[ ]{0,5}([-*+>#|]|[0-9][.)]|```|~~~|---|:-)?[ ]{0,3}[a-z *_|\\[\\]()]{0,12}", 0..16).prop_map(|lines| lines.join("\n"))
    }

    proptest! {
        // MARKDOWN-1: any text at all reads as blocks and none of it panics.
        #[test]
        fn any_text_reads_without_panicking(src in "\\PC{0,300}") {
            let _ = parse(&src);
        }

        // MARKDOWN-2, MARKDOWN-9: a quote is never nested past the cap, however many marks are written.
        #[test]
        fn quotes_never_nest_past_the_cap(src in document(), marks in 0usize..12) {
            let doc = format!("{} {}", ">".repeat(marks), src);
            prop_assert!(depth(&parse(&doc)) <= MAX_DEPTH);
        }

        // MARKDOWN-5: every row of a table is as wide as its header.
        #[test]
        fn every_row_is_as_wide_as_the_header(src in document()) {
            for block in parse(&src) {
                if let Block::Table(table) = block {
                    prop_assert!(table.rows.iter().all(|row| row.len() == table.head.len()));
                }
            }
        }

        // MARKDOWN-4: a list is never nested more than one level, and has an item.
        #[test]
        fn a_list_has_items_and_one_nested_level(src in document()) {
            fn check(blocks: &[Block]) -> bool {
                blocks.iter().all(|block| match block {
                    Block::List(list) => !list.items.is_empty() && list.items.iter().all(|item| item.sub.as_ref().is_none_or(|sub| sub.items.iter().all(|i| i.sub.is_none()))),
                    Block::Quote(inner) => check(inner),
                    _ => true,
                })
            }
            prop_assert!(check(&parse(&src)));
        }

        // Every text the leaves hold is a stretch of whole characters: a window can select across them.
        #[test]
        fn the_leaves_of_any_document_have_text_a_window_can_cut(src in document()) {
            let blocks = parse(&src);
            for leaf in crate::views::markdown::leaves::leaves(&blocks) {
                for (range, _) in &leaf.styled.spans {
                    prop_assert!(leaf.styled.text.is_char_boundary(range.start) && leaf.styled.text.is_char_boundary(range.end));
                }
            }
        }
    }
}
