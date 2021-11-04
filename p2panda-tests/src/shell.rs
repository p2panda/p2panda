use p2panda_rs::message::MessageValue;
use p2panda_rs::tests::utils::{
    create_message, delete_message, fields, new_key_pair, update_message, CHAT_SCHEMA,
};

use p2panda_tests::client::Client;
use p2panda_tests::node::Node;
use p2panda_tests::utils::send_to_node;
use p2panda_tests::utils::Result;

#[macro_use]
extern crate prettytable;
use prettytable::{Cell, Row, Table};

use shi::error::ShiError;
use shi::shell::Shell;
use shi::{cmd, parent};

pub struct ShellState {
    pub node: Node,
    pub authors: Vec<Client>,
    pub current_author: Option<String>,
}

impl ShellState {    
    pub fn author_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(row![Fgbc => "name", "id"]);

        for author in &self.authors {
            table.add_row(row![author.name(), author.public_key()]);
        }
        table
    }

    pub fn entry_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(row!["id", "message"]);

        for entry_data in &self.node.all_entries() {
            let hash = entry_data.entry_encoded();
            let message_value = entry_data
                .message()
                .fields()
                .unwrap()
                .get("message")
                .unwrap()
                .clone();
            let message_string = match message_value {
                MessageValue::Text(str) => str,
                MessageValue::Boolean(_) => todo!(),
                MessageValue::Integer(_) => todo!(),
                MessageValue::Float(_) => todo!(),
                MessageValue::Relation(_) => todo!(),
            };
            table.add_row(row![hash.as_str()[..8], message_string]);
        }
        table
    }
}

fn main() -> Result<()> {
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
                    return Err(ShiError::general(format!(
                        "expected 1 arguments, but got {}",
                        args.len()
                    )));
                }
                let client_name_str = args.get(0).unwrap().to_string();
                let client = Client::new(client_name_str.clone(), new_key_pair());

                match state
                    .authors
                    .iter()
                    .find(|author| author.name() == client_name_str)
                {
                    Some(_) => Err(ShiError::general(format!(
                        "Author with name {} already exists.",
                        client_name_str
                    ))),
                    None => {
                        state.authors.push(client);

                        println!("\n");
                        state.author_table().printstd();
                        Ok("\n".to_string())
                    }
                }
            }
        ),
        cmd!(
            "list",
            "List all authors",
            |state: &mut ShellState, args: &[String]| {
                if args.len() != 0 {
                    return Err(ShiError::general(format!(
                        "expected 0 arguments, but got {}",
                        args.len()
                    )));
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
                    return Err(ShiError::general(format!(
                        "expected 1 arguments, but got {}",
                        args.len()
                    )));
                };

                let client_name_str = args.get(0).unwrap().to_string();
                let client = state
                    .authors
                    .iter()
                    .find(|author| author.name() == client_name_str);

                if client.is_none() {
                    return Err(ShiError::general(format!(
                        "Author with name {} does not exist.",
                        client_name_str
                    )));
                };
                
                state.current_author = Some(client.unwrap().name());
                
                println!("\n");
                println!("You are now {}!", client_name_str);
                Ok("\n".to_string())
            }
        ),
        cmd!("whoami", "Check the current author", |state: &mut ShellState, args: &[String]| {
            if args.len() != 0 {
                return Err(ShiError::general(format!(
                    "expected 0 arguments, but got {}",
                    args.len()
                )));
            }
            
            match &state.current_author {
                Some(author) => {
                    println!("\n");
                    println!("You are {}", author);
                    Ok("\n".to_string())
                },
                None => {
                    println!("\n");
                    println!("No author set");
                    Ok("\n".to_string())
                },
            }
        })
    ))?;

    shell.register(parent!(
        "create",
        cmd!(
            "chat",
            "Publish a message following the simple chat schema",
            |state: &mut ShellState, args: &[String]| {
                if args.len() < 1 {
                    return Err(ShiError::general(
                        "expected chat message string as argument",
                    ));
                }

                if state.current_author.is_none() {
                    return Err(ShiError::general(format!(
                        "No author set, please set the author you wish to publish under via `author set <name>`."
                    )));
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
                state.entry_table().printstd();
                Ok("\n".to_string())
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
                    return Err(ShiError::general(format!(
                        "expected 0 arguments, but got {}",
                        args.len()
                    )));
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
