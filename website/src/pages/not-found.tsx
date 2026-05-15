import { A } from "@solidjs/router";
import { common } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { Button } from "@/shared/ui/button";

export default function NotFoundPage() {
  return (
    <div class="mx-auto flex max-w-md flex-col items-center gap-4 py-16 text-center">
      <h1 class="text-3xl font-semibold">404</h1>
      <p class="text-muted-foreground">{t(common.errorGeneric)}</p>
      <A href="/">
        <Button>{t(common.back)}</Button>
      </A>
    </div>
  );
}
