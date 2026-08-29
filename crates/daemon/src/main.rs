//! `cli-masterd` Unix socket server.

use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::thread;

use cli_master_core::{RequestEnvelope, RequestId, ResponseEnvelope};
use cli_master_daemon::{AppPaths, Daemon, read_frame, write_frame};
use serde_json::Value;

fn main() {
    let paths = AppPaths::for_user().unwrap_or_else(|error| {
        eprintln!("cli-masterd: cannot resolve paths: {error}");
        std::process::exit(1);
    });
    if paths.socket.exists() {
        let _ = std::fs::remove_file(&paths.socket);
    }
    let daemon = match Daemon::open(paths.clone()) {
        Ok(daemon) => Arc::new(daemon),
        Err(error) => {
            eprintln!("cli-masterd: {error}");
            std::process::exit(1);
        }
    };
    let listener = UnixListener::bind(&paths.socket).unwrap_or_else(|error| {
        eprintln!(
            "cli-masterd: cannot bind {}: {error}",
            paths.socket.display()
        );
        std::process::exit(1);
    });
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let daemon = Arc::clone(&daemon);
                thread::spawn(move || serve(&daemon, stream));
            }
            Err(error) => eprintln!("cli-masterd: accept failed: {error}"),
        }
    }
}

fn serve(daemon: &Daemon, mut stream: UnixStream) {
    while let Ok(payload) = read_frame(&mut stream) {
        let request: RequestEnvelope<Value> = match serde_json::from_slice(&payload) {
            Ok(request) => request,
            Err(error) => {
                let response = ResponseEnvelope::<Value>::failure(
                    RequestId::new(),
                    cli_master_core::ApiError::new("INVALID_REQUEST", error.to_string()),
                );
                let _ = write_frame(&mut stream, &response);
                continue;
            }
        };
        if !request.is_supported() {
            let response = ResponseEnvelope::<Value>::failure(
                request.request_id,
                cli_master_core::ApiError::new(
                    "PROTOCOL_UNSUPPORTED",
                    "Unsupported protocol version",
                )
                .with_action("Upgrade CLI Master to a matching daemon"),
            );
            let _ = write_frame(&mut stream, &response);
            continue;
        }
        let response = match daemon.dispatch(&request.method, request.payload) {
            Ok(data) => ResponseEnvelope::success(request.request_id, data),
            Err(error) => ResponseEnvelope::failure(request.request_id, error),
        };
        if write_frame(&mut stream, &response).is_err() {
            break;
        }
    }
}
