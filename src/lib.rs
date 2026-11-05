use std::{env, sync::Arc, time::Duration};

use minijinja::{Environment, path_loader};
use sqlx::PgPool;

pub mod antispam;
pub mod api;
pub mod availability;
pub mod database;
pub mod email;
pub mod lang;
pub mod model;
pub mod render;
pub mod utils;

#[derive(Clone)]
pub struct AppState {
    /// the plugin's own database (event configs, registrations, settings)
    pub db: PgPool,
    /// the Neleto CMS database, read for event titles, dates and status
    pub cms_db: PgPool,
    /// dashboard card templates from `./templates`
    pub env: Arc<Environment<'static>>,
    pub rate_limiter: Arc<antispam::RateLimiter>,
}

impl AppState {
    pub fn new(db: PgPool, cms_db: PgPool) -> Self {
        let mut env = Environment::new();
        env.set_loader(path_loader("./templates"));
        Self {
            db,
            cms_db,
            env: Arc::new(env),
            // 10 submits per 10 minutes and ip: room for a family registering one by one
            rate_limiter: Arc::new(antispam::RateLimiter::new(Duration::from_secs(600), 10)),
        }
    }
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
