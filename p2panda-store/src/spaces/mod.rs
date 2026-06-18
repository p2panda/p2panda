// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(test)]
mod tests;
mod traits;

use std::borrow::Borrow;

use p2panda_core::traits::{Digest, Provenance};
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteSpacesStore;
pub use traits::{SpacesMessageStore, SpacesStore, SpacesStoreWrite};

use p2panda_core::{Hash, VerifyingKey};

/// Spaces message type with generic parameter for additional arguments.
#[derive(Clone, Debug)]
pub struct SpacesMessage<ARG> {
    pub id: Hash,
    pub author: VerifyingKey,
    pub args: ARG,
}

impl<ARG> Digest<Hash> for SpacesMessage<ARG> {
    fn hash(&self) -> Hash {
        self.id
    }
}

impl<ARG> Provenance<VerifyingKey> for SpacesMessage<ARG> {
    fn author(&self) -> VerifyingKey {
        self.author
    }

    fn verify(&self) -> bool {
        unreachable!("not used by p2panda-spaces")
    }
}

impl<ARG> Borrow<ARG> for SpacesMessage<ARG> {
    fn borrow(&self) -> &ARG {
        &self.args
    }
}
