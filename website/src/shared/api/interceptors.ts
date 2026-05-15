import type { Interceptor } from "./client";
import { ApiError } from "./types";

/**
 * `Authorization: Bearer <jwt>` injection + re-auth on 401.
 *
 * On 401 the interceptor calls `reauth()` which:
 *  - in WebApp mode: re-reads `initData` and re-submits to /auth/login
 *  - in browser mode: signs out and bounces to /login
 *
 * Concurrent calls deduplicate through a `Promise<string> | null` mutex,
 * mirroring foxic's `refreshToken` pattern. After re-auth succeeds the
 * request is retried exactly once.
 *
 * The interceptor never persists tokens to localStorage — per CLAUDE.md JWTs
 * live only in memory.
 */
export function createAuthInterceptor(
  getToken: () => string | null,
  reauth: () => Promise<string>,
): Interceptor {
  let mutex: Promise<string> | null = null;

  return async (req, next) => {
    const token = getToken();
    const withAuth = (t: string | null) => {
      if (!t) return req;
      const headers = new Headers(req.headers);
      headers.set("authorization", `Bearer ${t}`);
      return new Request(req.url, {
        method: req.method,
        headers,
        body: req.body,
        signal: req.signal,
      });
    };

    const res = await next(withAuth(token));
    if (res.status !== 401) return res;

    // Drain the body so the connection can be reused.
    try {
      await res.clone().text();
    } catch {}

    if (!mutex) {
      mutex = reauth().finally(() => {
        mutex = null;
      });
    }
    let fresh: string;
    try {
      fresh = await mutex;
    } catch (e) {
      throw new ApiError(401, "REAUTH_FAILED", e instanceof Error ? e.message : "reauth failed");
    }
    return next(withAuth(fresh));
  };
}

/**
 * Adds `X-Request-ID` to every outbound request. The server echoes it back
 * via the `x-request-id` middleware; surfacing it in logs makes
 * client/server correlation possible.
 */
export const requestIdInterceptor: Interceptor = (req, next) => {
  const headers = new Headers(req.headers);
  if (!headers.has("x-request-id")) {
    headers.set("x-request-id", crypto.randomUUID());
  }
  return next(
    new Request(req.url, {
      method: req.method,
      headers,
      body: req.body,
      signal: req.signal,
    }),
  );
};
