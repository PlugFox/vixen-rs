//! Spam-pipeline corpus walker.
//!
//! Loads every YAML in `tests/spam_corpus/` and runs each sample through
//! `SpamService::inspect`. Adding a new rule means: (a) implement it in the
//! cascade, (b) drop a labelled YAML alongside, (c) run `cargo test --test
//! spam_pipeline`. A misclassification fails the test and blocks merge.
//!
//! `#[ignore]`-gated because the pipeline needs Postgres + Redis; CI's
//! `integration` job runs `--include-ignored`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use sqlx::PgPool;
use teloxide::types::Message;
use teloxide_tests::{MockMessageText, MockSupergroupChat, MockUser};
use vixen_server::database::Redis;
use vixen_server::services::cas_client::CasClient;
use vixen_server::services::spam::service::{SpamService, Verdict};

const CHAT_ID: i64 = -1001234567890;
const USER_ID: u64 = 4242;
const REDIS_URL: &str = "redis://localhost:6379/14";

#[derive(Default, Debug, Deserialize)]
#[serde(default)]
struct CorpusFile {
    must_ban: Vec<String>,
    must_delete: Vec<String>,
    must_allow: Vec<String>,
    must_ban_after_first: Vec<String>,
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spam_corpus")
}

fn load_corpus(path: &Path) -> CorpusFile {
    let raw =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_yaml::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

async fn seed_chat(pool: &PgPool, chat_id: i64) {
    sqlx::query("INSERT INTO chats (chat_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(chat_id)
        .execute(pool)
        .await
        .expect("seed chats");
    // CAS disabled in corpus tests — corpus exercises dedup + n-gram,
    // CAS has its own dedicated wiremock suite.
    sqlx::query(
        "INSERT INTO chat_config (chat_id, cas_enabled) VALUES ($1, FALSE)
         ON CONFLICT (chat_id) DO UPDATE SET cas_enabled = FALSE",
    )
    .bind(chat_id)
    .execute(pool)
    .await
    .expect("seed chat_config");
}

async fn fresh_redis() -> Arc<Redis> {
    let r = Redis::connect(REDIS_URL).await.expect("redis connect");
    let mut conn = r.pool().get().await.expect("pool acquire");
    let _: () = redis::cmd("FLUSHDB")
        .query_async(&mut *conn)
        .await
        .expect("flushdb");
    Arc::new(r)
}

fn mock_message_with_text(chat_id: i64, user_id: u64, text: &str) -> Message {
    let chat = MockSupergroupChat::new().id(chat_id).build();
    let user = MockUser::new().id(user_id).build();
    MockMessageText::new()
        .text(text)
        .chat(chat)
        .from(user)
        .id(rand_message_id())
        .build()
}

fn rand_message_id() -> i32 {
    use std::sync::atomic::{AtomicI32, Ordering};
    static N: AtomicI32 = AtomicI32::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

async fn make_service(pool: PgPool) -> SpamService {
    let redis = fresh_redis().await;
    // Base URL is unused once cas_enabled is FALSE in chat_config — the
    // CAS branch never runs, so we can pass any string.
    let cas = CasClient::new(redis, "http://localhost:0".to_string());
    SpamService::new(pool, cas)
}

// ── per-corpus tests ─────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn corpus_phrase_match(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool).await;
    let corpus = load_corpus(&corpus_dir().join("phrase_match.yaml"));

    for sample in &corpus.must_delete {
        let msg = mock_message_with_text(CHAT_ID, USER_ID, sample);
        let v = svc.inspect(&msg).await.expect("inspect");
        assert!(
            matches!(v, Verdict::Delete { .. }),
            "expected Delete, got {v:?} for sample: {sample:?}"
        );
    }

    for sample in &corpus.must_allow {
        let msg = mock_message_with_text(CHAT_ID, USER_ID, sample);
        let v = svc.inspect(&msg).await.expect("inspect");
        assert!(
            matches!(v, Verdict::Allow),
            "expected Allow, got {v:?} for sample: {sample:?}"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn corpus_clean_messages(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool).await;
    let corpus = load_corpus(&corpus_dir().join("clean_messages.yaml"));

    for sample in &corpus.must_allow {
        let msg = mock_message_with_text(CHAT_ID, USER_ID, sample);
        let v = svc.inspect(&msg).await.expect("inspect");
        assert!(
            matches!(v, Verdict::Allow),
            "expected Allow, got {v:?} for clean sample: {sample:?}"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn corpus_xxh3_dedup_second_pass_bans(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool).await;
    let corpus = load_corpus(&corpus_dir().join("xxh3_dedup.yaml"));

    for sample in &corpus.must_ban_after_first {
        let msg1 = mock_message_with_text(CHAT_ID, USER_ID, sample);
        let v1 = svc.inspect(&msg1).await.expect("inspect first");
        assert!(
            matches!(v1, Verdict::Delete { .. }),
            "first pass should Delete (n-gram), got {v1:?} for: {sample:?}"
        );

        let msg2 = mock_message_with_text(CHAT_ID, USER_ID + 1, sample);
        let v2 = svc.inspect(&msg2).await.expect("inspect second");
        assert!(
            matches!(v2, Verdict::Ban { .. }),
            "second pass should Ban (dedup), got {v2:?} for: {sample:?}"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn ngram_match_records_for_dedup(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool.clone()).await;

    let body = "Заработок в интернете на дому без вложений, пишите в лс для подробностей";
    let msg = mock_message_with_text(CHAT_ID, USER_ID, body);
    let v = svc.inspect(&msg).await.expect("inspect");
    assert!(matches!(v, Verdict::Delete { .. }), "got {v:?}");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM spam_messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "n-gram hit should record one row in spam_messages"
    );
}
