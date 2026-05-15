import { useParams } from "@solidjs/router";
import { SettingsForm } from "@/features/settings/components/settings-form";

export default function ChatSettingsPage() {
  const params = useParams<{ chatId: string }>();
  const chatId = Number.parseInt(params.chatId, 10);
  return <SettingsForm chatId={chatId} />;
}
