export interface ModerationActionItem {
  id: string; // UUID
  chat_id: number;
  target_user_id: number;
  action: string;
  actor_kind: string;
  actor_user_id: number | null;
  message_id: number | null;
  reason: string | null;
  created_at: string;
}

export interface Paginated<T> {
  items: T[];
  has_more: boolean;
  cursor: string | null;
}

export interface VerifiedUserItem {
  user_id: number;
  verified_at: string;
}

export interface BannedUserItem {
  user_id: number;
  banned_at: string;
  reason: string | null;
}

export interface ActionsQuery {
  cursor?: string;
  limit?: number;
  action?: string;
  actor_kind?: string;
  target_user_id?: number;
}

export interface ModerationActionResponse {
  id: string | null;
  outcome: "applied" | "already_applied";
}
