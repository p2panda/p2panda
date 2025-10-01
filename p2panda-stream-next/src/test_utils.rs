// SPDX-License-Identifier: MIT OR Apache-2.0

use std::task::Poll;

use futures_test::task::noop_context;
use tokio::pin;

/// Compare the resulting poll state from a future.
pub fn assert_poll_eq<Fut: Future>(fut: Fut, poll: Poll<Fut::Output>)
where
    <Fut as Future>::Output: PartialEq + std::fmt::Debug,
{
    assert_eq!(
        {
            pin!(fut);
            let mut cx = noop_context();
            fut.poll(&mut cx)
        },
        poll,
    );
}
