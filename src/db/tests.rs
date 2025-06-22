#[cfg(test)]
mod tests {
    use crate::db::DB;

    //use super::*;
    //use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tempfile::TempDir;
    use tokio::task::JoinSet;

    /// Helper to create a test database with a unique name
    async fn create_test_db() -> Result<(DB, TempDir), Box<dyn std::error::Error + Send + Sync>> {
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("test.db");
        let database_url = format!("sqlite://{}", db_path.display());

        let db = DB::connect(&database_url).await?;
        Ok((db, temp_dir))
    }

    /// Create test table for concurrent operations
    async fn setup_test_table(db: &DB) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS test_table (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                thread_id INTEGER NOT NULL,
                operation_id INTEGER NOT NULL,
                value TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(db.get_pool())
        .await?;

        Ok(())
    }

    /// cargo test test_basic_connection --release -- --nocapture
    #[tokio::test]
    async fn test_basic_connection() {
        let (db, _temp_dir) = create_test_db().await.expect("Failed to create test DB");

        // Test health check
        assert!(db.health_check().await.expect("Health check failed"));

        // Test basic query
        let result: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(db.get_pool())
            .await
            .expect("Basic query failed");
        assert_eq!(result, 1);
    }

    #[tokio::test]
    async fn test_concurrent_reads() {
        let (db, _temp_dir) = create_test_db().await.expect("Failed to create test DB");
        setup_test_table(&db)
            .await
            .expect("Failed to setup test table");

        // Insert some test data
        for i in 0..100 {
            sqlx::query("INSERT INTO test_table (thread_id, operation_id, value) VALUES (?, ?, ?)")
                .bind(0)
                .bind(i)
                .bind(format!("test_value_{}", i))
                .execute(db.get_pool())
                .await
                .expect("Failed to insert test data");
        }

        // Spawn multiple concurrent read operations
        let mut join_set = JoinSet::new();
        let read_count = 50;

        for i in 0..read_count {
            let db_clone = db.clone();
            join_set.spawn(async move {
                let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_table")
                    .fetch_one(db_clone.get_pool())
                    .await
                    .expect("Failed to count rows");
                (i, count)
            });
        }

        // Collect all results
        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            results.push(result.expect("Task failed"));
        }

        // All reads should return the same count
        assert_eq!(results.len(), read_count);
        for (_, count) in results {
            assert_eq!(count, 100);
        }
    }

    #[tokio::test]
    async fn test_concurrent_writes() {
        let (db, _temp_dir) = create_test_db().await.expect("Failed to create test DB");
        setup_test_table(&db)
            .await
            .expect("Failed to setup test table");

        let mut join_set = JoinSet::new();
        let thread_count = 10;
        let operations_per_thread = 20;
        let success_counter = Arc::new(AtomicU32::new(0));

        // Spawn multiple concurrent write operations
        for thread_id in 0..thread_count {
            let db_clone = db.clone();
            let counter_clone = success_counter.clone();

            join_set.spawn(async move {
                let mut local_successes = 0;

                for operation_id in 0..operations_per_thread {
                    let value = format!("thread_{}_op_{}", thread_id, operation_id);

                    let result = sqlx::query(
                        "INSERT INTO test_table (thread_id, operation_id, value) VALUES (?, ?, ?)",
                    )
                    .bind(thread_id)
                    .bind(operation_id)
                    .bind(&value)
                    .execute(db_clone.get_pool())
                    .await;

                    match result {
                        Ok(_) => local_successes += 1,
                        Err(e) => eprintln!(
                            "Write failed for thread {}, op {}: {}",
                            thread_id, operation_id, e
                        ),
                    }
                }

                counter_clone.fetch_add(local_successes, Ordering::SeqCst);
                (thread_id, local_successes)
            });
        }

        // Wait for all tasks to complete
        let mut thread_results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            thread_results.push(result.expect("Task failed"));
        }

        // Check that all writes succeeded
        let total_successes = success_counter.load(Ordering::SeqCst);
        let expected_total = thread_count * operations_per_thread;

        println!(
            "Total successful writes: {}/{}",
            total_successes, expected_total
        );
        assert_eq!(total_successes, expected_total);

        // Verify the data in the database
        let final_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_table")
            .fetch_one(db.get_pool())
            .await
            .expect("Failed to count final rows");

        assert_eq!(final_count as u32, expected_total);
    }

    #[tokio::test]
    async fn test_mixed_concurrent_operations() {
        let (db, _temp_dir) = create_test_db().await.expect("Failed to create test DB");
        setup_test_table(&db)
            .await
            .expect("Failed to setup test table");

        let mut join_set = JoinSet::new();
        let read_threads = 5;
        let write_threads = 5;
        let operations_per_thread = 10;

        let write_counter = Arc::new(AtomicU32::new(0));
        let read_counter = Arc::new(AtomicU32::new(0));

        // Spawn write threads
        for thread_id in 0..write_threads {
            let db_clone = db.clone();
            let counter_clone = write_counter.clone();

            join_set.spawn(async move {
                let mut successes = 0;
                for op_id in 0..operations_per_thread {
                    let result = sqlx::query(
                        "INSERT INTO test_table (thread_id, operation_id, value) VALUES (?, ?, ?)",
                    )
                    .bind(thread_id)
                    .bind(op_id)
                    .bind(format!("write_{}_{}", thread_id, op_id))
                    .execute(db_clone.get_pool())
                    .await;

                    if result.is_ok() {
                        successes += 1;
                    }

                    // Small delay to increase contention
                    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                }
                counter_clone.fetch_add(successes, Ordering::SeqCst);
                format!("writer_{}", thread_id)
            });
        }

        // Spawn read threads
        for thread_id in 0..read_threads {
            let db_clone = db.clone();
            let counter_clone = read_counter.clone();

            join_set.spawn(async move {
                let mut successes = 0;
                for _ in 0..operations_per_thread {
                    let result: Result<i64, _> =
                        sqlx::query_scalar("SELECT COUNT(*) FROM test_table")
                            .fetch_one(db_clone.get_pool())
                            .await;

                    if result.is_ok() {
                        successes += 1;
                    }

                    // Small delay to increase contention
                    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                }
                counter_clone.fetch_add(successes, Ordering::SeqCst);
                format!("reader_{}", thread_id)
            });
        }

        // Wait for all operations to complete
        let mut task_results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            task_results.push(result.expect("Task failed"));
        }

        let total_writes = write_counter.load(Ordering::SeqCst);
        let total_reads = read_counter.load(Ordering::SeqCst);
        let expected_writes = write_threads * operations_per_thread;
        let expected_reads = read_threads * operations_per_thread;

        println!(
            "Writes: {}/{}, Reads: {}/{}",
            total_writes, expected_writes, total_reads, expected_reads
        );

        // All operations should succeed
        assert_eq!(total_writes, expected_writes);
        assert_eq!(total_reads, expected_reads);
    }

    #[tokio::test]
    async fn test_transaction_concurrency() {
        let (db, _temp_dir) = create_test_db().await.expect("Failed to create test DB");
        setup_test_table(&db)
            .await
            .expect("Failed to setup test table");

        let mut join_set = JoinSet::new();
        let thread_count = 5;
        let success_counter = Arc::new(AtomicU32::new(0));

        // Each thread will perform a transaction with multiple operations
        for thread_id in 0..thread_count {
            let db_clone = db.clone();
            let counter_clone = success_counter.clone();

            join_set.spawn(async move {
                let mut tx = db_clone.begin().await.expect("Failed to begin transaction");

                // Perform multiple operations in a transaction
                for op_id in 0..5 {
                    let result = sqlx::query(
                        "INSERT INTO test_table (thread_id, operation_id, value) VALUES (?, ?, ?)",
                    )
                    .bind(thread_id)
                    .bind(op_id)
                    .bind(format!("tx_{}_{}", thread_id, op_id))
                    .execute(&mut *tx)
                    .await;

                    if result.is_err() {
                        tx.rollback().await.ok();
                        return (thread_id, false);
                    }
                }

                // Commit the transaction
                match tx.commit().await {
                    Ok(_) => {
                        counter_clone.fetch_add(1, Ordering::SeqCst);
                        (thread_id, true)
                    }
                    Err(_) => (thread_id, false),
                }
            });
        }

        // Wait for all transactions to complete
        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            results.push(result.expect("Task failed"));
        }

        let successful_transactions = success_counter.load(Ordering::SeqCst);
        println!(
            "Successful transactions: {}/{}",
            successful_transactions, thread_count
        );

        // All transactions should succeed
        assert_eq!(successful_transactions, thread_count);

        // Verify total rows (5 operations per successful transaction)
        let total_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_table")
            .fetch_one(db.get_pool())
            .await
            .expect("Failed to count rows");

        assert_eq!(total_rows as u32, successful_transactions * 5);
    }

    #[tokio::test]
    async fn test_high_contention_stress() {
        let (db, _temp_dir) = create_test_db().await.expect("Failed to create test DB");
        setup_test_table(&db)
            .await
            .expect("Failed to setup test table");

        // Create a single counter table for high contention
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS counter (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                value INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(db.get_pool())
        .await
        .expect("Failed to create counter table");

        sqlx::query("INSERT INTO counter (id, value) VALUES (1, 0)")
            .execute(db.get_pool())
            .await
            .expect("Failed to initialize counter");

        let mut join_set = JoinSet::new();
        let thread_count = 20;
        let increments_per_thread = 10;
        let success_counter = Arc::new(AtomicU32::new(0));

        // Each thread will try to increment the same counter
        for thread_id in 0..thread_count {
            let db_clone = db.clone();
            let counter_clone = success_counter.clone();

            join_set.spawn(async move {
                let mut successes = 0;

                for _ in 0..increments_per_thread {
                    // Retry logic for high contention scenarios
                    let mut retries = 0;
                    let max_retries = 5;

                    while retries < max_retries {
                        let result =
                            sqlx::query("UPDATE counter SET value = value + 1 WHERE id = 1")
                                .execute(db_clone.get_pool())
                                .await;

                        match result {
                            Ok(_) => {
                                successes += 1;
                                break;
                            }
                            Err(e) if retries < max_retries - 1 => {
                                eprintln!("Thread {} retry {}: {}", thread_id, retries, e);
                                retries += 1;
                                tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                            }
                            Err(e) => {
                                eprintln!("Thread {} failed after retries: {}", thread_id, e);
                                break;
                            }
                        }
                    }
                }

                counter_clone.fetch_add(successes, Ordering::SeqCst);
                (thread_id, successes)
            });
        }

        // Wait for all operations to complete
        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            results.push(result.expect("Task failed"));
        }

        let total_successes = success_counter.load(Ordering::SeqCst);
        let expected_total = thread_count * increments_per_thread;

        println!(
            "High contention test - Successful increments: {}/{}",
            total_successes, expected_total
        );

        // Verify the final counter value matches successful operations
        let final_value: i64 = sqlx::query_scalar("SELECT value FROM counter WHERE id = 1")
            .fetch_one(db.get_pool())
            .await
            .expect("Failed to read final counter value");

        assert_eq!(final_value as u32, total_successes);

        // In high contention, we might have some failures, but should get most operations
        // Let's be realistic and expect at least 80% success rate
        let success_rate = (total_successes as f64) / (expected_total as f64);
        println!("Success rate: {:.2}%", success_rate * 100.0);
        assert!(
            success_rate >= 0.8,
            "Success rate too low: {:.2}%",
            success_rate * 100.0
        );
    }

    #[tokio::test]
    async fn test_wal_mode_enabled() {
        let (db, _temp_dir) = create_test_db().await.expect("Failed to create test DB");

        // Check that WAL mode is enabled
        let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(db.get_pool())
            .await
            .expect("Failed to check journal mode");

        assert_eq!(
            journal_mode.to_lowercase(),
            "wal",
            "WAL mode should be enabled"
        ); // Check other pragma settings
        let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
            .fetch_one(db.get_pool())
            .await
            .expect("Failed to check synchronous setting");

        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(db.get_pool())
            .await
            .expect("Failed to check foreign keys setting");

        println!("Journal mode: {}", journal_mode);
        println!("Synchronous: {} (1=NORMAL, 2=FULL)", synchronous);
        println!("Foreign keys: {}", foreign_keys);

        assert_eq!(foreign_keys, 1, "Foreign keys should be enabled");
        assert!(
            synchronous >= 1,
            "Synchronous should be NORMAL (1) or FULL (2)"
        );
    }
}
