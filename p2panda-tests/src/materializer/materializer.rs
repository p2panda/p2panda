// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use p2panda_rs::message::{Message, MessageFields};

use crate::logs::LogEntry;
use crate::materializer::DAG;
use crate::node::utils::Result;

/// A wrapper type representing a HashMap of instances stored by Instance id.
type Instances = HashMap<String, MessageFields>;

/// A wrapper type representing a materialized database of Instances stored by Schema hash.
/// We lose Author data during materialization in this demo app...
type SchemaDatabase = HashMap<String, Instances>;

/// Struct which can process multiple append only logs of p2panda Entries, published by multiple Authors
/// and which might contain conncurent updates (forks). All logs are arranged into DAGs before being topologically sorted
/// Concurrent edits are resloved in a last-writer-wins, the order of writes being decided by alphabetically ordering
/// Entries by their hash.
pub struct Materializer {
    // The final data structure where materialized instances are stored
    data: SchemaDatabase,
    // Messages stored by Entry hash
    messages: HashMap<String, Message>,
    // DAGs stored by Instance id
    dags: HashMap<String, DAG>,
}

impl Materializer {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            messages: HashMap::new(),
            dags: HashMap::new(),
        }
    }

    // Get the materialized Instances
    pub fn data(&self) -> SchemaDatabase {
        self.data.clone()
    }

    // Get all Instance DAGs
    pub fn dags(&self) -> HashMap<String, DAG> {
        self.dags.clone()
    }

    /// Store messages
    pub fn store_messages(&mut self, entries: Vec<LogEntry>) {
        entries.iter().for_each(|entry| {
            self.messages.insert(entry.id(), entry.message());
        });
    }

    // Take an array of entries from a single author with multiple schema logs. Creates an update path DAG for
    // each instance of and also stores a list of all messages for materialization which takes place
    // in the next step.
    pub fn build_dags(&mut self, entries: Vec<LogEntry>) {
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        // Build instance DAGs
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        // Loop over remaining entries storing each message and building our dag
        entries.iter().for_each(|entry| {
            // If message.id() is None this is a CREATE message and
            // we need to set the instance_id manually
            let instance_id = match entry.message().id() {
                Some(id) => id.as_str().to_owned(),
                None => entry.id(),
            };

            // Check if this instance DAG already exists, create it if not
            if !self.dags.contains_key(&instance_id) {
                self.dags.insert(instance_id.clone(), DAG::new());
            }

            // Retrieve the instance DAG
            let dag = self.dags.get_mut(&instance_id).unwrap();

            // Create an edge for this message in the DAG, if it is a CREATE message
            // then it should be a root node.
            if entry.message().is_create() {
                dag.add_root(entry.id());
            } else {
                dag.add_edge(entry.instance_backlink().unwrap(), entry.id());
            }
        });
    }

    /// Apply changes to an instance from an ordered list of entries
    pub fn apply_instance_messages(&mut self, entries: Vec<String>, instance_id: String) {
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        // Materialize instances
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        // loop the ordered list of messages
        for entry_id in entries {
            // Get the actual message content by id
            let message = self.messages.get(&entry_id).unwrap();

            // Get the schema string
            let schema_str = message.schema().as_str();

            // Create schema map for instances if it doesn't exist
            if !self.data().contains_key(schema_str) {
                self.data.insert(schema_str.into(), Instances::new());
            }

            // Get all instances for this schema
            let instances = self.data.get_mut(schema_str).unwrap();

            // Materialize all messages in order!! Currently an UPDATE message replaces all
            // fields in the message. I guess we don't want this behaviour eventually.

            // If CREATE message insert new instance
            if message.is_create() {
                instances.insert(instance_id.to_owned(), message.fields().unwrap().to_owned());
            }

            // If UPDATE message update existing instance
            if message.is_update() {
                instances.insert(instance_id.to_owned(), message.fields().unwrap().to_owned());
            }

            // If DELETE message delete existing instance
            if message.is_delete() {
                instances.remove(&instance_id);
            }
        }
    }

    /// Materialize entries from multiple authors and schema logs into a database of Instancess
    pub fn materialize(&mut self, entries: &Vec<LogEntry>) -> Result<SchemaDatabase> {
        // Store all messages ready for processing after conflict resolution
        self.store_messages(entries.clone());

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        // Build DAGs for each Instances
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        // Process entries ready for ordering and materialization
        self.build_dags(entries.to_owned());

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        // Resolve conflicts and Materialize
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        // Loop over all instance DAGs
        for (instance_id, mut dag) in self.dags() {
            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            // Topologically sort instance DAGs
            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

            // Walk the graph depth first, creating a topological ordering of messages
            let ordered_messages = dag.topological();

            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            // Materialize instances
            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

            self.apply_instance_messages(ordered_messages, instance_id);
        }
        Ok(self.data())
    }

    /// Very raw POC methods, no error handling... :-(
    pub fn query_all(&self, schema_str: &String) -> Result<Instances> {
        match self.data.get(schema_str) {
            Some(result) => Ok(result.to_owned()),
            None => Err("No results found".into()),
        }
    }

    /// Very raw POC methods, no error handling... :-(
    pub fn query_instance(&self, schema_str: &String, hash: &String) -> Result<MessageFields> {
        let instances = match self.query_all(schema_str) {
            Ok(instances) => Ok(instances.to_owned()),
            Err(str) => Err(str),
        }?;

        match instances.get(hash) {
            Some(instance) => Ok(instance.to_owned()),
            None => Err("No results found".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    
    use super::Materializer;
    
    use crate::node::Node;
    use crate::client::Client;
    use crate::utils::send_to_node;

    use p2panda_rs::tests::utils::{
        create_message, delete_message, fields, keypair_from_private, update_message, MESSAGE_SCHEMA,
    };
    use p2panda_rs::tests::fixtures::private_key;
        
    #[rstest]
    fn build_dag(private_key: String) {
        let mut node = Node::new();

        // Instantiate one client called "panda"
        let panda = Client::new("panda".to_string(), keypair_from_private(private_key));
    
        // Publish a CREATE message
        let entry_1 = send_to_node(
            &mut node,
            &panda,
            &create_message(
                MESSAGE_SCHEMA.into(),
                fields(vec![("message", "Ohh, my first message!")]),
            ),
        )
        .unwrap();
    
        // Publish an UPDATE message
        let entry_2 = send_to_node(
            &mut node,
            &panda,
            &update_message(
                MESSAGE_SCHEMA.into(),
                entry_1.clone(),
                fields(vec![("message", "Which I now update.")]),
            ),
        )
        .unwrap();
    
        // Publish an DELETE message
        let entry_3 = send_to_node(
            &mut node,
            &panda,
            &delete_message(MESSAGE_SCHEMA.into(), entry_1.clone()),
        )
        .unwrap();
    
        // Publish another CREATE message
        let entry_4 = send_to_node(
            &mut node,
            &panda,
            &create_message(
                MESSAGE_SCHEMA.into(),
                fields(vec![("message", "Let's try that again.")]),
            ),
        )
        .unwrap();
        
        // Get all entries
        let enries = node.all_entries();
    
        // Initialize materializer
        let mut materializer = Materializer::new();
        
        // Build instance DAGs from vector of all entries of one author
        materializer.build_dags(enries);
        
        // Get the instance DAG (in the form of a vector of edges) for the two existing instances
        let mut instance_dag_1 = materializer.dags().get(entry_1.as_str()).unwrap().to_owned().graph();
        let mut instance_dag_2 = materializer.dags().get(entry_4.as_str()).unwrap().to_owned().graph();
        
        let entry_str_1 = entry_1.as_str().to_string();
        let entry_str_2 = entry_2.as_str().to_string();
        let entry_str_3 = entry_3.as_str().to_string();
        let entry_str_4 = entry_4.as_str().to_string();
        
        // Pop each edge from the vector and compare with what we expect to see
        assert_eq!(instance_dag_1.pop().unwrap(), (None, entry_str_1.clone()));
        assert_eq!(instance_dag_1.pop().unwrap(), (Some(entry_str_1), entry_str_2.clone()));
        assert_eq!(instance_dag_1.pop().unwrap(), (Some(entry_str_2), entry_str_3));
        assert_eq!(instance_dag_2.pop().unwrap(), (None, entry_str_4));
    }
}
