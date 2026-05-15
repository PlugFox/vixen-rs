import { useParams } from "@solidjs/router";
import { AuditTable } from "@/features/moderation/components/audit-table";
import { BanDialog } from "@/features/moderation/components/ban-dialog";
import { UnbanDialog } from "@/features/moderation/components/unban-dialog";
import { VerifyDialog } from "@/features/moderation/components/verify-dialog";
import { moderation } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { Button } from "@/shared/ui/button";

export default function ChatAuditPage() {
  const params = useParams<{ chatId: string }>();
  const chatId = Number.parseInt(params.chatId, 10);

  return (
    <section class="flex flex-col gap-3">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <h2 class="text-xl font-semibold">{t(moderation["actions.title"])}</h2>
        <div class="flex flex-wrap items-center gap-2">
          <BanDialog
            chatId={chatId}
            trigger={<Button variant="destructive">{t(moderation["dialogs.ban.title"])}</Button>}
          />
          <UnbanDialog
            chatId={chatId}
            trigger={<Button variant="outline">{t(moderation["dialogs.unban.title"])}</Button>}
          />
          <VerifyDialog
            chatId={chatId}
            trigger={<Button>{t(moderation["dialogs.verify.title"])}</Button>}
          />
        </div>
      </div>
      <AuditTable chatId={chatId} />
    </section>
  );
}
