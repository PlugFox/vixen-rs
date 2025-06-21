use core::fmt;
use reqwest::Client;
use serde::Deserialize;
use serde_json;
use sqlx::SqlitePool;
use std::{collections::HashSet, ops::Add, time::Duration};
use tracing::{debug, error, info, warn};

use crate::config;

#[derive(Debug, Deserialize)]
struct User {
    // Unique identifier for this user or bot
    id: i64,
    // True, if this user is a bot
    is_bot: bool,
    // User's or bot's first name
    first_name: String,
    // Optional. User's or bot's last name
    last_name: Option<String>,
    // Optional. User's or bot's username
    username: Option<String>,
    // Optional. IETF language tag of the user's language
    language_code: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ChatType {
    #[serde(rename = "private")]
    Private,
    #[serde(rename = "group")]
    Group,
    #[serde(rename = "supergroup")]
    Supergroup,
    #[serde(rename = "channel")]
    Channel,
}

#[derive(Debug, Deserialize)]
struct Chat {
    // Unique identifier for this chat
    id: i64,

    // Type of the chat, can be either “private”, “group”, “supergroup” or “channel”
    #[serde(rename = "type")]
    chat_type: ChatType,

    // Optional. Title, for supergroups, channels and group chats
    title: Option<String>,

    // Optional. Username, for private chats, supergroups and channels if available
    username: Option<String>,

    // Optional. First name of the other party in a private chat
    first_name: Option<String>,

    // Optional. Last name of the other party in a private chat
    last_name: Option<String>,

    // Optional. True, if the supergroup chat is a forum (has topics enabled)
    is_forum: Option<bool>, // Optional. True, if the supergroup chat is a forum (has topics enabled)
}

#[derive(Debug, Deserialize)]
struct Message {
    // Unique message identifier inside this chat
    message_id: i64,

    // Date the message was sent in Unix time
    // It is always a positive number, representing a valid date
    date: u64,

    // Chat the message belongs to
    chat: Chat,

    // Optional. Sender of the message; may be empty for messages sent to channels
    from: Option<User>,

    // Optional. For text messages, the actual UTF-8 text of the message
    text: Option<String>,

    // Optional. For text messages, special entities like usernames, URLs, bot commands, etc. that appear in the text
    #[serde(default)]
    entities: Option<Vec<serde_json::Value>>,

    // Optional. Message is a forwarded story
    #[serde(default)]
    story: Option<serde_json::Value>,

    // 	Optional. Message is a video, information about the video
    #[serde(default)]
    video: Option<serde_json::Value>,

    // Optional. Message is an animation, information about the animation
    // For backward compatibility, when this field is set, the document field will also be set
    #[serde(default)]
    animation: Option<serde_json::Value>,

    // Optional. Message is an audio file, information about the file
    #[serde(default)]
    audio: Option<serde_json::Value>,

    // Optional. Message is a general file, information about the file
    #[serde(default)]
    document: Option<serde_json::Value>,

    // Optional. Message is a photo, available sizes of the photo
    #[serde(default)]
    photo: Option<serde_json::Value>,

    // Optional. Message is a sticker, information about the sticker
    #[serde(default)]
    sticker: Option<serde_json::Value>,

    // Optional. Message is a contact, information about the contact
    #[serde(default)]
    video_note: Option<serde_json::Value>,

    // Optional. Message is a voice message, information about the voice message
    #[serde(default)]
    voice: Option<serde_json::Value>,

    // Optional. Message is a game, information about the game.
    #[serde(default)]
    game: Option<serde_json::Value>,

    // Optional. Caption for the animation, audio, document, paid media, photo, video or voice
    caption: Option<String>,

    // Optional. For messages with a caption,
    // special entities like usernames, URLs, bot commands, etc. that appear in the caption
    caption_entities: Option<Vec<serde_json::Value>>,
}

struct CallbackQuery {
    id: String,
    from: User,
    message: Option<Message>,
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Update {
    update_id: i64,
    message: Option<Message>,
}

impl fmt::Display for Update {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref message) = self.message {
            write!(
                f,
                "Update#{}: Message from chat#{}",
                self.update_id, message.chat.id
            )
        } else {
            write!(f, "Update#{}: Other", self.update_id)
        }
    }
}

#[derive(Debug, Deserialize)]
struct GetUpdates {
    ok: bool,
    result: Vec<Update>,
}

/// Long-poll Telegram updates, stop on shutdown signal
pub async fn poll<F>(conf: &config::Config, pool: SqlitePool, shutdown_signal: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let timeout = Duration::from_secs(30); // Default timeout for HTTP requests
    let client = Client::builder()
        .timeout(timeout.add(Duration::from_secs(5))) // A bit longer timeout for Telegram API requests
        .build()
        .expect("failed to build HTTP client");

    let mut offset: i64 = 0;
    info!("starting Telegram polling");
    let token: &str = &conf.telegram;
    let chats: HashSet<i64> = conf
        .chats
        .iter()
        .map(|chat| chat.parse::<i64>().unwrap_or(0))
        .filter(|&id| id != 0) // Filter out invalid chat IDs
        .collect(); // Collect chat IDs from configuration
    let has_chats = !chats.is_empty();

    tokio::pin!(shutdown_signal);

    loop {
        tokio::select! {
            _ = &mut shutdown_signal => {
                info!("telegram polling stopped");
                break;
            }
            result = get_updates(
                &client, // HTTP client for Telegram API
                token, // Telegram Bot API token
                offset, // offset for updates
                timeout.as_secs(), // timeout in seconds
                100, // limit of updates to fetch
                vec!["message", "callback_query"]
            ) => {
                match result {
                    Ok(updates) => {
                        for upd in updates {
                            offset = upd.update_id + 1;
                            info!("received update: {}", upd);
                            // Process each update from Telegram Bot API
                            if let Some(message) = upd.message {
                                if message.chat.chat_type == ChatType::Private {
                                    debug!("ignoring private message from chat#{}", message.chat.id);
                                    continue; // Skip private messages
                                } else if has_chats && !chats.contains(&message.chat.id) {
                                    debug!("ignoring message from chat#{}", message.chat.id);
                                    continue; // Skip messages from non-configured chats
                                }

                                info!("processing message from chat#{}", message.chat.id);
                                process_message(message, &pool).await;
                            }
                        }
                    }
                    Err(err) => {
                        error!(%err, "error polling Telegram ");
                        // A bit of backoff to avoid hammering the API
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
    timeout: u64,
    limit: u8,
    allowed_updates: Vec<&str>,
) -> Result<Vec<Update>, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!(
        concat!(
            "https://api.telegram.org",
            "/bot{token}",
            "/getUpdates",
            "?offset={offset}",
            "&timeout={timeout}",
            "&limit={limit}",
            "&allowed_updates={filter}"
        ),
        token = token,
        offset = offset,
        timeout = timeout,
        limit = limit,
        filter = allowed_updates.join(",")
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
