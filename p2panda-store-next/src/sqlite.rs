// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::EncodeError;
use sqlx::migrate::{MigrateDatabase, Migrator};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Sqlite, migrate};
use thiserror::Error;

/// Create SQLite database if it doesn't already exist.
pub async fn create_database(url: &str) -> Result<(), SqliteError> {
    if !Sqlite::database_exists(url).await? {
        Sqlite::create_database(url).await?
    }
    Ok(())
}

/// Drop SQLite database if it exists.
pub async fn drop_database(url: &str) -> Result<(), SqliteError> {
    if Sqlite::database_exists(url).await? {
        Sqlite::drop_database(url).await?
    }
    Ok(())
}

/// Create SQLite connection pool.
pub async fn connection_pool(
    url: &str,
    max_connections: u32,
) -> Result<sqlx::SqlitePool, SqliteError> {
    let pool: sqlx::SqlitePool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect(url)
        .await?;
    Ok(pool)
}

/// Get migrations from folder without running them.
pub fn migrations() -> Migrator {
    migrate!()
}

/// Run any pending database migrations from inside the application.
pub async fn run_pending_migrations(pool: &sqlx::SqlitePool) -> Result<(), SqliteError> {
    migrations().run(pool).await?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum SqliteError {
    /// SQLite database and connection error.
    #[error(transparent)]
    Sqlite(#[from] sqlx::Error),

    /// SQL table schema migration error.
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
}
