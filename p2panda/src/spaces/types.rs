// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_store::spaces::SqliteSpacesStore;

use crate::forge::OperationForge;
use crate::operation::Extensions;

/// Control messages do not have an operation Body.
pub type NoBody = ();

/// In the high-level API we don't do anything with auth capabilities (yet).
pub type AuthCapabilities = ();

pub type SpacesArgs = p2panda_spaces::SpacesArgs<AuthCapabilities>;

pub type SpacesStore = p2panda_store::spaces::SqliteSpacesStore<Extensions>;

pub type SpacesProcessor<T> =
    p2panda_stream::spaces::Spaces<T, SpacesStore, OperationForge, AuthCapabilities>;

pub type SpacesManagerError =
    p2panda_spaces::manager::ManagerError<OperationForge, AuthCapabilities>;

pub type InnerGroupError = p2panda_spaces::group::GroupError<OperationForge, AuthCapabilities>;

pub type InnerSpaceError = p2panda_spaces::space::SpaceError<OperationForge, AuthCapabilities>;

pub type SpacesManager = p2panda_spaces::manager::Manager<
    SqliteSpacesStore<Extensions>,
    OperationForge,
    AuthCapabilities,
>;

pub type InnerMember = p2panda_spaces::member::Member;

pub type InnerGroup =
    p2panda_spaces::group::Group<SqliteSpacesStore<Extensions>, OperationForge, AuthCapabilities>;

pub type InnerSpace =
    p2panda_spaces::space::Space<SqliteSpacesStore<Extensions>, OperationForge, AuthCapabilities>;
