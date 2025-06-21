use reqwest::Client;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::time::Duration;
use tracing::{error, info, warn};

#[derive(Debug, Deserialize)]
struct Message {
    message_id: i64,
    text: Option<String>,
    // добавьте другие поля по необходимости
}

#[derive(Debug, Deserialize)]
struct Update {
    update_id: i64,
    message: Option<Message>,
    // добавьте другие типы обновлений (callback_query, etc.)
}

#[derive(Debug, Deserialize)]
struct GetUpdates {
    ok: bool,
    result: Vec<Update>,
}

/// Long-poll Telegram updates, stop on shutdown signal
pub async fn poll<F>(token: String, pool: SqlitePool, shutdown_signal: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let client = Client::builder()
        .timeout(Duration::from_secs(35)) // немного больше чем timeout в API
        .build()
        .expect("failed to build HTTP client");

    let mut offset = 0;
    info!("Starting Telegram polling");

    tokio::pin!(shutdown_signal);

    loop {
        tokio::select! {
            _ = &mut shutdown_signal => {
                info!("Telegram polling stopped");
                break;
            }
            result = get_updates(&client, &token, offset) => {
                match result {
                    Ok(updates) => {
                        for upd in updates {
                            offset = upd.update_id + 1;
                            info!("received update: {:?}", upd);

                            // TODO: обработка обновления и сохранение в БД
                            if let Some(message) = upd.message {
                                process_message(message, &pool).await;
                            }
                        }
                    }
                    Err(err) => {
                        error!(%err, "error polling Telegram");
                        // Небольшая задержка при ошибке, чтобы не спамить API
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        }
    }
}

async fn get_updates(
    client: &Client,
    token: &str,
    offset: i64,
) -> Result<Vec<Update>, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!(
        "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=30&limit=100",
        token, offset
    );

    let resp = client.get(&url).send().await?;
    let body = resp.json::<GetUpdates>().await?;

    if !body.ok {
        return Err("Telegram API returned ok=false".into());
    }

    Ok(body.result)
}

async fn process_message(message: Message, pool: &SqlitePool) {
    // TODO: реализовать обработку сообщения и работу с БД
    info!(
        "processing message {}: {:?}",
        message.message_id, message.text
    );
}
