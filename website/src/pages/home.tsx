import { createResource, For, Show } from "solid-js";
import { Protected } from "@/features/auth/components/protected";
import { chatsApi } from "@/features/chats/api";
import { ChatCard } from "@/features/chats/components/chat-card";
import { chats, common } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { Skeleton } from "@/shared/ui/skeleton";

export default function HomePage() {
  return (
    <Protected>
      <ChatsListInner />
    </Protected>
  );
}

function ChatsListInner() {
  const [resource] = createResource(() => chatsApi.list());

  return (
    <section class="flex flex-col gap-4">
      <h1 class="text-2xl font-semibold tracking-tight">{t(chats["list.title"])}</h1>
      <Show
        when={resource()}
        fallback={
          <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <For each={[0, 1, 2]}>{() => <Skeleton class="h-32 w-full" />}</For>
          </div>
        }
      >
        {(data) => (
          <Show
            when={data().chats.length > 0}
            fallback={
              <p class="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
                {t(chats["list.empty"])}
              </p>
            }
          >
            <p class="text-sm text-muted-foreground">{t(chats["list.openHint"])}</p>
            <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              <For each={data().chats}>{(chat) => <ChatCard chat={chat} />}</For>
            </div>
          </Show>
        )}
      </Show>
      <p class="sr-only">{t(common.loading)}</p>
    </section>
  );
}
