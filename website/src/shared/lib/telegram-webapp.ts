/**
 * Thin accessor for `window.Telegram.WebApp`. The runtime is loaded by
 * `index.html`'s `<script src="https://telegram.org/js/telegram-web-app.js">`
 * — it self-no-ops outside of a Telegram WebView, so the predicate
 * `isInWebApp` is what differentiates the two modes.
 *
 * CRITICAL (per project CLAUDE.md): the dashboard never trusts
 * `initDataUnsafe`. Anything user-derived comes from `initData` via the
 * server-validated JWT.
 */

export function isInWebApp(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.Telegram?.WebApp?.initData === "string" &&
    window.Telegram.WebApp.initData.length > 0
  );
}

export function getInitData(): string | null {
  const data = window.Telegram?.WebApp?.initData;
  return data && data.length > 0 ? data : null;
}

export function webAppReady(): void {
  window.Telegram?.WebApp?.ready();
}

export function webAppExpand(): void {
  window.Telegram?.WebApp?.expand();
}

export function webAppClose(): void {
  window.Telegram?.WebApp?.close();
}

export const webAppBackButton = {
  show(): void {
    window.Telegram?.WebApp?.BackButton.show();
  },
  hide(): void {
    window.Telegram?.WebApp?.BackButton.hide();
  },
  onClick(cb: () => void): void {
    window.Telegram?.WebApp?.BackButton.onClick(cb);
  },
  offClick(cb: () => void): void {
    window.Telegram?.WebApp?.BackButton.offClick(cb);
  },
};

export function webAppColorScheme(): "light" | "dark" | null {
  return window.Telegram?.WebApp?.colorScheme ?? null;
}

export function onWebAppThemeChanged(cb: () => void): () => void {
  const tg = window.Telegram?.WebApp;
  if (!tg) return () => {};
  tg.onEvent("themeChanged", cb);
  return () => tg.offEvent("themeChanged", cb);
}

export function botUsername(): string {
  return window.__BOT_USERNAME__ ?? __BOT_USERNAME__;
}
