// SPDX-License-Identifier: AGPL-3.0-or-later

//! Experimental materialization logic for use in the mock node implementation.
//! 
//! Only to be used in a testing environment!

mod dag;
mod materializer;
mod processor;

pub use dag::DAG;
pub use materializer::Materializer;
pub use processor::filter_entries;
