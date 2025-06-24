use core::fmt;
use once_cell::sync::OnceCell;
use reqwest::Client;
use serde::Deserialize;
use serde_json;
use std::{collections::HashSet, ops::Add, time::Duration};
use tracing::{debug, error, info};

use crate::{captcha::CaptchaService, database::DB};

#[derive(Debug, Deserialize)]
pub struct User {
    // Unique identifier for this user or bot
    pub id: i64,
    // True, if this user is a bot
    pub is_bot: bool,
    // User's or bot's first name
    pub first_name: String,
    // Optional. User's or bot's last name
    pub last_name: Option<String>,
    // Optional. User's or bot's username
    pub username: Option<String>,
    // Optional. IETF language tag of the user's language
    pub language_code: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChatType {
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
pub struct Chat {
    // Unique identifier for this chat
    pub id: i64,

    // Type of the chat, can be either “private”, “group”, “supergroup” or “channel”
    #[serde(rename = "type")]
    pub chat_type: ChatType,

    // Optional. Title, for supergroups, channels and group chats
    pub title: Option<String>,

    // Optional. Username, for private chats, supergroups and channels if available
    pub username: Option<String>,

    // Optional. First name of the other party in a private chat
    pub first_name: Option<String>,

    // Optional. Last name of the other party in a private chat
    pub last_name: Option<String>,

    // Optional. True, if the supergroup chat is a forum (has topics enabled)
    pub is_forum: Option<bool>, // Optional. True, if the supergroup chat is a forum (has topics enabled)
}

#[derive(Debug, Clone)]
pub struct MessageMetadata {
    pub message_type: &'static str,
    pub content: String,
    pub length: i32,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    // Unique message identifier inside this chat
    pub message_id: i64,

    // Date the message was sent in Unix time
    // It is always a positive number, representing a valid date
    pub date: u64,

    // Chat the message belongs to
    pub chat: Chat,

    // Optional. Sender of the message; may be empty for messages sent to channels
    pub from: Option<User>,

    // Optional. For replies in the same chat and message thread, the original message.
    pub reply_to_message: Option<Box<Message>>,

    // Optional. For text messages, the actual UTF-8 text of the message
    pub text: Option<String>,

    // Optional. For text messages, special entities like usernames, URLs, bot commands, etc. that appear in the text
    #[serde(default)]
    pub entities: Option<Vec<serde_json::Value>>,

    // Optional. Message is a forwarded story
    #[serde(default)]
    pub story: Option<serde_json::Value>,

    // 	Optional. Message is a video, information about the video
    #[serde(default)]
    pub video: Option<serde_json::Value>,

    // Optional. Message is an animation, information about the animation
    // For backward compatibility, when this field is set, the document field will also be set
    #[serde(default)]
    pub animation: Option<serde_json::Value>,

    // Optional. Message is an audio file, information about the file
    #[serde(default)]
    pub audio: Option<serde_json::Value>,

    // Optional. Message is a general file, information about the file
    #[serde(default)]
    pub document: Option<serde_json::Value>,

    // Optional. Message is a photo, available sizes of the photo
    #[serde(default)]
    pub photo: Option<serde_json::Value>,

    // Optional. Message is a sticker, information about the sticker
    #[serde(default)]
    pub sticker: Option<serde_json::Value>,

    // Optional. Message is a contact, information about the contact
    #[serde(default)]
    pub video_note: Option<serde_json::Value>,

    // Optional. Message is a voice message, information about the voice message
    #[serde(default)]
    pub voice: Option<serde_json::Value>,

    // Optional. Message is a game, information about the game.
    #[serde(default)]
    pub game: Option<serde_json::Value>,

    // Optional. Caption for the animation, audio, document, paid media, photo, video or voice
    pub caption: Option<String>,

    // Optional. For messages with a caption,
    // special entities like usernames, URLs, bot commands, etc. that appear in the caption
    pub caption_entities: Option<Vec<serde_json::Value>>,

    // Lazy-computed metadata cache
    #[serde(skip)]
    metadata: OnceCell<MessageMetadata>,
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

#[derive(Clone)]
pub struct Bot {
    db: DB,
    client: Client,
    token: String,
    chats: HashSet<i64>,
}

/// A Telegram Bot API client
impl Bot {
    pub fn new(token: String, chats: Vec<String>, db: DB) -> Self {
        let chats_set: HashSet<i64> = chats
            .iter()
            .filter_map(|chat| chat.parse::<i64>().ok())
            .collect();
        Bot {
            db,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .connect_timeout(Duration::from_secs(10))
                .pool_idle_timeout(Duration::from_secs(60))
                .pool_max_idle_per_host(10)
                .build()
                .expect("failed to build HTTP client"),
            token: token.to_string(),
            chats: chats_set,
        }
    }

    /// Escape special characters in a MarkdownV2 string
    pub fn escape_markdown_v2(text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }

        const SPECIAL_CHARS: &str = r"_*\[]()~`>#+\-=|{}.!";
        let mut buffer = String::with_capacity(text.len() * 2); // Pre-allocate for better performance

        for ch in text.chars() {
            if SPECIAL_CHARS.contains(ch) {
                buffer.push('\\');
            }
            buffer.push(ch);
        }

        buffer
    }

    /// Format a user mention as a MarkdownV2 link
    pub fn user_mention(uid: i64, username: &str) -> String {
        format!(
            "[{}](tg://user?id={})",
            Self::escape_markdown_v2(username),
            uid
        )
    }

    /// Get the short ID of a user or chat
    /// Converts Telegram's full chat ID to a shorter representation
    pub fn short_id(cid: i64) -> i64 {
        cid.abs() - 1000000000000
    }

    /// Format a chat mention with its title
    pub fn chat_mention(chat: &Chat) -> String {
        match &chat.title {
            Some(title) => Self::escape_markdown_v2(title),
            None => match &chat.username {
                Some(username) => format!("@{}", Self::escape_markdown_v2(username)),
                None => format!("Chat {}", Self::short_id(chat.id)),
            },
        }
    }

    /// Format a user's display name safely for MarkdownV2
    pub fn user_display_name(user: &User) -> String {
        let mut name = Self::escape_markdown_v2(&user.first_name);

        if let Some(ref last_name) = user.last_name {
            if !last_name.is_empty() {
                name.push(' ');
                name.push_str(&Self::escape_markdown_v2(last_name));
            }
        }

        name
    }

    /// Create a formatted message with user mention
    pub fn format_message_with_user(user: &User, message: &str) -> String {
        let user_mention = Self::user_mention(user.id, &Self::user_display_name(user));
        format!("{}, {}", user_mention, Self::escape_markdown_v2(message))
    }

    /// Send a formatted message with error handling and retries
    pub async fn send_formatted_message(
        &self,
        chat_id: i64,
        text: &str,
        disable_notification: bool,
        protect_content: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.send_message_with_options(
            chat_id,
            text,
            Some("MarkdownV2"),
            disable_notification,
            protect_content,
            None,
        )
        .await
    }

    /// Send a message with full options support
    pub async fn send_message_with_options(
        &self,
        chat_id: i64,
        text: &str,
        parse_mode: Option<&str>,
        disable_notification: bool,
        protect_content: bool,
        reply_to_message_id: Option<i64>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.token);

        let mut payload = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "disable_notification": disable_notification,
            "protect_content": protect_content,
        });

        if let Some(mode) = parse_mode {
            payload["parse_mode"] = serde_json::Value::String(mode.to_string());
        }

        if let Some(reply_id) = reply_to_message_id {
            payload["reply_to_message_id"] = serde_json::Value::Number(reply_id.into());
        }

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&payload)
            .timeout(Duration::from_secs(12))
            .send()
            .await?;

        // Check HTTP status first
        if !resp.status().is_success() {
            return Err(format!(
                "failed to send message to chat {}: HTTP {}",
                chat_id,
                resp.status()
            )
            .into());
        }

        // Parse JSON response
        let json = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("failed to parse response: {}", e))?;

        // Handle Telegram API response
        match (
            json.get("ok").and_then(|v| v.as_bool()),
            json.get("result"),
            json.get("description").and_then(|v| v.as_str()),
        ) {
            (Some(true), Some(_result), _) => {
                debug!("message sent to chat {}: {}", chat_id, text);
                Ok(())
            }
            (Some(false), _, Some(description)) => {
                error!("telegram API error for chat {}: {}", chat_id, description);
                Err(format!(
                    "failed to send message to chat {}: {}",
                    chat_id, description
                )
                .into())
            }
            (Some(false), _, None) => {
                error!(
                    "telegram API error for chat {} without description",
                    chat_id
                );
                Err(format!("failed to send message to chat {}: unknown error", chat_id).into())
            }
            _ => {
                error!(
                    "malformed telegram API response for chat {}: {:?}",
                    chat_id, json
                );
                Err(format!(
                    "failed to send message to chat {}: malformed response",
                    chat_id
                )
                .into())
            }
        }
    }

    /// Send a reply to a specific message
    pub async fn send_reply_message(
        &self,
        chat_id: i64,
        reply_to_message_id: i64,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.send_message_with_options(
            chat_id,
            text,
            Some("MarkdownV2"),
            true, // disable notification for replies
            true, // protect content
            Some(reply_to_message_id),
        )
        .await
    }

    /// Delete a message from a chat
    pub async fn delete_message(
        &self,
        chat_id: i64,
        message_id: i64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://api.telegram.org/bot{}/deleteMessage", self.token);

        let payload = serde_json::json!({
            "chat_id": chat_id,
            "message_id": message_id,
        });

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&payload)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!(
                "failed to delete message {} in chat {}: HTTP {}",
                message_id,
                chat_id,
                resp.status()
            )
            .into());
        }

        let json = resp.json::<serde_json::Value>().await?;

        match json.get("ok").and_then(|v| v.as_bool()) {
            Some(true) => {
                debug!("message {} deleted from chat {}", message_id, chat_id);
                Ok(())
            }
            Some(false) => {
                let description = json
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                Err(format!("failed to delete message: {}", description).into())
            }
            _ => Err("malformed response".into()),
        }
    }

    /// Ban a user from the chat
    pub async fn ban_user(
        &self,
        chat_id: i64,
        user_id: i64,
        until_date: Option<i64>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://api.telegram.org/bot{}/banChatMember", self.token);

        let mut payload = serde_json::json!({
            "chat_id": chat_id,
            "user_id": user_id,
        });

        if let Some(date) = until_date {
            payload["until_date"] = serde_json::Value::Number(date.into());
        }

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&payload)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        Ok(())
    }

    /// Long-poll Telegram updates, stop on shutdown signal
    pub async fn poll<F>(&self, shutdown_signal: F)
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
        let token: &str = &self.token;
        let chats: HashSet<i64> = self.chats.clone();
        let has_chats = !chats.is_empty();
        let pool: &DB = &self.db;

        tokio::pin!(shutdown_signal);

        loop {
            tokio::select! {
                _ = &mut shutdown_signal => {
                    info!("telegram polling stopped");
                    break;
                }
                result = Self::get_updates(
                    &client, // HTTP client for Telegram API
                    token, // Telegram Bot API token
                    offset, // offset for updates
                    timeout.as_secs(), // timeout in seconds
                    100, // limit of updates to fetch
                    vec!["message", "callback_query"]
                ) => {
                    // Safely handle the result with panic protection
                    let panic_result = std::panic::AssertUnwindSafe(async {
                        match result {
                            Ok(updates) => {
                                for upd in updates {
                                    offset = upd.update_id + 1;
                                    info!("received update: {}", upd);

                                    // Process each update from Telegram Bot API with panic protection
                                    if let Some(message) = upd.message {
                                        if message.chat.chat_type == ChatType::Private {
                                            debug!("ignoring private message from chat#{}", message.chat.id);
                                            continue; // Skip private messages
                                        } else if has_chats && !chats.contains(&message.chat.id) {
                                            debug!("ignoring message from chat#{}", message.chat.id);
                                            continue; // Skip messages from non-configured chats
                                        } else if message.from.is_none() {
                                            debug!("ignoring message without sender from chat#{}", message.chat.id);
                                            continue; // Skip messages without text or photo
                                        } /* else if message.text.is_none() && message.caption.is_none() {
                                            debug!("ignoring message without text or caption from chat#{}", message.chat.id);
                                            continue; // Skip messages without text or photo
                                        } */

                                        info!("processing message from chat#{}", message.chat.id);

                                        // Process message in a separate task to isolate potential panics
                                        let process_result = tokio::spawn(Self::safe_process_message(
                                            self.clone(),
                                            message,
                                            pool.clone()
                                        )).await;

                                        if let Err(join_err) = process_result {
                                            if join_err.is_panic() {
                                                error!(
                                                    "panic occurred while processing message: {:?}",
                                                    join_err
                                                );
                                            } else {
                                                error!(
                                                    "task was cancelled while processing message: {:?}",
                                                    join_err
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                error!(%err, "error polling Telegram ");
                                // A bit of backoff to avoid hammering the API
                                tokio::time::sleep(Duration::from_secs(5)).await;
                            }
                        }
                    });

                    // Execute with panic protection
                    match std::panic::catch_unwind(move || {
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(panic_result)
                        })
                    }) {
                        Ok(_) => {
                            // Successfully processed updates
                        }
                        Err(panic_info) => {
                            let panic_msg = if let Some(s) = panic_info.downcast_ref::<String>() {
                                s.clone()
                            } else if let Some(s) = panic_info.downcast_ref::<&str>() {
                                s.to_string()
                            } else {
                                "unknown panic".to_string()
                            };

                            error!("panic caught in polling loop: {}", panic_msg);
                            // Add a longer backoff after panic to prevent rapid panic loops
                            tokio::time::sleep(Duration::from_secs(10)).await;
                        }
                    }
                }
            }
        }
    }

    /// Fetch updates from Telegram Bot API
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

    /// Send a simple text message to a chat (legacy wrapper)
    pub async fn send_text_message(
        &self,
        chat_id: i64,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.send_formatted_message(chat_id, text, true, true).await
    }

    /// Send a photo to a chat
    pub async fn send_photo(
        &self,
        chat_id: i64,
        photo_bytes: Vec<u8>,
        filename: &str,
        caption: Option<&str>,
        disable_notification: bool,
        reply_markup: Option<&str>,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://api.telegram.org/bot{}/sendPhoto", self.token);

        let form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .text("protect_content", "true")
            .text(
                "disable_notification",
                if disable_notification {
                    "true"
                } else {
                    "false"
                },
            )
            .part(
                "photo",
                reqwest::multipart::Part::bytes(photo_bytes)
                    .file_name(filename.to_string())
                    .mime_str("image/jpeg")?,
            );

        let form = if let Some(caption) = caption {
            form.text("parse_mode", "MarkdownV2")
                .text("caption", caption.to_string())
                .text("show_caption_above_media", "true")
        } else {
            form
        };

        let form = if let Some(reply) = reply_markup {
            form.text("reply_markup", reply.to_string())
        } else {
            form
        };

        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .timeout(Duration::from_secs(12))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!(
                "failed to send photo to chat {}: HTTP {}",
                chat_id,
                resp.status()
            )
            .into());
        }

        let json = resp.json::<serde_json::Value>().await?;

        match (
            json.get("ok").and_then(|v| v.as_bool()),
            json.get("result")
                .and_then(|r| r.get("message_id"))
                .and_then(|id| id.as_i64()),
            json.get("description").and_then(|v| v.as_str()),
        ) {
            (Some(true), Some(message_id), _) => {
                debug!("photo sent to chat {}: message_id {}", chat_id, message_id);
                Ok(message_id)
            }
            (Some(false), _, Some(description)) => {
                error!(
                    "telegram API error for photo in chat {}: {}",
                    chat_id, description
                );
                Err(format!("failed to send photo to chat {}: {}", chat_id, description).into())
            }
            (Some(false), _, None) => {
                error!(
                    "telegram API error for photo in chat {} without description",
                    chat_id
                );
                Err(format!("failed to send photo to chat {}: unknown error", chat_id).into())
            }
            _ => {
                error!(
                    "malformed telegram API response for photo in chat {}: {:?}",
                    chat_id, json
                );
                Err(format!(
                    "failed to send photo to chat {}: malformed response",
                    chat_id
                )
                .into())
            }
        }
    }

    /// Edit a photo caption in a chat
    pub async fn edit_photo_caption(
        &self,
        chat_id: i64,
        message_id: i64,
        caption: Option<&str>,
        reply_markup: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "https://api.telegram.org/bot{}/editMessageCaption",
            self.token
        );

        let mut payload = serde_json::json!({
            "chat_id": chat_id,
            "message_id": message_id,
        });

        if let Some(caption) = caption {
            payload["parse_mode"] = serde_json::Value::String("MarkdownV2".to_string());
            payload["caption"] = serde_json::Value::String(caption.to_string());
            payload["show_caption_above_media"] = serde_json::Value::Bool(true);
        }

        if let Some(reply) = reply_markup {
            payload["reply_markup"] = serde_json::Value::String(reply.to_string());
        }

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&payload)
            .timeout(Duration::from_secs(12))
            .send()
            .await?;

        // 200 OK or 400 Bad Request are both acceptable responses
        if resp.status().is_success() || resp.status() == reqwest::StatusCode::BAD_REQUEST {
            debug!(
                "photo caption edited in chat {}, message {}",
                chat_id, message_id
            );
            return Ok(());
        }

        error!(
            "failed to edit photo caption in chat {}, message {}: HTTP {}",
            chat_id,
            message_id,
            resp.status()
        );
        Err(format!(
            "failed to edit photo caption: status code {}",
            resp.status()
        )
        .into())
    }

    /// Edit message media in a chat
    pub async fn edit_message_media(
        &self,
        chat_id: i64,
        message_id: i64,
        photo_bytes: Vec<u8>,
        filename: &str,
        caption: Option<&str>,
        reply_markup: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "https://api.telegram.org/bot{}/editMessageMedia",
            self.token
        );

        let mut media = serde_json::json!({
            "type": "photo",
            "media": "attach://media",
        });

        if let Some(caption) = caption {
            media["parse_mode"] = serde_json::Value::String("MarkdownV2".to_string());
            media["caption"] = serde_json::Value::String(caption.to_string());
            media["show_caption_above_media"] = serde_json::Value::Bool(true);
        }

        let form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .text("message_id", message_id.to_string())
            .text("media", serde_json::to_string(&media)?)
            .part(
                "media",
                reqwest::multipart::Part::bytes(photo_bytes)
                    .file_name(filename.to_string())
                    .mime_str("image/jpeg")?,
            );

        let form = if let Some(reply) = reply_markup {
            form.text("reply_markup", reply.to_string())
        } else {
            form
        };

        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .timeout(Duration::from_secs(12))
            .send()
            .await?;

        // 200 OK or 400 Bad Request are both acceptable responses
        if resp.status().is_success() || resp.status() == reqwest::StatusCode::BAD_REQUEST {
            debug!(
                "message media edited in chat {}, message {}",
                chat_id, message_id
            );
            return Ok(());
        }

        error!(
            "failed to edit message media in chat {}, message {}: HTTP {}",
            chat_id,
            message_id,
            resp.status()
        );
        Err(format!(
            "failed to edit message media: status code {}",
            resp.status()
        )
        .into())
    }

    /// Process a single message from Telegram
    async fn process_message(&self, message: Message, db: &DB) {
        info!(
            "processing message {}: {:?}",
            message.message_id, message.text
        );

        // Upsert message and user to database
        match db.upsert_message(&message).await {
            Ok(_) => {
                debug!(
                    "successfully upserted message {} to database",
                    message.message_id
                );
            }
            Err(e) => {
                error!(
                    "failed to upsert message {} to database: {}",
                    message.message_id, e
                );
            }
        }

        // Check if user verified their identity
        let user = message.from.as_ref();
        if user.is_none() {
            debug!(
                "message from chat#{} without user, skipping",
                message.chat.id
            );
            return;
        }

        let user = user.unwrap();
        let user_id = user.id;
        let chat_id = message.chat.id;
        let message_id = message.message_id;
        debug!("checking user {} in database", user.id);
        match db.is_user_verified(user_id).await {
            true => {
                return; // User is verified, no further action needed
            }
            false => {
                debug!("user {} is not verified", user.id);
                // Here you would send a verification request to the user
                // For example, send a message with instructions to verify their identity
            }
        }

        let message_type = message.metadata().message_type;
        let verification_message = Self::format_message_with_user(
            user,
            "Please verify your identity to use this bot. You can do this by sending a verification code or following the instructions provided.",
        );

        self.delete_message(chat_id, message_id).await.ok();

        static VERY_BAD_TYPES: once_cell::sync::Lazy<HashSet<&'static str>> =
            once_cell::sync::Lazy::new(|| {
                let mut set = HashSet::new();
                set.insert("video");
                set.insert("audio");
                set.insert("voice");
                set.insert("video_note");
                set.insert("story");
                set
            });

        // Check if the message type is one of the very bad types
        if VERY_BAD_TYPES.contains(message_type) {
            // If the message is a media type that requires verification, ban the user
            db.ban_user(user_id, chat_id, "Send bad type of message", None)
                .await
                .unwrap_or_else(|e| {
                    error!("failed to ban user {}: {}", user.id, e);
                });

            self.ban_user(chat_id, user_id, None)
                .await
                .unwrap_or_else(|e| {
                    error!("failed to ban user {}: {}", user.id, e);
                });

            info!(
                "banned user {} for sending unsupported message type: {}",
                user.id, message_type
            );

            return;
        }

        {
            // Generate a captcha for the user and send it
            let service = CaptchaService::new();
            let captcha = service.generate().await;

            let verification_message = format!(
                "👋 Hello *{}* \\[`{}`\\] \\!\n\nPlease solve the _following captcha_ to start chatting\\.",
                Self::user_mention(user_id, &Self::user_display_name(user)),
                user_id,
            );

            let captcha_message_id = self
                .send_photo(
                    chat_id,
                    captcha.bytes,
                    "captcha.webp",
                    Some(&verification_message),
                    false,                             // disable notification
                    Some(&*KB_CAPTCHA_MARKUP_ENCODED), // captcha keyboard markup
                )
                .await
                .expect("failed to send verification photo");

            db.upsert_captcha(
                chat_id,
                user_id,
                captcha_message_id,
                &verification_message,
                &captcha.text,
                "",
                chrono::Utc::now()
                    .add(chrono::Duration::minutes(5))
                    .timestamp(),
            )
            .await
            .unwrap_or_else(|e| {
                error!("failed to upsert captcha for user {}: {}", user.id, e);
            });
        }

        // Send verification message to the user
        /* self.send_text_message(chat_id, &verification_message)
        .await
        .unwrap_or_else(|e| {
            error!(
                "failed to send verification message to user {}: {}",
                user.id, e
            );
        }); */

        // If the user is not verified, you might want to send them a message
    }

    /// Safely process a single message from Telegram with panic protection
    async fn safe_process_message(
        bot: Bot,
        message: Message,
        db: DB,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Use std::panic::catch_unwind with AssertUnwindSafe to catch any panics
        // that might occur during message processing
        let panic_result = std::panic::AssertUnwindSafe(async {
            bot.process_message(message, &db).await;
        });

        match std::panic::catch_unwind(move || {
            tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(panic_result))
        }) {
            Ok(_) => {
                debug!("message processed successfully");
                Ok(())
            }
            Err(panic_info) => {
                let panic_msg = panic_info
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic_info.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown panic");

                error!("panic caught while processing message: {}", panic_msg);
                Err(format!("message processing panic: {}", panic_msg).into())
            }
        }
    }
}

impl Message {
    /// Get message metadata (type, content, length) - computed lazily and cached
    pub fn metadata(&self) -> &MessageMetadata {
        self.metadata.get_or_init(|| {
            use MessageMetadata as Meta;

            match (
                &self.text,
                &self.photo,
                &self.video,
                &self.audio,
                &self.document,
                &self.sticker,
                &self.animation,
                &self.voice,
                &self.video_note,
                &self.game,
                &self.story,
            ) {
                // Text message
                (Some(text), None, None, None, None, None, None, None, None, None, None) => Meta {
                    message_type: "text",
                    content: text.clone(),
                    length: text.len() as i32,
                },
                // Media with possible caption
                (_, Some(_), _, _, _, _, _, _, _, _, _) => {
                    let caption = self.caption.clone().unwrap_or_default();
                    Meta {
                        message_type: "photo",
                        content: caption.clone(),
                        length: caption.len() as i32,
                    }
                }
                (_, _, Some(_), _, _, _, _, _, _, _, _) => {
                    let caption = self.caption.clone().unwrap_or_default();
                    Meta {
                        message_type: "video",
                        content: caption.clone(),
                        length: caption.len() as i32,
                    }
                }
                (_, _, _, Some(_), _, _, _, _, _, _, _) => {
                    let caption = self.caption.clone().unwrap_or_default();
                    Meta {
                        message_type: "audio",
                        content: caption.clone(),
                        length: caption.len() as i32,
                    }
                }
                (_, _, _, _, Some(_), _, _, _, _, _, _) => {
                    let caption = self.caption.clone().unwrap_or_default();
                    Meta {
                        message_type: "document",
                        content: caption.clone(),
                        length: caption.len() as i32,
                    }
                }
                (_, _, _, _, _, Some(_), _, _, _, _, _) => Meta {
                    message_type: "sticker",
                    content: String::new(),
                    length: 0,
                },
                (_, _, _, _, _, _, Some(_), _, _, _, _) => {
                    let caption = self.caption.clone().unwrap_or_default();
                    Meta {
                        message_type: "animation",
                        content: caption.clone(),
                        length: caption.len() as i32,
                    }
                }
                (_, _, _, _, _, _, _, Some(_), _, _, _) => {
                    let caption = self.caption.clone().unwrap_or_default();
                    Meta {
                        message_type: "voice",
                        content: caption.clone(),
                        length: caption.len() as i32,
                    }
                }
                (_, _, _, _, _, _, _, _, Some(_), _, _) => Meta {
                    message_type: "video_note",
                    content: String::new(),
                    length: 0,
                },
                (_, _, _, _, _, _, _, _, _, Some(_), _) => Meta {
                    message_type: "game",
                    content: String::new(),
                    length: 0,
                },
                (_, _, _, _, _, _, _, _, _, _, Some(_)) => Meta {
                    message_type: "story",
                    content: String::new(),
                    length: 0,
                },
                // Default case
                _ => Meta {
                    message_type: "other",
                    content: String::new(),
                    length: 0,
                },
            }
        })
    }

    /// Get the text content of the message (text or caption)
    pub fn text_content(&self) -> Option<&str> {
        self.text.as_deref().or(self.caption.as_deref())
    }

    /// Check if message contains media
    pub fn has_media(&self) -> bool {
        self.photo.is_some()
            || self.video.is_some()
            || self.audio.is_some()
            || self.document.is_some()
            || self.sticker.is_some()
            || self.animation.is_some()
            || self.voice.is_some()
            || self.video_note.is_some()
    }

    /// Check if message is a reply to another message
    pub fn is_reply(&self) -> bool {
        self.reply_to_message.is_some()
    }

    /// Get the username or display name of the sender
    pub fn sender_display_name(&self) -> Option<String> {
        self.from.as_ref().map(|user| match &user.username {
            Some(username) => format!("@{}", username),
            None => Bot::user_display_name(user),
        })
    }

    /// Check if message is from a bot
    pub fn is_from_bot(&self) -> bool {
        matches!(self.from.as_ref(), Some(user) if user.is_bot)
    }

    /// Get the age of the message in seconds
    pub fn age_seconds(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        now.saturating_sub(self.date)
    }

    /// Check if message is older than specified duration
    pub fn is_older_than(&self, duration: Duration) -> bool {
        self.age_seconds() > duration.as_secs()
    }
}

const KB_CAPTCHA_ONE: &str = r#"keyboard.captcha.one"#;

const KB_CAPTCHA_TWO: &str = r#"keyboard.captcha.two"#;

const KB_CAPTCHA_THREE: &str = r#"keyboard.captcha.three"#;

const KB_CAPTCHA_FOUR: &str = r#"keyboard.captcha.four"#;

const KB_CAPTCHA_FIVE: &str = r#"keyboard.captcha.five"#;

const KB_CAPTCHA_SIX: &str = r#"keyboard.captcha.six"#;

const KB_CAPTCHA_SEVEN: &str = r#"keyboard.captcha.seven"#;

const KB_CAPTCHA_EIGHT: &str = r#"keyboard.captcha.eight"#;

const KB_CAPTCHA_NINE: &str = r#"keyboard.captcha.nine"#;

const KB_CAPTCHA_ZERO: &str = r#"keyboard.captcha.zero"#;

const KB_CAPTCHA_REFRESH: &str = r#"keyboard.captcha.refresh"#;

const KB_CAPTCHA_BACKSPACE: &str = r#"keyboard.captcha.backspace"#;

/// Cached captcha keyboard markup - computed only once on first access
static KB_CAPTCHA_MARKUP_ENCODED: once_cell::sync::Lazy<String> =
    once_cell::sync::Lazy::new(|| {
        let keyboard = serde_json::json!({
            "inline_keyboard": [
                [
                    {"text": "1️⃣", "callback_data": KB_CAPTCHA_ONE},
                    {"text": "2️⃣", "callback_data": KB_CAPTCHA_TWO},
                    {"text": "3️⃣", "callback_data": KB_CAPTCHA_THREE},
                ],
                [
                    {"text": "4️⃣", "callback_data": KB_CAPTCHA_FOUR},
                    {"text": "5️⃣", "callback_data": KB_CAPTCHA_FIVE},
                    {"text": "6️⃣", "callback_data": KB_CAPTCHA_SIX},
                ],
                [
                    {"text": "7️⃣", "callback_data": KB_CAPTCHA_SEVEN},
                    {"text": "8️⃣", "callback_data": KB_CAPTCHA_EIGHT},
                    {"text": "9️⃣", "callback_data": KB_CAPTCHA_NINE},
                ],
                [
                    {"text": "🔄", "callback_data": KB_CAPTCHA_REFRESH},
                    {"text": "0️⃣", "callback_data": KB_CAPTCHA_ZERO},
                    {"text": "↩️", "callback_data": KB_CAPTCHA_BACKSPACE},
                ],
            ]
        });

        serde_json::to_string(&keyboard).unwrap_or_default()
    });
