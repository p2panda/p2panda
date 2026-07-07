// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example group chat CLI app.
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
//! ```
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

use futures_util::StreamExt;
use p2panda::{Hash, RelayUrl};
use p2panda_auth::AccessLevel;
use p2panda_core::test_utils::setup_logging;
use p2panda_core::traits::ShortFormat;
use p2panda_core::{IdentityError, Topic, VerifyingKey};
// @TODO: re-export from p2panda.
use p2panda_spaces::SpaceEvent;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::warn;

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

    let mut node = p2panda::builder().network_id(Hash::digest(NETWORK_ID).into());

    if let Some(bootstrap) = bootstrap {
        let relay_url: RelayUrl = RELAY_URL.parse().unwrap();
        node = node
            .relay_url(relay_url.clone())
            .bootstrap(bootstrap, relay_url);
    }

    let node = node.spawn().await?;

    println!("MEMBER ID: {}", node.id().to_hex());

    let (space, mut space_rx) = match space_id {
        Some(space_id) => node.space::<Message>(space_id).await?,
        None => {
            let space_id = Topic::random();
            node.create_space::<Message>(space_id).await?
        }
    };

    println!("log space id");
    println!("CHAT ID: {}", space.id().to_hex());

    {
        tokio::task::spawn(async move {
            while let Some(event) = space_rx.next().await {
                match event {
                    p2panda::streams::StreamEvent::Processed { operation, .. } => {
                        let message = operation.message();
                        println!("{}: {}", operation.author().fmt_short(), message);
                    }
                    p2panda::streams::StreamEvent::Space(event) => {
                        let members = match event {
                            SpaceEvent::Created { context, .. }
                            | SpaceEvent::Added { context, .. }
                            | SpaceEvent::Removed { context, .. } => context,
                            SpaceEvent::Ejected { .. } => {
                                println!("YOU WERE REMOVED");
                                continue;
                            }
                        }
                        .members
                        .iter()
                        .map(ShortFormat::fmt_short)
                        .collect::<Vec<_>>()
                        .join(", ");
                        println!("MEMBERS: [{}]", members)
                    }
                    _ => (),
                }
            }
        });
    }

    // Listen for text input via the terminal.
    let (line_tx, mut line_rx) = mpsc::channel(1);
    thread::spawn(move || input_loop(line_tx));

    while let Some(str) = line_rx.recv().await {
        let action = match parse_action(str).await {
            Ok(action) => action,
            Err(err) => {
                println!("invalid command: {err:?}");
                continue;
            }
        };

        match action {
            Action::Add { member, access } => {
                if let Err(err) = space.add(member, access).await {
                    warn!("add member error: {err:?}");
                }
            }
            Action::Remove { member } => {
                if let Err(err) = space.remove(member).await {
                    warn!("remove member error: {err:?}");
                }
            }
            Action::Message(message) => {
                let result = space.publish(message).await;
                let ready = match result {
                    Ok(ready) => ready,
                    Err(err) => {
                        warn!("publish message error: {err:?}");
                        continue;
                    }
                };

                if let Err(err) = ready.await {
                    warn!("await ready error: {err:?}");
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
            .blocking_send(buffer.clone())
            .map_err(|err| std::io::Error::other(err))?;
        buffer.clear();
    }
}

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
