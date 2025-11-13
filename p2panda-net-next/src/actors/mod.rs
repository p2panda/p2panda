// SPDX-License-Identifier: MIT OR Apache-2.0

mod address_book;
mod discovery;
mod endpoint_supervisor;
mod events;
mod gossip;
pub mod iroh;
mod stream_supervisor;
pub mod streams;
pub mod supervisor;
pub mod sync;

use p2panda_core::PublicKey;

/// A node-unique string which is appended to actor names.
///
/// Named actors are registered in a global registry. This can lead to panics if two or more nodes
/// are run in a single process or on a single machine. We prevent such conflicts by ensuring that
/// each node has a unique string which is appended to the actor name. For example, the "events"
/// actor might end up as "events+706C65" (where the namespace is "706C65").
pub type ActorNamespace = String;

/// Takes a public key and returns `+` with the last six characters of the associated public key.
pub fn generate_actor_namespace(public_key: &PublicKey) -> ActorNamespace {
    public_key.to_hex()[..6].to_string()
}

/// Combines an actor's name with a node-unique namespace as suffix.
pub fn with_namespace(name: &str, namespace: &ActorNamespace) -> String {
    format!("{name}+{namespace}")
}

/// Removes the node-unique namespace suffix from an actor's name.
pub fn without_namespace(name: &str) -> &str {
    &name[..name.len() - 7]
}

#[cfg(test)]
mod tests {
    use p2panda_core::PrivateKey;

    use super::{generate_actor_namespace, with_namespace, without_namespace};

    #[test]
    fn add_and_remove_actor_name_suffix() {
        let private_key = PrivateKey::new();
        let namespace = generate_actor_namespace(&private_key.public_key());

        let name = "mountain";

        let name_with_namespace = with_namespace(name, &namespace);
        let name_without_namespace = without_namespace(&name_with_namespace);

        assert_eq!(name, name_without_namespace);
    }
}
