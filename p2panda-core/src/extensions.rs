// SPDX-License-Identifier: AGPL-3.0-or-later

pub trait Extension<T> {
    fn extract(&self) -> &T;
}
