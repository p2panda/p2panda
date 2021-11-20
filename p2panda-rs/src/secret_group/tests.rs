// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::secret_group::lts::LongTermSecretEpoch;
use crate::secret_group::mls::MlsProvider;
use crate::secret_group::{SecretGroup, SecretGroupMember};

#[test]
fn long_term_secret_evolution() {
    // ~~~~~~~~~~~~~~
    // Group creation
    // ~~~~~~~~~~~~~~

    // Billie generates a new KeyPair and uses it to create a SecretGroupMember
    let billie_key_pair = KeyPair::new();
    let billie_provider = MlsProvider::new();
    let billie_member = SecretGroupMember::new(&billie_provider, &billie_key_pair);

    // Billie creates a new SecretGroup
    let secret_group_id = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
    let mut billie_group = SecretGroup::new(&billie_provider, &secret_group_id, &billie_member);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Add members & share secrets
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~

    // Ada generates a new KeyPair to also create a new `SecretGroupMember`
    let ada_key_pair = KeyPair::new();
    let ada_provider = MlsProvider::new();
    let ada_member = SecretGroupMember::new(&ada_provider, &ada_key_pair);

    // Ada publishes their KeyPackage for future group invitations
    let ada_key_package = ada_member.key_package(&ada_provider);

    // Billie invites Ada into their group, the return value is a `SecretGroupCommit` which
    // contains the MLS commit and MLS welcome message for this epoch, also the already generated
    // symmetrical long term secret will be encoded, encrypted and included in the same commit
    let group_commit = billie_group.add_members(&billie_provider, &[ada_key_package]);

    // Ada joins the group and decrypts the long term secret
    let mut ada_group = SecretGroup::new_from_welcome(&ada_provider, &group_commit);

    // ~~~~~~~~~~~~~~~~
    // En- & Decryption
    // ~~~~~~~~~~~~~~~~

    // Billie sends an symmetrically encrypted message to Ada, the `LongTermSecrets` will
    // automatically use the latest secret for encryption
    let message_ciphertext =
        billie_group.encrypt_with_long_term_secret(b"This is a secret message");

    // Ada decrypts the message with the known secret
    let message_plaintext = ada_group.decrypt(&ada_provider, &message_ciphertext);
    assert_eq!(b"This is a secret message".to_vec(), message_plaintext);

    // ~~~~~~~~~~~~
    // Late members
    // ~~~~~~~~~~~~

    // Calvin generates a new KeyPair and uses it to create a SecretGroupMember
    let calvin_key_pair = KeyPair::new();
    let calvin_provider = MlsProvider::new();
    let calvin_member = SecretGroupMember::new(&calvin_provider, &calvin_key_pair);

    // Calvin publishes their KeyPackage for future group invitations
    let calvin_key_package = calvin_member.key_package(&calvin_provider);

    // Billie invites Calvin into the group
    let group_commit = billie_group.add_members(&billie_provider, &[calvin_key_package]);

    // Calvin joins the group and decrypts the long term secret
    let mut calvin_group = SecretGroup::new_from_welcome(&calvin_provider, &group_commit);

    // Ada processes the commit as well to sync up with the others
    ada_group.process_commit(&ada_provider, &group_commit);

    // Calvin can still decrypt the old message of billie even though they joined the group later
    let message_plaintext = calvin_group.decrypt(&calvin_provider, &message_ciphertext);
    assert_eq!(b"This is a secret message".to_vec(), message_plaintext);

    // Ada, Billie and Calvin still share only one long term secret
    assert_eq!(ada_group.long_term_epoch(), Some(LongTermSecretEpoch(0)));
    assert_eq!(billie_group.long_term_epoch(), Some(LongTermSecretEpoch(0)));
    assert_eq!(calvin_group.long_term_epoch(), Some(LongTermSecretEpoch(0)));

    // ~~~~~~~~~~~~~~~
    // Secret rotation
    // ~~~~~~~~~~~~~~~

    // Billie removes Calvin and rotates the long term secret before to make sure Calvin will not
    // be able to decrypt future messages
    billie_group.rotate_long_term_secret(&billie_provider);
    let group_commit = billie_group.remove_members(&billie_provider, &[2]);

    // Ada processes this group commit
    ada_group.process_commit(&ada_provider, &group_commit);

    // .. Calvin can't as they are already out of the group
    // @TODO: Test this case after we've introduced proper error handling
    // assert!(calvin_group.process_commit(&calvin_provider, &group_commit).is_err());

    // Only Ada and Billie share the new long term secret
    assert_eq!(ada_group.long_term_epoch(), Some(LongTermSecretEpoch(1)));
    assert_eq!(billie_group.long_term_epoch(), Some(LongTermSecretEpoch(1)));
    assert_eq!(calvin_group.long_term_epoch(), Some(LongTermSecretEpoch(0)));

    // Ada sends a symmetrically encrypted message using the new secret
    let message_ciphertext =
        ada_group.encrypt_with_long_term_secret(b"This is another secret message");

    // Calvin can not decrypt the secret
    // @TODO: Test this case after we've introduced proper error handling
    // assert!(calvin_group.decrypt(&calvin_provider, &message_ciphertext).is_err());

    // Billie can read the message
    let message_plaintext = billie_group.decrypt(&billie_provider, &message_ciphertext);
    assert_eq!(b"This is another secret message".to_vec(), message_plaintext);
}
