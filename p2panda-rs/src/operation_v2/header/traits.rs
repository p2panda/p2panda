// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::SIGNATURE_SIZE;
use crate::hash_v2::Hash;
use crate::identity_v2::{PublicKey, Signature};
use crate::operation_v2::header::{HeaderExtension, HeaderVersion};

pub trait AsHeader {
    fn version(&self) -> HeaderVersion;
    fn public_key(&self) -> &PublicKey;
    fn payload_size(&self) -> u64;
    fn payload_hash(&self) -> &Hash;
    fn signature(&self) -> &Signature;
    fn extensions(&self) -> &HeaderExtension;
}

pub trait AsEncodedHeader {
    fn hash(&self) -> Hash;

    fn to_bytes(&self) -> Vec<u8>;

    fn size(&self) -> u64;

    fn unsigned_bytes(&self) -> Vec<u8> {
        let bytes = self.to_bytes();
        let signature_offset = bytes.len() - SIGNATURE_SIZE;
        bytes[..signature_offset].into()
    }

    fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }
}
