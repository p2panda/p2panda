// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::Header;

/// Extensions can be used to define custom fields in the operation header.
pub trait Extensions:
    Clone + Debug + Default + for<'de> Deserialize<'de> + Serialize + Send + Sync
{
}

impl<T> Extensions for T where
    T: Clone + Debug + Default + for<'de> Deserialize<'de> + Serialize + Send + Sync
{
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct DefaultExtensions {}

impl<T> Extension<T> for DefaultExtensions {
    fn extract(&self) -> Option<T> {
        None
    }
}

pub trait Extension<T>: Extensions {
    fn extract(&self) -> Option<T> {
        None
    }
}

impl<T, E> Extension<T> for Header<E>
where
    E: Extension<T>,
{
    fn extract(&self) -> Option<T> {
        match &self.extensions {
            Some(extensions) => Extension::<T>::extract(extensions),
            None => None,
        }
    }
}
