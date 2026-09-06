//! Bounded inventory and read capabilities for explicitly known rule/skill paths.

mod safe;

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, VecDeque},
    ffi::OsStr,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use cli_master_core::{
    ApiError, Project, ProjectId,
    knowledge::discovery::{
        KnowledgeDiscoverResponse, KnowledgeDiscoveryIssue, KnowledgeProvider,
        KnowledgeReadRequest, KnowledgeReadResponse, KnowledgeScanId, KnowledgeSourceAvailability,
        KnowledgeSourceEntry, KnowledgeSourceId, KnowledgeSourceKind, KnowledgeSourceScope,
    },
};

use safe::{Directory, DirectoryIdentity, FileFingerprint, SafeError};

const MAX_ENTRIES: usize = 512;
const MAX_VISITED: usize = 10_000;
const MAX_DEPTH: usize = 16;
const MAX_SCANS: usize = 4;
const SCAN_TTL: Duration = Duration::from_secs(300);
const MAX_ENTRY_JSON_BYTES: usize = 448 * 1_024;
const MAX_ISSUE_JSON_BYTES: usize = 32 * 1_024;
const MAX_ISSUES: usize = 32;

/// Daemon-selected roots; IPC callers cannot override any of these paths.
#[derive(Clone, Debug)]
pub(crate) struct DiscoveryRoots {
    pub(crate) home: Option<PathBuf>,
    pub(crate) codex_home: Option<PathBuf>,
    pub(crate) admin_skills: PathBuf,
}

impl DiscoveryRoots {
    /// Captures the owner's global roots once without reading configuration files.
    pub(crate) fn from_environment() -> Self {
        let absolute = |key: &str| {
            std::env::var_os(key)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        Self {
            home: absolute("HOME"),
            codex_home: absolute("CODEX_HOME"),
            admin_skills: PathBuf::from("/etc/codex/skills"),
        }
    }
}

/// Expiring read-only capabilities, synchronized by the existing daemon owner.
pub(crate) struct DiscoveryService {
    roots: DiscoveryRoots,
    scans: VecDeque<StoredScan>,
}

struct StoredScan {
    id: KnowledgeScanId,
    project_id: Option<ProjectId>,
    created: Instant,
    entries: BTreeMap<KnowledgeSourceId, StoredSource>,
}

struct StoredSource {
    entry: KnowledgeSourceEntry,
    parent_path: PathBuf,
    parent_identity: DirectoryIdentity,
    leaf: std::ffi::OsString,
    fingerprint: Option<FileFingerprint>,
}

impl DiscoveryService {
    pub(crate) fn new(roots: DiscoveryRoots) -> Self {
        Self {
            roots,
            scans: VecDeque::new(),
        }
    }

    /// Inventories known globals and project-root locations, without reading text.
    pub(crate) fn discover(
        &mut self,
        project: Option<&Project>,
    ) -> Result<KnowledgeDiscoverResponse, ApiError> {
        if project.is_some_and(|project| !project.path.is_absolute()) {
            return Err(ApiError::new(
                "invalid_project_path",
                "The registered project must have an absolute path.",
            ));
        }
        let scan_id = KnowledgeScanId::new();
        let mut scanner = Scanner::new(scan_id);
        scanner.issue("activation_not_evaluated", None,
            "Discovery reports known source locations, not whether a native CLI loaded them. Imports, config overrides, frontmatter activation and plugins are not evaluated.");
        if project.is_some() {
            scanner.issue("project_root_scope", None,
                "This inventory covers the registered project root. Rules or skills from a session's other working-directory ancestors are not evaluated.");
        }
        for spec in source_specs(&self.roots, project) {
            if scanner.full() || scanner.remaining_nodes == 0 {
                break;
            }
            scanner.scan_source(&spec);
        }
        let now = Instant::now();
        self.scans
            .retain(|scan| now.duration_since(scan.created) < SCAN_TTL);
        while self.scans.len() >= MAX_SCANS {
            self.scans.pop_front();
        }
        self.scans.push_back(StoredScan {
            id: scan_id,
            project_id: project.map(|project| project.id),
            created: now,
            entries: scanner.saved,
        });
        Ok(scanner.response)
    }

    /// Lets the dispatcher revalidate registered project ownership before reading.
    pub(crate) fn scan_project_id(
        &self,
        scan_id: KnowledgeScanId,
    ) -> Result<Option<ProjectId>, ApiError> {
        Ok(self.scan(scan_id)?.project_id)
    }

    /// Reopens only a cached descriptor path and revalidates its root and file identity.
    /// Retargeting a skill-directory alias does not change this scan's captured
    /// target. A new discovery is required to capture the replacement target.
    pub(crate) fn read(
        &self,
        request: &KnowledgeReadRequest,
    ) -> Result<KnowledgeReadResponse, ApiError> {
        let scan = self.scan(request.scan_id)?;
        let source = scan.entries.get(&request.entry_id).ok_or_else(|| {
            ApiError::new(
                "knowledge_entry_not_found",
                "The source does not belong to this scan.",
            )
        })?;
        let fingerprint = source
            .fingerprint
            .as_ref()
            .ok_or_else(|| unavailable(source.entry.availability))?;
        let (directory, _) =
            Directory::open_absolute(&source.parent_path, false).map_err(read_error)?;
        if directory.identity != source.parent_identity {
            return Err(read_error(SafeError::Changed));
        }
        let content = directory
            .read_candidate(&source.leaf, fingerprint)
            .map_err(read_error)?;
        Ok(KnowledgeReadResponse {
            entry: source.entry.clone(),
            content,
        })
    }

    fn scan(&self, id: KnowledgeScanId) -> Result<&StoredScan, ApiError> {
        self.scans
            .iter()
            .find(|scan| scan.id == id && scan.created.elapsed() < SCAN_TTL)
            .ok_or_else(|| {
                ApiError::new(
                    "knowledge_scan_expired",
                    "This source inventory expired. Refresh discovery and select the source again.",
                )
            })
    }
}

fn unavailable(availability: KnowledgeSourceAvailability) -> ApiError {
    let (code, message) = match availability {
        KnowledgeSourceAvailability::TooLarge => (
            "knowledge_source_too_large",
            "The source exceeds the 64 KiB read limit.",
        ),
        KnowledgeSourceAvailability::Symlink => (
            "knowledge_source_symlink",
            "Rule and SKILL.md leaf symlinks are not followed.",
        ),
        KnowledgeSourceAvailability::NonRegular => (
            "knowledge_source_non_regular",
            "The source is not a regular file.",
        ),
        KnowledgeSourceAvailability::Unreadable | KnowledgeSourceAvailability::Available => (
            "knowledge_source_unavailable",
            "The source could not be opened safely.",
        ),
    };
    ApiError::new(code, message)
}

fn read_error(error: SafeError) -> ApiError {
    match error {
        SafeError::Missing | SafeError::Changed | SafeError::Symlink | SafeError::NonRegular => {
            ApiError::new(
                "knowledge_source_changed",
                "The source changed since discovery. Refresh before reading it.",
            )
        }
        SafeError::TooLarge => unavailable(KnowledgeSourceAvailability::TooLarge),
        SafeError::InvalidText => ApiError::new(
            "knowledge_source_invalid_text",
            "The source is not NUL-free UTF-8 text.",
        ),
        SafeError::Unreadable | SafeError::Limit => {
            unavailable(KnowledgeSourceAvailability::Unreadable)
        }
    }
}

#[derive(Clone, Copy)]
enum Shape {
    Exact(&'static str),
    Rules(&'static str),
    Skills { recursive: bool },
}

struct SourceSpec {
    base: PathBuf,
    directory: &'static str,
    provider: KnowledgeProvider,
    scope: KnowledgeSourceScope,
    shape: Shape,
}

impl SourceSpec {
    fn kind(&self) -> KnowledgeSourceKind {
        match self.shape {
            Shape::Skills { .. } => KnowledgeSourceKind::Skill,
            _ => KnowledgeSourceKind::Rule,
        }
    }

    fn precedence(&self) -> &'static str {
        match (self.provider, self.shape) {
            (KnowledgeProvider::Codex, Shape::Skills { .. }) => {
                "Codex keeps same-named skills from different scopes distinct; discovery does not select one."
            }
            (KnowledgeProvider::Codex, _) => {
                "Codex prefers AGENTS.override.md over AGENTS.md within a directory; working-directory ancestry and load limits remain native CLI policy."
            }
            (KnowledgeProvider::Claude, Shape::Skills { .. }) => {
                "Claude personal skills take precedence over same-named project skills; filesystem names here are not parsed skill identifiers."
            }
            (KnowledgeProvider::Claude, _) => {
                "Claude combines applicable memory and rules; imports, rule path conditions and managed policy are not evaluated here."
            }
            (KnowledgeProvider::Cursor, Shape::Exact(_)) => {
                "Cursor recognizes AGENTS.md instructions; working-directory scope and rule activation remain native CLI policy."
            }
            (KnowledgeProvider::Cursor, _) => {
                "Cursor activation, frontmatter and duplicate precedence remain native CLI policy; this is an inventory only."
            }
        }
    }
}

fn source_specs(roots: &DiscoveryRoots, project: Option<&Project>) -> Vec<SourceSpec> {
    use KnowledgeProvider::{Claude, Codex, Cursor};
    use KnowledgeSourceScope::{Admin, Global, Project as ProjectScope};
    let mut specs = Vec::new();
    let codex_home = roots
        .codex_home
        .clone()
        .or_else(|| roots.home.as_ref().map(|home| home.join(".codex")));
    if let Some(base) = codex_home {
        for leaf in ["AGENTS.override.md", "AGENTS.md"] {
            specs.push(SourceSpec {
                base: base.clone(),
                directory: "",
                provider: Codex,
                scope: Global,
                shape: Shape::Exact(leaf),
            });
        }
    }
    if let Some(home) = &roots.home {
        specs.push(SourceSpec {
            base: home.clone(),
            directory: ".claude",
            provider: Claude,
            scope: Global,
            shape: Shape::Exact("CLAUDE.md"),
        });
        specs.push(SourceSpec {
            base: home.clone(),
            directory: ".claude/rules",
            provider: Claude,
            scope: Global,
            shape: Shape::Rules("md"),
        });
        add_skills(&mut specs, home, Global);
    }
    specs.push(SourceSpec {
        base: roots.admin_skills.clone(),
        directory: "",
        provider: Codex,
        scope: Admin,
        shape: Shape::Skills { recursive: false },
    });
    if let Some(project) = project {
        for leaf in ["AGENTS.override.md", "AGENTS.md"] {
            specs.push(SourceSpec {
                base: project.path.clone(),
                directory: "",
                provider: Codex,
                scope: ProjectScope,
                shape: Shape::Exact(leaf),
            });
        }
        for (directory, leaf) in [
            ("", "CLAUDE.md"),
            (".claude", "CLAUDE.md"),
            ("", "CLAUDE.local.md"),
        ] {
            specs.push(SourceSpec {
                base: project.path.clone(),
                directory,
                provider: Claude,
                scope: ProjectScope,
                shape: Shape::Exact(leaf),
            });
        }
        specs.push(SourceSpec {
            base: project.path.clone(),
            directory: ".claude/rules",
            provider: Claude,
            scope: ProjectScope,
            shape: Shape::Rules("md"),
        });
        specs.push(SourceSpec {
            base: project.path.clone(),
            directory: ".cursor/rules",
            provider: Cursor,
            scope: ProjectScope,
            shape: Shape::Rules("mdc"),
        });
        specs.push(SourceSpec {
            base: project.path.clone(),
            directory: "",
            provider: Cursor,
            scope: ProjectScope,
            shape: Shape::Exact("AGENTS.md"),
        });
        add_skills(&mut specs, &project.path, ProjectScope);
    }
    specs.sort_by_key(|spec| match spec.scope {
        ProjectScope => 0,
        Global => 1,
        Admin => 2,
    });
    specs
}

fn add_skills(specs: &mut Vec<SourceSpec>, base: &Path, scope: KnowledgeSourceScope) {
    for (provider, directory, recursive) in [
        (KnowledgeProvider::Codex, ".agents/skills", false),
        (KnowledgeProvider::Claude, ".claude/skills", false),
        (KnowledgeProvider::Cursor, ".cursor/skills", true),
        (KnowledgeProvider::Cursor, ".agents/skills", true),
    ] {
        specs.push(SourceSpec {
            base: base.to_path_buf(),
            directory,
            provider,
            scope,
            shape: Shape::Skills { recursive },
        });
    }
}

struct Scanner {
    response: KnowledgeDiscoverResponse,
    saved: BTreeMap<KnowledgeSourceId, StoredSource>,
    remaining_nodes: usize,
    entry_bytes: usize,
    issue_bytes: usize,
}

impl Scanner {
    fn new(scan_id: KnowledgeScanId) -> Self {
        Self {
            response: KnowledgeDiscoverResponse {
                scan_id,
                entries: Vec::new(),
                truncated: false,
                issues: Vec::new(),
            },
            saved: BTreeMap::new(),
            remaining_nodes: MAX_VISITED,
            entry_bytes: 0,
            issue_bytes: 0,
        }
    }

    fn full(&mut self) -> bool {
        if self.response.entries.len() >= MAX_ENTRIES || self.entry_bytes >= MAX_ENTRY_JSON_BYTES {
            self.response.truncated = true;
            true
        } else {
            false
        }
    }

    fn issue(&mut self, code: &'static str, path: Option<&Path>, message: &'static str) {
        let issue = KnowledgeDiscoveryIssue {
            code: code.to_owned(),
            source_path: path.and_then(Path::to_str).map(str::to_owned),
            message: message.to_owned(),
        };
        let bytes =
            serde_json::to_vec(&issue).map_or(MAX_ISSUE_JSON_BYTES, |value| value.len() + 1);
        if self.response.issues.len() < MAX_ISSUES
            && self.issue_bytes + bytes <= MAX_ISSUE_JSON_BYTES
        {
            self.issue_bytes += bytes;
            self.response.issues.push(issue);
        } else {
            self.response.truncated = true;
        }
    }

    fn scan_source(&mut self, spec: &SourceSpec) {
        let logical = spec.base.join(spec.directory);
        let (mut directory, _) = match Directory::open_absolute(&spec.base, true) {
            Ok(value) => value,
            Err(SafeError::Missing) => return,
            Err(_) => {
                self.issue(
                    "source_unavailable",
                    Some(&logical),
                    "The configured source directory could not be opened safely.",
                );
                return;
            }
        };
        let mut linked = false;
        for component in Path::new(spec.directory).components() {
            match directory.open_child(
                component.as_os_str(),
                spec.kind() == KnowledgeSourceKind::Skill,
            ) {
                Ok((child, via_link)) => {
                    directory = child;
                    linked |= via_link;
                }
                Err(SafeError::Missing) => return,
                Err(_) => {
                    self.issue("directory_unavailable", Some(&logical), "The source directory is unavailable; directory symlinks are supported only for known skill roots.");
                    return;
                }
            }
        }
        match spec.shape {
            Shape::Exact(leaf) => self.add_candidate(
                &directory,
                OsStr::new(leaf),
                spec,
                &logical.join(leaf),
                linked,
            ),
            Shape::Rules(extension) => {
                self.walk_rules(&directory, spec, &logical, extension, 0, linked);
            }
            Shape::Skills { recursive } => {
                self.walk_skills(&directory, spec, &logical, recursive, 0, linked, &[]);
            }
        }
    }

    fn names(&mut self, directory: &Directory, logical: &Path) -> Vec<std::ffi::OsString> {
        if let Ok(names) = directory.entry_names(&mut self.remaining_nodes) {
            if self.remaining_nodes == 0 {
                self.response.truncated = true;
                self.issue(
                    "scan_limit",
                    Some(logical),
                    "The filesystem-entry scan limit was reached.",
                );
            }
            names
        } else {
            self.issue(
                "directory_unavailable",
                Some(logical),
                "The source directory could not be enumerated safely.",
            );
            Vec::new()
        }
    }

    fn walk_rules(
        &mut self,
        directory: &Directory,
        spec: &SourceSpec,
        logical: &Path,
        extension: &str,
        depth: usize,
        linked: bool,
    ) {
        for name in self.names(directory, logical) {
            if self.full() {
                break;
            }
            if name == OsStr::new(".git") {
                continue;
            }
            let path = logical.join(&name);
            match directory.open_child(&name, false) {
                Ok((child, _)) => {
                    if self.remaining_nodes == 0 {
                        continue;
                    }
                    if depth >= MAX_DEPTH {
                        self.response.truncated = true;
                        self.issue(
                            "depth_limit",
                            Some(&path),
                            "The rule-directory depth limit was reached.",
                        );
                    } else {
                        self.walk_rules(&child, spec, &path, extension, depth + 1, linked);
                    }
                }
                Err(_) if Path::new(&name).extension() == Some(OsStr::new(extension)) => {
                    self.add_candidate(directory, &name, spec, &path, linked);
                }
                Err(SafeError::Symlink) => self.issue(
                    "directory_symlink_skipped",
                    Some(&path),
                    "Rule-directory symlinks are not followed.",
                ),
                Err(_) => (),
            }
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "bounded traversal carries source provenance and the ancestor cycle guard"
    )]
    fn walk_skills(
        &mut self,
        directory: &Directory,
        spec: &SourceSpec,
        logical: &Path,
        recursive: bool,
        depth: usize,
        linked: bool,
        ancestors: &[DirectoryIdentity],
    ) {
        let mut ancestors = ancestors.to_vec();
        ancestors.push(directory.identity.clone());
        for name in self.names(directory, logical) {
            if self.full() {
                break;
            }
            if name == OsStr::new(".git") {
                continue;
            }
            let path = logical.join(&name);
            match directory.open_child(&name, true) {
                Ok((child, via_link)) => {
                    if ancestors.contains(&child.identity) {
                        self.issue(
                            "symlink_cycle",
                            Some(&path),
                            "A skill-directory cycle was skipped.",
                        );
                        continue;
                    }
                    self.add_candidate(
                        &child,
                        OsStr::new("SKILL.md"),
                        spec,
                        &path.join("SKILL.md"),
                        linked || via_link,
                    );
                    if recursive && self.remaining_nodes > 0 {
                        if depth >= MAX_DEPTH {
                            self.response.truncated = true;
                            self.issue(
                                "depth_limit",
                                Some(&path),
                                "The skill-directory depth limit was reached.",
                            );
                        } else {
                            self.walk_skills(
                                &child,
                                spec,
                                &path,
                                true,
                                depth + 1,
                                linked || via_link,
                                &ancestors,
                            );
                        }
                    }
                }
                Err(SafeError::NonRegular | SafeError::Missing) => (),
                Err(_) => self.issue(
                    "skill_directory_unavailable",
                    Some(&path),
                    "The skill directory or its bounded symlink target could not be opened safely.",
                ),
            }
        }
    }

    fn add_candidate(
        &mut self,
        directory: &Directory,
        leaf: &OsStr,
        spec: &SourceSpec,
        source_path: &Path,
        linked: bool,
    ) {
        if self.full() {
            return;
        }
        let Some(path) = source_path.to_str() else {
            self.issue("non_utf8_path", None, "A source with a non-UTF-8 path was skipped; path bytes were not converted lossily.");
            return;
        };
        let candidate = match directory.candidate(leaf) {
            Ok(candidate) => candidate,
            Err(SafeError::Missing) => return,
            Err(_) => {
                self.issue(
                    "source_unavailable",
                    Some(source_path),
                    "Source metadata could not be read safely.",
                );
                return;
            }
        };
        let name = if spec.kind() == KnowledgeSourceKind::Skill {
            source_path
                .parent()
                .and_then(Path::file_name)
                .and_then(OsStr::to_str)
                .unwrap_or("SKILL.md")
        } else {
            leaf.to_str().unwrap_or("rule")
        };
        let entry = KnowledgeSourceEntry {
            entry_id: KnowledgeSourceId::new(),
            kind: spec.kind(),
            provider: spec.provider,
            scope: spec.scope,
            source_path: path.to_owned(),
            name: name.to_owned(),
            scope_directory: if spec.scope == KnowledgeSourceScope::Project {
                ".".to_owned()
            } else {
                String::new()
            },
            precedence_hint: spec.precedence().to_owned(),
            via_symlink: linked,
            availability: candidate.availability,
        };
        let bytes =
            serde_json::to_vec(&entry).map_or(MAX_ENTRY_JSON_BYTES, |value| value.len() + 1);
        if self.entry_bytes + bytes > MAX_ENTRY_JSON_BYTES {
            self.response.truncated = true;
            self.entry_bytes = MAX_ENTRY_JSON_BYTES;
            self.issue(
                "response_limit",
                None,
                "The bounded source-inventory response limit was reached.",
            );
            return;
        }
        self.entry_bytes += bytes;
        if linked
            && !self
                .response
                .issues
                .iter()
                .any(|issue| issue.code == "captured_skill_target")
        {
            self.issue("captured_skill_target", None, "Skill-directory targets are captured for this scan. Refresh discovery to follow a retargeted directory symlink.");
        }
        self.saved.insert(
            entry.entry_id,
            StoredSource {
                entry: entry.clone(),
                parent_path: directory.path.clone(),
                parent_identity: directory.identity.clone(),
                leaf: leaf.to_os_string(),
                fingerprint: candidate.fingerprint,
            },
        );
        self.response.entries.push(entry);
    }
}
