// SPDX-License-Identifier: AGPL-3.0-or-later

extern crate p2panda_rs;

use std::convert::TryFrom;

use p2panda_rs::identity::{Author, AuthorError, KeyPair};

fn main() -> Result<(), AuthorError> {
    // Long comment about key pairs....
    let key_pair = KeyPair::new();

    // Authors blah blah blah....
    let author = Author::try_from(key_pair.public_key().to_owned())?;

    // here's a short verison for debugging.
    println!("{}", author);

    // And a string representation for when that is what you neeeeeeed.
    println!("{}", author.as_str());

    Ok(())
}
