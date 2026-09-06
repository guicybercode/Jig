//! Descriptor-relative, read-only access to approved discovery locations.

use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use cli_master_core::knowledge::MAX_KNOWLEDGE_BODY_BYTES;
use cli_master_core::knowledge::discovery::KnowledgeSourceAvailability;
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags, Stat};
use rustix::io::Errno;

const MAX_PATH_BYTES: usize = 4_096;
const MAX_SYMLINK_HOPS: usize = 16;
const MAX_DIRECTORY_STEPS: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SafeError {
    Missing,
    Unreadable,
    Symlink,
    NonRegular,
    TooLarge,
    InvalidText,
    Changed,
    Limit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DirectoryIdentity {
    device: u64,
    inode: u64,
}

pub(super) struct Directory {
    pub(super) file: File,
    pub(super) path: PathBuf,
    pub(super) identity: DirectoryIdentity,
}

/// Native precision is retained: millisecond timestamps miss rapid edits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FileFingerprint {
    device: i128,
    inode: i128,
    size: i128,
    modified_seconds: i128,
    modified_nanos: i128,
    changed_seconds: i128,
    changed_nanos: i128,
}

impl FileFingerprint {
    fn from_stat(stat: &Stat) -> Self {
        // rustix's stat integer widths/signedness differ on Linux and macOS.
        Self {
            device: i128::from(stat.st_dev),
            inode: i128::from(stat.st_ino),
            size: i128::from(stat.st_size),
            modified_seconds: i128::from(stat.st_mtime),
            modified_nanos: i128::from(stat.st_mtime_nsec),
            changed_seconds: i128::from(stat.st_ctime),
            changed_nanos: i128::from(stat.st_ctime_nsec),
        }
    }
}

pub(super) struct Candidate {
    pub(super) availability: KnowledgeSourceAvailability,
    pub(super) fingerprint: Option<FileFingerprint>,
}

impl Candidate {
    fn unavailable(availability: KnowledgeSourceAvailability) -> Self {
        Self {
            availability,
            fingerprint: None,
        }
    }
}

impl Directory {
    pub(super) fn open_absolute(
        path: &Path,
        follow_directory_symlinks: bool,
    ) -> Result<(Self, bool), SafeError> {
        if !path.is_absolute() {
            return Err(SafeError::Unreadable);
        }
        check_path(path)?;
        walk_directories(
            root_directory()?,
            components(path),
            follow_directory_symlinks,
        )
    }

    pub(super) fn open_child(
        &self,
        name: &OsStr,
        allow_symlink: bool,
    ) -> Result<(Self, bool), SafeError> {
        check_name(name)?;
        let fd =
            fs::openat(&self.file, ".", directory_flags(), Mode::empty()).map_err(map_errno)?;
        let directory = Self::from_file(File::from(fd), self.path.clone())?;
        if directory.identity != self.identity {
            return Err(SafeError::Changed);
        }
        walk_directories(directory, VecDeque::from([name.to_owned()]), allow_symlink)
    }

    /// The caller marks the scan truncated when its shared budget reaches zero.
    pub(super) fn entry_names(&self, remaining: &mut usize) -> Result<Vec<OsString>, SafeError> {
        let mut names = Vec::new();
        if *remaining == 0 {
            return Ok(names);
        }
        let entries = fs::Dir::read_from(&self.file).map_err(map_errno)?;
        for entry in entries {
            let entry = entry.map_err(map_errno)?;
            let bytes = entry.file_name().to_bytes();
            if bytes == b"." || bytes == b".." {
                continue;
            }
            *remaining -= 1;
            names.push(OsString::from_vec(bytes.to_vec()));
            if *remaining == 0 {
                break;
            }
        }
        names.sort_unstable();
        Ok(names)
    }

    pub(super) fn candidate(&self, name: &OsStr) -> Result<Candidate, SafeError> {
        check_name(name)?;
        let before = match self.leaf_stat(name) {
            Ok(stat) => stat,
            Err(SafeError::Unreadable) => {
                return Ok(Candidate::unavailable(
                    KnowledgeSourceAvailability::Unreadable,
                ));
            }
            Err(error) => return Err(error),
        };
        if let Err(error) = regular_bounded(&before) {
            let availability = match error {
                SafeError::Symlink => KnowledgeSourceAvailability::Symlink,
                SafeError::NonRegular => KnowledgeSourceAvailability::NonRegular,
                SafeError::TooLarge => KnowledgeSourceAvailability::TooLarge,
                _ => KnowledgeSourceAvailability::Unreadable,
            };
            return Ok(Candidate::unavailable(availability));
        }
        let fingerprint = FileFingerprint::from_stat(&before);
        let file = match self.open_leaf(name) {
            Ok(file) => file,
            Err(SafeError::Unreadable) => {
                return Ok(Candidate::unavailable(
                    KnowledgeSourceAvailability::Unreadable,
                ));
            }
            Err(error) => return Err(error),
        };
        verify_file(&file, &fingerprint)?;
        self.verify_leaf(name, &fingerprint)?;
        Ok(Candidate {
            availability: KnowledgeSourceAvailability::Available,
            fingerprint: Some(fingerprint),
        })
    }

    pub(super) fn read_candidate(
        &self,
        name: &OsStr,
        fingerprint: &FileFingerprint,
    ) -> Result<String, SafeError> {
        check_name(name)?;
        self.verify_leaf(name, fingerprint)?;
        let file = self.open_leaf(name)?;
        verify_file(&file, fingerprint)?;
        let limit = u64::try_from(MAX_KNOWLEDGE_BODY_BYTES).map_err(|_| SafeError::Limit)?;
        let mut bytes = Vec::new();
        (&file)
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| map_io_error(&error))?;
        if bytes.len() > MAX_KNOWLEDGE_BODY_BYTES {
            return Err(SafeError::TooLarge);
        }
        verify_file(&file, fingerprint)?;
        self.verify_leaf(name, fingerprint)?;
        if bytes.contains(&0) {
            return Err(SafeError::InvalidText);
        }
        String::from_utf8(bytes).map_err(|_| SafeError::InvalidText)
    }

    fn from_file(file: File, path: PathBuf) -> Result<Self, SafeError> {
        let metadata = file.metadata().map_err(|error| map_io_error(&error))?;
        if !metadata.is_dir() {
            return Err(SafeError::NonRegular);
        }
        Ok(Self {
            identity: DirectoryIdentity {
                device: metadata.dev(),
                inode: metadata.ino(),
            },
            file,
            path,
        })
    }

    fn leaf_stat(&self, name: &OsStr) -> Result<Stat, SafeError> {
        fs::statat(&self.file, name, AtFlags::SYMLINK_NOFOLLOW).map_err(map_errno)
    }

    fn verify_leaf(&self, name: &OsStr, expected: &FileFingerprint) -> Result<(), SafeError> {
        let stat = self.leaf_stat(name)?;
        regular_bounded(&stat)?;
        if FileFingerprint::from_stat(&stat) != *expected {
            return Err(SafeError::Changed);
        }
        Ok(())
    }

    fn open_leaf(&self, name: &OsStr) -> Result<File, SafeError> {
        // NONBLOCK prevents a raced FIFO from waiting for a writer; NOCTTY
        // prevents a raced terminal device from becoming our controlling tty.
        fs::openat(
            &self.file,
            name,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::NOCTTY,
            Mode::empty(),
        )
        .map(File::from)
        .map_err(map_errno)
    }
}

fn directory_flags() -> OFlags {
    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

fn root_directory() -> Result<Directory, SafeError> {
    let fd = fs::open("/", directory_flags(), Mode::empty()).map_err(map_errno)?;
    Directory::from_file(File::from(fd), PathBuf::from("/"))
}

fn components(path: &Path) -> VecDeque<OsString> {
    path.components()
        .filter(|part| *part != Component::CurDir)
        .map(|part| part.as_os_str().to_owned())
        .collect()
}

fn walk_directories(
    mut directory: Directory,
    mut pending: VecDeque<OsString>,
    allow_symlinks: bool,
) -> Result<(Directory, bool), SafeError> {
    let mut hops = 0;
    let mut steps = 0;
    while let Some(name) = pending.pop_front() {
        steps += 1;
        if steps > MAX_DIRECTORY_STEPS {
            return Err(SafeError::Limit);
        }
        if name == OsStr::new("/") {
            directory = root_directory()?;
            continue;
        }
        let before = directory.leaf_stat(&name)?;
        if FileType::from_raw_mode(before.st_mode) == FileType::Symlink {
            if !allow_symlinks {
                return Err(SafeError::Symlink);
            }
            hops += 1;
            if hops > MAX_SYMLINK_HOPS {
                return Err(SafeError::Limit);
            }
            let target = capture_link(&directory.file, &name, &before)?;
            let mut expanded = components(&target);
            expanded.append(&mut pending);
            if expanded
                .iter()
                .map(|part| part.as_bytes().len() + 1)
                .sum::<usize>()
                > MAX_PATH_BYTES
            {
                return Err(SafeError::Limit);
            }
            pending = expanded;
            continue;
        }
        if FileType::from_raw_mode(before.st_mode) != FileType::Directory {
            return Err(SafeError::NonRegular);
        }
        let fd = fs::openat(&directory.file, &name, directory_flags(), Mode::empty())
            .map_err(map_errno)?;
        let opened = fs::fstat(&fd).map_err(map_errno)?;
        if before.st_dev != opened.st_dev || before.st_ino != opened.st_ino {
            return Err(SafeError::Changed);
        }
        let mut path = directory.path.clone();
        if name == OsStr::new("..") {
            path.pop();
        } else {
            path.push(name);
        }
        check_path(&path)?;
        directory = Directory::from_file(File::from(fd), path)?;
    }
    Ok((directory, hops > 0))
}

fn capture_link(parent: &File, name: &OsStr, before: &Stat) -> Result<PathBuf, SafeError> {
    let mut buffer = [0_u8; MAX_PATH_BYTES];
    let count = fs::readlinkat_raw(parent, name, &mut buffer[..]).map_err(map_errno)?;
    // readlinkat_raw silently truncates and never appends a NUL terminator.
    if count == 0 || count == buffer.len() {
        return Err(SafeError::Limit);
    }
    let after = fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW).map_err(map_errno)?;
    if FileType::from_raw_mode(after.st_mode) != FileType::Symlink
        || FileFingerprint::from_stat(before) != FileFingerprint::from_stat(&after)
    {
        return Err(SafeError::Changed);
    }
    let target = PathBuf::from(OsString::from_vec(buffer[..count].to_vec()));
    check_path(&target)?;
    Ok(target)
}

fn regular_bounded(stat: &Stat) -> Result<(), SafeError> {
    match FileType::from_raw_mode(stat.st_mode) {
        FileType::Symlink => return Err(SafeError::Symlink),
        FileType::RegularFile => {}
        _ => return Err(SafeError::NonRegular),
    }
    let size = usize::try_from(stat.st_size).map_err(|_| SafeError::TooLarge)?;
    if size > MAX_KNOWLEDGE_BODY_BYTES {
        return Err(SafeError::TooLarge);
    }
    Ok(())
}

fn verify_file(file: &File, expected: &FileFingerprint) -> Result<(), SafeError> {
    let stat = fs::fstat(file).map_err(map_errno)?;
    regular_bounded(&stat)?;
    if FileFingerprint::from_stat(&stat) != *expected {
        return Err(SafeError::Changed);
    }
    Ok(())
}

fn check_name(name: &OsStr) -> Result<(), SafeError> {
    let mut parts = Path::new(name).components();
    if !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
        || name.as_bytes().contains(&b'/')
        || name.as_bytes().contains(&0)
    {
        return Err(SafeError::Unreadable);
    }
    check_path(Path::new(name))
}

fn check_path(path: &Path) -> Result<(), SafeError> {
    let bytes = path.as_os_str().as_bytes();
    if bytes.len() > MAX_PATH_BYTES {
        return Err(SafeError::Limit);
    }
    if bytes.is_empty() || bytes.contains(&0) {
        return Err(SafeError::Unreadable);
    }
    Ok(())
}

fn map_errno(error: Errno) -> SafeError {
    match error {
        Errno::NOENT => SafeError::Missing,
        Errno::LOOP => SafeError::Symlink,
        Errno::NOTDIR => SafeError::NonRegular,
        Errno::NAMETOOLONG | Errno::MFILE | Errno::NFILE => SafeError::Limit,
        _ => SafeError::Unreadable,
    }
}

fn map_io_error(error: &io::Error) -> SafeError {
    match error.kind() {
        io::ErrorKind::NotFound => SafeError::Missing,
        _ => SafeError::Unreadable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    fn directory(path: &Path) -> Directory {
        Directory::open_absolute(path, true)
            .expect("open test directory")
            .0
    }

    fn fingerprint(directory: &Directory, name: &str) -> FileFingerprint {
        directory
            .candidate(OsStr::new(name))
            .expect("candidate")
            .fingerprint
            .expect("readable file")
    }

    #[test]
    fn regular_document_roundtrip_and_unchanged_reads() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("SKILL.md"), "# Skill\nOlá").expect("write");
        let directory = directory(temp.path());
        let fingerprint = fingerprint(&directory, "SKILL.md");
        for _ in 0..2 {
            assert_eq!(
                directory.read_candidate(OsStr::new("SKILL.md"), &fingerprint),
                Ok("# Skill\nOlá".into())
            );
        }
    }

    #[test]
    fn symlinked_directories_resolve_relative_absolute_and_parent_components() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("targets/skill")).expect("target");
        fs::create_dir(temp.path().join("links")).expect("links");
        symlink("../targets/skill", temp.path().join("links/relative")).expect("relative");
        symlink(
            temp.path().join("links/relative"),
            temp.path().join("absolute"),
        )
        .expect("absolute");
        let root = directory(temp.path());
        let (resolved, linked) = root
            .open_child(OsStr::new("absolute"), true)
            .expect("resolve");
        assert!(linked);
        assert_eq!(
            resolved.path,
            fs::canonicalize(temp.path().join("targets/skill")).expect("canonical")
        );
        assert!(matches!(
            root.open_child(OsStr::new("absolute"), false),
            Err(SafeError::Symlink)
        ));
        assert!(Directory::open_absolute(&resolved.path, false).is_ok());
    }

    #[test]
    fn leaves_never_follow_links_or_open_fifos() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("secret"), "private").expect("secret");
        symlink("secret", temp.path().join("AGENTS.md")).expect("link");
        // rustix's mkfifoat/mknodat are unavailable on macOS.
        assert!(
            std::process::Command::new("mkfifo")
                .arg(temp.path().join("SKILL.md"))
                .status()
                .expect("mkfifo")
                .success()
        );
        let directory = directory(temp.path());
        assert_eq!(
            directory
                .candidate(OsStr::new("AGENTS.md"))
                .expect("link candidate")
                .availability,
            KnowledgeSourceAvailability::Symlink
        );
        assert_eq!(
            directory
                .candidate(OsStr::new("SKILL.md"))
                .expect("fifo candidate")
                .availability,
            KnowledgeSourceAvailability::NonRegular
        );
        let fifo = directory
            .open_leaf(OsStr::new("SKILL.md"))
            .expect("nonblocking fifo open");
        let stamp = fingerprint(&directory, "secret");
        assert_eq!(verify_file(&fifo, &stamp), Err(SafeError::NonRegular));
    }

    #[test]
    fn mutations_and_replacements_conflict() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("AGENTS.md");
        fs::write(&path, "original").expect("original");
        let directory = directory(temp.path());
        let first = fingerprint(&directory, "AGENTS.md");
        fs::write(&path, "modified").expect("modify");
        assert_eq!(
            directory.read_candidate(OsStr::new("AGENTS.md"), &first),
            Err(SafeError::Changed)
        );
        let second = fingerprint(&directory, "AGENTS.md");
        fs::rename(&path, temp.path().join("previous")).expect("rename");
        fs::write(&path, "modified").expect("replace");
        assert_eq!(
            directory.read_candidate(OsStr::new("AGENTS.md"), &second),
            Err(SafeError::Changed)
        );
    }

    #[test]
    fn restored_mtime_does_not_hide_a_same_size_edit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("AGENTS.md");
        fs::write(&path, "original").expect("original");
        let original_time = fs::metadata(&path)
            .expect("metadata")
            .modified()
            .expect("mtime");
        let directory = directory(temp.path());
        let original = fingerprint(&directory, "AGENTS.md");
        fs::write(&path, "modified").expect("same size edit");
        File::options()
            .write(true)
            .open(&path)
            .expect("open timestamp target")
            .set_times(fs::FileTimes::new().set_modified(original_time))
            .expect("restore mtime");
        let edited = fingerprint(&directory, "AGENTS.md");
        assert_eq!(original.size, edited.size);
        assert_eq!(original.modified_seconds, edited.modified_seconds);
        assert_eq!(original.modified_nanos, edited.modified_nanos);
        assert_eq!(
            directory.read_candidate(OsStr::new("AGENTS.md"), &original),
            Err(SafeError::Changed)
        );
    }

    #[test]
    fn encoding_and_byte_limits_are_checked() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("SKILL.md");
        let directory = directory(temp.path());
        for bytes in [vec![0xff], b"hello\0there".to_vec()] {
            fs::write(&path, bytes).expect("write invalid");
            let stamp = fingerprint(&directory, "SKILL.md");
            assert_eq!(
                directory.read_candidate(OsStr::new("SKILL.md"), &stamp),
                Err(SafeError::InvalidText)
            );
        }
        fs::write(&path, vec![b'a'; MAX_KNOWLEDGE_BODY_BYTES]).expect("write limit");
        let stamp = fingerprint(&directory, "SKILL.md");
        assert_eq!(
            directory
                .read_candidate(OsStr::new("SKILL.md"), &stamp)
                .expect("bounded read")
                .len(),
            MAX_KNOWLEDGE_BODY_BYTES
        );
        fs::write(&path, vec![b'a'; MAX_KNOWLEDGE_BODY_BYTES + 1]).expect("write oversized");
        assert_eq!(
            directory
                .candidate(OsStr::new("SKILL.md"))
                .expect("large candidate")
                .availability,
            KnowledgeSourceAvailability::TooLarge
        );
    }

    #[test]
    fn loops_and_client_path_components_are_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        symlink("second", temp.path().join("first")).expect("first link");
        symlink("first", temp.path().join("second")).expect("second link");
        let directory = directory(temp.path());
        assert!(matches!(
            directory.open_child(OsStr::new("first"), true),
            Err(SafeError::Limit)
        ));
        for name in ["../secret", "/etc", ".", "..", "name/", "name\0suffix"] {
            assert!(matches!(
                directory.open_child(OsStr::new(name), false),
                Err(SafeError::Unreadable)
            ));
            assert!(matches!(
                directory.candidate(OsStr::new(name)),
                Err(SafeError::Unreadable)
            ));
        }
    }

    #[test]
    fn entry_iteration_is_sorted_and_bounded() {
        let temp = tempfile::tempdir().expect("tempdir");
        for name in ["z", "b", "a"] {
            fs::write(temp.path().join(name), "").expect("entry");
        }
        let directory = directory(temp.path());
        let mut remaining = 10;
        assert_eq!(
            directory.entry_names(&mut remaining).expect("names"),
            [
                OsString::from("a"),
                OsString::from("b"),
                OsString::from("z")
            ]
        );
        assert_eq!(remaining, 7);
        remaining = 2;
        assert_eq!(
            directory
                .entry_names(&mut remaining)
                .expect("bounded names")
                .len(),
            2
        );
        assert_eq!(remaining, 0);
        assert!(
            directory
                .entry_names(&mut remaining)
                .expect("exhausted")
                .is_empty()
        );
    }

    #[test]
    fn pinned_directory_survives_replacement_without_following_new_link() {
        let temp = tempfile::tempdir().expect("tempdir");
        let skill = temp.path().join("skill");
        fs::create_dir(&skill).expect("skill");
        fs::write(skill.join("SKILL.md"), "approved").expect("approved");
        let pinned = directory(&skill);
        let stamp = fingerprint(&pinned, "SKILL.md");
        fs::rename(&skill, temp.path().join("moved")).expect("move");
        fs::create_dir(temp.path().join("other")).expect("other");
        fs::write(temp.path().join("other/SKILL.md"), "unrelated").expect("unrelated");
        symlink("other", &skill).expect("replacement link");
        assert_eq!(
            pinned.read_candidate(OsStr::new("SKILL.md"), &stamp),
            Ok("approved".into())
        );
        assert!(matches!(
            Directory::open_absolute(&pinned.path, false),
            Err(SafeError::Symlink)
        ));
    }
}
