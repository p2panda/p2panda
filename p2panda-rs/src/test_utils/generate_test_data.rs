// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_rs::hash::Hash;
use p2panda_rs::identity::KeyPair;
use p2panda_rs::operation::{OperationId, OperationValue};
use p2panda_rs::schema::SchemaId;
use p2panda_rs::test_utils::constants::DEFAULT_PRIVATE_KEY;
use p2panda_rs::test_utils::fixtures::{
    create_operation, delete_operation, operation, operation_fields, random_key_pair,
    update_operation,
};
use p2panda_rs::test_utils::mocks::{send_to_node, Client, Node};
use p2panda_rs::test_utils::test_data::json_data::generate_test_data;

/// Generate CBOR encoded test data. This is run with the `cargo run --bin cbor-test-data`
/// command. The output data can be used for testing a p2panda implementation. It is currently used
/// in `p2panda-js`.
fn main() {
    // Instantiate mock node
    let mut node = Node::new();

    // Instantiate one client called "panda"
    let panda = Client::new(
        "panda".to_string(),
        KeyPair::from_private_key_str(DEFAULT_PRIVATE_KEY).unwrap(),
    );

    let default_schema_hash: OperationId = Hash::new_from_bytes(vec![3, 2, 1]).unwrap().into();
    let schema_id = SchemaId::new_application("chat", &default_schema_hash.into());

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
            Some(schema_id.clone()),
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
            Some(schema_id.clone()),
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
            Some(schema_id.clone()),
        ),
    )
    .unwrap();

    // Publish an DELETE operation
    send_to_node(
        &mut node,
        &panda,
        &operation(None, Some(entry3_hash.into()), Some(schema_id.clone())),
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
    use p2panda_rs::schema::SchemaId;

    use p2panda_rs::operation::{OperationId, OperationValue};
    use p2panda_rs::test_utils::constants::DEFAULT_PRIVATE_KEY;
    use p2panda_rs::test_utils::fixtures::{operation, operation_fields};
    use p2panda_rs::test_utils::mocks::{send_to_node, Client, Node};
    use p2panda_rs::test_utils::test_data::json_data::generate_test_data;

    #[test]
    fn test_data() {
        // Instantiate mock node
        let mut node = Node::new();

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

        const TEST_DATA: &str = "A16570616E6461A3697075626C69634B65797840326638653530633265646536643933366563633331343431383766663163323733383038313835636662633566663364333734386431666637333533666339366A707269766174654B6579784065623835326665666137303339303165343266313763646332616135303739343766333932613732313031623263316136643330303233616631346637356532646C6F677381A36E656E636F646564456E747269657381A766617574686F727840326638653530633265646536643933366563633331343431383766663163323733383038313835636662633566663364333734386431666637333533666339366A656E747279427974657379010C3030326638653530633265646536643933366563633331343431383766663163323733383038313835636662633566663364333734386431666637333533666339363031303161313030323065663737323334636134393539333765313537363865343232366564643438373132386263633433633965613063643339303834366334303836346435333030303335613262623162633837653637393431356133666431623731656537363564623163363364613265656362313530333266303932613635386139313262646437613330386632653330323762313564343031383038306431346131326138656164616133663133303332653036623261323732393562643062373935303969656E74727948617368784430303230363566373466366664383165623162616531396562306438646365313435666161366135366437623430373664376662613433383534313036303962326261656C7061796C6F61644279746573790142613436363631363337343639366636653636363337323635363137343635363637333633363836353664363137383439363336383631373435663330333033323330363333363335333533363337363136353333333736353636363536313332333933333635333333343631333936333337363433313333363633383636333236323636333233333634363236343633333336323335363333373632333936313632333433363332333933333331333133313633333433383636363333373338363236373736363537323733363936663665303136363636363936353663363437336131363736643635373337333631363736356132363437343739373036353633373337343732363537363631366337353635373634663638363832633230366437393230363636393732373337343230366436353733373336313637363532316B7061796C6F61644861736878443030323065663737323334636134393539333765313537363865343232366564643438373132386263633433633965613063643339303834366334303836346435333030656C6F6749646131667365714E756D6131716465636F6465644F7065726174696F6E7381A466616374696F6E6663726561746566736368656D617849636861745F30303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696F6E01666669656C6473A1676D657373616765A26474797065637374726576616C7565764F68682C206D79206669727374206D657373616765216D6E657874456E7472794172677382A471656E747279486173684261636B6C696E6BF671656E74727948617368536B69706C696E6BF6667365714E756D6131656C6F6749646131A471656E747279486173684261636B6C696E6B7844303032303635663734663666643831656231626165313965623064386463653134356661613661353664376234303736643766626134333835343130363039623262616571656E74727948617368536B69706C696E6BF6667365714E756D6132656C6F6749646131";

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
