use std::error::Error;
use std::io;
use std::process::ExitCode;

use cli_master_core::{APPLICATION_VERSION, PROTOCOL_V1};
use cli_master_daemon::{Daemon, DaemonConfig, run_preflight};
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::fmt::writer::MakeWriterExt;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => exit_result(run_daemon()),
        ["--version" | "-V"] => {
            println!("cli-masterd {APPLICATION_VERSION} (protocol {PROTOCOL_V1})");
            ExitCode::SUCCESS
        }
        ["--preflight"] => match emit_preflight() {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::from(1),
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        ["--help" | "-h"] => {
            print_usage();
            ExitCode::SUCCESS
        }
        [argument, ..] => {
            eprintln!("unknown argument: {argument}");
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn exit_result(result: Result<(), Box<dyn Error + Send + Sync>>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn print_usage() {
    eprintln!(
        "Usage: cli-masterd [--preflight | --version | --help]\n\n  \
         no arguments   start the per-user session daemon\n  \
         --preflight    check directories, Git, and optional agent CLIs\n  \
         --version      print the application and protocol versions"
    );
}

fn emit_preflight() -> Result<bool, Box<dyn Error + Send + Sync>> {
    let report = run_preflight()?;
    serde_json::to_writer_pretty(io::stdout(), &report)?;
    println!();
    Ok(report.ok)
}

fn run_daemon() -> Result<(), Box<dyn Error + Send + Sync>> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run_daemon_async())
}

async fn run_daemon_async() -> Result<(), Box<dyn Error + Send + Sync>> {
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
