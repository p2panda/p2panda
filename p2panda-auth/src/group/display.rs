// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::fmt::{Debug, Display};

use petgraph::algo::toposort;
use petgraph::dot::{Config, Dot};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::IntoNodeReferences;

use crate::group::crdt::StateChangeResult;
use crate::group::{GroupAction, GroupControlMessage, GroupCrdt, GroupCrdtState, GroupMember};
use crate::traits::{GroupStore, IdentityHandle, Operation, OperationId, Orderer, Resolver};

const OP_FILTER_NODE: &str = "#E63C3F";
const OP_OK_NODE: &str = "#BFC6C77F";
const OP_NOOP_NODE: &str = "#FFA142";
const OP_ROOT_NODE: &str = "#EDD7B17F";
const INDIVIDUAL_NODE: &str = "#EDD7B17F";
const ADD_MEMBER_EDGE: &str = "#0091187F";
const PREVIOUS_EDGE: &str = "#000000";
const DEPENDENCIES_EDGE: &str = "#B748E37F";

impl<ID, OP, C, RS, ORD, GS> GroupCrdtState<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle + Ord + Display,
    OP: OperationId + Ord + Display,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ID, OP, C, ORD, GS> + Clone + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Clone + Debug,
    ORD::State: Clone,
    ORD::Operation: Clone,
    GS: GroupStore<ID, OP, C, RS, ORD> + Clone + Debug,
{
    /// Print an auth group graph in DOT format for visualizing the group operation DAG.
    pub fn display(&self) -> String {
        let mut graph = DiGraph::new();
        graph = self.add_nodes_and_previous_edges(self.clone(), graph);

        graph.add_node((None, self.format_final_members()));

        let dag_graphviz = Dot::with_attr_getters(
            &graph,
            &[Config::NodeNoLabel, Config::EdgeNoLabel],
            &|_, edge| {
                let weight = edge.weight();
                if weight == "previous" {
                    return format!("color=\"{PREVIOUS_EDGE}\", penwidth = 2.0");
                }

                if weight == "member" || weight == "sub group" {
                    return format!("color=\"{ADD_MEMBER_EDGE}\", penwidth = 2.0");
                }

                format!("constraint = false, color=\"{DEPENDENCIES_EDGE}\", penwidth = 2.0")
            },
            &|_, (_, (_, s))| format!("label = {}", s),
        );

        let mut s = format!("{:?}", dag_graphviz);
        s = s.replace("digraph {", "digraph {\n    splines=polyline\n");
        s
    }

    fn add_nodes_and_previous_edges(
        &self,
        root: Self,
        mut graph: DiGraph<(Option<OP>, String), String>,
    ) -> DiGraph<(Option<OP>, String), String> {
        for operation in self.operations.values() {
            graph.add_node((Some(operation.id()), self.format_operation(operation)));

            let (operation_idx, _) = graph
                .node_references()
                .find(|(_, (op, _))| {
                    if let Some(op) = op {
                        *op == operation.id()
                    } else {
                        false
                    }
                })
                .unwrap();

            if let GroupControlMessage {
                action: GroupAction::Add { member, .. },
                ..
            } = operation.payload()
            {
                graph = self.add_member_to_graph(operation_idx, member, root.clone(), graph);
            }

            if let GroupControlMessage {
                action:
                    GroupAction::Create {
                        initial_members, ..
                    },
                ..
            } = operation.payload()
            {
                for (member, _access) in initial_members {
                    graph = self.add_member_to_graph(operation_idx, member, root.clone(), graph);
                }
            }

            let mut dependencies = operation.dependencies().clone();
            let previous = operation.previous();
            dependencies.retain(|id| !previous.contains(id));

            for dependency in dependencies {
                let (idx, _) = graph
                    .node_references()
                    .find(|(_, (op, _))| {
                        if let Some(op) = op {
                            *op == dependency
                        } else {
                            false
                        }
                    })
                    .unwrap();
                graph.add_edge(operation_idx, idx, "dependency".to_string());
            }

            for previous in previous {
                let (idx, _) = graph
                    .node_references()
                    .find(|(_, (op, _))| {
                        if let Some(op) = op {
                            *op == previous
                        } else {
                            false
                        }
                    })
                    .unwrap();
                graph.add_edge(operation_idx, idx, "previous".to_string());
            }
        }

        graph
    }

    fn format_operation(&self, operation: &ORD::Operation) -> String {
        let control_message = operation.payload();

        let mut s = String::new();

        let color = if control_message.is_create() {
            OP_ROOT_NODE
        } else {
            match GroupCrdt::apply_action(
                self.clone(),
                operation.id(),
                operation.author(),
                &HashSet::from_iter(operation.dependencies()),
                &control_message.action,
            )
            .expect("critical error when applying state change")
            {
                StateChangeResult::Ok { .. } => OP_OK_NODE,
                StateChangeResult::Noop { .. } => OP_NOOP_NODE,
                StateChangeResult::Filtered { .. } => OP_FILTER_NODE,
            }
        };

        s += &format!(
            "<<TABLE BGCOLOR=\"{color}\" BORDER=\"0\" CELLBORDER=\"1\" CELLSPACING=\"0\">"
        );
        s += &format!("<TR><TD>group</TD><TD>{}</TD></TR>", self.id());
        s += &format!("<TR><TD>operation id</TD><TD>{}</TD></TR>", operation.id());
        s += &format!("<TR><TD>actor</TD><TD>{}</TD></TR>", operation.author());
        let previous = operation.previous();
        if !previous.is_empty() {
            s += &format!(
                "<TR><TD>previous</TD><TD>{}</TD></TR>",
                self.format_dependencies(&previous)
            );
        }
        let dependencies = operation.dependencies().clone();
        if !dependencies.is_empty() {
            s += &format!(
                "<TR><TD>dependencies</TD><TD>{}</TD></TR>",
                self.format_dependencies(&dependencies)
            );
        }
        s += &format!(
            "<TR><TD COLSPAN=\"2\">{}</TD></TR>",
            self.format_control_message(&control_message)
        );
        s += &format!(
            "<TR><TD COLSPAN=\"2\">{}</TD></TR>",
            self.format_members(operation)
        );
        s += "</TABLE>>";
        s
    }

    fn format_final_members(&self) -> String {
        let mut s = String::new();
        s += "<<TABLE BGCOLOR=\"#00E30F7F\" BORDER=\"1\" CELLBORDER=\"1\" CELLSPACING=\"2\">";

        let members = self.transitive_members().unwrap();
        s += "<TR><TD>GROUP MEMBERS</TD></TR>";
        for (id, access) in members {
            s += &format!("<TR><TD> {} : {} </TD></TR>", id, access);
        }
        s += "</TABLE>>";
        s
    }

    fn format_control_message(&self, message: &GroupControlMessage<ID, C>) -> String {
        let mut s = String::new();
        s += "<TABLE BORDER=\"0\" CELLBORDER=\"1\" CELLSPACING=\"0\">";

        match &message.action {
            GroupAction::Create { initial_members } => {
                s += "<TR><TD>CREATE</TD></TR>";
                s += "<TR><TD>initial members</TD></TR>";
                for (member, access) in initial_members {
                    match member {
                        GroupMember::Individual(id) => {
                            s += &format!("<TR><TD>individual : {} : {}</TD></TR>", id, access)
                        }
                        GroupMember::Group(id) => {
                            s += &format!("<TR><TD>group : {} : {}</TD></TR>", id, access)
                        }
                    }
                }
            }
            GroupAction::Add { member, access } => {
                s += "<TR><TD>ADD</TD></TR>";
                match member {
                    GroupMember::Individual(id) => {
                        s += &format!("<TR><TD>individual : {} : {}</TD></TR>", id, access)
                    }
                    GroupMember::Group(id) => {
                        s += &format!("<TR><TD>group : {} : {}</TD></TR>", id, access)
                    }
                }
            }
            GroupAction::Remove { member } => {
                s += "<TR><TD>REMOVE</TD></TR>";
                match member {
                    GroupMember::Individual(id) => {
                        s += &format!("<TR><TD>individual : {}</TD></TR>", id)
                    }
                    GroupMember::Group(id) => s += &format!("<TR><TD>group : {}</TD></TR>", id),
                }
            }
            GroupAction::Promote { member, access } => {
                s += "<TR><TD>PROMOTE</TD></TR>";
                match member {
                    GroupMember::Individual(id) => {
                        s += &format!("<TR><TD>individual : {} : {}</TD></TR>", id, access)
                    }
                    GroupMember::Group(id) => {
                        s += &format!("<TR><TD>group : {} : {}</TD></TR>", id, access)
                    }
                }
            }
            GroupAction::Demote { member, access } => {
                s += "<TR><TD>DEMOTE</TD></TR>";
                match member {
                    GroupMember::Individual(id) => {
                        s += &format!("<TR><TD>individual : {} : {}</TD></TR>", id, access)
                    }
                    GroupMember::Group(id) => {
                        s += &format!("<TR><TD>group : {} : {}</TD></TR>", id, access)
                    }
                }
            }
        }
        s += "</TABLE>";
        s
    }

    fn format_members(&self, operation: &ORD::Operation) -> String {
        let mut dependencies = HashSet::from_iter(operation.dependencies().clone());
        dependencies.insert(operation.id());
        let mut members = self
            .transitive_members_at(&dependencies)
            .expect("state exists");
        members.sort_by(|(id_a, _), (id_b, _)| id_a.cmp(id_b));

        let mut s = String::new();
        s += "<TABLE BORDER=\"0\" CELLBORDER=\"1\" CELLSPACING=\"0\">";
        s += "<TR><TD>MEMBERS</TD></TR>";

        for (member, access) in members {
            s += &format!("<TR><TD>{member} : {access}</TD></TR>")
        }

        s += "</TABLE>";
        s
    }

    fn format_dependencies(&self, dependencies: &Vec<OP>) -> String {
        let mut s = String::new();
        s += "<TABLE BORDER=\"0\" CELLBORDER=\"1\" CELLSPACING=\"0\">";

        for id in dependencies {
            s += &format!("<TR><TD>{id}</TD></TR>")
        }

        s += "</TABLE>";
        s
    }

    fn add_member_to_graph(
        &self,
        operation_idx: NodeIndex,
        member: GroupMember<ID>,
        root: Self,
        mut graph: DiGraph<(Option<OP>, String), String>,
    ) -> DiGraph<(Option<OP>, String), String> {
        match member {
            GroupMember::Individual(id) => {
                let table = format!(
                    "<<TABLE BGCOLOR=\"{INDIVIDUAL_NODE}\" BORDER=\"0\" CELLBORDER=\"1\" CELLSPACING=\"0\"><TR><TD>individual</TD><TD>{id}</TD></TR></TABLE>>"
                );
                let idx = match graph.node_references().find(|(_, (_, t))| t == &table) {
                    Some((idx, _)) => idx,
                    None => graph.add_node((None, table)),
                };
                graph.add_edge(operation_idx, idx, "member".to_string());
            }
            GroupMember::Group(id) => {
                let sub_group = self.get_sub_group(id).unwrap();

                let topo_sort = toposort(&sub_group.graph, None)
                    .expect("group operation sets can be ordered topologically");
                let create_op_id = topo_sort
                    .first()
                    .expect("at least one operation exists in graph");

                let create_node = graph.node_references().find(|(_, (op, _))| {
                    if let Some(op) = op {
                        op == create_op_id
                    } else {
                        false
                    }
                });

                let create_operation_idx = match create_node {
                    Some((idx, _)) => idx,
                    None => {
                        graph = sub_group.add_nodes_and_previous_edges(root.clone(), graph);
                        let (idx, _) = graph
                            .node_references()
                            .find(|(_, (op, _))| {
                                if let Some(op) = op {
                                    op == create_op_id
                                } else {
                                    false
                                }
                            })
                            .unwrap();
                        idx
                    }
                };

                graph.add_edge(operation_idx, create_operation_idx, "sub group".to_string());
            }
        }
        graph
    }
}
