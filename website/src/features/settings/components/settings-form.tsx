import { createResource, createSignal, For, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import { ApiError } from "@/shared/api";
import { common, errors, settings } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import type { MessageDef } from "@/shared/i18n/types";
import { Button } from "@/shared/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/shared/ui/card";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";
import { Select } from "@/shared/ui/select";
import { Skeleton } from "@/shared/ui/skeleton";
import { Switch } from "@/shared/ui/switch";
import { showToast } from "@/shared/ui/toast";
import { settingsApi } from "../api";
import type { ChatConfigDto, ChatConfigPatch } from "../types";
import { OpenAiKeyField } from "./openai-key-field";

interface SettingsFormProps {
  chatId: number;
}

interface FieldRowProps {
  label: MessageDef;
  hint?: MessageDef;
  children: import("solid-js").JSX.Element;
}

function FieldRow(props: FieldRowProps) {
  return (
    <div class="flex flex-col gap-1">
      <Label>{t(props.label)}</Label>
      {props.children}
      {props.hint ? <p class="text-xs text-muted-foreground">{t(props.hint)}</p> : null}
    </div>
  );
}

const LANGUAGE_OPTIONS = [
  { value: "en", label: "English" },
  { value: "ru", label: "Русский" },
  { value: "auto", label: "Auto" },
] as const;

export function SettingsForm(props: SettingsFormProps) {
  const [resource, { refetch }] = createResource(() => props.chatId, settingsApi.get);
  const [draft, setDraft] = createStore<Partial<ChatConfigDto>>({});
  const [dirty, setDirty] = createSignal(false);
  const [saving, setSaving] = createSignal(false);

  function applyServerState(dto: ChatConfigDto) {
    setDraft({ ...dto });
    setDirty(false);
  }

  // First server payload populates the draft.
  resource(); // touch
  onMount(() => {
    const cur = resource();
    if (cur) applyServerState(cur);
    // Refetch on window focus so other-tab edits propagate.
    const refresh = () => refetch();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  });

  // Whenever the resource resolves (initial or after refetch), reset the
  // draft only if the user hasn't started editing. The hidden resource is
  // what gives us reactivity on `resource()` updates.
  createResource(resource, (cur) => {
    if (cur && !dirty()) applyServerState(cur);
    return null;
  });

  function set<K extends keyof ChatConfigDto>(key: K, value: ChatConfigDto[K]) {
    setDraft(key as keyof ChatConfigDto, value);
    setDirty(true);
  }

  function buildPatch(): ChatConfigPatch {
    const cur = resource();
    if (!cur) return {};
    const patch: ChatConfigPatch = {};
    const keys: (keyof ChatConfigDto)[] = [
      "captcha_enabled",
      "captcha_lifetime_secs",
      "captcha_attempts",
      "spam_enabled",
      "spam_threshold",
      "spam_weights",
      "cas_enabled",
      "clown_chance",
      "log_allowed_messages",
      "report_hour",
      "timezone",
      "summary_enabled",
      "summary_token_budget",
      "report_min_activity",
      "openai_model",
      "language",
    ];
    for (const k of keys) {
      const next = draft[k];
      if (next !== undefined && next !== cur[k]) {
        // Type-safe assignment: each key shares the type between DTO and patch.
        (patch as Record<string, unknown>)[k] = next;
      }
    }
    return patch;
  }

  async function save() {
    const patch = buildPatch();
    if (Object.keys(patch).length === 0) {
      setDirty(false);
      return;
    }
    setSaving(true);
    try {
      const updated = await settingsApi.patch(props.chatId, patch);
      applyServerState(updated);
      showToast({
        variant: "success",
        title: t(settings["form.savedToast"]),
      });
    } catch (e) {
      const code = e instanceof ApiError ? e.code : "UNKNOWN";
      showToast({
        variant: "destructive",
        title: t(settings["form.errorToast"]),
        description: t(errors[code as keyof typeof errors] ?? errors.UNKNOWN),
      });
    } finally {
      setSaving(false);
    }
  }

  async function updateOpenAiKey(key: string) {
    setSaving(true);
    try {
      const updated = await settingsApi.patch(props.chatId, { openai_api_key: key });
      applyServerState(updated);
      showToast({ variant: "success", title: t(settings["form.savedToast"]) });
    } catch (e) {
      const code = e instanceof ApiError ? e.code : "UNKNOWN";
      showToast({
        variant: "destructive",
        title: t(settings["form.errorToast"]),
        description: t(errors[code as keyof typeof errors] ?? errors.UNKNOWN),
      });
    } finally {
      setSaving(false);
    }
  }

  async function clearOpenAiKey() {
    setSaving(true);
    try {
      const updated = await settingsApi.patch(props.chatId, { openai_api_key: null });
      applyServerState(updated);
      showToast({ variant: "success", title: t(settings["form.savedToast"]) });
    } catch (e) {
      const code = e instanceof ApiError ? e.code : "UNKNOWN";
      showToast({
        variant: "destructive",
        title: t(settings["form.errorToast"]),
        description: t(errors[code as keyof typeof errors] ?? errors.UNKNOWN),
      });
    } finally {
      setSaving(false);
    }
  }

  return (
    <div class="flex flex-col gap-4">
      <Show
        when={resource()}
        fallback={
          <Card>
            <CardContent class="flex flex-col gap-2 p-6">
              <For each={[0, 1, 2, 3]}>{() => <Skeleton class="h-8 w-full" />}</For>
            </CardContent>
          </Card>
        }
      >
        {/* ── Captcha ──────────────────────────────────────── */}
        <Card>
          <CardHeader>
            <CardTitle>{t(settings["section.captcha"])}</CardTitle>
          </CardHeader>
          <CardContent class="flex flex-col gap-4">
            <FieldRow
              label={settings["field.captcha_enabled.label"]}
              hint={settings["field.captcha_enabled.hint"]}
            >
              <Switch
                checked={!!draft.captcha_enabled}
                onChange={(v: boolean) => set("captcha_enabled", v)}
              />
            </FieldRow>
            <FieldRow
              label={settings["field.captcha_lifetime_secs.label"]}
              hint={settings["field.captcha_lifetime_secs.hint"]}
            >
              <Input
                type="number"
                min="1"
                max="3600"
                value={draft.captcha_lifetime_secs ?? 60}
                onInput={(e) =>
                  set("captcha_lifetime_secs", Number.parseInt(e.currentTarget.value, 10))
                }
              />
            </FieldRow>
            <FieldRow
              label={settings["field.captcha_attempts.label"]}
              hint={settings["field.captcha_attempts.hint"]}
            >
              <Input
                type="number"
                min="1"
                max="100"
                value={draft.captcha_attempts ?? 5}
                onInput={(e) => set("captcha_attempts", Number.parseInt(e.currentTarget.value, 10))}
              />
            </FieldRow>
          </CardContent>
        </Card>

        {/* ── Spam ─────────────────────────────────────────── */}
        <Card>
          <CardHeader>
            <CardTitle>{t(settings["section.spam"])}</CardTitle>
          </CardHeader>
          <CardContent class="flex flex-col gap-4">
            <FieldRow
              label={settings["field.spam_enabled.label"]}
              hint={settings["field.spam_enabled.hint"]}
            >
              <Switch checked={!!draft.spam_enabled} onChange={(v: boolean) => set("spam_enabled", v)} />
            </FieldRow>
            <FieldRow
              label={settings["field.spam_threshold.label"]}
              hint={settings["field.spam_threshold.hint"]}
            >
              <Input
                type="number"
                step="0.05"
                min="0"
                value={draft.spam_threshold ?? 1.0}
                onInput={(e) => set("spam_threshold", Number.parseFloat(e.currentTarget.value))}
              />
            </FieldRow>
            <FieldRow
              label={settings["field.cas_enabled.label"]}
              hint={settings["field.cas_enabled.hint"]}
            >
              <Switch checked={!!draft.cas_enabled} onChange={(v: boolean) => set("cas_enabled", v)} />
            </FieldRow>
            <FieldRow
              label={settings["field.spam_weights.label"]}
              hint={settings["field.spam_weights.hint"]}
            >
              <Input
                type="text"
                value={JSON.stringify(draft.spam_weights ?? {})}
                onInput={(e) => {
                  try {
                    const parsed = JSON.parse(e.currentTarget.value);
                    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
                      set("spam_weights", parsed);
                    }
                  } catch {
                    // Keep typing — server validates on Save.
                  }
                }}
              />
            </FieldRow>
          </CardContent>
        </Card>

        {/* ── Report ───────────────────────────────────────── */}
        <Card>
          <CardHeader>
            <CardTitle>{t(settings["section.report"])}</CardTitle>
          </CardHeader>
          <CardContent class="flex flex-col gap-4">
            <FieldRow
              label={settings["field.report_hour.label"]}
              hint={settings["field.report_hour.hint"]}
            >
              <Input
                type="number"
                min="0"
                max="23"
                value={draft.report_hour ?? 17}
                onInput={(e) => set("report_hour", Number.parseInt(e.currentTarget.value, 10))}
              />
            </FieldRow>
            <FieldRow
              label={settings["field.timezone.label"]}
              hint={settings["field.timezone.hint"]}
            >
              <Input
                value={draft.timezone ?? "UTC"}
                onInput={(e) => set("timezone", e.currentTarget.value)}
              />
            </FieldRow>
            <FieldRow
              label={settings["field.report_min_activity.label"]}
              hint={settings["field.report_min_activity.hint"]}
            >
              <Input
                type="number"
                min="0"
                max="100000"
                value={draft.report_min_activity ?? 0}
                onInput={(e) =>
                  set("report_min_activity", Number.parseInt(e.currentTarget.value, 10))
                }
              />
            </FieldRow>
          </CardContent>
        </Card>

        {/* ── Summary ──────────────────────────────────────── */}
        <Card>
          <CardHeader>
            <CardTitle>{t(settings["section.summary"])}</CardTitle>
          </CardHeader>
          <CardContent class="flex flex-col gap-4">
            <FieldRow
              label={settings["field.summary_enabled.label"]}
              hint={settings["field.summary_enabled.hint"]}
            >
              <Switch
                checked={!!draft.summary_enabled}
                onChange={(v: boolean) => set("summary_enabled", v)}
              />
            </FieldRow>
            <FieldRow
              label={settings["field.summary_token_budget.label"]}
              hint={settings["field.summary_token_budget.hint"]}
            >
              <Input
                type="number"
                min="1"
                value={draft.summary_token_budget ?? 50000}
                onInput={(e) =>
                  set("summary_token_budget", Number.parseInt(e.currentTarget.value, 10))
                }
              />
            </FieldRow>
            <FieldRow
              label={settings["field.openai_model.label"]}
              hint={settings["field.openai_model.hint"]}
            >
              <Input
                value={draft.openai_model ?? "gpt-4o-mini"}
                onInput={(e) => set("openai_model", e.currentTarget.value)}
              />
            </FieldRow>
            <FieldRow
              label={settings["field.language.label"]}
              hint={settings["field.language.hint"]}
            >
              <Select
                value={(draft.language ?? "en") as "en" | "ru" | "auto"}
                onChange={(v) => set("language", v)}
                options={[...LANGUAGE_OPTIONS]}
              />
            </FieldRow>
            <OpenAiKeyField
              isSet={!!resource()?.openai_api_key_set || !!draft.openai_api_key_set}
              onUpdate={(k) => void updateOpenAiKey(k)}
              onClear={() => void clearOpenAiKey()}
              disabled={saving()}
            />
          </CardContent>
        </Card>

        {/* ── Misc ─────────────────────────────────────────── */}
        <Card>
          <CardHeader>
            <CardTitle>{t(settings["section.misc"])}</CardTitle>
          </CardHeader>
          <CardContent class="flex flex-col gap-4">
            <FieldRow
              label={settings["field.clown_chance.label"]}
              hint={settings["field.clown_chance.hint"]}
            >
              <Input
                type="number"
                min="0"
                max="100"
                value={draft.clown_chance ?? 0}
                onInput={(e) => set("clown_chance", Number.parseInt(e.currentTarget.value, 10))}
              />
            </FieldRow>
            <FieldRow
              label={settings["field.log_allowed_messages.label"]}
              hint={settings["field.log_allowed_messages.hint"]}
            >
              <Switch
                checked={!!draft.log_allowed_messages}
                onChange={(v: boolean) => set("log_allowed_messages", v)}
              />
            </FieldRow>
          </CardContent>
        </Card>

        <div class="sticky bottom-2 z-10 flex items-center justify-end gap-3 rounded-md border bg-background/95 p-3 shadow-md backdrop-blur">
          <Show when={dirty()}>
            <span class="text-sm text-warning">{t(settings["form.dirtyNotice"])}</span>
          </Show>
          <Button disabled={!dirty() || saving()} onClick={() => void save()}>
            {saving() ? t(common.loading) : t(common.save)}
          </Button>
        </div>
      </Show>
    </div>
  );
}
