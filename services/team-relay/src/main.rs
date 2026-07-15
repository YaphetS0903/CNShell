use cnshell_team_relay::{RelayStore, router};
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
    let store = RelayStore::open(&database_url).await?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(address = %listener.local_addr()?, "CNshell team relay listening");
    axum::serve(listener, router(store)).await?;
    Ok(())
}
