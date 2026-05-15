import { useNavigate } from "@solidjs/router";
import { ErrorBoundary, type JSX, onCleanup, onMount, Suspense } from "solid-js";
import { WebAppBootstrap } from "@/features/auth/components/webapp-bootstrap";
import { common } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { webAppBackButton, webAppClose } from "@/shared/lib/telegram-webapp";
import { Skeleton } from "@/shared/ui/skeleton";
import { ToastRegion } from "@/shared/ui/toast";

interface WebappLayoutProps {
  children?: JSX.Element;
}

export function WebappLayout(props: WebappLayoutProps) {
  const navigate = useNavigate();

  onMount(() => {
    const handler = () => {
      if (window.history.length > 1) {
        navigate(-1);
      } else {
        webAppClose();
      }
    };
    webAppBackButton.onClick(handler);
    webAppBackButton.show();

    onCleanup(() => {
      webAppBackButton.offClick(handler);
      webAppBackButton.hide();
    });
  });

  return (
    <div class="flex min-h-screen flex-col bg-background text-foreground">
      <WebAppBootstrap />
      <main class="mx-auto w-full max-w-3xl flex-1 px-3 py-4">
        <ErrorBoundary
          fallback={(err) => (
            <div class="rounded-md border border-destructive/40 bg-destructive/10 p-4 text-sm">
              <p class="font-medium">{t(common.errorGeneric)}</p>
              <p class="mt-1 text-xs text-muted-foreground">{String(err)}</p>
            </div>
          )}
        >
          <Suspense fallback={<Skeleton class="h-32 w-full" />}>{props.children}</Suspense>
        </ErrorBoundary>
      </main>
      <ToastRegion />
    </div>
  );
}
