// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::hash::Hash;
use crate::operation::OperationFields;

trait AsNode {
    fn key(&self) -> Hash;

    fn previous(&self) -> Vec<Hash>;

    fn data(&self) -> OperationFields;

    fn is_root(&self) -> bool {
        self.previous().is_empty()
    }

    fn has_many_previous(&self) -> bool {
        self.previous().len() > 1
    }
}
