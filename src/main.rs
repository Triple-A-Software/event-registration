use std::{env, net::SocketAddr};

use axum::{
    Router,
    routing::{get, post},
};
use event_registration::{
    AppState,
    api::{admin, dashboard, public},
    create_cms_db, create_db,
};
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

    let state = AppState::new(create_db().await, create_cms_db().await);

    let router = Router::new()
        .nest_service("/ui", ServeDir::new("ui/dist"))
        .route("/helper/event_registration", post(public::registration_helper))
        .route("/helper/event_spots", post(public::spots_helper))
        .route("/event-registration/submit", post(public::submit))
        .route("/event-registration/cancel", get(public::cancel_page).post(public::cancel))
        .route("/api/events", get(admin::events))
        .route("/api/events/{id}", get(admin::event).put(admin::save_event))
        .route(
            "/api/registrations",
            get(admin::registrations).put(admin::set_status).delete(admin::delete),
        )
        .route("/api/registrations/export", get(admin::export))
        .route("/api/settings", get(admin::settings).put(admin::save_settings))
        .route("/dashboard/upcoming", get(dashboard::upcoming))
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
