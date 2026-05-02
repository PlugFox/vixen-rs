//! `spam_cleanup` job — retention test. Pool-only; no Telegram bot required.

use sqlx::PgPool;
use vixen_server::jobs::spam_cleanup;

async fn seed_chat(pool: &PgPool, chat_id: i64) {
    sqlx::query("INSERT INTO chats (chat_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(chat_id)
        .execute(pool)
        .await
        .expect("seed chats");
}

async fn seed_spam_row(pool: &PgPool, hash: i64, days_old: i32) {
    sqlx::query(
        "INSERT INTO spam_messages (xxh3_hash, sample_body, last_seen)
         VALUES ($1, $2, NOW() - make_interval(days => $3::int))",
    )
    .bind(hash)
    .bind(format!("sample {hash}"))
    .bind(days_old)
    .execute(pool)
    .await
    .expect("seed spam_messages");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn prune_drops_only_aged_rows(pool: PgPool) {
    seed_chat(&pool, -100).await;
    seed_spam_row(&pool, 1, 1).await;
    seed_spam_row(&pool, 2, 7).await;
    seed_spam_row(&pool, 3, 20).await;
    seed_spam_row(&pool, 4, 100).await;

    let pruned = spam_cleanup::prune_expired(&pool, 14).await.expect("prune");
    assert_eq!(pruned, 2, "only the 20d and 100d rows should go");

    let remaining: Vec<i64> =
        sqlx::query_scalar("SELECT xxh3_hash FROM spam_messages ORDER BY xxh3_hash")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, vec![1, 2]);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn prune_is_idempotent_when_empty(pool: PgPool) {
    seed_chat(&pool, -100).await;
    let pruned = spam_cleanup::prune_expired(&pool, 14).await.unwrap();
    assert_eq!(pruned, 0);

    // Second call: still 0, no error.
    let pruned = spam_cleanup::prune_expired(&pool, 14).await.unwrap();
    assert_eq!(pruned, 0);
}
