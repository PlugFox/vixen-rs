import { onMount } from "solid-js";
import { webAppExpand, webAppReady } from "@/shared/lib/telegram-webapp";

/**
 * Mount once from `WebappLayout`. The actual sign-in happens in
 * `initAuth()` (called from `index.tsx` before render). This component
 * just signals to the Telegram host that the WebApp is ready to display
 * and expands it to full viewport.
 */
export function WebAppBootstrap() {
  onMount(() => {
    webAppReady();
    webAppExpand();
  });
  return null;
}
