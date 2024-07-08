// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::Operation;

pub trait Extensions: Clone + Serialize + DeserializeOwned {}

impl Extensions for () {}

pub trait Extension<Output>
where
    Self: Extensions,
{
    fn extract(operation: &Operation<Self>) -> Output;
}
