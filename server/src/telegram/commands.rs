//! Slash commands. M1 only ships `/verify` (manual moderator override). `/help`
//! and `/status` are stubs — they reply with a fixed string so users don't see
//! "unknown command" until M2/M5 fleshes them out.

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
}
