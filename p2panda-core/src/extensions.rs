// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::de::DeserializeOwned;
use serde::Serialize;

pub trait Extensions: Clone + Serialize + DeserializeOwned {}

impl Extensions for () {}
