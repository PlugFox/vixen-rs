//! Integration tests for `models::report_message`. The daily-report job's
//! replace-on-redo invariant lives here: a re-run on the same `report_date`
//! must read the prior pair (`prior_today`), let the bot delete those
//! messages, then drop the rows (`delete_for_day`) and re-insert via
//! `record`. These tests pin each leg of that contract.

#![cfg(unix)]

mod common;
use common::*;

use chrono::NaiveDate;
use sqlx::PgPool;
use vixen_server::models::report_message::{self, ReportKind};

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn record_inserts_both_kinds(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let date = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();

    report_message::record(&pool, chat_id, date, ReportKind::Text, 100)
        .await
        .unwrap();
    report_message::record(&pool, chat_id, date, ReportKind::Photo, 101)
        .await
        .unwrap();

    let prior = report_message::prior_today(&pool, chat_id, date)
        .await
        .unwrap();
    assert_eq!(prior.len(), 2);
    assert!(
        prior
            .iter()
            .any(|m| m.kind == ReportKind::Text && m.telegram_message_id == 100)
    );
    assert!(
        prior
            .iter()
            .any(|m| m.kind == ReportKind::Photo && m.telegram_message_id == 101)
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn record_upserts_on_same_kind(pool: PgPool) {
    // The replace-on-redo flow re-INSERTs after `delete_for_day`, but the
    // ON CONFLICT branch is also exercised when the dashboard's "redo"
    // button skips the delete step. We accept either path.
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let date = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();

    report_message::record(&pool, chat_id, date, ReportKind::Text, 100)
        .await
        .unwrap();
    report_message::record(&pool, chat_id, date, ReportKind::Text, 200)
        .await
        .unwrap();

    let prior = report_message::prior_today(&pool, chat_id, date)
        .await
        .unwrap();
    assert_eq!(prior.len(), 1);
    assert_eq!(prior[0].kind, ReportKind::Text);
    assert_eq!(prior[0].telegram_message_id, 200);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn prior_today_is_empty_when_no_rows(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let date = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();

    let prior = report_message::prior_today(&pool, chat_id, date)
        .await
        .unwrap();
    assert!(prior.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn delete_for_day_drops_only_target_date(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let day1 = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
    let day2 = NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();

    report_message::record(&pool, chat_id, day1, ReportKind::Text, 1)
        .await
        .unwrap();
    report_message::record(&pool, chat_id, day1, ReportKind::Photo, 2)
        .await
        .unwrap();
    report_message::record(&pool, chat_id, day2, ReportKind::Text, 3)
        .await
        .unwrap();

    report_message::delete_for_day(&pool, chat_id, day1)
        .await
        .unwrap();

    assert!(
        report_message::prior_today(&pool, chat_id, day1)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        report_message::prior_today(&pool, chat_id, day2)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn already_posted_today_requires_both_kinds(pool: PgPool) {
    // Half-posted days (e.g. text succeeded, chart `send_photo` failed) must
    // NOT short-circuit the next scheduler tick — the predicate flips to true
    // only after BOTH text + photo rows exist.
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let date = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();

    assert!(
        !report_message::already_posted_today(&pool, chat_id, date)
            .await
            .unwrap(),
        "no rows → not posted"
    );

    report_message::record(&pool, chat_id, date, ReportKind::Text, 99)
        .await
        .unwrap();
    assert!(
        !report_message::already_posted_today(&pool, chat_id, date)
            .await
            .unwrap(),
        "text only → still not posted (retry photo on next tick)"
    );

    report_message::record(&pool, chat_id, date, ReportKind::Photo, 100)
        .await
        .unwrap();
    assert!(
        report_message::already_posted_today(&pool, chat_id, date)
            .await
            .unwrap(),
        "both rows → posted"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn distinct_chats_dont_alias(pool: PgPool) {
    let chat_a = unique_chat_id();
    let chat_b = unique_chat_id();
    seed_chat(&pool, chat_a).await;
    seed_chat(&pool, chat_b).await;
    let date = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();

    report_message::record(&pool, chat_a, date, ReportKind::Text, 1)
        .await
        .unwrap();

    assert_eq!(
        report_message::prior_today(&pool, chat_a, date)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        report_message::prior_today(&pool, chat_b, date)
            .await
            .unwrap()
            .is_empty()
    );
}
