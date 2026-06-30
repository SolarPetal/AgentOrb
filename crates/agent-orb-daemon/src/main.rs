mod api;
mod security;
mod session_store;

use std::net::SocketAddr;

use agent_orb_core::config::Config;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config_dir = security::default_config_dir();
    let config = Config::load_from_dir_or_default(&config_dir);
    let token = security::load_or_create_token(&config_dir).expect("load or create local token");
    let app = api::build_app(api::AppState::with_completed_hold_seconds(
        token,
        config.behavior.completed_hold_seconds,
    ));

    let addr: SocketAddr = format!("{}:{}", config.daemon.host, config.daemon.port)
        .parse()
        .expect("daemon host and port should form a valid socket address");
    assert!(
        addr.ip().is_loopback(),
        "agent_orbd refuses to bind non-loopback address for MVP safety"
    );
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind agent_orbd to configured local address");

    tracing::info!(%addr, token_path = %security::token_path(&config_dir).display(), "agent_orbd listening");
    axum::serve(listener, app).await.expect("serve agent_orbd");
}
