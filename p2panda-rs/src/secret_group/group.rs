// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::group::{GroupEpoch, GroupId};
use openmls::prelude::KeyPackage;
use openmls_traits::OpenMlsCryptoProvider;

use crate::hash::Hash;
use crate::secret_group::lts::{LongTermSecretEpoch, LongTermSecret};
use crate::secret_group::mls::MlsGroup;
use crate::secret_group::{SecretGroupCommit, SecretGroupMember, SecretGroupMessage};

pub struct SecretGroup {
    mls_group: MlsGroup,
    long_term_secrets: Vec<LongTermSecret>,
}

impl SecretGroup {
    pub fn new(
        provider: &impl OpenMlsCryptoProvider,
        group_instance_id: &Hash,
        member: &SecretGroupMember,
    ) -> Self {
        let init_key_package = member.key_package(provider);
        let group_id = GroupId::from_slice(&group_instance_id.to_bytes());
        let mls_group = MlsGroup::new(provider, group_id, init_key_package);

        Self {
            mls_group,
            long_term_secrets: Vec::new(),
        }
    }

    pub fn new_from_welcome(
        provider: &impl OpenMlsCryptoProvider,
        commit: &SecretGroupCommit,
    ) -> Self {
        todo!();
    }

    // Membership
    // ==========

    pub fn add_members(
        &self,
        provider: &impl OpenMlsCryptoProvider,
        members: &[KeyPackage],
    ) -> SecretGroupCommit {
        todo!();
    }

    pub fn remove_members(
        &self,
        provider: &impl OpenMlsCryptoProvider,
        members: &[KeyPackage],
    ) -> SecretGroupCommit {
        todo!();
    }

    // Commits
    // =======

    pub fn process_commit(
        &self,
        provider: &impl OpenMlsCryptoProvider,
        commit: &SecretGroupCommit,
    ) {
        todo!();
    }

    // Encryption
    // ==========

    pub fn encrypt(
        &self,
        provider: &impl OpenMlsCryptoProvider,
        data: &[u8],
    ) -> SecretGroupMessage {
        todo!();
    }

    pub fn encrypt_with_long_term_secret(
        &self,
        provider: &impl OpenMlsCryptoProvider,
        data: &[u8],
    ) -> SecretGroupMessage {
        todo!();
    }

    pub fn decrypt(
        &self,
        provider: &impl OpenMlsCryptoProvider,
        message: &SecretGroupMessage,
    ) -> Vec<u8> {
        todo!();
    }

    // State
    // =====

    pub fn group_id(&self) -> &Hash {
        todo!();
    }

    pub fn epoch(&self) -> &GroupEpoch {
        todo!();
    }

    pub fn long_term_epoch(&self) -> &LongTermSecretEpoch {
        todo!();
    }
}

#[cfg(test)]
mod tests {
    use crate::secret_group::mls::MlsProvider;
    use crate::{hash::Hash, identity::KeyPair};

    use super::{SecretGroup, SecretGroupMember};

    #[test]
    fn long_term_secret_evolution() {
        // ~~~~~~~~~~~~~~
        // Group creation
        // ~~~~~~~~~~~~~~

        // * `SecretGroup` instance will be created, the resulting hash will be the
        // `secret_group_id`. All future `SecretGroupCommit` instances will relate to a
        // `SecretGroup` instance.
        //
        // So far there is no scenario where a `SecretGroup` instance would need to be updated.
        //
        // ```
        // SecretGroup {
        //   // No fields so far ..
        // }
        // ```
        let secret_group_id = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();

        // Billie generates a new key pair to create a new `SecretGroupMember` instance which holds
        // the MLS credentials based on the given key pair. A `SecretGroupMember` instance can also
        // be used to generate MLS key packages.
        let billie_key_pair = KeyPair::new();
        let billie_public_key = billie_key_pair.public_key().clone();
        let billie_provider = MlsProvider::new(billie_key_pair);
        let billie_member = SecretGroupMember::new(&billie_provider, &billie_public_key);

        // Billie creates a new SecretGroup with themselves as the only member. At this state the
        // group is in epoch 0 and no commit messages are generated since the group is already
        // established on Billies device.
        let mut billie_group = SecretGroup::new(&billie_provider, &secret_group_id, &billie_member);

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~
        // Add members & share secrets
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~

        // Ada generates a new key pair to also create a new `SecretGroupMember` instance
        let ada_key_pair = KeyPair::new();
        let ada_public_key = ada_key_pair.public_key().clone();
        let ada_provider = MlsProvider::new(ada_key_pair);
        let ada_member = SecretGroupMember::new(&billie_provider, &ada_public_key);

        // Ada publishes their KeyPackage for future group invitations
        let ada_key_package = ada_member.key_package(&ada_provider);

        // * `KeyPackage` instance will be created.
        //
        // So far there is no scenario where a `KeyPackage` instance would need to be updated.
        //
        // ```
        // KeyPackage {
        //   mls_key_package: binary(TLS encoded) {
        //      @TODO
        //   }
        // }
        // ```

        // Billie invites Ada into their group, the return value is a `SecretGroupCommit` instance
        // which contains the MLS commit and MLS welcome message for this MLS epoch, also the first
        // symmetrical long term secret will be generated, encrypted and published in the
        // `SecretGroupCommit`.
        //
        // Pseudo code how long term secret generation could look like:
        //
        // ```
        // // Billie creates a new LongTermSecrets instance
        // let mut long_term_secrets = LongTermSecrets::new();
        //
        // // Billie generates a new symmetrical secret and publishes it securely by encrypting it
        // // for all SecretGroup members
        // let secret = group.generate_long_term_secret();
        // long_term_secrets.add_next_epoch(secret);
        //
        // // Export encoded version of all secrets
        // let encoded_secrets = long_term_secrets.export_encoded();
        //
        // // Encrypt encoded secrets with MLS group
        // let ciphertext_secrets = group.encrypt(ciphertext_secrets);
        // ```
        let group_commit = billie_group.add_members(&billie_provider, &[ada_key_package]);

        // * `SecretGroupCommit` instance will be created, pointing at the `SecretGroup` instance.
        // It contains the MLS commit message and optionally also the welcome message which was
        // generated when a new member got invited to the group.
        //
        // Additionally every instance contains the new and all previous secrets, encrypted by the
        // regarding MLS group. Storing the previous secret allows new group members to also
        // decrypt previous data while removed members will not learn about the new introduced
        // long term secret anymore.
        //
        // Future ideas:
        //
        // It is theoretically possible to remove secrets or even reset all secrets during a new
        // MLS group epoch for special scenarios (for example the group decides to not allow any
        // new members to access previous data). The protocol does not prescribe any integrity of
        // the long term secrets, though the default implementation will keep all secrets for now.
        //
        // Also we could introduce `SecretGroupProposals` following the MLS specification which
        // allows non-Group admins to propose adding / removing or updating members in the group.
        // Group admins could then take these proposals into account and write new
        // `SecretGroupCommit` instances addressing them.
        //
        // ```
        // SecretGroupCommit {
        //      secret_group: authorized_relation(SecretGroup),
        //      previous_commit: authorized_relation(SecretGroupCommit),
        //      encrypted_long_term_secrets: binary(encrypted & TLS encoded) [
        //          LongTermSecret {
        //              long_term_epoch: u64,
        //              value: binary(AEAD secret),
        //              ciphersuite: u8(LongTermSecretCiphersuite),
        //          },
        //          LongTermSecret {
        //              ...
        //          },
        //          ...
        //      ],
        //      mls_commit: binary(TLS encoded) {
        //          @TODO
        //      },
        //      ? mls_welcome: binary(TLS encoded) {
        //          @TODO
        //      },
        // }
        // ```

        // Billie downloads and processes the commit to move to the next group epoch where Ada is a
        // member. Ada does the same and will join the group now in this epoch, with the help of
        // the welcome message which is stored inside the `SecretGroupCommit`.
        billie_group.process_commit(&billie_provider, &group_commit);
        // Ada will be able to retreive the group_id from the group_commit. Also they create their
        // own LongTermSecrets state and imports the public, encrypted secret from Billies
        // `SecretGroupCommit` instance.
        let mut ada_group = SecretGroup::new_from_welcome(&ada_provider, &group_commit);
        assert_eq!(ada_group.epoch(), billie_group.epoch());

        // ~~~~~~~~~~~~~~~~
        // En- & Decryption
        // ~~~~~~~~~~~~~~~~

        // Billie sends an symmetrically encrypted message to Ada, the `LongTermSecrets` will
        // automatically use the latest secret for encryption
        let message_ciphertext = billie_group
            .encrypt_with_long_term_secret(&billie_provider, b"This is a secret message");

        // * Any instance will be created, using `message_ciphertext` as a message field
        //
        // ```
        // <Schema> {
        //     <key>: {
        //         long_term_secret: relation(LongTermSecret),
        //         ciphertext: binary(TLS encoded) {
        //             ? long_term_ciphertext: binary(TLS encoded) {
        //                 group_id: GroupId,
        //                 long_term_epoch: u64,
        //                 ciphertext: binary
        //             },
        //             ? mls_application_message: binary(TLS encoded) {
        //                 wire_format: WireFormat,
        //                 group_id: GroupId,
        //                 epoch: u64,
        //                 encrypted_sender_data: binary,
        //                 authenticated_data: binary,
        //                 ciphertext: binary,
        //             },
        //         },
        //     },
        //     ...
        // }
        // ```

        // Ada decrypts the message with the secret, the `LongTermSecrets` will find the right key
        // based on the given secret epoch encoded in the message
        let message_plaintext = ada_group.decrypt(&ada_provider, &message_ciphertext);
        assert_eq!(b"This is a secret message".to_vec(), message_plaintext);

        // ~~~~~~~~~~~~~~~
        // Secret rotation
        // ~~~~~~~~~~~~~~~

        // ...
    }

    // @TODO: Clean this up here
    /* #[test]
    fn encoding() {
        // SymmetricalMessage
        let message = SymmetricalMessage {
            group_id: GroupId::from_slice(b"test"),
            epoch: GroupEpoch(12),
            nonce: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12].into(),
            ciphertext: vec![4, 5, 6].into(),
        };

        let encoded = message.tls_serialize_detached().unwrap();
        let decoded = SymmetricalMessage::tls_deserialize(&mut encoded.as_slice()).unwrap();
        assert_eq!(message, decoded);

        // SymmetricalSecret
        let secret = SymmetricalSecret {
            ciphersuite: SymmetricalCiphersuite::PANDA_AES256GCMSIV,
            epoch: GroupEpoch(12),
            value: vec![4, 12, 3, 6].into(),
        };

        let encoded = secret.tls_serialize_detached().unwrap();
        let decoded = SymmetricalSecret::tls_deserialize(&mut encoded.as_slice()).unwrap();
        assert_eq!(secret, decoded);
    } */
}
