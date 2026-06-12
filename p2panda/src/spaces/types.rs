// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_spaces::StrongRemoveResolver;
use p2panda_spaces::test_utils::{MemoryStore, TestKeyStore};

use crate::forge::OperationForge;
use crate::spaces::SpaceId;
use crate::spaces::message::SpacesMessage;

/// Control messages do not have an operation Body.
pub type NoBody = ();

/// In the high-level API we don't do anything with auth capabilities (yet).
pub type AuthCapabilities = ();

// TODO: Remove and replace with SQLite implementations when ready.
pub type TestSpacesStore = MemoryStore<SpaceId, SpacesMessage, AuthCapabilities>;

pub type SpacesArgs = p2panda_spaces::SpacesArgs<SpaceId, AuthCapabilities>;

pub type SpacesProcessor<T> = p2panda_stream::spaces::Spaces<
    T,
    SpaceId,
    TestSpacesStore,
    TestKeyStore,
    OperationForge,
    AuthCapabilities,
>;

pub type SpacesManagerError = p2panda_spaces::manager::ManagerError<
    SpaceId,
    TestSpacesStore,
    TestKeyStore,
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;

pub type InnerGroupError = p2panda_spaces::group::GroupError<
    SpaceId,
    TestSpacesStore,
    TestKeyStore,
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;

pub type InnerSpaceError = p2panda_spaces::space::SpaceError<
    SpaceId,
    TestSpacesStore,
    TestKeyStore,
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;

pub type SpacesManager = p2panda_spaces::manager::Manager<
    SpaceId,
    TestSpacesStore,
    TestKeyStore,
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;

pub type InnerMember = p2panda_spaces::member::Member;

pub type InnerGroup = p2panda_spaces::group::Group<
    SpaceId,
    TestSpacesStore,
    TestKeyStore,
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;

pub type InnerSpace = p2panda_spaces::space::Space<
    SpaceId,
    TestSpacesStore,
    TestKeyStore,
    OperationForge,
    AuthCapabilities,
    StrongRemoveResolver<AuthCapabilities>,
>;
