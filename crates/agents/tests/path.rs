mod common;

use std::path::PathBuf;

use cli_master_agents::{AgentAdapter, CodexAdapter, DetectionResult, LaunchEnvironment};
use tempfile::TempDir;

use common::executable;

#[test]
fn extra_paths_are_searched_before_standard_fallbacks() {
    let inherited = TempDir::new().expect("inherited");
    let extra = TempDir::new().expect("extra");
    let standard = TempDir::new().expect("standard");
    executable(inherited.path(), "other");
    let extra_binary = executable(extra.path(), "codex");
    executable(standard.path(), "codex");

    let environment = LaunchEnvironment::from_search_paths([inherited.path()])
        .with_extra_paths([extra.path()])
        .with_standard_path_override([standard.path()]);

    assert_eq!(
        CodexAdapter.detect(&environment),
        DetectionResult::Found {
            executable: extra_binary
        }
    );

    let diagnostics = environment.path_diagnostics();
    assert!(
        diagnostics
            .notes
            .iter()
            .any(|note| note.contains("Inherited PATH"))
    );
    assert!(!format!("{diagnostics:?}").contains("SECRET="));
}

#[test]
fn standard_override_is_used_when_not_on_inherited_path() {
    let standard = TempDir::new().expect("standard");
    let expected = executable(standard.path(), "codex");
    let environment = LaunchEnvironment::default().with_standard_path_override([standard.path()]);
    assert_eq!(
        CodexAdapter.detect(&environment),
        DetectionResult::Found {
            executable: expected
        }
    );
}

#[test]
fn isolated_search_paths_do_not_observe_host_binaries() {
    let temp = TempDir::new().expect("temp");
    let environment = LaunchEnvironment::from_search_paths([temp.path()]);
    assert_eq!(CodexAdapter.detect(&environment), DetectionResult::NotFound);
    assert!(
        !environment
            .search_paths()
            .contains(&PathBuf::from("/usr/bin"))
    );
}

#[test]
fn absolute_executable_does_not_require_path_membership() {
    let temp = TempDir::new().expect("temp");
    let path = executable(temp.path(), "codex");
    let environment = LaunchEnvironment::default();
    assert_eq!(
        environment.detect(&path),
        DetectionResult::Found { executable: path }
    );
}
