// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::test_utils::setup_logging;

#[tokio::test]
async fn spaces_api() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    use p2panda::Topic;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct SecretData {
        message: String,
    }

    let node = p2panda::spawn().await?;
    let topic = Topic::random();

    let (_tx, _rx) = node.create_space::<SecretData>(topic, &[]).await?;

    Ok(())
}
