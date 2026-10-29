use super::*;

/// Handy's own filter cases (`audio_toolkit/text.rs`), as learn-harness's
/// `tests/dictation-local.test.ts` runs them through its port: removal with the language known, then
/// the final normalisation.
fn handy(text: &str, language: Option<&str>) -> String {
    normalize(&remove_filler_words(text, language))
}

const LIVE: Clean<'static> = Clean { remove_fillers: true, language: Some("en"), final_text: false };

#[test]
fn removes_fillers_case_insensitively_with_their_punctuation() {
    assert_eq!(handy("So uhm I was thinking uh about this", Some("en")), "So I was thinking about this");
    assert_eq!(handy("UHM this is UH a test", Some("en")), "This is a test");
    assert_eq!(handy("Well, uhm, I think, uh. that's right", Some("en")), "Well, I think, that's right");
    assert_eq!(handy("  Uhm, so I was, uh, thinking about this  ", Some("en")), "So I was, thinking about this");
}

#[test]
fn a_sentence_opening_filler_hands_its_capital_to_the_next_word() {
    assert_eq!(handy("Um, so I think we should ship it.", Some("en")), "So I think we should ship it.");
    assert_eq!(handy("That works. Um, let me check.", Some("en")), "That works. Let me check.");
    assert_eq!(handy("He said, Um, not today.", Some("en")), "He said, not today.");
}

#[test]
fn real_word_fillers_go_only_in_the_language_known_and_an_unknown_one_fails_closed() {
    assert_eq!(handy("um I think um this is good", Some("en")), "I think this is good");
    assert_eq!(handy("um gato bonito", Some("pt")), "um gato bonito");
    assert_eq!(handy("um gato bonito", Some("pt-BR")), "um gato bonito");
    assert_eq!(handy("ha sido un buen día", Some("es")), "ha sido un buen día");
    assert_eq!(handy("uh I think uhm this works", Some("xx")), "I think this works");
    assert_eq!(handy("um I think this works", None), "um I think this works");
    assert_eq!(handy("uhh bueno hmm creo que um ha llegado", None), "bueno creo que um ha llegado");
    assert_eq!(handy("Er griff ähm alles an", Some("de")), "Er griff alles an", "German 'er' is a word, never a filler");
}

#[test]
fn a_cyrillic_filler_is_found_by_its_own_letters() {
    assert_eq!(handy("хм ну да", None), "ну да");
    assert_eq!(handy("Хм, ну да", None), "Ну да");
}

#[test]
fn a_stutter_of_three_or_more_is_said_once_and_the_whitespace_collapses() {
    assert_eq!(normalize("w wh wh wh wh wh wh wh wh wh why"), "w wh why");
    assert_eq!(normalize("I I I I think so so so so"), "I think so");
    assert_eq!(normalize("No NO no NO no"), "No");
    assert_eq!(normalize("no no is fine"), "no no is fine");
    assert_eq!(normalize("  Hello    world  "), "Hello world");
}

#[test]
fn a_live_transcript_holds_back_the_word_it_may_still_be_in_the_middle_of() {
    assert_eq!(clean("Um, so I thi", LIVE), "So I ");
    assert_eq!(clean("Americ", LIVE), "");
    assert_eq!(clean("And so, my fellow Americ", LIVE), "And so, my fellow ");
}

#[test]
fn a_final_transcript_is_whole_and_normalised() {
    assert_eq!(clean("Um, so I think", Clean { final_text: true, ..LIVE }), "So I think");
    assert_eq!(clean("so so so  fine ", Clean { remove_fillers: false, language: None, final_text: true }), "so fine");
}

#[test]
fn with_fillers_kept_a_live_transcript_is_shown_whole() {
    assert_eq!(clean("Um, so I thi", Clean { remove_fillers: false, ..LIVE }), "Um, so I thi");
    assert_eq!(clean("  two  spaces\n", Clean { remove_fillers: false, ..LIVE }), "two spaces\n");
}

#[test]
fn the_panel_shows_the_settled_words_cleaned_and_the_held_back_word_with_the_guess() {
    // The design page's caption: settled words in white, the model's guess in grey; the half-word
    // held back from cleaning is shown with the guess, so no word the speaker said goes unshown.
    assert_eq!(live("Um, so I thi", "nk", true, Some("en")), ("So I".to_owned(), " think".to_owned()));
    assert_eq!(live("so the ", "set", false, None), ("so the".to_owned(), " set".to_owned()));
    assert_eq!(live("so th", "e set", false, None), ("so th".to_owned(), "e set".to_owned()));
    assert_eq!(live("", "", true, None), (String::new(), String::new()));
}

#[test]
fn a_word_with_a_filler_inside_it_is_left_alone() {
    assert_eq!(handy("huh, the hummus was fine", Some("en")), "huh, the hummus was fine");
    assert_eq!(handy("an umbrella_um", Some("en")), "an umbrella_um");
}

mod properties {
    use proptest::prelude::*;

    use super::super::*;

    /// Words a model writes, none of them a filler in any tier; and the fillers, every tier.
    const PLAIN: [&str; 16] = ["so", "I", "think", "the", "panel", "should", "list", "both", "models", "Hello", "world", "gato", "über", "ну", "ok", "42"];
    const FILLERS: [&str; 10] = ["uh", "Uhm", "UMM", "hmm", "hm", "um", "Ah", "eh", "äh", "хм"];
    const PUNCTUATION: [&str; 5] = ["", ",", ".", "?", "-"];

    fn word(fillers: bool) -> impl Strategy<Value = String> {
        let pool: Vec<&'static str> = if fillers { PLAIN.iter().chain(FILLERS.iter()).copied().collect() } else { PLAIN.to_vec() };
        (prop::sample::select(pool), prop::sample::select(PUNCTUATION.to_vec())).prop_map(|(word, mark)| format!("{word}{mark}"))
    }

    fn text(fillers: bool) -> impl Strategy<Value = String> {
        prop::collection::vec((word(fillers), prop::sample::select(vec![" ", "  ", "\n", " "])), 0..24)
            .prop_map(|words| words.into_iter().map(|(word, gap)| format!("{word}{gap}")).collect())
    }

    fn language() -> impl Strategy<Value = Option<&'static str>> {
        prop::sample::select(vec![None, Some("en"), Some("en-US"), Some("de-DE"), Some("fr"), Some("pt-BR")])
    }

    /// The words of `text`, case aside: runs of the chars a filler's edge is drawn against.
    fn words(text: &str) -> Vec<String> {
        let mut words: Vec<String> = text.split(|c: char| !is_word_char(c)).filter(|w| !w.is_empty()).map(str::to_lowercase).collect();
        words.sort();
        words
    }

    /// Whether every word of `less`, as often as it comes, is a word of `more`.
    fn within(less: &[String], more: &[String]) -> bool {
        let mut left = more.to_vec();
        less.iter().all(|word| match left.iter().position(|w| w == word) {
            Some(at) => {
                left.remove(at);
                true
            }
            None => false,
        })
    }

    proptest! {
        /// Cleaning what was cleaned changes nothing: a final transcript pasted twice reads the same
        /// (Handy's `text.rs` runs removal and normalisation once on a final; learn-harness's
        /// `cleanDictation` is called again on text it already cleaned when a stream re-commits).
        #[test]
        fn cleaning_a_final_transcript_twice_is_cleaning_it_once(raw in text(true), language in language(), remove_fillers in any::<bool>()) {
            let how = Clean { remove_fillers, language, final_text: true };
            let once = clean(&raw, how);
            prop_assert_eq!(clean(&once, how), once);
        }

        /// Cleaning only takes words away and recases a first letter; it never writes one the
        /// speaker did not say (Handy's `text.rs`: removal deletes matches, normalisation drops
        /// repeats).
        #[test]
        fn cleaning_never_adds_a_word(raw in text(true), language in language(), final_text in any::<bool>()) {
            let cleaned = clean(&raw, Clean { remove_fillers: true, language, final_text });
            prop_assert!(within(&words(&cleaned), &words(&raw)), "{:?} became {:?}", raw, cleaned);
        }

        /// Text with no filler in it comes through removal as it went in (learn-harness's
        /// `tests/dictation-local.test.ts`: "um gato bonito" in Portuguese is left alone, and Handy's
        /// `text.rs` cases remove only the listed words).
        #[test]
        fn text_with_no_filler_is_left_as_it_was(raw in text(false), language in language()) {
            prop_assert_eq!(remove_filler_words(&raw, language), raw);
        }
    }
}
