use super::*;

#[test]
fn nested_formats_keep_owner_scope_and_provider_identity() {
    let mut fixture = Fixture::new();
    write(fixture.project.path.join("AGENTS.md"), b"Root instructions");
    let nested = fixture.project.path.join("packages/api");
    for relative in [
        "AGENTS.md",
        "AGENTS.override.md",
        "CLAUDE.md",
        "CLAUDE.local.md",
        ".claude/CLAUDE.md",
        ".claude/rules/security/request.md",
        ".cursor/rules/nested/request.mdc",
        ".agents/skills/shared/SKILL.md",
        ".claude/skills/local/SKILL.md",
        ".cursor/skills/group/nested/SKILL.md",
    ] {
        write(nested.join(relative), b"Nested source\n@private.txt");
    }
    for relative in [
        ".cursor/rules/ignored.md",
        ".claude/settings.json",
        ".claude/skills/local/AGENTS.md",
        "private.txt",
    ] {
        write(nested.join(relative), b"PRIVATE_CONFIG_SENTINEL");
    }

    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    assert!(!scan.truncated);
    assert_eq!(scan.entries.len(), 14);
    assert_eq!(
        scan.entries
            .iter()
            .filter(|entry| entry.scope_directory == ".")
            .count(),
        2
    );
    let nested_entries = scan
        .entries
        .iter()
        .filter(|entry| entry.scope_directory != ".");
    for entry in nested_entries {
        assert_eq!(entry.scope, KnowledgeSourceScope::Project);
        assert_eq!(entry.scope_directory, "packages/api");
        assert!(!entry.via_symlink);
        let read = fixture
            .service
            .read(&KnowledgeReadRequest {
                scan_id: scan.scan_id,
                entry_id: entry.entry_id,
            })
            .unwrap();
        assert_eq!(read.entry, *entry);
        assert_eq!(read.content, "Nested source\n@private.txt");
    }
    let shared = scan
        .entries
        .iter()
        .filter(|entry| entry.name == "shared")
        .collect::<Vec<_>>();
    assert_eq!(shared.len(), 2);
    assert_ne!(shared[0].entry_id, shared[1].entry_id);
    assert_ne!(shared[0].provider, shared[1].provider);
    assert_eq!(shared[0].source_path, shared[1].source_path);
}

#[test]
fn pruning_is_exact_and_does_not_hide_build_or_registered_roots() {
    let mut fixture = Fixture::new();
    // The explicitly registered root remains eligible even with a pruned name.
    fixture.project.path = fixture.project.path.join("vendor");
    write(fixture.project.path.join("CLAUDE.md"), b"Selected root");
    for name in [".git", "node_modules", "vendor", ".venv", "venv", ".codex"] {
        write(
            fixture.project.path.join(name).join("cache/AGENTS.md"),
            b"Pruned",
        );
    }
    for name in ["vendor-tools", "build", "dist", "target"] {
        write(
            fixture.project.path.join(name).join("CLAUDE.md"),
            b"Valid scope",
        );
    }
    for name in [".agents", ".claude", ".cursor"] {
        write(
            fixture
                .project
                .path
                .join(name)
                .join("skills/tool/AGENTS.md"),
            b"Not a project scope",
        );
    }
    write(
        fixture.project.path.join(".agents/skills/tool/SKILL.md"),
        b"Known skill",
    );

    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    assert!(!scan.truncated);
    assert_eq!(scan.entries.len(), 7);
    assert!(scan.entries.iter().all(|entry| entry.name != "AGENTS.md"));
    for scope in [".", "vendor-tools", "build", "dist", "target"] {
        assert!(
            scan.entries
                .iter()
                .any(|entry| entry.scope_directory == scope)
        );
    }
    assert_eq!(
        scan.entries
            .iter()
            .filter(|entry| entry.name == "tool")
            .count(),
        2
    );
    assert!(
        scan.issues
            .iter()
            .any(|issue| issue.code == "project_scan_policy")
    );
}

#[test]
fn nested_directory_links_are_skipped_except_known_skill_locations() {
    let mut fixture = Fixture::new();
    let outside = fixture.root.path().join("outside");
    write(outside.join("CLAUDE.md"), b"Outside instructions");
    write(outside.join("SKILL.md"), b"Allowed linked skill");
    let nested = fixture.project.path.join("packages/api");
    fs::create_dir_all(nested.join(".claude/skills")).unwrap();
    symlink(&outside, nested.join("escape")).unwrap();
    symlink(&outside, nested.join(".claude/skills/linked")).unwrap();
    symlink(outside.join("CLAUDE.md"), nested.join("CLAUDE.md")).unwrap();

    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    assert_eq!(scan.entries.len(), 2);
    assert!(
        scan.entries
            .iter()
            .all(|entry| entry.scope_directory == "packages/api")
    );
    let linked = select(&scan, "/linked/SKILL.md", KnowledgeProvider::Claude);
    let read = fixture.service.read(&linked).unwrap();
    assert!(read.entry.via_symlink);
    assert_eq!(read.content, "Allowed linked skill");
    let leaf = select(&scan, "/CLAUDE.md", KnowledgeProvider::Claude);
    assert_eq!(
        fixture.service.read(&leaf).unwrap_err().code,
        "knowledge_source_symlink"
    );
    assert!(
        scan.issues
            .iter()
            .any(|issue| issue.code == "directory_symlink_skipped")
    );
}

#[test]
fn pinned_nested_scope_never_reopens_a_replacement_directory_link() {
    let fixture = Fixture::new();
    let nested = fixture.project.path.join("nested");
    write(nested.join("CLAUDE.md"), b"Original pinned directory");
    write(nested.join(".claude/CLAUDE.md"), b"Original pinned child");
    let outside = fixture.root.path().join("outside");
    write(outside.join("AGENTS.md"), b"OUTSIDE_SENTINEL");
    write(
        outside.join(".claude/rules/outside.md"),
        b"OUTSIDE_SENTINEL",
    );
    let (root, _) = Directory::open_absolute(&fixture.project.path, true).unwrap();
    let (pinned, _) = root.open_child(OsStr::new("nested"), false).unwrap();
    fs::rename(&nested, fixture.root.path().join("moved")).unwrap();
    symlink(&outside, &nested).unwrap();

    let mut scanner = Scanner::new(KnowledgeScanId::new());
    scanner.scan_project_scope(&pinned, &nested, Path::new("nested"), 1);
    assert_eq!(scanner.response.entries.len(), 2);
    assert!(
        scanner
            .response
            .entries
            .iter()
            .all(|entry| entry.name == "CLAUDE.md")
    );
    assert!(
        scanner
            .saved
            .values()
            .all(|source| source.entry.provider == KnowledgeProvider::Claude)
    );
}

#[test]
fn nested_parent_replacement_invalidates_existing_read_capability() {
    let mut fixture = Fixture::new();
    let nested = fixture.project.path.join("packages/api");
    write(nested.join("CLAUDE.md"), b"Original");
    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    let request = select(&scan, "/api/CLAUDE.md", KnowledgeProvider::Claude);
    fs::rename(&nested, fixture.root.path().join("moved")).unwrap();
    write(nested.join("CLAUDE.md"), b"Replacement");
    assert_eq!(
        fixture.service.read(&request).unwrap_err().code,
        "knowledge_source_changed"
    );

    let fresh = fixture.service.discover(Some(&fixture.project)).unwrap();
    let read = fixture
        .service
        .read(&select(&fresh, "/api/CLAUDE.md", KnowledgeProvider::Claude))
        .unwrap();
    assert_eq!(read.content, "Replacement");
    assert_eq!(read.entry.scope_directory, "packages/api");
}

#[test]
fn depth_is_cumulative_across_project_rules_and_skill_directories() {
    let mut fixture = Fixture::new();
    let mut scopes = vec![fixture.project.path.clone()];
    for depth in 1..=MAX_DEPTH + 1 {
        scopes.push(scopes[depth - 1].join("nested"));
    }
    for (depth, path, content) in [
        (16, "CLAUDE.md", "Allowed deepest scope"),
        (17, "CLAUDE.md", "Beyond project depth"),
        (14, ".claude/rules/allowed.md", "Allowed deepest rule"),
        (14, ".claude/rules/group/denied.md", "Beyond rule depth"),
        (
            13,
            ".claude/skills/allowed/SKILL.md",
            "Allowed deepest skill",
        ),
        (14, ".claude/skills/denied/SKILL.md", "Beyond skill depth"),
        (
            12,
            ".cursor/skills/group/allowed/SKILL.md",
            "Allowed recursive skill",
        ),
        (
            12,
            ".cursor/skills/group/deeper/denied/SKILL.md",
            "Beyond recursive depth",
        ),
    ] {
        write(scopes[depth].join(path), content.as_bytes());
    }
    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    assert!(scan.truncated);
    assert!(scan.issues.iter().any(|issue| issue.code == "depth_limit"));
    assert_eq!(scan.entries.len(), 4);
    for entry in &scan.entries {
        let read = fixture
            .service
            .read(&KnowledgeReadRequest {
                scan_id: scan.scan_id,
                entry_id: entry.entry_id,
            })
            .unwrap();
        assert!(read.content.starts_with("Allowed"));
    }
}

#[test]
fn enumeration_budget_is_shared_and_keeps_already_collected_scope_candidates() {
    let fixture = Fixture::new();
    for name in ["first", "second"] {
        write(
            fixture.project.path.join(name).join("CLAUDE.md"),
            b"Collected scope",
        );
        write(
            fixture.project.path.join(name).join("deeper/CLAUDE.md"),
            b"Not enumerated",
        );
    }
    write(fixture.home().join(".codex/AGENTS.md"), b"Global");
    let mut scanner = Scanner::new(KnowledgeScanId::new());
    scanner.remaining_nodes = 2;
    scanner.scan_inventory(&fixture.service.roots, Some(&fixture.project));
    assert_eq!(scanner.remaining_nodes, 0);
    assert!(scanner.response.truncated);
    assert_eq!(scanner.response.entries.len(), 3);
    assert_eq!(
        scanner.response.entries[0].scope,
        KnowledgeSourceScope::Global
    );
    assert!(
        scanner.response.entries[1..]
            .iter()
            .all(|entry| { matches!(entry.scope_directory.as_str(), "first" | "second") })
    );
    assert!(
        scanner
            .response
            .issues
            .iter()
            .any(|issue| issue.code == "scan_limit")
    );
}

#[test]
fn root_and_globals_precede_descendants_under_the_shared_entry_limit() {
    let mut fixture = Fixture::new();
    write(fixture.project.path.join("CLAUDE.md"), b"Root");
    write(fixture.home().join(".codex/AGENTS.md"), b"Global");
    for index in 0..MAX_ENTRIES + 4 {
        write(
            fixture
                .project
                .path
                .join(format!("scopes/scope-{index:04}/CLAUDE.md")),
            b"Nested",
        );
    }
    let scan = fixture.service.discover(Some(&fixture.project)).unwrap();
    assert_eq!(scan.entries.len(), MAX_ENTRIES);
    assert!(scan.truncated);
    assert_eq!(scan.entries[0].scope_directory, ".");
    assert_eq!(scan.entries[1].scope, KnowledgeSourceScope::Global);
    assert!(
        scan.entries[2..]
            .iter()
            .all(|entry| entry.scope_directory.starts_with("scopes/"))
    );
    assert!(serde_json::to_vec(&scan).unwrap().len() < 512 * 1_024);
}

#[test]
fn subtree_inventory_does_not_infer_ancestors_or_walk_the_global_home() {
    let mut fixture = Fixture::new();
    write(
        fixture.root.path().join("CLAUDE.md"),
        b"Ancestor is outside this inventory",
    );
    write(
        fixture.home().join("arbitrary/CLAUDE.md"),
        b"Home is not a project subtree",
    );
    write(fixture.home().join(".claude/CLAUDE.md"), b"Known global");
    write(
        fixture.project.path.join("nested/CLAUDE.md"),
        b"Nested project",
    );
    let globals = fixture.service.discover(None).unwrap();
    assert_eq!(globals.entries.len(), 1);
    assert_eq!(globals.entries[0].scope, KnowledgeSourceScope::Global);
    let project = fixture.service.discover(Some(&fixture.project)).unwrap();
    assert_eq!(project.entries.len(), 2);
    assert!(project.issues.iter().any(|issue| {
        issue.code == "project_subtree_scope" && issue.message.contains("ancestors")
    }));
    assert!(
        project
            .entries
            .iter()
            .any(|entry| entry.scope_directory == "nested")
    );
}
