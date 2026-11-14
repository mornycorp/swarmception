//! The one file writer (#164, swarm-core's docs/app-architecture.md › Layers and State on disk:
//! only the layer that touches the outside writes a file, through one atomic writer). Everything
//! Swarmception persists (the window's state and recent list, the issues' cache, the control
//! channel's token file, a screenshot) is written here: a temporary file beside the target,
//! written whole and flushed to the disk, then renamed over it, so a crash leaves the old file or
//! the new, never half of one. `cargo xtask writes` fails on a write anywhere else.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Writes `bytes` to `path`, whole or not at all; its directory is made when it isn't there.
///
/// # Errors
///
/// The directory, the temporary file or the rename's I/O error; no temporary file is left.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write(path, bytes, false)
}

/// As [`write_atomic`], the file readable and writable by its owner only where the system has
/// permissions (the control channel's token file).
///
/// # Errors
///
/// As [`write_atomic`].
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write(path, bytes, true)
}

/// The text of the file at `path`, read whole when it is at most `most` bytes: none when there is
/// no file. A file read as hostile input (the window's place, #178): one too large or not UTF-8 is
/// an error, never read in part.
///
/// # Errors
///
/// The file is over `most` bytes, isn't UTF-8, or couldn't be read.
pub fn read_capped_text(path: &Path, most: u64) -> io::Result<Option<String>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut bytes = Vec::new();
    io::Read::read_to_end(&mut io::Read::take(file, most.saturating_add(1)), &mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > most {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("it is over {most} bytes")));
    }
    String::from_utf8(bytes).map(Some).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "it is not UTF-8 text"))
}

fn write(path: &Path, bytes: &[u8], private: bool) -> io::Result<()> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(parent) = parent {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = beside(path);
    let written = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        if private {
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        }
        #[cfg(not(unix))]
        let _ = private;
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

/// A temporary file's path beside `path`, its own to this process and this write.
fn beside(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    /// A file read as hostile input: none when it isn't there, whole when it fits, an error when
    /// it is over its cap or isn't text.
    #[test]
    fn a_capped_read_is_whole_or_refused() {
        let dir = std::env::temp_dir().join(format!("swarmception-files-capped-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("w.json");
        assert_eq!(read_capped_text(&file, 10).unwrap(), None, "no file");
        write_atomic(&file, b"0123456789").unwrap();
        assert_eq!(read_capped_text(&file, 10).unwrap().as_deref(), Some("0123456789"));
        assert!(read_capped_text(&file, 9).is_err(), "over its cap");
        write_atomic(&file, &[0xff, 0xfe]).unwrap();
        assert!(read_capped_text(&file, 10).is_err(), "not text");
        std::fs::remove_dir_all(dir).ok();
    }

    use super::*;

    fn scratch(what: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swarmception-files-{what}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_file_is_written_whole_its_directory_made_and_no_temporary_left() {
        let dir = scratch("whole");
        let file = dir.join("deeper/state.json");
        write_atomic(&file, b"one\n").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"one\n");
        write_atomic(&file, b"two, longer\n").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two, longer\n", "written over whole");
        assert_eq!(std::fs::read_dir(dir.join("deeper")).unwrap().count(), 1, "no temporary file left");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_write_that_fails_leaves_the_old_file_and_no_temporary() {
        let dir = scratch("fails");
        std::fs::create_dir_all(dir.join("taken")).unwrap();
        // The target is a directory: the rename fails, the temporary goes.
        assert!(write_atomic(&dir.join("taken"), b"x").is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        // Its directory can't be made: a file is in the way.
        std::fs::write(dir.join("file"), b"a file").unwrap();
        assert!(write_atomic(&dir.join("file/below.json"), b"x").is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn a_private_file_is_its_owners_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("private");
        let file = dir.join("control.json");
        write_atomic(&file, b"open\n").unwrap();
        write_private(&file, b"{\"token\": \"t\"}\n").unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600, "written over: private");
        assert_eq!(std::fs::read(&file).unwrap(), b"{\"token\": \"t\"}\n");
        std::fs::remove_dir_all(dir).ok();
    }
}
