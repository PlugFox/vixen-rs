import { A } from "@solidjs/router";
import { ErrorBoundary, type JSX, Show, Suspense } from "solid-js";
import { currentUser, isAuthenticated, signOut } from "@/features/auth/store";
import { auth, common } from "@/shared/i18n/generated";
import { currentLocale, setLocale, t } from "@/shared/i18n/i18n";
import { currentTheme, setTheme, type Theme } from "@/shared/lib/theme";
import { Button } from "@/shared/ui/button";
import { Select } from "@/shared/ui/select";
import { Skeleton } from "@/shared/ui/skeleton";
import { ToastRegion } from "@/shared/ui/toast";

interface RootLayoutProps {
  children?: JSX.Element;
}

export function RootLayout(props: RootLayoutProps) {
  return (
    <div class="flex min-h-screen flex-col bg-background text-foreground">
      <header class="sticky top-0 z-40 border-b bg-background/95 backdrop-blur">
        <div class="mx-auto flex h-14 w-full max-w-5xl items-center gap-3 px-4">
          <A href="/" class="font-semibold tracking-tight">
            🦊 {t(common.appTitle)}
          </A>
          <span class="ml-auto" />
          <Select<"light" | "dark" | "system">
            value={currentTheme()}
            onChange={(v) => setTheme(v as Theme)}
            options={[
              { value: "system", label: t(common["theme.system"]) },
              { value: "light", label: t(common["theme.light"]) },
              { value: "dark", label: t(common["theme.dark"]) },
            ]}
            aria-label="Theme"
            class="w-28"
          />
          <Select<"en" | "ru">
            value={currentLocale()}
            onChange={(v) => void setLocale(v)}
            options={[
              { value: "en", label: t(common["locale.en"]) },
              { value: "ru", label: t(common["locale.ru"]) },
            ]}
            aria-label="Language"
            class="w-28"
          />
          <Show when={isAuthenticated()}>
            <span class="hidden text-sm text-muted-foreground sm:inline">
              {currentUser()?.username ?? currentUser()?.first_name ?? t(auth.dashboardLabel)}
            </span>
            <Button variant="ghost" size="sm" onClick={signOut}>
              {t(common.signOut)}
            </Button>
          </Show>
        </div>
      </header>

      <main class="mx-auto w-full max-w-5xl flex-1 px-4 py-6">
        <ErrorBoundary
          fallback={(err, reset) => (
            <div class="rounded-md border border-destructive/40 bg-destructive/10 p-4">
              <p class="font-medium">{t(common.errorGeneric)}</p>
              <p class="mt-1 text-xs text-muted-foreground">{String(err)}</p>
              <Button class="mt-3" size="sm" onClick={reset}>
                {t(common.retry)}
              </Button>
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
