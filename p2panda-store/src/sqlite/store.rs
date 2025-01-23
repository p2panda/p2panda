// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite persistent storage.
use std::hash::{DefaultHasher, Hash as StdHash, Hasher};

use anyhow::{Error, Result};
use sqlx::migrate;
use sqlx::migrate::MigrateDatabase;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{query, Sqlite};

use p2panda_core::{Body, Extensions, Hash, Header, RawOperation};

use crate::{LogId, OperationStore};

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

impl<L, E> OperationStore<L, E> for SqliteStore
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
                    header_bytes,
                )
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ",
        )
        .bind(hash.to_hex())
        .bind(calculate_hash(log_id).to_string())
        .bind(header.version.to_string())
        .bind(header.public_key.to_string())
        .bind(header.signature.map(|sig| sig.to_string()))
        .bind(header.payload_size.to_string())
        .bind(header.payload_hash.map(|hash| hash.to_string()))
        .bind(header.timestamp.to_string())
        .bind(header.seq_num.to_string())
        .bind(header.backlink.map(|backlink| backlink.to_string()))
        .bind(
            header
                .previous
                .iter()
                .map(|previous| previous.to_string())
                .collect::<Vec<String>>()
                .concat(),
        )
        .bind(serialize_extensions(&header.extensions)?)
        .bind(body.map(|body| body.to_bytes()))
        .bind(header_bytes)
        .execute(&mut *tx)
        .await?;

        Ok(true)
    }

    async fn get_operation(
        &self,
        _hash: Hash,
    ) -> Result<Option<(Header<E>, Option<Body>)>, Self::Error> {
        /*
        let operation_rows = query_as::<_, OperationFieldsJoinedRow>(
            "
            SELECT
                operations_v1.public_key,
                operations_v1.document_id,
                operations_v1.operation_id,
                operations_v1.action,
                operations_v1.schema_id,
                operations_v1.previous,
                operations_v1.sorted_index,
                operation_fields_v1.name,
                operation_fields_v1.field_type,
                operation_fields_v1.value,
                operation_fields_v1.list_index
            FROM
                operations_v1
            LEFT JOIN operation_fields_v1
                ON
                    operation_fields_v1.operation_id = operations_v1.operation_id
            WHERE
                operations_v1.operation_id = $1
            ORDER BY
                operation_fields_v1.list_index ASC
            ",
        )
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OperationStorageError::FatalStorageError(e.to_string()))?;

        let operation = parse_operation_rows(operation_rows);
        Ok(operation)
        */

        Ok(None)
    }

    async fn get_raw_operation(&self, _hash: Hash) -> Result<Option<RawOperation>, Self::Error> {
        todo!()
    }

    async fn has_operation(&self, _hash: Hash) -> Result<bool, Self::Error> {
        todo!()
    }

    async fn delete_operation(&mut self, _hash: Hash) -> Result<bool, Self::Error> {
        todo!()
    }

    async fn delete_payload(&mut self, _hash: Hash) -> Result<bool, Self::Error> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::SqliteStore;

    #[tokio::test]
    async fn default_sqlite_store() {
        todo!()
    }
}
