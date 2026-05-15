import { api } from "@/shared/api";
import type {
  ActionsQuery,
  BannedUserItem,
  ModerationActionItem,
  ModerationActionResponse,
  Paginated,
  VerifiedUserItem,
} from "./types";

function qs(params: Record<string, unknown>): string {
  const u = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === "") continue;
    u.append(k, String(v));
  }
  const s = u.toString();
  return s ? `?${s}` : "";
}

export const moderationApi = {
  listActions: (chatId: number, q: ActionsQuery = {}) =>
    api.get<Paginated<ModerationActionItem>>(
      `/chats/${chatId}/moderation/actions${qs(q as Record<string, unknown>)}`,
    ),
  listVerified: (chatId: number, q: { cursor?: string; limit?: number } = {}) =>
    api.get<Paginated<VerifiedUserItem>>(
      `/chats/${chatId}/moderation/verified${qs(q as Record<string, unknown>)}`,
    ),
  listBanned: (chatId: number, q: { cursor?: string; limit?: number } = {}) =>
    api.get<Paginated<BannedUserItem>>(
      `/chats/${chatId}/moderation/banned${qs(q as Record<string, unknown>)}`,
    ),
  ban: (chatId: number, user_id: number, reason?: string) =>
    api.post<ModerationActionResponse>(`/chats/${chatId}/moderation/ban`, { user_id, reason }),
  unban: (chatId: number, user_id: number) =>
    api.post<ModerationActionResponse>(`/chats/${chatId}/moderation/unban`, { user_id }),
  verify: (chatId: number, user_id: number) =>
    api.post<ModerationActionResponse>(`/chats/${chatId}/moderation/verify`, { user_id }),
};
