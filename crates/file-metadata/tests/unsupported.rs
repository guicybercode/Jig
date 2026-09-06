#![cfg(not(target_os = "macos"))]

use std::io;

use cli_master_file_metadata::has_extended_acl;

#[test]
fn unavailable_inspection_is_an_error_instead_of_missing_acl() {
    let file = tempfile::tempfile().unwrap();
    assert_eq!(
        has_extended_acl(&file).unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
}
