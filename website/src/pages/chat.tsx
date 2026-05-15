import { useLocation, useParams } from "@solidjs/router";
import { type JSX, createMemo, createResource, Show } from "solid-js";
import { Protected } from "@/features/auth/components/protected";
import { chatsApi } from "@/features/chats/api";
import { ChatHeader } from "@/features/chats/components/chat-header";

/**
 * Wraps the nested chat-tab pages. `@solidjs/router` 0.16 renders nested
 * routes via `props.children` rather than an explicit <Outlet> — so this
 * component takes children and slots them under the header.
 */
export default function ChatPage(props: { children?: JSX.Element }) {
  const params = useParams<{ chatId: string }>();
  const location = useLocation();

  const chatId = createMemo(() => {
    const n = Number.parseInt(params.chatId, 10);
    return Number.isFinite(n) ? n : null;
  });

  const activeTab = createMemo<"settings" | "audit" | "verified" | "banned">(() => {
    const segment = location.pathname.split("/").pop();
    if (segment === "audit" || segment === "verified" || segment === "banned") return segment;
    return "settings";
  });

  const [chats] = createResource(chatId, async (id) => {
    if (id === null) return null;
    const list = await chatsApi.list();
    return list.chats.find((c) => c.chat_id === id) ?? null;
  });

  return (
    <Show
      when={chatId() !== null}
      fallback={<p class="text-sm text-muted-foreground">Invalid chat id</p>}
    >
      <Protected chatId={chatId() ?? undefined}>
        <div class="flex flex-col gap-4">
          <ChatHeader
            chatId={chatId() ?? 0}
            title={chats()?.title ?? null}
            activeTab={activeTab()}
          />
          {props.children}
        </div>
      </Protected>
    </Show>
  );
}
