//! What a dictation's raw transcript becomes before anyone reads it: filler words gone, a
//! stutter collapsed, and, while it is still being spoken, the half-word at its end held back.
//!
//! A port of Handy's filler-word removal (cjpais/Handy, `src-tauri/src/audio_toolkit/text.rs`) by
//! way of learn-harness's `src/core/dictation-text.ts`, which made its word boundaries Unicode-aware.
//! Two tiers, as Handy ships them: the universal fillers are no word in any language the models
//! write, so they always go; the gated ones are real words somewhere (Portuguese "um" is "a"), so
//! they go only when the run's language is known, and an unknown one fails closed. A capitalised
//! filler that opened a sentence hands its capital to the word that takes its place.
//!
//! Ported from Handy, Copyright (c) 2025 CJ Pais, MIT License
//! (<https://github.com/cjpais/Handy/blob/main/LICENSE>).

const UNIVERSAL: [&str; 13] = ["uh", "uhm", "umm", "uhh", "uhhh", "ehh", "ehm", "ahm", "hmm", "hm", "mmm", "хм", "ммм"];

/// The fillers that are words in some other language, by the base subtag of the one known.
fn gated(language: &str) -> &'static [&'static str] {
    match language.split(['-', '_']).next().unwrap_or_default() {
        "en" => &["um", "ah", "eh"],
        "de" => &["äh", "ähm"],
        "fr" => &["euh"],
        _ => &[],
    }
}

/// What a word is made of, for its edges: JavaScript's `[\p{L}\p{N}_]`, which the port's
/// boundaries are.
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// How to clean a transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clean<'a> {
    pub remove_fillers: bool,
    /// The language the text is known to be in, a BCP-47-ish tag; `None` when unknown.
    pub language: Option<&'a str>,
    /// The run is over: the text gets Handy's normalisation.
    pub final_text: bool,
}

/// Removes the filler words, the gated ones only when `language` is known.
pub fn remove_filler_words(text: &str, language: Option<&str>) -> String {
    let gated = language.map(gated).unwrap_or_default();
    UNIVERSAL.iter().chain(gated).fold(text.to_owned(), |out, word| remove_filler(&out, word))
}

/// Where `word` stands whole in `text` from char `at`, case aside: the chars it takes, with one
/// `,` or `.` after it (Handy's `(?i)\bword\b[,.]?`).
fn filler_at(chars: &[char], at: usize, word: &[char]) -> Option<usize> {
    if at > 0 && is_word_char(chars[at - 1]) {
        return None;
    }
    let end = at + word.len();
    if end > chars.len() || !chars[at..end].iter().zip(word).all(|(c, w)| c.to_lowercase().eq(w.to_lowercase())) {
        return None;
    }
    if chars.get(end).is_some_and(|c| is_word_char(*c)) {
        return None;
    }
    Some(if matches!(chars.get(end), Some(',' | '.')) { end + 1 - at } else { end - at })
}

/// Whether a word put after `kept` would open a sentence.
fn opens_sentence(kept: &str) -> bool {
    kept.trim_end().chars().last().is_none_or(|last| ".!?…".contains(last))
}

fn remove_filler(text: &str, word: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let word: Vec<char> = word.chars().collect();
    let mut kept = String::with_capacity(text.len());
    let mut capital_owed = false;
    let mut segment = String::new();
    let mut at = 0;
    while at < chars.len() {
        let Some(len) = filler_at(&chars, at, &word) else {
            segment.push(chars[at]);
            at += 1;
            continue;
        };
        push(&mut kept, &mut segment, &mut capital_owed);
        capital_owed |= chars[at].is_uppercase() && opens_sentence(&kept);
        at += len;
    }
    push(&mut kept, &mut segment, &mut capital_owed);
    kept
}

/// Appends `segment` to `kept`, giving its first letter or digit the capital a removed filler owes.
fn push(kept: &mut String, segment: &mut String, capital_owed: &mut bool) {
    if *capital_owed {
        if let Some((at, first)) = segment.char_indices().find(|(_, c)| c.is_alphanumeric()) {
            *capital_owed = false;
            kept.push_str(&segment[..at]);
            kept.extend(first.to_uppercase());
            kept.push_str(&segment[at + first.len_utf8()..]);
            segment.clear();
            return;
        }
    }
    kept.push_str(segment);
    segment.clear();
}

/// Handy's `normalize_transcription_output`, run on every final text whether or not fillers are
/// removed: a word of letters said three or more times in a row, case aside, is said once, then the
/// whitespace collapses and the ends are trimmed.
pub fn normalize(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out: Vec<&str> = Vec::with_capacity(words.len());
    let mut at = 0;
    while at < words.len() {
        let word = words[at];
        let mut count = 1;
        if word.chars().all(char::is_alphabetic) {
            let lower = word.to_lowercase();
            while words.get(at + count).is_some_and(|next| next.to_lowercase() == lower) {
                count += 1;
            }
        }
        out.push(word);
        at += if count >= 3 { count } else { 1 };
    }
    out.join(" ")
}

/// The text a run shows for its raw transcript so far, or the text it delivers once `final_text`.
///
/// A streaming model commits mid-word ("Americ", then "ans"), so while fillers are being removed and
/// the run is live, the token after the last space is held back until the next one arrives: a filler
/// is only known as one once it is whole. The live text never starts with whitespace, since what it
/// is pasted after owns the separator.
pub fn clean(raw: &str, how: Clean) -> String {
    let mut text = raw.to_owned();
    if how.remove_fillers {
        if !how.final_text {
            text = match text.char_indices().rev().find(|(_, c)| c.is_whitespace()) {
                Some((at, space)) => text[..at + space.len_utf8()].to_owned(),
                None => String::new(),
            };
        }
        text = remove_filler_words(&text, how.language);
    }
    if how.final_text {
        return normalize(&text);
    }
    collapse_spaces(&text).trim_start().to_owned()
}

/// The text so far as the panel shows it: what the model settled on, cleaned, and its guess at the
/// rest. The settled text's last token is held back with the guess while fillers are removed, as
/// [`clean`] holds it back: "um" may yet be the start of "umbrella".
pub fn live(committed: &str, tentative: &str, remove_fillers: bool, language: Option<&str>) -> (String, String) {
    let settled = clean(committed, Clean { remove_fillers, language, final_text: false });
    let held = match committed.char_indices().rev().find(|(_, c)| c.is_whitespace()) {
        _ if !remove_fillers => "",
        Some((at, space)) => &committed[at + space.len_utf8()..],
        None => committed,
    };
    let mut guess = format!("{held}{tentative}");
    let apart = !held.is_empty() || committed.ends_with(char::is_whitespace);
    if !settled.trim().is_empty() && apart && !guess.is_empty() && !guess.starts_with(char::is_whitespace) {
        guess.insert(0, ' ');
    }
    (settled.trim_end().to_owned(), guess)
}

/// Every run of two or more whitespace characters as one space.
fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = 0;
    for c in text.chars() {
        if c.is_whitespace() {
            run += 1;
            if run == 2 {
                out.pop();
                out.push(' ');
            } else if run == 1 {
                out.push(c);
            }
        } else {
            run = 0;
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests;
