// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example CLI app for group management.
//!
//! ## Usage
//!
//! Run the example in any number of terminal windows:
//!
//! `cargo run --example groups`
//!
//! ### Commands
//!
//! The public key of the running node is used as the `<MEMBER_PUBLIC_KEY>` for the local actor
//! and is added to any newly created group as a manager member. Valid values for `<ACCESS_LEVEL>`
//! are "pull" | "read" | "write" | "manage".
//!
//! ```
//! # create a group with the local author as manage member
//! create
//!
//! # add a member to an existing group
//! add <MEMBER_PUBLIC_KEY> <GROUP_ID> <ACCESS_LEVEL>
//!
//! # add a member to an existing group
//! remove <MEMBER_PUBLIC_KEY> <GROUP_ID>
//! ```
use std::collections::VecDeque;
use std::thread;

use anyhow::Result;
use futures_util::StreamExt;
use p2panda_auth::group::{GroupAction, GroupCrdtState, GroupMember};
use p2panda_auth::processor::{GroupsArgs, GroupsOperation};
use p2panda_auth::test_utils::setup_logging;
use p2panda_auth::traits::Operation as GroupsOperationTrait;
use p2panda_auth::{Access, AccessError};
use p2panda_core::test_utils::TestLog;
use p2panda_core::{
    Extension, Hash, Header, IdentityError, Operation, SigningKey, Topic, VerifyingKey,
};
use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net::utils::ShortFormat;
use p2panda_net::{AddressBook, Discovery, Endpoint, Gossip, LogSync, MdnsDiscovery};
use p2panda_store::groups::GroupsStore;
use p2panda_store::{SqliteStore, tx_unwrap};
use p2panda_sync::protocols::TopicLogSyncEvent as SyncEvent;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::runtime::Builder;
use tokio::sync::mpsc;
use tokio::task::LocalSet;
use tracing::{debug, info};

type LogId = u64;
type GroupsState = GroupCrdtState<VerifyingKey, Hash, GroupsOperation<()>, ()>;
type GroupsProcessor = p2panda_auth::processor::GroupsProcessor<Topic, AppExtensions, LogId>;

/// This application maintains only one log per author, this is why we can hard-code it.
const LOG_ID: LogId = 1;

/// Identifier for the singleton group state used in this example.
const GROUPS_STATE_ID: u32 = 0;

/// Topic id for this example.
const TOPIC: [u8; 32] = [1; 32];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppExtensions {
    groups: Option<GroupsArgs>,
    log_id: LogId,
}

impl Extension<GroupsArgs> for AppExtensions {
    fn extract(header: &Header<Self>) -> Option<GroupsArgs> {
        header.extensions.groups.clone()
    }
}

impl Extension<LogId> for AppExtensions {
    fn extract(header: &Header<Self>) -> Option<LogId> {
        Some(header.extensions.log_id)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    let signing_key = SigningKey::generate();
    let verifying_key = signing_key.verifying_key();
    let topic = Topic::from(TOPIC);

    // Setup p2panda networking stack.
    let store = SqliteStore::temporary().await;
    let address_book = AddressBook::builder().spawn().await?;

    let endpoint = Endpoint::builder(address_book.clone())
        .signing_key(signing_key.clone())
        .spawn()
        .await?;

    println!("public key: {}", verifying_key.to_hex());

    let _discovery = Discovery::builder(address_book.clone(), endpoint.clone())
        .spawn()
        .await?;

    let _mdns = MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
        .mode(MdnsDiscoveryMode::Active)
        .spawn()
        .await?;

    let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
        .spawn()
        .await?;

    let sync: LogSync<_, LogId, AppExtensions> = LogSync::builder(store.clone(), endpoint, gossip)
        .spawn()
        .await?;

    let sync_tx = sync.stream(topic, true).await?;
    let mut sync_rx = sync_tx.subscribe().await?;

    // Construct groups processor.
    let groups = GroupsProcessor::new(store.clone());

    // Receive messages from the sync stream.
    {
        let store = store.clone();
        let groups = groups.clone();
        tokio::task::spawn(async move {
            while let Some(Ok(from_sync)) = sync_rx.next().await {
                match from_sync.event {
                    SyncEvent::SyncFinished { metrics } => {
                        info!(
                            "finished sync session with {}, bytes received = {}, bytes sent = {}",
                            from_sync.remote.fmt_short(),
                            metrics.received_bytes(),
                            metrics.sent_bytes()
                        );
                    }
                    SyncEvent::OperationReceived { operation, .. } => {
                        if let Err(err) = groups.process(&GROUPS_STATE_ID, &topic, &operation).await
                        {
                            debug!("error: {err:?}");
                            continue;
                        };

                        print_group(&store, &operation).await;
                    }
                    _ => (),
                }
            }
        });
    }

    // Listen for text input via the terminal.
    let (line_tx, mut line_rx) = mpsc::channel(1);
    thread::spawn(move || input_loop(line_tx));

    let rt = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime for current thread");

    // Sign and encode each line of text input and broadcast it on the chat topic.
    thread::spawn(move || {
        let local = LocalSet::new();

        local.spawn_local(async move {
            let log = TestLog::from_signing_key(signing_key);

            while let Some(text) = line_rx.recv().await {
                let (group_id, action) = match text_2_action(&store, verifying_key, text).await {
                    Ok(action) => action,
                    Err(err) => {
                        debug!("error: {err:?}");
                        continue;
                    }
                };

                let y: GroupsState = tx_unwrap!(store, {
                    store
                        .get_groups_state(&GROUPS_STATE_ID)
                        .await
                        .unwrap()
                        .unwrap_or_default()
                });

                let groups_args = GroupsArgs {
                    group_id,
                    action,
                    dependencies: y.heads(),
                };

                let operation = log.operation(
                    &[],
                    AppExtensions {
                        groups: Some(groups_args),
                        log_id: LOG_ID,
                    },
                );

                if let Err(err) = groups.process(&GROUPS_STATE_ID, &topic, &operation).await {
                    debug!("error: {err:?}");
                    continue;
                };

                print_group(&store, &operation).await;

                sync_tx.publish(operation).await.unwrap();
            }
        });

        rt.block_on(local);
    });

    // Listen for `Ctrl+c` and shutdown the node.
    tokio::signal::ctrl_c().await?;

    Ok(())
}

fn input_loop(line_tx: mpsc::Sender<String>) -> Result<()> {
    let mut buffer = String::new();
    let stdin = std::io::stdin();
    loop {
        stdin.read_line(&mut buffer)?;
        line_tx.blocking_send(buffer.clone())?;
        buffer.clear();
    }
}

#[derive(Debug, Error)]
enum Text2ActionError {
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),

    #[error("unknown command: {0}")]
    UnknownCommand(String),

    #[error(transparent)]
    Identity(#[from] IdentityError),

    #[error(transparent)]
    Access(#[from] AccessError),
}

async fn text_2_action(
    store: &SqliteStore,
    me: VerifyingKey,
    text: String,
) -> Result<(VerifyingKey, GroupAction<VerifyingKey>), Text2ActionError> {
    let y = tx_unwrap!(store, {
        store
            .get_groups_state(&GROUPS_STATE_ID)
            .await
            .unwrap()
            .unwrap_or_default()
    });
    let args = if let Some(_text) = text.strip_prefix("create") {
        let group_id = SigningKey::generate().verifying_key();
        (
            group_id,
            GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(me), Access::manage())],
            },
        )
    } else if let Some(text) = text.strip_prefix("add") {
        let text = text.trim();
        let mut args: VecDeque<&str> = text.split(" ").filter(|s| !s.is_empty()).collect();
        let Some(member_id) = args.pop_front() else {
            return Err(Text2ActionError::InvalidArgs(
                "member_id required when adding member to a group".to_string(),
            ));
        };
        let member = parse_member(&y, member_id)?;

        let Some(group_id) = args.pop_front() else {
            return Err(Text2ActionError::InvalidArgs(
                "group_id required when adding member to a group".to_string(),
            ));
        };

        let group_id: VerifyingKey = group_id.trim().parse()?;

        let Some(access) = args.pop_front() else {
            return Err(Text2ActionError::InvalidArgs(
                "access level required for new group member".to_string(),
            ));
        };
        let access: Access = access.parse()?;
        let action = GroupAction::Add { member, access };
        (group_id, action)
    } else if let Some(text) = text.strip_prefix("remove") {
        let text = text.trim();
        let mut args: VecDeque<&str> = text.split(" ").filter(|s| !s.is_empty()).collect();
        let Some(member_id) = args.pop_front() else {
            return Err(Text2ActionError::InvalidArgs(
                "member_id required when removing member from a group".to_string(),
            ));
        };
        let member = parse_member(&y, member_id)?;

        let Some(group_id) = args.pop_front() else {
            return Err(Text2ActionError::InvalidArgs(
                "group_id required when removing member from a group".to_string(),
            ));
        };

        let group_id: VerifyingKey = group_id.trim().parse()?;
        let action = GroupAction::Remove { member };
        (group_id, action)
    } else {
        return Err(Text2ActionError::UnknownCommand(text));
    };

    Ok(args)
}

fn parse_member(
    y: &GroupsState,
    member_id: &str,
) -> Result<GroupMember<VerifyingKey>, Text2ActionError> {
    let member_id: VerifyingKey = member_id.trim().parse()?;

    // Check if this member is a group or individual.
    let member = if y.has_group(member_id) {
        GroupMember::Group(member_id)
    } else {
        GroupMember::Individual(member_id)
    };

    Ok(member)
}

async fn print_group(store: &SqliteStore, operation: &Operation<AppExtensions>) {
    let args = operation.header.extension::<GroupsArgs>().unwrap();
    let groups_operation = GroupsOperation {
        id: operation.hash,
        author: operation.header.verifying_key,
        dependencies: args.dependencies,
        group_id: args.group_id,
        action: args.action,
    };
    let y: GroupsState = tx_unwrap!(store, {
        store
            .get_groups_state(&GROUPS_STATE_ID)
            .await
            .unwrap()
            .unwrap_or_default()
    });
    let members = y
        .members(groups_operation.group_id())
        .into_iter()
        .map(|(member, access)| format!("{}:{}", member.fmt_short(), access))
        .collect::<Vec<_>>()
        .join(", ");

    let action = match groups_operation.action() {
        GroupAction::Create { initial_members } => {
            let members = initial_members
                .into_iter()
                .map(|(member, access)| format!("{}:{}", member.id().fmt_short(), access))
                .collect::<Vec<_>>()
                .join(", ");

            format!(
                "author={}, action=create, initial members=[{}]",
                groups_operation.author().fmt_short(),
                members
            )
        }
        GroupAction::Add { member, access } => {
            format!(
                "author={}, action=add, member={}:{}",
                groups_operation.author().fmt_short(),
                member.id().fmt_short(),
                access
            )
        }
        GroupAction::Remove { member } => {
            format!(
                "author={}, action=remove, member={}",
                groups_operation.author().fmt_short(),
                member.id().fmt_short()
            )
        }
        _ => unimplemented!(),
    };

    println!();
    println!("group id : {}", groups_operation.group_id());
    println!("action   : {action}");
    println!("members  : [{members}]");
    println!();
}
