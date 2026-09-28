//! Zebra Store server: loads configuration, wires dependencies and runs the HTTP API
//! and background worker.

mod cli;
mod serving;
mod settings;

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use zs_app::config::Config;

use cli::{Cli, Command};

/// Upper bound for handling one request (original `WriteTimeout = 60s`).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cfg = settings::load(&cli.config)?;
    init_tracing(&cfg);
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(cfg).await,
        Command::Admin(cmd) => cli::run_admin(cfg, cmd).await,
        Command::Backup { output } => cli::run_backup(cfg, output).await,
    }
}

fn init_tracing(cfg: &Config) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cfg.log.level));
    // Live QA I-24: no ANSI colour escapes when stdout is not a terminal (`docker logs`).
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(ansi_enabled(
            std::io::IsTerminal::is_terminal(&std::io::stdout()),
            std::env::var("NO_COLOR").ok().as_deref(),
        ));
    if cfg.log.json {
        builder.json().init();
    } else {
        builder.init();
    }
}

/// Colour escapes only on an interactive terminal, and never with `NO_COLOR` set.
fn ansi_enabled(stdout_is_tty: bool, no_color: Option<&str>) -> bool {
    stdout_is_tty && no_color.is_none_or(str::is_empty)
}

async fn serve(cfg: Config) -> anyhow::Result<()> {
    cfg.validate().context("invalid configuration")?;
    let db = zs_infra::db::connect(&cfg.database)
        .await
        .context("connect database")?;
    zs_infra::db::sync_schema(&db)
        .await
        .context("sync database schema")?;

    let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
    let services = zs_infra::wire::services(&ctx);
    zs_infra::wire::bootstrap(&ctx, &services)
        .await
        .context("bootstrap")?;

    // Background worker (database-backed job queue).
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let worker = zs_infra::queue::Worker::new(
        db.clone(),
        zs_infra::wire::jobs(&ctx, &services),
        cfg.queue.concurrency,
        Duration::from_millis(cfg.queue.poll_interval_ms),
    );
    let worker_handle = tokio::spawn(worker.run(stop_rx));

    let app = zs_api::build(zs_api::AppState::new(services, cfg.clone()));

    let router = app
        .router
        .nest_service(
            "/uploads",
            zs_api::routes::content::uploads::uploads_router(&cfg.upload.dir),
        )
        .layer(cors(&cfg))
        // MISC-01: bound every request (the original sets Read/WriteTimeout 30s/60s);
        // connection-level limits are applied by `http::serve`.
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            http::StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ));

    let addr: SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port)
        .parse()
        .context("invalid server.host/server.port")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("bind listener")?;
    tracing::info!(%addr, "http server listening");
    serving::serve(
        listener,
        router,
        serving::Limits::ORIGINAL,
        shutdown_signal(),
    )
    .await;
    let _ = stop_tx.send(true);
    if let Err(error) = worker_handle.await {
        tracing::error!(%error, "job worker panicked");
    }
    Ok(())
}

fn cors(cfg: &Config) -> CorsLayer {
    let origins = &cfg.cors.allowed_origins;
    let layer = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .max_age(Duration::from_secs(cfg.cors.max_age));
    if origins.iter().any(|o| o == "*") {
        layer.allow_origin(Any)
    } else {
        let list = origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect::<Vec<http::HeaderValue>>();
        layer.allow_origin(AllowOrigin::list(list))
    }
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to listen for shutdown signal");
    }
    tracing::info!("shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;

    // QA-A24 (live QA I-24): `docker logs` (no TTY) gets plain text.
    #[test]
    fn qa_a24_no_ansi_without_terminal() {
        assert!(!ansi_enabled(false, None));
        assert!(!ansi_enabled(true, Some("1")));
        assert!(ansi_enabled(true, None));
        assert!(ansi_enabled(true, Some("")));
    }
}
