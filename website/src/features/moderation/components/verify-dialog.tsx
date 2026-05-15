import { moderation } from "@/shared/i18n/generated";
import { moderationApi } from "../api";
import { ActionDialog } from "./action-dialog";

export function VerifyDialog(props: {
  chatId: number;
  trigger: import("solid-js").JSX.Element;
  onSuccess?: () => void;
}) {
  return (
    <ActionDialog
      trigger={props.trigger}
      title={moderation["dialogs.verify.title"]}
      description={moderation["dialogs.verify.description"]}
      submitLabel={moderation["dialogs.verify.submit"]}
      withReason={false}
      toastApplied={moderation["toast.verify-applied"]}
      toastAlready={moderation["toast.verify-already"]}
      submit={(userId) => moderationApi.verify(props.chatId, userId)}
      onSuccess={props.onSuccess}
    />
  );
}
