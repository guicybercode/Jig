//! Local knowledge and source discovery; no session or process side effects.

pub(crate) mod discovery;

use cli_master_core::ApiError;
use cli_master_core::wire::{
    EmptyResponse, KnowledgeDeleteRequest, KnowledgeDiscoverRequest, KnowledgeListRequest,
    KnowledgeReadRequest, KnowledgeSaveRequest,
};
use cli_master_storage::Storage;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Handle only the registered knowledge methods through the existing database.
pub(crate) fn dispatch(
    method: &str,
    payload: Value,
    storage: &Storage,
    discovery: &Mutex<discovery::DiscoveryService>,
) -> Result<Value, ApiError> {
    use cli_master_core::wire::method;
    match method {
        method::KNOWLEDGE_DISCOVER => {
            let request: KnowledgeDiscoverRequest = decode(payload)?;
            let project = request
                .project_id
                .map(|id| registered_project(storage, id))
                .transpose()?;
            let mut service = discovery.lock().map_err(|_| discovery_unavailable())?;
            encode(service.discover(project.as_ref())?)
        }
        method::KNOWLEDGE_READ => {
            let request: KnowledgeReadRequest = decode(payload)?;
            let service = discovery.lock().map_err(|_| discovery_unavailable())?;
            if let Some(project_id) = service.scan_project_id(request.scan_id)? {
                registered_project(storage, project_id)?;
            }
            encode(service.read(&request)?)
        }
        method::KNOWLEDGE_LIST => {
            let request: KnowledgeListRequest = decode(payload)?;
            encode(
                storage
                    .list_knowledge(&request)
                    .map_err(|error| error.to_api_error())?,
            )
        }
        method::KNOWLEDGE_SAVE => {
            let request: KnowledgeSaveRequest = decode(payload)?;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
                .ok_or_else(|| {
                    ApiError::new("clock_unavailable", "The local clock is unavailable.")
                })?;
            encode(
                storage
                    .save_knowledge(&request, now)
                    .map_err(|error| error.to_api_error())?,
            )
        }
        method::KNOWLEDGE_DELETE => {
            let request: KnowledgeDeleteRequest = decode(payload)?;
            storage
                .delete_knowledge(&request)
                .map_err(|error| error.to_api_error())?;
            encode(EmptyResponse::default())
        }
        _ => Err(ApiError::new(
            "unsupported_method",
            "Unknown knowledge operation.",
        )),
    }
}

fn decode<T: DeserializeOwned>(payload: Value) -> Result<T, ApiError> {
    // Serde errors can quote a user value (e.g. an invalid enum variant). Never
    // attach them to errors for a text-bearing knowledge request.
    serde_json::from_value(payload).map_err(|_| {
        ApiError::new("invalid_payload", "The local knowledge request is invalid.")
            .with_action("Check the selected source, scope and revision, then retry.")
    })
}

fn encode(value: impl serde::Serialize) -> Result<Value, ApiError> {
    serde_json::to_value(value)
        .map_err(|_| ApiError::new("internal_error", "The local response could not be encoded."))
}

fn registered_project(
    storage: &Storage,
    id: cli_master_core::ProjectId,
) -> Result<cli_master_core::Project, ApiError> {
    storage
        .get_project(id)
        .map_err(|_| {
            ApiError::new(
                "knowledge_storage_unavailable",
                "The registered project could not be checked.",
            )
        })?
        .ok_or_else(|| {
            ApiError::new("project_not_found", "This project is no longer registered.")
                .with_action("Choose a registered project and refresh the sources.")
        })
}

fn discovery_unavailable() -> ApiError {
    ApiError::new(
        "knowledge_discovery_unavailable",
        "The local source inventory is unavailable.",
    )
    .with_action("Reconnect and refresh the sources.")
}
