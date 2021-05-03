use crate::atomic::{Entry, EntrySigned, Hash, LogId, Message, MessageFields, MessageValue, SeqNum,};

/// Returns a Message for testing
pub fn mock_message(text: String) -> Message {
    let mut fields = MessageFields::new();
    fields
        .add("test", MessageValue::Text(text.to_owned()))
        .unwrap();
    Message::new_create(Hash::new_from_bytes(vec![1, 2, 3]).unwrap(), fields).unwrap()
}

/// Returns a mock first entry for log with Id of 1
pub fn mock_first_entry(message: Message) -> Entry {
    Entry::new(
        &LogId::default(), 
        Some(&message), 
        None, 
        None, 
        &SeqNum::new(1).unwrap()
    )
    .unwrap()
}

/// Returns a mock entry for log with Id of 1
pub fn mock_entry(message: Message, backlink: Option<EntrySigned>, skiplink: Option<EntrySigned>, seq_no: i64) -> Entry {
    
    let entry_hash_backlink: Option<Hash> = match backlink {
        Some(link) => Some(link.hash()),
        None => None,
    };

    let entry_hash_skiplink: Option<Hash> = match skiplink {
        Some(link) => Some(link.hash()),
        None => None,
    };

    Entry::new(
        &LogId::default(),
        Some(&message),
        entry_hash_skiplink.as_ref(),
        entry_hash_backlink.as_ref(),
        &SeqNum::new(seq_no).unwrap(),
    )
    .unwrap()
}