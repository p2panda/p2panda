// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite persistent storage.
use std::hash::{DefaultHasher, Hash as StdHash, Hasher};
use std::marker::PhantomData;

use sqlx::migrate;
use sqlx::migrate::{MigrateDatabase, MigrateError};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{Error as SqlxError, Sqlite, query, query_as};
use thiserror::Error;

use p2panda_core::cbor::{DecodeError, EncodeError, encode_cbor};
use p2panda_core::{Body, Extensions, Hash, Header, PublicKey, RawOperation};

use crate::sqlite::models::{LogHeightRow, OperationRow, RawOperationRow};
use crate::{LogId, LogStore, OperationStore};

#[derive(Debug, Error)]
pub enum SqliteStoreError {
    #[error("failed to encode operation extensions: {0}")]
    EncodingFailed(#[from] EncodeError),

    #[error("failed to decode operation extensions: {0}")]
    DecodingFailed(#[from] DecodeError),

    #[error("an error occurred with the sqlite database: {0}")]
    Database(#[from] SqlxError),
}

impl From<MigrateError> for SqliteStoreError {
    fn from(error: MigrateError) -> Self {
        Self::Database(SqlxError::Migrate(Box::new(error)))
    }
}

/// Re-export of SQLite connection pool type.
pub type Pool = SqlitePool;

/// SQLite-based persistent store.
#[derive(Clone, Debug)]
pub struct SqliteStore<L, E> {
    pub(crate) pool: Pool,
    _marker: PhantomData<(L, E)>,
}

impl<L, E> SqliteStore<L, E>
where
    L: LogId,
    E: Extensions,
{
    /// Create a new `SqliteStore` using the provided db `Pool`.
    pub fn new(pool: Pool) -> Self {
        Self {
            pool,
            _marker: PhantomData {},
        }
    }
}

/// Create the database if it doesn't already exist.
pub async fn create_database(url: &str) -> Result<(), SqliteStoreError> {
    if !Sqlite::database_exists(url).await? {
        Sqlite::create_database(url).await?
    }

    Ok(())
}

/// Drop the database if it exists.
pub async fn drop_database(url: &str) -> Result<(), SqliteStoreError> {
    if Sqlite::database_exists(url).await? {
        Sqlite::drop_database(url).await?
    }

    Ok(())
}

/// Create a connection pool.
pub async fn connection_pool(url: &str, max_connections: u32) -> Result<Pool, SqliteStoreError> {
    let pool: Pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect(url)
        .await?;

    Ok(pool)
}

/// Get migrations without running them
pub fn migrations() -> migrate::Migrator {
    migrate!()
}

/// Run any pending database migrations from inside the application.
pub async fn run_pending_migrations(pool: &Pool) -> Result<(), SqliteStoreError> {
    migrations().run(pool).await?;

    Ok(())
}

fn calculate_hash<T: StdHash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

impl<L, E> OperationStore<L, E> for SqliteStore<L, E>
where
    L: LogId + Send + Sync,
    E: Extensions + Send + Sync,
{
    type Error = SqliteStoreError;

    async fn insert_operation(
        &mut self,
        hash: Hash,
        header: &Header<E>,
        body: Option<&Body>,
        header_bytes: &[u8],
        log_id: &L,
    ) -> Result<bool, Self::Error> {
        query(
            "
            INSERT INTO
                operations_v1 (
                    hash,
                    log_id,
                    version,
                    public_key,
                    signature,
                    payload_size,
                    payload_hash,
                    timestamp,
                    seq_num,
                    backlink,
                    previous,
                    extensions,
                    body,
                    header_bytes
                )
            VALUES
                (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(hash.to_string())
        .bind(calculate_hash(log_id).to_string())
        .bind(header.version.to_string())
        .bind(header.public_key.to_hex())
        .bind(header.signature.map(|sig| sig.to_hex()))
        .bind(header.payload_size.to_string())
        .bind(header.payload_hash.map(|hash| hash.to_hex()))
        .bind(header.timestamp.to_string())
        .bind(header.seq_num.to_string())
        .bind(header.backlink.map(|backlink| backlink.to_hex()))
        .bind(
            header
                .previous
                .iter()
                .map(|previous| previous.to_hex())
                .collect::<Vec<String>>()
                .concat(),
        )
        .bind(
            header
                .extensions
                .as_ref()
                .map(|extensions| encode_cbor(extensions).expect("extenions are serializable")),
        )
        .bind(body.map(|body| body.to_bytes()))
        .bind(header_bytes)
        .execute(&self.pool)
        .await?;

        Ok(true)
    }

    async fn get_operation(
        &self,
        hash: Hash,
    ) -> Result<Option<(Header<E>, Option<Body>)>, Self::Error> {
        if let Some(operation) = query_as::<_, OperationRow>(
            "
            SELECT
                hash,
                log_id,
                version,
                public_key,
                signature,
                payload_size,
                payload_hash,
                timestamp,
                seq_num,
                backlink,
                previous,
                extensions,
                body,
                header_bytes
            FROM
                operations_v1
            WHERE
                hash = ?
            ",
        )
        .bind(hash.to_string())
        .fetch_optional(&self.pool)
        .await?
        {
            let body = operation.body.clone().map(|body| body.into());
            let header: Header<E> = operation.into();

            Ok(Some((header, body)))
        } else {
            Ok(None)
        }
    }

    async fn get_raw_operation(&self, hash: Hash) -> Result<Option<RawOperation>, Self::Error> {
        if let Some(operation) = query_as::<_, RawOperationRow>(
            "
            SELECT
                hash,
                body,
                header_bytes
            FROM
                operations_v1
            WHERE
                hash = ?
            ",
        )
        .bind(hash.to_string())
        .fetch_optional(&self.pool)
        .await?
        {
            let raw_operation = operation.into();

            Ok(Some(raw_operation))
        } else {
            Ok(None)
        }
    }

    async fn has_operation(&self, hash: Hash) -> Result<bool, Self::Error> {
        let exists = query(
            "
            SELECT
                1
            FROM
                operations_v1
            WHERE
                hash = ?
            ",
        )
        .bind(hash.to_string())
        .fetch_optional(&self.pool)
        .await?;

        Ok(exists.is_some())
    }

    async fn delete_operation(&mut self, hash: Hash) -> Result<bool, Self::Error> {
        let result = query(
            "
            DELETE
            FROM
                operations_v1
            WHERE
                hash = ?
            ",
        )
        .bind(hash.to_string())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_payload(&mut self, hash: Hash) -> Result<bool, Self::Error> {
        let result = query(
            "
            UPDATE
                operations_v1
            SET
                body = NULL
            WHERE
                operations_v1.hash = ?
            ",
        )
        .bind(hash.to_string())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

impl<L, E> LogStore<L, E> for SqliteStore<L, E>
where
    L: LogId + Send + Sync,
    E: Extensions + Send + Sync,
{
    type Error = SqliteStoreError;

    async fn get_log(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Result<Option<Vec<(Header<E>, Option<Body>)>>, Self::Error> {
        let operations = query_as::<_, OperationRow>(
            "
            SELECT
                hash,
                log_id,
                version,
                public_key,
                signature,
                payload_size,
                payload_hash,
                timestamp,
                seq_num,
                backlink,
                previous,
                extensions,
                body,
                header_bytes
            FROM
                operations_v1
            WHERE
                public_key = ?
                AND log_id = ?
                AND CAST(seq_num AS NUMERIC) >= CAST(? as NUMERIC)
            ORDER BY
                CAST(seq_num AS NUMERIC)
            ",
        )
        .bind(public_key.to_string())
        .bind(calculate_hash(log_id).to_string())
        .bind(from.unwrap_or(0).to_string())
        .fetch_all(&self.pool)
        .await?;

        let log: Vec<(Header<E>, Option<Body>)> = operations
            .into_iter()
            .map(|operation| {
                (
                    operation.clone().into(),
                    operation.body.map(|body| body.into()),
                )
            })
            .collect();

        if log.is_empty() {
            Ok(None)
        } else {
            Ok(Some(log))
        }
    }

    async fn get_raw_log(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Result<Option<Vec<RawOperation>>, Self::Error> {
        let operations = query_as::<_, RawOperationRow>(
            "
            SELECT
                hash,
                body,
                header_bytes
            FROM
                operations_v1
            WHERE
                public_key = ?
                AND log_id = ?
                AND CAST(seq_num AS NUMERIC) >= CAST(? as NUMERIC)
            ORDER BY
                CAST(seq_num AS NUMERIC)
            ",
        )
        .bind(public_key.to_string())
        .bind(calculate_hash(log_id).to_string())
        .bind(from.unwrap_or(0).to_string())
        .fetch_all(&self.pool)
        .await?;

        let log: Vec<RawOperation> = operations
            .into_iter()
            .map(|operation| operation.into())
            .collect();

        if log.is_empty() {
            Ok(None)
        } else {
            Ok(Some(log))
        }
    }

    async fn latest_operation(
        &self,
        public_key: &PublicKey,
        log_id: &L,
    ) -> Result<Option<(Header<E>, Option<Body>)>, Self::Error> {
        if let Some(operation) = query_as::<_, OperationRow>(
            "
            SELECT
                hash,
                log_id,
                version,
                public_key,
                signature,
                payload_size,
                payload_hash,
                timestamp,
                seq_num,
                backlink,
                previous,
                extensions,
                body,
                header_bytes
            FROM
                operations_v1
            WHERE
                public_key = ?
                AND log_id = ?
            ORDER BY
                CAST(seq_num AS NUMERIC) DESC LIMIT 1
            ",
        )
        .bind(public_key.to_string())
        .bind(calculate_hash(log_id).to_string())
        .fetch_optional(&self.pool)
        .await?
        {
            let body = operation.body.clone().map(|body| body.into());
            let header: Header<E> = operation.into();

            Ok(Some((header, body)))
        } else {
            Ok(None)
        }
    }

    async fn delete_operations(
        &mut self,
        public_key: &PublicKey,
        log_id: &L,
        before: u64,
    ) -> Result<bool, Self::Error> {
        let result = query(
            "
            DELETE
            FROM
                operations_v1
            WHERE
                public_key = ?
                AND log_id = ?
                AND CAST(seq_num AS NUMERIC) < CAST(? as NUMERIC)
            ",
        )
        .bind(public_key.to_string())
        .bind(calculate_hash(log_id).to_string())
        .bind(before.to_string())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_payloads(
        &mut self,
        public_key: &PublicKey,
        log_id: &L,
        from: u64,
        to: u64,
    ) -> Result<bool, Self::Error> {
        let result = query(
            "
            UPDATE
                operations_v1
            SET
                body = NULL
            WHERE
                public_key = ?
                AND log_id = ?
                AND CAST(seq_num AS NUMERIC) >= CAST(? as NUMERIC)
                AND CAST(seq_num AS NUMERIC) < CAST(? as NUMERIC)
            ",
        )
        .bind(public_key.to_string())
        .bind(calculate_hash(log_id).to_string())
        .bind(from.to_string())
        .bind(to.to_string())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_log_heights(&self, log_id: &L) -> Result<Vec<(PublicKey, u64)>, Self::Error> {
        let operations = query_as::<_, LogHeightRow>(
            "
            SELECT
                public_key,
                CAST(MAX(CAST(seq_num AS NUMERIC)) AS TEXT) as seq_num
            FROM
                operations_v1
            WHERE
                log_id = ?
            GROUP BY
                public_key
            ",
        )
        .bind(calculate_hash(log_id).to_string())
        .fetch_all(&self.pool)
        .await?;

        let log_heights: Vec<(PublicKey, u64)> = operations
            .into_iter()
            .map(|operation| operation.into())
            .collect();

        Ok(log_heights)
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::{Body, Hash, Header, PrivateKey};
    use serde::{Deserialize, Serialize};

    use crate::sqlite::test_utils::initialize_sqlite_db;
    use crate::{LogStore, OperationStore};

    use super::SqliteStore;

    fn create_operation(
        private_key: &PrivateKey,
        body: &Body,
        seq_num: u64,
        timestamp: u64,
        backlink: Option<Hash>,
    ) -> (Hash, Header<()>, Vec<u8>) {
        let mut header = Header {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp,
            seq_num,
            backlink,
            previous: vec![],
            extensions: None,
        };
        header.sign(private_key);
        let header_bytes = header.to_bytes();
        (header.hash(), header, header_bytes)
    }

    #[tokio::test]
    async fn default_sqlite_store() {
        let db_pool = initialize_sqlite_db().await;

        let mut store = SqliteStore::new(db_pool);
        let private_key = PrivateKey::new();

        let body = Body::new("hello!".as_bytes());
        let (hash, header, header_bytes) = create_operation(&private_key, &body, 0, 0, None);

        let inserted = store
            .insert_operation(hash, &header, Some(&body), &header_bytes, &0)
            .await
            .expect("no errors");

        assert!(inserted);
    }

    #[tokio::test]
    async fn generic_extensions_mem_store() {
        // Define our own custom extension type.
        #[derive(Clone, Debug, Default, Serialize, Deserialize)]
        struct MyExtension {}

        // Instantiate a database pool backed by an in-memory db.
        let db_pool = initialize_sqlite_db().await;

        // Construct a new store.
        let mut store = SqliteStore::new(db_pool);

        // Construct an operation using the custom extension.
        let private_key = PrivateKey::new();
        let body = Body::new("hello!".as_bytes());
        let mut header = Header {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: Some(MyExtension {}),
        };
        header.sign(&private_key);

        // Insert the operation into the store, the extension type is inferred.
        let inserted = store
            .insert_operation(header.hash(), &header, Some(&body), &header.to_bytes(), &0)
            .await
            .expect("no errors");
        assert!(inserted);
    }

    #[tokio::test]
    async fn insert_operation_with_unsigned_header() {
        let db_pool = initialize_sqlite_db().await;
        let mut store = SqliteStore::new(db_pool);
        let private_key = PrivateKey::new();

        // Create the first operation.
        let body = Body::new("hello!".as_bytes());
        let (hash, mut header, header_bytes) = create_operation(&private_key, &body, 0, 0, None);

        // Set signature to `None` for the sake of the test.
        header.signature = None;

        // Only insert the first operation into the store.
        let inserted = store
            .insert_operation(hash, &header, Some(&body), &header_bytes, &0)
            .await;

        // Ensure that the lack of a header signature returns an error.
        assert!(inserted.is_err());
        assert_eq!(
            format!("{}", inserted.unwrap_err()),
            "an error occurred with the sqlite database: error returned from database: (code: 1299) NOT NULL constraint failed: operations_v1.signature"
        );
    }

    #[tokio::test]
    async fn insert_get_operation() {
        let db_pool = initialize_sqlite_db().await;
        let mut store = SqliteStore::new(db_pool);
        let private_key = PrivateKey::new();

        // Create the first operation.
        let body = Body::new("hello!".as_bytes());
        let (hash, header, header_bytes) = create_operation(&private_key, &body, 0, 0, None);

        // Create the second operation.
        let body_2 = Body::new("buenas!".as_bytes());
        let (hash_2, _header_2, _header_bytes_2) =
            create_operation(&private_key, &body_2, 0, 0, None);

        // Only insert the first operation into the store.
        let inserted = store
            .insert_operation(hash, &header, Some(&body), &header_bytes, &0)
            .await
            .expect("no errors");
        assert!(inserted);
        // Ensure the store contains the first operation but not the second.
        assert!(store.has_operation(hash).await.expect("no error"));
        assert!(!store.has_operation(hash_2).await.expect("no error"));

        let (header_again, body_again) = store
            .get_operation(hash)
            .await
            .expect("no error")
            .expect("operation exist");

        // Ensure the hash of the created operation header matches that of the retrieved
        // header hash.
        assert_eq!(header.hash(), header_again.hash());
        // Ensure the body of the created operation matches that of the retrieved body.
        assert_eq!(Some(body.clone()), body_again);

        let (header_bytes_again, body_bytes_again) = store
            .get_raw_operation(hash)
            .await
            .expect("no error")
            .expect("operation exist");

        assert_eq!(header_bytes_again, header_bytes);
        assert_eq!(body_bytes_again, Some(body.to_bytes()));
    }

    #[tokio::test]
    async fn delete_operation() {
        let db_pool = initialize_sqlite_db().await;
        let mut store = SqliteStore::new(db_pool);
        let private_key = PrivateKey::new();

        let body = Body::new("hello!".as_bytes());
        let (hash, header, header_bytes) = create_operation(&private_key, &body, 0, 0, None);

        // Insert one operation.
        let inserted = store
            .insert_operation(hash, &header, Some(&body), &header_bytes, &0)
            .await
            .expect("no errors");
        assert!(inserted);

        // Ensure the store contains the operation.
        assert!(store.has_operation(hash).await.expect("no error"));

        // Delete the operation.
        assert!(store.delete_operation(hash).await.expect("no error"));

        let deleted_operation = store.get_operation(hash).await.expect("no error");
        assert!(deleted_operation.is_none());
        assert!(!store.has_operation(hash).await.expect("no error"));

        let deleted_raw_operation = store.get_raw_operation(hash).await.expect("no error");
        assert!(deleted_raw_operation.is_none());
    }

    #[tokio::test]
    async fn delete_payload() {
        let db_pool = initialize_sqlite_db().await;
        let mut store = SqliteStore::new(db_pool);
        let private_key = PrivateKey::new();

        let body = Body::new("hello!".as_bytes());
        let (hash, header, header_bytes) = create_operation(&private_key, &body, 0, 0, None);

        let inserted = store
            .insert_operation(hash, &header, Some(&body), &header_bytes, &0)
            .await
            .expect("no errors");
        assert!(inserted);

        assert!(store.delete_payload(hash).await.expect("no error"));

        let (_, no_body) = store
            .get_operation(hash)
            .await
            .expect("no error")
            .expect("operation exist");
        assert!(no_body.is_none());
        assert!(store.has_operation(hash).await.expect("no error"));

        let (_, no_body) = store
            .get_raw_operation(hash)
            .await
            .expect("no error")
            .expect("operation exist");
        assert!(no_body.is_none());
    }

    #[tokio::test]
    async fn get_log() {
        let db_pool = initialize_sqlite_db().await;
        let mut store = SqliteStore::new(db_pool);
        let private_key = PrivateKey::new();
        let log_id = 0;

        let body_0 = Body::new("hello!".as_bytes());
        let body_1 = Body::new("hello again!".as_bytes());
        let body_2 = Body::new("hello for a third time!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body_0, 0, 0, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body_1, 1, 0, Some(hash_0));
        let (hash_2, header_2, header_bytes_2) =
            create_operation(&private_key, &body_2, 2, 0, Some(hash_1));

        store
            .insert_operation(hash_0, &header_0, Some(&body_0), &header_bytes_0, &0)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_1, &header_1, Some(&body_1), &header_bytes_1, &0)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_2, &header_2, Some(&body_2), &header_bytes_2, &0)
            .await
            .expect("no errors");

        // Get all log operations.
        let log = store
            .get_log(&private_key.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log.len(), 3);
        assert_eq!(log[0].0.hash(), hash_0);
        assert_eq!(log[1].0.hash(), hash_1);
        assert_eq!(log[2].0.hash(), hash_2);
        assert_eq!(log[0].1, Some(body_0.clone()));
        assert_eq!(log[1].1, Some(body_1.clone()));
        assert_eq!(log[2].1, Some(body_2.clone()));

        // Get all log operations starting from sequence number 1.
        let log = store
            .get_log(&private_key.public_key(), &log_id, Some(1))
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log.len(), 2);
        assert_eq!(log[0].0.hash(), hash_1);
        assert_eq!(log[1].0.hash(), hash_2);
        assert_eq!(log[0].1, Some(body_1.clone()));
        assert_eq!(log[1].1, Some(body_2.clone()));

        // Get all raw log operations.
        let log = store
            .get_raw_log(&private_key.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log.len(), 3);
        assert_eq!(log[0].0, header_bytes_0);
        assert_eq!(log[1].0, header_bytes_1);
        assert_eq!(log[2].0, header_bytes_2);
        assert_eq!(log[0].1, Some(body_0.to_bytes()));
        assert_eq!(log[1].1, Some(body_1.to_bytes()));
        assert_eq!(log[2].1, Some(body_2.to_bytes()));

        // Get all raw log operations starting from sequence number 1.
        let log = store
            .get_raw_log(&private_key.public_key(), &log_id, Some(1))
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log.len(), 2);
        assert_eq!(log[0].0, header_bytes_1);
        assert_eq!(log[1].0, header_bytes_2);
        assert_eq!(log[0].1, Some(body_1.to_bytes()));
        assert_eq!(log[1].1, Some(body_2.to_bytes()));
    }

    #[tokio::test]
    async fn get_latest_operation() {
        let db_pool = initialize_sqlite_db().await;
        let mut store = SqliteStore::new(db_pool);
        let private_key = PrivateKey::new();
        let log_id = 0;

        let body_0 = Body::new("hello!".as_bytes());
        let body_1 = Body::new("hello again!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body_0, 0, 0, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body_1, 1, 0, Some(hash_0));

        store
            .insert_operation(hash_0, &header_0, Some(&body_0), &header_bytes_0, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_1, &header_1, Some(&body_1), &header_bytes_1, &log_id)
            .await
            .expect("no errors");

        let (latest_header, latest_body) = store
            .latest_operation(&private_key.public_key(), &log_id)
            .await
            .expect("no errors")
            .expect("there's an operation");

        assert_eq!(latest_header.hash(), header_1.hash());
        assert_eq!(latest_body, Some(body_1));
    }

    #[tokio::test]
    async fn delete_operations() {
        let db_pool = initialize_sqlite_db().await;
        let mut store = SqliteStore::new(db_pool);
        let private_key = PrivateKey::new();
        let log_id = 0;

        let body_0 = Body::new("hello!".as_bytes());
        let body_1 = Body::new("hello again!".as_bytes());
        let body_2 = Body::new("final hello!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body_0, 0, 0, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body_1, 1, 100, Some(hash_0));
        let (hash_2, header_2, header_bytes_2) =
            create_operation(&private_key, &body_2, 2, 200, Some(hash_1));

        store
            .insert_operation(hash_0, &header_0, Some(&body_0), &header_bytes_0, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_1, &header_1, Some(&body_1), &header_bytes_1, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_2, &header_2, Some(&body_2), &header_bytes_2, &log_id)
            .await
            .expect("no errors");

        // Get all log operations.
        let log = store
            .get_log(&private_key.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        // We expect the log to have 3 operations.
        assert_eq!(log.len(), 3);

        // Delete all operations _before_ seq_num 2.
        let deleted = store
            .delete_operations(&private_key.public_key(), &log_id, 2)
            .await
            .expect("no errors");
        assert!(deleted);

        let log = store
            .get_log(&private_key.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        // There is now only one operation in the log.
        assert_eq!(log.len(), 1);

        // The remaining operation should be the latest (seq_num == 2).
        assert_eq!(log[0].0.hash(), header_2.hash());

        // Deleting the same range again should return `false`, meaning no deletion occurred.
        let deleted = store
            .delete_operations(&private_key.public_key(), &log_id, 2)
            .await
            .expect("no errors");
        assert!(!deleted);
    }

    #[tokio::test]
    async fn delete_payloads() {
        let db_pool = initialize_sqlite_db().await;
        let mut store = SqliteStore::new(db_pool);
        let private_key = PrivateKey::new();
        let log_id = 0;

        let body_0 = Body::new("hello!".as_bytes());
        let body_1 = Body::new("hello again!".as_bytes());
        let body_2 = Body::new("final hello!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body_0, 0, 0, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body_1, 1, 100, Some(hash_0));
        let (hash_2, header_2, header_bytes_2) =
            create_operation(&private_key, &body_2, 2, 200, Some(hash_1));

        store
            .insert_operation(hash_0, &header_0, Some(&body_0), &header_bytes_0, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_1, &header_1, Some(&body_1), &header_bytes_1, &log_id)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_2, &header_2, Some(&body_2), &header_bytes_2, &log_id)
            .await
            .expect("no errors");

        // Get all log operations.
        let log = store
            .get_log(&private_key.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        // We expect the log to have 3 operations.
        assert_eq!(log.len(), 3);

        assert_eq!(log[0].1, Some(body_0));
        assert_eq!(log[1].1, Some(body_1));
        assert_eq!(log[2].1, Some(body_2.clone()));

        // Delete all operation payloads from sequence number 0 up to but not including 2.
        let deleted = store
            .delete_payloads(&private_key.public_key(), &log_id, 0, 2)
            .await
            .expect("no errors");
        assert!(deleted);

        let log = store
            .get_log(&private_key.public_key(), &log_id, None)
            .await
            .expect("no errors")
            .expect("log should exist");

        assert_eq!(log[0].1, None);
        assert_eq!(log[1].1, None);
        assert_eq!(log[2].1, Some(body_2));
    }

    #[tokio::test]
    async fn get_log_heights() {
        let db_pool = initialize_sqlite_db().await;
        let mut store = SqliteStore::new(db_pool);

        let log_id = 0;

        let private_key_0 = PrivateKey::new();
        let private_key_1 = PrivateKey::new();
        let private_key_2 = PrivateKey::new();

        let body_0 = Body::new("hello!".as_bytes());
        let body_1 = Body::new("hello again!".as_bytes());
        let body_2 = Body::new("hello for a third time!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key_0, &body_0, 0, 0, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key_0, &body_1, 1, 0, Some(hash_0));
        let (hash_2, header_2, header_bytes_2) =
            create_operation(&private_key_0, &body_2, 2, 0, Some(hash_1));

        let log_heights = store.get_log_heights(&log_id).await.expect("no errors");
        assert!(log_heights.is_empty());

        store
            .insert_operation(hash_0, &header_0, Some(&body_0), &header_bytes_0, &0)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_1, &header_1, Some(&body_1), &header_bytes_1, &0)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_2, &header_2, Some(&body_2), &header_bytes_2, &0)
            .await
            .expect("no errors");

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key_1, &body_0, 0, 0, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key_1, &body_1, 1, 0, Some(hash_0));

        store
            .insert_operation(hash_0, &header_0, Some(&body_0), &header_bytes_0, &0)
            .await
            .expect("no errors");
        store
            .insert_operation(hash_1, &header_1, Some(&body_1), &header_bytes_1, &0)
            .await
            .expect("no errors");

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key_2, &body_0, 0, 0, None);

        store
            .insert_operation(hash_0, &header_0, Some(&body_0), &header_bytes_0, &0)
            .await
            .expect("no errors");

        let log_heights = store.get_log_heights(&log_id).await.expect("no errors");

        assert_eq!(log_heights.len(), 3);

        // Ensure the correct sequence number for each public key.
        assert!(log_heights.contains(&(private_key_0.public_key(), 2)));
        assert!(log_heights.contains(&(private_key_1.public_key(), 1)));
        assert!(log_heights.contains(&(private_key_2.public_key(), 0)));
    }
}
