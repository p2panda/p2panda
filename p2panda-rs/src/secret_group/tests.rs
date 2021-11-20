use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::secret_group::mls::MlsProvider;
use crate::secret_group::{SecretGroup, SecretGroupMember};

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
    let billie_provider = MlsProvider::new();
    let billie_member = SecretGroupMember::new(&billie_provider, &billie_key_pair);

    // Billie creates a new SecretGroup with themselves as the only member. At this state the
    // group is in epoch 0 and no commit messages are generated since the group is already
    // established on Billies device.
    //
    // A first symmetrical long term secret will be generated for every group.
    let mut billie_group = SecretGroup::new(&billie_provider, &secret_group_id, &billie_member);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Add members & share secrets
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~

    // Ada generates a new key pair to also create a new `SecretGroupMember` instance
    let ada_key_pair = KeyPair::new();
    let ada_provider = MlsProvider::new();
    let ada_member = SecretGroupMember::new(&ada_provider, &ada_key_pair);

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
    //

    // Billie invites Ada into their group, the return value is a `SecretGroupCommit` instance
    // which contains the MLS commit and MLS welcome message for this MLS epoch, also the
    // already generated symmetrical long term secret will be encoded, encrypted and published
    // in the `SecretGroupCommit`.
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
    //
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
