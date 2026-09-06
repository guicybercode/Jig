use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use cli_master_core::wire::{FilePath, FileReadRequest, FileTarget, FileWriteRequest};
use cli_master_core::{ApiError, ProjectId};
use tempfile::TempDir;

use super::write::{WriteFaults, save};
use super::{FileTargetAccess, LocalFileService, ResolvedFileTarget};

struct Access {
    root: PathBuf,
}

impl FileTargetAccess for Access {
    fn resolve_target(
        &self,
        _target: &FileTarget,
        _write: bool,
    ) -> Result<ResolvedFileTarget, ApiError> {
        Ok(ResolvedFileTarget {
            root: self.root.clone(),
        })
    }

    fn with_write_lease<T>(
        &self,
        _target: &FileTarget,
        expected_root: &Path,
        operation: impl FnOnce() -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        if self.root != expected_root {
            return Err(super::error::target_changed());
        }
        operation()
    }
}

struct Fixture {
    _directory: TempDir,
    access: Access,
    target: FileTarget,
    service: LocalFileService,
}

impl Fixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let root = directory.path().join("project");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        fs::write(root.join("file.txt"), "original\r\n").unwrap();
        Self {
            _directory: directory,
            access: Access { root },
            target: FileTarget::Project {
                project_id: ProjectId::new(),
            },
            service: LocalFileService::default(),
        }
    }

    fn write_request(&self, path: &[u8], text: &str) -> FileWriteRequest {
        let path = FilePath::try_from_bytes(path).unwrap();
        let read = self
            .service
            .read(
                &FileReadRequest::try_new(self.target, path.clone()).unwrap(),
                &self.access,
            )
            .unwrap();
        FileWriteRequest::try_new(self.target, path, text, read.revision).unwrap()
    }

    fn staged_files(&self) -> usize {
        fs::read_dir(&self.access.root)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".cli-master-save-")
            })
            .count()
    }
}

#[test]
fn observed_external_write_before_rename_preserves_external_bytes_and_cleans_temp() {
    let fixture = Fixture::new();
    let request = fixture.write_request(b"file.txt", "our draft");
    let changed_path = fixture.access.root.join("file.txt");
    let error = save(
        &fixture.service,
        &request,
        &fixture.access,
        &WriteFaults {
            after_staging: Some(Box::new(move || {
                fs::write(&changed_path, "external edit").unwrap();
            })),
            ..WriteFaults::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "file_conflict");
    assert_eq!(
        fs::read(fixture.access.root.join("file.txt")).unwrap(),
        b"external edit"
    );
    assert_eq!(fixture.staged_files(), 0);
}

#[test]
fn applied_write_reports_durability_uncertain_without_a_second_implicit_write() {
    let fixture = Fixture::new();
    let request = fixture.write_request(b"file.txt", "applied once\r\n");
    let error = save(
        &fixture.service,
        &request,
        &fixture.access,
        &WriteFaults {
            fail_after_rename: true,
            ..WriteFaults::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "file_durability_uncertain");
    let encoded = serde_json::to_value(&error).unwrap();
    assert_eq!(encoded["details"]["writeApplied"], true);
    assert!(
        encoded["details"]["currentRevision"]
            .as_str()
            .unwrap()
            .starts_with("v1:")
    );
    assert_eq!(
        fs::read(fixture.access.root.join("file.txt")).unwrap(),
        b"applied once\r\n"
    );
    assert_eq!(fixture.staged_files(), 0);
}

#[test]
fn relocated_parent_aborts_before_publication_and_cleans_only_its_temp() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.access.root.join("folder")).unwrap();
    fs::write(fixture.access.root.join("folder/file.txt"), "original").unwrap();
    let request = fixture.write_request(b"folder/file.txt", "draft");
    let root = fixture.access.root.clone();
    let error = save(
        &fixture.service,
        &request,
        &fixture.access,
        &WriteFaults {
            after_staging: Some(Box::new(move || {
                fs::rename(root.join("folder"), root.join("relocated")).unwrap();
                fs::create_dir(root.join("folder")).unwrap();
                fs::write(root.join("folder/file.txt"), "replacement root").unwrap();
            })),
            ..WriteFaults::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "file_target_changed");
    assert_eq!(
        fs::read(fixture.access.root.join("relocated/file.txt")).unwrap(),
        b"original"
    );
    assert_eq!(
        fs::read(fixture.access.root.join("folder/file.txt")).unwrap(),
        b"replacement root"
    );
    assert_eq!(
        fs::read_dir(fixture.access.root.join("relocated"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn extended_attributes_are_rejected_without_losing_the_original_inode() {
    let fixture = Fixture::new();
    let path = fixture.access.root.join("file.txt");
    let file = fs::File::open(&path).unwrap();
    #[cfg(target_os = "linux")]
    let attribute = "user.cli_master_test";
    #[cfg(target_os = "macos")]
    let attribute = "com.cli-master.test";
    rustix::fs::fsetxattr(
        &file,
        attribute,
        b"retain attribute",
        rustix::fs::XattrFlags::empty(),
    )
    .unwrap();
    let request = fixture.write_request(b"file.txt", "draft");
    let before = rustix::fs::fstat(&file).unwrap();
    let error = fixture
        .service
        .write(&request, &fixture.access)
        .unwrap_err();
    assert_eq!(error.code, "file_metadata_unsupported");
    let after = rustix::fs::fstat(fs::File::open(path).unwrap()).unwrap();
    assert_eq!(before.st_ino, after.st_ino);
    assert_eq!(fixture.staged_files(), 0);
}

#[test]
fn mode_and_line_endings_are_preserved_and_revision_survives_new_service() {
    let fixture = Fixture::new();
    let path = fixture.access.root.join("file.txt");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let request = fixture.write_request(b"file.txt", "\u{feff}Olá\r\n");
    let written = fixture.service.write(&request, &fixture.access).unwrap();
    let fresh = LocalFileService::default();
    let read = fresh
        .read(
            &FileReadRequest::try_new(fixture.target, request.path_base64).unwrap(),
            &fixture.access,
        )
        .unwrap();
    assert_eq!(read.revision, written.revision);
    assert_eq!(read.text, request.text);
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_provenance_and_complete_attribute_list_survive_replacement() {
    fn snapshot(file: &fs::File) -> (Vec<u8>, Option<Vec<u8>>) {
        let mut names = vec![0_u8; 1_024];
        let length = rustix::fs::flistxattr(file, &mut names[..]).unwrap();
        names.truncate(length);
        let provenance = if names
            .split(|byte| *byte == 0)
            .any(|name| name == b"com.apple.provenance")
        {
            let mut value = vec![0_u8; 4_096];
            let length =
                rustix::fs::fgetxattr(file, "com.apple.provenance", &mut value[..]).unwrap();
            value.truncate(length);
            Some(value)
        } else {
            None
        };
        (names, provenance)
    }

    let fixture = Fixture::new();
    let path = fixture.access.root.join("file.txt");
    let original = fs::File::open(&path).unwrap();
    let before = snapshot(&original);
    let request = fixture.write_request(b"file.txt", "preserve provenance\r\n");
    fixture.service.write(&request, &fixture.access).unwrap();
    let replacement = fs::File::open(path).unwrap();
    assert_ne!(
        rustix::fs::fstat(&original).unwrap().st_ino,
        rustix::fs::fstat(&replacement).unwrap().st_ino
    );
    assert_eq!(snapshot(&replacement), before);
}

#[test]
fn symlink_leaf_is_rejected_before_reading_outside() {
    let fixture = Fixture::new();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("secret.txt"), "outside").unwrap();
    let request = fixture.write_request(b"file.txt", "draft");
    fs::remove_file(fixture.access.root.join("file.txt")).unwrap();
    symlink(
        outside.path().join("secret.txt"),
        fixture.access.root.join("file.txt"),
    )
    .unwrap();
    let error = fixture
        .service
        .write(&request, &fixture.access)
        .unwrap_err();
    assert_eq!(error.code, "file_symlink_not_allowed");
    assert_eq!(
        fs::read(outside.path().join("secret.txt")).unwrap(),
        b"outside"
    );
}

#[test]
fn previously_observed_root_replaced_by_another_directory_is_rejected() {
    let fixture = Fixture::new();
    fixture.write_request(b"file.txt", "draft");
    let old = fixture.access.root.with_file_name("original-project");
    fs::rename(&fixture.access.root, &old).unwrap();
    fs::create_dir(&fixture.access.root).unwrap();
    fs::write(
        fixture.access.root.join("file.txt"),
        "replacement directory",
    )
    .unwrap();
    let request = FileReadRequest::try_new(
        fixture.target,
        FilePath::try_from_bytes(b"file.txt").unwrap(),
    )
    .unwrap();
    let error = fixture.service.read(&request, &fixture.access).unwrap_err();
    assert_eq!(error.code, "file_target_changed");
    assert_eq!(fs::read(old.join("file.txt")).unwrap(), b"original\r\n");
}
