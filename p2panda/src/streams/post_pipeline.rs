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

    // Collection of events to be returned from this function.
    //
    // Processing of a spaces event may yield multiple events to be forwarded to the user, while
    // processing any other event will only yield a single user event.
    let mut forward_events = Vec::new();

    process_groups_events(&event, &mut forward_events);

    if let ProcessorStatus::Completed(SpacesResult::Processed { .. }) = event.spaces {
        process_spaces_events(&event, topic, ack_policy, acked, &mut forward_events).await;
    } else {
        process_topics_events(&event, topic, ack_policy, acked, &mut forward_events).await;
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

/// Handle processed spaces events.
async fn process_spaces_events<M>(
    event: &Event,
    topic: Topic,
    ack_policy: AckPolicy,
    acked: &Acked,
    forward_events: &mut Vec<ForwardEvent<M>>,
) where
    M: for<'a> Deserialize<'a> + Send + 'static,
{
    let ProcessorStatus::Completed(SpacesResult::Processed { ref events }) = event.spaces else {
        return;
    };

    // Multiple events can be released at once.
    for space_event in events {
        match space_event {
            p2panda_spaces::Event::Application { space_id: _, data } => {
                handle_application_event(event, topic, ack_policy, acked, data, forward_events)
                    .await;
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

/// Handle processed topic events.
async fn process_topics_events<M>(
    event: &Event,
    topic: Topic,
    ack_policy: AckPolicy,
    acked: &Acked,
    forward_events: &mut Vec<ForwardEvent<M>>,
) where
    M: for<'a> Deserialize<'a> + Send + 'static,
{
    // Only forward topic-stream operations to the application-layer if they have a body.
    match event.body() {
        Some(body) => {
            handle_application_event(
                event,
                topic,
                ack_policy,
                acked,
                body.as_bytes(),
                forward_events,
            )
            .await;
        }
        None => {
            // _Always_ ack system-level events, even if no automatic policy was configured.
            if let Err(error) = acked.ack(event).await {
                forward_events.push(ForwardEvent::topic_stream(StreamEvent::AckFailed {
                    event: event.clone(),
                    error: Arc::new(error),
                }));
            }
        }
    }
}

async fn handle_application_event<M>(
    event: &Event,
    topic: Topic,
    ack_policy: AckPolicy,
    acked: &Acked,
    body: &[u8],
    forward_events: &mut Vec<ForwardEvent<M>>,
) where
    M: for<'a> Deserialize<'a> + Send + 'static,
{
    // Attempt decoding application-layer message. This takes place _after_ system-level
    // processing completed and the operation was ingested.
    //
    // In case decoding fails due to an application bug, users have the option to re-play this
    // persisted operation and attempt decoding again.
    //
    // If application data is malformed users can choose to remove the payload of the operation
    // or delete the whole log altogether.
    debug!(id = %event.operation.hash(), "processing application message");

    // TODO: Is this mixing up concerns? We can only handle bytes on our end and let the users do
    // decoding on application layer?
    //
    // Related issue: <https://github.com/p2panda/p2panda/issues/1072>
    match decode_cbor::<M, _>(body) {
        Ok(message) => {
            // Only ack events automatically if processing or decoding did not fail.
            if ack_policy == AckPolicy::Automatic
                && let Err(error) = acked.ack(event).await
            {
                forward_events.push(ForwardEvent::topic_stream(StreamEvent::AckFailed {
                    event: event.clone(),
                    error: Arc::new(error),
                }));
            }

            let source = event.source.clone();

            forward_events.push(ForwardEvent::topic_stream(StreamEvent::Processed {
                operation: ProcessedOperation {
                    event: event.clone(),
                    topic,
                    acked: acked.clone(),
                    message,
                },
                source,
            }));
        }
        Err(error) => forward_events.push(ForwardEvent::topic_stream(StreamEvent::DecodeFailed {
            event: event.clone(),
            error,
        })),
    }
}
