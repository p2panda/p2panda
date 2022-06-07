// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for Messaging Layer Security (MLS).
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum MlsError {
    #[error(transparent)]
    Credential(#[from] openmls::credentials::errors::CredentialError),

    #[error(transparent)]
    KeyPackage(#[from] openmls::key_packages::errors::KeyPackageBundleNewError),

    #[error(transparent)]
    Welcome(#[from] openmls::group::errors::WelcomeError),

    #[error(transparent)]
    GroupState(#[from] openmls::group::errors::MlsGroupStateError),

    #[error(transparent)]
    ParseMessage(#[from] openmls::group::errors::ParseMessageError),

    #[error(transparent)]
    UnverifiedMessage(#[from] openmls::group::errors::UnverifiedMessageError),

    #[error(transparent)]
    ExportSecret(#[from] openmls::group::errors::ExportSecretError),

    #[error(transparent)]
    CreateMessage(#[from] openmls::group::errors::CreateMessageError),

    #[error(transparent)]
    NewGroup(#[from] openmls::group::errors::NewGroupError),

    #[error(transparent)]
    AddMembers(#[from] openmls::group::errors::AddMembersError),

    #[error(transparent)]
    RemoveMembers(#[from] openmls::group::errors::RemoveMembersError),

    #[error(transparent)]
    Library(#[from] openmls::error::LibraryError),

    /// Internal `openmls_memory_keystore` serialisation error.
    // @TODO: This will be changed as soon as we have our own key store implementation.
    #[error("KeyStore failed during serialisation")]
    KeyStoreSerialization,
}
