// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::encode_cbor;
use p2panda_core::{Hash, Topic, VerifyingKey};

use crate::forge::{Forge, ForgeError, OperationForge};
use crate::operation::{Extensions, LogId};
use crate::spaces::message::SpacesMessage;
use crate::spaces::types::AuthCapabilities;

const KEY_BUNDLE_LOG_ID: &[u8] = b"key_bundle/v1";

const GROUP_CONTROL_MESSAGE: &[u8] = b"group_control/v1";

const SPACE_CONTROL_MESSAGE: &[u8] = b"space_control/v1";

const SPACE_APPLICATION_MESSAGE: &[u8] = b"space_application/v1";

/// This forge maintains space, key-bundle and group logs which are organised independently.
///
/// Space Operations have dependencies to Group and KeyBundle Operations. Through topic-mapping they
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
///     KeyBundle    Group (by ID) | Space (by ID)   (by Space ID)  |
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
/// |   KeyBundle  | Group (by ID)   Space (by ID)   (by Space ID) | |
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

    type Error = ForgeError;

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
        //
        // TODO: We need to apply a topic mapping so our TopicStore is aware of the relationship
        // between key bundles, groups and spaces. Where does this take place? The trick will be to
        // make also other nodes manage this mapping _before_ and _while_ they process spaces.
        //
        // 1. For locally created operations we can do the mapping here.
        // 2. For remote, incoming operations we can do the mapping in the -spaces processor or
        //    after.

        let operation = match args {
            // 1. Key Bundle logs.
            p2panda_spaces::SpacesArgs::KeyBundle { ref key_bundle } => {
                // TODO: Check actual encoding format of key bundle. Will require versioning.
                let bytes = encode_cbor(&key_bundle).expect("serialisation of key bundle to CBOR");

                // Every author maintains their own key bundle log.
                let log_id = LogId::digest(KEY_BUNDLE_LOG_ID);

                // TODO: The key bundle itself should not be in the header to allow deleting it
                // after a while (without breaking the whole key bundle log.
                let extensions = Extensions::builder(log_id).build_space(args);

                // TODO: The associated topic should be the space_id but we don't have this info
                // here (should be passed in the spaces args).
                let topic = Topic::from(Hash::digest(b"space_id"));

                self.create_operation(topic, log_id, Some(bytes), extensions)
                    .await?
            }

            // 2. Group logs.
            p2panda_spaces::SpacesArgs::Auth { group_id, .. } => {
                // Every author maintains their own log of control messages _per_ group.
                let log_id = LogId::digest(&{
                    let mut bytes = Vec::new();
                    bytes.extend_from_slice(group_id.as_bytes());
                    // The group id would be enough to indicate the log id, we hash it here together
                    // with a constant value to prevent possible collisions with logs of same id but
                    // different purpose.
                    bytes.extend_from_slice(GROUP_CONTROL_MESSAGE);
                    bytes
                });

                let extensions = Extensions::builder(log_id).build_space(args);

                // TODO: The associated topic should be the space_id but we don't have this info
                // here (should be passed in the spaces args).
                let topic = Topic::from(Hash::digest(b"space_id"));

                self.create_operation(topic, log_id, None, extensions)
                    .await?
            }

            // 3. Space logs.
            //
            // TODO: These variants hav: a pending naming change in -spaces.
            p2panda_spaces::SpacesArgs::SpaceMembership { space_id, .. }
            | p2panda_spaces::SpacesArgs::SpaceUpdate { space_id, .. } => {
                // Every author maintains their own log of control messages _per_ space.
                let log_id = LogId::digest(&{
                    let mut bytes = Vec::new();
                    bytes.extend_from_slice(space_id.as_bytes());
                    bytes.extend_from_slice(SPACE_CONTROL_MESSAGE);
                    bytes
                });

                let extensions = Extensions::builder(log_id).build_space(args);

                // Associate this log with the space id / topic.
                let topic = Topic::from(space_id);

                self.create_operation(topic, log_id, None, extensions)
                    .await?
            }

            // 4. Application logs.
            //
            // TODO: We likely don't want to forge application messages here _at all_ and rather use
            // an independent (new) method on `Manager` which allows us to encrypt anything (control
            // messages & application messages) against the latest secret _without_ forging
            // something.
            p2panda_spaces::SpacesArgs::Application {
                space_id,
                ref ciphertext,
                ..
            } => {
                // TODO: This should be plaintext. We encrypt _later_ in the processor.
                let body = ciphertext.clone();

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

                self.create_operation(topic, log_id, Some(body), extensions)
                    .await?
            }
        };

        Ok(operation.into())
    }
}
