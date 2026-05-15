import { A } from "@solidjs/router";
import { createResource, For, Show } from "solid-js";
import { chats, common } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { Badge } from "@/shared/ui/badge";
import { Card, CardContent } from "@/shared/ui/card";
import { Skeleton } from "@/shared/ui/skeleton";
import { chatsApi } from "../api";

interface ChatHeaderProps {
  chatId: number;
  title: string | null;
  activeTab: "settings" | "audit" | "verified" | "banned";
}

const TABS = [
  { key: "settings", label: chats["header.tabs.settings"] },
  { key: "audit", label: chats["header.tabs.audit"] },
  { key: "verified", label: chats["header.tabs.verified"] },
  { key: "banned", label: chats["header.tabs.banned"] },
] as const;

export function ChatHeader(props: ChatHeaderProps) {
  const [stats] = createResource(
    () => props.chatId,
    (id) => chatsApi.getStats(id),
  );

  return (
    <header class="flex flex-col gap-4">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <h1 class="text-2xl font-semibold tracking-tight">
          {props.title ?? `chat ${props.chatId}`}
        </h1>
        <A href="/" class="text-sm text-muted-foreground hover:text-foreground hover:underline">
          ← {t(common.back)}
        </A>
      </div>

      <Card>
        <CardContent class="grid grid-cols-2 gap-3 p-4 sm:grid-cols-5">
          <Show
            when={stats()}
            fallback={<For each={[0, 1, 2, 3, 4]}>{() => <Skeleton class="h-12 w-full" />}</For>}
          >
            {(s) => (
              <>
                <StatTile label={t(chats["header.members"])} value={s().members_count ?? "—"} />
                <StatTile label={t(chats["header.verified"])} value={s().verified_count} />
                <StatTile label={t(chats["header.banned"])} value={s().banned_count} />
                <StatTile
                  label={t(chats["header.captchaSolved24h"])}
                  value={s().captcha_solved_24h}
                />
                <StatTile
                  label={t(chats["header.captchaFailed24h"])}
                  value={s().captcha_failed_24h}
                />
              </>
            )}
          </Show>
        </CardContent>
      </Card>

      <nav class="flex flex-wrap items-center gap-2 border-b" aria-label="Chat sections">
        <For each={TABS}>
          {(tab) => (
            <A
              href={`/chats/${props.chatId}/${tab.key}`}
              class="-mb-px border-b-2 border-transparent px-3 py-2 text-sm font-medium transition-colors hover:text-foreground data-[active]:border-primary data-[active]:text-foreground"
              activeClass="border-primary text-foreground"
              inactiveClass="text-muted-foreground"
              end={false}
            >
              {t(tab.label)}
              <Show when={props.activeTab === tab.key}>
                <Badge variant="primary" class="ml-2 hidden sm:inline-flex">
                  •
                </Badge>
              </Show>
            </A>
          )}
        </For>
      </nav>
    </header>
  );
}

function StatTile(props: { label: string; value: number | string }) {
  return (
    <div class="flex flex-col items-start rounded-md bg-muted/40 p-3">
      <span class="text-xs uppercase tracking-wide text-muted-foreground">{props.label}</span>
      <span class="mt-1 text-xl font-semibold tabular-nums">{props.value}</span>
    </div>
  );
}
