// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_rs::hash::Hash;
/// Generate CBOR encoded test data. This is run with the `cargo run --bin cbor-test-data`
/// command. The output data can be used for testing a p2panda implementation. It is currently used
/// in `p2panda-js`.
use p2panda_rs::operation::{OperationId, OperationValue};
use p2panda_rs::schema::SchemaId;
use p2panda_rs::test_utils::mocks::Client;
use p2panda_rs::test_utils::mocks::{send_to_node, Node};
use p2panda_rs::test_utils::test_data::json_data::generate_test_data;
use p2panda_rs::test_utils::utils::{
    create_operation, delete_operation, new_key_pair, operation_fields, update_operation,
};

fn main() {
    // Instantiate mock node
    let mut node = Node::new();

    // Instantiate one client called "panda"
    let panda = Client::new("panda".to_string(), new_key_pair());

    let default_schema_hash: OperationId = Hash::new_from_bytes(vec![3, 2, 1]).unwrap().into();
    let schema_id = SchemaId::new_application("chat", &default_schema_hash.into());

    // Publish a CREATE operation
    let (entry1_hash, _) = send_to_node(
        &mut node,
        &panda,
        &create_operation(
            schema_id.clone(),
            operation_fields(vec![(
                "message",
                OperationValue::Text("Ohh, my first message!".to_string()),
            )]),
        ),
    )
    .unwrap();

    // Publish an UPDATE operation
    let (entry2_hash, _) = send_to_node(
        &mut node,
        &panda,
        &update_operation(
            schema_id.clone(),
            vec![entry1_hash.into()],
            operation_fields(vec![(
                "message",
                OperationValue::Text("Which I now update.".to_string()),
            )]),
        ),
    )
    .unwrap();

    // Publish another UPDATE operation
    let (entry3_hash, _) = send_to_node(
        &mut node,
        &panda,
        &update_operation(
            schema_id.clone(),
            vec![entry2_hash.into()],
            operation_fields(vec![(
                "message",
                OperationValue::Text("And then update again.".to_string()),
            )]),
        ),
    )
    .unwrap();

    // Publish an DELETE operation
    send_to_node(
        &mut node,
        &panda,
        &delete_operation(schema_id, vec![entry3_hash.into()]),
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
    use p2panda_rs::schema::SchemaId;

    use p2panda_rs::operation::{OperationId, OperationValue};
    use p2panda_rs::test_utils::constants::DEFAULT_PRIVATE_KEY;
    use p2panda_rs::test_utils::mocks::Client;
    use p2panda_rs::test_utils::mocks::{send_to_node, Node};
    use p2panda_rs::test_utils::test_data::json_data::generate_test_data;
    use p2panda_rs::test_utils::utils::{create_operation, keypair_from_private, operation_fields};

    #[test]
    fn test_data() {
        // Instantiate mock node
        let mut node = Node::new();

        // Instantiate one client called "panda"
        let panda = Client::new(
            "panda".to_string(),
            keypair_from_private(DEFAULT_PRIVATE_KEY.into()),
        );

        let default_schema_hash: OperationId = Hash::new_from_bytes(vec![3, 2, 1]).unwrap().into();
        let schema_id = SchemaId::new_application("chat", &default_schema_hash.into());

        // Publish a CREATE operation
        send_to_node(
            &mut node,
            &panda,
            &create_operation(
                schema_id,
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Ohh, my first message!".to_string()),
                )]),
            ),
        )
        .unwrap();

        const TEST_DATA: &str = "A16570616E6461A3697075626C69634B65797840326638653530633265646536643933366563633331343431383766663163323733383038313835636662633566663364333734386431666637333533666339366A707269766174654B6579784065623835326665666137303339303165343266313763646332616135303739343766333932613732313031623263316136643330303233616631346637356532646C6F677381A36E656E636F646564456E747269657381A766617574686F727840326638653530633265646536643933366563633331343431383766663163323733383038313835636662633566663364333734386431666637333533666339366A656E747279427974657379010C3030326638653530633265646536643933366563633331343431383766663163323733383038313835636662633566663364333734386431666637333533666339363031303139363030323036303035373238353130653566396464643434613236363065623638343931386238643533646238396439356165386334336638633033363331383465343463353463353237656236376531353261663638666333626566643139346238646561333864633932666566663333663739366236336161373561633333626635333233643031333962633532663335376134663532613330666664366131363463643162616135313364623735303338353963383635356237313863303964303569656E74727948617368784430303230343738313130353130663639373139666330643237363430623135636636386566663634383166393631613465353935313838323436356437383262663364336C7061796C6F6164427974657379012C6134363636313633373436393666366536363633373236353631373436353636373336333638363536643631373834393633363836313734356633303330333233303633333633353335333633373631363533333337363536363635363133323339333336353333333436313339363333373634333133333636333836363332363236363332333336343632363436333333363233353633333736323339363136323334333633323339333333313331333136333334333836363633333733383632363737363635373237333639366636653031363636363639363536633634373361313637366436353733373336313637363561313633373337343732373634663638363832633230366437393230363636393732373337343230366436353733373336313637363532316B7061796C6F61644861736878443030323036303035373238353130653566396464643434613236363065623638343931386238643533646238396439356165386334336638633033363331383465343463656C6F6749646131667365714E756D6131716465636F6465644F7065726174696F6E7381A466616374696F6E6663726561746566736368656D617849636861745F30303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696F6E01666669656C6473A1676D657373616765A163737472764F68682C206D79206669727374206D657373616765216D6E657874456E7472794172677382A471656E747279486173684261636B6C696E6BF671656E74727948617368536B69706C696E6BF6667365714E756D6131656C6F6749646131A471656E747279486173684261636B6C696E6B7844303032303437383131303531306636393731396663306432373634306231356366363865666636343831663936316134653539353138383234363564373832626633643371656E74727948617368536B69706C696E6BF6667365714E756D6132656C6F6749646131";

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
