// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_spaces::StrongRemoveResolver;
use p2panda_store::spaces::SqliteSpacesStore;

use crate::forge::OperationForge;
use crate::operation::Extensions;

/// Control messages do not have an operation Body.
pub type NoBody = ();

/// In the high-level API we don't do anything with auth capabilities (yet).
pub type AuthCapabilities = ();

pub type SpacesArgs = p2panda_spaces::SpacesArgs<AuthCapabilities>;

pub type SpacesStore = p2panda_store::spaces::SqliteSpacesStore<Extensions>;

pub type SpacesManagerError = p2panda_spaces::manager::ManagerError<
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;

pub type InnerGroupError = p2panda_spaces::group::GroupError<
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;

pub type InnerSpaceError = p2panda_spaces::space::SpaceError<
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;

pub type SpacesManager = p2panda_spaces::manager::Manager<
    SqliteSpacesStore<Extensions>,
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;

pub type InnerMember = p2panda_spaces::member::Member;

pub type InnerGroup = p2panda_spaces::group::Group<
    SqliteSpacesStore<Extensions>,
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;

pub type InnerSpace = p2panda_spaces::space::Space<
    SqliteSpacesStore<Extensions>,
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;
