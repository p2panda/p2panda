// SPDX-License-Identifier: AGPL-3.0-or-later

mod dag;
mod utils;
mod error;

pub use error::MaterialisationError;
pub use dag::{Node, Edge, DAG};
pub use utils::marshall_entries;
