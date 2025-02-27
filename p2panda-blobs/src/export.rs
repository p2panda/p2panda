// SPDX-License-Identifier: MIT OR Apache-2.0

use std::env::temp_dir;
use std::path::PathBuf;

use anyhow::Context;
use iroh_blobs::Hash as IrohHash;
use iroh_blobs::export::ExportProgress;
use iroh_blobs::store::{ExportMode, MapEntry, Store};
use iroh_blobs::util::progress::{AsyncChannelProgressSender, IdGenerator, ProgressSender};
use p2panda_core::Hash;
use tracing::trace;

/// Export a blob from the store to the given filesystem path.
pub(crate) async fn export_blob<S: Store>(
    store: &S,
    hash: Hash,
    outpath: &PathBuf,
) -> anyhow::Result<()> {
    let (sender, _receiver) = async_channel::bounded(1024);
    let progress = AsyncChannelProgressSender::new(sender);

    if let Some(parent) = outpath.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    trace!("exporting blob {} to {}", hash, outpath.display());
    let id = progress.new_id();
    let hash = IrohHash::from_bytes(*hash.as_bytes());
    let entry = store.get(&hash).await?.context("entry not there")?;

    let file_name = outpath
        .file_name()
        .expect("no filename")
        .to_str()
        .expect("filename not valid UTF-8 string");

    // Create temporary directory where we first export the blob to.
    let tmp_path = temp_dir();
    let tmp_file = tmp_path.join(file_name);

    progress
        .send(ExportProgress::Found {
            id,
            hash,
            outpath: tmp_file.clone(),
            size: entry.size(),
            meta: None,
        })
        .await?;
    let progress1 = progress.clone();
    store
        .export(
            hash,
            tmp_file.clone(),
            ExportMode::Copy,
            Box::new(
                move |offset| Ok(progress1.try_send(ExportProgress::Progress { id, offset })?),
            ),
        )
        .await?;

    // Copy the blob file into place once exporting is complete.
    tokio::fs::copy(tmp_file.clone(), outpath).await?;

    // Drop the temporary file.
    drop(tmp_file);
    progress.send(ExportProgress::Done { id }).await?;
    Ok(())
}
