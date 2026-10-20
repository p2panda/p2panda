// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use crate::node::AckPolicy;
use crate::processor::ProcessorStatus;
use crate::streams::acked::Acked;
use crate::streams::stream::{to_group_stream_event, to_stream_event, to_system_event};
use crate::streams::{Event, ForwardEvent, ProcessedOperation, StreamEvent};
use p2panda_core::Topic;
use p2panda_core::cbor::decode_cbor;
use p2panda_core::traits::Digest;
use p2panda_stream::groups::{GroupsProcessorArgs, GroupsResult};
use p2panda_stream::spaces::SpacesResult;
use serde::Deserialize;
use tracing::{debug, warn};

/// Handle the resulting event output coming from the processor pipeline.
///
/// Events are acked and forwarded to application-layer based on the following rules:
///
/// 1. If we forward any processed operation / non-error result to frontend, ack it if policy is set
///    to automatic.
/// 2. Always ack events if never forwarded to frontend.
/// 3. What we don't forward: pruning operations without body.
pub(crate) async fn process_operation_out<M>(
    event: Event,
    topic: Topic,
    ack_policy: AckPolicy,
    acked: &Acked,
) -> Vec<ForwardEvent<M>>
where
    M: for<'a> Deserialize<'a> + Send + 'static,
{
    let source = event.source.clone();

    // If the event failed during processing we don't continue. Nothing gets decoded or acked.
    if let Some(error) = event.failure_reason() {
        warn!(
            id = %event.hash(),
            "processing operation failed: {}",
            error,
        );

        let failure_event = ForwardEvent::topic_stream(StreamEvent::ProcessingFailed {
            event,
            error,
            source,
        });

        return vec![failure_event];
    }

    // Collection of events to be returned from this function which will be forwarded to:
    //
    // 1. Topic-scoped event stream
    // 2. System-level event stream
    let mut forward_events = Vec::new();

    // Convert events from groups and spaces.
    //
    // Processing of a spaces event may yield multiple events to be forwarded to the user, while
    // processing any other event will only yield a single user event.
    process_groups_events(&event, &mut forward_events);
    process_spaces_events(&event, &mut forward_events);

    // Handle application events: Decoding & acking.
    //
    // Most events with an body are meant for the application-layer, except of member's key bundle
    // messages (via spaces).
    //
    // Application-layer bodies can come from two sources:
    //
    // 1. Regular topic-streams
    // 2. Encrypted application messages from spaces-streams
    let application_bytes = application_body(&event);
    if let Some(bytes) = application_bytes {
        let success = decode_application_event(&event, topic, acked, bytes, &mut forward_events);

        // Only ack events automatically if decoding did not fail.
        if success {
            ack_if_automatic(&event, ack_policy, acked, &mut forward_events).await;
        }
    }

    // Always ack events which don't have an application body and are not spaces events.
    if application_bytes.is_none() && !is_spaces_event(&event) {
        always_ack(&event, acked, &mut forward_events).await;
    }

    // Spaces events are forwarded to the application layer.
    if is_spaces_event(&event) {
        ack_if_automatic(&event, ack_policy, acked, &mut forward_events).await;
    }

    forward_events
}

/// Handle processed groups events.
fn process_groups_events<M>(event: &Event, forward_events: &mut Vec<ForwardEvent<M>>) {
    if let ProcessorStatus::Completed(GroupsResult::Processed) = &event.groups
        && let GroupsProcessorArgs::Process { message } = &event.groups_args
    {
        forward_events.push(ForwardEvent::Topic(Box::new(to_group_stream_event(
            event, message,
        ))));
    }
}

fn is_spaces_event(event: &Event) -> bool {
    event.operation.header.extensions.spaces_args().is_some()
}

/// Extracts the body from an event which is meant for the application layer.
fn application_body(event: &Event) -> Option<&[u8]> {
    if let ProcessorStatus::Completed(SpacesResult::Processed { ref events }) = event.spaces {
        for space_event in events {
            if let p2panda_spaces::Event::Application { data, .. } = space_event {
                // Encrypted application body from a space.
                return Some(data);
            } else {
                // Some spaces events can have bodies but they are not meant for the application
                // layer (such as key bundles).
            }
        }
    } else {
        return event.body().map(|body| body.as_bytes());
    }

    None
}

/// Handle processed spaces events.
fn process_spaces_events<M>(event: &Event, forward_events: &mut Vec<ForwardEvent<M>>) {
    let ProcessorStatus::Completed(SpacesResult::Processed { ref events }) = event.spaces else {
        return;
    };

    // Multiple spaces events can be released at once, convert them to stream- or system-events.
    for space_event in events {
        match space_event {
            p2panda_spaces::Event::Application { .. } => {
                // Noop. Here we only handle system events.
            }
            p2panda_spaces::Event::Member(member) => {
                forward_events.push(ForwardEvent::topic_stream(StreamEvent::Member(member.id())))
            }
            p2panda_spaces::Event::Groups(group_event) => forward_events.push(
                ForwardEvent::system(to_system_event(group_event.to_owned())),
            ),
            p2panda_spaces::Event::Spaces(space_event) => forward_events.push(
                ForwardEvent::topic_stream(to_stream_event(space_event.to_owned(), event.clone())),
            ),
        }
    }
}

/// Attempt decoding application-layer message. This takes place _after_ system-level
/// processing completed and the operation was ingested.
///
/// In case decoding fails due to an application bug, users have the option to re-play this
/// persisted operation and attempt decoding again.
///
/// If application data is malformed users can choose to remove the payload of the operation
/// or delete the whole log altogether.
//
// TODO: Is this mixing up concerns? We can only handle bytes on our end and let the users do
// decoding on application layer?
//
// Related issue: <https://github.com/p2panda/p2panda/issues/1072>
fn decode_application_event<M>(
    event: &Event,
    topic: Topic,
    acked: &Acked,
    body: &[u8],
    forward_events: &mut Vec<ForwardEvent<M>>,
) -> bool
where
    M: for<'a> Deserialize<'a> + Send + 'static,
{
    debug!(id = %event.operation.hash(), "processing application message");

    match decode_cbor::<M, _>(body) {
        Ok(message) => {
            forward_events.push(ForwardEvent::topic_stream(StreamEvent::Processed {
                operation: ProcessedOperation {
                    event: event.clone(),
                    topic,
                    acked: acked.clone(),
                    message,
                },
                source: event.source.clone(),
            }));

            true
        }
        Err(error) => {
            forward_events.push(ForwardEvent::topic_stream(StreamEvent::DecodeFailed {
                event: event.clone(),
                error,
            }));

            false
        }
    }
}

async fn ack_if_automatic<M>(
    event: &Event,
    ack_policy: AckPolicy,
    acked: &Acked,
    forward_events: &mut Vec<ForwardEvent<M>>,
) where
    M: for<'a> Deserialize<'a> + Send + 'static,
{
    if ack_policy == AckPolicy::Automatic
        && let Err(error) = acked.ack(event).await
    {
        forward_events.push(ForwardEvent::topic_stream(StreamEvent::AckFailed {
            event: event.clone(),
            error: Arc::new(error),
        }));
    }
}

async fn always_ack<M>(event: &Event, acked: &Acked, forward_events: &mut Vec<ForwardEvent<M>>) {
    if let Err(error) = acked.ack(event).await {
        forward_events.push(ForwardEvent::topic_stream(StreamEvent::AckFailed {
            event: event.clone(),
            error: Arc::new(error),
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use p2panda_auth::group::GroupAction;
    use p2panda_core::cbor::encode_cbor;
    use p2panda_core::{PruneFlag, SigningKey, Topic};
    use p2panda_spaces::{GroupContext, SpaceContext, SpaceEvent};
    use p2panda_store::SqliteStore;
    use p2panda_stream::spaces::SpacesResult;

    use crate::Credentials;
    use crate::forge::{Forge, OperationForge};
    use crate::node::AckPolicy;
    use crate::operation::{Builder, Extensions, LogId};
    use crate::processor::ProcessorStatus;
    use crate::spaces::types::AuthCapabilities;
    use crate::streams::acked::Acked;
    use crate::streams::{Event, ForwardEvent, Source, StreamEvent, StreamFrom};

    use super::process_operation_out;

    struct TestNode {
        topic: Topic,
        forge: OperationForge,
        acked: Acked,
    }

    impl TestNode {
        async fn new() -> Self {
            let topic = Topic::random();
            let store = SqliteStore::temporary().await;
            let forge = OperationForge::new(Credentials::generate(), store.clone());
            let acked = Acked::new(store, topic);

            Self {
                topic,
                forge,
                acked,
            }
        }

        async fn forge_event(&self, body: Option<Vec<u8>>) -> Event {
            let log_id = LogId::from_topic(self.topic);

            let operation = self
                .forge
                .create_operation(
                    Some(self.topic),
                    log_id,
                    body,
                    Extensions::from_topic(self.topic),
                )
                .await
                .unwrap();

            Event::new(
                operation,
                Source::Publish,
                log_id,
                self.topic,
                PruneFlag::new(false),
                None,
                None,
            )
        }

        async fn forge_space_event(&self, body: Option<Vec<u8>>) -> Event {
            let log_id = LogId::from_topic(self.topic);

            let space_id = Topic::random();
            let group_id = SigningKey::generate().verifying_key();

            let space_event = SpaceEvent::<AuthCapabilities>::Created {
                space_id: space_id.into(),
                initial_members: vec![],
                context: SpaceContext {
                    author: self.forge.verifying_key(),
                    group_id: group_id.into(),
                    members: vec![],
                    actors: vec![],
                },
                groups_context: GroupContext {
                    author: self.forge.verifying_key(),
                    actors: vec![],
                    members: vec![],
                    ancestors: vec![],
                    effected_group_members: HashMap::new(),
                    effected_group_actors: HashMap::new(),
                },
            };

            let spaces_args = p2panda_spaces::SpacesArgs::Group {
                group_id,
                group_action: GroupAction::Create {
                    initial_members: vec![],
                },
                auth_dependencies: vec![],
            };

            let operation = self
                .forge
                .create_operation(
                    Some(self.topic),
                    log_id,
                    body,
                    Builder::new(log_id).build_space(spaces_args.clone()),
                )
                .await
                .unwrap();

            let mut event = Event::new(
                operation,
                Source::Publish,
                log_id,
                self.topic,
                PruneFlag::new(false),
                Some(spaces_args),
                None,
            );

            // We don't really process the event, but attach a "fake" result.
            let events = vec![p2panda_spaces::Event::<AuthCapabilities>::Spaces(
                space_event,
            )];
            event.spaces = ProcessorStatus::Completed(SpacesResult::Processed { events });

            event
        }

        async fn forge_prune_event(&self, body: Option<Vec<u8>>) -> Event {
            let log_id = LogId::from_topic(self.topic);

            let operation = self
                .forge
                .create_operation(
                    Some(self.topic),
                    log_id,
                    body,
                    Extensions::from_topic(self.topic),
                )
                .await
                .unwrap();

            Event::new(
                operation,
                Source::Publish,
                log_id,
                self.topic,
                PruneFlag::new(true), // Prune flag is set.
                None,
                None,
            )
        }

        async fn has_nacked_events(&self) -> bool {
            !self
                .acked
                .nacked_log_ranges(StreamFrom::Frontier)
                .await
                .unwrap()
                .is_empty()
        }
    }

    #[tokio::test]
    async fn application_events_automatically_acked() {
        let node = TestNode::new().await;

        let cbor_bytes = encode_cbor(&"Hallo Hallo!").unwrap();
        let event = node.forge_event(Some(cbor_bytes)).await;

        let forwarded =
            process_operation_out::<String>(event, node.topic, AckPolicy::Automatic, &node.acked)
                .await;

        assert_eq!(forwarded.len(), 1);
        assert!(!node.has_nacked_events().await, "was automatically acked");
    }

    #[tokio::test]
    async fn failed_decoding_doesnt_ack() {
        let node = TestNode::new().await;

        // Not really a CBOR string.
        let event = node.forge_event(Some(vec![1, 2, 3])).await;

        // Try to decode as CBOR.
        let mut forwarded =
            process_operation_out::<String>(event, node.topic, AckPolicy::Automatic, &node.acked)
                .await;

        assert_eq!(forwarded.len(), 1);
        if let ForwardEvent::Topic(inner) = forwarded.pop().unwrap() {
            let inner = inner.as_ref();

            if let StreamEvent::DecodeFailed { .. } = inner {
                // Looking good!
            } else {
                panic!("expected decode failed error");
            }
        } else {
            panic!("unexpected forward event type");
        }

        assert!(node.has_nacked_events().await, "do not ack failed events");
    }

    #[tokio::test]
    async fn always_ack_non_forwarded_system_events() {
        let node = TestNode::new().await;

        // Pruning events without bodies are never forwarded to application layer.
        let event = node.forge_prune_event(None).await;

        // .. we should automatically ack them, independent of the given ack policy.
        let forwarded =
            process_operation_out::<Vec<u8>>(event, node.topic, AckPolicy::Explicit, &node.acked)
                .await;

        assert_eq!(forwarded.len(), 0, "not forwarded to application layer");
        assert!(!node.has_nacked_events().await, "was automatically acked");

        // If the prune event has a body, we should forward it.
        let cbor_bytes = encode_cbor(&"Hallo Hallo!").unwrap();
        let event = node.forge_prune_event(Some(cbor_bytes)).await;

        let forwarded =
            process_operation_out::<Vec<u8>>(event, node.topic, AckPolicy::Explicit, &node.acked)
                .await;

        assert_eq!(forwarded.len(), 1);
        assert!(node.has_nacked_events().await, "need to be manually acked");
    }

    #[tokio::test]
    async fn application_events_not_acked_with_explicit_policy() {
        let node = TestNode::new().await;

        let cbor_bytes = encode_cbor(&"Hallo Hallo!").unwrap();
        let event = node.forge_event(Some(cbor_bytes)).await;

        let forwarded =
            process_operation_out::<Vec<u8>>(event, node.topic, AckPolicy::Explicit, &node.acked)
                .await;

        assert_eq!(forwarded.len(), 1);
        assert!(node.has_nacked_events().await, "needs to be manually acked");
    }

    #[tokio::test]
    async fn spaces_events_acked_with_automatic_policy() {
        let node = TestNode::new().await;

        let event = node.forge_space_event(None).await;

        let forwarded =
            process_operation_out::<Vec<u8>>(event, node.topic, AckPolicy::Automatic, &node.acked)
                .await;

        // We expect spaces events to be forwarded to application layer.
        assert_eq!(forwarded.len(), 1);
        assert!(!node.has_nacked_events().await, "was automatically acked");
    }

    #[tokio::test]
    async fn spaces_events_not_acked_with_explicit_policy() {
        let node = TestNode::new().await;

        // Space events (for example members) can have a body.
        let cbor_bytes = encode_cbor(&"Hallo Hallo!").unwrap();
        let event = node.forge_space_event(Some(cbor_bytes)).await;

        let forwarded =
            process_operation_out::<String>(event, node.topic, AckPolicy::Explicit, &node.acked)
                .await;

        assert_eq!(forwarded.len(), 1);
        assert!(node.has_nacked_events().await, "needs to be manually acked");
    }
}
