// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite persistent storage.
use std::hash::{DefaultHasher, Hash as StdHash, Hasher};
use std::marker::PhantomData;

use anyhow::{Error, Result};
use sqlx::migrate;
use sqlx::migrate::MigrateDatabase;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{query, query_as, Sqlite};

use p2panda_core::{Body, Extensions, Hash, Header, RawOperation};

use crate::sqlite::models::{OperationRow, RawOperationRow};
use crate::{LogId, OperationStore};

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
pub async fn create_database(url: &str) -> Result<()> {
    if !Sqlite::database_exists(url).await? {
        Sqlite::create_database(url).await?;
    }

    Ok(())
}

/// Drop the database if it exists.
pub async fn drop_database(url: &str) -> Result<()> {
    if Sqlite::database_exists(url).await? {
        Sqlite::drop_database(url).await?;
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

/// Run any pending database migrations from inside the application.
pub async fn run_pending_migrations(pool: &Pool) -> Result<()> {
    migrate!().run(pool).await?;
    Ok(())
}

fn calculate_hash<T: StdHash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

fn serialize_extensions<T: Extensions>(extensions: &T) -> Result<Vec<u8>> {
    let mut bytes: Vec<u8> = Vec::new();
    ciborium::ser::into_writer(extensions, &mut bytes)?;

    Ok(bytes)
}

pub(crate) fn deserialize_extensions<T>(bytes: Vec<u8>) -> Result<T>
where
    T: Extensions,
{
    let extensions = ciborium::de::from_reader(&bytes[..])?;

    Ok(extensions)
}

impl<L, E> OperationStore<L, E> for SqliteStore<L, E>
where
    L: LogId + Send + Sync,
    E: Extensions + Send + Sync,
{
    type Error = Error;

    async fn insert_operation(
        &mut self,
        hash: Hash,
        header: &Header<E>,
        body: Option<&Body>,
        header_bytes: &[u8],
        log_id: &L,
    ) -> Result<bool, Self::Error> {
        // Start a transaction.
        //
        // Any insertions after this point, and before `execute()`, will be rolled back in the
        // event of an error.
        let mut tx = self.pool.begin().await?;

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
        .bind(header.extensions.as_ref().map(|extensions| {
            serialize_extensions(extensions).expect("extenions are serializable")
        }))
        .bind(body.map(|body| body.to_bytes()))
        .bind(header_bytes)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

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
        let mut tx = self.pool.begin().await?;

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
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_payload(&mut self, hash: Hash) -> Result<bool, Self::Error> {
        let mut tx = self.pool.begin().await?;

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
        .execute(&mut *tx)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::{Body, Hash, Header, PrivateKey};
    use serde::{Deserialize, Serialize};

    use crate::{LogStore, OperationStore};

    use super::{
        connection_pool, create_database, drop_database, run_pending_migrations, Pool, SqliteStore,
    };

    fn db_test_url() -> String {
        // Give each database a unique name.
        let db_name = format!("dbmem{}", rand::random::<u32>());

        // SQLite database stored in memory.
        let url = format!("sqlite://{db_name}?mode=memory&cache=private");

        url
    }

    async fn initialize_sqlite_db() -> Pool {
        let url = db_test_url();

        drop_database(&url).await.unwrap();
        create_database(&url).await.unwrap();

        let pool = connection_pool(&url, 1).await.unwrap();

        if run_pending_migrations(&pool).await.is_err() {
            pool.close().await;
            panic!("Database migration failed");
        }

        pool
    }

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
    async fn insert_get_operation() {
        let db_pool = initialize_sqlite_db().await;
        let mut store = SqliteStore::new(db_pool);
        let private_key = PrivateKey::new();

        // Create the first operation.
        let body = Body::new("hello!".as_bytes());
        let (hash, header, header_bytes) = create_operation(&private_key, &body, 0, 0, None);

        // Create the second operation.
        let body_2 = Body::new("buenas!".as_bytes());
        let (hash_2, header_2, header_bytes_2) =
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
}
