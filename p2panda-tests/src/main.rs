// SPDX-License-Identifier: AGPL-3.0-or-later
use p2panda_tests::Panda;
use p2panda_tests::utils::MESSAGE_SCHEMA;

use p2panda_rs::entry::decode_entry;

fn main() {
    let mut panda = Panda::new(Panda::keypair());
    panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "hello!")]));
    panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "poop!")]));
    let entry1 = &panda.logs.get(MESSAGE_SCHEMA).unwrap()[0];
    let decoded_entry1 = decode_entry(&entry1.0, Some(&entry1.1)).unwrap();
    let entry2 = &panda.logs.get(MESSAGE_SCHEMA).unwrap()[1];
    let decoded_entry2 = decode_entry(&entry2.0, Some(&entry2.1)).unwrap();
    println!("{:#?}", panda.logs);
    println!("{:#?}", decoded_entry1);
    println!("{:#?}", decoded_entry2);
}
