// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::hash::Hash;
use crate::operation::OperationFields;

pub trait AsNode {
    fn key(&self) -> &Hash;

    fn previous(&self) -> Option<&Vec<Hash>>;

    fn data(&self) -> Option<&OperationFields>;

    fn is_root(&self) -> bool {
        self.previous().is_none()
    }

    fn has_many_previous(&self) -> bool {
        match self.previous() {
            Some(previous) => previous.len() > 1,
            None => false,
        }
    }
}

    }
}
