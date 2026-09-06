//! Local user-authored knowledge; no session or process side effects.

use cli_master_core::ApiError;
use cli_master_core::wire::{
    EmptyResponse, KnowledgeDeleteRequest, KnowledgeListRequest, KnowledgeSaveRequest,
};
use cli_master_storage::Storage;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Handle only the registered knowledge methods through the existing database.
pub(crate) fn dispatch(method: &str, payload: Value, storage: &Storage) -> Result<Value, ApiError> {
    use cli_master_core::wire::method;
    match method {
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
        ApiError::new(
            "invalid_payload",
            "The saved prompt or context request is invalid.",
        )
        .with_action("Check the title, text, scope and revision, then retry.")
    })
}

fn encode(value: impl serde::Serialize) -> Result<Value, ApiError> {
    serde_json::to_value(value)
        .map_err(|_| ApiError::new("internal_error", "The local response could not be encoded."))
}
