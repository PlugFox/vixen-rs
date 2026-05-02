//! Integration tests for the captcha pipeline.
//!
//! Each `#[sqlx::test]` gets a fresh database with the migrations applied;
//! tests are isolated and can run concurrently. There is no Telegram I/O
//! here — the service is Telegram-free; the bot calls live in handlers and
//! the expiry job, which we cover with manual end-to-end testing.

use sqlx::PgPool;
use vixen_server::services::captcha::{CaptchaService, Fonts, Outcome, solution_for};

const CHAT_ID: i64 = -1001234567890;
const USER_ID: i64 = 42;

fn make_service(pool: PgPool) -> CaptchaService {
    let fonts = Fonts::load().expect("load fonts");
    CaptchaService::new(pool, fonts)
}

async fn seed_chat(pool: &PgPool, chat_id: i64) {
    sqlx::query("INSERT INTO chats (chat_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(chat_id)
        .execute(pool)
        .await
        .expect("seed chats");
    sqlx::query("INSERT INTO chat_config (chat_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(chat_id)
        .execute(pool)
        .await
        .expect("seed chat_config");
}

// ── issue ─────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn issue_writes_row_and_returns_image(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool.clone());

    let issued = svc
        .issue_challenge(CHAT_ID, USER_ID)
        .await
        .expect("issue challenge");

    let expected_solution = solution_for(issued.challenge_id);
    assert_eq!(expected_solution.len(), 4);
    assert!(!issued.image_webp.is_empty());
    assert!(issued.image_webp.len() <= 30_000);
    assert_eq!(issued.attempts_left, 5);

    let row = sqlx::query!(
        "SELECT id, solution, attempts_left FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2",
        CHAT_ID,
        USER_ID,
    )
    .fetch_one(&pool)
    .await
    .expect("row exists");
    assert_eq!(row.id, issued.challenge_id);
    assert_eq!(row.solution, expected_solution);
}

#[sqlx::test(migrations = "./migrations")]
async fn issue_is_idempotent_per_user(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool.clone());

    let first = svc.issue_challenge(CHAT_ID, USER_ID).await.unwrap();
    let second = svc.issue_challenge(CHAT_ID, USER_ID).await.unwrap();

    // Re-issue overwrites the row (UNIQUE (chat_id,user_id)).
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM captcha_challenges WHERE chat_id = $1")
            .bind(CHAT_ID)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "exactly one row per (chat,user)");
    assert_ne!(
        first.challenge_id, second.challenge_id,
        "second issue brings a fresh UUID"
    );
}

// ── solve ─────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn solve_correct_marks_verified(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool.clone());
    let issued = svc.issue_challenge(CHAT_ID, USER_ID).await.unwrap();
    let solution = solution_for(issued.challenge_id);

    let outcome = svc.solve(CHAT_ID, USER_ID, &solution).await.unwrap();
    assert_eq!(outcome, Outcome::Solved);

    let verified: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM verified_users WHERE chat_id = $1 AND user_id = $2)",
    )
    .bind(CHAT_ID)
    .bind(USER_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(verified);

    let challenges: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2",
    )
    .bind(CHAT_ID)
    .bind(USER_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(challenges, 0, "challenge row deleted on solve");

    // Idempotent re-fire — already verified.
    let second = svc.solve(CHAT_ID, USER_ID, &solution).await.unwrap();
    assert_eq!(second, Outcome::AlreadyVerified);
}

#[sqlx::test(migrations = "./migrations")]
async fn solve_wrong_decrements_attempts(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool.clone());
    let issued = svc.issue_challenge(CHAT_ID, USER_ID).await.unwrap();
    let wrong = wrong_solution(&solution_for(issued.challenge_id));

    let r1 = svc.solve(CHAT_ID, USER_ID, &wrong).await.unwrap();
    assert_eq!(r1, Outcome::WrongLeft(4));
    let r2 = svc.solve(CHAT_ID, USER_ID, &wrong).await.unwrap();
    assert_eq!(r2, Outcome::WrongLeft(3));

    let attempts: i16 = sqlx::query_scalar(
        "SELECT attempts_left FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2",
    )
    .bind(CHAT_ID)
    .bind(USER_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(attempts, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn solve_final_wrong_kicks(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool.clone());
    let issued = svc.issue_challenge(CHAT_ID, USER_ID).await.unwrap();
    let wrong = wrong_solution(&solution_for(issued.challenge_id));

    // Defaults: 5 attempts. The fifth wrong is the final.
    for expected in [4i16, 3, 2, 1] {
        let outcome = svc.solve(CHAT_ID, USER_ID, &wrong).await.unwrap();
        assert_eq!(outcome, Outcome::WrongLeft(expected));
    }
    let last = svc.solve(CHAT_ID, USER_ID, &wrong).await.unwrap();
    assert_eq!(last, Outcome::WrongFinal);

    // Challenge row gone, ledger row present.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2",
    )
    .bind(CHAT_ID)
    .bind(USER_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);

    let actions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM moderation_actions WHERE chat_id = $1 AND target_user_id = $2 AND action = 'captcha_failed'",
    )
    .bind(CHAT_ID)
    .bind(USER_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(actions, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn solve_expired_returns_expired(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool.clone());
    let issued = svc.issue_challenge(CHAT_ID, USER_ID).await.unwrap();

    sqlx::query("UPDATE captcha_challenges SET expires_at = NOW() - INTERVAL '5 seconds' WHERE chat_id = $1")
        .bind(CHAT_ID)
        .execute(&pool)
        .await
        .unwrap();

    let outcome = svc
        .solve(CHAT_ID, USER_ID, &solution_for(issued.challenge_id))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Expired);
}

#[sqlx::test(migrations = "./migrations")]
async fn solve_no_row_returns_not_found(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool.clone());
    let outcome = svc.solve(CHAT_ID, 9999, "0000").await.unwrap();
    assert_eq!(outcome, Outcome::NotFound);
}

// ── concurrency ───────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_solve_picks_one_winner(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool.clone());
    let issued = svc.issue_challenge(CHAT_ID, USER_ID).await.unwrap();
    let solution = solution_for(issued.challenge_id);

    let svc1 = svc.clone();
    let svc2 = svc.clone();
    let s1 = solution.clone();
    let s2 = solution.clone();
    let h1 = tokio::spawn(async move { svc1.solve(CHAT_ID, USER_ID, &s1).await });
    let h2 = tokio::spawn(async move { svc2.solve(CHAT_ID, USER_ID, &s2).await });

    let r1 = h1.await.unwrap().unwrap();
    let r2 = h2.await.unwrap().unwrap();
    let outcomes = (r1, r2);

    // Exactly one Solved + exactly one AlreadyVerified — order is racy.
    let solved_count =
        matches!(outcomes.0, Outcome::Solved) as u8 + matches!(outcomes.1, Outcome::Solved) as u8;
    let already_count = matches!(outcomes.0, Outcome::AlreadyVerified) as u8
        + matches!(outcomes.1, Outcome::AlreadyVerified) as u8;
    assert_eq!(
        solved_count, 1,
        "exactly one solver must win, got {outcomes:?}"
    );
    assert_eq!(
        already_count, 1,
        "the loser sees AlreadyVerified, got {outcomes:?}"
    );
}

// ── verify_manual ─────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn verify_manual_writes_ledger(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;
    let svc = make_service(pool.clone());
    let _ = svc.issue_challenge(CHAT_ID, USER_ID).await.unwrap();

    let outcome = svc.verify_manual(CHAT_ID, USER_ID, 555).await.unwrap();
    assert_eq!(outcome, Outcome::Solved);

    let row = sqlx::query!(
        r#"SELECT actor_kind, actor_user_id FROM moderation_actions
           WHERE chat_id = $1 AND target_user_id = $2 AND action = 'verify'"#,
        CHAT_ID,
        USER_ID,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.actor_kind, "moderator");
    assert_eq!(row.actor_user_id, Some(555));

    // Repeat — idempotent.
    let again = svc.verify_manual(CHAT_ID, USER_ID, 555).await.unwrap();
    assert_eq!(again, Outcome::AlreadyVerified);
}

// ── helpers ───────────────────────────────────────────────────────────────

fn wrong_solution(real: &str) -> String {
    real.chars()
        .map(|c| {
            let d = c.to_digit(10).unwrap_or(0);
            std::char::from_digit((d + 1) % 10, 10).unwrap()
        })
        .collect()
}
