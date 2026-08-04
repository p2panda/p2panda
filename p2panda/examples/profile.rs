// SPDX-License-Identifier: MIT OR Apache-2.0

//! Publically readable profiles with write control.
//! Simple auth example - approach 1.
//! This approach lets many users share one topic, but we merge each user's ops to a separate object - they can only edit their own state.
//! It is very simplistic, not using p2panda-auth groups as we should for multi-device support, but done intentionally to expose the core power of the merge function.
//!
//! ## Usage
//!
//! ```text
//! # Start a new todo list, a random id will be generated
//! cargo run --example profile
//!
//! # Join an existing profile list by entering the id
//! cargo run --example profile -- <id>
//!
//! # Type /set-name <name> to set your profile name
//! /set-name Lothar
//!
//! # Type /set-bio <bio> to set your bio
//! /set-bio King of the Hill People
//!
//! # Print current profile list
//! /show
//! ```
//!
//! ## How does this work?
//!
//! This builds on todo and is another example of an "event sourcing" approach: We are creating "events" triggered by
//! "commands" (see `set-name` and `set-bio` methods) which are then processed (see `process`
//! method). Every processed event changes our internal state, this is also called "materialisation".
//!
//! In distinction from the todo example, we do not store one object, but rather one per device identifier (public key).
//! This allows us to provide world-readable data, but grant authorization on each "object" to only one key.
//! Beyond this specific example of auth, it also shows the power of the "merge function" and how it can be customized
//!
//! ```plain
//! [Command: "set-name Bob"] ..
//!     |                                      |       [Event Processor / Merge Function]
//!     v                                      |
//!  [Event: "SetName"]                     => | Database w. materialised state:
//!                                            |
//!                                            | {
//!                                            |    <id>: description", ...
//!                                            | }
//! ```
//!
//! We are handling both our own, locally created events and events from remote nodes through the
//! same event processor.
//!
//! ## Minimal Example
//!
//! In one terminal:
//!
//! ```bash
//! cargo run --example profile
//!
//! /set-name George
//! /set-bio One of a Kind
//! /show
//! ```
//!
//! Keep running and in a second terminal
//!
//! ```bash
//! cargo run --example profile
//!
//! /show # Will show George's info
//! /set-name Fred
//! /set-bio Your favorite guest
//! /show # Will show both info separately
//! ```
//!
//! ## Can I use this over the Internet?
//!
//! This example only works over LAN, you can consult the `NodeBuilder` API documentation to extend
//! this code with a bootstrap and relay argument, which will allow you to then connect to nodes and
//! sync with them over the Internet.
//!
//! [CRDT]: https://en.wikipedia.org/wiki/Conflict-free_replicated_data_type
use std::collections::HashMap;
use std::str::FromStr;

use futures_util::StreamExt;
use p2panda::streams::StreamEvent;
use p2panda_core::{Topic, VerifyingKey};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Profile event type which is replicated across nodes in the p2p network.
///
/// Note that each device's events are written to an append-only log, so we are guaranteed to receive the events from one user
/// in the order they signed them. This is not true for events from multiple users, but in this special cases, we know all writes
/// to one Profile return in a consistent order, so we do not need a Timestamp or other counter to ensure consistent LWW behavior.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
enum ProfileEvent {
    /// Sets the name of this user's profile.
    ///
    /// If no name exists yet, it will be inserted, otherwise updated.
    SetName { name: String },

    /// Sets the bio of this user's profile.
    ///
    /// If no bio exists yet, it will be inserted, otherwise updated.
    SetBio { bio: String },
}

impl ProfileEvent {
    pub fn name(name: impl Into<String>) -> Self {
        ProfileEvent::SetName { name: name.into() }
    }

    pub fn bio(bio: impl Into<String>) -> Self {
        ProfileEvent::SetBio { bio: bio.into() }
    }
}

// impl std::hash::Hash for TodoItem {
//     fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
//         self.id.hash(state);
//     }
// }

/// One User's Profile - both fields may be None if never set. They may later be set back to Some("") but never None again
#[derive(Debug, PartialEq, Eq, Clone, Default)]
struct Profile {
    pub name: Option<String>,
    pub bio: Option<String>,
}

struct ProfileSet(HashMap<VerifyingKey, Profile>);

impl ProfileSet {
    pub fn new() -> Self {
        ProfileSet(HashMap::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&VerifyingKey, &Profile)> {
        self.0.iter()
    }

    pub fn process(&mut self, author: VerifyingKey, event: &ProfileEvent) {
        // Create new profile on the first time it is called, return mutable reference to profile stored in the HashSet
        let profile = match self.0.get_mut(&author) {
            Some(v) => v,
            None => {
                self.0.insert(author, Profile::default());
                self.0.get_mut(&author).unwrap()
            }
        };

        match event {
            ProfileEvent::SetName { name } => {
                println!("Set name for {}", &author);
                profile.name = Some(name.clone());
            }
            ProfileEvent::SetBio { bio } => {
                println!("Set bio for {}", &author);
                profile.bio = Some(bio.clone());
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Pass in topic id as an argument to find other nodes interested in the same list. If not
    // set, we are generating a new, random identifier and print it.
    //
    // Usage:
    //
    // ```bash
    // cargo run --example profile -- <todo_list_id>
    // ```
    let args: Vec<String> = std::env::args().collect();
    let mut profiles = ProfileSet::new();

    let topic = if args.len() > 1 {
        Topic::from_str(&args[1])
            .map_err(|err| format!("passed invalid topic as argument: {err}"))?
    } else {
        Topic::random()
    };

    // Spawn a p2panda node where all state is persisted in memory. Since we're not adding any
    // bootstrap node and relay server we can't connect over the internet. This example only works
    // on the LAN.
    //
    // Check out our `NodeBuilder` documentation if you want to learn how to add a bootstrap node
    // and relay.
    let node = p2panda::spawn().await?;

    println!("PROFILE");
    println!("⎯⎯⎯⎯⎯");
    println!("★ topic id: {}", topic);
    println!("★ my node id: {}", node.id());
    println!("⎯⎯⎯⎯⎯\n");

    // Establish a publish/subscribe topic stream which will help us to find nodes who are also
    // interested in the same todo list. We will automatically sync all `TodoEvent` messages with
    // these nodes so we can process them.
    let (tx, mut rx) = node.stream::<ProfileEvent>(topic).await?;

    let (line_tx, mut line_rx) = mpsc::channel(1);
    std::thread::spawn(move || input_loop(line_tx));

    loop {
        tokio::select! {
            biased;

            // Parse user input via stdin. These inputs trigger our "commands" which again will
            // create and publish single events into the topic stream via `tx`.
            Some(input) = line_rx.recv() => {
                // Set your profile name
                //
                // ```text
                // /set-name Bugs Bunny
                // ```
                if let Some(name) = input.strip_prefix("/set-name") {
                    let event = ProfileEvent::name(name.trim());
                    tx.publish(event).await?;
                }

                // Set your profile bio
                //
                // ```text
                // /set-bio What's Up Doc?
                // ```
                if let Some(bio) = input.strip_prefix("/set-bio") {
                    let event = ProfileEvent::bio(bio.trim());
                    tx.publish(event).await?;
                }

                // Print current profile state.
                //
                // ```text
                // /show
                // ```
                if input.strip_prefix("/show").is_some() {
                    println!("⎯⎯⎯⎯⎯");
                    println!("Profiles: {}", topic);

                    if profiles.is_empty() {
                        println!(".. no items yet ..");
                    } else {
                        println!("⎯⎯⎯⎯⎯");
                        for (node, profile) in profiles.iter() {
                            println!("{}", node);
                            println!("  Name: {}", profile.name.as_deref().unwrap_or(""));
                            println!("  Bio: {}", profile.bio.as_deref().unwrap_or(""));
                        }
                    }

                    println!("⎯⎯⎯⎯⎯");
                }
            }

            // We handle all todo list events through the same processor. This includes a) events
            // received from remote nodes and b) our own, locally created events.
            Some(ref event) = rx.next() => {
                if let StreamEvent::SyncStarted { remote_node_id, incoming_bytes, .. } = event {
                    println!("∇ start sync with node {remote_node_id}, downloading {incoming_bytes} bytes");
                }

                if let StreamEvent::Processed { operation, .. } = event {
                    profiles.process(operation.author(), operation.message());
                }
            }
        }
    }
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
