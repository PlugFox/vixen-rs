import { afterEach, describe, expect, it, vi } from "vitest";
import { createApiClient } from "./client";
import { createAuthInterceptor, requestIdInterceptor } from "./interceptors";
import { ApiError } from "./types";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("createAuthInterceptor", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("does NOT call reauth when /auth/telegram/login itself returns 401", async () => {
    // Regression for the recursive-loop bug Copilot flagged on PR #99: a
    // failing initData submission used to trigger `reauth()` which calls
    // `signInWithInitData` which hits the same endpoint again.
    const reauth = vi.fn(async () => "shouldnotbecalled");
    const fetchMock = vi.fn(async () =>
      jsonResponse({ status: "error", error: { code: "INVALID_INIT_DATA", message: "x" } }, 401),
    );
    vi.stubGlobal("fetch", fetchMock);

    const api = createApiClient({
      baseUrl: "http://localhost/api",
      interceptors: [createAuthInterceptor(() => null, reauth)],
    });

    await expect(api.post("/auth/telegram/login", { init_data: "x" })).rejects.toBeInstanceOf(
      ApiError,
    );
    expect(reauth).not.toHaveBeenCalled();
    expect(fetchMock).toHaveBeenCalledTimes(1); // exactly one fetch — no retry
  });

  it("preserves the request body on 401 retry", async () => {
    // Regression for the consumed-stream bug Copilot flagged on PR #99: the
    // retry used `req.body` which is a one-shot stream, so the retried
    // mutation went out without a body.
    const reauth = vi.fn(async () => "fresh-token");
    const seenBodies: string[] = [];

    const fetchMock = vi.fn(async (req: Request) => {
      const body = await req.text();
      seenBodies.push(body);
      if (seenBodies.length === 1) {
        return jsonResponse(
          { status: "error", error: { code: "INVALID_TOKEN", message: "x" } },
          401,
        );
      }
      return jsonResponse({ status: "ok", data: { ok: true } });
    });
    vi.stubGlobal("fetch", fetchMock);

    const api = createApiClient({
      baseUrl: "http://localhost/api",
      interceptors: [createAuthInterceptor(() => "stale-token", reauth)],
    });

    await api.post("/chats/1/moderation/ban", { user_id: 4242, reason: "spam" });

    expect(reauth).toHaveBeenCalledTimes(1);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(seenBodies).toHaveLength(2);
    expect(seenBodies[0]).toBe(seenBodies[1]); // body replayed verbatim
    expect(JSON.parse(seenBodies[1])).toEqual({ user_id: 4242, reason: "spam" });
  });

  it("deduplicates concurrent 401s — reauth runs once", async () => {
    let calls = 0;
    const reauth = vi.fn(async () => {
      calls += 1;
      // Slow enough that all three concurrent requests pile up on the mutex.
      await new Promise((r) => setTimeout(r, 10));
      return `token-${calls}`;
    });
    const fetchMock = vi.fn(async (req: Request) => {
      if (!req.headers.get("authorization")?.includes("token-")) {
        return jsonResponse(
          { status: "error", error: { code: "INVALID_TOKEN", message: "x" } },
          401,
        );
      }
      return jsonResponse({ status: "ok", data: null });
    });
    vi.stubGlobal("fetch", fetchMock);

    const api = createApiClient({
      baseUrl: "http://localhost/api",
      interceptors: [createAuthInterceptor(() => "stale", reauth)],
    });

    await Promise.all([api.get("/chats"), api.get("/chats/1"), api.get("/chats/2")]);

    expect(reauth).toHaveBeenCalledTimes(1); // mutex held
  });

  it("attaches Authorization header when a token is present", async () => {
    const fetchMock = vi.fn(async (req: Request) => {
      expect(req.headers.get("authorization")).toBe("Bearer abc");
      return jsonResponse({ status: "ok", data: null });
    });
    vi.stubGlobal("fetch", fetchMock);

    const api = createApiClient({
      baseUrl: "http://localhost/api",
      interceptors: [
        createAuthInterceptor(
          () => "abc",
          async () => "fresh",
        ),
      ],
    });
    await api.get("/anything");
  });
});

describe("requestIdInterceptor", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("adds X-Request-ID when missing", async () => {
    let captured: string | null = null;
    const fetchMock = vi.fn(async (req: Request) => {
      captured = req.headers.get("x-request-id");
      return jsonResponse({ status: "ok", data: null });
    });
    vi.stubGlobal("fetch", fetchMock);

    const api = createApiClient({
      baseUrl: "http://localhost/api",
      interceptors: [requestIdInterceptor],
    });
    await api.get("/anything");
    expect(captured).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
  });

  it("preserves a caller-supplied X-Request-ID", async () => {
    let captured: string | null = null;
    const fetchMock = vi.fn(async (req: Request) => {
      captured = req.headers.get("x-request-id");
      return jsonResponse({ status: "ok", data: null });
    });
    vi.stubGlobal("fetch", fetchMock);

    const api = createApiClient({
      baseUrl: "http://localhost/api",
      interceptors: [requestIdInterceptor],
    });
    await api.get("/anything", { headers: { "x-request-id": "preset-id" } });
    expect(captured).toBe("preset-id");
  });
});
