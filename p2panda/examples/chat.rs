// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example E2EE group chat CLI app using the spaces API.
//!
//! ## Usage
//!
//! Run the example in one terminal to create a new chat:
//!
//! `cargo run --example chat`
//!
//! Then using the CHAT_ID output from the first instance for the <CHAT_ID> argument, run as many
//! more instances as you like. The <BOOTSTRAP> argument is optional and only required if
//! discovery should run over the internet. Any other member's MEMBER_ID can be used as a
//! bootstrap.
//!
//! `cargo run --example chat <CHAT_ID> <BOOTSTRAP>`
//!
//! ### Commands
//!
//! ```text
//! # add a member to the chat
//! add <MEMBER_ID>
//!
//! # add a member to the chat with manager rights
//! add <MEMBER_ID> manage
//!
//! # remove a member from the chat
//! remove <MEMBER_ID>
//! ```
use std::collections::VecDeque;
use std::str::FromStr;
use std::thread;

use p2panda::streams::StreamEvent;
use p2panda::{AccessLevel, Hash, RelayUrl, SpaceEvent, Topic, VerifyingKey};
use p2panda_core::IdentityError;
use p2panda_core::test_utils::setup_logging;
use p2panda_core::traits::ShortFormat;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

type Message = String;

const RELAY_URL: &str = "https://euc1-1.relay.n0.iroh.link/.";

const NETWORK_ID: &str = "chat";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    let args: Vec<String> = std::env::args().collect();

    let space_id = if args.len() > 1 {
        let topic = Topic::from_str(&args[1]).map_err(|err| format!("invalid space id: {err}"))?;
        Some(topic)
    } else {
        None
    };

    let bootstrap = if args.len() > 2 {
        let bootstrap =
            VerifyingKey::from_str(&args[2]).map_err(|err| format!("invalid bootstrap: {err}"))?;
        Some(bootstrap)
    } else {
        None
    };

    // Build and spawn the node, adding an optional bootstrap which is required for discovery over
    // the internet with the help of a relay. Any <MEMBER_ID> can be used as the bootstrap.
    let node = {
        let mut builder = p2panda::builder().network_id(Hash::digest(NETWORK_ID).into());

        if let Some(bootstrap) = bootstrap {
            let relay_url: RelayUrl = RELAY_URL.parse().unwrap();
            builder = builder
                .relay_url(relay_url.clone())
                .bootstrap(bootstrap, relay_url);
        }

        builder.spawn().await?
    };

    println!("MEMBER ID: {}", node.id().to_hex());

    // If a space id was provided subscribe to the existing space, otherwise create a new one.
    // Share the id <CHAT ID> with other instances to allow them to join.
    let (space, mut space_rx) = match space_id {
        Some(space_id) => node.space::<Message>(space_id).await?,
        None => {
            let space_id = Topic::random();
            node.create_space::<Message>(space_id).await?
        }
    };

    println!("CHAT ID: {}", space.id().to_hex());

    // Spawn process for handling events arriving on the space stream. Both events resulting from
    // our own actions as well as those from other members are emitted here.
    {
        tokio::task::spawn(async move {
            while let Some(event) = space_rx.next().await {
                match event {
                    // Chat message.
                    StreamEvent::Processed { operation, .. } => {
                        let message = operation.message();
                        println!("{}: {}", operation.author().fmt_short(), message);
                    }
                    // Space membership change.
                    StreamEvent::Space { members, inner, .. } => {
                        if let SpaceEvent::Ejected { .. } = inner {
                            println!("YOU WERE REMOVED");
                            continue;
                        };

                        let members = members
                            .iter()
                            .map(|(member, _)| member.fmt_short())
                            .collect::<Vec<_>>()
                            .join(", ");

                        println!("MEMBERS: [{}]", members)
                    }
                    // Key bundle.
                    StreamEvent::Member(actor) => {
                        println!("KEY BUNDLE RECEIVED: {}", actor)
                    }
                    _ => (),
                }
            }
        });
    }

    // Listen for text input via the terminal.
    let (line_tx, mut line_rx) = mpsc::channel(1);
    thread::spawn(move || input_loop(line_tx));

    // Parse text commands into space membership actions and call related space API methods.
    while let Some(str) = line_rx.recv().await {
        let action = match parse_action(str).await {
            Ok(action) => action,
            Err(err) => {
                println!("invalid command: {err}");
                continue;
            }
        };

        match action {
            // Add a new member to the space.
            Action::Add { member, access } => {
                if let Err(err) = space.add(member, access).await {
                    println!("add member error: {err}");
                }
            }
            // Remove an existing member from the space.
            Action::Remove { member } => {
                if let Err(err) = space.remove(member).await {
                    println!("remove member error: {err}");
                }
            }
            // Publish a message to the space.
            Action::Message(message) => {
                let result = space.publish(message).await;
                let ready = match result {
                    Ok(ready) => ready,
                    Err(err) => {
                        println!("publish message error: {err}");
                        continue;
                    }
                };

                if let Err(err) = ready.await {
                    println!("await ready error: {err}");
                };

                continue;
            }
        };
    }

    Ok(())
}

fn input_loop(line_tx: mpsc::Sender<String>) -> Result<(), std::io::Error> {
    let mut buffer = String::new();
    let stdin = std::io::stdin();
    loop {
        stdin.read_line(&mut buffer)?;
        line_tx
            .blocking_send(buffer.trim().to_string())
            .map_err(|err| std::io::Error::other(err))?;
        buffer.clear();
    }
}

/// Supported space actions.
#[derive(Debug)]
enum Action {
    Add {
        member: VerifyingKey,
        access: AccessLevel,
    },
    Remove {
        member: VerifyingKey,
    },
    Message(String),
}

/// Parse CLI text into an action.
async fn parse_action(str: String) -> Result<Action, ParseActionError> {
    if let Some(str) = str.strip_prefix("add") {
        let str = str.trim();
        let mut args: VecDeque<&str> = str.split(" ").filter(|s| !s.is_empty()).collect();
        let Some(member) = args.pop_front() else {
            return Err(ParseActionError::InvalidArgs(
                "member_id required when adding member to a group".to_string(),
            ));
        };

        let access = parse_access_level(args.pop_front())?;
        Ok(Action::Add {
            member: member.parse()?,
            access,
        })
    } else if let Some(str) = str.strip_prefix("remove") {
        let str = str.trim();
        let mut args: VecDeque<&str> = str.split(" ").filter(|s| !s.is_empty()).collect();
        let Some(member) = args.pop_front() else {
            return Err(ParseActionError::InvalidArgs(
                "member_id required when removing member from a group".to_string(),
            ));
        };
        Ok(Action::Remove {
            member: member.parse()?,
        })
    } else {
        Ok(Action::Message(str))
    }
}

fn parse_access_level(str: Option<&str>) -> Result<AccessLevel, ParseActionError> {
    let access = match str {
        None => AccessLevel::Write,
        Some("manage") => AccessLevel::Manage,
        Some(str) => return Err(ParseActionError::UnknownAccessLevel(str.to_string())),
    };
    Ok(access)
}

#[derive(Debug, Error)]
enum ParseActionError {
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),

    #[error("unknown access level: {0}")]
    UnknownAccessLevel(String),

    #[error(transparent)]
    Identity(#[from] IdentityError),
}
