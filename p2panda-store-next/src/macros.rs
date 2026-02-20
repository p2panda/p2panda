// SPDX-License-Identifier: MIT OR Apache-2.0

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
