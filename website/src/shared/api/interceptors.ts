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
 *
 * Two non-obvious guards (Copilot review on PR #99):
 *  - **Auth-endpoint skip**: `reauth()` itself calls `signInWithInitData()`
 *    which hits `/auth/telegram/login`. Without this guard a failing login
 *    re-enters the interceptor, calls `reauth()` again, and loops. We
 *    therefore short-circuit on the auth-endpoint URLs and bubble the 401
 *    straight up.
 *  - **Body snapshot**: `req.body` is a one-shot stream. After `next(req)`
 *    consumes it the retry's `new Request(...)` would send the mutation
 *    without a body. We snapshot to an `ArrayBuffer` once before the first
 *    fetch and reuse it across the retry.
 */
const AUTH_ENDPOINT_SUFFIXES = ["/auth/telegram/login", "/auth/me"] as const;

function isAuthEndpoint(url: string): boolean {
  try {
    const path = new URL(url).pathname;
    return AUTH_ENDPOINT_SUFFIXES.some((s) => path.endsWith(s));
  } catch {
    return false;
  }
}

export function createAuthInterceptor(
  getToken: () => string | null,
  reauth: () => Promise<string>,
): Interceptor {
  let mutex: Promise<string> | null = null;

  return async (req, next) => {
    const skipReauth = isAuthEndpoint(req.url);

    // Snapshot the body before any fetch consumes it. GET / DELETE have
    // `req.body === null` so the snapshot stays `null` and the retry
    // builds a fresh Request without a body.
    const bodySnapshot = req.body ? await req.clone().arrayBuffer() : null;

    const buildRequest = (t: string | null): Request => {
      const headers = new Headers(req.headers);
      if (t) headers.set("authorization", `Bearer ${t}`);
      return new Request(req.url, {
        method: req.method,
        headers,
        body: bodySnapshot,
        signal: req.signal,
      });
    };

    const res = await next(buildRequest(getToken()));
    if (res.status !== 401 || skipReauth) return res;

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
    return next(buildRequest(fresh));
  };
}

/**
 * Adds `X-Request-ID` to every outbound request. The server echoes it back
 * via the `x-request-id` middleware; surfacing it in logs makes
 * client/server correlation possible.
 *
 * Body is forwarded by reference — this interceptor lives **above** the
 * auth interceptor in the chain, so the auth-side body snapshot will
 * happen on the (still-unread) stream we hand down. Don't re-snapshot
 * here or the cost doubles on every request.
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
