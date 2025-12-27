// SPDX-License-Identifier: MIT OR Apache-2.0

//! Trait definitions for read-only queries on p2panda operation and log state.
use std::error::Error as StdError;
use std::fmt::{Debug, Display};
use std::pin::Pin;

use p2panda_core::{Body, Hash, Header, PublicKey, RawOperation};

/// Uniquely identify a single-author log.
///
/// The `LogId` exists purely to group a set of operations and is intended to be implemented for
/// any type which meets the design requirements of a particular application.
///
/// A blanket implementation is provided for any type meeting the required trait bounds.
///
/// Here we briefly outline several implementation scenarios:
///
/// An application relying on a one-log-per-author design might choose to implement `LogId` for a thin
/// wrapper around an Ed25519 public key; this effectively ties the log to the public key of the
/// author. Secure Scuttlebutt (SSB) is an example of a protocol which relies on this model.
///
/// In an application where one author may produce operations grouped into multiple logs,
/// `LogId` might be implemented for a `struct` which includes both the public key of the author
/// and a unique number for each log instance.
///
/// Some applications might require semantic grouping of operations. For example, a chat
/// application may choose to create a separate log for each author-channel pairing. In such a
/// scenario, `LogId` might be implemented for a `struct` containing a `String` representation of
/// the channel name.
///
/// Finally, please note that implementers of `LogId` must take steps to ensure their log design is
/// fit for purpose and that all operations have been thoroughly validated before being persisted.
/// No such validation checks are provided by `p2panda-store`.
pub trait LogId: Clone + Debug + Eq + std::hash::Hash {}

impl<T> LogId for T where T: Clone + Debug + Eq + std::hash::Hash {}

/// Interface for storing, deleting and querying operations.
///
/// Two variants of the trait are provided: one which is thread-safe (implementing `Sync`) and one
/// which is purely intended for single-threaded execution contexts.
pub trait OperationStore<LogId, Extensions>: Clone {
    type Error: Display + Debug;

    /// Insert an operation.
    ///
    /// Returns `true` when the insert occurred, or `false` when the operation already existed and
    /// no insertion occurred.
    fn insert_operation(
        &mut self,
        hash: Hash,
        header: &Header<Extensions>,
        body: Option<&Body>,
        header_bytes: &[u8],
        log_id: &LogId,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Get an operation.
    fn get_operation(
        &self,
        hash: Hash,
    ) -> impl Future<Output = Result<Option<(Header<Extensions>, Option<Body>)>, Self::Error>>;

    /// Get the "raw" header and body bytes of an operation.
    fn get_raw_operation(
        &self,
        hash: Hash,
    ) -> impl Future<Output = Result<Option<RawOperation>, Self::Error>>;

    /// Query the existence of an operation.
    ///
    /// Returns `true` if the operation was found in the store and `false` if not.
    fn has_operation(&self, hash: Hash) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Delete an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation was not found in
    /// the store.
    fn delete_operation(&mut self, hash: Hash) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Delete the payload of an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation was not found in
    /// the store or the payload was already deleted.
    fn delete_payload(&mut self, hash: Hash) -> impl Future<Output = Result<bool, Self::Error>>;
}

/// Interface for storing, deleting and querying logs.
///
/// Two variants of the trait are provided: one which is thread-safe (implementing `Sync`) and one
/// which is purely intended for single-threaded execution contexts.
pub trait LogStore<LogId, Extensions> {
    type Error: Display + Debug;

    /// Get operations from an authors' log ordered by sequence number.
    ///
    /// The `from` value will be used as the starting index for log retrieval, if supplied,
    /// otherwise all operations will be returned.
    ///
    /// Returns `None` when either the author or a log with the requested id was not found.
    fn get_log(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        from: Option<u64>,
    ) -> impl Future<Output = Result<Option<Vec<(Header<Extensions>, Option<Body>)>>, Self::Error>>;

    /// Get "raw" header and body bytes from an authors' log ordered by sequence number.
    ///
    /// The `from` value will be used as the starting index for log retrieval, if supplied,
    /// otherwise all operations will be returned.
    ///
    /// Returns `None` when either the author or a log with the requested id was not found.
    fn get_raw_log(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        from: Option<u64>,
    ) -> impl Future<Output = Result<Option<Vec<RawOperation>>, Self::Error>>;

    /// Get the sum of header and body bytes from an authors' log.
    ///
    /// The `from` value will be used as the starting index for log retrieval, if supplied,
    /// otherwise the sum of all operation bytes will be returned.
    ///
    /// Returns `None` when either the author or a log with the requested id was not found.
    fn get_log_size(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        from: Option<u64>,
    ) -> impl Future<Output = Result<Option<u64>, Self::Error>>;

    /// Get hashes from an authors' log ordered by sequence number.
    ///
    /// The `from` value will be used as the starting index for log retrieval, if supplied,
    /// otherwise hashes for all operations will be returned.
    ///
    /// Returns `None` when either the author or a log with the requested id was not found.
    fn get_log_hashes(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        from: Option<u64>,
    ) -> impl Future<Output = Result<Option<Vec<Hash>>, Self::Error>>;

    /// Get the log heights of all logs, by any author, which are stored under the passed log id.
    fn get_log_heights(
        &self,
        log_id: &LogId,
    ) -> impl Future<Output = Result<Vec<(PublicKey, u64)>, Self::Error>>;

    /// Get only the latest operation from an authors' log.
    ///
    /// Returns None when the author or a log with the requested id was not found.
    fn latest_operation(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
    ) -> impl Future<Output = Result<Option<(Header<Extensions>, Option<Body>)>, Self::Error>>;

    /// Delete all operations in a log before the given sequence number.
    ///
    /// Returns `true` when any operations were deleted, returns `false` when the author or log
    /// could not be found, or no operations were deleted.
    fn delete_operations(
        &mut self,
        public_key: &PublicKey,
        log_id: &LogId,
        before: u64,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Delete a range of operation payloads in an authors' log.
    ///
    /// The range of deleted payloads includes it's lower bound `from` but excludes the upper bound
    /// `to`.
    ///
    /// Returns `true` when operations within the requested range were deleted, or `false` when the
    /// author or log could not be found, or no operations were deleted.
    fn delete_payloads(
        &mut self,
        public_key: &PublicKey,
        log_id: &LogId,
        from: u64,
        to: u64,
    ) -> impl Future<Output = Result<bool, Self::Error>>;
}

pub type BoxedError = Box<dyn StdError + Send + Sync + 'static>;

pub trait DynOperationStore<L, E> {
    fn clone_box(&self) -> Box<dyn DynOperationStore<L, E> + Send + 'static>;

    fn insert_operation(
        &mut self,
        hash: Hash,
        header: &Header<E>,
        body: Option<&Body>,
        header_bytes: &[u8],
        log_id: &L,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>>;

    fn get_operation(
        &self,
        hash: Hash,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(Header<E>, Option<Body>)>, BoxedError>> + '_>>;

    fn get_raw_operation(
        &self,
        hash: Hash,
    ) -> Pin<Box<dyn Future<Output = Result<Option<RawOperation>, BoxedError>> + '_>>;

    fn has_operation(
        &self,
        hash: Hash,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>>;

    fn delete_operation(
        &mut self,
        hash: Hash,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>>;

    fn delete_payload(
        &mut self,
        hash: Hash,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>>;
}

pub trait DynLogStore<L, E> {
    fn clone_box(&self) -> Box<dyn DynLogStore<L, E> + Send + 'static>;

    fn get_log(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Pin<
        Box<dyn Future<Output = Result<Option<Vec<(Header<E>, Option<Body>)>>, BoxedError>> + '_>,
    >;

    fn get_raw_log(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<RawOperation>>, BoxedError>> + '_>>;

    fn get_log_size(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<u64>, BoxedError>> + '_>>;

    fn get_log_hashes(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<Hash>>, BoxedError>> + '_>>;

    fn get_log_heights(
        &self,
        log_id: &L,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(PublicKey, u64)>, BoxedError>> + '_>>;

    fn latest_operation(
        &self,
        public_key: &PublicKey,
        log_id: &L,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(Header<E>, Option<Body>)>, BoxedError>> + '_>>;

    fn delete_operations(
        &mut self,
        public_key: &PublicKey,
        log_id: &L,
        before: u64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>>;

    fn delete_payloads(
        &mut self,
        public_key: &PublicKey,
        log_id: &L,
        from: u64,
        to: u64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>>;
}

pub type BoxedOperationStore<L, E> = Box<dyn DynOperationStore<L, E> + Send + 'static>;

impl<T, L, E> DynOperationStore<L, E> for T
where
    T: OperationStore<L, E> + Send + 'static,
    T::Error: StdError + Send + Sync + 'static,
    L: Clone + 'static,
    E: Clone + 'static,
{
    fn clone_box(&self) -> BoxedOperationStore<L, E> {
        Box::new(self.clone())
    }

    fn insert_operation(
        &mut self,
        hash: Hash,
        header: &Header<E>,
        body: Option<&Body>,
        header_bytes: &[u8],
        log_id: &L,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>> {
        let log_id = log_id.clone();
        let header = header.to_owned();
        let header_bytes = header_bytes.to_vec();
        let body = body.cloned();

        Box::pin(async move {
            self.insert_operation(hash, &header, body.as_ref(), &header_bytes, &log_id)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn get_operation(
        &self,
        hash: Hash,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(Header<E>, Option<Body>)>, BoxedError>> + '_>>
    {
        Box::pin(async move {
            self.get_operation(hash)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn get_raw_operation(
        &self,
        hash: Hash,
    ) -> Pin<Box<dyn Future<Output = Result<Option<RawOperation>, BoxedError>> + '_>> {
        Box::pin(async move {
            self.get_raw_operation(hash)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn has_operation(
        &self,
        hash: Hash,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>> {
        Box::pin(async move {
            self.has_operation(hash)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn delete_operation(
        &mut self,
        hash: Hash,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>> {
        Box::pin(async move {
            self.delete_operation(hash)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn delete_payload(
        &mut self,
        hash: Hash,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>> {
        Box::pin(async move {
            self.delete_payload(hash)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }
}

pub type BoxedLogStore<L, E> = Box<dyn DynLogStore<L, E> + Send + 'static>;

impl<T, L, E> DynLogStore<L, E> for T
where
    T: Clone + LogStore<L, E> + Send + 'static,
    T::Error: StdError + Send + Sync + 'static,
    L: Clone + 'static,
    E: Clone + 'static,
{
    fn clone_box(&self) -> BoxedLogStore<L, E> {
        Box::new(self.clone())
    }

    fn get_log(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Pin<
        Box<dyn Future<Output = Result<Option<Vec<(Header<E>, Option<Body>)>>, BoxedError>> + '_>,
    > {
        let public_key = public_key.clone();
        let log_id = log_id.clone();

        Box::pin(async move {
            self.get_log(&public_key, &log_id, from)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn get_raw_log(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<RawOperation>>, BoxedError>> + '_>> {
        let public_key = public_key.clone();
        let log_id = log_id.clone();

        Box::pin(async move {
            self.get_raw_log(&public_key, &log_id, from)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn get_log_size(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<u64>, BoxedError>> + '_>> {
        let public_key = public_key.clone();
        let log_id = log_id.clone();

        Box::pin(async move {
            self.get_log_size(&public_key, &log_id, from)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn get_log_hashes(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<Hash>>, BoxedError>> + '_>> {
        let public_key = public_key.clone();
        let log_id = log_id.clone();

        Box::pin(async move {
            self.get_log_hashes(&public_key, &log_id, from)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn get_log_heights(
        &self,
        log_id: &L,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(PublicKey, u64)>, BoxedError>> + '_>> {
        let log_id = log_id.clone();

        Box::pin(async move {
            self.get_log_heights(&log_id)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn latest_operation(
        &self,
        public_key: &PublicKey,
        log_id: &L,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(Header<E>, Option<Body>)>, BoxedError>> + '_>>
    {
        let public_key = public_key.clone();
        let log_id = log_id.clone();

        Box::pin(async move {
            self.latest_operation(&public_key, &log_id)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn delete_operations(
        &mut self,
        public_key: &PublicKey,
        log_id: &L,
        before: u64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>> {
        let public_key = public_key.clone();
        let log_id = log_id.clone();

        Box::pin(async move {
            self.delete_operations(&public_key, &log_id, before)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn delete_payloads(
        &mut self,
        public_key: &PublicKey,
        log_id: &L,
        from: u64,
        to: u64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>> {
        let public_key = public_key.clone();
        let log_id = log_id.clone();

        Box::pin(async move {
            self.delete_payloads(&public_key, &log_id, from, to)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }
}

pub struct WrappedStore<L, E>(BoxedOperationStore<L, E>, BoxedLogStore<L, E>);

impl<L, E> WrappedStore<L, E> {
    pub fn new(operation_store: BoxedOperationStore<L, E>, log_store: BoxedLogStore<L, E>) -> Self {
        Self(operation_store, log_store)
    }
}

impl<L, E> Clone for WrappedStore<L, E> {
    fn clone(&self) -> Self {
        Self(self.0.clone_box(), self.1.clone_box())
    }
}

impl<L, E> OperationStore<L, E> for WrappedStore<L, E> {
    type Error = BoxedError;

    async fn insert_operation(
        &mut self,
        hash: Hash,
        header: &Header<E>,
        body: Option<&Body>,
        header_bytes: &[u8],
        log_id: &L,
    ) -> Result<bool, Self::Error> {
        self.0
            .as_mut()
            .insert_operation(hash, header, body, header_bytes, log_id)
            .await
    }

    async fn get_operation(
        &self,
        hash: Hash,
    ) -> Result<Option<(Header<E>, Option<Body>)>, Self::Error> {
        self.0.as_ref().get_operation(hash).await
    }

    async fn get_raw_operation(&self, hash: Hash) -> Result<Option<RawOperation>, Self::Error> {
        self.0.as_ref().get_raw_operation(hash).await
    }

    async fn has_operation(&self, hash: Hash) -> Result<bool, Self::Error> {
        self.0.as_ref().has_operation(hash).await
    }

    async fn delete_operation(&mut self, hash: Hash) -> Result<bool, Self::Error> {
        self.0.as_mut().delete_operation(hash).await
    }

    async fn delete_payload(&mut self, hash: Hash) -> Result<bool, Self::Error> {
        self.0.as_mut().delete_payload(hash).await
    }
}

impl<L, E> LogStore<L, E> for WrappedStore<L, E> {
    type Error = BoxedError;

    async fn get_log(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Result<Option<Vec<(Header<E>, Option<Body>)>>, Self::Error> {
        self.1.get_log(public_key, log_id, from).await
    }

    async fn get_raw_log(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Result<Option<Vec<RawOperation>>, Self::Error> {
        self.1.get_raw_log(public_key, log_id, from).await
    }

    async fn get_log_size(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Result<Option<u64>, Self::Error> {
        self.1.get_log_size(public_key, log_id, from).await
    }

    async fn get_log_hashes(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        from: Option<u64>,
    ) -> Result<Option<Vec<Hash>>, Self::Error> {
        self.1.get_log_hashes(public_key, log_id, from).await
    }

    async fn get_log_heights(&self, log_id: &L) -> Result<Vec<(PublicKey, u64)>, Self::Error> {
        self.1.get_log_heights(log_id).await
    }

    async fn latest_operation(
        &self,
        public_key: &PublicKey,
        log_id: &L,
    ) -> Result<Option<(Header<E>, Option<Body>)>, Self::Error> {
        self.1.latest_operation(public_key, log_id).await
    }

    async fn delete_operations(
        &mut self,
        public_key: &PublicKey,
        log_id: &L,
        before: u64,
    ) -> Result<bool, Self::Error> {
        self.1.delete_operations(public_key, log_id, before).await
    }

    async fn delete_payloads(
        &mut self,
        public_key: &PublicKey,
        log_id: &L,
        from: u64,
        to: u64,
    ) -> Result<bool, Self::Error> {
        self.1.delete_payloads(public_key, log_id, from, to).await
    }
}
