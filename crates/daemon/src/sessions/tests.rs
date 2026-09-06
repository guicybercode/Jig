use std::fs;
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use super::*;

fn registry(directory: &Path) -> SessionRegistry {
    let storage = Storage::open_migrated(directory.join("sessions.sqlite3"))
        .expect("fixture storage should open");
    SessionRegistry::new(
        storage,
        DaemonInstanceId::new(),
        directory.join("worktrees"),
        None,
    )
    .expect("session registry should initialize")
}

#[test]
fn gemini_detection_checks_real_executable_permissions_without_running_it() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let registry = registry(temporary.path());
    let environment = LaunchEnvironment::from_search_paths([temporary.path()]);
    let request = AgentDetectRequest::try_new(vec![builtin_agent_ids::gemini()])
        .expect("Gemini detection request should validate");
    let executable = temporary.path().join("gemini");

    let missing = registry
        .detect_agents_in_environment(&request, &environment)
        .expect("missing executable should produce a detection result");
    assert_eq!(missing.detections.len(), 1);
    assert_eq!(missing.detections[0].agent_id, builtin_agent_ids::gemini());
    assert!(!missing.detections[0].available);
    assert_eq!(
        missing.detections[0].error_code.as_deref(),
        Some("executable_not_found")
    );

    // This is deliberately not an executable program: discovery must inspect it, not launch it.
    fs::write(&executable, "not a program\n").expect("fixture file should write");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o600))
        .expect("fixture permissions should apply");
    let denied = registry
        .detect_agents_in_environment(&request, &environment)
        .expect("non-executable file should produce a detection result");
    assert!(!denied.detections[0].available);
    assert!(denied.detections[0].executable_path.is_none());
    assert_eq!(
        denied.detections[0].error_code.as_deref(),
        Some("executable_not_executable")
    );

    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("executable permissions should apply");
    let found = registry
        .detect_agents_in_environment(&request, &environment)
        .expect("executable fixture should be detected");
    assert!(found.detections[0].available);
    assert_eq!(
        found.detections[0].executable_path.as_deref(),
        Some(
            executable
                .canonicalize()
                .expect("fixture should canonicalize")
                .as_path()
        )
    );
    assert!(found.detections[0].error_code.is_none());
}

#[test]
fn seeding_gemini_upgrades_existing_catalog_and_preserves_disabled_agents() {
    let temporary = TempDir::new().expect("temporary directory should exist");
    let first = registry(temporary.path());
    let legacy_agents = {
        let storage = first.storage().expect("fixture storage should lock");
        storage
            .remove_agent_metadata(builtin_agent_ids::gemini())
            .expect("fixture should represent the catalog before Gemini support");
        storage.list_agents().expect("legacy catalog should load")
    };
    assert_eq!(legacy_agents.len(), 4);
    drop(first);

    let upgraded = registry(temporary.path());
    {
        let storage = upgraded.storage().expect("fixture storage should lock");
        for agent in &legacy_agents {
            assert_eq!(
                storage
                    .get_agent(agent.id)
                    .expect("legacy agent should load"),
                Some(agent.clone())
            );
        }
        storage
            .set_agent_enabled(builtin_agent_ids::gemini(), false, 2)
            .expect("Gemini should be present and configurable");
    }
    drop(upgraded);

    let reopened = registry(temporary.path());
    let agents = reopened.agents().expect("reopened catalog should load");
    assert_eq!(agents.len(), 5);
    let gemini = agents
        .iter()
        .find(|agent| agent.id == builtin_agent_ids::gemini())
        .expect("Gemini should retain its stable ID");
    assert!(!gemini.enabled);
    let detections = reopened
        .detect_agents_in_environment(
            &AgentDetectRequest::default(),
            &LaunchEnvironment::default(),
        )
        .expect("disabled agent should be omitted from detection");
    assert!(
        detections
            .detections
            .iter()
            .all(|agent| agent.agent_id != gemini.id)
    );
}
