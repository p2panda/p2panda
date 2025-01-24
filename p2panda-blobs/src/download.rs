// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{ensure, Result};
use futures_lite::{Stream, StreamExt};
use iroh::NodeAddr;
use iroh_blobs::downloader::{DownloadRequest, Downloader};
use iroh_blobs::get::db::DownloadProgress;
use iroh_blobs::get::Stats;
use iroh_blobs::util::local_pool::LocalPoolHandle;
use iroh_blobs::util::progress::{AsyncChannelProgressSender, ProgressSender};
use iroh_blobs::{BlobFormat, Hash as IrohHash, HashAndFormat};
use p2panda_core::Hash;
use p2panda_net::{Network, TopicId};
use p2panda_sync::TopicQuery;
use serde::{Deserialize, Serialize};
use serde_error::Error as RpcError;

use crate::from_node_addr;

/// Status of a blob download attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownloadBlobEvent {
    Done,
    Abort(RpcError),
}

pub(crate) async fn download_blob<T: TopicQuery + TopicId + 'static>(
    network: Network<T>,
    downloader: Downloader,
    pool_handle: LocalPoolHandle,
    hash: Hash,
) -> impl Stream<Item = DownloadBlobEvent> {
    let (sender, receiver) = async_channel::bounded(1024);
    let progress = AsyncChannelProgressSender::new(sender);
    let hash_and_format = HashAndFormat {
        hash: IrohHash::from_bytes(*hash.as_bytes()),
        format: BlobFormat::Raw,
    };

    pool_handle.spawn_detached(move || async move {
        match download_queued(network, &downloader, hash_and_format, progress.clone()).await {
            Ok(stats) => {
                progress.send(DownloadProgress::AllDone(stats)).await.ok();
            }
            Err(err) => {
                progress
                    .send(DownloadProgress::Abort(RpcError::new(&*err)))
                    .await
                    .ok();
            }
        }
    });

    receiver.filter_map(|event| match event {
        DownloadProgress::AllDone(_) => Some(DownloadBlobEvent::Done),
        // @TODO: Use own error type here
        DownloadProgress::Abort(err) => Some(DownloadBlobEvent::Abort(err)),
        _ => {
            // @TODO: Add more event types
            None
        }
    })
}

async fn download_queued<T: TopicQuery + TopicId + 'static>(
    network: Network<T>,
    downloader: &Downloader,
    hash_and_format: HashAndFormat,
    progress: AsyncChannelProgressSender<DownloadProgress>,
) -> Result<Stats> {
    let addrs = network.known_peers().await?;
    ensure!(!addrs.is_empty(), "no way to reach a node for download");

    let iroh_addrs: Vec<NodeAddr> = addrs
        .iter()
        .map(|addr| from_node_addr(addr.clone()))
        .collect();
    let req = DownloadRequest::new(hash_and_format, iroh_addrs).progress_sender(progress);
    let handle = downloader.queue(req).await;

    let stats = handle.await?;
    Ok(stats)
}
