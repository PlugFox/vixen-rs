import { createEffect, createSignal } from "solid-js";
import { isInWebApp, onWebAppThemeChanged, webAppColorScheme } from "./telegram-webapp";

export type Theme = "light" | "dark" | "system";

const STORAGE_KEY = "vixen_theme";

function getStoredTheme(): Theme {
  if (typeof localStorage === "undefined") return "system";
  const v = localStorage.getItem(STORAGE_KEY);
  if (v === "light" || v === "dark" || v === "system") return v;
  return "system";
}

function getSystemTheme(): "light" | "dark" {
  if (typeof window === "undefined") return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function resolveTheme(t: Theme): "light" | "dark" {
  return t === "system" ? getSystemTheme() : t;
}

const [theme, setThemeSignal] = createSignal<Theme>(getStoredTheme());

export const currentTheme = theme;

export function setTheme(t: Theme): void {
  setThemeSignal(t);
}

function applyResolved(resolved: "light" | "dark"): void {
  if (typeof document === "undefined") return;
  document.documentElement.setAttribute("data-kb-theme", resolved);
  document.documentElement.classList.toggle("dark", resolved === "dark");
}

/**
 * Apply theme once at startup AND wire reactive updates. Subscribes to:
 *  - the Solid signal (user toggle from settings UI)
 *  - `matchMedia` change (OS-level when theme = system)
 *  - Telegram WebApp `themeChanged` event (override when inside the WebView)
 *
 * In WebApp mode the Telegram theme wins: user-side theme preferences are
 * ignored because the WebView ships its own chrome. The locale switcher
 * (foxic pattern) hides the theme control in WebappLayout.
 */
export function initTheme(): void {
  // Initial paint already happens in `index.html`'s inline script; this call
  // re-derives in case localStorage was modified between paint and module
  // load (rare but possible across page transitions inside a SPA).
  applyResolved(resolveTheme(theme()));

  createEffect(() => {
    const t = theme();
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(STORAGE_KEY, t);
    }
    if (isInWebApp()) {
      const tgScheme = webAppColorScheme();
      if (tgScheme) {
        applyResolved(tgScheme);
        return;
      }
    }
    applyResolved(resolveTheme(t));
  });

  if (typeof window !== "undefined") {
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
      if (theme() === "system" && !isInWebApp()) {
        applyResolved(getSystemTheme());
      }
    });
    onWebAppThemeChanged(() => {
      const s = webAppColorScheme();
      if (s) applyResolved(s);
    });
  }
}
