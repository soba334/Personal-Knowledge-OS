use personal_knowledge_os::{AppState, build_app, config::Config, services::worker};
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .json()
        .init();

    let config = Config::from_env()?;
    let bind_addr = config.bind_addr;
    let worker_enabled = config.worker_enabled;
    let state = AppState::from_config(config).await?;

    if worker_enabled {
        let worker_state = state.clone();
        tokio::spawn(async move {
            if let Err(err) = worker::run(worker_state).await {
                error!(error = %err, "background worker stopped");
            }
        });
    }

    let app = build_app(state);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    info!(%bind_addr, "Personal Knowledge OS listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
