export interface Chat {
  chat_id: number;
  slug: string | null;
  title: string | null;
  kind: string | null;
  members_count: number | null;
}

export interface ChatsListResponse {
  chats: Chat[];
}

export interface ChatStats {
  chat_id: number;
  members_count: number | null;
  verified_count: number;
  banned_count: number;
  captcha_solved_24h: number;
  captcha_failed_24h: number;
}
