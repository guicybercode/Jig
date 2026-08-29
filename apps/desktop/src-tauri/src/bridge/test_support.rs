use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;

use cli_master_core::wire::HelloResponse;
use cli_master_core::{
    DaemonInstanceId, EventEnvelope, RequestEnvelope, RequestId, ResponseEnvelope,
};
use serde_json::Value;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use super::frame::{FrameDecoder, encode_frame};

pub fn temp_socket() -> (TempDir, PathBuf) {
    let directory = TempDir::new().expect("temporary directory");
    let socket = directory.path().join("daemon.sock");
    (directory, socket)
}

/// Length-delimited reader that keeps leftover bytes between frames.
pub struct FrameReader<'a> {
    stream: &'a mut UnixStream,
    decoder: FrameDecoder,
    ready: VecDeque<Vec<u8>>,
}

impl<'a> FrameReader<'a> {
    pub fn new(stream: &'a mut UnixStream) -> Self {
        Self {
            stream,
            decoder: FrameDecoder::new(),
            ready: VecDeque::new(),
        }
    }

    pub async fn read_request(&mut self) -> RequestEnvelope<Value> {
        let bytes = self.read_frame().await;
        serde_json::from_slice(&bytes).expect("request envelope")
    }

    async fn read_frame(&mut self) -> Vec<u8> {
        if let Some(frame) = self.ready.pop_front() {
            return frame;
        }
        let mut buffer = vec![0_u8; 4096];
        loop {
            let count = tokio::time::timeout(Duration::from_secs(1), self.stream.read(&mut buffer))
                .await
                .expect("read should not time out")
                .expect("read");
            assert_ne!(count, 0, "server closed before a request frame arrived");
            for frame in self.decoder.push(&buffer[..count]).expect("decode") {
                self.ready.push_back(frame);
            }
            if let Some(frame) = self.ready.pop_front() {
                return frame;
            }
        }
    }
}

pub async fn read_request(stream: &mut UnixStream) -> RequestEnvelope<Value> {
    FrameReader::new(stream).read_request().await
}

pub async fn write_response(stream: &mut UnixStream, request_id: RequestId, data: Value) {
    write_json(stream, &ResponseEnvelope::success(request_id, data)).await;
}

pub async fn write_hello(stream: &mut UnixStream, request_id: RequestId, protocol_version: u16) {
    let hello = HelloResponse {
        protocol_version,
        daemon_version: "0.1.0".to_owned(),
        instance_id: DaemonInstanceId::new(),
    };
    write_response(
        stream,
        request_id,
        serde_json::to_value(hello).expect("hello json"),
    )
    .await;
}

pub async fn write_event(stream: &mut UnixStream, event: &str, payload: Value) {
    write_json(stream, &EventEnvelope::v1(event, 1, payload)).await;
}

pub async fn write_json<T: serde::Serialize>(stream: &mut UnixStream, value: &T) {
    let encoded = serde_json::to_vec(value).expect("json");
    let frame = encode_frame(&encoded).expect("frame");
    stream.write_all(&frame).await.expect("write frame");
}
