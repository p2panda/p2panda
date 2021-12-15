// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use crate::operation::{Operation, OperationFields};

use crate::test_utils::mocks::logs::LogEntry;
use crate::test_utils::mocks::materialisation::DAG;
use crate::test_utils::mocks::utils::Result;

/// A wrapper type representing a HashMap of instances stored by Instance id.
type Instances = HashMap<String, OperationFields>;

/// A wrapper type representing a materialised database of Instances stored by Schema hash.
/// We lose Author data during materialisation in this demo app...
type SchemaDatabase = HashMap<String, Instances>;

/// Struct for materialising Instances from Operations/Operations published to append only logs by multiple authors.
/// If concurrent Operations were published then conflicts are resolved through building and ordering a Directed Acyclic
/// Graph of operations arranged causally. Operations are ordered into a linear queue through topologically
/// sorting the graph. Operations are then applied sequentially with any conflicts that occur being resolved
/// through last-write-wins rules.  
#[derive(Debug, Default)]
pub struct Materialiser {
    // The final data structure where materialised instances are stored
    data: SchemaDatabase,
    // Operations stored by Entry hash
    operations: HashMap<String, Operation>,
    // DAGs stored by Instance id
    dags: HashMap<String, DAG>,
}

impl Materialiser {
    /// Create new materialiser
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            operations: HashMap::new(),
            dags: HashMap::new(),
        }
    }

    /// Get the materialised Instances
    pub fn data(&self) -> SchemaDatabase {
        self.data.clone()
    }

    /// Get all Instance DAGs
    pub fn dags(&self) -> HashMap<String, DAG> {
        self.dags.clone()
    }

    /// Store operations
    pub fn store_operations(&mut self, entries: Vec<LogEntry>) {
        entries.iter().for_each(|entry| {
            self.operations.insert(entry.hash_str(), entry.operation());
        });
    }

    /// Take an array of entries from a single author with multiple schema logs. Creates an update path DAG for
    /// each instance of and also stores a list of all operations for materialisation which takes place
    /// in the next step.
    pub fn build_dags(&mut self, entries: Vec<LogEntry>) {
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        // Build instance DAGs
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        // Loop over remaining entries storing each operation and building our dag
        entries.iter().for_each(|entry| {
            // If operation.id() is None this is a CREATE operation and
            // we need to set the instance_id manually
            let instance_id = match entry.operation().id() {
                Some(id) => id.as_str().to_owned(),
                None => entry.hash_str(),
            };

            // Check if this instance DAG already exists, create it if not
            if !self.dags.contains_key(&instance_id) {
                self.dags.insert(instance_id.clone(), DAG::new());
            }

            // Retrieve the instance DAG
            let dag = self.dags.get_mut(&instance_id).unwrap();

            // Create an edge for this operation in the DAG, if it is a CREATE operation
            // then it should be a root node.
            if entry.operation().is_create() {
                dag.add_root(entry.hash_str());
            } else {
                dag.add_edge(entry.previous_operation().unwrap(), entry.hash_str());
            }
        });
    }

    /// Apply changes to an instance from an ordered list of entries
    pub fn apply_instance_operations(&mut self, entries: Vec<String>, instance_id: String) {
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        // Materialise instances
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        // loop the ordered list of operations
        for entry_id in entries {
            // Get the actual operation content by id
            let operation = self.operations.get(&entry_id).unwrap();

            // Get the schema string
            let schema_str = operation.schema().as_str();

            // Create schema map for instances if it doesn't exist
            if !self.data().contains_key(schema_str) {
                self.data.insert(schema_str.into(), Instances::new());
            }

            // Get all instances for this schema
            let instances = self.data.get_mut(schema_str).unwrap();

            // Materialise all operations in order!! Currently an UPDATE operation replaces all
            // fields in the operation. I guess we don't want this behaviour eventually.

            // If CREATE operation insert new instance
            if operation.is_create() {
                instances.insert(
                    instance_id.to_owned(),
                    operation.fields().unwrap().to_owned(),
                );
            }

            // If UPDATE operation update existing instance
            if operation.is_update() {
                instances.insert(
                    instance_id.to_owned(),
                    operation.fields().unwrap().to_owned(),
                );
            }

            // If DELETE operation delete existing instance
            if operation.is_delete() {
                instances.remove(&instance_id);
            }
        }
    }

    /// Materialise entries from multiple authors and schema logs into a database of Instancess
    pub fn materialise(&mut self, entries: &[LogEntry]) -> Result<SchemaDatabase> {
        // Store all operations ready for processing after conflict resolution
        self.store_operations(entries.to_owned());

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~LogEntryclone
        // Build DAGs for each Instances
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        // Process entries ready for ordering and materialisation
        self.build_dags(entries.to_owned());

        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        // Resolve conflicts and Materialise
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        // Loop over all instance DAGs
        for (instance_id, mut dag) in self.dags() {
            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            // Topologically sort instance DAGs
            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

            // Walk the graph depth first, creating a topological ordering of operations
            let ordered_operations = dag.topological();

            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            // Materialise instances
            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

            self.apply_instance_operations(ordered_operations, instance_id);
        }
        Ok(self.data())
    }

    /// Very raw POC methods, no error handling... :-(
    pub fn query_all(&self, schema_str: &str) -> Result<Instances> {
        match self.data.get(schema_str) {
            Some(result) => Ok(result.to_owned()),
            None => Err("No results found".into()),
        }
    }

    /// Very raw POC methods, no error handling... :-(
    pub fn query_instance(&self, schema_str: &str, hash: &str) -> Result<OperationFields> {
        let instances = match self.query_all(schema_str) {
            Ok(instances) => Ok(instances),
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

    use super::Materialiser;

    use crate::operation::OperationValue;
    use crate::test_utils::constants::DEFAULT_SCHEMA_HASH;
    use crate::test_utils::fixtures::private_key;
    use crate::test_utils::mocks::client::Client;
    use crate::test_utils::mocks::node::{send_to_node, Node};
    use crate::test_utils::utils::{
        create_operation, delete_operation, hash, keypair_from_private, operation_fields,
        update_operation,
    };

    fn mock_node(panda: Client) -> Node {
        let mut node = Node::new();

        // Publish a CREATE operation
        let instance_1 = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                hash(DEFAULT_SCHEMA_HASH),
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Ohh, my first message!".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Publish an UPDATE operation
        send_to_node(
            &mut node,
            &panda,
            &update_operation(
                hash(DEFAULT_SCHEMA_HASH),
                instance_1.clone(),
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Which I now update.".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Publish an DELETE operation
        send_to_node(
            &mut node,
            &panda,
            &delete_operation(hash(DEFAULT_SCHEMA_HASH), instance_1),
        )
        .unwrap();

        // Publish another CREATE operation
        send_to_node(
            &mut node,
            &panda,
            &create_operation(
                hash(DEFAULT_SCHEMA_HASH),
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Let's try that again.".to_string()),
                )]),
            ),
        )
        .unwrap();

        node
    }

    #[rstest]
    fn build_dag(private_key: String) {
        let panda = Client::new("panda".to_string(), keypair_from_private(private_key));
        let node = mock_node(panda);

        // Get all entries
        let entries = node.all_entries();

        // Initialize materialiser
        let mut materialiser = Materialiser::new();

        // Build instance DAGs from vector of all entries of one author
        materialiser.build_dags(entries.clone());

        // Get the instance DAG (in the form of a vector of edges) for the two existing instances
        let mut instance_dag_1 = materialiser
            .dags()
            .get(&entries[0].hash_str())
            .unwrap()
            .to_owned()
            .graph();
        let mut instance_dag_2 = materialiser
            .dags()
            .get(&entries[3].hash_str())
            .unwrap()
            .to_owned()
            .graph();

        let entry_str_1 = entries[0].hash_str();
        let entry_str_2 = entries[1].hash_str();
        let entry_str_3 = entries[2].hash_str();
        let entry_str_4 = entries[3].hash_str();

        // Pop each edge from the vector and compare with what we expect to see
        assert_eq!(instance_dag_1.pop().unwrap(), (None, entry_str_1.clone()));
        assert_eq!(
            instance_dag_1.pop().unwrap(),
            (Some(entry_str_1), entry_str_2.clone())
        );
        assert_eq!(
            instance_dag_1.pop().unwrap(),
            (Some(entry_str_2), entry_str_3)
        );
        assert_eq!(instance_dag_2.pop().unwrap(), (None, entry_str_4));
    }

    #[rstest]
    fn materialise_instances(private_key: String) {
        let panda = Client::new(
            "panda".to_string(),
            keypair_from_private(private_key.clone()),
        );
        let mut node = mock_node(panda);

        // Get all entries
        let entries = node.all_entries();

        // Initialize materialiser
        let mut materialiser = Materialiser::new();

        // Materialise all instances
        let instances = materialiser.materialise(&entries).unwrap();

        // Get instances for DEFAULT_SCHEMA_HASH
        let schema_instances = instances.get(DEFAULT_SCHEMA_HASH).unwrap();

        // Get an instance by id
        let instance_1 = schema_instances.get(&entries[0].hash_str());
        let instance_2 = schema_instances.get(&entries[3].hash_str());

        // Instance 1 was deleted
        assert_eq!(instance_1, None);
        // Instance 2 should be there
        assert_eq!(
            instance_2.unwrap().get("message").unwrap().to_owned(),
            OperationValue::Text("Let's try that again.".to_string())
        );

        // Create Panda again...
        let panda = Client::new("panda".to_string(), keypair_from_private(private_key));

        // Publish an UPDATE operation targeting instance 2
        send_to_node(
            &mut node,
            &panda,
            &update_operation(
                hash(DEFAULT_SCHEMA_HASH),
                entries[3].hash(),
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Now it's updated.".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Get all entries
        let entries = node.all_entries();

        // Materialise all instances
        let instances = materialiser.materialise(&entries).unwrap();

        // Get instances for DEFAULT_SCHEMA_HASH
        let schema_instances = instances.get(DEFAULT_SCHEMA_HASH).unwrap();

        // Get an instance by id
        let instance_2 = schema_instances.get(&entries[3].hash_str());

        // Instance 2 should be there
        assert_eq!(
            instance_2.unwrap().get("message").unwrap().to_owned(),
            OperationValue::Text("Now it's updated.".to_string())
        );
    }

    #[rstest]
    fn query_instances(private_key: String) {
        let panda = Client::new("panda".to_string(), keypair_from_private(private_key));
        let node = mock_node(panda);

        // Get all entries
        let entries = node.all_entries();

        // Initialize materialiser
        let mut materialiser = Materialiser::new();

        // Materialise entries
        materialiser.materialise(&entries).unwrap();

        // Fetch all instances
        let instances = materialiser
            .query_all(&DEFAULT_SCHEMA_HASH.to_string())
            .unwrap();

        // There should be one instance
        assert_eq!(instances.len(), 1);

        // Query for one instance by id
        let instance = materialiser
            .query_instance(&DEFAULT_SCHEMA_HASH.to_string(), &entries[3].hash_str())
            .unwrap();

        assert_eq!(
            instance.get("message").unwrap().to_owned(),
            OperationValue::Text("Let's try that again.".to_string())
        );
    }
}
