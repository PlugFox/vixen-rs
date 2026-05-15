import { createMemo, type JSX, Show } from "solid-js";
import { auth, common } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { Skeleton } from "@/shared/ui/skeleton";
import { currentChatIds, isAuthenticated, isLoadingSession } from "../store";
import { LoginPrompt } from "./login-prompt";

interface ProtectedProps {
  chatId?: number;
  children: JSX.Element;
}

export function Protected(props: ProtectedProps) {
  const chatAllowed = createMemo(() => {
    if (!isAuthenticated()) return false;
    if (typeof props.chatId === "number") return currentChatIds().includes(props.chatId);
    return true;
  });

  return (
    <Show
      when={!isLoadingSession()}
      fallback={
        <div class="flex flex-col items-center justify-center gap-3 py-10">
          <Skeleton class="h-6 w-40" />
          <p class="text-sm text-muted-foreground">{t(common.loading)}</p>
        </div>
      }
    >
      <Show when={isAuthenticated()} fallback={<LoginPrompt />}>
        <Show
          when={chatAllowed()}
          fallback={
            <p class="mx-auto max-w-md text-center text-sm text-muted-foreground">
              {t(auth.notAuthorized)}
            </p>
          }
        >
          {props.children}
        </Show>
      </Show>
    </Show>
  );
}
