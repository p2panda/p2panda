// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Header;

pub trait Extension<T> {
    fn extract(&self) -> Option<T>;
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
