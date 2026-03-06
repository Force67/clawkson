use crate::{Db, DbError};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

pub async fn run_migrations(db: &Db) -> Result<(), DbError> {
    MIGRATOR.run(db.pool()).await?;
    Ok(())
}
