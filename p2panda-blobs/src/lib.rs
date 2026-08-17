// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(docsrs, feature(doc_cfg))]

use std::collections::HashMap;
use std::path::Path;

use iroh_blobs::ALPN as IROH_BLOBS_ALPN;
use n0_future::StreamExt;
use p2panda_core::{Hash, Signature, SigningKey, Topic};
use p2panda_net::gossip::GossipHandle;
use p2panda_net::{Endpoint, Gossip, NodeId};
use rand::RngExt;
use rand::rand_core::UnwrapErr;
use rand::rngs::SysRng;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ContentDiscoveryMessage {
    Want {
        nonce: u32,
        content_id: Hash,
        node_id: NodeId,
        signature: Signature,
    },
    Have {
        nonce: u32,
        content_id: Hash,
        node_id: NodeId,
        signature: Signature,
    },
}

impl ContentDiscoveryMessage {
    fn sign(nonce: u32, content_id: Hash, node_id: NodeId, signing_key: &SigningKey) -> Signature {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&nonce.to_be_bytes());
        bytes.extend_from_slice(content_id.as_bytes());
        bytes.extend_from_slice(node_id.as_bytes());
        signing_key.sign(&bytes)
    }

    fn verify(nonce: &u32, content_id: &Hash, node_id: &NodeId, signature: &Signature) -> bool {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&nonce.to_be_bytes());
        bytes.extend_from_slice(content_id.as_bytes());
        bytes.extend_from_slice(node_id.as_bytes());
        node_id.verify(&bytes, signature)
    }

    pub fn want(content_id: Hash, signing_key: &SigningKey) -> Self {
        let nonce: u32 = {
            let mut csprng = UnwrapErr(SysRng);
            csprng.random()
        };
        let node_id = signing_key.verifying_key();
        let signature = Self::sign(nonce, content_id, node_id, signing_key);

        Self::Want {
            nonce,
            content_id,
            node_id,
            signature,
        }
    }

    pub fn have(nonce: u32, content_id: Hash, signing_key: &SigningKey) -> Self {
        let node_id = signing_key.verifying_key();
        let signature = Self::sign(nonce, content_id, node_id, signing_key);

        Self::Have {
            nonce,
            content_id,
            node_id,
            signature,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(bytes)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(self)
    }

    pub fn is_valid_request(request: &Self) -> bool {
        let Self::Want {
            nonce,
            content_id,
            node_id,
            signature,
        } = request
        else {
            return false;
        };

        if !Self::verify(nonce, content_id, node_id, signature) {
            return false;
        }

        true
    }

    pub fn is_valid_response(request: &Self, response: &Self) -> bool {
        let Self::Want {
            nonce: request_nonce,
            content_id: request_content_id,
            ..
        } = request
        else {
            return false;
        };

        if !Self::is_valid_request(&request) {
            return false;
        }

        let Self::Have {
            nonce: response_nonce,
            content_id: response_content_id,
            node_id: response_node_id,
            signature: response_signature,
        } = request
        else {
            return false;
        };

        if !Self::verify(
            response_nonce,
            response_content_id,
            response_node_id,
            response_signature,
        ) {
            return false;
        }

        if request_nonce == response_nonce {
            return false;
        }

        if request_content_id == response_content_id {
            return false;
        }

        true
    }
}

#[derive(Clone, Debug)]
struct Want {
    message: ContentDiscoveryMessage,
    tx: mpsc::UnboundedSender<NodeId>,
}

#[derive(Clone, Debug)]
struct ContentDiscovery {
    handle: GossipHandle,
    store: iroh_blobs::api::Store,
    wants: HashMap<Hash, Want>,
    signing_key: SigningKey,
}

impl ContentDiscovery {
    pub async fn new(
        gossip: Gossip,
        store: iroh_blobs::api::Store,
        topic: Topic,
        signing_key: SigningKey,
    ) -> Result<Self, p2panda_net::gossip::GossipError> {
        let handle = gossip.stream(topic).await?;
        let mut gossip_rx = handle.subscribe();

        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    Some(event) = gossip_rx.next() => {
                        let Err(ref err) = event else {
                            break;
                        };

                        let Ok(bytes) = event else {
                            unreachable!("error handled before");
                        };
                    }
                }
            }
        });

        Ok(Self {
            handle,
            store,
            wants: HashMap::with_capacity(32),
            signing_key,
        })
    }

    pub fn find_providers(&self, content_id: Hash) -> mpsc::UnboundedReceiver<NodeId> {
        let (tx, rx) = mpsc::unbounded_channel();

        self.wants.insert(
            content_id,
            Want {
                message: ContentDiscoveryMessage::want(content_id, &self.signing_key),
                tx,
            },
        );

        rx
    }
}

impl iroh_blobs::api::downloader::ContentDiscovery for ContentDiscovery {
    fn find_providers(
        &self,
        query: iroh_blobs::HashAndFormat,
    ) -> n0_future::stream::Boxed<iroh_base::EndpointId> {
        let rx = self.find_providers(from_blobs_hash(query.hash));
        let stream = async move {}.boxed();

        Box::pin(stream)
    }
}

#[derive(Clone, Debug)]
pub struct Blobs {
    endpoint: Endpoint,
    downloader: iroh_blobs::api::downloader::Downloader,
    content_discovery: ContentDiscovery,
    store: iroh_blobs::api::Store,
}

impl Blobs {
    pub(crate) async fn new(
        endpoint: Endpoint,
        gossip: Gossip,
        store: iroh_blobs::api::Store,
        discovery_topic: Topic,
        signing_key: SigningKey,
    ) -> Result<Self, SpawnError> {
        let downloader = store.downloader(&endpoint.endpoint().await?);
        let content_discovery =
            ContentDiscovery::new(gossip, store.clone(), discovery_topic, signing_key).await?;

        Ok(Self {
            downloader,
            endpoint,
            content_discovery,
            store,
        })
    }

    pub fn builder(endpoint: Endpoint, gossip: Gossip) -> Builder {
        Builder::new(endpoint, gossip)
    }

    pub async fn request(&self, hash: Hash) {
        let _progress =
            self.downloader
                .download_with_opts(iroh_blobs::api::downloader::DownloadOptions::new(
                    to_blobs_hash(hash),
                    self.content_discovery.clone(),
                    iroh_blobs::api::downloader::SplitStrategy::Split,
                ));
    }

    pub async fn request_from(
        &self,
        hash: Hash,
        node_id: NodeId,
    ) -> Result<(), p2panda_net::iroh_endpoint::EndpointError> {
        let connection = self.endpoint.connect(node_id, self.protocol_id()).await?;
        let _progress = iroh_blobs::get::request::get_blob(connection, to_blobs_hash(hash));

        Ok(())
    }

    pub async fn import(&self, file_path: impl AsRef<Path>) -> Result<(), ImportError> {
        let file_path = std::path::absolute(&file_path)?;
        let _tag = self.store.blobs().add_path(file_path).await?;

        Ok(())
    }

    fn protocol_id(&self) -> Vec<u8> {
        p2panda_net::hash_protocol_id_with_network_id(IROH_BLOBS_ALPN, self.endpoint.network_id())
    }
}

pub struct Builder {
    endpoint: Endpoint,
    gossip: Gossip,
    store: Option<iroh_blobs::api::Store>,
    discovery_topic: Option<Topic>,
    signing_key: Option<SigningKey>,
}

impl Builder {
    pub fn new(endpoint: Endpoint, gossip: Gossip) -> Self {
        Self {
            endpoint,
            gossip,
            store: None,
            discovery_topic: None,
            signing_key: None,
        }
    }

    pub fn store(mut self, store: iroh_blobs::api::Store) -> Self {
        self.store = Some(store);
        self
    }

    pub fn discovery_topic(mut self, topic: impl Into<Topic>) -> Self {
        self.discovery_topic = Some(topic.into());
        self
    }

    pub fn signing_key(mut self, signing_key: SigningKey) -> Self {
        self.signing_key = Some(signing_key);
        self
    }

    pub async fn spawn(self) -> Result<Blobs, SpawnError> {
        let store = self
            .store
            .unwrap_or(iroh_blobs::store::mem::MemStore::new().into());
        let discovery_topic = self.discovery_topic.unwrap_or(Topic::random());
        let protocol = iroh_blobs::BlobsProtocol::new(&store, None);
        let signing_key = self.signing_key.unwrap_or(SigningKey::generate());

        self.endpoint.accept(IROH_BLOBS_ALPN, protocol).await?;

        Ok(Blobs::new(
            self.endpoint,
            self.gossip,
            store,
            discovery_topic,
            signing_key,
        )
        .await?)
    }
}

fn to_blobs_hash(hash: p2panda_core::Hash) -> iroh_blobs::Hash {
    iroh_blobs::Hash::from_bytes(*hash.as_bytes())
}

fn from_blobs_hash(hash: iroh_blobs::Hash) -> p2panda_core::Hash {
    p2panda_core::Hash::from_bytes(*hash.as_bytes())
}

#[derive(Debug, Error)]
pub enum SpawnError {
    #[error(transparent)]
    Endpoint(#[from] p2panda_net::iroh_endpoint::EndpointError),

    #[error(transparent)]
    Gossip(#[from] p2panda_net::gossip::GossipError),
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error(transparent)]
    BlobsRpcRequest(#[from] iroh_blobs::api::RequestError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
