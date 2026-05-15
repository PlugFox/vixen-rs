import { auth } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/shared/ui/card";
import { LoginWidget } from "./login-widget";

export function LoginPrompt() {
  return (
    <Card class="mx-auto max-w-md">
      <CardHeader>
        <CardTitle>{t(auth.title)}</CardTitle>
        <CardDescription>{t(auth.prompt)}</CardDescription>
      </CardHeader>
      <CardContent>
        <LoginWidget />
      </CardContent>
    </Card>
  );
}
