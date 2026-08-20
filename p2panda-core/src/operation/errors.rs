// SPDX-License-Identifier: MIT OR Apache-2.0

use thiserror::Error;

use crate::logs::SeqNum;
use crate::operation::Version;

#[derive(Debug, Error)]
pub enum HeaderError {
    #[error("failed decoding CBOR byte string for header: {0}")]
    DecodingHeader(cbor_core::Error),

    #[error("failed decoding CBOR byte string for extensions: {0}")]
    DecodingExtensions(cbor_core::SerdeError),

    #[error("expected CBOR array for header: {0}")]
    UnexpectedHeaderType(cbor_core::Error),

    #[error("missing \"{0}\" field in header")]
    MissingField(&'static str),

    #[error("unexpected \"{0}\" field type for \"{0}\"")]
    UnexpectedFieldType(cbor_core::Error, &'static str),

    #[error("invalid verifying key: {0}")]
    InvalidVerifyingKey(crate::identity::IdentityError),

    #[error("invalid bytes length for \"{0}\", expected {1}, got {2} bytes")]
    InvalidBytesLen(&'static str, usize, usize),

    #[error("operation version {0} is not supported, needs to be <= {1}")]
    UnsupportedVersion(Version, Version),

    #[error("invalid signature")]
    InvalidSignature,

    #[error("unexpected excessive fields in header")]
    ExcessiveFields,

    #[error("didn't expect extensions but header contained excessive field")]
    UnexpectedExtensions,

    #[error("expected extensions but header didn't contain any")]
    MissingExtensions,

    #[error("failed encoding CBOR byte string for extensions: {0}")]
    EncodingExtensions(cbor_core::SerdeError),
}

#[derive(Clone, Debug, Error)]
pub enum OperationError {
    #[error("operation version {0} is not supported, needs to be <= {1}")]
    UnsupportedVersion(Version, Version),

    #[error("operation needs to be signed")]
    MissingSignature,

    #[error("signature does not match claimed public key")]
    SignatureMismatch,

    #[error("sequence number can't be 0 when backlink is given")]
    SeqNumMismatch,

    #[error("payload hash and -size need to be defined together")]
    InconsistentPayloadInfo,

    #[error("needs payload hash in header when body is given")]
    MissingPayloadHash,

    #[error("payload hash and size do not match given body")]
    PayloadMismatch,

    #[error("logs can not contain operations of different authors")]
    TooManyAuthors,

    #[error("expected sequence number {0} but found {1}")]
    SeqNumNonIncremental(SeqNum, SeqNum),

    #[error("expected backlink but none was given")]
    BacklinkMissing,

    #[error("given backlink did not match previous operation")]
    BacklinkMismatch,
}
