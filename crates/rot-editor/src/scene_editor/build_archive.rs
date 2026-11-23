//! Stream a verified staging tree into a Linux distribution archive.
use flate2::{write::GzEncoder, Compression};
use rustix::fs::{self as at, Mode, OFlags};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const CHUNK_BYTES: usize = 64 * 1024;

pub(super) fn game_name(archive: &OsStr) -> Option<&str> {
    let name = archive.to_str()?.strip_suffix(".tar.gz")?;
    valid_member_name(name.as_ref()).then_some(name)
}

fn valid_member_name(name: &OsStr) -> bool {
    name.to_str().is_some_and(|name| {
        !matches!(name, "" | "." | "..")
            && !name
                .chars()
                .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
    })
}

pub(super) fn write(
    destination: &mut File,
    stage: &Path,
    top: &str,
    cancel: &AtomicBool,
) -> io::Result<u64> {
    if !valid_member_name(top.as_ref()) {
        return Err(io::Error::other("Invalid archive folder name."));
    }
    let directory = File::from(at::open(
        stage,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    let mut archive = tar::Builder::new(GzEncoder::new(destination, Compression::fast()));
    // Staging is private. The extracted top-level folder is ordinary.
    append_header(
        &mut archive,
        Path::new(top),
        tar::EntryType::Directory,
        0o755,
        0,
        io::empty(),
    )?;
    append_children(&mut archive, &directory, Path::new(top), cancel)?;
    check_cancel(cancel)?;
    let file = archive.into_inner()?.finish()?;
    file.sync_all()?;
    Ok(file.metadata()?.len())
}

fn append_children(
    archive: &mut tar::Builder<GzEncoder<&mut File>>,
    directory: &File,
    member: &Path,
    cancel: &AtomicBool,
) -> io::Result<()> {
    check_cancel(cancel)?;
    if member.components().count() > 8 {
        return Err(io::Error::other(
            "Build staging has unexpected directory depth.",
        ));
    }
    let mut names = fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    names.sort_unstable();
    for name in names {
        check_cancel(cancel)?;
        if !valid_member_name(&name) {
            return Err(io::Error::other("Invalid file name in build staging."));
        }
        // Reject non-ordinary entries before opening, then read only the exact
        // opened file relative to the held staging directory.
        let stat = at::statat(directory, &name, at::AtFlags::SYMLINK_NOFOLLOW)?;
        let kind = at::FileType::from_raw_mode(stat.st_mode);
        if !matches!(kind, at::FileType::RegularFile | at::FileType::Directory) {
            return Err(io::Error::other(
                "An archive may contain only files and directories.",
            ));
        }
        let file = File::from(at::openat(
            directory,
            &name,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let meta = file.metadata()?;
        if meta.ino() != stat.st_ino || meta.dev() != stat.st_dev {
            return Err(io::Error::other("A staged archive entry was replaced."));
        }
        let mode = meta.permissions().mode() & 0o777;
        let path: PathBuf = member.join(&name);
        if meta.is_dir() && kind == at::FileType::Directory {
            append_header(
                archive,
                &path,
                tar::EntryType::Directory,
                mode,
                0,
                io::empty(),
            )?;
            append_children(archive, &file, &path, cancel)?;
        } else if meta.is_file() && kind == at::FileType::RegularFile {
            append_header(
                archive,
                &path,
                tar::EntryType::Regular,
                mode,
                meta.len(),
                Chunks {
                    file: &file,
                    remaining: meta.len(),
                    cancel,
                },
            )?;
        } else {
            return Err(io::Error::other(
                "A staged archive entry changed while opening it.",
            ));
        }
    }
    Ok(())
}

fn append_header(
    archive: &mut tar::Builder<GzEncoder<&mut File>>,
    path: &Path,
    kind: tar::EntryType,
    mode: u32,
    size: u64,
    body: impl Read,
) -> io::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(kind);
    header.set_mode(mode);
    header.set_size(size);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    archive.append_data(&mut header, path, body)
}

struct Chunks<'a> {
    file: &'a File,
    remaining: u64,
    cancel: &'a AtomicBool,
}

impl Read for Chunks<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        check_cancel(self.cancel)?;
        let length = self.remaining.min(buffer.len().min(CHUNK_BYTES) as u64) as usize;
        if length == 0 {
            return Ok(0);
        }
        let count = self.file.read(&mut buffer[..length])?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "A staged file was truncated.",
            ));
        }
        self.remaining -= count as u64;
        Ok(count)
    }
}

fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::other("Archive compression cancelled."))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_reads_stop_on_the_next_chunk_after_cancel() {
        use std::io::{Seek, Write};
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&[42; CHUNK_BYTES * 2]).unwrap();
        file.rewind().unwrap();
        let cancel = AtomicBool::new(false);
        let mut reader = Chunks {
            file: &file,
            remaining: (CHUNK_BYTES * 2) as u64,
            cancel: &cancel,
        };
        let mut buffer = vec![0; CHUNK_BYTES * 2];
        assert_eq!(reader.read(&mut buffer).unwrap(), CHUNK_BYTES);
        cancel.store(true, Ordering::Relaxed);
        assert!(reader.read(&mut buffer).is_err());
        assert_eq!(reader.remaining, CHUNK_BYTES as u64);
    }
}
