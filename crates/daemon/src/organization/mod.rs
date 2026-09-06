//! Organization metadata has no session/process side effects.
use std::time::{SystemTime, UNIX_EPOCH};

use cli_master_core::{
    ApiError,
    wire::{OrganizationGetRequest, OrganizationSaveRequest, method},
};
use cli_master_storage::Storage;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

pub(crate) fn dispatch(method: &str, payload: Value, storage: &Storage) -> Result<Value, ApiError> {
    match method {
        method::ORGANIZATION_GET => {
            let request: OrganizationGetRequest = decode(payload)?;
            encode(
                storage
                    .get_organization(&request)
                    .map_err(|error| error.to_api_error())?,
            )
        }
        method::ORGANIZATION_SAVE => {
            let request: OrganizationSaveRequest = decode(payload)?;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
                .ok_or_else(|| {
                    ApiError::new("clock_unavailable", "The local clock is unavailable.")
                })?;
            encode(
                storage
                    .save_organization(&request, now)
                    .map_err(|error| error.to_api_error())?,
            )
        }
        _ => Err(ApiError::new(
            "unsupported_method",
            "Unknown organization operation.",
        )),
    }
}

fn decode<T: DeserializeOwned>(payload: Value) -> Result<T, ApiError> {
    serde_json::from_value(payload).map_err(|_| {
        ApiError::new("invalid_payload", "The organization request is invalid.")
            .with_action("Check the selected entity, flags, workflow and revision.")
    })
}

fn encode(value: impl Serialize) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(|_| {
        ApiError::new(
            "internal_error",
            "The organization response could not be encoded.",
        )
    })
}
