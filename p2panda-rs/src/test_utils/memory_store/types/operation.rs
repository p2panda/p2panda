// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::{DocumentId, DocumentViewId};
use crate::identity::PublicKey;
use crate::operation::traits::{AsOperation, WithPublicKey};
use crate::operation::{
    Operation, OperationAction, OperationFields, OperationId, OperationVersion,
};
use crate::schema::SchemaId;
use crate::WithId;

/// An operation with it's id and the public key of the keypair which signed it.
#[derive(Debug, Clone)]
pub struct PublishedOperation(
    pub OperationId,
    pub Operation,
    pub PublicKey,
    pub DocumentId,
);

impl WithPublicKey for PublishedOperation {
    /// Returns the public key of the author of this operation.
    fn public_key(&self) -> &PublicKey {
        &self.2
    }
}

impl WithId<OperationId> for PublishedOperation {
    /// Returns the identifier for this operation.
    fn id(&self) -> &OperationId {
        &self.0
    }
}

impl WithId<DocumentId> for PublishedOperation {
    /// Returns the identifier for this operation.
    fn id(&self) -> &DocumentId {
        &self.3
    }
}

impl AsOperation for PublishedOperation {
    /// Returns action type of operation.
    fn action(&self) -> OperationAction {
        self.1.action.to_owned()
    }

    /// Returns schema if of operation.
    fn schema_id(&self) -> SchemaId {
        self.1.schema_id.to_owned()
    }

    /// Returns version of operation.
    fn version(&self) -> OperationVersion {
        self.1.version.to_owned()
    }

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        self.1.fields.clone()
    }

    /// Returns vector of this operation's previous operation ids
    fn previous(&self) -> Option<DocumentViewId> {
        self.1.previous.clone()
    }
}
