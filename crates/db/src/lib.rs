mod bootstrap;
mod config;
mod migrate;

pub mod conversation;
pub mod message;

pub use bootstrap::{
    bootstrap_database,
    ensure_database,
    ensure_role,
};
pub use config::DbConfig;
pub use migrate::run_migrations;

use std::time::Duration;

use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    ConnectOptions,
    PgPool,
};

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("invalid database identifier: {0}")]
    InvalidIdentifier(String),
    #[error("invalid environment variable {name}: {value}")]
    InvalidEnv {
        name: &'static str,
        value: String,
    },
    #[error("cannot reach database at {host}:{port} — is it running?")]
    Unreachable {
        host: String,
        port: u16,
        #[source]
        source: sqlx::Error,
    },
}

pub async fn connect(
    options: PgConnectOptions,
) -> Result<Db, DbError> {
    let host = options.get_host().to_string();
    let port = options.get_port();

    let options = options.disable_statement_logging();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
        .map_err(|e| DbError::Unreachable {
            host,
            port,
            source: e,
        })?;

    Ok(Db { pool })
}
