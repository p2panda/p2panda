// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite database implementation with associated utility functions.
use std::sync::Arc;
use std::time::Duration;

use p2panda_core::cbor::EncodeError;
use sqlx::migrate::{MigrateDatabase, Migrator};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Sqlite, migrate};
use thiserror::Error;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

/// Creates the SQLite database if it doesn't already exist.
pub async fn create_database(url: &str) -> Result<(), SqliteError> {
    if !Sqlite::database_exists(url).await? {
        Sqlite::create_database(url).await?
    }
    Ok(())
}

/// Drops the SQLite database if it exists.
pub async fn drop_database(url: &str) -> Result<(), SqliteError> {
    if Sqlite::database_exists(url).await? {
        Sqlite::drop_database(url).await?
    }
    Ok(())
}

/// Creates the SQLite connection pool.
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

/// Gets migrations from folder without running them.
pub fn migrations() -> Migrator {
    migrate!()
}

/// Runs any pending database migrations from inside the application.
pub async fn run_pending_migrations(pool: &sqlx::SqlitePool) -> Result<(), SqliteError> {
    migrations().run(pool).await?;
    Ok(())
}

/// Builder for `SqliteStore`.
///
/// To create the database call `SqliteStoreBuilder::build()`.
///
/// By default, the builder configures an in-memory database with a maximum number of 16
/// connections. The database is created if it doesn't already exist and migrations are
/// automatically run on start-up.
pub struct SqliteStoreBuilder {
    url: String,
    min_connections: u32,
    max_connections: u32,
    idle_timeout: Option<Duration>,
    max_lifetime: Option<Duration>,
    run_migrations: bool,
    create_database: bool,
}

impl Default for SqliteStoreBuilder {
    fn default() -> Self {
        Self {
            url: "sqlite::memory:".into(),
            min_connections: 3,
            max_connections: 16,
            idle_timeout: Some(Duration::from_secs(10 * 60)),
            max_lifetime: Some(Duration::from_secs(30 * 60)),
            create_database: true,
            run_migrations: true,
        }
    }
}

impl SqliteStoreBuilder {
    /// Creates a new `SqliteStoreBuilder` using default configuration values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new in-memory `SqliteStoreBuilder` using recommended configuration values.
    ///
    /// The configuration values have been chosen to prevent the in-memory database being dropped
    /// when there are no active connections and the idle timeout or max lifetime limit is
    /// reached.
    pub fn memory() -> Self {
        Self::default()
            .random_memory_url()
            .min_connections(1)
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
    }

    /// Assigns a randomly-generated in-memory database URL with private cache.
    #[cfg(any(test, feature = "test_utils"))]
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

    /// Sets the database URL.
    ///
    /// If left unset, the database will use an ephemeral in-memory URL.
    pub fn database_url(mut self, url: &str) -> Self {
        self.url = url.to_string();
        self
    }

    /// Sets the minimum number of connections to be maintained by the database pool.
    ///
    /// If left unset, a minimum of 3 connections will be maintained.
    pub fn min_connections(mut self, min_connections: u32) -> Self {
        self.min_connections = min_connections;
        self
    }

    /// Sets the maximum number of connections to be maintained by the database pool.
    ///
    /// If left unset, a maximum of 16 connections will be maintained.
    pub fn max_connections(mut self, max_connections: u32) -> Self {
        self.max_connections = max_connections;
        self
    }

    /// Set a maximum idle duration for individual connections.
    ///
    /// Any connection that remains in the idle queue longer than this will be closed.
    ///
    /// For usage-based database server billing, this can be a cost saver.
    pub fn idle_timeout(mut self, timeout: impl Into<Option<Duration>>) -> Self {
        self.idle_timeout = timeout.into();
        self
    }

    /// Set the maximum lifetime of individual connections.
    ///
    /// Any connection with a lifetime greater than this will be closed.
    ///
    /// When set to `None`, all connections live until either reaped by [`idle_timeout`]
    /// or explicitly disconnected.
    ///
    /// Infinite connections are not recommended due to the unfortunate reality of memory/resource
    /// leaks on the database-side. It is better to retire connections periodically
    /// (even if only once daily) to allow the database the opportunity to clean up data structures
    /// (parse trees, query metadata caches, thread-local storage, etc.) that are associated with a
    /// session.
    pub fn max_lifetime(mut self, lifetime: impl Into<Option<Duration>>) -> Self {
        self.max_lifetime = lifetime.into();
        self
    }

    /// Creates the database if it doesn't already exist.
    ///
    /// If left unset, the database will be created by default.
    pub fn create_database(mut self, create_database: bool) -> Self {
        self.create_database = create_database;
        self
    }

    /// Sets whether pending migrations should be applied when the database is built.
    ///
    /// If left unset, the database will apply any pending migrations.
    pub fn run_default_migrations(mut self, run_migrations: bool) -> Self {
        self.run_migrations = run_migrations;
        self
    }

    /// Builds the `SqliteStore`.
    pub async fn build(self) -> Result<SqliteStore, SqliteError> {
        if self.create_database {
            create_database(&self.url).await?;
        }

        let pool: sqlx::SqlitePool = SqlitePoolOptions::new()
            .min_connections(self.min_connections)
            .max_connections(self.max_connections)
            .idle_timeout(self.idle_timeout)
            .max_lifetime(self.max_lifetime)
            .connect(&self.url)
            .await?;

        if self.run_migrations {
            run_pending_migrations(&pool).await?;
        }

        Ok(SqliteStore::new(pool))
    }
}

/// An in-progress database transaction.
pub type Transaction<'a> = sqlx::Transaction<'a, Sqlite>;

/// Sqlite connection pool.
pub type SqlitePool = sqlx::SqlitePool;

/// SQLite database with connection pool and transaction provider.
///
/// This struct can be cloned and used in multiple places in the application. Every cloned instance
/// will re-use the same connection pool and have access to the same transaction instance if one
/// was started. To guard against sharing transactions unknowingly across unrelated database
/// queries, a concept of a `TransactionPermit` was introduced which does not protect from misuse
/// but helps to make "holding" a transaction explicit.
///
/// Please note that SQLite strictly serializes transactions with _writes_ and will block any
/// parallel attempt to begin another one. Processes starting a transaction will acquire a
/// `TransactionPermit` and keep it until the transaction was committed or rolled back. If the
/// query only involves _reads_ it is recommended to not use transactions and use the `execute`
/// method directly as acquiring transactions will potentially block other processes to do work.
///
/// ## Design decisions
///
/// This storage API design was chosen to make the dynamics of the underlying SQLite database
/// explicit to avoid potentially introducing subtle bugs. Internally any process can access the
/// transaction object to do writes and (uncommitted) reads (see "Transaction I" in diagram). Care
/// is required when designing systems like that as it's still possible to allow concurrent
/// processes to read and write within the same transaction (for example one process could roll
/// back the transaction while the other one assumed it will be committed). Usually developers want
/// to design _writes_ to the database within a transaction if they need consistency and atomicity
/// guarantees. "Unrelated" queries _can_ be "pooled" in one transaction (for performance reasons
/// for example) if consistency is guaranteed by all involved processes and the underlying
/// data-model (see "Transaction II" in diagram).
///
/// ```text
/// Transaction I:
/// begin ---------------------> commit
///
/// Process I:
///       --> write --> read -->
///
///                                             Transaction II:
///                                             begin ----------------------> commit
///
///                                             Process II:
///                                                   --> write --> write -->
///
///                                             Process III:
///                                                   --> read --> write --->
/// ```
///
/// Another design decision is to not expose transactions to the high-level storage APIs (similar
/// to the "Repository Pattern"). Users of the storage methods like `get_operation` (in
/// `OperationStore`) etc. do _not_ need to explicity deal with transaction objects, as this is
/// handled internally now. Like this it is possible to separate the "logic" from the "storage"
/// layer and keep the code clean.
#[derive(Clone, Debug)]
pub struct SqliteStore {
    tx: Arc<Mutex<Option<Transaction<'static>>>>,
    pub(crate) pool: sqlx::SqlitePool,
    semaphore: Arc<Semaphore>,
}

impl SqliteStore {
    /// Creates a new `SqliteStore` using the provided connection pool.
    pub(crate) fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            tx: Arc::default(),
            pool,
            // SQLite only ever allows _one_ transaction at a time. This might be a repetition of
            // what sqlx and SQLite do under the hood, but we want to make this behaviour explicit
            // right from the beginning with this semaphore.
            semaphore: Arc::new(Semaphore::new(1)),
        }
    }

    /// Creates a new `SqliteStore` using the provided connection pool.
    pub fn from_pool(pool: sqlx::SqlitePool) -> Self {
        Self::new(pool)
    }

    /// Returns a reference to the connection pool.
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    /// Builds an in-memory SQLite database with a randomised name for testing purposes.
    #[cfg(any(test, feature = "test_utils"))]
    pub async fn temporary() -> Self {
        SqliteStoreBuilder::memory()
            .build()
            .await
            .expect("migrations succeeded")
    }

    /// Executes a SQL query within a transaction.
    ///
    /// This method will return an error when no transaction is currently given. Make sure to call
    /// `begin` before.
    ///
    /// If the query fails the user probably wants to roll back the transaction and free the
    /// permit. This is _not_ handled automatically.
    pub async fn tx<F, R>(&self, f: F) -> Result<R, SqliteError>
    where
        F: AsyncFnOnce(&mut Transaction) -> Result<R, SqliteError>,
    {
        let mut tx_ref = self.tx.lock().await;
        let tx = tx_ref.as_mut().ok_or(SqliteError::TransactionMissing)?;

        f(tx).await
    }

    /// Executes a SQL query directly.
    pub async fn execute<F, R>(&self, f: F) -> Result<R, SqliteError>
    where
        F: AsyncFnOnce(&sqlx::SqlitePool) -> Result<R, SqliteError>,
    {
        f(&self.pool).await
    }
}

impl crate::traits::Transaction for SqliteStore {
    type Error = SqliteError;

    type Permit = TransactionPermit;

    /// Begins a transaction.
    ///
    /// Transactions are strictly serialized, this is expressed in form of a `TransactionPermit`
    /// processes need to hold when acquiring access to a new transaction. Any concurrent process
    /// calling it will await here if there's already another process holding a permit, this will
    /// potentially "slow down" work and should be carefully used.
    ///
    /// Any process with a transaction can now start using the `tx` method to execute writes within
    /// this transaction or perform uncommitted "dirty" reads on it.
    ///
    /// It is usually not necessary to acquire a transaction when the logic only requires committed
    /// _reads_ to the database. Use `execute` instead.
    async fn begin(&self) -> Result<TransactionPermit, SqliteError> {
        // Acquire a permit from the semaphore, it will await if currently another process has the
        // permit. Here we enforce strict serialization of transactions (similar to what SQLite
        // does under the hood).
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("if semaphore is closed then the whole struct is gone as well");

        // Access the transaction object which we've placed behind a Mutex. This lock follows a
        // different logic and only makes sure that mutable access to it is exclusive _within_ a
        // process "holding" the transaction permit.
        let mut tx_ref = self.tx.lock().await;
        assert!(
            tx_ref.is_none(),
            "can't have an already existing transaction after an just-acquired permit"
        );
        let tx = self.pool.begin().await?;
        tx_ref.replace(tx);

        Ok(TransactionPermit::new(permit, self.tx.clone()))
    }

    /// Rolls back the transaction and with that all uncommitted changes.
    ///
    /// This takes the permit and frees it after the rollback has finished. Other processes can now
    /// begin new transactions.
    async fn rollback(&self, permit: TransactionPermit) -> Result<(), SqliteError> {
        let Some(tx) = self.tx.lock().await.take() else {
            panic!("can't have no transaction without dropping permit first")
        };

        let result = tx.rollback().await.map_err(SqliteError::Sqlite);

        // Always drop the permit, both on successful rollback and error. This will allow other
        // processes now to begin a new transaction and acquire the permit.
        permit.mark_committed_and_drop();

        result
    }

    /// Commits the transaction.
    ///
    /// This takes the permit and frees it after the commit has finished. Other processes can now
    /// begin new transactions.
    async fn commit(&self, permit: TransactionPermit) -> Result<(), SqliteError> {
        let Some(tx) = self.tx.lock().await.take() else {
            panic!("can't have no transaction without dropping permit first")
        };

        let result = tx.commit().await.map_err(SqliteError::Sqlite);

        // Always drop the permit, both on successful commit and error. This will allow other
        // processes now to begin a new transaction and acquire the permit.
        permit.mark_committed_and_drop();

        result
    }
}

/// Locked context marking the lifetime of a single transaction.
pub struct TransactionPermit {
    permit: Arc<OwnedSemaphorePermit>,
    tx: Arc<Mutex<Option<Transaction<'static>>>>,
    committed: bool,
}

impl TransactionPermit {
    /// Creates a new `TransactionPermit` using the given permit and transaction.
    pub(super) fn new(
        permit: OwnedSemaphorePermit,
        tx: Arc<Mutex<Option<Transaction<'static>>>>,
    ) -> Self {
        Self {
            permit: Arc::new(permit),
            tx,
            committed: false,
        }
    }

    /// Marks the transaction as committed and drops the permit.
    ///
    /// In the case that the permit was never used, whether due to an early return or error, the
    /// transaction is automatically rolled-back to prevent corrupted state.
    pub(super) fn mark_committed_and_drop(mut self) {
        self.committed = true;
        drop(self)
    }
}

impl Drop for TransactionPermit {
    fn drop(&mut self) {
        // If the permit was never used (due to an early return / error / etc.) we automatically
        // roll-back the transaction.
        if !self.committed {
            let permit = self.permit.clone();
            let tx = self.tx.clone();

            tokio::spawn(async move {
                if let Some(tx) = tx.lock().await.take() {
                    let _ = tx.rollback().await;
                }

                drop(permit); // Semaphore released only after rollback completes.
            });
        }
    }
}

/// Error when interacting with a SQLite store implementation.
#[derive(Debug, Error)]
pub enum SqliteError {
    /// This is a critical error as it indicates that something is wrong with the usage of this
    /// API: Queries using transactions can only ever occur if a transaction was started _before_.
    #[error("tried to interact with inexistant transaction")]
    TransactionMissing,

    /// SQLite database and connection error.
    #[error(transparent)]
    Sqlite(#[from] sqlx::Error),

    /// SQL table schema migration error.
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),

    /// An I/O error occurred while encoding bytes before storing them into the database. This is a
    /// critical error.
    #[error("failed encoding '{0}' value before storing to database: {1}")]
    Encode(String, EncodeError),

    /// Invalid, corrupted data was found in the database. This is a critical error.
    #[error("could not decode corrupted '{0}' value from database: {1}")]
    Decode(String, DecodeError),
}

/// Error decoding value retrieved from a store.
#[derive(Debug, Error)]
pub enum DecodeError {
    #[error(transparent)]
    DecodeCbor(#[from] p2panda_core::cbor::DecodeError),

    #[error(transparent)]
    Hash(#[from] p2panda_core::hash::HashError),

    #[error(transparent)]
    Topic(#[from] p2panda_core::topic::TopicError),

    #[error("parsing from string failed")]
    FromStr,
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use futures_test::task::noop_context;
    use sqlx::{Executor, query, query_as, query_scalar};
    use tokio::pin;

    use crate::sqlite::{SqliteError, SqliteStore};
    use crate::traits::Transaction;

    #[tokio::test]
    async fn transaction_provider() {
        let pool = SqliteStore::temporary().await;

        // Executing with an in-existant transaction should throw error.
        assert!(matches!(
            pool.tx(async |_| Ok(())).await,
            Err(SqliteError::TransactionMissing)
        ));

        // Starting a new transaction should work.
        let permit = pool.begin().await.expect("no error");

        // .. attempting to start a second one should make us wait.
        assert!(matches!(
            {
                let fut = pool.begin();
                let mut cx = noop_context();
                pin!(fut);
                fut.poll(&mut cx)
            },
            Poll::Pending
        ));

        // Using the transaction should work without failure.
        assert!(pool.tx(async |_| Ok(())).await.is_ok());

        // Committing should work as well.
        assert!(pool.commit(permit).await.is_ok());

        // .. and now running a transaction should fail.
        assert!(matches!(
            pool.tx(async |_| Ok(())).await,
            Err(SqliteError::TransactionMissing)
        ));
    }

    #[tokio::test]
    async fn early_permit_drop_causing_rollback() {
        let pool = SqliteStore::temporary().await;

        // Create test-table schema.
        pool.execute(async |pool| {
            pool.execute("CREATE TABLE test(x INTEGER)").await?;
            Ok(())
        })
        .await
        .unwrap();

        let permit = pool.begin().await.unwrap();

        pool.tx(async |tx| {
            query("INSERT INTO test (x) VALUES (10)")
                .execute(&mut **tx)
                .await?;
            Ok(())
        })
        .await
        .unwrap();

        // Permit was dropped prematurely without committing.
        drop(permit);

        // It is okay to start another permit.
        assert!(pool.begin().await.is_ok());

        // The data was not written as the transaction got rolled back.
        let count: i64 = pool
            .execute(async |pool| {
                query_scalar("SELECT COUNT(*) FROM test")
                    .fetch_one(pool)
                    .await
                    .map_err(SqliteError::Sqlite)
            })
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn serialized_transactions() {
        let pool_1 = SqliteStore::temporary().await;

        let pool_2 = pool_1.clone();

        // Create test-table schema.
        pool_1
            .execute(async |pool| {
                pool.execute("CREATE TABLE test(x INTEGER)").await?;
                Ok(())
            })
            .await
            .unwrap();

        // 1. Pool 1 acquires the permit to run a transaction.
        let permit_1 = pool_1.begin().await.unwrap();

        // .. parallely Pool 2 also tries to do some work.
        let handle = tokio::spawn(async move {
            // Try to acquire a permit, this will "block" for now as pool 1 already is doing
            // something and we need to wait.
            let permit_2 = pool_2.begin().await.unwrap();

            // 5. We should see now the previously change made by pool 1.
            let result = pool_2
                .tx(async |tx| {
                    let row: (i64,) = query_as("SELECT x FROM test").fetch_one(&mut **tx).await?;
                    Ok(row.0)
                })
                .await
                .unwrap();
            assert_eq!(result, 5);

            // 6. Change the value to something else.
            pool_2
                .tx(async |tx| {
                    query("INSERT INTO test (x) VALUES (10)")
                        .execute(&mut **tx)
                        .await?;
                    Ok(())
                })
                .await
                .unwrap();

            // 7. .. but abort the transaction and roll back.
            pool_2.rollback(permit_2).await.unwrap();

            // The value should still be the same as before.
            let result = pool_2
                .execute(async |pool| {
                    let row: (i64,) = query_as("SELECT x FROM test").fetch_one(pool).await?;
                    Ok(row.0)
                })
                .await
                .unwrap();
            assert_eq!(result, 5);
        });

        // 2. Pool 1 changes the value.
        pool_1
            .tx(async |tx| {
                query("INSERT INTO test (x) VALUES (5)")
                    .execute(&mut **tx)
                    .await?;
                Ok(())
            })
            .await
            .unwrap();

        // 3. Result is already 5 during "dirty read".
        let result = pool_1
            .tx(async |tx| {
                let row: (i64,) = query_as("SELECT x FROM test").fetch_one(&mut **tx).await?;
                Ok(row.0)
            })
            .await
            .unwrap();
        assert_eq!(result, 5);

        // 4. Commit the change to database and free permit. This will allow now pool_2 to read the
        //    changed value.
        pool_1.commit(permit_1).await.unwrap();

        // Result is still 5 after commit.
        let result = pool_1
            .execute(async |pool| {
                let row: (i64,) = query_as("SELECT x FROM test").fetch_one(pool).await?;
                Ok(row.0)
            })
            .await
            .unwrap();
        assert_eq!(result, 5);

        // Make sure we give pool 2 the time it needs to finish.
        handle.await.unwrap();
    }
}
