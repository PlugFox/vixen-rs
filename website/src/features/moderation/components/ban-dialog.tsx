import { moderation } from "@/shared/i18n/generated";
import { moderationApi } from "../api";
import { ActionDialog } from "./action-dialog";

export function BanDialog(props: {
  chatId: number;
  trigger: import("solid-js").JSX.Element;
  onSuccess?: () => void;
}) {
  return (
    <ActionDialog
      trigger={props.trigger}
      title={moderation["dialogs.ban.title"]}
      description={moderation["dialogs.ban.description"]}
      submitLabel={moderation["dialogs.ban.submit"]}
      withReason={true}
      variant="destructive"
      toastApplied={moderation["toast.ban-applied"]}
      toastAlready={moderation["toast.ban-already"]}
      submit={(userId, reason) => moderationApi.ban(props.chatId, userId, reason)}
      onSuccess={props.onSuccess}
    />
  );
}
