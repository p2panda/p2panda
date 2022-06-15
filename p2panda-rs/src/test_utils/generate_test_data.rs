// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_rs::identity::KeyPair;
use p2panda_rs::operation::OperationValue;
use p2panda_rs::schema::FieldType;
use p2panda_rs::test_utils::constants::DEFAULT_PRIVATE_KEY;
use p2panda_rs::test_utils::fixtures::{operation, operation_fields, schema_item};
use p2panda_rs::test_utils::mocks::{send_to_node, Client, Node};
use p2panda_rs::test_utils::test_data::json_data::generate_test_data;

/// Generate CBOR encoded test data. This is run with the `cargo run --bin cbor-test-data`
/// command. The output data can be used for testing a p2panda implementation. It is currently used
/// in `p2panda-js`.
fn main() {
    // Instantiate mock node
    let test_schema = schema_item(
        "chat_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
            .parse()
            .unwrap(),
        "",
        vec![("message", FieldType::String)],
    );
    let mut node = Node::new(vec![test_schema.clone()]);

    // Instantiate one client called "panda"
    let panda = Client::new(
        "panda".to_string(),
        KeyPair::from_private_key_str(DEFAULT_PRIVATE_KEY).unwrap(),
    );

    // Publish a CREATE operation
    let (entry1_hash, _) = send_to_node(
        &mut node,
        &panda,
        &operation(
            Some(operation_fields(vec![(
                "message",
                OperationValue::Text("Ohh, my first message!".to_string()),
            )])),
            None,
            Some(test_schema.id().clone()),
        ),
    )
    .unwrap();

    // Publish an UPDATE operation
    let (entry2_hash, _) = send_to_node(
        &mut node,
        &panda,
        &operation(
            Some(operation_fields(vec![(
                "message",
                OperationValue::Text("Which I now update.".to_string()),
            )])),
            Some(entry1_hash.into()),
            Some(test_schema.id().clone()),
        ),
    )
    .unwrap();

    // Publish another UPDATE operation
    let (entry3_hash, _) = send_to_node(
        &mut node,
        &panda,
        &operation(
            Some(operation_fields(vec![(
                "message",
                OperationValue::Text("And then update again.".to_string()),
            )])),
            Some(entry2_hash.into()),
            Some(test_schema.id().clone()),
        ),
    )
    .unwrap();

    // Publish an DELETE operation
    send_to_node(
        &mut node,
        &panda,
        &operation(
            None,
            Some(entry3_hash.into()),
            Some(test_schema.id().clone()),
        ),
    )
    .unwrap();

    // Get the database represented as json and formatted ready to be used as test data in
    // `p2panda-js`
    let formatted_data = generate_test_data(&mut node, vec![panda]);

    let mut cbor = vec![];
    ciborium::ser::into_writer(&formatted_data, &mut cbor).unwrap();
    println!("{}", hex::encode(cbor));
}

#[cfg(test)]
mod tests {
    use p2panda_rs::hash::Hash;
    use p2panda_rs::identity::KeyPair;
    use p2panda_rs::schema::{FieldType, SchemaId};

    use p2panda_rs::operation::{OperationId, OperationValue};
    use p2panda_rs::test_utils::constants::{DEFAULT_PRIVATE_KEY, TEST_SCHEMA_ID};
    use p2panda_rs::test_utils::fixtures::{operation, operation_fields, schema, schema_item};
    use p2panda_rs::test_utils::mocks::{send_to_node, Client, Node};
    use p2panda_rs::test_utils::test_data::json_data::generate_test_data;

    #[test]
    fn test_data() {
        // Instantiate mock node
        let test_schema = schema_item(
            schema("chat_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"),
            "",
            vec![("message", FieldType::String)],
        );
        let mut node = Node::new(vec![test_schema]);

        // Instantiate one client called "panda"
        let panda = Client::new(
            "panda".to_string(),
            KeyPair::from_private_key_str(DEFAULT_PRIVATE_KEY).unwrap(),
        );

        let default_schema_hash: OperationId = Hash::new_from_bytes(vec![3, 2, 1]).unwrap().into();
        let schema_id = SchemaId::new_application("chat", &default_schema_hash.into());

        // Publish a CREATE operation
        send_to_node(
            &mut node,
            &panda,
            &operation(
                Some(operation_fields(vec![(
                    "message",
                    OperationValue::Text("Ohh, my first message!".to_string()),
                )])),
                None,
                Some(schema_id),
            ),
        )
        .unwrap();

        const TEST_DATA: &str = "A16570616E6461A3697075626C69634B65797840326638653530633265646536643933366563633331343431383766663163323733383038313835636662633566663364333734386431666637333533666339366A707269766174654B6579784065623835326665666137303339303165343266313763646332616135303739343766333932613732313031623263316136643330303233616631346637356532646C6F677381A36E656E636F646564456E747269657381A766617574686F727840326638653530633265646536643933366563633331343431383766663163323733383038313835636662633566663364333734386431666637333533666339366A656E747279427974657379010C3030326638653530633265646536643933366563633331343431383766663163323733383038313835636662633566663364333734386431666637333533666339363031303139313030323030376133303939656465363365396431393431373165323834326532356337376334626262663537363665393137653335343632303936303836343939613635383863613663616462393638626162616238656339363564643530643865336430373066643537346232356635613865356137343136343265346233626630353134366531303963326464653762666131386566663935666233653661653936666138336635303766613265613530323236636536396336333161636462306469656E74727948617368784430303230343834613938323532316534616131326361343838653265333761656334346538643765373234623836363335353635303335346332393130346532376662376C7061796C6F6164427974657379012261343636363136333734363936663665363636333732363536313734363536363733363336383635366436313738343936333638363137343566333033303332333036333336333533353336333736313635333333373635363636353631333233393333363533333334363133393633333736343331333336363338363633323632363633323333363436323634363333333632333536333337363233393631363233343336333233393333333133313331363333343338363636333337333836323637373636353732373336393666366530313636363636393635366336343733613136373664363537333733363136373635373634663638363832633230366437393230363636393732373337343230366436353733373336313637363532316B7061796C6F61644861736878443030323030376133303939656465363365396431393431373165323834326532356337376334626262663537363665393137653335343632303936303836343939613635656C6F6749646131667365714E756D6131716465636F6465644F7065726174696F6E7381A466616374696F6E6663726561746566736368656D617849636861745F30303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696F6E01666669656C6473A1676D657373616765764F68682C206D79206669727374206D657373616765216D6E657874456E7472794172677382A471656E747279486173684261636B6C696E6BF671656E74727948617368536B69706C696E6BF6667365714E756D6131656C6F6749646131A471656E747279486173684261636B6C696E6B7844303032303438346139383235323165346161313263613438386532653337616563343465386437653732346238363633353536353033353463323931303465323766623771656E74727948617368536B69706C696E6BF6667365714E756D6132656C6F6749646131";

        // Generate test data
        let generated_test_data = generate_test_data(&mut node, vec![panda]);

        // Encode as CBOR
        let mut cbor = vec![];
        ciborium::ser::into_writer(&generated_test_data, &mut cbor).unwrap();

        // Both should be equal
        assert_eq!(TEST_DATA, hex::encode_upper(cbor));
    }

    #[test]
    fn test_main() {
        // Check that example values actually work
        crate::main();
    }
}
