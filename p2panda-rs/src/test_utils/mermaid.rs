// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods for expressing p2panda document graphs in mermaid-js

use std::collections::HashMap;

use crate::operation::{AsOperation, OperationWithMeta};

/// Trait for parsing a struct mermaid-js strings.
pub trait ToMermaid {
    /// Ouput a html string representation of this struct.
    fn to_html(&self) -> String;

    /// Ouput the css class of this node.
    fn node_class(&self) -> String;
}

impl ToMermaid for OperationWithMeta {
    fn to_html(&self) -> String {
        let action = match self.operation().action() {
            crate::operation::OperationAction::Create => "CREATE",
            crate::operation::OperationAction::Update => "UPDATE",
            crate::operation::OperationAction::Delete => "DELETE",
        };

        let mut id = String::with_capacity(68);
        id.push_str(self.operation_id().as_str());

        let mut fields = "".to_string();

        for (key, value) in self.fields().unwrap().iter() {
            match value {
                crate::operation::OperationValue::Boolean(_) => todo!(),
                crate::operation::OperationValue::Integer(_) => todo!(),
                crate::operation::OperationValue::Float(_) => todo!(),
                crate::operation::OperationValue::Text(str) => {
                    fields.push_str(&format!("<tr><td>{key}</td><td>{str}</td></tr>"))
                }
                crate::operation::OperationValue::Relation(_) => todo!(),
            }
        }

        format!(r#"<table><tr><th>{}<br>{}</th></tr>"#, &id[..34], &id[34..],)
            + &format!(r#"<tr><td>{action}</td></tr>"#)
            + &format!(r#"<tr><td><table>{fields}</table><td><tr></table>"#)
    }

    fn node_class(&self) -> String {
        // Classes have to start with a letter so we prepend an "A" to the public key.
        // Here we return the operation author so we can color nodes by author.
        format!("A{}", self.public_key().as_str())
    }
}

/// Parse a collection of operations into a mermaid-js string. Optionally pass
/// in author -> color mappings as a hashmap for node styling.
pub fn into_mermaid(
    operations: &[OperationWithMeta],
    author_colors: Option<HashMap<String, String>>,
) -> String {
    let mut mermaid_str = "\ngraph TD;\n".to_string();

    for op in operations {
        mermaid_str += &format!("{}[{}];\n", op.operation_id().as_str(), op.to_html());
    }

    for op in operations {
        if let Some(prev_ops) = op.previous_operations() {
            for prev_op in prev_ops {
                mermaid_str +=
                    &format!("{} --> {};\n", prev_op.as_str(), op.operation_id().as_str());
            }
        }
    }
    if let Some(author_colors) = author_colors {
        // HashMaps aren't ordered, so we sort the authors alphabetically so the resultant mermaid string
        // is deterministic. Mainly to help with testing.
        let mut authors: Vec<String> = author_colors
            .iter()
            .map(|(author, _)| author.to_owned())
            .collect();
        authors.sort();

        for author in authors {
            mermaid_str += &format!(
                "classDef {author} fill:{};\n",
                author_colors.get(&author).unwrap()
            );
        }
        for op in operations {
            mermaid_str += &format!(
                "class {} {};\n",
                op.operation_id().as_str(),
                op.node_class()
            )
        }
    };

    mermaid_str
}

#[cfg(test)]
mod test {

    use std::collections::HashMap;

    use crate::{
        document::DocumentBuilder,
        hash::Hash,
        identity::KeyPair,
        operation::{OperationValue, OperationWithMeta},
        test_utils::{
            constants::DEFAULT_SCHEMA_HASH,
            fixtures::{fields, update_operation},
            mermaid::into_mermaid,
            mocks::{send_to_node, Client, Node},
            utils::create_operation,
        },
    };

    #[test]
    fn generate_mermaid() {
        let panda = Client::new(
            "panda".to_string(),
            KeyPair::from_private_key_str(
                "ddcafe34db2625af34c8ba3cf35d46e23283d908c9848c8b43d1f5d0fde779ea",
            )
            .unwrap(),
        );
        let penguin = Client::new(
            "penguin".to_string(),
            KeyPair::from_private_key_str(
                "1c86b2524b48f0ba86103cddc6bdfd87774ab77ab4c0ea989ed0eeab3d28827a",
            )
            .unwrap(),
        );
        let mut node = Node::new();
        let schema = Hash::new(DEFAULT_SCHEMA_HASH).unwrap();

        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                schema.clone(),
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        let (panda_entry_2_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                schema.clone(),
                vec![panda_entry_1_hash.clone()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe!".to_string()),
                )]),
            ),
        )
        .unwrap();

        let (penguin_entry_1_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema.clone(),
                vec![panda_entry_1_hash],
                fields(vec![(
                    "name",
                    OperationValue::Text("Penguin Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        let (penguin_entry_2_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema.clone(),
                vec![penguin_entry_1_hash, panda_entry_2_hash],
                fields(vec![(
                    "name",
                    OperationValue::Text("Polar Bear Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        let (_, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema,
                vec![penguin_entry_2_hash],
                fields(vec![(
                    "name",
                    OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
                )]),
            ),
        )
        .unwrap();

        let operations: Vec<OperationWithMeta> = node
            .all_entries()
            .into_iter()
            .map(|entry| {
                OperationWithMeta::new(&entry.entry_encoded(), &entry.operation_encoded()).unwrap()
            })
            .collect();

        let document = DocumentBuilder::new(operations).build().unwrap();

        let mut author_colors = HashMap::new();
        author_colors.insert(format!("A{}", &panda.public_key()), "#fc7e58".to_string());
        author_colors.insert(format!("A{}", &penguin.public_key()), "#baf477".to_string());

        let expected_mermaid_str = "\ngraph TD;\n".to_string() +
                "0020b22e51716fdc436ab5f6ab7822f7440d7cf27bd0281e80dcb53f5ffe5b19079c[<table><tr><th>0020b22e51716fdc436ab5f6ab7822f744<br>0d7cf27bd0281e80dcb53f5ffe5b19079c</th></tr><tr><td>CREATE</td></tr><tr><td><table><tr><td>name</td><td>Panda Cafe</td></tr></table><td><tr></table>];\n" + 
                "002018f7ba553e196c59d15a569df57d283f4e1551b8f8fb946b942574ca0b9441b7[<table><tr><th>002018f7ba553e196c59d15a569df57d28<br>3f4e1551b8f8fb946b942574ca0b9441b7</th></tr><tr><td>UPDATE</td></tr><tr><td><table><tr><td>name</td><td>Panda Cafe!</td></tr></table><td><tr></table>];\n" + 
                "0020ba32164f5cdcee9bc74cb1aa87ae88cd8582755bed1a6f29be84bf034119b049[<table><tr><th>0020ba32164f5cdcee9bc74cb1aa87ae88<br>cd8582755bed1a6f29be84bf034119b049</th></tr><tr><td>UPDATE</td></tr><tr><td><table><tr><td>name</td><td>Penguin Cafe</td></tr></table><td><tr></table>];\n" + 
                "00203b0de0195b30e61dfef57e2a85c67d006be6b643f78e1743af134cae3fe63372[<table><tr><th>00203b0de0195b30e61dfef57e2a85c67d<br>006be6b643f78e1743af134cae3fe63372</th></tr><tr><td>UPDATE</td></tr><tr><td><table><tr><td>name</td><td>Polar Bear Cafe</td></tr></table><td><tr></table>];\n" + 
                "00205415c88c8a445208b29bd3a6cc70c65b2f58e9aabb2172bf088a0e4f86bf28a9[<table><tr><th>00205415c88c8a445208b29bd3a6cc70c6<br>5b2f58e9aabb2172bf088a0e4f86bf28a9</th></tr><tr><td>UPDATE</td></tr><tr><td><table><tr><td>name</td><td>Polar Bear Cafe!!!!!!!!!!</td></tr></table><td><tr></table>];\n" + 
                "0020b22e51716fdc436ab5f6ab7822f7440d7cf27bd0281e80dcb53f5ffe5b19079c --> 002018f7ba553e196c59d15a569df57d283f4e1551b8f8fb946b942574ca0b9441b7;\n" + 
                "0020b22e51716fdc436ab5f6ab7822f7440d7cf27bd0281e80dcb53f5ffe5b19079c --> 0020ba32164f5cdcee9bc74cb1aa87ae88cd8582755bed1a6f29be84bf034119b049;\n" + 
                "0020ba32164f5cdcee9bc74cb1aa87ae88cd8582755bed1a6f29be84bf034119b049 --> 00203b0de0195b30e61dfef57e2a85c67d006be6b643f78e1743af134cae3fe63372;\n" + 
                "002018f7ba553e196c59d15a569df57d283f4e1551b8f8fb946b942574ca0b9441b7 --> 00203b0de0195b30e61dfef57e2a85c67d006be6b643f78e1743af134cae3fe63372;\n" + 
                "00203b0de0195b30e61dfef57e2a85c67d006be6b643f78e1743af134cae3fe63372 --> 00205415c88c8a445208b29bd3a6cc70c65b2f58e9aabb2172bf088a0e4f86bf28a9;\n" + 
                "classDef A0d498015c5b5dd2708d8520380a49e7f6e596cdb3609a37e9299ab6dbd5ce174 fill:#fc7e58;\n" + 
                "classDef Afd078fa2f2cca9db13a329a46792d4bb90273b49867c0592decaa8a8d8e5cc0a fill:#baf477;\n" + 
                "class 0020b22e51716fdc436ab5f6ab7822f7440d7cf27bd0281e80dcb53f5ffe5b19079c A0d498015c5b5dd2708d8520380a49e7f6e596cdb3609a37e9299ab6dbd5ce174;\n" + 
                "class 002018f7ba553e196c59d15a569df57d283f4e1551b8f8fb946b942574ca0b9441b7 A0d498015c5b5dd2708d8520380a49e7f6e596cdb3609a37e9299ab6dbd5ce174;\n" + 
                "class 0020ba32164f5cdcee9bc74cb1aa87ae88cd8582755bed1a6f29be84bf034119b049 Afd078fa2f2cca9db13a329a46792d4bb90273b49867c0592decaa8a8d8e5cc0a;\n" + 
                "class 00203b0de0195b30e61dfef57e2a85c67d006be6b643f78e1743af134cae3fe63372 Afd078fa2f2cca9db13a329a46792d4bb90273b49867c0592decaa8a8d8e5cc0a;\n" + 
                "class 00205415c88c8a445208b29bd3a6cc70c65b2f58e9aabb2172bf088a0e4f86bf28a9 Afd078fa2f2cca9db13a329a46792d4bb90273b49867c0592decaa8a8d8e5cc0a;\n";
        assert_eq!(
            expected_mermaid_str,
            into_mermaid(document.operations(), Some(author_colors))
        )
    }
}
