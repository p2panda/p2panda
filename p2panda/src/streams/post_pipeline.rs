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
) -> Option<Vec<ForwardEvent<M>>>
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

        let failure_event = vec![ForwardEvent::topic_stream(StreamEvent::ProcessingFailed {
            event,
            error,
            source,
        })];

        return Some(failure_event);
    }

    // Collection of events to be returned from this function.
    //
    // Processing of a spaces event may yield multiple events to be forwarded to the user, while
    // processing any other event will only yield a single user event.
    let mut forward_events = Vec::new();

    // Process group processor events.
    if let ProcessorStatus::Completed(GroupsResult::Processed) = &event.groups
        && let GroupsProcessorArgs::Process { message } = &event.groups_args
    {
        forward_events.push(ForwardEvent::Topic(Box::new(to_group_stream_event(
            &event, message,
        ))));
    }

    // Process spaces events.
    if let ProcessorStatus::Completed(SpacesResult::Processed { ref events }) = event.spaces {
        let mut is_system_message = true;

        // Multiple events can be released at once.
        for space_event in events {
            match space_event {
                p2panda_spaces::Event::Application { space_id: _, data } => {
                    is_system_message = false;

                    match decode_cbor::<M, _>(&data[..]) {
                        Ok(message) => {
                            // Only ack events automatically if processing or decoding did not fail.
                            if ack_policy == AckPolicy::Automatic
                                && let Err(error) = acked.ack(&event).await
                            {
                                forward_events.push(ForwardEvent::topic_stream(
                                    StreamEvent::AckFailed {
                                        event: event.clone(),
                                        error: Arc::new(error),
                                    },
                                ));
                            }

                            forward_events.push(ForwardEvent::topic_stream(
                                StreamEvent::Processed {
                                    operation: ProcessedOperation {
                                        event: event.clone(),
                                        topic,
                                        acked: acked.clone(),
                                        message,
                                    },
                                    source: source.clone(),
                                },
                            ));
                        }
                        Err(error) => forward_events.push(ForwardEvent::topic_stream(
                            StreamEvent::DecodeFailed {
                                event: event.clone(),
                                error,
                            },
                        )),
                    }
                }

                p2panda_spaces::Event::Member(member) => forward_events
                    .push(ForwardEvent::topic_stream(StreamEvent::Member(member.id()))),

                p2panda_spaces::Event::Groups(group_event) => forward_events.push(
                    ForwardEvent::system(to_system_event(group_event.to_owned())),
                ),

                p2panda_spaces::Event::Spaces(space_event) => {
                    forward_events.push(ForwardEvent::topic_stream(to_stream_event(
                        space_event.to_owned(),
                        event.clone(),
                    )))
                }
            }

            // _Always_ ack system-level events, even if no automatic policy was configured.
            if is_system_message {
                if let Err(error) = acked.ack(&event).await {
                    return Some(vec![ForwardEvent::topic_stream(StreamEvent::AckFailed {
                        event,
                        error: Arc::new(error),
                    })]);
                }
            }
        }
    } else {
        // Only forward non-spaces operations to the application-layer if they have a body.
        let Some(body) = event.body() else {
            // _Always_ ack system-level events, even if no automatic policy was configured.
            if let Err(error) = acked.ack(&event).await {
                return Some(vec![ForwardEvent::topic_stream(StreamEvent::AckFailed {
                    event,
                    error: Arc::new(error),
                })]);
            }

            return None;
        };

        // Attempt decoding application-layer message. This takes place _after_ system-level
        // processing completed and the operation was ingested.
        //
        // In case decoding fails due to an application bug, users have the option to re-play this
        // persisted operation and attempt decoding again.
        //
        // If application data is malformed users can choose to remove the payload of the operation
        // or delete the whole log altogether.
        //
        // TODO: Is this mixing up concerns? We can only handle bytes on our end and let the users
        // do decoding on application layer?
        debug!(id = %event.operation.hash(), "processing application message");
        match decode_cbor::<M, _>(body.as_bytes()) {
            Ok(message) => {
                // Only ack events automatically if processing or decoding did not fail.
                if ack_policy == AckPolicy::Automatic
                    && let Err(error) = acked.ack(&event).await
                {
                    return Some(vec![ForwardEvent::topic_stream(StreamEvent::AckFailed {
                        event,
                        error: Arc::new(error),
                    })]);
                }

                return Some(vec![ForwardEvent::topic_stream(StreamEvent::Processed {
                    operation: ProcessedOperation {
                        event,
                        topic,
                        acked: acked.clone(),
                        message,
                    },
                    source,
                })]);
            }
            Err(error) => {
                return Some(vec![ForwardEvent::topic_stream(
                    StreamEvent::DecodeFailed { event, error },
                )]);
            }
        }
    }

    Some(forward_events)
}
