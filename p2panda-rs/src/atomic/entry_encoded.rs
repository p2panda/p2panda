use crate::atomic::Entry;
use crate::error::Result;

#[derive(Clone, Debug)]
pub struct EntryEncoded(String);

impl EntryEncoded {
    pub fn new(value: String) -> Result<Self> {
        todo!();
    }

    pub fn decode(&self) -> Entry {
        todo!();
    }
}
