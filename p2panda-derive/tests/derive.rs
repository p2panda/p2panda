use p2panda_derive::{Topic, TopicId};
use p2panda_net::TopicId as TopicIdTrait;
use serde::{Deserialize, Serialize};

#[test]
fn topic_derive() {
    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Topic)]
    struct Test;

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Topic)]
    struct MultipleFields {
        a: String,
        b: u64,
        c: bool,
    }
}

#[test]
fn topic_id_derive() {
    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TopicId)]
    struct SingleUnnamedField(String);
    assert_eq!(SingleUnnamedField("hello".into()).id().len(), 32);

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TopicId)]
    struct MultipleUnnamedFields(String, String);
    assert_eq!(
        MultipleUnnamedFields("hello".into(), "again".into())
            .id()
            .len(),
        32
    );

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TopicId)]
    struct NamedFields {
        name: String,
        again: String,
    }

    assert_eq!(
        NamedFields {
            name: "panda".into(),
            again: "again".into(),
        }
        .id()
        .len(),
        32
    );
}
