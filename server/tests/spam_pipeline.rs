//! Spam-pipeline corpus walker.
//!
//! `corpus_walks_all_yaml_files` actually iterates every `*.yaml` under
//! `tests/spam_corpus/` and runs each labelled sample through
//! `SpamService::inspect`. Adding a new rule means: (a) implement it in the
//! cascade, (b) drop a labelled YAML alongside, (c) run `cargo test --test
//! spam_pipeline -- --include-ignored`. A misclassification fails the test
//! and blocks merge — no per-file boilerplate needed.
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
    /// Each sample MUST yield `Verdict::Ban` on the first call. The harness
    /// pre-seeds `spam_messages` with the sample's xxh3 so the dedup branch
    /// fires deterministically — use this for samples that should be banned
    /// without depending on CAS.
    must_ban: Vec<String>,
    /// Each sample MUST yield `Verdict::Delete` on the first call (n-gram
    /// match without a dedup hit).
    must_delete: Vec<String>,
    /// Each sample MUST yield `Verdict::Allow` (clean text or below the
    /// MIN_NORMALIZED_LEN threshold).
    must_allow: Vec<String>,
    /// First call MUST yield `Verdict::Delete` (records the hash); second
    /// call from a different user MUST yield `Verdict::Ban` via dedup.
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

/// Connect to Redis. We deliberately do NOT `FLUSHDB`: the corpus tests force
/// `cas_enabled = FALSE` in chat_config, so the spam pipeline never touches
/// Redis. A global flush would have been racy with parallel test execution
/// inside this file.
async fn redis() -> Arc<Redis> {
    Arc::new(Redis::connect(REDIS_URL).await.expect("redis connect"))
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
    let redis = redis().await;
    // Base URL is unused once cas_enabled is FALSE in chat_config — the
    // CAS branch never runs, so we can pass any string.
    let cas = CasClient::new(redis, "http://localhost:0".to_string());
    SpamService::new(pool, cas)
}

/// Wipe the global `spam_messages` table between samples. The table has no
/// chat_id column, so without this every prior `must_delete`/`must_ban`
/// sample's hash would prime dedup and skew later assertions.
async fn reset_spam_messages(pool: &PgPool) {
    sqlx::query("DELETE FROM spam_messages")
        .execute(pool)
        .await
        .expect("DELETE spam_messages");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn corpus_walks_all_yaml_files(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool.clone()).await;

    let mut paths: Vec<PathBuf> = std::fs::read_dir(corpus_dir())
        .expect("read corpus_dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("yaml"))
        .collect();
    paths.sort();
    assert!(
        !paths.is_empty(),
        "no .yaml files under {}",
        corpus_dir().display()
    );

    for path in &paths {
        let corpus = load_corpus(path);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();

        for sample in &corpus.must_allow {
            reset_spam_messages(&pool).await;
            let v = svc
                .inspect(&mock_message_with_text(CHAT_ID, USER_ID, sample))
                .await
                .expect("inspect must_allow");
            assert!(
                matches!(v, Verdict::Allow),
                "{name}: must_allow {sample:?} returned {v:?}"
            );
        }

        for sample in &corpus.must_delete {
            reset_spam_messages(&pool).await;
            let v = svc
                .inspect(&mock_message_with_text(CHAT_ID, USER_ID, sample))
                .await
                .expect("inspect must_delete");
            assert!(
                matches!(v, Verdict::Delete { .. }),
                "{name}: must_delete {sample:?} returned {v:?}"
            );
        }

        for sample in &corpus.must_ban {
            // Pre-seed the dedup table with the sample's hash so the first
            // pass returns Ban via the dedup branch — without this, must_ban
            // would only fire if CAS were enabled (it isn't, in this suite).
            reset_spam_messages(&pool).await;
            seed_dedup(&pool, sample).await;
            let v = svc
                .inspect(&mock_message_with_text(CHAT_ID, USER_ID, sample))
                .await
                .expect("inspect must_ban");
            assert!(
                matches!(v, Verdict::Ban { .. }),
                "{name}: must_ban {sample:?} returned {v:?}"
            );
        }

        for sample in &corpus.must_ban_after_first {
            reset_spam_messages(&pool).await;
            let v1 = svc
                .inspect(&mock_message_with_text(CHAT_ID, USER_ID, sample))
                .await
                .expect("inspect must_ban_after_first first");
            assert!(
                matches!(v1, Verdict::Delete { .. }),
                "{name}: must_ban_after_first {sample:?} first call returned {v1:?}"
            );
            let v2 = svc
                .inspect(&mock_message_with_text(CHAT_ID, USER_ID + 1, sample))
                .await
                .expect("inspect must_ban_after_first second");
            assert!(
                matches!(v2, Verdict::Ban { .. }),
                "{name}: must_ban_after_first {sample:?} second call returned {v2:?}"
            );
        }
    }
}

/// Pre-seed `spam_messages` with the normalized hash of `sample`, so the
/// dedup branch fires on the next inspect call. Mirrors what the n-gram /
/// CAS branches would have written on a real first-pass hit.
async fn seed_dedup(pool: &PgPool, sample: &str) {
    use vixen_server::services::spam::normalize::normalize;
    use xxhash_rust::xxh3::xxh3_64;
    let normalized = normalize(sample);
    let hash = xxh3_64(normalized.as_bytes()) as i64;
    sqlx::query(
        "INSERT INTO spam_messages (xxh3_hash, sample_body) VALUES ($1, $2)
         ON CONFLICT (xxh3_hash) DO NOTHING",
    )
    .bind(hash)
    .bind(&normalized)
    .execute(pool)
    .await
    .expect("seed spam_messages");
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
