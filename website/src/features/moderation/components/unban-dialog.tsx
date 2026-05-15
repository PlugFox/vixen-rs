import { moderation } from "@/shared/i18n/generated";
import { moderationApi } from "../api";
import { ActionDialog } from "./action-dialog";

export function UnbanDialog(props: {
  chatId: number;
  trigger: import("solid-js").JSX.Element;
  onSuccess?: () => void;
}) {
  return (
    <ActionDialog
      trigger={props.trigger}
      title={moderation["dialogs.unban.title"]}
      submitLabel={moderation["dialogs.unban.submit"]}
      withReason={false}
      toastApplied={moderation["toast.unban-applied"]}
      toastAlready={moderation["toast.unban-already"]}
      submit={(userId) => moderationApi.unban(props.chatId, userId)}
      onSuccess={props.onSuccess}
    />
  );
}
