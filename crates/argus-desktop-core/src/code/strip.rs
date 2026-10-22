//! What the strip says on the Code stage: the checkout and, when it is another word, its branch,
//! after the repository (`code/stage.ts`, `codeStrip`). Back, Open in editor and the version
//! belong to the file, so its header carries them.

use super::stage::CodePage;
use crate::chrome::strip::Part;

/// The trail after the repository, in the mono face: nothing until the stage is open.
pub fn trail(page: Option<&CodePage>) -> Vec<Part> {
    let Some(page) = page.filter(|page| page.open) else { return Vec::new() };
    let mut parts = vec![Part::Mono(page.name.clone())];
    if !page.branch.is_empty() && page.branch != page.name {
        parts.push(Part::Mono(page.branch.clone()));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(name: &str, branch: &str, open: bool) -> CodePage {
        CodePage { open, name: name.into(), branch: branch.into(), ..CodePage::default() }
    }

    #[test]
    fn the_branch_follows_the_checkout_only_when_it_is_another_word() {
        assert_eq!(trail(Some(&page("main", "main", true))), [Part::Mono("main".into())]);
        assert_eq!(trail(Some(&page("desktop", "desktop-design", true))), [Part::Mono("desktop".into()), Part::Mono("desktop-design".into())]);
        assert_eq!(trail(Some(&page("detached", "", true))), [Part::Mono("detached".into())]);
    }

    #[test]
    fn a_stage_not_open_says_nothing() {
        assert!(trail(None).is_empty());
        assert!(trail(Some(&page("main", "main", false))).is_empty());
    }
}
