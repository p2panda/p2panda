use p2panda_rs::entry::EntrySigned;
use p2panda_rs::hash::Hash;
use p2panda_rs::identity::Author;
use p2panda_rs::message::Message;

// This struct encapsulates the properties we need to store in our logs in order
// to materialize the instances later on. In particular it has a `instance_backlink`
// which our panda entries currently don't have.
#[derive(Clone, Debug)]
pub struct LogEntry {
    author: Author,
    instance_author: Option<String>,
    entry_encoded: EntrySigned,
    message: Message,
    instance_backlink: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Log {
    id: i64,
    schema: String,
    entries: Vec<LogEntry>,
}

/// This is a helper struct which wraps data needed for materialization
/// as well ad several meta data values which don't have a place to live in current
/// p2panda architecture. These all need to be placed somewhere or expanded into other concepts.
/// Most important/new are instance_backlink and is_permitted
impl LogEntry {
    pub fn new(
        author: Author,
        instance_author: Option<String>,
        entry_encoded: EntrySigned,
        message: Message,
        instance_backlink: Option<String>,
    ) -> Self {
        Self {
            author,
            instance_author,
            entry_encoded,
            message,
            instance_backlink,
        }
    }

    pub fn author(&self) -> String {
        self.author.as_str().to_string().clone()
    }

    pub fn entry_encoded(&self) -> Hash {
        self.entry_encoded.hash().clone()
    }

    pub fn instance_author(&self) -> String {
        self.instance_author.clone().unwrap().as_str().to_string()
    }

    pub fn message(&self) -> Message {
        self.message.clone()
    }

    pub fn id(&self) -> String {
        self.entry_encoded.hash().as_str().to_string().clone()
    }

    pub fn instance_backlink(&self) -> Option<String> {
        self.instance_backlink.to_owned().clone()
    }
}

impl Log {
    pub fn new(log_id: i64, schema: String) -> Self {
        Self {
            id: log_id,
            schema: schema.into(),
            entries: Vec::new(),
        }
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.to_owned()
    }

    pub fn id(&self) -> i64 {
        self.id.to_owned()
    }

    pub fn schema(&self) -> String {
        self.schema.to_owned()
    }

    pub fn add_entry(&mut self, entry: LogEntry) {
        self.entries.push(entry)
    }
}
