// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::identity::Signer;
use p2panda_core::{AnyOperation, Body, Hash, Header, Operation, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use serde::{Deserialize, Serialize};

pub type LogId = Hash;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomExtensions {
    log_id: LogId,
}

/// Create a signed operation, append it to log and insert into the store.
pub async fn create_operation<S>(
    store: &SqliteStore,
    signer: &S,
    topic: Topic,
    body: &[u8],
) -> Result<AnyOperation, SqliteError>
where
    S: Signer,
{
    // Derive the log id from the topic. This allows applications to not reveal the topic if they
    // want to treat it as a secret.
    let log_id = LogId::digest(topic.as_bytes());

    // Mention the log id in header extensions to enable multiple logs per author.
    let extensions = CustomExtensions { log_id };

    let operation = tx!(store, {
        let verifying_key = signer.verifying_key();

        let (seq_num, backlink) = store
            .get_latest_entry_tx(&verifying_key, &log_id)
            .await?
            .map(|operation| (operation.header.seq_num + 1, Some(operation.hash)))
            .unwrap_or((0, None));

        let body = Body::from_bytes(body);

        let header = Header::builder()
            .seq_num(seq_num)
            .backlink(backlink)
            .body(&body)
            .build(signer, extensions);

        <SqliteStore as TopicStore<Topic, VerifyingKey, LogId>>::associate(
            &store,
            &topic,
            &verifying_key,
            &log_id,
        )
        .await?;

        let operation = Operation::from_parts(header, Some(body));

        store
            .insert_operation(&operation.hash, &operation, &log_id)
            .await?;

        operation
    });

    Ok(operation.into())
}
