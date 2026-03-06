use sqlx::postgres::PgConnectOptions;

use crate::DbError;

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub admin_user: String,
    pub admin_password: String,
    pub admin_database: String,
    pub database_name: String,
    pub database_user: String,
    pub database_password: String,
    pub admin_database_url: Option<String>,
    pub database_url: Option<String>,
}

impl DbConfig {
    pub fn from_env() -> Result<Self, DbError> {
        Ok(Self {
            host: env_or_default("CLAWKSON_DB_HOST", "127.0.0.1"),
            port: env_or_default("CLAWKSON_DB_PORT", "55435")
                .parse()
                .map_err(|_| DbError::InvalidEnv {
                    name: "CLAWKSON_DB_PORT",
                    value: std::env::var("CLAWKSON_DB_PORT")
                        .unwrap_or_else(|_| "55435".to_string()),
                })?,
            admin_user: env_or_default("CLAWKSON_DB_ADMIN_USER", "postgres"),
            admin_password: env_or_default(
                "CLAWKSON_DB_ADMIN_PASSWORD",
                "change-me-superuser-password",
            ),
            admin_database: env_or_default("CLAWKSON_DB_ADMIN_DATABASE", "postgres"),
            database_name: env_or_default("CLAWKSON_DB_NAME", "clawkson"),
            database_user: env_or_default("CLAWKSON_DB_USER", "clawkson"),
            database_password: env_or_default(
                "CLAWKSON_DB_PASSWORD",
                "change-me-clawkson-password",
            ),
            admin_database_url: std::env::var("CLAWKSON_ADMIN_DATABASE_URL").ok(),
            database_url: std::env::var("CLAWKSON_DATABASE_URL").ok(),
        })
    }

    pub fn admin_connect_options(&self) -> Result<PgConnectOptions, DbError> {
        if let Some(url) = &self.admin_database_url {
            return parse_connect_options("CLAWKSON_ADMIN_DATABASE_URL", url);
        }

        Ok(PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .username(&self.admin_user)
            .password(&self.admin_password)
            .database(&self.admin_database))
    }

    pub fn app_connect_options(&self) -> Result<PgConnectOptions, DbError> {
        Ok(PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .username(&self.database_user)
            .password(&self.database_password)
            .database(&self.database_name))
    }

    pub fn migration_connect_options(&self) -> Result<PgConnectOptions, DbError> {
        if let Some(url) = &self.database_url {
            return parse_connect_options("CLAWKSON_DATABASE_URL", url);
        }

        Ok(PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .username(&self.admin_user)
            .password(&self.admin_password)
            .database(&self.database_name))
    }
}

fn env_or_default(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn parse_connect_options(
    name: &'static str,
    value: &str,
) -> Result<PgConnectOptions, DbError> {
    value.parse().map_err(|_| DbError::InvalidEnv {
        name,
        value: value.to_string(),
    })
}
