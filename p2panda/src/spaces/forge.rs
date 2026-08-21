// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::encode_cbor;
use p2panda_core::{Hash, Topic, VerifyingKey};
use p2panda_spaces::SpaceId;
use p2panda_store::SqliteError;
use thiserror::Error;

use crate::forge::{Forge, ForgeError, OperationForge};
use crate::operation::{Extensions, LogId};
use crate::spaces::message::SpacesMessage;
use crate::spaces::types::AuthCapabilities;

pub(crate) const MEMBER_CONTROL_MESSAGE: &[u8] = b"member_control/v1";

const GROUP_CONTROL_MESSAGE: &[u8] = b"group_control/v1";

const SPACE_CONTROL_MESSAGE: &[u8] = b"space_control/v1";

const SPACE_APPLICATION_MESSAGE: &[u8] = b"space_application/v1";

/// This forge maintains space, member (key-bundles) and group logs which are organised
/// independently.
///
/// Space operations have dependencies to Group and Member operations. Through topic-mapping they
/// are grouped to allow being synced and processed together.
///
/// ```plain
///
///                  Independently published logs
///                  ============================
///
///
///                                +--------------------------------+
///                                |                  Application   |
///       Member     Group (by ID) | Space (by ID)   (by Space ID)  |
///                                |                                |
///       +---+          +---+     |     +---+          +---+       |
///       +---+          +---+     |     +---+          +---+       |
///         ^              ^       |       ^              ^         |
///         |              |       |       |              |         |
///       +---+          +---+     |     +---+          +---+       |
///       +---+          +---+     |     +---+          +---+       |
///         ^              ^       |       ^              ^         |
///         |              |       |       |              |         |
///       +---+          +---+     |     +---+          +---+       |
///       +---+          +---+     |     +---+          +---+       |
///         ^              ^       |       ^              ^         |
///         |              |       |       |              |         |
///                                |                                |
///                                +--------------------------------+
///
///
///               Grouped by topic mapping => Space Id
///               ====================================
///
///
/// +----------------------------------------------------------------+
/// |              Fully encrypted operations by Space               |
/// |              +-----------------------------------------------+ |
/// |              |                                               | |
/// |              |                                  Application  | |
/// |     Member   | Group (by ID)   Space (by ID)   (by Space ID) | |
/// |              |                                               | |
/// |     +---+    |     +---+           +---+          +---+      | |
/// |     +---+    |     +---+           +---+          +---+      | |
/// |       ^      |       ^               ^              ^        | |
/// |       |      |       |               |              |        | |
/// |     +---+    |     +---+           +---+          +---+      | |
/// |     +---+    |     +---+           +---+          +---+      | |
/// |       ^      |       ^               ^              ^        | |
/// |       |      |       |               |              |        | |
/// |     +---+    |     +---+           +---+          +---+      | |
/// |     +---+    |     +---+           +---+          +---+      | |
/// |       ^      |       ^               ^              ^        | |
/// |       |      |       |               |              |        | |
/// |              |                                               | |
/// |              +-----------------------------------------------+ |
/// +----------------------------------------------------------------+
/// ```
impl p2panda_spaces::Forge<AuthCapabilities> for OperationForge {
    type Message = SpacesMessage;

    type Error = SpacesForgeError;

    fn verifying_key(&self) -> VerifyingKey {
        crate::forge::Forge::verifying_key(self)
    }

    async fn forge(
        &self,
        args: p2panda_spaces::SpacesArgs<AuthCapabilities>,
    ) -> Result<SpacesMessage, Self::Error> {
        // TODO: Do we need to query graph tips here for causal ordering or is this taken care off
        // by -spaces? If yes, are declaring _all_ dependencies really it's concern or only the ones
        // which are relevant to the spaces protocol?
        let operation = match args {
            // 1. Member logs (with associated key bundles).
            p2panda_spaces::SpacesArgs::Member(ref member) => {
                let bytes = encode_cbor(&member).expect("serialisation of key bundle to CBOR");

                // Every member maintains their own log with key bundles inside. Please note that
                // this is _one single log_ across all spaces.
                //
                // New member messages are created automatically by a background task.
                let log_id = member_log_id();

                // TODO: The key bundle itself should not be in the header to allow deleting it
                // after a while (without breaking the whole key bundle log.
                let extensions = Extensions::builder(log_id).build_space(args);

                self.create_operation(None, log_id, Some(bytes), extensions)
                    .await?
            }

            // 2. Group logs.
            p2panda_spaces::SpacesArgs::Group { group_id, .. } => {
                // Every author maintains their own log of control messages _per_ group.
                let log_id = group_log_id(group_id);

                let extensions = Extensions::builder(log_id).build_space(args);

                self.create_operation(None, log_id, None, extensions)
                    .await?
            }

            // 3. Space logs.
            //
            // TODO: These variants have a pending naming change in -spaces.
            p2panda_spaces::SpacesArgs::SpaceMembership { space_id, .. } => {
                // Every author maintains their own log of control messages _per_ space.
                let log_id = space_log_id(space_id);

                let extensions = Extensions::builder(log_id).build_space(args);

                let topic = Topic::from(space_id);

                self.create_operation(Some(topic), log_id, None, extensions)
                    .await?
            }

            p2panda_spaces::SpacesArgs::SpaceUpdate { .. } => {
                // Rotates secrets in the encryption scheme to bring fresh entropy into the group.
                //
                // TODO: Not implemented in -spaces API yet.
                unimplemented!()
            }

            // 4. Application logs.
            p2panda_spaces::SpacesArgs::Application {
                space_id,
                ref ciphertext,
                ..
            } => {
                // TODO: This should be plaintext. We encrypt _later_ in the processor.
                let _body = ciphertext.clone();

                // Every author maintains their own log of application messages _per_ space.
                let log_id = LogId::digest(&{
                    let mut bytes = Vec::new();
                    bytes.extend_from_slice(space_id.as_bytes());
                    bytes.extend_from_slice(SPACE_APPLICATION_MESSAGE);
                    bytes
                });

                let extensions = Extensions::builder(log_id).build_space(args);

                // Associate this log with the space id / topic.
                let topic = Topic::from(space_id);

                self.create_operation(Some(topic), log_id, None, extensions)
                    .await?
            }
        };

        Ok(operation.into())
    }
}

pub(crate) fn member_log_id() -> LogId {
    LogId::digest(MEMBER_CONTROL_MESSAGE)
}

fn space_log_id(space_id: SpaceId) -> LogId {
    LogId::digest(&{
        let mut bytes = Vec::new();
        bytes.extend_from_slice(space_id.as_bytes());
        // The group id would be enough to indicate the log id, we hash it here together with a
        // constant value to prevent possible collisions with logs of same id but different purpose.
        bytes.extend_from_slice(SPACE_CONTROL_MESSAGE);
        bytes
    })
}

pub(crate) fn group_log_id(group_id: VerifyingKey) -> LogId {
    LogId::digest(&{
        let mut bytes = Vec::new();
        bytes.extend_from_slice(group_id.as_bytes());
        // The group id would be enough to indicate the log id, we hash it here together with a
        // constant value to prevent possible collisions with logs of same id but different purpose.
        bytes.extend_from_slice(GROUP_CONTROL_MESSAGE);
        bytes
    })
}

// pub(crate) async fn make_space_group_log_associations(
//     store: &SqliteStore,
//     me: VerifyingKey,
//     space_id: SpaceId,
//     space_group_id: VerifyingKey,
//     groups_message_id: Hash,
// ) -> Result<(), SpacesForgeError> {
//     let Some(groups_operation): Option<Operation> =
//         store.get_operation_tx(&groups_message_id).await?
//     else {
//         return Err(SpacesForgeError::MissingGroupsOperation(groups_message_id));
//     };
//
//     // Associate all group logs for members introduced by this operation.
//     if let Some(SpacesArgs::Group {
//         group_id,
//         group_action: GroupAction::Create { .. },
//         ..
//     }) = groups_operation.header.extensions.spaces_args()
//     {
//         // Every author maintains their own log of control messages _per_ group.
//         let log_id = group_log_id(group_id);
//
//         debug!(
//             topic = space_id.fmt_short(),
//             group_id = group_id.fmt_short(),
//             log_id = Hash::from(log_id.as_bytes()).fmt_short(),
//             "associate group log with space topic"
//         );
//
//         // Associate this topic with our own log for each group. As we assume all actors do this,
//         // then we can rely on performing this association on a "push" basis when we receive group
//         // operations and process them in the pipeline.
//         //
//         // TODO: We only really need to make this association if we were ever group managers (this
//         // is the only case where we would publish operations to this log). We could optimise here
//         // based on that assumption by not always making this association.
//         store
//             .associate(&Topic::from(space_id), &me, &log_id)
//             .await?;
//     };
//
//     let log_id = group_log_id(space_group_id);
//
//     debug!(
//         topic = space_id.fmt_short(),
//         group_id = space_group_id.fmt_short(),
//         log_id = Hash::from(log_id.as_bytes()).fmt_short(),
//         "associate space group log with space topic"
//     );
//
//     // Also associate the spaces group itself.
//     store
//         .associate(&Topic::from(space_id), &me, &log_id)
//         .await?;
//
//     Ok(())
// }

#[derive(Debug, Error)]
pub enum SpacesForgeError {
    #[error(transparent)]
    Sqlite(#[from] SqliteError),

    #[error(transparent)]
    Forge(#[from] ForgeError),

    #[error("missing auth groups operation: {0}")]
    MissingGroupsOperation(Hash),
}
