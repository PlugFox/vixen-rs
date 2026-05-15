import { api } from "@/shared/api";
import type { ChatConfigDto, ChatConfigPatch } from "./types";

export const settingsApi = {
  get: (chatId: number) => api.get<ChatConfigDto>(`/chats/${chatId}/config`),
  patch: (chatId: number, patch: ChatConfigPatch) =>
    api.patch<ChatConfigDto>(`/chats/${chatId}/config`, patch),
};
