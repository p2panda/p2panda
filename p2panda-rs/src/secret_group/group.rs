// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::framing::VerifiableMlsPlaintext;
use openmls::group::{GroupId, MlsMessageIn, MlsMessageOut};
use openmls::prelude::KeyPackage;
use openmls_traits::OpenMlsCryptoProvider;
use tls_codec::{Serialize as TlsSerialize, TlsVecU32};

use crate::hash::Hash;
use crate::identity::Author;
use crate::secret_group::lts::{LongTermSecret, LongTermSecretCiphersuite, LongTermSecretEpoch};
use crate::secret_group::mls::MlsGroup;
use crate::secret_group::{SecretGroupCommit, SecretGroupMember, SecretGroupMessage};

const LTS_EXPORTER_LABEL: &str = "long_term_secret";
const LTS_EXPORTER_LENGTH: usize = 32; // AES256 key

#[derive(Debug)]
pub struct SecretGroup {
    mls_group: MlsGroup,
    long_term_secrets: TlsVecU32<LongTermSecret>,
}

impl SecretGroup {
    // Creation
    // ========

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
            long_term_secrets: Vec::new().into(),
        }
    }

    pub fn new_from_welcome(
        provider: &impl OpenMlsCryptoProvider,
        commit: &SecretGroupCommit,
    ) -> Self {
        let mls_group = MlsGroup::new_from_welcome(
            provider,
            commit
                .welcome()
                .expect("This SecretGroupCommit does not contain a welcome message!"),
        );

        Self {
            mls_group,
            long_term_secrets: Vec::new().into(),
        }
    }

    // Membership
    // ==========

    pub fn add_members(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        key_packages: &[KeyPackage],
    ) -> SecretGroupCommit {
        // Add members
        let (mls_message_out, mls_welcome) = self.mls_group.add_members(provider, key_packages);

        // Process message directly to establish group state for encryption
        self.process_commit_directly(provider, &mls_message_out);

        // Re-Encrypt long term secrets for this group epoch
        let encrypt_long_term_secrets = self.encrypt_long_term_secrets(provider);

        SecretGroupCommit::new(mls_message_out, Some(mls_welcome), encrypt_long_term_secrets)
    }

    pub fn remove_members(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        public_keys: &[Author],
    ) -> SecretGroupCommit {
        // @TODO: Identify leaf indexes based on public keys.
        let member_leaf_indexes: Vec<usize> = vec![];

        // Remove members
        let mls_message_out = self
            .mls_group
            .remove_members(provider, member_leaf_indexes.as_slice());

        // Process message directly to establish group state for encryption
        self.process_commit_directly(provider, &mls_message_out);

        // Re-Encrypt long term secrets for this group epoch
        let encrypt_long_term_secrets = self.encrypt_long_term_secrets(provider);

        SecretGroupCommit::new(mls_message_out, None, encrypt_long_term_secrets)
    }

    // Commits
    // =======

    fn process_commit_directly(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        mls_message_out: &MlsMessageOut,
    ) {
        // Convert "out" to "in" message
        let mls_commit_message = match mls_message_out {
            MlsMessageOut::Plaintext(message) => MlsMessageIn::Plaintext(
                VerifiableMlsPlaintext::from_plaintext(message.clone(), None),
            ),
            _ => panic!("This is not a plaintext message"),
        };

        self.mls_group.process_commit(provider, mls_commit_message);
    }

    pub fn process_commit(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        commit: &SecretGroupCommit,
    ) {
        // @TODO: Process long term secrets as well

        self.mls_group.process_commit(provider, commit.commit());
    }

    // Long Term secrets
    // =================

    pub fn rotate_long_term_secret(&mut self, provider: &impl OpenMlsCryptoProvider) {
        let value = self
            .mls_group
            .export_secret(provider, LTS_EXPORTER_LABEL, LTS_EXPORTER_LENGTH);

        let mut long_term_epoch = self.long_term_epoch();
        long_term_epoch.increment();

        self.long_term_secrets.push(LongTermSecret::new(
            LongTermSecretCiphersuite::PANDA_AES256GCMSIV,
            long_term_epoch,
            value.into(),
        ));
    }

    // Encryption
    // ==========

    fn encrypt_long_term_secrets(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
    ) -> SecretGroupMessage {
        // Encode all long term secrets
        let encoded_secrets = self.long_term_secrets.tls_serialize_detached().unwrap();

        // Encrypt encoded secrets
        self.encrypt(provider, &encoded_secrets)
    }

    pub fn encrypt(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        data: &[u8],
    ) -> SecretGroupMessage {
        let mls_ciphertext = self.mls_group.encrypt(provider, data);
        SecretGroupMessage::MlsApplicationMessage(mls_ciphertext)
    }

    pub fn encrypt_with_long_term_secret(&self, data: &[u8]) -> SecretGroupMessage {
        // @TODO: Get latest long term secret from array and encrypt data with it
        todo!();
    }

    pub fn decrypt(
        &mut self,
        provider: &impl OpenMlsCryptoProvider,
        message: &SecretGroupMessage,
    ) -> Vec<u8> {
        match message {
            SecretGroupMessage::MlsApplicationMessage(ciphertext) => {
                self.mls_group.decrypt(provider, ciphertext.clone())
            }
            _ => {
                todo!();
            }
        }
    }

    // Status
    // ======

    pub fn is_active(&self) -> bool {
        self.mls_group.is_active()
    }

    pub fn group_id(&self) -> Hash {
        let group_id_bytes = self.mls_group.group_id().as_slice().to_vec();
        Hash::new_from_bytes(group_id_bytes).unwrap()
    }

    pub fn long_term_epoch(&self) -> LongTermSecretEpoch {
        todo!();
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use crate::hash::Hash;
    use crate::identity::{Author, KeyPair};
    use crate::secret_group::mls::MlsProvider;

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
        let billie_public_key = Author::try_from(billie_key_pair.public_key().clone()).unwrap();
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
        let ada_public_key = Author::try_from(ada_key_pair.public_key().clone()).unwrap();
        let ada_provider = MlsProvider::new(ada_key_pair);
        let ada_member = SecretGroupMember::new(&ada_provider, &ada_public_key);

        // Ada publishes their KeyPackage for future group invitations
        let ada_key_package = ada_member.key_package(&ada_provider);

        // * `KeyPackage` instance will be created.
        //
        // So far there is no scenario where a `KeyPackage` instance would need to be updated.
        //
        // ```
        // KeyPackage {
        //   mls_key_package: binary(TLS encoded) {
        //      previous_key_package: authorized_relation(KeyPackage),
        //      @TODO ...
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

        // ~~~~~~~~~~~~~~~~
        // En- & Decryption
        // ~~~~~~~~~~~~~~~~

        // Billie sends an symmetrically encrypted message to Ada, the `LongTermSecrets` will
        // automatically use the latest secret for encryption
        let message_ciphertext =
            billie_group.encrypt_with_long_term_secret(b"This is a secret message");

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
}
