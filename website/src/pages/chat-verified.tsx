import { useParams } from "@solidjs/router";
import { VerifiedTable } from "@/features/moderation/components/verified-table";

export default function ChatVerifiedPage() {
  const params = useParams<{ chatId: string }>();
  const chatId = Number.parseInt(params.chatId, 10);
  return <VerifiedTable chatId={chatId} />;
}
