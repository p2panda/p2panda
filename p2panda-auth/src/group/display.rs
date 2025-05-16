use std::fmt::{Debug, Display};

use petgraph::dot::{Config, Dot};
use petgraph::graph::DiGraph;
use petgraph::visit::IntoNodeReferences;

use crate::group::{GroupAction, GroupControlMessage, GroupMember, GroupState, GroupStateInner};
use crate::traits::{GroupStore, IdentityHandle, Operation, OperationId, Ordering, Resolver};

impl<ID, OP, RS, ORD, GS> GroupState<ID, OP, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Display + Ord,
    RS: Resolver<GroupState<ID, OP, RS, ORD, GS>, ORD::Message> + Clone + Debug,
    ORD: Clone + Debug + Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    GS: Clone + Debug + GroupStore<ID, GroupStateInner<ID, OP, ORD::Message>>,
{
    fn add_nodes_and_previous_edges(
        &self,
        root: Self,
        visited: &mut Vec<OP>,
        mut graph: DiGraph<(ORD::Message, String), String>,
    ) -> DiGraph<(ORD::Message, String), String> {
        for operation in &self.inner.operations {
            visited.push(operation.id());
            graph.add_node((
                operation.clone(),
                self.format_operation(&root, operation, visited),
            ));

            let (operation_idx, _) = graph
                .node_references()
                .find(|(idx, (op, _))| op.id() == operation.id())
                .unwrap();

            if let GroupControlMessage::GroupAction {
                action:
                    GroupAction::Add {
                        member: GroupMember::Group { id },
                        ..
                    },
                ..
            } = operation.payload()
            {
                let sub_group = self.get_sub_group(*id).unwrap();
                graph = sub_group.add_nodes_and_previous_edges(root.clone(), visited, graph);

                let create_operation = sub_group
                    .inner
                    .operations
                    .first()
                    .expect("create operation exists");

                let (create_operation_idx, _) = graph
                    .node_references()
                    .find(|(idx, (op, _))| op.id() == create_operation.id())
                    .unwrap();

                graph.add_edge(operation_idx, create_operation_idx, "sub group".to_string());
            }

            for dependency in operation.dependencies() {
                let (idx, _) = graph
                    .node_references()
                    .find(|(idx, (op, _))| op.id() == *dependency)
                    .unwrap();
                graph.add_edge(operation_idx, idx, "dependency".to_string());
            }
        }

        // graph = self.add_nodes_and_previous_edges(graph);
        graph
    }

    pub fn display(&self) -> String {
        let mut graph = DiGraph::new();
        let mut visited = vec![];
        graph = self.add_nodes_and_previous_edges(self.clone(), &mut visited, graph);

        let dag_graphviz = Dot::with_attr_getters(
            &graph,
            &[Config::NodeNoLabel, Config::EdgeNoLabel],
            &|_, edge| format!("label = \"{}\"", edge.weight()),
            &|_, (idx, (_, s))| format!("label = {}", s),
        );

        format!("{:?}", dag_graphviz)
    }

    fn format_operation(&self, root: &Self, operation: &ORD::Message, visited: &Vec<OP>) -> String {
        let control_message = operation.payload();
        let mut dependencies = operation.dependencies().clone();
        dependencies.push(operation.id());
        let members = self
            .transitive_members_at(&dependencies)
            .expect("state exists");
        let mut f = String::new();

        let color = if control_message.is_create() {
            "bisque"
        } else {
            "grey"
        };

        f += &format!(
            "<<TABLE BGCOLOR=\"{color}\" BORDER=\"0\" CELLBORDER=\"1\" CELLSPACING=\"0\">"
        );
        f += &format!("<TR><TD>id</TD><TD>{}</TD></TR>", operation.id());
        f += &format!("<TR><TD>actor</TD><TD>{}</TD></TR>", operation.sender());
        f += &format!(
            "<TR><TD>previous</TD><TD>{:?}</TD></TR>",
            operation.previous()
        );
        f += &format!(
            "<TR><TD>dependencies</TD><TD>{:?}</TD></TR>",
            operation.dependencies()
        );
        f += &format!("<TR><TD COLSPAN=\"2\">{:?}</TD></TR>", control_message);
        f += &format!("<TR><TD COLSPAN=\"2\">{:?}</TD></TR>", members);
        f += "</TABLE>>";
        f
    }
}
