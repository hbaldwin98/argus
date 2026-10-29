//! A download resumes, is checked and stops as learn-harness's model manager does
//! (`electron/dictation-models.cjs`: resume with a Range, size and sha256 checks, cancel keeps the
//! part, a mismatch keeps nothing).

use std::cell::RefCell;
use std::io::Cursor;

use super::*;

const DATA: &[u8] = b"a model's bytes, as small as a test wants them";

fn entry(data: &[u8]) -> Entry {
    let sha: String = Sha256::digest(data).iter().map(|byte| format!("{byte:02x}")).collect();
    Entry { id: "tiny", repo: "a/b", file: "tiny.gguf", size: data.len() as u64, sha256: Box::leak(sha.into_boxed_str()) }
}

/// Serves `data`, honouring a range unless told not to, and remembers where it was asked from.
struct Served {
    data: Vec<u8>,
    ranges: bool,
    asked: RefCell<Vec<u64>>,
}

impl Served {
    fn new(data: &[u8], ranges: bool) -> Self {
        Served { data: data.to_vec(), ranges, asked: RefCell::new(Vec::new()) }
    }
}

impl Source for Served {
    fn open(&self, _: &str, from: u64) -> Result<Opened, String> {
        self.asked.borrow_mut().push(from);
        if self.ranges && from as usize >= self.data.len() {
            return Ok(Opened::Whole);
        }
        let start = if self.ranges { from as usize } else { 0 };
        Ok(Opened::Body { body: Box::new(Cursor::new(self.data[start..].to_vec())), resumed: self.ranges && from > 0 })
    }
}

fn run(entry: &Entry, dir: &Path, source: &dyn Source) -> Result<Fetched, String> {
    fetch(entry, dir, source, &AtomicBool::new(false), &mut |_| {})
}

#[test]
fn a_whole_download_is_checked_and_takes_its_own_name() {
    let dir = tempfile::tempdir().unwrap();
    let entry = entry(DATA);
    let mut seen = Vec::new();
    let fetched = fetch(&entry, dir.path(), &Served::new(DATA, true), &AtomicBool::new(false), &mut |done| seen.push(done));
    assert_eq!(fetched, Ok(Fetched::Done));
    assert_eq!(std::fs::read(store::path(dir.path(), &entry)).unwrap(), DATA);
    assert!(!store::partial(dir.path(), &entry).exists());
    assert_eq!(seen.last(), Some(&(DATA.len() as u64)));
}

#[test]
fn a_stopped_download_resumes_from_where_it_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let entry = entry(DATA);
    std::fs::write(store::partial(dir.path(), &entry), &DATA[..10]).unwrap();
    let served = Served::new(DATA, true);
    assert_eq!(run(&entry, dir.path(), &served), Ok(Fetched::Done));
    assert_eq!(*served.asked.borrow(), vec![10]);
    assert_eq!(std::fs::read(store::path(dir.path(), &entry)).unwrap(), DATA);
}

#[test]
fn a_server_that_ignores_the_range_is_downloaded_again_from_the_start() {
    let dir = tempfile::tempdir().unwrap();
    let entry = entry(DATA);
    std::fs::write(store::partial(dir.path(), &entry), &DATA[..10]).unwrap();
    assert_eq!(run(&entry, dir.path(), &Served::new(DATA, false)), Ok(Fetched::Done));
    assert_eq!(std::fs::read(store::path(dir.path(), &entry)).unwrap(), DATA);
}

#[test]
fn a_part_that_was_already_whole_is_checked_and_kept() {
    let dir = tempfile::tempdir().unwrap();
    let entry = entry(DATA);
    std::fs::write(store::partial(dir.path(), &entry), DATA).unwrap();
    assert_eq!(run(&entry, dir.path(), &Served::new(DATA, true)), Ok(Fetched::Done));
    assert_eq!(store::on_disk(dir.path(), &entry), store::OnDisk::Ready);
}

#[test]
fn a_file_that_does_not_match_its_hash_is_deleted_whole() {
    let dir = tempfile::tempdir().unwrap();
    let entry = entry(DATA);
    let mut other = DATA.to_vec();
    other[0] ^= 1;
    let refused = run(&entry, dir.path(), &Served::new(&other, true)).unwrap_err();
    assert!(refused.contains("sha256"), "{refused}");
    assert_eq!(store::on_disk(dir.path(), &entry), store::OnDisk::Missing, "nothing suspect is resumed from");
}

#[test]
fn a_stop_keeps_the_part_to_resume_from() {
    let dir = tempfile::tempdir().unwrap();
    let entry = entry(DATA);
    std::fs::write(store::partial(dir.path(), &entry), &DATA[..5]).unwrap();
    let stop = AtomicBool::new(true);
    assert_eq!(fetch(&entry, dir.path(), &Served::new(DATA, true), &stop, &mut |_| {}), Ok(Fetched::Stopped));
    assert_eq!(store::on_disk(dir.path(), &entry), store::OnDisk::Partial(5));
}

#[test]
fn a_server_that_sends_more_than_the_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let entry = entry(DATA);
    let mut longer = DATA.to_vec();
    longer.extend_from_slice(b"and more");
    assert!(run(&entry, dir.path(), &Served::new(&longer, true)).is_err());
}

#[test]
fn a_file_already_there_is_not_downloaded_again() {
    let dir = tempfile::tempdir().unwrap();
    let entry = entry(DATA);
    std::fs::write(store::path(dir.path(), &entry), DATA).unwrap();
    let served = Served::new(DATA, true);
    assert_eq!(run(&entry, dir.path(), &served), Ok(Fetched::Done));
    assert!(served.asked.borrow().is_empty());
}
