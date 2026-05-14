// SPDX-License-Identifier: MIT OR Apache-2.0

/// Acquire a permit and execute one or more database queries within a single transaction.
///
/// This macro takes a store as the first argument and a code block as the second.
///
/// Internally it acquires a permit, executes the given code block and then commits the transaction
/// before returning with the result of the code block.
///
/// ## Example
///
/// ```rust,ignore
/// tx!(store, {
///     store.set_cursor(&new_cursor).await?;
/// });
/// ```
#[macro_export]
macro_rules! tx {
    ($store:expr, $body:expr) => {{
        use $crate::Transaction;
        let permit = $store.begin().await?;
        let result = $body;
        $store.commit(permit).await?;
        result
    }};
}

#[macro_export]
#[cfg(any(test, feature = "test_utils"))]
macro_rules! tx_unwrap {
    ($store:expr, $body:expr) => {{
        use $crate::Transaction;
        let permit = $store.begin().await.unwrap();
        let result = $body;
        $store.commit(permit).await.unwrap();
        result
    }};
}
