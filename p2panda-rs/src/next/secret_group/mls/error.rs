// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for Messaging Layer Security (MLS).
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum MlsError {
    /// Could not process this type of message.
    #[error("Unexpected message")]
    UnexpectedMessage,

    /// Internal `openmls_memory_keystore` serialisation error.
    // @TODO: This will be changed as soon as we have our own key store implementation.
    #[error("KeyStore failed during serialisation")]
    KeyStoreSerialization,

    /// Validating or creating `Credential` instances failed.
    #[error(transparent)]
    Credential(#[from] openmls::credentials::errors::CredentialError),

    /// Creating a new `KeyPackageBundle` failed.
    #[error(transparent)]
    KeyPackage(#[from] openmls::key_packages::errors::KeyPackageBundleNewError),

    /// Processing a `Welcome` message failed.
    #[error(transparent)]
    Welcome(#[from] openmls::group::errors::WelcomeError),

    /// MLS group state is inconsistence.
    #[error(transparent)]
    GroupState(#[from] openmls::group::errors::MlsGroupStateError),

    /// Could not parse incoming message.
    #[error(transparent)]
    ParseMessage(#[from] openmls::group::errors::ParseMessageError),

    /// Could not process and validate incoming message.
    #[error(transparent)]
    UnverifiedMessage(#[from] openmls::group::errors::UnverifiedMessageError),

    /// Exporting a MLS secret failed.
    #[error(transparent)]
    ExportSecret(#[from] openmls::group::errors::ExportSecretError),

    /// Encrypting new message failed.
    #[error(transparent)]
    CreateMessage(#[from] openmls::group::errors::CreateMessageError),

    /// Creating a new MLS group failed.
    #[error(transparent)]
    NewGroup(#[from] openmls::group::errors::NewGroupError),

    /// Adding members to a MLS group failed.
    #[error(transparent)]
    AddMembers(#[from] openmls::group::errors::AddMembersError),

    /// Removing members from a MLS group failed.
    #[error(transparent)]
    RemoveMembers(#[from] openmls::group::errors::RemoveMembersError),

    /// Critical internal `openmls` library error.
    #[error(transparent)]
    Library(#[from] openmls::error::LibraryError),
}
