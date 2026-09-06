use std::{fs, os::unix::fs::symlink, path::Path};

use cli_master_core::knowledge::discovery::KnowledgeSourceAvailability;
use tempfile::TempDir;

use super::*;

struct Fixture {
    root: TempDir,
    project: Project,
    service: DiscoveryService,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let home = root.path().join("home");
        let project_path = root.path().join("project");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&project_path).unwrap();
        let project = Project {
            id: ProjectId::new(),
            name: "Discovery fixture".to_owned(),
            path: project_path,
            repository_root: None,
            current_branch: None,
            created_at_ms: 1,
            last_opened_at_ms: 1,
        };
        let service = DiscoveryService::new(DiscoveryRoots {
            home: Some(home),
            codex_home: None,
            admin_skills: root.path().join("admin-skills"),
        });
        Self {
            root,
            project,
            service,
        }
    }

    fn home(&self) -> &Path {
        self.service.roots.home.as_deref().unwrap()
    }
}

fn write(path: impl AsRef<Path>, content: &[u8]) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn select(
    scan: &KnowledgeDiscoverResponse,
    suffix: &str,
    provider: KnowledgeProvider,
) -> KnowledgeReadRequest {
    let entry = scan
        .entries
        .iter()
        .find(|entry| entry.source_path.ends_with(suffix) && entry.provider == provider)
        .unwrap();
    KnowledgeReadRequest {
        scan_id: scan.scan_id,
        entry_id: entry.entry_id,
    }
}

#[test]
fn discovers_exact_provider_locations_without_configs_or_imports() {
    let mut fixture = Fixture::new();
    for path in [
        "AGENTS.md",
        "AGENTS.override.md",
        "CLAUDE.md",
        "CLAUDE.local.md",
        ".claude/CLAUDE.md",
        ".claude/rules/api/requests.md",
        ".cursor/rules/rust.mdc",
    ] {
        write(
            fixture.project.path.join(path),
            b"Local instructions\n@private.txt",
        );
    }
    for path in [
        ".cursor/rules/ignored.md",
        "private.txt",
        ".env",
        ".claude/settings.json",
        ".codex/auth.json",
        "nested/AGENTS.md",
    ] {
        write(fixture.project.path.join(path), b"PRIVATE_CONFIG_SENTINEL");
    }
    write(fixture.home().join(".codex/AGENTS.md"), b"Global Codex");
    write(fixture.home().join(".claude/CLAUDE.md"), b"Global Claude");
    write(
        fixture.home().join(".claude/rules/nested/personal.md"),
        b"Personal rule",
    );
    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    assert_eq!(scan.entries.len(), 11);
    assert!(!scan.truncated);
    assert!(
        scan.issues
            .iter()
            .any(|issue| issue.code == "project_root_scope")
    );
    assert_eq!(
        fixture.service.scan_project_id(scan.scan_id).unwrap(),
        Some(fixture.project.id)
    );
    for entry in &scan.entries {
        assert!(!entry.precedence_hint.is_empty());
        assert_eq!(entry.availability, KnowledgeSourceAvailability::Available);
        if entry.scope == KnowledgeSourceScope::Project {
            assert_eq!(entry.scope_directory, ".");
        }
        let read = fixture
            .service
            .read(&KnowledgeReadRequest {
                scan_id: scan.scan_id,
                entry_id: entry.entry_id,
            })
            .unwrap();
        assert!(!read.content.contains("PRIVATE_CONFIG_SENTINEL"));
        assert!(!format!("{read:?}").contains("Local instructions"));
    }
}

#[test]
fn codex_home_override_changes_global_rule_root_without_changing_shared_skills() {
    let mut fixture = Fixture::new();
    let override_home = fixture.root.path().join("custom-codex");
    write(override_home.join("AGENTS.override.md"), b"Override root");
    write(fixture.home().join(".codex/AGENTS.md"), b"Ignored default");
    write(
        fixture.home().join(".agents/skills/shared/SKILL.md"),
        b"Shared skill",
    );
    fixture.service.roots.codex_home = Some(override_home.clone());
    let scan = fixture.service.discover(None).unwrap();
    assert!(scan.entries.iter().any(
        |entry| entry.source_path == override_home.join("AGENTS.override.md").to_str().unwrap()
    ));
    assert!(
        !scan
            .entries
            .iter()
            .any(|entry| entry.source_path.ends_with(".codex/AGENTS.md"))
    );
    assert_eq!(
        scan.entries
            .iter()
            .filter(|entry| entry.name == "shared")
            .count(),
        2
    );
    assert_eq!(fixture.service.scan_project_id(scan.scan_id).unwrap(), None);
}

#[test]
fn duplicate_names_and_recursive_cursor_skills_keep_their_origins() {
    let mut fixture = Fixture::new();
    for base in [fixture.home(), fixture.project.path.as_path()] {
        write(base.join(".claude/skills/shared/SKILL.md"), b"Claude skill");
        write(base.join(".agents/skills/shared/SKILL.md"), b"Common skill");
        write(
            base.join(".cursor/skills/group/nested/SKILL.md"),
            b"Nested Cursor skill",
        );
    }
    write(
        fixture.service.roots.admin_skills.join("managed/SKILL.md"),
        b"Admin skill",
    );
    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    assert_eq!(
        scan.entries
            .iter()
            .filter(|entry| entry.name == "shared")
            .count(),
        6
    );
    assert_eq!(
        scan.entries
            .iter()
            .filter(|entry| entry.name == "nested")
            .count(),
        2
    );
    assert!(
        scan.entries
            .iter()
            .any(|entry| entry.scope == KnowledgeSourceScope::Admin)
    );
    let personal = scan
        .entries
        .iter()
        .filter(|entry| entry.provider == KnowledgeProvider::Claude)
        .collect::<Vec<_>>();
    assert_eq!(personal.len(), 2);
    assert!(personal.iter().all(|entry| {
        entry
            .precedence_hint
            .contains("personal skills take precedence")
    }));
}

#[test]
fn skill_symlink_targets_are_captured_and_leaf_symlinks_are_unavailable() {
    let mut fixture = Fixture::new();
    let first = fixture.root.path().join("external-first");
    let second = fixture.root.path().join("external-second");
    write(first.join("SKILL.md"), b"Captured skill content");
    write(second.join("SKILL.md"), b"Retargeted skill content");
    let skill_dir = fixture.project.path.join(".claude/skills");
    fs::create_dir_all(&skill_dir).unwrap();
    let alias = skill_dir.join("linked");
    symlink(&first, &alias).unwrap();
    let secret = fixture.root.path().join("auth.json");
    write(&secret, b"PRIVATE_CREDENTIAL_SENTINEL");
    symlink(&secret, fixture.project.path.join("AGENTS.md")).unwrap();
    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    let request = select(&scan, "linked/SKILL.md", KnowledgeProvider::Claude);
    let selected = scan
        .entries
        .iter()
        .find(|entry| entry.entry_id == request.entry_id)
        .unwrap();
    assert!(selected.via_symlink);
    assert!(
        scan.issues
            .iter()
            .any(|issue| issue.code == "captured_skill_target")
    );
    fs::remove_file(&alias).unwrap();
    symlink(&second, &alias).unwrap();
    assert_eq!(
        fixture.service.read(&request).unwrap().content,
        "Captured skill content"
    );
    let leaf = select(&scan, "AGENTS.md", KnowledgeProvider::Codex);
    assert_eq!(
        fixture.service.read(&leaf).unwrap_err().code,
        "knowledge_source_symlink"
    );
    let fresh = fixture.service.discover(Some(&fixture.project)).unwrap();
    assert_eq!(
        fixture
            .service
            .read(&select(
                &fresh,
                "linked/SKILL.md",
                KnowledgeProvider::Claude
            ))
            .unwrap()
            .content,
        "Retargeted skill content"
    );
}

#[test]
fn replaced_files_and_parent_directories_require_rediscovery() {
    let mut fixture = Fixture::new();
    let rule = fixture.project.path.join(".claude/rules/local.md");
    write(&rule, b"Original text");
    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    let request = select(&scan, "local.md", KnowledgeProvider::Claude);
    write(&rule, b"Changed text!");
    assert_eq!(
        fixture.service.read(&request).unwrap_err().code,
        "knowledge_source_changed"
    );
    let fresh = fixture.service.discover(Some(&fixture.project)).unwrap();
    let request = select(&fresh, "local.md", KnowledgeProvider::Claude);
    let outside = fixture.root.path().join("outside");
    write(outside.join("local.md"), b"PRIVATE_OUTSIDE_SENTINEL");
    fs::rename(
        rule.parent().unwrap(),
        fixture.root.path().join("moved-rules"),
    )
    .unwrap();
    symlink(&outside, rule.parent().unwrap()).unwrap();
    assert_eq!(
        fixture.service.read(&request).unwrap_err().code,
        "knowledge_source_changed"
    );
}

#[test]
fn text_read_limits_include_non_utf8_nul_and_worst_json_expansion() {
    let mut fixture = Fixture::new();
    write(fixture.project.path.join("AGENTS.md"), &vec![1_u8; 65_536]);
    write(fixture.project.path.join("CLAUDE.md"), &[0xff, 0xfe]);
    write(
        fixture.project.path.join("CLAUDE.local.md"),
        b"PRIVATE_NUL\0",
    );
    write(
        fixture.project.path.join(".cursor/rules/large.mdc"),
        &vec![b'a'; 65_537],
    );
    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    let content = fixture
        .service
        .read(&select(&scan, "AGENTS.md", KnowledgeProvider::Codex))
        .unwrap();
    assert_eq!(content.content.len(), 65_536);
    assert!(serde_json::to_vec(&content).unwrap().len() < 512 * 1_024);
    for suffix in ["CLAUDE.md", "CLAUDE.local.md"] {
        let error = fixture
            .service
            .read(&select(&scan, suffix, KnowledgeProvider::Claude))
            .unwrap_err();
        assert_eq!(error.code, "knowledge_source_invalid_text");
        assert!(!format!("{error:?}").contains("PRIVATE_NUL"));
    }
    assert_eq!(
        fixture
            .service
            .read(&select(&scan, "large.mdc", KnowledgeProvider::Cursor))
            .unwrap_err()
            .code,
        "knowledge_source_too_large"
    );
}

#[test]
fn cache_expiry_eviction_and_scan_entry_binding_are_enforced() {
    let mut fixture = Fixture::new();
    write(fixture.project.path.join("AGENTS.md"), b"Content");
    let first = fixture.service.discover(Some(&fixture.project)).unwrap();
    let second = fixture.service.discover(Some(&fixture.project)).unwrap();
    let entry = first.entries[0].entry_id;
    assert_eq!(
        fixture
            .service
            .read(&KnowledgeReadRequest {
                scan_id: second.scan_id,
                entry_id: entry
            })
            .unwrap_err()
            .code,
        "knowledge_entry_not_found"
    );
    fixture.service.scans.front_mut().unwrap().created =
        Instant::now().checked_sub(SCAN_TTL).unwrap();
    assert_eq!(
        fixture
            .service
            .scan_project_id(first.scan_id)
            .unwrap_err()
            .code,
        "knowledge_scan_expired"
    );
    for _ in 0..MAX_SCANS {
        fixture.service.discover(Some(&fixture.project)).unwrap();
    }
    assert_eq!(fixture.service.scans.len(), MAX_SCANS);
    assert_eq!(
        fixture
            .service
            .scan_project_id(second.scan_id)
            .unwrap_err()
            .code,
        "knowledge_scan_expired"
    );
}

#[test]
fn bounds_are_visible_and_project_sources_precede_large_global_libraries() {
    let mut fixture = Fixture::new();
    write(
        fixture.project.path.join("AGENTS.md"),
        b"Project is never starved",
    );
    for index in 0..520 {
        write(
            fixture
                .home()
                .join(format!(".claude/skills/skill-{index:04}/SKILL.md")),
            b"Skill",
        );
    }
    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    assert!(scan.truncated);
    assert!(scan.entries.len() <= MAX_ENTRIES);
    assert_eq!(scan.entries[0].scope, KnowledgeSourceScope::Project);
    assert!(serde_json::to_vec(&scan).unwrap().len() < 512 * 1_024);
}

#[test]
fn recursive_skill_cycles_and_depth_limits_terminate_with_visible_issues() {
    let mut fixture = Fixture::new();
    let root = fixture.project.path.join(".cursor/skills");
    fs::create_dir_all(root.join("group")).unwrap();
    symlink(&root, root.join("group/loop")).unwrap();
    let mut deep = root.clone();
    for _ in 0..20 {
        deep.push("nested");
    }
    write(deep.join("SKILL.md"), b"Beyond depth bound");
    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    assert!(scan.truncated);
    assert!(
        scan.issues
            .iter()
            .any(|issue| issue.code == "symlink_cycle")
    );
    assert!(scan.issues.iter().any(|issue| issue.code == "depth_limit"));
}

#[test]
fn enumeration_limit_preserves_candidates_already_collected() {
    let fixture = Fixture::new();
    for name in ["first", "second"] {
        write(
            fixture
                .project
                .path
                .join(format!(".claude/rules/{name}.md")),
            b"Rule",
        );
        write(
            fixture
                .project
                .path
                .join(format!(".cursor/skills/{name}/SKILL.md")),
            b"Skill",
        );
    }
    for (directory, shape, provider) in [
        (
            ".claude/rules",
            Shape::Rules("md"),
            KnowledgeProvider::Claude,
        ),
        (
            ".cursor/skills",
            Shape::Skills { recursive: true },
            KnowledgeProvider::Cursor,
        ),
    ] {
        let mut scanner = Scanner::new(KnowledgeScanId::new());
        // Use a small exact budget with real directories to exercise the same
        // boundary as a full 10,000-node scan without creating irrelevant files.
        scanner.remaining_nodes = 2;
        scanner.scan_source(&SourceSpec {
            base: fixture.project.path.clone(),
            directory,
            provider,
            scope: KnowledgeSourceScope::Project,
            shape,
        });
        assert_eq!(scanner.remaining_nodes, 0);
        assert!(scanner.response.truncated);
        assert_eq!(scanner.response.entries.len(), 2);
        assert!(
            scanner
                .response
                .issues
                .iter()
                .any(|issue| issue.code == "scan_limit")
        );
    }
}
