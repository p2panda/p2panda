// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use crate::hash::Hash;
use crate::identity::VerifyingKey;
use crate::logs::SeqNum;
use crate::operation::{AnyHeader, AnyOperation, Body, Header, HeaderError, PayloadSize};
use crate::traits::{Chain, Digest, Extensions, Offchain, Provenance};

/// Encoded bytes of an operation header and optional body.
pub type RawOperation = (Vec<u8>, Option<Vec<u8>>);

/// Combined [`Header`], [`Body`] and operation [`struct@Hash`] (Operation Id).
#[derive(Clone, Debug)]
pub struct Operation<E = ()> {
    pub hash: Hash,
    pub header: Header<E>,
    pub body: Option<Body>,
}

impl<E> Operation<E>
where
    E: Extensions,
{
    pub fn from_parts(header: Header<E>, body: Option<Body>) -> Self {
        Self {
            hash: header.hash(),
            header,
            body,
        }
    }
}

impl<E> PartialEq for Operation<E> {
    fn eq(&self, other: &Self) -> bool {
        self.hash.eq(&other.hash)
    }
}

impl<E> Eq for Operation<E> {}

impl<E> Borrow<Header<E>> for Operation<E> {
    fn borrow(&self) -> &Header<E> {
        &self.header
    }
}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl<E> PartialOrd for Operation<E> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.hash.cmp(&other.hash))
    }
}

impl<E> Ord for Operation<E> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.hash.cmp(&other.hash)
    }
}

impl<E> Digest<Hash> for Operation<E> {
    fn hash(&self) -> Hash {
        self.hash
    }
}

impl<E> Provenance<VerifyingKey> for Operation<E>
where
    E: Extensions,
{
    fn author(&self) -> VerifyingKey {
        self.header.verifying_key
    }

    fn verify(&self) -> bool {
        self.header.verify()
    }
}

impl<E> Chain<Hash> for Operation<E> {
    fn backlink(&self) -> Option<Hash> {
        self.header.backlink
    }

    fn seq_num(&self) -> SeqNum {
        self.header.seq_num
    }
}

impl<E> Offchain<Hash> for Operation<E> {
    fn payload(&self) -> Option<&Body> {
        self.body.as_ref()
    }

    fn payload_hash(&self) -> Option<Hash> {
        self.header.payload_hash
    }

    fn payload_size(&self) -> PayloadSize {
        self.header.payload_size
    }
}

impl<E> TryFrom<AnyOperation> for Operation<E>
where
    E: Extensions,
{
    type Error = HeaderError;

    fn try_from(any_operation: AnyOperation) -> Result<Self, Self::Error> {
        let header: Header<E> = any_operation.header.try_into()?;
        Ok(Operation {
            header,
            body: any_operation.body,
            hash: any_operation.hash,
        })
    }
}

impl<E> TryFrom<(AnyHeader, Option<Body>)> for Operation<E>
where
    E: Extensions,
{
    type Error = HeaderError;

    fn try_from(value: (AnyHeader, Option<Body>)) -> Result<Self, Self::Error> {
        let (any_header, body) = value;

        // Take the already computed hash from AnyHeader to save some time.
        let hash = any_header.hash();

        // Most fields have already been decoded, at this stage we only need to take the already
        // decoded CBOR values into a Rust type representation.
        let header: Header<E> = any_header.try_into()?;

        Ok(Operation { header, body, hash })
    }
}
