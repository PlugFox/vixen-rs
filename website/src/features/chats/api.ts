import { api } from "@/shared/api";
import type { Chat, ChatStats, ChatsListResponse } from "./types";

export const chatsApi = {
  list: () => api.get<ChatsListResponse>("/chats"),
  getStats: (chatId: number) => api.get<ChatStats>(`/chats/${chatId}/stats`),
};

export type { Chat, ChatStats };
