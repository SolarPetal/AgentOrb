mod api;
mod security;
mod session_store;

use agent_orb_core::config::{loopback_socket_addr, Config};

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

    let addr = loopback_socket_addr(&config.daemon.host, config.daemon.port)
        .expect("agent_orbd refuses to bind a non-loopback daemon host");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind agent_orbd to configured local address");

    tracing::info!(%addr, token_path = %security::token_path(&config_dir).display(), "agent_orbd listening");
    axum::serve(listener, app).await.expect("serve agent_orbd");
}
