/**
 * Hand-mirrored types from `server/src/models/chat_config.rs` —
 * `ChatConfigDto` on GET, `ChatConfigPatch` on PATCH. Per CLAUDE.md we keep
 * these in lockstep manually; the surface is small (18 fields).
 */

export interface ChatConfigDto {
  chat_id: number;
  captcha_enabled: boolean;
  captcha_lifetime_secs: number;
  captcha_attempts: number;
  spam_enabled: boolean;
  spam_threshold: number;
  spam_weights: Record<string, unknown>;
  cas_enabled: boolean;
  clown_chance: number;
  log_allowed_messages: boolean;
  report_hour: number;
  timezone: string;
  summary_enabled: boolean;
  summary_token_budget: number;
  report_min_activity: number;
  openai_api_key_set: boolean;
  openai_model: string;
  language: string;
  created_at: string;
  updated_at: string;
}

/**
 * PATCH semantics:
 *  - omitted field = unchanged
 *  - explicit value = update
 *  - `openai_api_key: null` = clear (only nullable field)
 *
 * deny_unknown_fields on the server rejects unknown keys with 400.
 */
export type ChatConfigPatch = Partial<{
  captcha_enabled: boolean;
  captcha_lifetime_secs: number;
  captcha_attempts: number;
  spam_enabled: boolean;
  spam_threshold: number;
  spam_weights: Record<string, unknown>;
  cas_enabled: boolean;
  clown_chance: number;
  log_allowed_messages: boolean;
  report_hour: number;
  timezone: string;
  summary_enabled: boolean;
  summary_token_budget: number;
  report_min_activity: number;
  openai_api_key: string | null;
  openai_model: string;
  language: string;
}>;
