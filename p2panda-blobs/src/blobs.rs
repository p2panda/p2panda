// SPDX-License-Identifier: MIT OR Apache-2.0

use std::io;
use std::path::PathBuf;

use anyhow::Result;
use bytes::Bytes;
use futures_util::Stream;
use iroh_base::hash::Hash as IrohHash;
use iroh_blobs::downloader::Downloader;
use iroh_blobs::store::{Map, Store};
use iroh_blobs::util::local_pool::{Config as LocalPoolConfig, LocalPool};
use p2panda_core::Hash;
use p2panda_net::{Network, NetworkBuilder, TopicId};
use p2panda_sync::Topic;

use crate::config::Config;
use crate::download::download_blob;
use crate::export::export_blob;
use crate::import::{import_blob, import_blob_from_stream, ImportBlobEvent};
use crate::protocol::{BlobsProtocol, BLOBS_ALPN};
use crate::DownloadBlobEvent;

#[derive(Debug)]
pub struct Blobs<T, S>
where
    S: Store,
{
    downloader: Downloader,
    network: Network<T>,
    rt: LocalPool,
    store: S,
}

impl<T, S> Blobs<T, S>
where
    T: Topic + TopicId + 'static,
    S: Store,
{
    pub async fn from_builder(
        network_builder: NetworkBuilder<T>,
        store: S,
    ) -> Result<(Network<T>, Self)> {
        Blobs::from_builder_with_config(network_builder, store, Config::default()).await
    }

    pub async fn from_builder_with_config(
        network_builder: NetworkBuilder<T>,
        store: S,
        config: Config,
    ) -> Result<(Network<T>, Self)> {
        // Calls `num_cpus::get()` tc o define thread count.
        let local_pool_config = LocalPoolConfig::default();
        let local_pool = LocalPool::new(local_pool_config);

        let network = network_builder
            .protocol(
                BLOBS_ALPN,
                BlobsProtocol::new(store.clone(), local_pool.handle().clone()),
            )
            .build()
            .await?;

        let downloader = Downloader::with_config(
            store.clone(),
            network.endpoint().clone(),
            local_pool.handle().clone(),
            config.clone().into(),
            config.into(),
        );

        let blobs = Self {
            downloader,
            network: network.clone(),
            rt: local_pool,
            store,
        };

        Ok((network, blobs))
    }

    /// Get an entry for a hash.
    ///
    /// The entry gives us access to a blobs metadata and methods for accessing the actual
    /// blob data. Getting only the entry is a cheap operation though.
    pub async fn get(&self, hash: Hash) -> anyhow::Result<Option<<S as Map>::Entry>> {
        let hash = IrohHash::from_bytes(*hash.as_bytes());
        let entry = self.store.get(&hash).await?;
        Ok(entry)
    }

    pub async fn import_blob(&self, path: PathBuf) -> impl Stream<Item = ImportBlobEvent> {
        import_blob(self.store.clone(), self.rt.handle().clone(), path).await
    }

    pub async fn import_blob_from_stream<D>(&self, data: D) -> impl Stream<Item = ImportBlobEvent>
    where
        D: Stream<Item = io::Result<Bytes>> + Send + Unpin + 'static,
    {
        import_blob_from_stream(self.store.clone(), self.rt.handle().clone(), data).await
    }

    pub async fn download_blob(&self, hash: Hash) -> impl Stream<Item = DownloadBlobEvent> {
        download_blob(
            self.network.clone(),
            self.downloader.clone(),
            self.rt.handle().clone(),
            hash,
        )
        .await
    }

    pub async fn export_blob(&self, hash: Hash, path: &PathBuf) -> Result<()> {
        export_blob(&self.store, hash, path).await?;
        Ok(())
    }
}
