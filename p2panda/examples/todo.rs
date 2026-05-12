// SPDX-License-Identifier: MIT OR Apache-2.0

//! Simple, collaborative todo list command line application.
//!
//! ## Usage
//!
//! ```text
//! # Start a new todo list, a random id will be generated
//! cargo run --example todo
//!
//! # Join an existing todo list by entering the id
//! cargo run --example todo -- <id>
//!
//! # Type /create <description> to create a new todo list item
//! /create Do laundry
//!
//! # Type /update <id> <description> to update an existing item
//! /update 34af Make a salad
//!
//! # Type /delete <id> to remove an existing item
//! /delete 34af
//!
//! # Print current todo list
//! /show
//! ```
//!
//! ## How does this work?
//!
//! This is an example of how to express a [CRDT], such as an LWW (Last-Write-Wins) and 2P-Set
//! (Two-Phase Set) for deletions on top of the `p2panda` Node API.
//!
//! This is a basic example of an "event sourcing" approach: We are creating "events" triggered by
//! "commands" (see `create`, `update` and `delete` methods) which are then processed (see `process`
//! method). Every processed event changes our internal state, this is also called
//! "materialisation".
//!
//! ```plain
//! [Command: "Update item"] ..
//!     |                                      |       [Event Processor]
//!     v                                      |
//!  [Event: "Update"] -- [Event: "Create"] => | Database w. materialised state:
//!                                            |
//!                                            | {
//!                                            |    <id>: description", ...
//!                                            | }
//! ```
//!
//! We are handling both our own, locally created events and events from remote nodes through the
//! same event processor.
//!
//! ## Can I use this over the Internet?
//!
//! This example only works over LAN, you can consult the `NodeBuilder` API documentation to extend
//! this code with a bootstrap and relay argument, which will allow you to then connect to nodes and
//! sync with them over the Internet.
//!
//! [CRDT]: https://en.wikipedia.org/wiki/Conflict-free_replicated_data_type
use std::collections::HashSet;
use std::str::FromStr;

use futures_util::StreamExt;
use p2panda::streams::StreamEvent;
use p2panda_core::{Hash, Timestamp, Topic};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

type TodoItemId = Hash;

type TodoListId = Topic;

/// Todo list message type which is replicated across nodes in the p2p network.
///
/// Every event describes a change `kind` to a specific todo list item, addressed by the `id` field.
#[derive(Debug, Serialize, Deserialize)]
struct TodoEvent {
    id: TodoItemId,
    kind: TodoEventKind,
}

/// Changes we are applying to a todo list item.
#[derive(Debug, Serialize, Deserialize)]
enum TodoEventKind {
    /// Sets the description of an todo list item.
    ///
    /// If no item exists yet, it will be created, otherwise updated.
    Set { description: String },

    /// Delete todo list item.
    ///
    /// This "tombstones" the item internally.
    Delete,
}

/// Todo list item.
///
/// Representing the materialised state of the application in memory.
#[derive(Clone, Debug)]
struct TodoItem {
    id: TodoItemId,
    description: String,
    timestamp: Timestamp,
}

impl PartialEq for TodoItem {
    fn eq(&self, other: &Self) -> bool {
        // Compare only over id, so when inserted into `HashSet` we can replace existing items by
        // id. We need to implement the same for `Eq` and `Hash`.
        self.id == other.id
    }
}

impl Eq for TodoItem {}

impl std::hash::Hash for TodoItem {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Todo list with items inside.
///
/// Representing the materialised state of the application in memory.
///
/// The 2P-Set (Two-Phase Set) is used to mark "deleted" items in a second set named `tombstoned`.
/// The difference between the two `items` and `tombstoned` sets is the 2P-set CRDT state with
/// "remove-wins" semantics.
struct TodoList {
    id: TodoListId,
    items: HashSet<TodoItem>,
    tombstoned: HashSet<TodoItemId>,
}

impl TodoList {
    pub fn new() -> Self {
        Self::from_id(TodoListId::new())
    }

    pub fn from_id(id: TodoListId) -> Self {
        Self {
            id,
            items: HashSet::new(),
            tombstoned: HashSet::new(),
        }
    }

    pub fn id(&self) -> TodoListId {
        self.id
    }

    pub fn is_empty(&self) -> bool {
        self.items
            .iter()
            .filter(|item| !self.tombstoned.contains(&item.id))
            .count()
            == 0
    }

    pub fn items(&self) -> Vec<&TodoItem> {
        self.items
            .iter()
            .filter(|item| !self.tombstoned.contains(&item.id))
            .collect()
    }

    pub fn find_item_id(&self, prefix: &str) -> Option<TodoItemId> {
        self.items
            .iter()
            .find(|item| {
                !self.tombstoned.contains(&item.id) && item.id.to_hex().starts_with(prefix)
            })
            .map(|item| item.id)
    }

    pub fn create(&mut self, description: &str) -> TodoEvent {
        TodoEvent {
            id: Topic::new().into(),
            kind: TodoEventKind::Set {
                description: description.into(),
            },
        }
    }

    pub fn update(&mut self, id: TodoItemId, description: &str) -> Result<TodoEvent> {
        let Some(item) = self.items.iter().find(|item| item.id == id) else {
            return Err(format!("unknown item with id {id}").into());
        };

        Ok(TodoEvent {
            id: item.id,
            kind: TodoEventKind::Set {
                description: description.into(),
            },
        })
    }

    pub fn delete(&mut self, id: TodoItemId) -> Result<TodoEvent> {
        let Some(item) = self.items.iter().find(|item| item.id == id) else {
            return Err(format!("unknown item with id {id}").into());
        };

        Ok(TodoEvent {
            id: item.id,
            kind: TodoEventKind::Delete,
        })
    }

    pub fn process(&mut self, event: &TodoEvent, timestamp: Timestamp) {
        let item = self.items.iter().find(|item| item.id == event.id).cloned();

        // Ignore event if it is older than our latest write ("last-write wins") or if the addressed
        // item was already tombstoned ("remove-wins").
        if let Some(ref item) = item
            && (item.timestamp > timestamp || self.tombstoned.contains(&item.id))
        {
            return;
        }

        match &event.kind {
            TodoEventKind::Set { description } => {
                println!(
                    "➭ {} todo item with id {}",
                    if item.is_none() { "created" } else { "updated" },
                    event.id,
                );

                // We've checked via the timestamp before that any event here is "later" than the
                // current one, so we can simply replace what was in the state before with the
                // "later" version.
                self.items.replace(TodoItem {
                    id: event.id,
                    description: description.clone(),
                    timestamp,
                });
            }
            TodoEventKind::Delete => {
                println!("➭ deleted todo item with id {}", event.id);

                // Remove item if it exists in our state.
                if let Some(item) = item {
                    self.items.remove(&item);
                }

                // Add item to "removed" set for our 2P-Set (Two-Phase Set) CRDT.
                self.tombstoned.insert(event.id);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Pass in todo list id as an argument to find other nodes interested in the same list. If not
    // set, we are generating a new, random identifier and print it.
    //
    // Usage:
    //
    // ```bash
    // cargo run --example todo -- <todo_list_id>
    // ```
    let args: Vec<String> = std::env::args().collect();

    let mut todo_list = if args.len() > 1 {
        let id = TodoListId::from_str(&args[1])
            .map_err(|err| format!("passed invalid todo list id as argument: {err}"))?;
        TodoList::from_id(id)
    } else {
        TodoList::new()
    };

    // Spawn a p2panda node where all state is persisted in memory. Since we're not adding any
    // bootstrap node and relay server we can't connect over the internet. This example only works
    // on the LAN.
    //
    // Check out our `NodeBuilder` documentation if you want to learn how to add a bootstrap node
    // and relay.
    let node = p2panda::spawn().await?;

    println!("TODO");
    println!("⎯⎯⎯⎯⎯");
    println!("★ todo list id: {}", todo_list.id());
    println!("★ my node id: {}", node.id());
    println!("⎯⎯⎯⎯⎯\n");

    // Establish a publish/subscribe topic stream which will help us to find nodes who are also
    // interested in the same todo list. We will automatically sync all `TodoEvent` messages with
    // these nodes so we can process them.
    let (tx, mut rx) = node.stream::<TodoEvent>(todo_list.id()).await?;

    let (line_tx, mut line_rx) = mpsc::channel(1);
    std::thread::spawn(move || input_loop(line_tx));

    loop {
        tokio::select! {
            biased;

            // Parse user input via stdin. These inputs trigger our "commands" which again will
            // create and publish single events into the topic stream via `tx`.
            //
            // We "prune" on every published event, which will automatically remove all previously
            // published messages (also for other nodes) from us. Since we are working with a
            // LWW-logic, we don't need to keep around old messages, keeping storage usage to a
            // minimum.
            Some(input) = line_rx.recv() => {
                // Create a new todo list item.
                //
                // ```text
                // /create Do laundry
                // ```
                if let Some(description) = input.strip_prefix("/create") {
                    let event = todo_list.create(description.trim());
                    tx.prune(Some(event)).await?;
                }

                // Update an existing todo list item.
                //
                // ```text
                // /update be2a Make a salad
                // ```
                if let Some(value) = input.strip_prefix("/update") {
                    let mut parts = value.split_whitespace();

                    let Some(hash_str) = parts.next() else {
                        println!("✖ err: missing todo item id");
                        continue;
                    };

                    let Some(item_id) = todo_list.find_item_id(hash_str.trim()) else {
                        println!("✖ err: unknown todo item id");
                        continue;
                    };

                    let Some(description) = parts.next() else {
                        println!("✖ err: missing todo item description");
                        continue;
                    };

                    let mut description = description.to_string();
                    while let Some(remainder) = parts.next() {
                        description.push_str(" ");
                        description.push_str(remainder);
                    }

                    match todo_list.update(item_id, description.trim()) {
                        Ok(event) => {
                            tx.prune(Some(event)).await?;
                        }
                        Err(err) => {
                            println!("err: {}", err);
                        }
                    }
                }

                // Delete an existing todo list item.
                //
                // ```text
                // /delete be2a
                // ```
                if let Some(value) = input.strip_prefix("/delete") {
                    let mut parts = value.split_whitespace();

                    let Some(hash_str) = parts.next() else {
                        println!("✖ err: missing todo item id");
                        continue;
                    };

                    let Some(item_id) = todo_list.find_item_id(hash_str.trim()) else {
                        println!("✖ err: unknown todo item id");
                        continue;
                    };

                    match todo_list.delete(item_id) {
                        Ok(event) => {
                            tx.prune(Some(event)).await?;
                        }
                        Err(err) => {
                            println!("✖ err: {err}");
                        }
                    }
                }

                // Print current todo list state.
                //
                // ```text
                // /show
                // ```
                if input.strip_prefix("/show").is_some() {
                    println!("⎯⎯⎯⎯⎯");
                    println!("TODO LIST: {}", todo_list.id());

                    if todo_list.is_empty() {
                        println!(".. no items yet ..");
                    } else {
                        println!("⎯⎯⎯⎯⎯");
                        for item in todo_list.items() {
                            let short_hex = item.id.to_hex()[0..4].to_string();
                            println!("◆ [{}]: {}", short_hex, item.description);
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
                    todo_list.process(operation.message(), operation.timestamp().into());
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
