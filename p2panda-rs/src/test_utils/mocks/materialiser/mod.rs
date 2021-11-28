// SPDX-License-Identifier: AGPL-3.0-or-later

//! Experimental materialisation logic for use in the mock node implementation.
//!
//! This is experimental code which implements functionality which doesn't exist in the
//! core library yet. It will be replaced as the official modules are developed. For this reason
//! it is only intended to be in the mock node implementation.

mod dag;
mod materialiser;
mod processor;

pub use dag::DAG;
pub use materialiser::Materialiser;
pub use processor::filter_entries;
