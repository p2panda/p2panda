// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::secret_group::lts::LongTermSecretCiphersuite;

pub struct LongTermSecretEpoch(pub u64);

pub struct LongTermSecret {
    ciphersuite: LongTermSecretCiphersuite,
    long_term_epoch: LongTermSecretEpoch,
    value: Vec<u8>,
}
