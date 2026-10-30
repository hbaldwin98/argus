//! Which letters of a file's name the go-to-file field's text lit, so a result shows why it
//! matched (`project/CodeFind.dc.html`: `mod` in `mod.rs`).
//!
//! Core ranks the paths and says only which they are, so the window finds the letters itself: each
//! word of the field is lit where it stands whole in the name, or else letter by letter in order,
//! as a loose match is. A word that is nowhere in the name (it matched the folder) lights nothing.

/// The name cut into stretches, each with whether it is lit.
pub fn lit(name: &str, typed: &str) -> Vec<(String, bool)> {
    let chars: Vec<char> = name.chars().collect();
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let folded: Vec<char> = chars.iter().map(|&c| fold(c)).collect();
    let mut on = vec![false; chars.len()];
    for word in typed.trim_start_matches('#').split_whitespace() {
        let word: Vec<char> = word.chars().map(fold).collect();
        if word.is_empty() || word.len() > folded.len() {
            continue;
        }
        if let Some(start) = (0..=folded.len() - word.len()).find(|&at| folded[at..at + word.len()] == word[..]) {
            on[start..start + word.len()].iter_mut().for_each(|lit| *lit = true);
            continue;
        }
        // Letter by letter in order, for a word the name holds only loosely.
        let mut found = Vec::new();
        let mut from = 0;
        for letter in &word {
            match folded[from..].iter().position(|c| c == letter) {
                Some(at) => {
                    found.push(from + at);
                    from += at + 1;
                }
                None => {
                    found.clear();
                    break;
                }
            }
        }
        found.into_iter().for_each(|at| on[at] = true);
    }
    let mut parts: Vec<(String, bool)> = Vec::new();
    for (c, lit) in chars.into_iter().zip(on) {
        match parts.last_mut() {
            Some((text, was)) if *was == lit => text.push(c),
            _ => parts.push((c.to_string(), lit)),
        }
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(name: &str, typed: &str) -> String {
        lit(name, typed).iter().map(|(text, on)| if *on { format!("[{text}]") } else { text.clone() }).collect()
    }

    /// CodeFind.dc.html: `mod` is lit in `mod.rs`, and in `rail_model.rs` both words are.
    #[test]
    fn a_word_is_lit_where_it_stands_in_the_name() {
        assert_eq!(shown("mod.rs", "rail mod"), "[mod].rs");
        assert_eq!(shown("rail_model.rs", "rail mod"), "[rail]_[mod]el.rs");
    }

    #[test]
    fn case_is_not_asked_and_a_hash_is_not_a_letter() {
        assert_eq!(shown("README.md", "#read"), "[READ]ME.md");
    }

    #[test]
    fn a_word_the_name_holds_only_loosely_is_lit_letter_by_letter_and_one_it_lacks_not_at_all() {
        assert_eq!(shown("status_bar.rs", "stbr"), "[st]atus_[b]a[r].rs");
        assert_eq!(shown("mod.rs", "chrome"), "mod.rs");
    }

    #[test]
    fn an_empty_field_lights_nothing_and_an_empty_name_has_no_stretches() {
        assert_eq!(shown("a.rs", ""), "a.rs");
        assert!(lit("", "a").is_empty());
    }
}
