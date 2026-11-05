use std::{env, net::SocketAddr};

use axum::Router;
use event_registration::{AppState, create_cms_db, create_db};
use tokio::net::TcpListener;
use tower_http::{
    normalize_path::NormalizePathLayer,
    services::ServeDir,
    trace::{self, TraceLayer},
};
use tracing::Level;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).compact().init();

    let state = AppState {
        db: create_db().await,
        cms_db: create_cms_db().await,
    };

    // routes of plugin.json are added here as they get built, see DEVELOPMENT.md
    let router = Router::new()
        .nest_service("/ui", ServeDir::new("ui/dist"))
        .layer(NormalizePathLayer::trim_trailing_slash())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state);

    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("failed to bind port");
    println!("event-registration listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}
