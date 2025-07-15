use ccsched::cli::*;
use ccsched::client::*;
use ccsched::server::start_server;
use ccsched_core::config::Config;
use clap::Parser;
use tracing::debug;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start(args) => {
            init_logging(true).await?;
            info!("Starting Claude Code Scheduler");

            let config = Config::with_overrides(
                Some(args.host),
                Some(args.port),
                Some(args.claude_path),
                args.env,
            )?;

            debug!("Configuration: {:?}", config);

            start_server(config).await?;
        }
        Commands::Submit(args) => {
            init_logging(false).await?;
            submit_task(args).await?;
        }
        Commands::Add(args) => {
            init_logging(false).await?;
            add_task(args).await?;
        }
        Commands::List(args) => {
            init_logging(false).await?;
            list_tasks(args).await?;
        }
        Commands::Show(args) => {
            init_logging(false).await?;
            show_task(args).await?;
        }
        Commands::Resume(args) => {
            init_logging(false).await?;
            resume_task(args).await?;
        }
        Commands::Delete(args) => {
            init_logging(false).await?;
            delete_task(args).await?;
        }
        Commands::Rename(args) => {
            init_logging(false).await?;
            rename_task(args).await?;
        }
        Commands::Edit(args) => {
            init_logging(false).await?;
            edit_task(args).await?;
        }
    }

    Ok(())
}

async fn init_logging(server_mode: bool) -> anyhow::Result<()> {
    use tracing_subscriber::fmt;
    use std::sync::OnceLock;

    if server_mode {
        // Only create logs directory and file logging for server mode
        std::fs::create_dir_all("./logs")?;

        // Create file appender for ccsched.log
        let file_appender = tracing_appender::rolling::never("./logs", "ccsched.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        
        // Store guard globally to keep it alive
        static GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();
        let _ = GUARD.set(guard);

        // Create layers
        let file_layer = fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false); // No colors in file

        let console_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true); // Colors for console

        // Initialize subscriber with both layers
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".into()),
            )
            .with(file_layer)
            .with(console_layer)
            .init();
    } else {
        // Client mode: only console logging
        let console_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true); // Colors for console

        // Initialize subscriber with only console layer
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".into()),
            )
            .with(console_layer)
            .init();
    }

    Ok(())
}