import { useNavigate } from "@solidjs/router";
import { createSignal, onCleanup, onMount } from "solid-js";
import { ApiError } from "@/shared/api";
import { auth } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { botUsername } from "@/shared/lib/telegram-webapp";
import { signInWithInitData } from "../store";

/**
 * Mounts the official Telegram Login Widget. The widget script (loaded via
 * a side-effect <script>) calls `window.onTelegramAuth(user)` with the
 * flat-field payload, which we re-encode into an initData-shaped string
 * (alphabetical key order, URL-encoded) and submit to /auth/login.
 *
 * Build needs `VITE_BOT_USERNAME`; runtime falls back to
 * `window.__BOT_USERNAME__`. Without either, the widget is hidden and an
 * inline error explains how to configure the env.
 */
function composeInitData(user: TelegramLoginWidgetUser): string {
  const params = new URLSearchParams();
  const sortedKeys = Object.keys(user).sort();
  for (const k of sortedKeys) {
    if (k === "hash") continue;
    const v = user[k as keyof TelegramLoginWidgetUser];
    if (v !== undefined && v !== null) params.append(k, String(v));
  }
  params.append("hash", user.hash);
  return params.toString();
}

export function LoginWidget() {
  let containerRef!: HTMLDivElement;
  const navigate = useNavigate();
  const [error, setError] = createSignal<string | null>(null);

  const username = botUsername();

  onMount(() => {
    if (!username) {
      setError(t(auth.botUsernameMissing));
      return;
    }
    window.onTelegramAuth = (user) => {
      const initData = composeInitData(user);
      signInWithInitData(initData)
        .then(() => navigate("/"))
        .catch((e) => {
          if (e instanceof ApiError) {
            setError(e.message);
          } else {
            setError(t({ ns: "errors", key: "NETWORK", en: "Could not reach the server." }));
          }
        });
    };

    const script = document.createElement("script");
    script.async = true;
    script.src = "https://telegram.org/js/telegram-widget.js?22";
    script.setAttribute("data-telegram-login", username);
    script.setAttribute("data-size", "large");
    script.setAttribute("data-radius", "8");
    script.setAttribute("data-onauth", "onTelegramAuth(user)");
    script.setAttribute("data-request-access", "write");
    containerRef.appendChild(script);

    onCleanup(() => {
      window.onTelegramAuth = undefined;
    });
  });

  return (
    <div class="flex flex-col items-center gap-3">
      <div ref={containerRef} class="min-h-[40px]" />
      {error() ? <p class="text-sm text-destructive">{error()}</p> : null}
    </div>
  );
}
