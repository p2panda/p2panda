// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

pub trait Message: Serialize + for<'de> Deserialize<'de> {}
