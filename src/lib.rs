use std::env;

use sqlx::PgPool;

pub mod availability;
pub mod database;
pub mod model;

#[derive(Clone)]
pub struct AppState {
    /// the plugin's own database (event configs, registrations, settings)
    pub db: PgPool,
    /// the Neleto CMS database, read for event titles, dates and status
    pub cms_db: PgPool,
}

/// Connects to the plugin's own database and runs migrations.
pub async fn create_db() -> PgPool {
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = PgPool::connect(&db_url)
        .await
        .expect("failed to connect to plugin database");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("failed to run migrations");
    db
}

pub async fn create_cms_db() -> PgPool {
    let db_url = env::var("CMS_DATABASE_URL").expect("CMS_DATABASE_URL must be set");
    PgPool::connect(&db_url)
        .await
        .expect("failed to connect to CMS database")
}
