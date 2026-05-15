import { useParams } from "@solidjs/router";
import { BannedTable } from "@/features/moderation/components/banned-table";

export default function ChatBannedPage() {
  const params = useParams<{ chatId: string }>();
  const chatId = Number.parseInt(params.chatId, 10);
  return <BannedTable chatId={chatId} />;
}
