// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use bytes::Bytes;
use futures_lite::StreamExt;
use futures_util::Stream;
use iroh_base::rpc::RpcError;
use iroh_blobs::provider::AddProgress;
use iroh_blobs::store::{ImportMode, ImportProgress, Store};
use iroh_blobs::util::local_pool::LocalPoolHandle;
use iroh_blobs::util::progress::{AsyncChannelProgressSender, ProgressSender};
use iroh_blobs::{BlobFormat, HashAndFormat};
use p2panda_core::Hash;
use serde::{Deserialize, Serialize};

pub async fn import_blob<S: Store>(
    store: S,
    pool_handle: LocalPoolHandle,
    path: PathBuf,
) -> impl Stream<Item = ImportBlobEvent> {
    let (sender, receiver) = async_channel::bounded(32);

    let sender = sender.clone();
    pool_handle.spawn_detached(|| async move {
        if let Err(e) = add_from_path(store, path, sender.clone()).await {
            sender.send(AddProgress::Abort(e.into())).await.ok();
        }
    });

    receiver.filter_map(|event| {
        match event {
            AddProgress::AllDone { hash, .. } => {
                Some(ImportBlobEvent::Done(Hash::from_bytes(*hash.as_bytes())))
            }
            // @TODO: Use own error type here
            AddProgress::Abort(err) => Some(ImportBlobEvent::Abort(err)),
            _ => {
                // @TODO: Add more event types
                None
            }
        }
    })
}

pub async fn import_blob_from_stream<S, T>(
    store: S,
    pool_handle: LocalPoolHandle,
    data: T,
) -> impl Stream<Item = ImportBlobEvent>
where
    T: Stream<Item = io::Result<Bytes>> + Send + Unpin + 'static,
    S: Store,
{
    let (sender, receiver) = async_channel::bounded(32);

    let sender = sender.clone();
    pool_handle.spawn_detached(|| async move {
        if let Err(e) = add_from_stream(store, data, sender.clone()).await {
            sender.send(AddProgress::Abort(e.into())).await.ok();
        }
    });

    receiver.filter_map(|event| {
        match event {
            AddProgress::AllDone { hash, .. } => {
                Some(ImportBlobEvent::Done(Hash::from_bytes(*hash.as_bytes())))
            }
            // @TODO: Use own error type here
            AddProgress::Abort(err) => Some(ImportBlobEvent::Abort(err)),
            _ => {
                // @TODO: Add more event types
                None
            }
        }
    })
}

async fn add_from_path<S: Store>(
    store: S,
    path: PathBuf,
    progress: async_channel::Sender<AddProgress>,
) -> Result<()> {
    let progress = AsyncChannelProgressSender::new(progress);
    let names = Arc::new(Mutex::new(BTreeMap::new()));

    let import_progress = progress.clone().with_filter_map(move |x| match x {
        ImportProgress::Found { id, name } => {
            names.lock().unwrap().insert(id, name);
            None
        }
        ImportProgress::Size { id, size } => {
            let name = names.lock().unwrap().remove(&id)?;
            Some(AddProgress::Found { id, name, size })
        }
        ImportProgress::OutboardProgress { id, offset } => {
            Some(AddProgress::Progress { id, offset })
        }
        ImportProgress::OutboardDone { hash, id } => Some(AddProgress::Done { hash, id }),
        _ => None,
    });

    let import_mode = ImportMode::default();
    let (tag, _size) = store
        .import_file(path, import_mode, BlobFormat::Raw, import_progress)
        .await?;

    let hash_and_format = tag.inner();
    let HashAndFormat { hash, format } = *hash_and_format;
    let tag = store.create_tag(*hash_and_format).await?;
    progress
        .send(AddProgress::AllDone { hash, format, tag })
        .await?;

    Ok(())
}

async fn add_from_stream<T, S>(
    store: S,
    data: T,
    progress: async_channel::Sender<AddProgress>,
) -> Result<()>
where
    T: Stream<Item = io::Result<Bytes>> + Send + Unpin + 'static,
    S: Store,
{
    let progress = AsyncChannelProgressSender::new(progress);
    let names = Arc::new(Mutex::new(BTreeMap::new()));

    let import_progress = progress.clone().with_filter_map(move |x| match x {
        ImportProgress::Found { id, name } => {
            names.lock().unwrap().insert(id, name);
            None
        }
        ImportProgress::Size { id, size } => {
            let name = names.lock().unwrap().remove(&id)?;
            Some(AddProgress::Found { id, name, size })
        }
        ImportProgress::OutboardProgress { id, offset } => {
            Some(AddProgress::Progress { id, offset })
        }
        ImportProgress::OutboardDone { hash, id } => Some(AddProgress::Done { hash, id }),
        ImportProgress::CopyProgress { id, offset } => Some(AddProgress::Progress { id, offset }),
    });

    let (tag, _size) = store
        .import_stream(data, BlobFormat::Raw, import_progress)
        .await?;

    let hash_and_format = tag.inner();
    let HashAndFormat { hash, format } = *hash_and_format;
    let tag = store.create_tag(*hash_and_format).await?;
    progress
        .send(AddProgress::AllDone { hash, format, tag })
        .await?;

    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ImportBlobEvent {
    Done(Hash),
    Abort(RpcError),
}


#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use iroh_base::hash::Hash as IrohHash;
    use iroh_blobs::store::mem::Store;
    use iroh_blobs::store::Map;
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn import_blob_from_fs() {
        let store = Store::new();
        let pool_handle = LocalPoolHandle::new(2);

        // Create a temporary directory and filepath.
        let tmp_dir = tempdir().unwrap();
        let tmp_path = tmp_dir.path().join("test.txt");

        // Create a file in the temporary directory and write to it.
        let mut tmp_file = File::create(&tmp_path).unwrap();
        write!(tmp_file, "Testing file import...").unwrap();

        let path = PathBuf::from(tmp_path.clone());

        // Import the file as a blob and ensure success.
        let mut stream = import_blob(store.clone(), pool_handle, path).await;
        let event = stream.next().await.unwrap();
        assert!(matches!(event, ImportBlobEvent::Done(_)));

        // Obtain an iroh hash from the event.
        let ImportBlobEvent::Done(hash) = event else {
            panic!("expected ImportBlobEvent::Done containing hash")
        };
        let iroh_hash = IrohHash::from_bytes(*hash.as_bytes());

        // Ensure that the blob did indeed make it into the store.
        assert!(store.get(&iroh_hash).await.unwrap().is_some());

        // Explicitly drop the file handle to avoid resource leaks.
        drop(tmp_file);
        tmp_dir.close().unwrap();
    }
}
