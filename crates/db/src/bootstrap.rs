use crate::{Db, DbConfig, DbError};

pub async fn bootstrap_database(
    db: &Db,
    config: &DbConfig,
) -> Result<(), DbError> {
    ensure_role(
        db,
        &config.database_user,
        &config.database_password,
    )
    .await?;

    ensure_database(
        db,
        &config.database_name,
        &config.database_user,
    )
    .await?;

    Ok(())
}

pub async fn ensure_role(
    db: &Db,
    role_name: &str,
    password: &str,
) -> Result<(), DbError> {
    let quoted_role_name = quote_identifier(role_name)?;
    let quoted_password = quote_literal(password);
    let role_exists = sqlx::query(
        "SELECT 1
         FROM pg_catalog.pg_roles
         WHERE rolname = $1",
    )
    .bind(role_name)
    .fetch_optional(db.pool())
    .await?
    .is_some();

    let statement = if role_exists {
        format!("ALTER ROLE {quoted_role_name} WITH LOGIN PASSWORD {quoted_password}")
    } else {
        format!("CREATE ROLE {quoted_role_name} WITH LOGIN PASSWORD {quoted_password}")
    };

    sqlx::query(&statement).execute(db.pool()).await?;

    Ok(())
}

pub async fn ensure_database(
    db: &Db,
    database_name: &str,
    owner_name: &str,
) -> Result<(), DbError> {
    let quoted_database_name = quote_identifier(database_name)?;
    let quoted_owner_name = quote_identifier(owner_name)?;
    let database_exists = sqlx::query(
        "SELECT 1
         FROM pg_catalog.pg_database
         WHERE datname = $1",
    )
    .bind(database_name)
    .fetch_optional(db.pool())
    .await?
    .is_some();

    if database_exists {
        let statement = format!(
            "ALTER DATABASE {quoted_database_name} OWNER TO {quoted_owner_name}"
        );
        sqlx::query(&statement).execute(db.pool()).await?;
    } else {
        let statement = format!(
            "CREATE DATABASE {quoted_database_name} OWNER {quoted_owner_name}"
        );
        sqlx::query(&statement).execute(db.pool()).await?;
    }

    Ok(())
}

fn quote_identifier(identifier: &str) -> Result<String, DbError> {
    if !is_valid_identifier(identifier) {
        return Err(DbError::InvalidIdentifier(identifier.to_string()));
    }

    Ok(format!("\"{identifier}\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn is_valid_identifier(identifier: &str) -> bool {
    let mut chars = identifier.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }

    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
