use cnshell_team_relay::{
    AccountRegistrationMode, RelayStore, SmtpVerificationEmailSender, router_with_registration_mode,
};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let database_url = std::env::var("CNSHELL_RELAY_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://cnshell-team-relay.sqlite?mode=rwc".into());
    let bind = std::env::var("CNSHELL_RELAY_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let address: SocketAddr = bind.parse()?;
    if !address.ip().is_loopback()
        && std::env::var("CNSHELL_RELAY_BEHIND_TLS_PROXY").as_deref() != Ok("1")
    {
        return Err("refusing non-loopback bind without CNSHELL_RELAY_BEHIND_TLS_PROXY=1".into());
    }
    let registration = match SmtpVerificationEmailSender::from_env()? {
        Some(sender) => AccountRegistrationMode::RequireEmail(std::sync::Arc::new(sender)),
        None if address.ip().is_loopback()
            || std::env::var("CNSHELL_RELAY_ALLOW_UNVERIFIED_ACCOUNTS").as_deref() == Ok("1") =>
        {
            tracing::warn!(
                "SMTP is not configured; development registrations are trusted without email verification"
            );
            AccountRegistrationMode::TrustedLocal
        }
        None => {
            return Err(
                "refusing production relay startup without CNSHELL_RELAY_SMTP_HOST and CNSHELL_RELAY_SMTP_FROM"
                    .into(),
            );
        }
    };
    let store = RelayStore::open(&database_url).await?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(address = %listener.local_addr()?, "CNshell team relay listening");
    let (router, shutdown) = router_with_registration_mode(store, registration);
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            shutdown.shutdown();
        })
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to register Ctrl-C shutdown signal");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "failed to register SIGTERM shutdown signal");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("CNshell team relay shutting down");
}
