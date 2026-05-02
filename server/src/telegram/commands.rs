//! Slash commands. `/help` and `/status` are stub replies; `/verify`, `/ban`
//! and `/unban` are moderator-gated and routed through their respective
//! services.

use teloxide::utils::command::BotCommands;

#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase", description = "Vixen bot commands")]
pub enum Command {
    #[command(description = "show help")]
    Help,
    #[command(description = "show bot status in this chat")]
    Status,
    /// Reply-mode: `/verify` (replied to the target user).
    /// Id-mode: `/verify <user_id>`.
    #[command(description = "manually verify a user (moderator)")]
    Verify(String),
    /// Reply-mode: `/ban <optional reason>` (replied to the target message).
    /// Id-mode: `/ban <user_id> <optional reason>`.
    #[command(description = "ban a user (reply or with user_id)")]
    Ban(String),
    /// Id-mode only: `/unban <user_id>`.
    #[command(description = "lift a ban by user_id (moderator)")]
    Unban(String),
    /// Inline last-24h activity summary (moderator-only, 60s cooldown).
    #[command(description = "last 24h activity summary (moderator)")]
    Stats,
    /// On-demand full daily report (moderator-only). Re-runs delete the
    /// prior day's report messages and post a fresh pair.
    #[command(description = "post the daily report now (moderator)")]
    Report,
    /// AI-generated summary of recent chat activity (moderator-only).
    /// Requires `chat_config.openai_api_key` set for this chat.
    #[command(description = "AI summary of recent chat (moderator)")]
    Summary,
}
