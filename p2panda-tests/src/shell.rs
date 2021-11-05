use p2panda_rs::message::MessageValue;
use p2panda_rs::hash::Hash;
use p2panda_rs::entry::decode_entry;
use p2panda_rs::tests::utils::{
    create_message, delete_message, fields, new_key_pair, update_message, CHAT_SCHEMA,
};

use p2panda_tests::client::Client;
use p2panda_tests::node::Node;
use p2panda_tests::utils::send_to_node;

#[macro_use]
extern crate prettytable;
use prettytable::Table;

use shi::error::ShiError;
use shi::shell::Shell;
use shi::{cmd, parent};

use colored::*;

pub struct ShellState {
    pub node: Node,
    pub authors: Vec<Client>,
    pub current_author: Option<String>,
}

pub fn print_message_ok(msg_str: String) -> Result<String, ShiError> {
    Ok(format!("\n> {}\n", msg_str))
}

pub fn print_message_error(msg_str: String) -> Result<String, ShiError> {
    println!("");
    Err(ShiError::general(format!(
        "> {} {} {}\n",
        "!!".red(),
        msg_str,
        "!!".red()
    )))
}

impl ShellState {
    pub fn author_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(row![Fgbc => "name", "public key"]);

        for author in &self.authors {
            table.add_row(row![author.name(), author.public_key()]);
        }
        table
    }

    pub fn entry_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(row![Fg => "hash", "author", "log id", "seq num", "action", "message"]);

        for entry_data in &self.node.all_entries() {
            let entry = decode_entry(&entry_data.entry_encoded, None).expect("Error decoding signed entry");
            let message = entry_data
                .message();
                
            let message_string = match message.fields() {
                Some(fields) => {
                    let value = fields.get("message")
                        .unwrap()
                        .clone();
                    match value {
                        MessageValue::Text(str) => str,
                        MessageValue::Boolean(_) => todo!(),
                        MessageValue::Integer(_) => todo!(),
                        MessageValue::Float(_) => todo!(),
                        MessageValue::Relation(_) => todo!(),
                    }
                },
                None => "".to_string(),
            };
            let sliced_hash = format!("..{}", entry_data.entry_encoded.hash().as_str()[120..].to_owned());
            let action = match message.action() {
                p2panda_rs::message::MessageAction::Create => "create",
                p2panda_rs::message::MessageAction::Update => "update",
                p2panda_rs::message::MessageAction::Delete => "delete",
            };
            let author = self.authors
            .iter()
            .find(|author| author.public_key() == entry_data.entry_encoded.author().as_str()).unwrap().name();

            table.add_row(row![sliced_hash, author, entry.log_id().as_i64(), entry.seq_num().as_i64(), action, message_string]);
        }
        table
    }

    pub fn instance_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(row![Fg => "id", "author", "message"]);

        for (id, fields) in &self.node.query_all(&CHAT_SCHEMA.to_string()).unwrap() {
            let message_value = fields.get("message").unwrap().clone();
            let message_string = match message_value {
                MessageValue::Text(str) => str,
                MessageValue::Boolean(_) => todo!(),
                MessageValue::Integer(_) => todo!(),
                MessageValue::Float(_) => todo!(),
                MessageValue::Relation(_) => todo!(),
            };
            
            let sliced_hash = format!("..{}", id.as_str()[120..].to_owned());
            
            let instance_author = self.node.get_instance_author(id.to_owned()).unwrap();
            
            let author = self.authors
            .iter()
            .find(|author| author.public_key() == instance_author).unwrap().name();

            table.add_row(row![sliced_hash, author, message_string]);
        }
        table
    }
}

fn main() -> Result<(), ShiError> {
    let state = ShellState {
        node: Node::new(),
        authors: Vec::new(),
        current_author: None,
    };

    let mut shell = Shell::new_with_state("| ", state);

    shell.register(parent!(
        "author",
        cmd!(
            "new",
            "Create new author",
            |state: &mut ShellState, args: &[String]| {
                if args.len() != 1 {
                    return print_message_error(format!(
                        "expected 1 arguments, but got {}",
                        args.len()
                    ));
                }
                let client_name_str = args.get(0).unwrap().to_string();
                let client = Client::new(client_name_str.clone(), new_key_pair());

                match state
                    .authors
                    .iter()
                    .find(|author| author.name() == client_name_str)
                {
                    Some(_) => print_message_error(format!(
                        "Author with name {} already exists.",
                        client_name_str
                    )),
                    None => {
                        state.authors.push(client);
                        state.current_author = Some(client_name_str.clone());

                        println!("\n");
                        state.author_table().printstd();
                        print_message_ok(format!("You are now {}!", client_name_str.green()))
                    }
                }
            }
        ),
        cmd!(
            "list",
            "List all authors",
            |state: &mut ShellState, args: &[String]| {
                if args.len() != 0 {
                    return print_message_error(format!(
                        "expected 0 arguments, but got {}",
                        args.len()
                    ));
                };

                println!("\n");
                state.author_table().printstd();
                Ok("\n".to_string())
            }
        ),
        cmd!(
            "set",
            "Set the author you want to publish entries as",
            |state: &mut ShellState, args: &[String]| {
                if args.len() != 1 {
                    return print_message_error(format!(
                        "expected 1 arguments, but got {}",
                        args.len()
                    ));
                };

                let client_name_str = args.get(0).unwrap().to_string();
                let client = state
                    .authors
                    .iter()
                    .find(|author| author.name() == client_name_str);

                if client.is_none() {
                    return print_message_error(format!(
                        "Author with name {} does not exist.",
                        client_name_str
                    ));
                };

                state.current_author = Some(client.unwrap().name());

                print_message_ok(format!("You are now {}!", client_name_str.green()))
            }
        ),
        cmd!(
            "whoami",
            "Check the current author",
            |state: &mut ShellState, args: &[String]| {
                if args.len() != 0 {
                    return print_message_error(format!(
                        "expected 0 arguments, but got {}",
                        args.len()
                    ));
                }

                match &state.current_author {
                    Some(author) => {
                        print_message_ok(format!("You are {}!", author.green()))}
                    None => {
                        print_message_error(format!("No author set"))}
                }
            }
        )
    ))?;

    shell.register(parent!(
        "create",
        cmd!(
            "chat",
            "Publish a create message following the simple chat schema",
            |state: &mut ShellState, args: &[String]| {
                if args.len() < 1 {
                    return print_message_error(
                        "expected chat message string as argument".to_string()
                    )
                }

                if state.current_author.is_none() {
                    return print_message_error(
                        "No author set, please set the author you wish to publish under via `author set <name>`."
                        .to_string()
                    )
                };

                let client_name_str = state.current_author.clone().unwrap();
                let client = state
                    .authors
                    .iter()
                    .find(|author| author.name() == client_name_str);

                let message = args[0..].join(" ");

                send_to_node(
                    &mut state.node,
                    &client.unwrap(),
                    &create_message(CHAT_SCHEMA.into(), fields(vec![("message", &message)])),
                )
                .unwrap();

                println!("\n");
                state.instance_table().printstd();
                Ok("\n".to_string())
            }
        ),
    ))?;

    shell.register(parent!(
        "update",
        cmd!(
            "chat",
            "Publish an update message following the simple chat schema",
            |state: &mut ShellState, args: &[String]| {
                if args.len() < 2 {
                    return print_message_error(
                        "expected instance id and chat message string as argument".to_string()
                    )
                }

                if state.current_author.is_none() {
                    return print_message_error(
                        "No author set, please set the author you wish to publish under via `author set <name>`."
                        .to_string()
                    )
                };
                
                let instance_hash = Hash::new(&args[0]);
                
                if instance_hash.is_err() {
                    return print_message_error("Invalid hash provided".to_string())
                }
                                
                if state.node.query(&CHAT_SCHEMA.to_string(), &args[0]).is_err() {
                    return print_message_error("No instance matching provided ID exists".to_string())
                };

                let client_name_str = state.current_author.clone().unwrap();
                let client = state
                    .authors
                    .iter()
                    .find(|author| author.name() == client_name_str);

                let message = args[1..].join(" ");

                send_to_node(
                    &mut state.node,
                    &client.unwrap(),
                    &update_message(CHAT_SCHEMA.into(), instance_hash.unwrap(), fields(vec![("message", &message)])),
                )
                .unwrap();

                println!("\n");
                state.instance_table().printstd();
                Ok("\n".to_string())
            }
        ),
    ))?;
    
    shell.register(parent!(
        "delete",
        cmd!(
            "chat",
            "Publish a delete message for a chat schema instance",
            |state: &mut ShellState, args: &[String]| {
                if args.len() < 1 {
                    return print_message_error(
                        "expected instance id as argument".to_string()
                    )
                }

                if state.current_author.is_none() {
                    return print_message_error(
                        "No author set, please set the author you wish to publish under via `author set <name>`."
                        .to_string()
                    )
                };
                
                let instance_hash = Hash::new(&args[0]);
                
                if instance_hash.is_err() {
                    return print_message_error("Invalid hash provided".to_string())
                }
                                
                if state.node.query(&CHAT_SCHEMA.to_string(), &args[0]).is_err() {
                    return print_message_error("No instance matching provided ID exists".to_string())
                };

                let client_name_str = state.current_author.clone().unwrap();
                let client = state
                    .authors
                    .iter()
                    .find(|author| author.name() == client_name_str);

                send_to_node(
                    &mut state.node,
                    &client.unwrap(),
                    &delete_message(CHAT_SCHEMA.into(), instance_hash.unwrap()),
                )
                .unwrap();

                println!("\n");
                state.instance_table().printstd();
                Ok("\n".to_string())
            }
        ),
    ))?;

    shell.register(parent!(
        "instances",
        cmd!(
            "list",
            "List all instances for chat schema",
            |state: &mut ShellState, args: &[String]| {
                if args.len() != 0 {
                    return print_message_error(format!(
                        "expected 0 arguments, but got {}",
                        args.len()
                    ));
                };

                println!("\n");
                state.instance_table().printstd();
                Ok("\n".to_string())
            }
        ),
        cmd!(
            "find",
            "Find a full instance id from a partial",
            |state: &mut ShellState, args: &[String]| {
                if args.len() != 1 {
                    return print_message_error(format!(
                        "expected 1 arguments, but got {}",
                        args.len()
                    ));
                };

                let instances = state.node.query_all(&CHAT_SCHEMA.to_string()).unwrap();
                let instance = instances.iter().find(|(id, _)| id.contains(&args[0])).unwrap();
                print_message_ok(instance.0.to_owned())
            }
        ),

    ))?;

    shell.register(parent!(
        "entries",
        cmd!(
            "list",
            "List all entries",
            |state: &mut ShellState, args: &[String]| {
                if args.len() != 0 {
                    return print_message_error(format!(
                        "expected 0 arguments, but got {}",
                        args.len()
                    ));
                };

                println!("\n");
                state.entry_table().printstd();
                Ok("\n".to_string())
            }
        )
    ))?;

    shell.run()?;

    Ok(())
}
