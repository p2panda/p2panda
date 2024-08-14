// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::Header;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct DefaultExtensions {}

impl<T> Extension<T> for DefaultExtensions {
    fn extract(&self) -> Option<T> {
        None
    }
}

pub trait Extension<T> {
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
