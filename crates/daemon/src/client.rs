use std::collections::VecDeque;
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cli_master_core::wire::{
    self, DiagnosticsResponse, EmptyRequest, EmptyResponse, GitDiffRequest, GitStatusRequest,
    SessionSubscribeRequest, SessionUnsubscribeRequest, StateSnapshotResponse,
};
use cli_master_core::{
    ApiError, EnvelopeKind, PROTOCOL_V1, RequestEnvelope, RequestId, ResponseEnvelope,
};
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::net::UnixStream;
use tokio_util::codec::LengthDelimitedCodec;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::events::{ClientHandle, FanoutEvent, SubscribeError};
use crate::server::{MAX_FRAME_LENGTH, ServerState};

pub(crate) async fn serve_client(
    stream: UnixStream,
    state: Arc<ServerState>,
    cancellation: CancellationToken,
) {
    if let Err(error) = validate_peer(&stream) {
        warn!(%error, "rejecting daemon client with invalid peer credentials");
        return;
    }
    let client = state.events.connect_client();
    let mut framed = LengthDelimitedCodec::builder()
        .max_frame_length(MAX_FRAME_LENGTH)
        .new_framed(stream);
    let mut pending: VecDeque<FanoutEvent> = VecDeque::new();

    loop {
        while let Some(event) = pending.pop_front() {
            if !send_json(&mut framed, &event.into_envelope()).await {
                state.events.disconnect_client(client.id);
                return;
            }
        }
        pending.extend(client.drain());
        if !pending.is_empty() {
            continue;
        }

        let notified = client.notify().notified();
        tokio::pin!(notified);
        pending.extend(client.drain());
        if !pending.is_empty() {
            continue;
        }

        let frame = tokio::select! {
            () = cancellation.cancelled() => break,
            () = notified => {
                pending.extend(client.drain());
                continue;
            }
            frame = framed.next() => frame,
        };

        let Some(frame) = frame else {
            break;
        };
        let bytes = match frame {
            Ok(bytes) => bytes,
            Err(error) => {
                warn!(%error, max_frame_length = MAX_FRAME_LENGTH, "closing client with invalid frame");
                break;
            }
        };

        match decode_request(&bytes) {
            Ok(request) => {
                let (response, replay) = dispatch(request, &state, &client).await;
                if !send_json(&mut framed, &response).await {
                    break;
                }
                pending.extend(replay);
            }
            Err(failure) => {
                let Some(request_id) = failure.request_id else {
                    warn!(error_code = %failure.error.code, "closing uncorrelatable invalid request");
                    break;
                };
                if !send_json(
                    &mut framed,
                    &ResponseEnvelope::<Value>::failure(request_id, *failure.error),
                )
                .await
                {
                    break;
                }
            }
        }
    }

    state.events.disconnect_client(client.id);
}

async fn send_json<T: serde::Serialize>(
    framed: &mut tokio_util::codec::Framed<UnixStream, LengthDelimitedCodec>,
    value: &T,
) -> bool {
    let encoded = match serde_json::to_vec(value) {
        Ok(encoded) => encoded,
        Err(error) => {
            warn!(%error, "could not encode daemon frame");
            return false;
        }
    };
    if let Err(error) = framed.send(encoded.into()).await {
        debug!(%error, "client disconnected before frame completed");
        return false;
    }
    true
}

#[cfg(target_os = "linux")]
fn validate_peer(stream: &UnixStream) -> Result<(), io::Error> {
    let credentials = rustix::net::sockopt::socket_peercred(stream).map_err(io::Error::from)?;
    if credentials.uid != rustix::process::geteuid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "peer effective user does not own the daemon",
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(
    clippy::unnecessary_wraps,
    reason = "keeps the security check call site identical across supported targets"
)]
fn validate_peer(_stream: &UnixStream) -> Result<(), io::Error> {
    Ok(())
}

async fn dispatch(
    request: RequestEnvelope<Value>,
    state: &ServerState,
    client: &ClientHandle,
) -> (ResponseEnvelope<Value>, Vec<FanoutEvent>) {
    if let Some(response) = reject_bad_envelope(&request) {
        return (response, Vec::new());
    }

    let result = match request.method.as_str() {
        wire::method::SYSTEM_HELLO => {
            if let Err(error) = decode_payload::<EmptyRequest>(request.payload) {
                return (invalid_payload(request.request_id, &error), Vec::new());
            }
            serde_json::to_value(&state.hello)
        }
        wire::method::STATE_SNAPSHOT => {
            if let Err(error) = decode_payload::<EmptyRequest>(request.payload) {
                return (invalid_payload(request.request_id, &error), Vec::new());
            }
            serde_json::to_value(StateSnapshotResponse {
                schema_version: state.schema_version,
                projects: Vec::new(),
                agents: Vec::new(),
                sessions: Vec::new(),
                worktrees: Vec::new(),
            })
        }
        wire::method::SESSION_SUBSCRIBE => {
            return handle_subscribe(request.request_id, request.payload, state, client);
        }
        wire::method::SESSION_UNSUBSCRIBE => {
            let payload = match decode_payload::<SessionUnsubscribeRequest>(request.payload) {
                Ok(payload) => payload,
                Err(error) => return (invalid_payload(request.request_id, &error), Vec::new()),
            };
            state.events.unsubscribe(client.id, payload.session_id);
            serde_json::to_value(EmptyResponse {})
        }
        wire::method::DIAGNOSTICS_GET => {
            if let Err(error) = decode_payload::<EmptyRequest>(request.payload) {
                return (invalid_payload(request.request_id, &error), Vec::new());
            }
            serde_json::to_value(diagnostics_payload(state))
        }
        wire::method::GIT_STATUS => {
            return handle_git_status(request.request_id, request.payload, state).await;
        }
        wire::method::GIT_DIFF => {
            return handle_git_diff(request.request_id, request.payload, state).await;
        }
        method if wire::method::is_supported(method) => {
            return (
                ResponseEnvelope::failure(
                    request.request_id,
                    ApiError::new(
                        "method_not_implemented",
                        "The requested daemon method is part of the Beta contract but is not implemented",
                    )
                    .with_detail("method", method),
                ),
                Vec::new(),
            );
        }
        _ => {
            return (
                ResponseEnvelope::failure(
                    request.request_id,
                    ApiError::new("method_not_found", "The requested daemon method is unknown")
                        .with_detail("method", request.method),
                ),
                Vec::new(),
            );
        }
    };

    match result {
        Ok(value) => (
            ResponseEnvelope::success(request.request_id, value),
            Vec::new(),
        ),
        Err(error) => (
            ResponseEnvelope::failure(
                request.request_id,
                ApiError::new("internal_error", "The daemon could not encode its response")
                    .with_detail("reason", error.to_string()),
            ),
            Vec::new(),
        ),
    }
}

async fn handle_git_status(
    request_id: RequestId,
    payload: Value,
    state: &ServerState,
) -> (ResponseEnvelope<Value>, Vec<FanoutEvent>) {
    let payload = match decode_payload::<GitStatusRequest>(payload) {
        Ok(payload) => payload,
        Err(error) => return (invalid_payload(request_id, &error), Vec::new()),
    };
    match crate::git_inspection::status(&state.storage, state.git.as_ref(), payload).await {
        Ok(response) => (encode_success(request_id, response), Vec::new()),
        Err(error) => (ResponseEnvelope::failure(request_id, error), Vec::new()),
    }
}

async fn handle_git_diff(
    request_id: RequestId,
    payload: Value,
    state: &ServerState,
) -> (ResponseEnvelope<Value>, Vec<FanoutEvent>) {
    let payload = match decode_payload::<GitDiffRequest>(payload) {
        Ok(payload) => payload,
        Err(error) => return (invalid_payload(request_id, &error), Vec::new()),
    };
    match crate::git_inspection::diff(&state.storage, state.git.as_ref(), payload).await {
        Ok(response) => (encode_success(request_id, response), Vec::new()),
        Err(error) => (ResponseEnvelope::failure(request_id, error), Vec::new()),
    }
}

fn reject_bad_envelope(request: &RequestEnvelope<Value>) -> Option<ResponseEnvelope<Value>> {
    if request.kind != EnvelopeKind::Request {
        return Some(ResponseEnvelope::failure(
            request.request_id,
            ApiError::new(
                "invalid_envelope_kind",
                "Daemon commands must use the request envelope kind",
            )
            .with_detail("receivedKind", format!("{:?}", request.kind).to_lowercase())
            .with_detail("expectedKind", "request"),
        ));
    }
    if request.version != PROTOCOL_V1 {
        return Some(ResponseEnvelope::failure(
            request.request_id,
            ApiError::new(
                "unsupported_protocol_version",
                "The requested IPC protocol version is not supported",
            )
            .with_action("Update CLI Master so the desktop and daemon versions match")
            .with_detail("receivedVersion", request.version)
            .with_detail("supportedVersion", PROTOCOL_V1),
        ));
    }
    None
}

fn handle_subscribe(
    request_id: RequestId,
    payload: Value,
    state: &ServerState,
    client: &ClientHandle,
) -> (ResponseEnvelope<Value>, Vec<FanoutEvent>) {
    let payload = match decode_payload::<SessionSubscribeRequest>(payload) {
        Ok(payload) => payload,
        Err(error) => return (invalid_payload(request_id, &error), Vec::new()),
    };
    match state
        .events
        .subscribe(client, payload.session_id, payload.cursor)
    {
        Ok(outcome) => (encode_success(request_id, EmptyResponse {}), outcome.replay),
        Err(SubscribeError::SessionNotFound) => (
            ResponseEnvelope::failure(
                request_id,
                ApiError::new("session_not_found", "The session has no live event stream")
                    .with_action("Create and start the session before subscribing")
                    .with_detail("sessionId", payload.session_id.to_string()),
            ),
            Vec::new(),
        ),
    }
}

fn diagnostics_payload(state: &ServerState) -> DiagnosticsResponse {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path != Path::new("/"));
    let mut response = DiagnosticsResponse {
        daemon_version: state.hello.daemon_version.clone(),
        protocol_version: PROTOCOL_V1,
        schema_version: state.schema_version,
        daemon_instance_id: state.hello.instance_id,
        data_path: sanitize_diagnostic_path(state.config.data_directory(), home.as_deref()),
        runtime_path: sanitize_diagnostic_path(state.config.runtime_directory(), home.as_deref()),
        log_path: sanitize_diagnostic_path(
            state.config.log_directory(),
            home.as_deref(),
        ),
        effective_path: Vec::new(),
        recent_issues: state.events.diagnostics().recent(),
        export_text: String::new(),
    };
    response.refresh_export_text();
    response
}

fn sanitize_diagnostic_path(path: &Path, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return path.to_path_buf();
    };
    let Ok(relative) = path.strip_prefix(home) else {
        return path.to_path_buf();
    };
    if relative.as_os_str().is_empty() {
        PathBuf::from("~")
    } else {
        PathBuf::from("~").join(relative)
    }
}

fn encode_success<T: serde::Serialize>(request_id: RequestId, data: T) -> ResponseEnvelope<Value> {
    match serde_json::to_value(data) {
        Ok(value) => ResponseEnvelope::success(request_id, value),
        Err(error) => ResponseEnvelope::failure(
            request_id,
            ApiError::new("internal_error", "The daemon could not encode its response")
                .with_detail("reason", error.to_string()),
        ),
    }
}

fn decode_payload<T: DeserializeOwned>(payload: Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(payload)
}

fn invalid_payload(request_id: RequestId, error: &serde_json::Error) -> ResponseEnvelope<Value> {
    ResponseEnvelope::failure(
        request_id,
        ApiError::new(
            "invalid_payload",
            "Request payload does not match the method contract",
        )
        .with_detail("reason", error.to_string()),
    )
}

struct RequestFailure {
    request_id: Option<RequestId>,
    error: Box<ApiError>,
}

fn decode_request(bytes: &[u8]) -> Result<RequestEnvelope<Value>, RequestFailure> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| RequestFailure {
        request_id: None,
        error: Box::new(
            ApiError::new("invalid_json", "Request frame is not valid JSON")
                .with_detail("reason", error.to_string()),
        ),
    })?;
    let request_id = value
        .get("requestId")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse().ok());

    serde_json::from_value(value).map_err(|error| RequestFailure {
        request_id,
        error: Box::new(
            ApiError::new(
                "invalid_request",
                "Request does not match the IPC envelope schema",
            )
            .with_detail("reason", error.to_string()),
        ),
    })
}

#[cfg(test)]
mod tests {
    use cli_master_core::wire::HelloResponse;
    use cli_master_core::{DaemonInstanceId, EnvelopeKind, PROTOCOL_V1, RequestEnvelope};
    use cli_master_storage::Storage;
    use serde_json::json;

    use super::*;
    use crate::{DaemonConfig, EventBus};

    #[tokio::test]
    async fn dispatch_rejects_non_request_kind() {
        let request_id = RequestId::new();
        let events = EventBus::new(crate::DiagnosticLog::default());
        let client = events.connect_client();
        let (response, replay) = dispatch(
            RequestEnvelope {
                kind: EnvelopeKind::Event,
                version: PROTOCOL_V1,
                request_id,
                method: "system.hello".to_owned(),
                payload: json!({}),
            },
            &ServerState {
                hello: HelloResponse {
                    protocol_version: PROTOCOL_V1,
                    daemon_version: "test".to_owned(),
                    instance_id: DaemonInstanceId::new(),
                },
                schema_version: 1,
                config: DaemonConfig::from_paths("/tmp/data", "/tmp/run"),
                events,
                storage: Storage::open_in_memory_migrated().expect("test storage should migrate"),
                git: None,
            },
            &client,
        )
        .await;

        assert!(replay.is_empty());
        match response.payload {
            cli_master_core::ResponsePayload::Error { error } => {
                assert_eq!(error.code, "invalid_envelope_kind");
            }
            cli_master_core::ResponsePayload::Success { .. } => panic!("wrong kind must fail"),
        }
    }
}
