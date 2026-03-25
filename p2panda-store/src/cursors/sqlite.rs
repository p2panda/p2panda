// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::identity::Author;
use p2panda_core::{Cursor, LogId};
use sqlx::{query, query_scalar};

use crate::cursors::CursorStore;
use crate::{SqliteError, SqliteStore};

impl<A, L> CursorStore<A, L> for SqliteStore
where
    A: Author,
    L: LogId,
{
    type Error = SqliteError;

    async fn get_cursor(&self, name: impl AsRef<str>) -> Result<Option<Cursor<A, L>>, Self::Error> {
        let state_bytes: Option<Vec<u8>> = self
            .execute(async |pool| {
                query_scalar(
                    "
                    SELECT
                        cursor
                    FROM
                        cursors_v1
                    WHERE
                        name = ?
                    ",
                )
                .bind(name.as_ref())
                .fetch_optional(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        if let Some(bytes) = state_bytes {
            let cursor: Cursor<A, L> = decode_cbor(&bytes[..])
                .map_err(|err| SqliteError::Decode("cursor".into(), err.into()))?;
            Ok(Some(cursor))
        } else {
            Ok(None)
        }
    }

    async fn set_cursor(&self, cursor: &Cursor<A, L>) -> Result<(), Self::Error> {
        self.tx(async |tx| {
            query(
                "
                INSERT
                    INTO
                        cursors_v1(name, cursor)
                    VALUES
                        (?, ?)
                ON CONFLICT(name)
                DO UPDATE
                    SET
                        cursor = EXCLUDED.cursor
                ",
            )
            .bind(cursor.name())
            .bind(
                encode_cbor(&cursor)
                    .map_err(|err| SqliteError::Encode("cursor".to_string(), err))?,
            )
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(())
    }

    async fn delete_cursor(&self, name: impl AsRef<str>) -> Result<(), Self::Error> {
        self.tx(async |tx| {
            query(
                "
                DELETE FROM
                    cursors_v1
                WHERE
                    name = ?
                ",
            )
            .bind(name.as_ref())
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(())
    }
}
