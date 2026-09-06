#![cfg(target_os = "macos")]

use std::fs::{self, File};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use cli_master_file_metadata::has_extended_acl;
use tempfile::TempDir;

fn chmod(path: &Path, arguments: &[&str]) {
    let output = Command::new("/bin/chmod")
        .args(arguments)
        .arg(path)
        .output()
        .expect("run chmod for real temporary-file ACL fixture");
    assert!(
        output.status.success(),
        "chmod fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn clean_directory() -> TempDir {
    let directory = tempfile::tempdir().expect("temporary directory");
    chmod(directory.path(), &["-N"]);
    directory
}

#[test]
fn ordinary_file_without_acl_remains_editable() {
    let directory = clean_directory();
    let path = directory.path().join("ordinary.txt");
    fs::write(&path, "ordinary text\n").unwrap();
    let file = File::open(path).unwrap();

    assert!(!has_extended_acl(&file).unwrap());
    assert!(file.metadata().is_ok(), "inspection must not close the fd");
}

#[test]
fn explicit_acl_is_observed_without_modifying_content_or_mode() {
    let directory = clean_directory();
    let path = directory.path().join("explicit.txt");
    fs::write(&path, "keep this content\n").unwrap();
    chmod(&path, &["+a", "everyone allow read"]);
    let file = File::open(&path).unwrap();
    let permissions = file.metadata().unwrap().permissions().mode();

    assert!(has_extended_acl(&file).unwrap());
    assert_eq!(fs::read(&path).unwrap(), b"keep this content\n");
    assert_eq!(file.metadata().unwrap().permissions().mode(), permissions);
    chmod(&path, &["-N"]);
    assert!(!has_extended_acl(&file).unwrap());
}

#[test]
fn inherited_acl_is_detected_on_new_files() {
    let directory = clean_directory();
    chmod(
        directory.path(),
        &["+a", "everyone allow read,file_inherit,directory_inherit"],
    );
    let path = directory.path().join("inherited.txt");
    fs::write(&path, "inherited policy\n").unwrap();
    let file = File::open(&path).unwrap();

    assert!(has_extended_acl(&file).unwrap());
    chmod(&path, &["-N"]);
    assert!(!has_extended_acl(&file).unwrap());
}

#[test]
fn inspection_follows_the_open_descriptor_after_path_replacement() {
    let directory = clean_directory();
    let path = directory.path().join("document.txt");
    fs::write(&path, "original\n").unwrap();
    chmod(&path, &["+a", "everyone allow read"]);
    let original = File::open(&path).unwrap();

    fs::rename(&path, directory.path().join("moved.txt")).unwrap();
    fs::write(&path, "replacement\n").unwrap();
    let replacement = File::open(&path).unwrap();

    assert!(has_extended_acl(&original).unwrap());
    assert!(!has_extended_acl(&replacement).unwrap());
}

#[test]
fn denied_acl_inspection_is_an_error_instead_of_missing_acl() {
    let directory = clean_directory();
    let path = directory.path().join("denied.txt");
    fs::write(&path, "protected security metadata\n").unwrap();
    let file = File::open(&path).unwrap();
    chmod(&path, &["+a", "everyone deny readsecurity"]);

    let result = has_extended_acl(&file);
    chmod(&path, &["-N"]);
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
}
