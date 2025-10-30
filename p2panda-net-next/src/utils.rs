// SPDX-License-Identifier: MIT OR Apache-2.0

/// Combines an actor's name with a node-unique suffix.
pub(crate) fn with_suffix(name: &str, suffix: &str) -> String {
    format!("{}+{}", name, suffix)
}

/// Removes the node-unique suffix from an actor's name.
pub(crate) fn without_suffix(name: &str) -> &str {
    &name[..name.len() - 7]
}

#[cfg(test)]
mod tests {
    use p2panda_core::PrivateKey;

    use super::{with_suffix, without_suffix};

    #[test]
    fn add_and_remove_actor_name_suffix() {
        let private_key = PrivateKey::new();
        let public_key_suffix = &private_key.public_key().to_hex()[..6];

        let name = "mountain";

        let name_with_suffix = with_suffix(name, public_key_suffix);
        let name_without_suffix = without_suffix(&name_with_suffix);

        assert_eq!(name, name_without_suffix);
    }
}
