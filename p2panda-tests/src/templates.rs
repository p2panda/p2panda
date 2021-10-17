
use rstest_reuse::template;
use crate::utils::{Fixture, MESSAGE_SCHEMA};
use crate::Panda;

#[template]
#[rstest]
#[should_panic]
#[case(message(Some(vec![("message", "Boo!")]), None))]
#[should_panic]
#[case(message(Some(vec![("date", "2021-05-02T20:06:45.430Z")]), None))]
#[should_panic]
#[case(message(Some(vec![("message", "Hello!"), ("date", "2021-05-02T20:06:45.430Z")]), None))]
fn messages_not_matching_entry_should_fail(entry: Entry, #[case] message: Message, key_pair: KeyPair) {}        

#[template]
#[rstest]
#[case::first_entry(first_entry())]
#[case::entry_with_backlink(entry_with_backlink())]
#[case::entry_with_backlink_and_skiplink(entry_with_backlink_and_skiplink())]
fn many_entry_versions(#[case] entry: Entry, key_pair: KeyPair) {}        

#[template]
#[rstest]
#[case::create_message(create_message())]
#[case::update_message(update_message())]
#[case::delete_message(delete_message())]
fn all_message_types(entry: Entry, #[case] message: Message, key_pair: KeyPair) {}        

pub (crate) use messages_not_matching_entry_should_fail;
pub (crate) use many_entry_versions;
pub (crate) use all_message_types;
