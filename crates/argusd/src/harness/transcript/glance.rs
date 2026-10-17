//! How much of a transcript a listing reads: a short one whole, a long one
//! only at its two ends — the opening says where the session ran, what it
//! was for and when it started, the end when it was last heard from and
//! any title given since. A session that ran all day is megabytes, and a
//! list is a hundred of them.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Take};
use std::path::{Path, PathBuf};

use argus_protocol::agents::transcript::Meta;

use super::{Format, Reading};

/// The longest transcript read whole, and so the longest whose turns a
/// listing counts.
const WHOLE_BYTES: u64 = 256 * 1024;
/// How much of a longer one's opening is read. Codex's first record
/// carries its whole system prompt, and Claude's first turn can follow a
/// skill's worth of injected context.
const HEAD_BYTES: u64 = 128 * 1024;
const TAIL_BYTES: u64 = 32 * 1024;

/// What a listing knows of a session from its transcript.
#[derive(Debug)]
pub(super) struct Glance {
    pub meta: Meta,
    /// Whether the whole file was read, and so `meta.turns` counts every
    /// turn.
    pub whole: bool,
    /// The id the transcript records for its session.
    pub session_id: Option<String>,
}

/// A transcript's opening, read only as far as it has been asked about.
pub(super) struct Head {
    format: Format,
    path: PathBuf,
    size: u64,
    from: Take<BufReader<File>>,
    reading: Reading,
}

impl Head {
    pub fn open(format: Format, path: &Path) -> std::io::Result<Head> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        let limit = if size <= WHOLE_BYTES { size } else { HEAD_BYTES };
        Ok(Head {
            format,
            path: path.to_path_buf(),
            size,
            from: BufReader::new(file).take(limit),
            reading: Reading::default(),
        })
    }

    /// Where the session ran, reading no further than the record that says
    /// so: for a transcript filed by date rather than by checkout, most
    /// that are opened belong to another repository.
    pub fn cwd(&mut self) -> std::io::Result<Option<&str>> {
        let format = self.format;
        self.reading.lines(format, &mut self.from, |r| r.transcript.meta.cwd.is_some())?;
        Ok(self.reading.transcript.meta.cwd.as_deref())
    }

    pub fn finish(mut self) -> std::io::Result<Glance> {
        let format = self.format;
        self.reading.lines(format, &mut self.from, |_| false)?;
        let whole = self.size <= WHOLE_BYTES;
        let mut meta = std::mem::take(&mut self.reading.transcript.meta);
        if !whole {
            let tail = tail(format, &self.path, self.size)?;
            if tail.transcript.meta.ended.is_some() {
                meta.ended = tail.transcript.meta.ended;
            }
            if tail.named {
                meta.title = tail.transcript.meta.title;
            }
        }
        Ok(Glance { meta, whole, session_id: self.reading.session_id })
    }
}

/// The last records of a transcript longer than its head.
fn tail(format: Format, path: &Path, size: u64) -> std::io::Result<Reading> {
    let mut file = File::open(path)?;
    let start = size.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut from = BufReader::new(file.take(size - start));
    if start > 0 {
        // The line the seek landed in is a fragment, and a fragment of a
        // record can be a record of its own.
        from.read_until(b'\n', &mut Vec::new())?;
    }
    let mut reading = Reading::default();
    reading.lines(format, from, |_| false)?;
    Ok(reading)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, at: &str) -> String {
        format!("{{\"kind\":\"user\",\"text\":\"{text}\",\"at\":\"{at}\"}}\n")
    }

    #[test]
    fn a_short_transcript_is_read_whole_and_its_turns_counted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        std::fs::write(&path, line("first", "09:00") + &line("second", "09:05")).unwrap();
        let glance = Head::open(Format::Neutral, &path).unwrap().finish().unwrap();
        assert!(glance.whole);
        assert_eq!(glance.meta.turns, 2);
        assert_eq!(glance.meta.title.as_deref(), Some("first"));
        assert_eq!(glance.meta.ended.as_deref(), Some("09:05"));
    }

    #[test]
    fn a_long_transcript_is_read_at_its_two_ends_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let filler = line(&"x".repeat(1000), "10:00");
        let mut text = line("what it was for", "09:00");
        while (text.len() as u64) < WHOLE_BYTES * 2 {
            text.push_str(&filler);
        }
        text.push_str(&line("last word", "17:00"));
        text.push_str("{\"kind\":\"title\",\"text\":\"Named late\"}\n");
        std::fs::write(&path, text).unwrap();

        let glance = Head::open(Format::Neutral, &path).unwrap().finish().unwrap();
        assert!(!glance.whole);
        assert_eq!(glance.meta.started.as_deref(), Some("09:00"));
        assert_eq!(glance.meta.ended.as_deref(), Some("17:00"));
        assert_eq!(glance.meta.title.as_deref(), Some("Named late"));
        // Counted only as far as the head: a listing says nothing of them.
        assert!(u64::from(glance.meta.turns) < WHOLE_BYTES / 1000);
    }

    #[test]
    fn where_a_session_ran_is_read_no_further_than_the_record_saying_so() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let mut text = String::from("{\"type\":\"session\",\"id\":\"s1\",\"cwd\":\"/repo\"}\n");
        text.push_str("{\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n");
        std::fs::write(&path, text).unwrap();

        let mut head = Head::open(Format::Pi, &path).unwrap();
        assert_eq!(head.cwd().unwrap(), Some("/repo"));
        assert_eq!(head.reading.transcript.meta.turns, 0);
        let glance = head.finish().unwrap();
        assert_eq!(glance.meta.turns, 1);
        assert_eq!(glance.session_id.as_deref(), Some("s1"));
    }
}
