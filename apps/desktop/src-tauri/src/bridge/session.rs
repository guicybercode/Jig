use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cli_master_core::wire::{HelloResponse, method};
use cli_master_core::{
    EnvelopeKind, EventEnvelope, PROTOCOL_V1, RequestEnvelope, RequestId, ResponseEnvelope,
    ResponsePayload,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::error::BridgeError;
use super::frame::{FrameDecoder, encode_frame};
use super::log;
use super::relay::BridgeRelay;

const READ_BUFFER_BYTES: usize = 8192;

struct Pending {
    method: String,
    request_id: RequestId,
    tx: oneshot::Sender<Result<ResponseEnvelope<Value>, BridgeError>>,
}

/// One multiplexed Unix-socket session after TCP-style connect.
///
/// Requests are correlated by `requestId`. The reader task owns incoming
/// frames so responses may complete out of order relative to writes.
pub struct ConnectedClient {
    writer: tokio::sync::Mutex<OwnedWriteHalf>,
    pending: Arc<Mutex<HashMap<RequestId, Pending>>>,
    closed: watch::Sender<bool>,
    reader: JoinHandle<()>,
}

impl ConnectedClient {
    /// Connects to `socket` and starts the reader task.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError::Unavailable`] when the connect times out or fails.
    #[cfg(test)]
    pub async fn connect(
        socket: &std::path::Path,
        relay: Arc<dyn BridgeRelay>,
        connect_timeout: Duration,
    ) -> Result<Arc<Self>, BridgeError> {
        let stream = timeout(connect_timeout, UnixStream::connect(socket))
            .await
            .map_err(|_| BridgeError::Unavailable {
                detail: "timed out connecting to the daemon socket".to_owned(),
            })?
            .map_err(|_| BridgeError::Unavailable {
                detail: "could not connect to the daemon socket".to_owned(),
            })?;
        Ok(Self::from_stream(stream, relay))
    }

    pub(crate) fn from_stream(stream: UnixStream, relay: Arc<dyn BridgeRelay>) -> Arc<Self> {
        let (reader, writer) = stream.into_split();
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let (closed, _) = watch::channel(false);
        let reader_pending = Arc::clone(&pending);
        let reader_closed = closed.clone();
        let reader = tokio::spawn(read_loop(reader, reader_pending, relay, reader_closed));
        Arc::new(Self {
            writer: tokio::sync::Mutex::new(writer),
            pending,
            closed,
            reader,
        })
    }

    /// Completes `system.hello` and rejects a mismatched protocol version.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError::Incompatible`] when the daemon's protocol version
    /// is not [`PROTOCOL_V1`], and other variants when the handshake cannot
    /// complete.
    pub async fn handshake(
        self: &Arc<Self>,
        handshake_timeout: Duration,
    ) -> Result<HelloResponse, BridgeError> {
        let response = self
            .invoke(
                method::SYSTEM_HELLO.to_owned(),
                json!({}),
                handshake_timeout,
            )
            .await?;
        match response.payload {
            ResponsePayload::Success { data } => {
                let hello: HelloResponse =
                    serde_json::from_value(data).map_err(|_| BridgeError::Protocol {
                        detail: "system.hello response was not a hello payload".to_owned(),
                    })?;
                if hello.protocol_version != PROTOCOL_V1 {
                    return Err(BridgeError::Incompatible {
                        protocol_version: Some(hello.protocol_version),
                        daemon_version: Some(hello.daemon_version.clone()),
                        detail: "daemon protocol version is not 1".to_owned(),
                    });
                }
                Ok(hello)
            }
            ResponsePayload::Error { error } if error.code == "unsupported_protocol_version" => {
                Err(BridgeError::Incompatible {
                    protocol_version: None,
                    daemon_version: None,
                    detail: "daemon rejected the handshake protocol version".to_owned(),
                })
            }
            ResponsePayload::Error { error } => {
                Err(BridgeError::Unavailable { detail: error.code })
            }
        }
    }

    /// Writes one request envelope and waits for the correlated response.
    ///
    /// # Errors
    ///
    /// Returns a transport [`BridgeError`] when the request cannot be framed,
    /// written, or matched before `timeout` elapses.
    pub async fn invoke(
        self: &Arc<Self>,
        method: String,
        payload: Value,
        request_timeout: Duration,
    ) -> Result<ResponseEnvelope<Value>, BridgeError> {
        if *self.closed.borrow() {
            return Err(BridgeError::Disconnected {
                method,
                request_id: RequestId::new(),
            });
        }
        let envelope = RequestEnvelope::v1(method.clone(), payload);
        let request_id = envelope.request_id;
        let encoded = serde_json::to_vec(&envelope).map_err(|_| BridgeError::Protocol {
            detail: "could not encode request envelope".to_owned(),
        })?;
        let frame = encode_frame(&encoded)?;
        let frame_bytes = frame.len();
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            pending.insert(
                request_id,
                Pending {
                    method: method.clone(),
                    request_id,
                    tx,
                },
            );
        }
        if self.write_all(&frame).await.is_err() {
            self.take_pending(request_id);
            return Err(BridgeError::Disconnected { method, request_id });
        }
        log::request_sent(&method, request_id, frame_bytes);
        match timeout(request_timeout, rx).await {
            Ok(Ok(result)) => {
                let error_code = result.as_ref().ok().and_then(|response| {
                    if let ResponsePayload::Error { error } = &response.payload {
                        Some(error.code.as_str())
                    } else {
                        None
                    }
                });
                log::request_finished(&method, request_id, error_code);
                result
            }
            Ok(Err(_)) => Err(BridgeError::Disconnected { method, request_id }),
            Err(_) => {
                self.take_pending(request_id);
                Err(BridgeError::Timeout { method, request_id })
            }
        }
    }

    /// Completes when the reader task observes EOF or a framing error.
    pub async fn wait_closed(&self) {
        let mut rx = self.closed.subscribe();
        while !*rx.borrow() {
            if rx.changed().await.is_err() {
                break;
            }
        }
    }

    async fn write_all(&self, frame: &[u8]) -> Result<(), std::io::Error> {
        let mut writer = self.writer.lock().await;
        writer.write_all(frame).await?;
        writer.flush().await
    }

    fn take_pending(&self, request_id: RequestId) -> Option<Pending> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&request_id)
    }
}

impl Drop for ConnectedClient {
    fn drop(&mut self) {
        self.reader.abort();
        fail_all(&self.pending, &|pending| BridgeError::Disconnected {
            method: pending.method.clone(),
            request_id: pending.request_id,
        });
        let _ = self.closed.send(true);
    }
}

async fn read_loop(
    mut reader: OwnedReadHalf,
    pending: Arc<Mutex<HashMap<RequestId, Pending>>>,
    relay: Arc<dyn BridgeRelay>,
    closed: watch::Sender<bool>,
) {
    let mut decoder = FrameDecoder::new();
    let mut buffer = vec![0_u8; READ_BUFFER_BYTES];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(count) => match decoder.push(&buffer[..count]) {
                Ok(frames) => {
                    for frame in frames {
                        dispatch_frame(&frame, &pending, relay.as_ref());
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "closing daemon connection after invalid frame");
                    break;
                }
            },
            Err(error) => {
                tracing::warn!(%error, "daemon socket read failed");
                break;
            }
        }
    }
    fail_all(&pending, &|pending| BridgeError::Disconnected {
        method: pending.method.clone(),
        request_id: pending.request_id,
    });
    let _ = closed.send(true);
}

fn dispatch_frame(
    frame: &[u8],
    pending: &Mutex<HashMap<RequestId, Pending>>,
    relay: &dyn BridgeRelay,
) {
    let value: Value = if let Ok(value) = serde_json::from_slice(frame) {
        value
    } else {
        tracing::warn!(
            frame_bytes = frame.len(),
            "dropping undecodable daemon frame"
        );
        return;
    };
    match value.get("kind").and_then(Value::as_str) {
        Some("event") => {
            if let Ok(envelope) = serde_json::from_value::<EventEnvelope<Value>>(value) {
                relay.event(envelope);
            } else {
                tracing::warn!("dropping invalid daemon event envelope");
            }
        }
        Some("response") => match serde_json::from_value::<ResponseEnvelope<Value>>(value) {
            Ok(response) if response.kind == EnvelopeKind::Response => {
                complete_pending(pending, response);
            }
            Ok(_) | Err(_) => tracing::warn!("dropping invalid daemon response envelope"),
        },
        _ => tracing::warn!("dropping daemon frame with unknown kind"),
    }
}

fn complete_pending(
    pending: &Mutex<HashMap<RequestId, Pending>>,
    response: ResponseEnvelope<Value>,
) {
    let request_id = response.request_id;
    let Some(pending) = pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&request_id)
    else {
        tracing::debug!(request_id = %request_id, "ignoring unmatched daemon response");
        return;
    };
    let _ = pending.tx.send(Ok(response));
}

fn fail_all(pending: &Mutex<HashMap<RequestId, Pending>>, error: &dyn Fn(&Pending) -> BridgeError) {
    let drained: Vec<Pending> = pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .drain()
        .map(|(_, pending)| pending)
        .collect();
    for pending in drained {
        let error = error(&pending);
        let _ = pending.tx.send(Err(error));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use cli_master_core::{EnvelopeKind, PROTOCOL_V1, ResponseEnvelope, ResponsePayload};
    use serde_json::json;
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixListener;

    use super::ConnectedClient;
    use crate::bridge::error::BridgeError;
    use crate::bridge::frame::encode_frame;
    use crate::bridge::relay::{BridgeRelay, NoopRelay, RecordingRelay};
    use crate::bridge::test_support::{
        FrameReader, read_request, temp_socket, write_event, write_hello, write_response,
    };

    async fn connect_pair(
        listener: &UnixListener,
        socket: &std::path::Path,
        relay: Arc<dyn BridgeRelay>,
    ) -> (tokio::net::UnixStream, Arc<ConnectedClient>) {
        let connect = ConnectedClient::connect(socket, relay, Duration::from_secs(1));
        let accept = listener.accept();
        let (client, server) = tokio::join!(connect, accept);
        (
            server.expect("accept").0,
            client.expect("client should connect"),
        )
    }

    #[tokio::test]
    async fn out_of_order_responses_match_request_ids() {
        let (_dir, socket) = temp_socket();
        let listener = UnixListener::bind(&socket).expect("bind");
        let (mut server, client) = connect_pair(&listener, &socket, Arc::new(NoopRelay)).await;

        let first = tokio::spawn({
            let client = Arc::clone(&client);
            async move {
                client
                    .invoke("state.snapshot".into(), json!({}), Duration::from_secs(1))
                    .await
            }
        });
        let second = tokio::spawn({
            let client = Arc::clone(&client);
            async move {
                client
                    .invoke("diagnostics.get".into(), json!({}), Duration::from_secs(1))
                    .await
            }
        });

        let (request_a, request_b) = {
            let mut reader = FrameReader::new(&mut server);
            let request_a = reader.read_request().await;
            let request_b = reader.read_request().await;
            (request_a, request_b)
        };
        let (snapshot, diagnostics) = if request_a.method == "state.snapshot" {
            (request_a, request_b)
        } else {
            (request_b, request_a)
        };
        write_response(&mut server, diagnostics.request_id, json!({"order": "b"})).await;
        write_response(&mut server, snapshot.request_id, json!({"order": "a"})).await;

        let response_snapshot = first.await.expect("join").expect("snapshot");
        let response_diagnostics = second.await.expect("join").expect("diagnostics");
        let ResponsePayload::Success {
            data: data_snapshot,
        } = response_snapshot.payload
        else {
            panic!("snapshot should succeed");
        };
        let ResponsePayload::Success {
            data: data_diagnostics,
        } = response_diagnostics.payload
        else {
            panic!("diagnostics should succeed");
        };
        assert_eq!(data_snapshot, json!({"order": "a"}));
        assert_eq!(data_diagnostics, json!({"order": "b"}));
        assert_eq!(response_snapshot.request_id, snapshot.request_id);
        assert_eq!(response_diagnostics.request_id, diagnostics.request_id);
    }

    #[tokio::test]
    async fn timeout_when_the_server_stays_silent() {
        let (_dir, socket) = temp_socket();
        let listener = UnixListener::bind(&socket).expect("bind");
        let (mut server, client) = connect_pair(&listener, &socket, Arc::new(NoopRelay)).await;
        let invoke = tokio::spawn({
            let client = Arc::clone(&client);
            async move {
                client
                    .invoke(
                        "state.snapshot".into(),
                        json!({}),
                        Duration::from_millis(80),
                    )
                    .await
            }
        });
        let _request = read_request(&mut server).await;
        let error = invoke.await.expect("join").expect_err("should time out");
        assert!(matches!(error, BridgeError::Timeout { .. }));
    }

    #[tokio::test]
    async fn disconnect_fails_pending_requests() {
        let (_dir, socket) = temp_socket();
        let listener = UnixListener::bind(&socket).expect("bind");
        let (server, client) = connect_pair(&listener, &socket, Arc::new(NoopRelay)).await;
        let invoke = tokio::spawn({
            let client = Arc::clone(&client);
            async move {
                client
                    .invoke("state.snapshot".into(), json!({}), Duration::from_secs(1))
                    .await
            }
        });
        drop(server);
        let error = invoke.await.expect("join").expect_err("should disconnect");
        assert!(matches!(error, BridgeError::Disconnected { .. }));
    }

    #[tokio::test]
    async fn partial_response_frame_is_assembled() {
        let (_dir, socket) = temp_socket();
        let listener = UnixListener::bind(&socket).expect("bind");
        let (mut server, client) = connect_pair(&listener, &socket, Arc::new(NoopRelay)).await;
        let invoke = tokio::spawn({
            let client = Arc::clone(&client);
            async move {
                client
                    .invoke("state.snapshot".into(), json!({}), Duration::from_secs(1))
                    .await
            }
        });
        let request = read_request(&mut server).await;
        let envelope = ResponseEnvelope::success(request.request_id, json!({"ok": true}));
        let encoded = serde_json::to_vec(&envelope).expect("json");
        let frame = encode_frame(&encoded).expect("frame");
        server.write_all(&frame[..3]).await.expect("partial header");
        tokio::time::sleep(Duration::from_millis(20)).await;
        server.write_all(&frame[3..]).await.expect("remainder");
        let response = invoke.await.expect("join").expect("assembled");
        assert!(matches!(response.payload, ResponsePayload::Success { .. }));
    }

    #[tokio::test]
    async fn handshake_accepts_protocol_v1() {
        let (_dir, socket) = temp_socket();
        let listener = UnixListener::bind(&socket).expect("bind");
        let (mut server, client) = connect_pair(&listener, &socket, Arc::new(NoopRelay)).await;
        let handshake = tokio::spawn({
            let client = Arc::clone(&client);
            async move { client.handshake(Duration::from_secs(1)).await }
        });
        let request = read_request(&mut server).await;
        assert_eq!(request.method, "system.hello");
        write_hello(&mut server, request.request_id, PROTOCOL_V1).await;
        let hello = handshake.await.expect("join").expect("hello");
        assert_eq!(hello.protocol_version, PROTOCOL_V1);
    }

    #[tokio::test]
    async fn handshake_rejects_incompatible_protocol() {
        let (_dir, socket) = temp_socket();
        let listener = UnixListener::bind(&socket).expect("bind");
        let (mut server, client) = connect_pair(&listener, &socket, Arc::new(NoopRelay)).await;
        let handshake = tokio::spawn({
            let client = Arc::clone(&client);
            async move { client.handshake(Duration::from_secs(1)).await }
        });
        let request = read_request(&mut server).await;
        write_hello(&mut server, request.request_id, PROTOCOL_V1 + 1).await;
        let error = handshake.await.expect("join").expect_err("incompatible");
        assert!(matches!(error, BridgeError::Incompatible { .. }));
    }

    #[tokio::test]
    async fn events_are_relayed_without_matching_a_request() {
        let (_dir, socket) = temp_socket();
        let listener = UnixListener::bind(&socket).expect("bind");
        let relay = Arc::new(RecordingRelay::default());
        let (mut server, _client) = connect_pair(&listener, &socket, Arc::clone(&relay) as _).await;
        write_event(&mut server, "session.updated", json!({"sessionId": "s"})).await;
        let events = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let events = relay.events();
                if !events.is_empty() {
                    return events;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("event should be relayed");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "session.updated");
        assert_eq!(events[0].kind, EnvelopeKind::Event);
    }
}
