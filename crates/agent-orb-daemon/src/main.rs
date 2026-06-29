mod api;
mod security;
mod session_store;

use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config_dir = security::default_config_dir();
    let token = security::load_or_create_token(&config_dir).expect("load or create local token");
    let app = api::build_app(api::AppState::new(token));

    let addr = SocketAddr::from(([127, 0, 0, 1], 17321));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind agent_orbd to 127.0.0.1:17321");

    tracing::info!(%addr, token_path = %security::token_path(&config_dir).display(), "agent_orbd listening");
    axum::serve(listener, app).await.expect("serve agent_orbd");
}
