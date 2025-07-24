// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;

use crate::types::{EncryptionControlMessage, EncryptionDirectMessage};

#[derive(Clone, Debug)]
pub enum EncryptionArgs {
    // @TODO: Here we will fill in the "dependencies", which will be later used by ForgeArgs.
    System {
        control_message: EncryptionControlMessage,
        direct_messages: Vec<EncryptionDirectMessage>,
    },
    Application {
        group_secret_id: GroupSecretId,
        nonce: XAeadNonce,
        ciphertext: Vec<u8>,
    },
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum EncryptionMessage<M> {
    Args(EncryptionArgs),
    Forged(M),
}
