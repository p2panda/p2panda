use std::error::Error;
use std::fmt::Debug;

use super::IdentityHandle;

pub trait GroupStore<ID, G>
where
    ID: IdentityHandle,
{
    type State: Clone + Debug;

    type Error: Error;

    fn insert(y: Self::State, id: &ID, group: &G) -> Result<Self::State, Self::Error>;

    fn get(y: &Self::State, id: &ID) -> Result<Option<G>, Self::Error>;
}
