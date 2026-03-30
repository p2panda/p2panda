// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example CLI app for group management.
//!
//! ## Usage
//!
//! Run the example on the first node:
//!
//! `cargo run --example groups`
//!
//! Run the example on a second node, using the topic ID and public key of the first node:
//!
//! `cargo run --example groups -- -t <TOPIC_ID> -b <FIRST_NODE_PUBLIC_KEY>`
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

use anyhow::Result;
use clap::Parser;
use futures_util::StreamExt;
use p2panda_auth::group::{GroupAction, GroupCrdtState, GroupMember};
use p2panda_auth::processor::{GroupsArgs, GroupsOperation};
use p2panda_auth::traits::Operation as GroupsOperationTrait;
use p2panda_auth::{Access, AccessError};
use p2panda_core::{
    Extension, Hash, Header, IdentityError, Operation, PrivateKey, PublicKey, Timestamp, Topic,
};
use p2panda_net::addrs::NodeInfo;
use p2panda_net::iroh_endpoint::{EndpointAddr, from_public_key};
use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net::utils::ShortFormat;
use p2panda_net::{AddressBook, Discovery, Endpoint, Gossip, LogSync, MdnsDiscovery, NodeId};
use p2panda_store::SqliteStore;
use p2panda_store::groups::GroupsStore;
use p2panda_store::topics::TopicStore;
use p2panda_sync::protocols::TopicLogSyncEvent as SyncEvent;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

type LogId = u64;
type GroupsState = GroupCrdtState<PublicKey, Hash, GroupsOperation<()>, ()>;
type GroupsProcessor = p2panda_auth::processor::GroupsProcessor<AppExtensions, LogId>;

/// This application maintains only one log per author, this is why we can hard-code it.
const LOG_ID: LogId = 1;

const RELAY_URL: &str = "https://euc1-1.relay.n0.iroh-canary.iroh.link.";

pub fn setup_logging() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .try_init()
        .ok();
}

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

#[derive(Parser)]
struct Args {
    /// Bootstrap node identifier.
    #[arg(short = 'b', long, value_name = "BOOTSTRAP_ID")]
    bootstrap_id: Option<NodeId>,

    /// Topic identifier.
    #[arg(short = 't', long, value_name = "TOPIC")]
    topic: Option<String>,

    /// Enable mDNS discovery
    #[arg(short = 'm', long, action)]
    mdns: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    let args = Args::parse();

    let private_key = PrivateKey::new();
    let public_key = private_key.public_key();

    // Retrieve the chat topic ID from the provided arguments, otherwise generate a new, random,
    // cryptographically-secure identifier.
    let topic: Topic = if let Some(topic) = args.topic {
        let topic = hex::decode(topic)?;
        topic.try_into().expect("topic id should be 32 bytes")
    } else {
        Topic::new()
    };

    // Set up sync for p2panda operations.
    let store = SqliteStore::temporary().await;
    store.associate(&topic, &public_key, &LOG_ID).await.unwrap();

    // Prepare address book.
    let address_book = AddressBook::builder().spawn().await?;

    // Add a bootstrap node to our address book if one was supplied by the user.
    if let Some(id) = args.bootstrap_id {
        let endpoint_addr = EndpointAddr::new(from_public_key(id));
        let endpoint_addr = endpoint_addr.with_relay_url(RELAY_URL.parse()?);
        address_book
            .insert_node_info(NodeInfo::from(endpoint_addr).bootstrap())
            .await?;
    }

    let endpoint = Endpoint::builder(address_book.clone())
        .private_key(private_key.clone())
        .relay_url(RELAY_URL.parse().unwrap())
        .spawn()
        .await?;

    println!("network id: {}", endpoint.network_id().fmt_short());
    println!("topic id: {}", topic.to_string());
    println!("public key: {}", public_key.to_hex());
    println!("relay url: {}", RELAY_URL);

    let _discovery = Discovery::builder(address_book.clone(), endpoint.clone())
        .spawn()
        .await?;

    let mdns_discovery_mode = if args.mdns {
        MdnsDiscoveryMode::Active
    } else {
        MdnsDiscoveryMode::Passive
    };
    let _mdns = MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
        .mode(mdns_discovery_mode)
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

    // Receive messages from the sync stream.
    {
        let store = store.clone();
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
                        if let Err(err) = GroupsProcessor::process(&topic, &store, &operation).await
                        {
                            println!();
                            println!("error: {err:?}");
                            println!();
                            continue;
                        };

                        store
                            .associate(&topic, &operation.header.public_key, &LOG_ID)
                            .await
                            .unwrap();

                        print_group(&topic, &store, &operation).await;
                    }
                    _ => (),
                }
            }
        });
    }

    // Listen for text input via the terminal.
    let (line_tx, mut line_rx) = mpsc::channel(1);
    std::thread::spawn(move || input_loop(line_tx));

    let mut seq_num = 0;
    let mut backlink = None;

    // Sign and encode each line of text input and broadcast it on the chat topic.
    tokio::task::spawn(async move {
        while let Some(text) = line_rx.recv().await {
            let (group_id, action) = match text_2_action(&topic, &store, public_key, text).await {
                Ok(action) => action,
                Err(err) => {
                    println!();
                    println!("error: {err:?}");
                    println!();
                    continue;
                }
            };

            let y: GroupsState = store.get_state(&topic).await.unwrap().unwrap();

            let groups_args = GroupsArgs {
                group_id,
                action,
                dependencies: y.heads(),
            };

            let (hash, operation) = create_operation(&private_key, seq_num, backlink, groups_args);

            if let Err(err) = GroupsProcessor::process(&topic, &store, &operation).await {
                println!();
                println!("error: {err:?}");
                println!();
                continue;
            };

            print_group(&topic, &store, &operation).await;

            sync_tx.publish(operation).await.unwrap();

            seq_num += 1;
            backlink = Some(hash);
        }
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

fn create_operation(
    private_key: &PrivateKey,
    seq_num: u64,
    backlink: Option<Hash>,
    groups_args: GroupsArgs,
) -> (Hash, Operation<AppExtensions>) {
    let mut header = Header {
        version: 1,
        public_key: private_key.public_key(),
        signature: None,
        payload_size: 0,
        payload_hash: None,
        timestamp: Timestamp::now(),
        seq_num,
        backlink,
        extensions: AppExtensions {
            groups: Some(groups_args),
            log_id: LOG_ID,
        },
    };

    header.sign(private_key);
    let hash = header.hash();

    let operation = Operation {
        hash,
        header: header.clone(),
        body: None,
    };

    (hash, operation)
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
    id: &Topic,
    store: &SqliteStore,
    me: PublicKey,
    text: String,
) -> Result<(PublicKey, GroupAction<PublicKey>), Text2ActionError> {
    let y = store.get_state(id).await.unwrap().unwrap();
    let args = if let Some(_text) = text.strip_prefix("create") {
        let group_id = PrivateKey::new().public_key();
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

        let group_id: PublicKey = group_id.trim().parse()?;

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

        let group_id: PublicKey = group_id.trim().parse()?;
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
) -> Result<GroupMember<PublicKey>, Text2ActionError> {
    let member_id: PublicKey = member_id.trim().parse()?;

    // Check if this member is a group or individual.
    let member = if y.has_group(member_id) {
        GroupMember::Group(member_id)
    } else {
        GroupMember::Individual(member_id)
    };

    Ok(member)
}

async fn print_group(id: &Topic, store: &SqliteStore, operation: &Operation<AppExtensions>) {
    let groups_operation: GroupsOperation = operation.clone().try_into().unwrap();
    let y: GroupsState = store.get_state(id).await.unwrap().unwrap();
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
