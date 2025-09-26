// SPDX-License-Identifier: MIT OR Apache-2.0

use sqlx::migrate::{MigrateDatabase, Migrator};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Sqlite, migrate};
use thiserror::Error;
use tokio::sync::Mutex;

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

pub struct SqlitePoolBuilder {
    url: String,
    max_connections: u32,
    run_migrations: bool,
    create_database: bool,
}

impl Default for SqlitePoolBuilder {
    fn default() -> Self {
        Self {
            url: "sqlite::memory:".into(),
            max_connections: 16,
            create_database: true,
            run_migrations: true,
        }
    }
}

impl SqlitePoolBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn random_memory_url(mut self) -> Self {
        // Combining Rust tests with in-memory databases can lead to unsound behaviour, this
        // "workaround" assigns every temporary database a different, random name and keeps them
        // isolated from other tests.
        //
        // See related issue: https://github.com/launchbadge/sqlx/issues/2510
        self.url = format!(
            "sqlite://dbmem{}?mode=memory&cache=private",
            rand::random::<u32>()
        );
        self
    }

    pub fn database_url(mut self, url: &str) -> Self {
        self.url = url.to_string();
        self
    }

    pub fn max_connections(mut self, max_connections: u32) -> Self {
        self.max_connections = max_connections;
        self
    }

    pub fn create_database(mut self, create_database: bool) -> Self {
        self.create_database = create_database;
        self
    }

    pub fn run_default_migrations(mut self, run_migrations: bool) -> Self {
        self.run_migrations = run_migrations;
        self
    }

    pub async fn build(self) -> Result<SqlitePool, SqliteError> {
        if self.create_database {
            create_database(&self.url).await?;
        }

        let pool: sqlx::SqlitePool = SqlitePoolOptions::new()
            .max_connections(self.max_connections)
            .connect(&self.url)
            .await?;

        if self.run_migrations {
            run_pending_migrations(&pool).await?;
        }

        Ok(SqlitePool::new(pool))
    }
}

pub type Transaction = sqlx::Transaction<'static, Sqlite>;

/// SQLite connection pool with transaction provider.
///
/// This struct can be cloned and used in multiple places in the application. Every cloned instance
/// will re-use the same connection pool but create a new transaction provider instance. This
/// allows users to theoretically run multiple transactions.
///
/// Please note that SQLite strictly serializes transactions with writes. This abstraction thus
/// doesn't give us any real performance benefits for parallelization but allows instead designing
/// isolated and atomic transactions.
///
/// Please note that this interface needs to be used with care: Transactions are managed per single
/// `SqlitePool` instance (and not shared across them, reference-counted etc.) and need to be
/// explicitly started _before_ any queries can take place, otherwise errors will occur which
/// should be understood as implementation bugs.
pub struct SqlitePool {
    tx: Mutex<Option<Transaction>>,
    pool: sqlx::SqlitePool,
}

impl Clone for SqlitePool {
    fn clone(&self) -> Self {
        Self {
            // Cloning the pool gives us another handle for it but creates a completly new
            // transaction state only managed by this instance.
            tx: Mutex::new(None),
            pool: self.pool.clone(),
        }
    }
}

impl SqlitePool {
    pub(crate) fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            tx: Mutex::new(None),
            pool,
        }
    }

    /// Begins a transaction, otherwise an error is returned.
    ///
    /// Any process can now start using the `tx` method to execute writes within this transaction
    /// or perform uncommitted "dirty" reads on it.
    pub async fn begin(&self) -> Result<(), SqliteError> {
        let mut tx_ref = self.tx.lock().await;

        // @TODO: Learn a bit how this used and then decide if we want to have a semaphore here.
        // This will then not fail and instead await "queing-up" the next transaction.
        if tx_ref.is_some() {
            return Err(SqliteError::TransactionPending);
        }

        let tx = self.pool.begin().await?;
        tx_ref.replace(tx);

        Ok(())
    }

    /// Rolls back the transaction and with that all pending changes.
    ///
    /// This will return an error if no transaction was given in the first place.
    pub async fn rollback(&self) -> Result<(), SqliteError> {
        match self.tx.lock().await.take() {
            Some(tx) => Ok(tx.rollback().await?),
            None => Err(SqliteError::TransactionMissing),
        }
    }

    /// Commits the transaction.
    ///
    /// This will return an error if no transaction was given in the first place.
    pub async fn commit(&self) -> Result<(), SqliteError> {
        match self.tx.lock().await.take() {
            Some(tx) => Ok(tx.commit().await?),
            None => Err(SqliteError::TransactionMissing),
        }
    }

    /// Execute SQL query within transaction.
    ///
    /// This method will return an error when no transaction is currently given. Make sure to call
    /// `begin` before.
    ///
    /// If the query fails the transaction is automatically rolled back.
    pub async fn tx<F, R>(&self, f: F) -> Result<R, SqliteError>
    where
        F: AsyncFnOnce(&mut Transaction) -> Result<R, SqliteError>,
    {
        let mut tx_ref = self.tx.lock().await;
        let tx = tx_ref.as_mut().ok_or(SqliteError::TransactionMissing)?;

        match f(tx).await {
            Ok(result) => Ok(result),
            Err(err) => {
                // Something went wrong, we need to roll back and abort here.
                self.rollback().await?;
                Err(err)
            }
        }
    }

    /// Execute SQL query directly.
    pub async fn execute<F, R>(&self, f: F) -> Result<R, SqliteError>
    where
        F: AsyncFnOnce(&sqlx::SqlitePool) -> Result<R, SqliteError>,
    {
        f(&self.pool).await
    }
}

#[derive(Debug, Error)]
pub enum SqliteError {
    /// We can't begin a new transaction as one is already pending.
    #[error("can't begin a new transaction as one is currently pending")]
    TransactionPending,

    /// This is a critical error as it indicates that something is wrong with our implementation:
    /// Queries using transactions, commits or rollbacks can only ever occur if a transaction was
    /// started _before_.
    #[error("tried to interact with inexistant transaction")]
    TransactionMissing,

    /// SQLite database and connection error.
    #[error(transparent)]
    Sqlite(#[from] sqlx::Error),

    /// SQL table schema migration error.
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

#[cfg(test)]
mod tests {
    use sqlx::{Executor, query, query_as};

    use crate::sqlite::{SqliteError, SqlitePoolBuilder};

    #[tokio::test]
    async fn transaction_provider() {
        let pool = SqlitePoolBuilder::new()
            .run_default_migrations(false)
            .random_memory_url()
            .build()
            .await
            .unwrap();

        // Executing with an in-existant transaction should throw error.
        assert!(matches!(
            pool.tx(async |_| Ok(())).await,
            Err(SqliteError::TransactionMissing)
        ));

        // Commiting or rolling back an in-existant transaction should fail.
        assert!(matches!(
            pool.commit().await,
            Err(SqliteError::TransactionMissing)
        ));
        assert!(matches!(
            pool.rollback().await,
            Err(SqliteError::TransactionMissing)
        ));

        // Starting a new transaction should work.
        assert!(pool.begin().await.is_ok());

        // .. attempting to start a second one should fail.
        assert!(matches!(
            pool.begin().await,
            Err(SqliteError::TransactionPending)
        ));

        // Using the transaction should work without failure.
        assert!(pool.tx(async |_| Ok(())).await.is_ok());

        // Committing should work as well.
        assert!(pool.commit().await.is_ok());

        // .. and now running a transaction should fail again.
        assert!(matches!(
            pool.tx(async |_| Ok(())).await,
            Err(SqliteError::TransactionMissing)
        ));
    }

    #[tokio::test]
    async fn isolated_transaction_providers() {
        let pool_1 = SqlitePoolBuilder::new()
            .run_default_migrations(false)
            .random_memory_url()
            .build()
            .await
            .unwrap();

        // Cloning will re-use the connection pool but _not_ the transaction provider.
        let pool_2 = pool_1.clone();

        assert!(pool_1.begin().await.is_ok());
        assert!(pool_2.begin().await.is_ok());

        use std::ptr::addr_of;

        let tx_addr_1a = pool_1
            .tx(async |tx| Ok(format!("{:?}", addr_of!(tx))))
            .await
            .unwrap();
        let tx_addr_1b = pool_1
            .tx(async |tx| Ok(format!("{:?}", addr_of!(tx))))
            .await
            .unwrap();
        let tx_addr_2 = pool_2
            .tx(async |tx| Ok(format!("{:?}", addr_of!(tx))))
            .await
            .unwrap();

        assert_eq!(tx_addr_1a, tx_addr_1b);

        // @TODO: Why does this fail?
        // assert_ne!(tx_addr_2, tx_addr_1a);
    }

    #[tokio::test]
    async fn committed_and_uncommitted_reads() {
        let pool = SqlitePoolBuilder::new()
            .run_default_migrations(false)
            .max_connections(1)
            .random_memory_url()
            .build()
            .await
            .unwrap();

        let pool_2 = pool.clone();

        // Create test-table schema.
        pool.execute(async |pool| {
            pool.execute("CREATE TABLE test(x INTEGER)").await?;
            Ok(())
        })
        .await
        .unwrap();

        // ...

        pool.begin().await.unwrap();

        pool.tx(async |tx| {
            query("INSERT INTO test (x) VALUES (5)")
                .execute(&mut **tx)
                .await?;
            Ok(())
        })
        .await
        .unwrap();

        let result = pool
            .tx(async |tx| {
                let row: (i64,) = query_as("SELECT x FROM test").fetch_one(&mut **tx).await?;
                Ok(row.0)
            })
            .await
            .unwrap();
        assert_eq!(result, 5);

        // ...

        pool.commit().await.unwrap();

        pool_2.begin().await.unwrap();

        let result = pool_2
            .tx(async |tx| {
                let row: (i64,) = query_as("SELECT x FROM test").fetch_one(&mut **tx).await?;
                Ok(row.0)
            })
            .await
            .unwrap();
        assert_eq!(result, 5);
    }
}
