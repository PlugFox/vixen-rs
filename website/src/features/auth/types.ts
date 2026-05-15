/**
 * Wire shapes for `/api/v1/auth/*`. Mirrors `server/src/api/routes_auth.rs`
 * by hand — per CLAUDE.md we don't pull in an OpenAPI codegen for the small
 * type surface.
 *
 * Telegram IDs are `i64` server-side. In TS they're plain `number` — safe
 * up to 2^53. CLAUDE.md explicitly forbids `bigint` for IDs in v1.
 */

export interface User {
  id: number;
  username: string | null;
  first_name: string;
  last_name: string | null;
}

export interface LoginResponse {
  token: string;
  expires_in: number;
  user: User;
  chat_ids: number[];
}

export interface MeResponse {
  user_id: number;
  username: string | null;
  first_name: string;
  last_name: string | null;
  chat_ids: number[];
}
