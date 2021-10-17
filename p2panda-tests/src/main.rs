// SPDX-License-Identifier: AGPL-3.0-or-later
use p2panda_tests::Panda;
use p2panda_tests::utils::MESSAGE_SCHEMA;

fn main() {
    let mut panda = Panda::new(Panda::keypair());
    
    panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "hello!")]));
    panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "poop!")]));
    
    let (entry_encoded_1, _) = panda.get_encoded_entry_and_message(MESSAGE_SCHEMA, 1);
    panda.publish_entry(Panda::update_message(MESSAGE_SCHEMA, entry_encoded_1.hash(), vec![("message", "Smelly!")]));
    
    println!("{}", panda.to_json());
    println!("{}", panda.to_json_decoded());
}
