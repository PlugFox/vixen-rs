import { createSignal, Show } from "solid-js";
import { ApiError } from "@/shared/api";
import { common, errors, moderation } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import type { MessageDef } from "@/shared/i18n/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";
import { showToast } from "@/shared/ui/toast";
import type { ModerationActionResponse } from "../types";

export interface ActionDialogProps {
  trigger: import("solid-js").JSX.Element;
  title: MessageDef;
  description?: MessageDef;
  submitLabel: MessageDef;
  withReason: boolean;
  toastApplied: MessageDef;
  toastAlready: MessageDef;
  variant?: "primary" | "destructive";
  submit: (userId: number, reason?: string) => Promise<ModerationActionResponse>;
  onSuccess?: () => void;
}

/**
 * Generic confirmation dialog used by Ban / Unban / Verify. Each consumer
 * picks the i18n keys + the API call; the dialog handles form state, error
 * surfacing, toast, and `onSuccess` (used to refetch parent lists).
 */
export function ActionDialog(props: ActionDialogProps) {
  const [open, setOpen] = createSignal(false);
  const [userId, setUserId] = createSignal("");
  const [reason, setReason] = createSignal("");
  const [submitting, setSubmitting] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  function reset() {
    setUserId("");
    setReason("");
    setError(null);
  }

  async function submit() {
    const parsed = Number.parseInt(userId(), 10);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      setError(t(errors.BAD_USER_ID));
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      const resp = await props.submit(parsed, props.withReason ? reason() || undefined : undefined);
      const key = resp.outcome === "applied" ? props.toastApplied : props.toastAlready;
      showToast({
        variant: resp.outcome === "applied" ? "success" : "default",
        title: t(key),
      });
      props.onSuccess?.();
      setOpen(false);
      reset();
    } catch (e) {
      const code = e instanceof ApiError ? e.code : "UNKNOWN";
      setError(t(errors[code as keyof typeof errors] ?? errors.UNKNOWN));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Dialog
      open={open()}
      onOpenChange={(v) => {
        setOpen(v);
        if (!v) reset();
      }}
    >
      <DialogTrigger as="span">{props.trigger}</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t(props.title)}</DialogTitle>
          <Show when={props.description}>
            {(d) => <DialogDescription>{t(d())}</DialogDescription>}
          </Show>
        </DialogHeader>

        <div class="flex flex-col gap-3">
          <div class="flex flex-col gap-1">
            <Label for="action-dialog-user-id">{t(moderation["dialogs.ban.user-id"])}</Label>
            <Input
              id="action-dialog-user-id"
              type="number"
              min="1"
              value={userId()}
              onInput={(e) => setUserId(e.currentTarget.value)}
              data-invalid={error() ? "" : undefined}
              autocomplete="off"
              required
            />
          </div>
          <Show when={props.withReason}>
            <div class="flex flex-col gap-1">
              <Label for="action-dialog-reason">{t(moderation["dialogs.ban.reason"])}</Label>
              <Input
                id="action-dialog-reason"
                value={reason()}
                onInput={(e) => setReason(e.currentTarget.value)}
                maxlength="500"
                autocomplete="off"
              />
            </div>
          </Show>
          <Show when={error()}>{(e) => <p class="text-sm text-destructive">{e()}</p>}</Show>
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={() => setOpen(false)}>
            {t(common.cancel)}
          </Button>
          <Button
            variant={props.variant ?? "primary"}
            disabled={submitting()}
            onClick={() => void submit()}
          >
            {submitting() ? t(common.loading) : t(props.submitLabel)}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
