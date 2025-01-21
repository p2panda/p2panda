// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite persistent storage.
use anyhow::{Error, Result};
use sqlx::migrate::MigrateDatabase;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Sqlite;
//use sqlx::migrate;

/// Re-export of SQLite connection pool type.
pub type Pool = SqlitePool;

/// SQLite-based persistent store.
#[derive(Clone, Debug)]
pub struct SqliteStore {
    pub(crate) pool: Pool,
}

impl SqliteStore {
    /// Create a new `SqliteStore` using the provided db `Pool`.
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

/// Create the database if it doesn't already exist.
pub async fn create_database(url: &str) -> Result<()> {
    if !Sqlite::database_exists(url).await? {
        Sqlite::create_database(url).await?;
    }

    Ok(())
}

/// Create a connection pool.
pub async fn connection_pool(url: &str, max_connections: u32) -> Result<Pool, Error> {
    let pool: Pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect(url)
        .await?;

    Ok(pool)
}

/*
/// Run any pending database migrations from inside the application.
pub async fn run_pending_migrations(pool: &Pool) -> Result<()> {
    migrate!().run(pool).await?;
    Ok(())
}
*/
