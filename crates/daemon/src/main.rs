use std::error::Error;

use cli_master_daemon::{Daemon, DaemonConfig};
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::INFO)
        .with_target(true)
        .try_init()?;

    let config = DaemonConfig::discover()?;
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

async fn wait_for_shutdown_signal() -> Result<(), std::io::Error> {
    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result,
        _ = terminate.recv() => Ok(()),
    }
}
