//! File IPC boundary tests: exact Unix identifiers, bounds and optimistic saves.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use cli_master_core::wire::{
    DEFAULT_FILE_LIST_LIMIT, FileEntry, FileEntryKind, FileListRequest, FileListResponse, FileName,
    FilePath, FileReadRequest, FileReadResponse, FileRevision, FileTarget, FileWriteRequest,
    FileWriteResponse, MAX_FILE_LIST_LIMIT, MAX_FILE_PATH_BYTES, MAX_FILE_TEXT_BYTES,
};
use cli_master_core::{ProjectId, SessionId, WorktreeId};
use serde_json::{Value, json};

fn target() -> FileTarget {
    FileTarget::Project {
        project_id: ProjectId::new(),
    }
}

fn revision() -> FileRevision {
    FileRevision::try_new(format!("v1:{}", "0123456789abcdef".repeat(4))).unwrap()
}

fn path() -> FilePath {
    FilePath::try_from_bytes(b"src/editor.rs").unwrap()
}

fn write_payload(text: &str) -> Value {
    json!({
        "target": target(),
        "pathBase64": path(),
        "text": text,
        "expectedRevision": revision(),
    })
}

#[test]
fn paths_round_trip_exact_unix_bytes_without_display_normalization() {
    let paths: &[&[u8]] = &[
        b"src/ordinary.txt",
        b"space in name.txt",
        "açúcar/日本語.md".as_bytes(),
        b"binary-name-\xff\xfe.txt",
        b"-option-looking",
        b"--",
        b"literal\\backslash:colon",
        b"C:/still-a-relative-unix-name",
        b"line\nbreak\tname",
    ];
    for bytes in paths {
        let path = FilePath::try_from_bytes(bytes).unwrap();
        let value = serde_json::to_value(&path).unwrap();
        assert_eq!(value, json!(STANDARD.encode(bytes)));
        assert_eq!(FilePath::try_new(path.as_str()).unwrap().as_bytes(), *bytes);
        assert_eq!(serde_json::from_value::<FilePath>(value).unwrap(), path);
    }
}

#[test]
fn path_boundary_rejects_traversal_and_nul_without_echoing_path_contents() {
    let invalid: &[&[u8]] = &[
        b"/absolute",
        b"trailing/",
        b"empty//component",
        b".",
        b"..",
        b"./child",
        b"parent/../child",
        b"parent/./child",
        b"private-value\0suffix",
    ];
    for bytes in invalid {
        assert!(FilePath::try_from_bytes(bytes).is_err());
        let error = FilePath::try_new(STANDARD.encode(bytes)).unwrap_err();
        assert!(!error.to_string().contains("private-value"));
        assert!(serde_json::from_value::<FilePath>(json!(STANDARD.encode(bytes))).is_err());
    }
}

#[test]
fn path_limit_counts_decoded_bytes_and_canonical_padding_is_required() {
    let maximum = vec![b'x'; MAX_FILE_PATH_BYTES];
    assert_eq!(
        FilePath::try_from_bytes(&maximum).unwrap().as_bytes(),
        maximum
    );
    assert!(FilePath::try_new(STANDARD.encode(&maximum)).is_ok());
    let oversized = vec![b'x'; MAX_FILE_PATH_BYTES + 1];
    assert!(FilePath::try_from_bytes(&oversized).is_err());
    assert!(FilePath::try_new(STANDARD.encode(&oversized)).is_err());

    for invalid in ["YQ", "YQ=", "YR==", "YWJ=", "YQ==\n", "_w==", "YQ===="] {
        assert!(FilePath::try_new(invalid).is_err(), "accepted {invalid:?}");
    }
    assert_eq!(FilePath::try_new("/w==").unwrap().as_bytes(), &[0xff]);
}

#[test]
fn root_is_listable_but_never_a_read_or_write_target() {
    let root = FilePath::root();
    assert!(root.is_root());
    assert_eq!(serde_json::to_value(&root).unwrap(), json!(""));
    assert_eq!(FilePath::try_new("").unwrap(), root);
    assert!(FileListRequest::try_new(target(), root.clone(), None, None).is_ok());
    assert!(FileReadRequest::try_new(target(), root.clone()).is_err());
    assert!(FileWriteRequest::try_new(target(), root, "", revision()).is_err());

    let payload = json!({ "target": target(), "pathBase64": "" });
    assert!(serde_json::from_value::<FileListRequest>(payload.clone()).is_ok());
    assert!(serde_json::from_value::<FileReadRequest>(payload).is_err());
    let mut payload = write_payload("");
    payload["pathBase64"] = json!("");
    assert!(serde_json::from_value::<FileWriteRequest>(payload).is_err());
}

#[test]
fn directory_cursors_are_exact_single_names() {
    let cursor = FileName::try_from_bytes(b"-next-\xff.txt").unwrap();
    assert_eq!(cursor.as_bytes(), b"-next-\xff.txt");
    assert_eq!(FileName::try_new(cursor.as_str()).unwrap(), cursor);
    assert_eq!(
        serde_json::from_value::<FileName>(json!(cursor)).unwrap(),
        cursor
    );
    for invalid in [&b""[..], b".", b"..", b"dir/leaf", b"a\0b"] {
        assert!(FileName::try_from_bytes(invalid).is_err());
        assert!(FileName::try_new(STANDARD.encode(invalid)).is_err());
    }
}

#[test]
fn list_defaults_and_limits_are_enforced_at_the_wire_boundary() {
    let payload = json!({ "target": target(), "pathBase64": "" });
    let request: FileListRequest = serde_json::from_value(payload.clone()).unwrap();
    assert_eq!(request.limit, DEFAULT_FILE_LIST_LIMIT);
    assert!(request.after_name_base64.is_none());

    for limit in [1, MAX_FILE_LIST_LIMIT] {
        let mut value = payload.clone();
        value["limit"] = json!(limit);
        assert_eq!(
            serde_json::from_value::<FileListRequest>(value)
                .unwrap()
                .limit,
            limit
        );
    }
    for invalid in [json!(0), json!(201), json!(-1), json!(1.5), json!(65536)] {
        let mut value = payload.clone();
        value["limit"] = invalid;
        assert!(serde_json::from_value::<FileListRequest>(value).is_err());
    }
    assert!(FileListRequest::try_new(target(), FilePath::root(), Some(0), None).is_err());
    assert!(FileListRequest::try_new(target(), FilePath::root(), Some(201), None).is_err());
    let mut value = payload;
    value["afterNameBase64"] = json!(STANDARD.encode(b"dir/leaf"));
    assert!(serde_json::from_value::<FileListRequest>(value).is_err());
}

#[test]
fn targets_are_registered_ids_and_reject_arbitrary_root_overrides() {
    for target in [
        target(),
        FileTarget::Session {
            session_id: SessionId::new(),
        },
        FileTarget::Worktree {
            worktree_id: WorktreeId::new(),
        },
    ] {
        let request = FileReadRequest::try_new(target, path()).unwrap();
        let payload = serde_json::to_value(&request).unwrap();
        assert_eq!(
            serde_json::from_value::<FileReadRequest>(payload.clone()).unwrap(),
            request
        );
        assert!(payload.get("pathBase64").unwrap().is_string());
        assert!(payload.get("cwd").is_none());
        let mut overridden = payload;
        overridden["target"]["path"] = json!("/tmp/unregistered");
        assert!(serde_json::from_value::<FileReadRequest>(overridden).is_err());
    }
    for invalid in [
        json!({ "kind": "path", "path": "/tmp/unregistered" }),
        json!({ "kind": "project", "projectId": "codex" }),
        json!({ "kind": "worktree", "worktreeId": "invalid-uuid" }),
    ] {
        assert!(serde_json::from_value::<FileTarget>(invalid).is_err());
    }
}

#[test]
fn request_payloads_reject_unknown_fields_and_missing_revision() {
    let mut read = json!({ "target": target(), "pathBase64": path() });
    read["root"] = json!("/tmp");
    assert!(serde_json::from_value::<FileReadRequest>(read).is_err());
    let mut list = json!({ "target": target(), "pathBase64": "" });
    list["recursive"] = json!(true);
    assert!(serde_json::from_value::<FileListRequest>(list).is_err());
    let mut write = write_payload("content");
    write["force"] = json!(true);
    assert!(serde_json::from_value::<FileWriteRequest>(write).is_err());
    let mut write = write_payload("content");
    write.as_object_mut().unwrap().remove("expectedRevision");
    assert!(serde_json::from_value::<FileWriteRequest>(write).is_err());
}

#[test]
fn revision_requires_the_versioned_lowercase_digest_shape() {
    let valid = revision();
    assert_eq!(
        serde_json::from_value::<FileRevision>(json!(valid.as_str())).unwrap(),
        valid
    );
    for invalid in [
        String::new(),
        "v1:".to_owned(),
        format!("v1:{}", "a".repeat(63)),
        format!("v1:{}", "a".repeat(65)),
        format!("v2:{}", "a".repeat(64)),
        format!("v1:{}", "A".repeat(64)),
        format!("v1:{}", "g".repeat(64)),
        format!("v1:{}", "é".repeat(32)),
    ] {
        assert!(FileRevision::try_new(&invalid).is_err());
        assert!(serde_json::from_value::<FileRevision>(json!(invalid)).is_err());
    }
}

#[test]
fn writes_count_utf8_bytes_and_preserve_line_endings_bom_and_empty_text() {
    for content in [
        String::new(),
        "\u{feff}first\r\nsecond\r\n".to_owned(),
        "é".repeat(MAX_FILE_TEXT_BYTES / 2),
    ] {
        let request = FileWriteRequest::try_new(target(), path(), &content, revision()).unwrap();
        assert_eq!(request.text, content);
        assert_eq!(
            serde_json::from_value::<FileWriteRequest>(write_payload(&content))
                .unwrap()
                .text,
            content
        );
    }
    let oversized = format!("{}x", "é".repeat(MAX_FILE_TEXT_BYTES / 2));
    for invalid in [oversized, "before\0after".to_owned()] {
        assert!(FileWriteRequest::try_new(target(), path(), &invalid, revision()).is_err());
        assert!(serde_json::from_value::<FileWriteRequest>(write_payload(&invalid)).is_err());
    }
}

#[test]
fn maximum_valid_text_fits_the_existing_ipc_frame_even_with_json_escaping() {
    let request = FileWriteRequest::try_new(
        target(),
        FilePath::try_from_bytes(vec![b'x'; MAX_FILE_PATH_BYTES]).unwrap(),
        "\u{1}".repeat(MAX_FILE_TEXT_BYTES),
        revision(),
    )
    .unwrap();
    let envelope = cli_master_core::RequestEnvelope::v1("file.write", request);
    let encoded = serde_json::to_vec(&envelope).unwrap();
    assert!(encoded.len() < 1024 * 1024);
}

#[test]
fn response_contracts_preserve_byte_identity_and_epoch_ms_fields() {
    let entry = FileEntry {
        path_base64: FilePath::try_from_bytes(b"raw-\xff.txt").unwrap(),
        display_name: "raw-\\xFF.txt".to_owned(),
        kind: FileEntryKind::File,
        size_bytes: Some(12),
        modified_at_ms: None,
    };
    let list = FileListResponse {
        entries: vec![entry],
        next_after_name_base64: Some(FileName::try_from_bytes(b"raw-\xff.txt").unwrap()),
        observed_at_ms: 1_788_566_400_123,
    };
    let list_json = serde_json::to_value(&list).unwrap();
    assert_eq!(list_json["observedAtMs"], json!(1_788_566_400_123_i64));
    assert!(list_json["entries"][0].get("modifiedAtMs").is_none());
    assert_eq!(
        list_json["entries"][0]["pathBase64"],
        json!(STANDARD.encode(b"raw-\xff.txt"))
    );
    assert_eq!(
        serde_json::from_value::<FileListResponse>(list_json).unwrap(),
        list
    );

    let read = FileReadResponse {
        path_base64: path(),
        text: "é\r\n".to_owned(),
        revision: revision(),
        size_bytes: 4,
        modified_at_ms: Some(1_788_566_400_100),
        observed_at_ms: 1_788_566_400_123,
    };
    let value = serde_json::to_value(&read).unwrap();
    assert_eq!(value["text"], json!("é\r\n"));
    assert_eq!(
        serde_json::from_value::<FileReadResponse>(value).unwrap(),
        read
    );

    let write = FileWriteResponse {
        path_base64: path(),
        revision: revision(),
        size_bytes: 4,
        modified_at_ms: Some(1_788_566_400_200),
        written_at_ms: 1_788_566_400_201,
    };
    let value = serde_json::to_value(&write).unwrap();
    assert_eq!(value["writtenAtMs"], json!(1_788_566_400_201_i64));
    assert_eq!(
        serde_json::from_value::<FileWriteResponse>(value).unwrap(),
        write
    );
}

#[test]
fn unsupported_entry_kinds_stay_noneditable_and_text_is_redacted_from_debug() {
    assert_eq!(
        serde_json::from_value::<FileEntryKind>(json!("future_device")).unwrap(),
        FileEntryKind::Other
    );
    let text = "sensitive document contents";
    let write = FileWriteRequest::try_new(target(), path(), text, revision()).unwrap();
    assert!(!format!("{write:?}").contains(text));
    let read = FileReadResponse {
        path_base64: path(),
        text: text.to_owned(),
        revision: revision(),
        size_bytes: u64::try_from(text.len()).unwrap(),
        modified_at_ms: None,
        observed_at_ms: 1_788_566_400_123,
    };
    assert!(!format!("{read:?}").contains(text));
}
