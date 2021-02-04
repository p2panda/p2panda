use crate::Result;
use crate::atomic::Entry;

#[derive(Clone, Debug)]
pub struct EntryEncoded(String);

impl EntryEncoded {
    pub fn new(_value: String) -> Result<Self> {
        todo!();
    }

    pub fn decode(&self) -> Entry {
        todo!();
    }
}
