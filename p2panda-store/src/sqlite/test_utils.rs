// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::sqlite::store::{
    Pool, connection_pool, create_database, drop_database, run_pending_migrations,
};

pub fn db_test_url() -> String {
    // Give each database a unique name.
    let db_name = format!("dbmem{}", rand::random::<u32>());

    // SQLite database stored in memory.
    let url = format!("sqlite://{db_name}?mode=memory&cache=private");

    url
}

pub async fn initialize_sqlite_db() -> Pool {
    let url = db_test_url();

    drop_database(&url).await.unwrap();
    create_database(&url).await.unwrap();

    let pool = connection_pool(&url, 1).await.unwrap();

    if run_pending_migrations(&pool).await.is_err() {
        pool.close().await;
        panic!("Database migration failed");
    }

    pool
}
