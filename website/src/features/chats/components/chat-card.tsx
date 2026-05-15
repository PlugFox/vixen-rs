import { A } from "@solidjs/router";
import { Card, CardContent, CardHeader, CardTitle } from "@/shared/ui/card";
import type { Chat } from "../types";

interface ChatCardProps {
  chat: Chat;
}

export function ChatCard(props: ChatCardProps) {
  const href = () => `/chats/${props.chat.chat_id}/settings`;
  return (
    <A
      href={href()}
      class="block transition-shadow hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-lg"
    >
      <Card>
        <CardHeader>
          <CardTitle class="line-clamp-2">
            {props.chat.title ?? `chat ${props.chat.chat_id}`}
          </CardTitle>
        </CardHeader>
        <CardContent class="flex flex-col gap-1 text-sm text-muted-foreground">
          <span>ID: {props.chat.chat_id}</span>
          {props.chat.kind ? <span>{props.chat.kind}</span> : null}
          {typeof props.chat.members_count === "number" ? (
            <span>{props.chat.members_count} members</span>
          ) : null}
        </CardContent>
      </Card>
    </A>
  );
}
