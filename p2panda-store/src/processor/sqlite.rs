// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::Hash;
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use serde::{Deserialize, Serialize};
use sqlx::{query, query_scalar};

use crate::processor::traits::ProcessorStore;

use crate::{SqliteError, SqliteStore};

impl<T> ProcessorStore<T> for SqliteStore
where
    T: Serialize + for<'a> Deserialize<'a>,
{
    type Error = SqliteError;

    async fn get_event(&self, id: &Hash) -> Result<Option<T>, Self::Error> {
        let event_bytes: Option<Vec<u8>> = self
            .execute(async |pool| {
                query_scalar(
                    "
                    SELECT
                        event
                    FROM
                        processor_v1
                    WHERE
                        id = ?
                    ",
                )
                .bind(id.to_hex())
                .fetch_optional(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        if let Some(bytes) = event_bytes {
            let event = decode_cbor(&bytes[..])
                .map_err(|err| SqliteError::Decode("event".into(), err.into()))?;
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    async fn set_event(&self, id: &Hash, event: &T) -> Result<(), Self::Error> {
        // TODO: Do we expect to only ever store an operation-related event once or might it be
        // stored multiple times as it moves through the pipeline?
        //
        // If we expect multiple valid insertions then we want to rather INSERT OR REPLACE.
        self.tx(async |tx| {
            query(
                "
                INSERT OR IGNORE
                    INTO
                        processor_v1(id, event)
                    VALUES
                        (?, ?)
                ",
            )
            .bind(id.to_hex())
            .bind(encode_cbor(&event).map_err(|err| SqliteError::Encode("event".to_string(), err))?)
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(())
    }
}
