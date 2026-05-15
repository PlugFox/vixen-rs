import { createSignal, For, Show } from "solid-js";
import { ApiError } from "@/shared/api";
import { common, errors, moderation } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { Badge } from "@/shared/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/shared/ui/table";
import { showToast } from "@/shared/ui/toast";
import { moderationApi } from "../api";
import type { ModerationActionItem } from "../types";
import { type AuditFilters, AuditFiltersForm } from "./audit-filters";
import { LoadMoreButton } from "./load-more-button";

interface AuditTableProps {
  chatId: number;
}

const emptyFilters: AuditFilters = { action: "", actorKind: "", userId: "" };

function variantForAction(action: string): "destructive" | "success" | "warning" | "neutral" {
  if (action === "ban" || action === "delete" || action === "kick") return "destructive";
  if (action === "verify" || action === "unban") return "success";
  if (action === "captcha_failed" || action === "captcha_expired") return "warning";
  return "neutral";
}

export function AuditTable(props: AuditTableProps) {
  const [items, setItems] = createSignal<ModerationActionItem[]>([]);
  const [cursor, setCursor] = createSignal<string | null>(null);
  const [hasMore, setHasMore] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [filters, setFilters] = createSignal<AuditFilters>(emptyFilters);

  async function fetchPage(reset: boolean) {
    setLoading(true);
    const f = filters();
    try {
      const page = await moderationApi.listActions(props.chatId, {
        cursor: reset ? undefined : (cursor() ?? undefined),
        action: f.action || undefined,
        actor_kind: f.actorKind || undefined,
        target_user_id: f.userId ? Number.parseInt(f.userId, 10) : undefined,
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

  // Initial load + reload on filter change.
  let lastFiltersKey = "";
  const reactToFilters = () => {
    const f = filters();
    const key = `${f.action}|${f.actorKind}|${f.userId}`;
    if (key !== lastFiltersKey) {
      lastFiltersKey = key;
      void fetchPage(true);
    }
  };
  reactToFilters();

  return (
    <div class="flex flex-col gap-3">
      <AuditFiltersForm
        value={filters()}
        onChange={(v) => {
          setFilters(v);
          reactToFilters();
        }}
        onReset={() => {
          setFilters(emptyFilters);
          reactToFilters();
        }}
      />

      <Show
        when={items().length > 0 || !loading()}
        fallback={<p class="text-sm text-muted-foreground">{t(common.loading)}</p>}
      >
        <Show
          when={items().length > 0}
          fallback={
            <p class="py-6 text-center text-sm text-muted-foreground">
              {t(moderation["actions.empty"])}
            </p>
          }
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t(moderation["table.columns.when"])}</TableHead>
                <TableHead>{t(moderation["table.columns.action"])}</TableHead>
                <TableHead>{t(moderation["table.columns.actor"])}</TableHead>
                <TableHead>{t(moderation["table.columns.target"])}</TableHead>
                <TableHead>{t(moderation["table.columns.reason"])}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <For each={items()}>
                {(row) => (
                  <TableRow>
                    <TableCell class="text-xs tabular-nums">
                      {new Date(row.created_at).toLocaleString()}
                    </TableCell>
                    <TableCell>
                      <Badge variant={variantForAction(row.action)}>{row.action}</Badge>
                    </TableCell>
                    <TableCell>
                      {row.actor_kind}
                      {row.actor_user_id !== null ? ` · ${row.actor_user_id}` : ""}
                    </TableCell>
                    <TableCell class="tabular-nums">{row.target_user_id}</TableCell>
                    <TableCell class="max-w-xs truncate" title={row.reason ?? ""}>
                      {row.reason ?? "—"}
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
