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

/// Returns a mock second entry for log with Id of 1
pub fn mock_second_entry(first_entry: EntrySigned, message: Message) -> Entry {
    Entry::new(
        &LogId::default(),
        Some(&message),
        None,
        Some(&first_entry.hash()),
        &SeqNum::new(2).unwrap(),
    )
    .unwrap()
}