import { createSignal, Show } from "solid-js";
import { common, settings } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";

interface OpenAiKeyFieldProps {
  isSet: boolean;
  onUpdate: (key: string) => void;
  onClear: () => void;
  disabled?: boolean;
}

export function OpenAiKeyField(props: OpenAiKeyFieldProps) {
  const [editing, setEditing] = createSignal(false);
  const [draft, setDraft] = createSignal("");

  function submit() {
    const v = draft().trim();
    if (v.length > 0) {
      props.onUpdate(v);
      setDraft("");
      setEditing(false);
    }
  }

  function clear() {
    if (window.confirm(t(settings["field.openai_api_key.clear-confirm"]))) {
      props.onClear();
    }
  }

  return (
    <div class="flex flex-col gap-2">
      <Label>{t(settings["field.openai_api_key.label"])}</Label>
      <Show
        when={!editing()}
        fallback={
          <div class="flex flex-wrap items-center gap-2">
            <Input
              type="password"
              value={draft()}
              onInput={(e) => setDraft(e.currentTarget.value)}
              placeholder="sk-…"
              disabled={props.disabled}
              autocomplete="off"
            />
            <Button onClick={submit} disabled={props.disabled || draft().trim().length === 0}>
              {t(common.save)}
            </Button>
            <Button
              variant="ghost"
              onClick={() => {
                setEditing(false);
                setDraft("");
              }}
            >
              {t(common.cancel)}
            </Button>
          </div>
        }
      >
        <div class="flex flex-wrap items-center gap-2 text-sm">
          <span class="text-muted-foreground">
            {props.isSet
              ? t(settings["field.openai_api_key.set"])
              : t(settings["field.openai_api_key.unset"])}
          </span>
          <Button
            size="sm"
            variant="outline"
            onClick={() => setEditing(true)}
            disabled={props.disabled}
          >
            {t(settings["field.openai_api_key.update"])}
          </Button>
          <Show when={props.isSet}>
            <Button size="sm" variant="destructive" onClick={clear} disabled={props.disabled}>
              {t(settings["field.openai_api_key.clear"])}
            </Button>
          </Show>
        </div>
      </Show>
    </div>
  );
}
