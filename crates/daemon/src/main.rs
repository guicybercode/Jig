use std::error::Error;
use std::io;

use cli_master_daemon::{Daemon, DaemonConfig};
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::fmt::writer::MakeWriterExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let config = DaemonConfig::discover()?;
    config.prepare_private_directories()?;
    let log_file = config.open_structured_log()?;
    tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::INFO)
        .with_target(true)
        .with_writer(io::stderr.and(std::sync::Mutex::new(log_file)))
        .try_init()?;

    let daemon = Daemon::bind(config)?;
    let cancellation = CancellationToken::new();
    let signal_cancellation = cancellation.clone();
    let signal_task = tokio::spawn(async move {
        if let Err(error) = wait_for_shutdown_signal().await {
            error!(%error, "could not install daemon shutdown signal handler");
        }
        signal_cancellation.cancel();
    });

    let result = daemon.run(cancellation).await;
    signal_task.abort();
    info!("cli-masterd exiting");
    result.map_err(Into::into)
}

async fn wait_for_shutdown_signal() -> Result<(), io::Error> {
    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result,
        _ = terminate.recv() => Ok(()),
    }
}
