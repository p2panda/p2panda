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
    let billie_member = SecretGroupMember::new(&billie_provider, &billie_key_pair).unwrap();

    // Billie creates a new SecretGroup
    let secret_group_id = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
    let mut billie_group =
        SecretGroup::new(&billie_provider, &secret_group_id, &billie_member).unwrap();
    assert!(billie_group.is_active());

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Add members & share secrets
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~

    // Ada generates a new KeyPair to also create a new `SecretGroupMember`
    let ada_key_pair = KeyPair::new();
    let ada_provider = MlsProvider::new();
    let ada_member = SecretGroupMember::new(&ada_provider, &ada_key_pair).unwrap();

    // Ada publishes their KeyPackage for future group invitations
    let ada_key_package = ada_member.key_package(&ada_provider).unwrap();

    // Billie invites Ada into their group, the return value is a `SecretGroupCommit` which
    // contains the MLS commit and MLS welcome message for this epoch, also the already generated
    // symmetrical long term secret will be encoded, encrypted and included in the same commit
    let group_commit = billie_group
        .add_members(&billie_provider, &[ada_key_package])
        .unwrap();
    assert!(group_commit.welcome().is_some());

    // Ada joins the group and decrypts the long term secret
    let mut ada_group = SecretGroup::new_from_welcome(&ada_provider, &group_commit).unwrap();
    assert!(ada_group.is_active());

    // ~~~~~~~~~~~~~~~~
    // En- & Decryption
    // ~~~~~~~~~~~~~~~~

    // Billie sends an symmetrically encrypted message to Ada, the `LongTermSecrets` will
    // automatically use the latest secret for encryption
    let message_ciphertext = billie_group
        .encrypt_with_long_term_secret(&billie_provider, b"This is a secret message")
        .unwrap();

    // Ada decrypts the message with the known secret
    let message_plaintext = ada_group
        .decrypt(&ada_provider, &message_ciphertext)
        .unwrap();
    assert_eq!(b"This is a secret message".to_vec(), message_plaintext);

    // ~~~~~~~~~~~~
    // Late members
    // ~~~~~~~~~~~~

    // Calvin generates a new KeyPair and uses it to create a SecretGroupMember
    let calvin_key_pair = KeyPair::new();
    let calvin_provider = MlsProvider::new();
    let calvin_member = SecretGroupMember::new(&calvin_provider, &calvin_key_pair).unwrap();

    // Calvin publishes their KeyPackage for future group invitations
    let calvin_key_package = calvin_member.key_package(&calvin_provider).unwrap();

    // Billie invites Calvin into the group
    let group_commit = billie_group
        .add_members(&billie_provider, &[calvin_key_package])
        .unwrap();

    // Calvin joins the group and decrypts the long term secret
    let mut calvin_group = SecretGroup::new_from_welcome(&calvin_provider, &group_commit).unwrap();
    assert!(calvin_group.is_active());

    // Ada processes the commit as well to sync up with the others
    ada_group
        .process_commit(&ada_provider, &group_commit)
        .unwrap();

    // Calvin can still decrypt the old message of Billie even though they joined the group later
    let message_plaintext = calvin_group
        .decrypt(&calvin_provider, &message_ciphertext)
        .unwrap();
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
    billie_group
        .rotate_long_term_secret(&billie_provider)
        .unwrap();
    let group_commit = billie_group.remove_members(&billie_provider, &[2]).unwrap();
    assert!(group_commit.welcome().is_none());

    // Ada and Calvin processes this group commit
    ada_group
        .process_commit(&ada_provider, &group_commit)
        .unwrap();

    calvin_group
        .process_commit(&calvin_provider, &group_commit)
        .unwrap();
    assert_eq!(calvin_group.is_active(), false);

    // Only Ada and Billie share the new long term secret
    assert_eq!(ada_group.long_term_epoch(), Some(LongTermSecretEpoch(1)));
    assert_eq!(billie_group.long_term_epoch(), Some(LongTermSecretEpoch(1)));
    assert_eq!(calvin_group.long_term_epoch(), Some(LongTermSecretEpoch(0)));

    // Ada sends a symmetrically encrypted message using the new secret
    let message_ciphertext = ada_group
        .encrypt_with_long_term_secret(&ada_provider, b"This is another secret message")
        .unwrap();

    // Calvin can not decrypt the secret
    assert!(calvin_group
        .decrypt(&calvin_provider, &message_ciphertext)
        .is_err());

    // Billie can read the message
    let message_plaintext = billie_group
        .decrypt(&billie_provider, &message_ciphertext)
        .unwrap();
    assert_eq!(
        b"This is another secret message".to_vec(),
        message_plaintext
    );
}

#[test]
fn sender_ratchet_evolution() {
    // ~~~~~~~~~~~~~~
    // Group creation
    // ~~~~~~~~~~~~~~

    // Billie generates a new KeyPair and uses it to create a SecretGroupMember
    let billie_key_pair = KeyPair::new();
    let billie_provider = MlsProvider::new();
    let billie_member = SecretGroupMember::new(&billie_provider, &billie_key_pair).unwrap();

    // Billie creates a new SecretGroup
    let secret_group_id = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
    let mut billie_group =
        SecretGroup::new(&billie_provider, &secret_group_id, &billie_member).unwrap();
    assert!(billie_group.is_active());

    // ~~~~~~~~~~~
    // Add members
    // ~~~~~~~~~~-

    // Ada generates a new KeyPair to also create a new `SecretGroupMember`
    let ada_key_pair = KeyPair::new();
    let ada_provider = MlsProvider::new();
    let ada_member = SecretGroupMember::new(&ada_provider, &ada_key_pair).unwrap();

    // Ada publishes their KeyPackage for future group invitations
    let ada_key_package = ada_member.key_package(&ada_provider).unwrap();

    // Billie invites Ada into their group
    let group_commit = billie_group
        .add_members(&billie_provider, &[ada_key_package])
        .unwrap();

    // Ada joins the group
    let mut ada_group = SecretGroup::new_from_welcome(&ada_provider, &group_commit).unwrap();
    assert!(ada_group.is_active());

    // ~~~~~~~~~~~~~~~~
    // En- & Decryption
    // ~~~~~~~~~~~~~~~~

    // Billie sends an encrypted message to Ada
    let message_ciphertext = billie_group
        .encrypt(&billie_provider, b"This is a secret message")
        .unwrap();

    // Ada decrypts the message with the known secret
    let message_plaintext = ada_group
        .decrypt(&ada_provider, &message_ciphertext)
        .unwrap();
    assert_eq!(b"This is a secret message".to_vec(), message_plaintext);

    // ~~~~~~~~~~~~
    // Late members
    // ~~~~~~~~~~~~

    // Calvin generates a new KeyPair and uses it to create a SecretGroupMember
    let calvin_key_pair = KeyPair::new();
    let calvin_provider = MlsProvider::new();
    let calvin_member = SecretGroupMember::new(&calvin_provider, &calvin_key_pair).unwrap();

    // Calvin publishes their KeyPackage for future group invitations
    let calvin_key_package = calvin_member.key_package(&calvin_provider).unwrap();

    // Billie invites Calvin into the group
    let group_commit = billie_group
        .add_members(&billie_provider, &[calvin_key_package])
        .unwrap();

    // Calvin joins the group
    let mut calvin_group = SecretGroup::new_from_welcome(&calvin_provider, &group_commit).unwrap();
    assert!(calvin_group.is_active());

    // Ada processes the commit as well to sync up with the others
    ada_group
        .process_commit(&ada_provider, &group_commit)
        .unwrap();

    // Calvin can not decrypt the former message as they were not participating in that epoch
    assert!(calvin_group
        .decrypt(&calvin_provider, &message_ciphertext)
        .is_err());
}
