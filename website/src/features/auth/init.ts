import { registerAuth } from "@/shared/api/auth-bridge";
import { getInitData, isInWebApp, webAppReady } from "@/shared/lib/telegram-webapp";
import { finishLoading, getToken, reauth, signInWithInitData } from "./store";

/**
 * Boot-time auth wiring. Called from `src/index.tsx` BEFORE `render()` so
 * the API client picks up the token getter / reauth callback that the
 * interceptor chain depends on.
 *
 * Inside Telegram WebApp we silently submit the initData; in a browser the
 * `/login` page is responsible for kicking off auth via the Login Widget.
 */
export async function initAuth(): Promise<void> {
  registerAuth(
    () => getToken(),
    () => reauth(),
  );

  if (isInWebApp()) {
    webAppReady();
    const data = getInitData();
    if (data) {
      try {
        await signInWithInitData(data);
      } catch {
        // Failure here is non-fatal — the protected guard will render the
        // not-authorized page. The interceptor's 401 path will retry on the
        // next API call anyway.
      }
    }
  }

  finishLoading();
}
