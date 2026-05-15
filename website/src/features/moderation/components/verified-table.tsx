import { createSignal, For, onMount, Show } from "solid-js";
import { ApiError } from "@/shared/api";
import { common, errors, moderation } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/shared/ui/table";
import { showToast } from "@/shared/ui/toast";
import { moderationApi } from "../api";
import type { VerifiedUserItem } from "../types";
import { LoadMoreButton } from "./load-more-button";

interface VerifiedTableProps {
  chatId: number;
}

export function VerifiedTable(props: VerifiedTableProps) {
  const [items, setItems] = createSignal<VerifiedUserItem[]>([]);
  const [cursor, setCursor] = createSignal<string | null>(null);
  const [hasMore, setHasMore] = createSignal(false);
  const [loading, setLoading] = createSignal(false);

  async function fetchPage(reset: boolean) {
    setLoading(true);
    try {
      const page = await moderationApi.listVerified(props.chatId, {
        cursor: reset ? undefined : (cursor() ?? undefined),
      });
      setItems(reset ? page.items : [...items(), ...page.items]);
      setCursor(page.cursor);
      setHasMore(page.has_more);
    } catch (e) {
      const code = e instanceof ApiError ? e.code : "UNKNOWN";
      showToast({
        variant: "destructive",
        title: t(errors[code as keyof typeof errors] ?? errors.UNKNOWN),
      });
    } finally {
      setLoading(false);
    }
  }

  onMount(() => void fetchPage(true));

  return (
    <div class="flex flex-col gap-3">
      <Show
        when={items().length > 0 || !loading()}
        fallback={<p class="text-sm text-muted-foreground">{t(common.loading)}</p>}
      >
        <Show
          when={items().length > 0}
          fallback={
            <p class="py-6 text-center text-sm text-muted-foreground">
              {t(moderation["verified.empty"])}
            </p>
          }
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t(moderation["table.columns.target"])}</TableHead>
                <TableHead>{t(moderation["table.columns.when"])}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <For each={items()}>
                {(row) => (
                  <TableRow>
                    <TableCell class="tabular-nums">{row.user_id}</TableCell>
                    <TableCell class="text-xs tabular-nums">
                      {new Date(row.verified_at).toLocaleString()}
                    </TableCell>
                  </TableRow>
                )}
              </For>
            </TableBody>
          </Table>
          <LoadMoreButton
            hasMore={hasMore()}
            loading={loading()}
            onClick={() => void fetchPage(false)}
          />
        </Show>
      </Show>
    </div>
  );
}
