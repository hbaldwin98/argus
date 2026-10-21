//! The executables in a release archive, read out of it and nothing else.
//!
//! Only an entry at `PACKAGE/NAME`, for one of Argus's own executable
//! names, is taken, and it is written under that name, never under the
//! path the archive gives: an archive cannot place a file anywhere, or
//! replace one Argus does not ship.

use std::io::{Cursor, Read};

/// What a release archive holds that an update replaces.
pub const EXECUTABLES: [&str; 4] = ["argus", "argusd", "argus-hook", "argus-desktop"];

/// One executable as the archive held it: its file name on this platform,
/// and its bytes.
#[derive(Debug, PartialEq, Eq)]
pub struct Executable {
    pub file: String,
    pub bytes: Vec<u8>,
}

/// Every one of [`EXECUTABLES`] from the archive, which is a zip when
/// `zip` and a gzipped tar otherwise, all inside the folder `package`.
pub fn executables(archive: &[u8], zip: bool, package: &str, suffix: &str) -> anyhow::Result<Vec<Executable>> {
    let wanted: Vec<String> = EXECUTABLES.iter().map(|name| format!("{name}{suffix}")).collect();
    let mut found: Vec<Executable> = Vec::new();
    let mut take = |path: &str, read: &mut dyn Read| -> anyhow::Result<()> {
        let Some(file) = executable_at(path, package, &wanted) else {
            return Ok(());
        };
        if found.iter().any(|seen| seen.file == file) {
            anyhow::bail!("the archive holds {file} twice");
        }
        let mut bytes = Vec::new();
        read.read_to_end(&mut bytes)?;
        found.push(Executable { file: file.to_string(), bytes });
        Ok(())
    };
    if zip {
        let mut archive = zip::ZipArchive::new(Cursor::new(archive))?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            if entry.is_file() {
                let path = entry.name()?.into_owned();
                take(&path, &mut entry)?;
            }
        }
    } else {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(archive));
        for entry in archive.entries()? {
            let mut entry = entry?;
            if entry.header().entry_type().is_file() {
                let path = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
                take(&path, &mut entry)?;
            }
        }
    }
    let missing: Vec<&str> = wanted
        .iter()
        .map(String::as_str)
        .filter(|name| !found.iter().any(|seen| seen.file == *name))
        .collect();
    if !missing.is_empty() {
        anyhow::bail!("the archive is missing {}", missing.join(", "));
    }
    Ok(found)
}

/// The executable `path` is, if it is one of `wanted` directly inside
/// `package`. Either slash separates: Windows PowerShell's
/// `Compress-Archive` wrote backslashes.
fn executable_at<'a>(path: &str, package: &str, wanted: &'a [String]) -> Option<&'a str> {
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|part| !part.is_empty() && *part != ".").collect();
    match parts.as_slice() {
        [folder, file] if *folder == package => wanted.iter().find(|name| name == file).map(String::as_str),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const PACKAGE: &str = "argus-0.38.1-x86_64-unknown-linux-gnu";

    fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        for (path, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, *bytes).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (path, bytes) in entries {
            writer.start_file(*path, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn release(package: &str, suffix: &str) -> Vec<(String, Vec<u8>)> {
        let mut entries: Vec<(String, Vec<u8>)> =
            EXECUTABLES.iter().map(|name| (format!("{package}/{name}{suffix}"), name.as_bytes().to_vec())).collect();
        entries.push((format!("{package}/README.md"), b"readme".to_vec()));
        entries
    }

    fn borrowed(entries: &[(String, Vec<u8>)]) -> Vec<(&str, &[u8])> {
        entries.iter().map(|(path, bytes)| (path.as_str(), bytes.as_slice())).collect()
    }

    #[test]
    fn a_tar_release_yields_its_four_executables_and_nothing_else() {
        let archive = tar_gz(&borrowed(&release(PACKAGE, "")));
        let found = executables(&archive, false, PACKAGE, "").unwrap();
        let mut files: Vec<&str> = found.iter().map(|e| e.file.as_str()).collect();
        files.sort();
        assert_eq!(files, ["argus", "argus-desktop", "argus-hook", "argusd"]);
        assert!(found.iter().all(|e| e.bytes == e.file.as_bytes()));
    }

    #[test]
    fn a_zip_release_is_read_whichever_slash_its_paths_use() {
        let package = "argus-0.38.1-x86_64-pc-windows-msvc";
        let forward = zip_of(&borrowed(&release(package, ".exe")));
        assert_eq!(executables(&forward, true, package, ".exe").unwrap().len(), 4);
        let back: Vec<(String, Vec<u8>)> =
            release(package, ".exe").into_iter().map(|(path, bytes)| (path.replace('/', "\\"), bytes)).collect();
        let backslashed = zip_of(&borrowed(&back));
        assert_eq!(executables(&backslashed, true, package, ".exe").unwrap().len(), 4);
    }

    #[test]
    fn an_entry_outside_the_package_folder_is_never_taken() {
        let mut entries = release(PACKAGE, "");
        entries.retain(|(path, _)| !path.ends_with("/argusd"));
        entries.push(("../argusd".into(), b"escaped".to_vec()));
        entries.push((format!("{PACKAGE}/bin/argusd"), b"nested".to_vec()));
        entries.push(("other/argusd".into(), b"elsewhere".to_vec()));
        // A zip, since the tar builder itself refuses a `..`.
        let refused = executables(&zip_of(&borrowed(&entries)), true, PACKAGE, "").unwrap_err();
        assert!(refused.to_string().contains("missing argusd"), "{refused}");
    }

    #[test]
    fn an_archive_missing_an_executable_or_holding_one_twice_is_refused() {
        let mut entries = release(PACKAGE, "");
        entries.pop();
        entries.pop();
        assert!(executables(&tar_gz(&borrowed(&entries)), false, PACKAGE, "").is_err());

        let mut twice = release(PACKAGE, "");
        twice.push((format!("{PACKAGE}/./argus"), b"again".to_vec()));
        let refused = executables(&tar_gz(&borrowed(&twice)), false, PACKAGE, "").unwrap_err();
        assert!(refused.to_string().contains("twice"), "{refused}");
    }

    #[test]
    fn what_is_not_an_archive_is_refused() {
        assert!(executables(b"not an archive", false, PACKAGE, "").is_err());
        assert!(executables(b"not an archive", true, PACKAGE, "").is_err());
    }
}
