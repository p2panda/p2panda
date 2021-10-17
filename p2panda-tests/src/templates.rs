
use rstest_reuse::template;

#[template]
#[rstest]
#[should_panic]
#[case(message(Some(vec![("message", "Boo!")]), None))]
#[should_panic]
#[case(message(Some(vec![("date", "2021-05-02T20:06:45.430Z")]), None))]
#[should_panic]
#[case(message(Some(vec![("message", "Hello!"), ("date", "2021-05-02T20:06:45.430Z")]), None))]
fn messages_not_matching_entry_should_fail(entry: Entry, #[case] message: Message, key_pair: KeyPair) {}        

pub (crate) use messages_not_matching_entry_should_fail;
