import { moderation } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";
import { Select } from "@/shared/ui/select";

export interface AuditFilters {
  action: string;
  actorKind: string;
  userId: string;
}

interface AuditFiltersProps {
  value: AuditFilters;
  onChange: (next: AuditFilters) => void;
  onReset: () => void;
}

const ACTION_OPTIONS = [
  { value: "", label: "—" },
  { value: "ban", label: "ban" },
  { value: "unban", label: "unban" },
  { value: "verify", label: "verify" },
  { value: "unverify", label: "unverify" },
  { value: "delete", label: "delete" },
  { value: "captcha_expired", label: "captcha_expired" },
  { value: "captcha_failed", label: "captcha_failed" },
  { value: "kick", label: "kick" },
];

const ACTOR_OPTIONS = [
  { value: "", label: "—" },
  { value: "bot", label: "bot" },
  { value: "moderator", label: "moderator" },
];

export function AuditFiltersForm(props: AuditFiltersProps) {
  return (
    <div class="flex flex-wrap items-end gap-3">
      <div class="flex flex-col gap-1">
        <Label>{t(moderation["filters.action"])}</Label>
        <Select
          value={props.value.action}
          onChange={(v) => props.onChange({ ...props.value, action: v })}
          options={ACTION_OPTIONS}
        />
      </div>
      <div class="flex flex-col gap-1">
        <Label>{t(moderation["filters.actorKind"])}</Label>
        <Select
          value={props.value.actorKind}
          onChange={(v) => props.onChange({ ...props.value, actorKind: v })}
          options={ACTOR_OPTIONS}
        />
      </div>
      <div class="flex flex-col gap-1">
        <Label>{t(moderation["filters.userId"])}</Label>
        <Input
          type="number"
          value={props.value.userId}
          onInput={(e) => props.onChange({ ...props.value, userId: e.currentTarget.value })}
          class="w-40"
        />
      </div>
      <Button variant="ghost" onClick={props.onReset}>
        {t(moderation["filters.reset"])}
      </Button>
    </div>
  );
}
